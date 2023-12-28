# Rhesus Macaque 🐒

Lightweight tool helping internationalization of [Hugo] websites.
It uses LLMs (currently OpenAI GPTs) to translate content files
while keeping meaningful data intact such as [shortcodes] names and [Orangutan] profiles.

<!-- Design decisions are detailed in [`design/README.md`](./design/README.md). -->

## Disclaimer 🙅

<a href="http://www.wtfpl.net/">
  <img
    src="http://www.wtfpl.net/wp-content/uploads/2012/12/wtfpl-badge-1.png"
    width="88" height="31"
    alt="WTFPL"
  />
</a>

- This project is **not** production-ready.
- This project is **in development**, but only as a side project.
- This project is **not** actively maintained.

## Main features ✨

- **Translates** whole Hugo content pages ([front matter] included)
- **Automatic or manual** translation (with or without [OpenAI API] token)
- Supports **all languages** supported by OpenAI GPTs
- **Avoids re-translating** the same content pages twice (if it hasn't changed)
- Makes sure the website **still generates successfully**
- **Labels pages** as "Generated by an AI"
- Allows **manual overrides**
- Takes care of putting **foreign works in italics**

Also worth mentioning:

- Works with [Orangutan] (as it doesn't break profile names)

## Why this name? 🤨

> Which primate (except the Human) can be found in the most countries in the world?

> The Rhesus Macaque (Macaca mulatta) is one of the most widely distributed non-human primates in the world after humans. They are native to South, Central, and Southeast Asia, and have been introduced to various other regions, often due to human activities such as the pet trade or scientific research. Rhesus macaques can be found in several countries across Asia, the Middle East, the Caribbean, and even parts of Europe, making them one of the most geographically widespread primate species apart from humans.
>
> Source: GPT 3.5, 2023-12-27

Also, I like monkeys.

[front matter]: https://gohugo.io/content-management/front-matter/ "Front matter | Hugo"
[Hugo]: https://gohugo.io/ "The world’s fastest framework for building websites | Hugo"
[OpenAI API]: https://openai.com/product#made-for-developers
[Orangutan]: https://github.com/RemiBardon/Orangutan "RemiBardon/Orangutan: Lightweight authorization layer for static sites"
[shortcodes]: https://gohugo.io/content-management/shortcodes/ "Shortcodes | Hugo"