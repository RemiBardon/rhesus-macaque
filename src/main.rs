mod translator;

use clap::Parser;
use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use walkdir::WalkDir;

/// TODO
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the root of the website.
    #[arg(long)]
    root: PathBuf,
    /// Do not translate.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
    /// Translate automatically using OppenAI API.
    #[arg(long, default_value_t = false)]
    auto: bool,
    /// Translate draft pages.
    #[arg(long, default_value_t = false)]
    drafts: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct HugoConfigDTO {
    #[serde(rename(deserialize = "defaultcontentlanguage"))]
    default_content_language: String,
    #[serde(rename(deserialize = "contentdir"))]
    content_dir: Option<String>,
    languages: HashMap<String, HugoLanguageConfigDTO>,
    module: HugoModuleDTO,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct HugoLanguageConfigDTO {
    #[serde(rename(deserialize = "languagename"))]
    language_name: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct HugoModuleDTO {
    mounts: Vec<HugoMountDTO>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct HugoMountDTO {
    lang: Option<String>,
    source: String,
}

#[derive(Debug, Clone, PartialEq)]
struct HugoConfig {
    language_configs: IndexMap<String, HugoLanguageConfig>,
}

impl HugoConfig {
    fn new(config: HugoConfigDTO, root: PathBuf) -> HugoConfig {
        // `content_dirs.keys()` is ordered depending on language weights, no need to do the sorting manually.
        let mut content_dirs: IndexMap<String, PathBuf> = IndexMap::with_capacity(config.languages.len());
        for mount in config.module.mounts {
            if let Some(lang) = mount.lang {
                content_dirs.insert(lang, root.join(mount.source));
            }
        }

        let mut language_configs: IndexMap<String, HugoLanguageConfig> = IndexMap::new();

        for language_identifier in content_dirs.keys() {
            let language_config = config.languages.get(language_identifier)
                .expect("")
                .to_owned();

            let content_dir = content_dirs.get(language_identifier)
                .expect(&format!("Language '{}' has no 'contentDir'", language_identifier))
                .to_owned();

            language_configs.insert(language_identifier.to_owned(), HugoLanguageConfig {
                content_dir,
                language_name: language_config.language_name,
            });
        }

        HugoConfig { language_configs }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct HugoLanguageConfig {
    content_dir: PathBuf,
    language_name: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct FrontMatter {
    #[serde(rename(deserialize = "translationKey"))]
    translation_key: String,
}

#[derive(Debug, Clone, PartialEq)]
struct FileMetadata {
    path: PathBuf,
    language_identifier: String,
    base_name: String,
    translation_key: String,
}

impl FileMetadata {
    fn try_from(path: PathBuf, language_identifier: String) -> Result<Self, Error> {
        let base_name = path.file_stem().ok_or(Error::FileHasNoName)?.to_string_lossy().to_string();

        let front_matter = {
            let file_content = fs::read_to_string(&path)
                .map_err(|err| Error::CouldNotReadFile(path.clone(), err))?;

            // Split the file content by lines
            let lines: Vec<&str> = file_content.split('\n').collect();

            // Find the start and end indices of the first two '---' lines
            let mut start_index: Option<usize> = None;
            let mut end_index: Option<usize> = None;

            for (idx, line) in lines.iter().enumerate() {
                if line.trim() == "---" {
                    if start_index.is_none() {
                        start_index = Some(idx);
                    } else if end_index.is_none() {
                        end_index = Some(idx);
                        break; // Stop when both '---' lines are found
                    }
                }
            }

            let (Some(start), Some(end)) = (start_index, end_index) else {
                return Err(Error::NoFrontMatterFound(path.clone()))
            };

            // Join the lines between the first two '---' markers
            let yaml_lines: Vec<&str> = lines[start + 1..end].to_vec();
            let yaml_content = yaml_lines.join("\n");

            // Parse YAML content into FrontMatter struct
            let front_matter = serde_yaml::from_str::<FrontMatter>(&yaml_content)
                .map_err(Error::FrontMatterParsingFailed)?;
            // println!("Parsed frontmatter: {:#?}", front_matter);

            Ok(front_matter)
        }?;

        Ok(Self {
            path,
            language_identifier,
            base_name,
            translation_key: front_matter.translation_key,
        })
    }
}

fn hugo(cmd_args: &Args, hugo_args: Vec<&str>) -> Result<String, Box<Error>> {
    let output = Command::new("hugo")
        .args(vec![
            vec!["-s", &cmd_args.root.display().to_string()],
            hugo_args,
        ].concat())
        .output()
        .map_err(Error::CommandInvocationFailed)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Command failed with error:\n{}", stderr);
        return Err(Box::new(Error::HugoCommandFailed(stderr.to_string())))
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // println!("Command executed successfully:\n{}", stdout);

    Ok(stdout.to_string())
}

fn draft_files(cmd_args: &Args) -> Result<Vec<PathBuf>, Box<Error>> {
    // List draft pages using `hugo list drafts` so [front matter cascade](https://gohugo.io/content-management/front-matter/#front-matter-cascade)
    // is correctly handled.
    // Output is in CSV format.
    let stdout = hugo(cmd_args, vec!["list", "drafts"])?;
    let site_root = &cmd_args.root;
    let draft_files = stdout.lines()
        // Skip CSV header row (`path,slug,title,date,expiryDate,publishDate,draft,permalink`)
        .skip(1)
        // Get first element (`path`) for each CSV line
        // NOTE: File paths must not contain commas (`,`)
        // NOTE: Unwrapping here is safe as the CSV line will always contains at least one comma (`,`)
        .map(|l| l.split_once(",").unwrap().0)
        // Map string path relative to site root to `PathBuf` relative to current working directory
        .map(|p| site_root.join(p))
        .collect();

    Ok(draft_files)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cmd_args = Args::parse();
    let translator = translator::auto_detect(&cmd_args)?;

    let hugo_config = {
        let stdout = hugo(&cmd_args, vec!["config", "--format", "yaml"])?;

        let hugo_config_dto: HugoConfigDTO = serde_yaml::from_str(&stdout)
            .map_err(Error::Yaml)?;
        // println!("Found config: {:?}", hugo_config_dto);
    
        let hugo_config = HugoConfig::new(hugo_config_dto, cmd_args.root.clone());
        // println!("Derived config: {:?}", hugo_config);

        hugo_config
    };

    if hugo_config.language_configs.len() < 2 {
        return Err(Box::new(Error::NoTranslationPossible))
    }

    let mut files_metadata: Vec<Box<FileMetadata>> = Vec::new();
    let mut all_translations: HashMap<String, HashMap<String, Box<FileMetadata>>> = HashMap::new();
    let draft_files = if cmd_args.drafts { vec![] } else { draft_files(&cmd_args)? };
    for (language_identifier, language_config) in hugo_config.language_configs.iter() {
        // println!("Finding files in '{}'…", language_config.language_name);
        let files = find_markdown_files(&language_config.content_dir);
        // println!("Files found:\n{:?}", files);

        let translatable_files = files
            .iter()
            // Mapping to `FileMetadata` has the side effect of filtering out files which do not contain a `translationKey` in their front matter.
            .flat_map(|path| FileMetadata::try_from(path.to_owned(), language_identifier.clone()))
            .map(Box::new)
            .collect::<Vec<_>>();

        for metadata in translatable_files.iter() {
            all_translations
                .entry(metadata.clone().translation_key)
                .or_insert(HashMap::new())
                .insert(metadata.clone().language_identifier, metadata.to_owned());
        }

        let translatable_files = translatable_files.into_iter().filter(|p| {
            if draft_files.contains(&p.path) {
                println!("Skipping draft page <{}>…", &p.path.display());
                false
            } else {
                true
            }
        }).collect::<Vec<_>>();
        files_metadata.extend(translatable_files);
    }
    // println!("Derived metadata: {:?}", files_metadata);
    // println!("All translations: {:?}", all_translations);

    let all_languages: HashSet<_> = hugo_config.language_configs.keys().collect();
    for metadata in files_metadata {
        let translation_key = metadata.translation_key;
        let translations = all_translations.get(&translation_key).cloned().unwrap_or_default();
        let from_lang = metadata.language_identifier;

        let already_translated_languages: HashSet<_> = translations.keys().collect();
        let to_translate: HashSet<_> = all_languages.difference(&already_translated_languages).collect();

        let from_language_config = hugo_config.language_configs
            .get(&from_lang)
            .expect("TODO");

        let original_content = fs::read_to_string(&metadata.path)
            .map_err(|err| Error::CouldNotReadFile(metadata.path.clone(), err))?;

        let content_file_path = metadata.path
            .strip_prefix(from_language_config.content_dir.clone())
                .expect(&format!("{}", from_language_config.content_dir.display()))
            .to_path_buf();

        for to_lang in to_translate {
            println!("Translating <{}> from '{}' to '{}'…", content_file_path.display(), from_lang, to_lang);

            let to_language_config = hugo_config.language_configs
                .get(to_lang.to_owned())
                .expect("TODO");

            let translated_file_path = translator.translate_path(&content_file_path, &from_lang, &to_lang)?;
            let translated_file_path = to_language_config.content_dir.join(translated_file_path);

            let translation = translator.translate_content(&original_content, &from_lang, &to_lang, "hash".to_string())?;

            println!("Saving '{}' translation of <{}> in <{}>…", to_lang, content_file_path.display(), translated_file_path.display());
            fs::create_dir_all(translated_file_path.parent().unwrap())?;
            fs::write(translated_file_path, translation)?;
        }
    }

    Ok(())
}

#[derive(Debug)]
enum Error {
    /// Failed to execute command
    CommandInvocationFailed(std::io::Error),
    HugoCommandFailed(String),
    Yaml(serde_yaml::Error),
    NoTranslationPossible,
    FileHasNoName,
    CouldNotReadFile(PathBuf, std::io::Error),
    NoFrontMatterFound(PathBuf),
    FrontMatterParsingFailed(serde_yaml::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error: {:?}", self)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::CommandInvocationFailed(err) => Some(err),
            Error::Yaml(err) => Some(err),
            Error::CouldNotReadFile(_, err) => Some(err),
            Error::FrontMatterParsingFailed(err) => Some(err),
            _ => None,
        }
    }
}

fn find_markdown_files(directory: &PathBuf) -> Vec<PathBuf> {
    let mut markdown_files = Vec::new();

    let entries = WalkDir::new(directory)
        .into_iter()
        .filter_map(|e| e.ok());
    for entry in entries {
        let path = entry.into_path();
        if let Some(extension) = path.extension() {
            if extension == "md" {
                markdown_files.push(path);
            }
        }
    }

    markdown_files
}
