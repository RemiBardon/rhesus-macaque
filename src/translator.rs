use clipboard::{ClipboardContext, ClipboardProvider};
use openai_api_rs::v1::assistant::{AssistantRequest, AssistantObject};
use openai_api_rs::v1::common::GPT3_5_TURBO_1106;
use openai_api_rs::v1::api::Client;
use openai_api_rs::v1::error::APIError;
use openai_api_rs::v1::message::{CreateMessageRequest, MessageRole};
use openai_api_rs::v1::run::CreateRunRequest;
use openai_api_rs::v1::thread::{CreateThreadRequest, ThreadObject};
use std::cell::OnceCell;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::Mutex;
use std::{env, io, fmt};
use std::error::Error;
use std::time::Duration;

use crate::Args;

pub fn auto_detect(args: &Args) -> Result<Box<dyn Translator>, Box<dyn Error>> {
    if args.dry_run {
        return Ok(Box::new(DryRunTranslator));
    }
    if args.auto {
        return Ok(GPTAutoTranslator::new().map(Box::new)?)
    }
    return Ok(GPTManualTranslator::new().map(Box::new)?)
}

pub trait Translator {
    /// Name of the generator (eg. "gpt-3.5-turbo-1106", "GPT-4", "DeepL"…).
    fn generator(&self) -> &str;

    /// Translate a file path synchronously.
    fn translate_path(
        &self,
        path: &PathBuf,
        from_lang: &String,
        to_lang: &String,
    ) -> Result<PathBuf, Box<dyn Error>>;

    /// Translate a text synchronously.
    fn translate_content(
        &self,
        text: &String,
        from_lang: &String,
        to_lang: &String,
        source_hash: String,
    ) -> Result<String, Box<dyn Error>>;

    fn path_translate_prompt(
        &self,
        path: &PathBuf,
        from_lang: &String,
        to_lang: &String,
    ) -> String {
        format!(r#"Translate the file path "{}" from {} to {}"#, path.display(), from_lang, to_lang)
    }

    /// Prompt sendable to a LLM for content translation.
    fn content_translate_prompt(
        &self,
        text: &String,
        from_lang: &String,
        to_lang: &String,
        source_hash: String,
    ) -> String {
        format!(
            "Translate the following Hugo SSG markdown content file from {} to {}. Do not translate YAML items in `read_allowed` and `translationKey`. Add YAML front matter keys `translator: \"{}\"` and `sourceHash: \"{}\"` before all other keys and `# GENERATED BY {}` at the very start of the front matter. Remove italics from words in {} and add italics to words in {}. Do not translate \"TODO\" and \"FIXME\".\n\n```md\n{}\n```",
            from_lang,
            to_lang,
            self.generator(),
            source_hash,
            self.generator(),
            to_lang,
            from_lang,
            text,
        )
    }
}

struct DryRunTranslator;

impl Translator for DryRunTranslator {
    fn generator(&self) -> &str {
        "DRY_RUN"
    }

    fn translate_path(
        &self,
        _path: &PathBuf,
        _from_lang: &String,
        _to_lang: &String,
    ) -> Result<PathBuf, Box<dyn Error>> {
        Ok("/dev/null".into())
    }

    fn translate_content(
        &self,
        _text: &String,
        _from_lang: &String,
        _to_lang: &String,
        _source_hash: String,
    ) -> Result<String, Box<dyn Error>> {
        Ok("DRY_RUN".to_string())
    }
}

fn wait_for_user_input() {
    let mut user_input = String::new();
    match io::stdin().read_line(&mut user_input) {
        Ok(_) => (),
        Err(error) => {
            eprintln!("Error reading input: {}", error);
        },
    }
}

struct GPTManualTranslator {
    model: String,
    clipboard: Mutex<ClipboardContext>,
}

impl GPTManualTranslator {
    fn new() -> Result<Self, Box<dyn Error>> {
        dotenvy::dotenv()?;
        let model = env::var("OPENAI_CHAT_MODEL")
            .expect(r#"Environment variable `OPENAI_CHAT_MODEL` must be set to the version of ChatGPT used for manual translation ("GPT-3.5", "GPT-4"…)"#);
        let clipboard: ClipboardContext = ClipboardProvider::new()?;

        Ok(Self { model, clipboard: Mutex::new(clipboard) })
    }
}

impl Translator for GPTManualTranslator {
    fn generator(&self) -> &str {
        &self.model
    }

    fn translate_path(
        &self,
        path: &PathBuf,
        from_lang: &String,
        to_lang: &String,
    ) -> Result<PathBuf, Box<dyn Error>> {
        let mut clipboard = self.clipboard.lock()
            .map_err(|e| e.to_string())?;
        let prompt = self.path_translate_prompt(path, from_lang, to_lang);

        println!("Paste the following prompt into ChatGPT (it's already in your clipboard), copy the result, come back and hit [Enter]:\n> {}", prompt);
        clipboard.set_contents(prompt)?;
        wait_for_user_input();
        clipboard.get_contents().map(PathBuf::from)
    }

    fn translate_content(
        &self,
        text: &String,
        from_lang: &String,
        to_lang: &String,
        source_hash: String,
    ) -> Result<String, Box<dyn Error>> {
        let mut clipboard = self.clipboard.lock()
            .map_err(|e| e.to_string())?;
        let prompt = self.content_translate_prompt(text, from_lang, to_lang, source_hash);

        println!("Paste the copied prompt into ChatGPT (it's already in your clipboard), copy the result, come back and hit [Enter]");
        clipboard.set_contents(prompt)?;
        wait_for_user_input();
        clipboard.get_contents()
    }
}

struct GPTAutoTranslator {
    client: Client,
    model: String,
    assistant_description: String,
    assistant: OnceCell<Result<AssistantObject, APIError>>,
    thread: OnceCell<Result<ThreadObject, APIError>>,
}

impl GPTAutoTranslator {
    fn new() -> Result<Self, Box<dyn Error>> {
        dotenvy::dotenv()?;
        let api_key = env::var("OPENAI_API_KEY").expect("The `OPENAI_API_KEY` environment variable must be defined.");
        let model = match env::var("OPENAI_API_MODEL") {
            Ok(v) => v,
            Err(err) => {
                let model = GPT3_5_TURBO_1106.to_string();
                println!("`OPENAI_API_MODEL` environment variable not found ({}), using '{}'", err, model);
                model
            },
        };
        let assistant_description = env::var("OPENAI_ASSISTANT_DESCRIPTION").unwrap_or("Test assistant".to_string());

        let client = Client::new(api_key);

        Ok(Self {
            client,
            model,
            assistant_description,
            assistant: OnceCell::new(),
            thread: OnceCell::new(),
        })
    }

    fn assistant(&self) -> Result<&AssistantObject, TranslationError> {
        self.assistant.get_or_init(|| {
            let req = AssistantRequest::new(self.model.clone())
            .description(self.assistant_description.clone())
            .instructions("You are a personal math tutor. When asked a question, write and run Python code to answer the question.".to_string());
            println!("Assistant request: {:?}", req);

            let assistant = self.client.create_assistant(req)?;
            println!("Created assistant '{:?}'", assistant.id);

            Ok(assistant)
        }).as_ref().map_err(|e| TranslationError::OpenAIError(e.message.clone()))
    }

    fn thread(&self) -> Result<&ThreadObject, TranslationError> {
        self.thread.get_or_init(|| {
            let client = &self.client;

            let req = CreateThreadRequest::new();
            let thread = client.create_thread(req)?;
            println!("Created thread: {:?}", thread.id);

            Ok(thread)
        }).as_ref().map_err(|e| TranslationError::OpenAIError(e.message.clone()))
    }

    fn run(&self, prompt: String) -> Result<String, Box<dyn Error>> {
        let client = &self.client;
        let assistant = self.assistant()?;
        let thread = self.thread()?;

        let message_req = CreateMessageRequest::new(MessageRole::user, prompt);

        let message_result = client.create_message(thread.id.clone(), message_req)?;
        println!("{:?}", message_result.id.clone());

        let run_req = CreateRunRequest::new(assistant.id.clone());
        let run_result = client.create_run(thread.id.clone(), run_req)?;

        loop {
            let run_result = client
                .retrieve_run(thread.id.clone(), run_result.id.clone())
                .unwrap();
            if run_result.status == "completed" {
                break;
            } else {
                println!("waiting...");
                std::thread::sleep(Duration::from_secs(1));
            }
        }

        let list_message_result = client.list_messages(thread.id.clone()).unwrap();
        let mut result = "".to_string();
        for data in list_message_result.data {
            for content in data.content {
                println!(
                    "{:?}: {:?} {:?}",
                    data.role, content.text.value, content.text.annotations
                );
                result.push_str(&content.text.value);
            }
        }

        Ok(result)
    }
}

impl Translator for GPTAutoTranslator {
    fn generator(&self) -> &str {
        &self.model
    }

    fn translate_path(
        &self,
        path: &PathBuf,
        from_lang: &String,
        to_lang: &String,
    ) -> Result<PathBuf, Box<dyn Error>> {
        self.run(self.path_translate_prompt(path, from_lang, to_lang)).map(PathBuf::from)
    }

    fn translate_content(
        &self,
        text: &String,
        from_lang: &String,
        to_lang: &String,
        source_hash: String,
    ) -> Result<String, Box<dyn Error>> {
        self.run(self.content_translate_prompt(text, from_lang, to_lang, source_hash))
    }
}

#[derive(Debug, Clone)]
enum TranslationError {
    OpenAIError(String),
}

impl Display for TranslationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error: {:?}", self)
    }
}

impl Error for TranslationError {}
