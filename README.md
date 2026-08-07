# mdweb

**mdweb** is a static blog engine written in pure Rust. It renders a directory
of Markdown documents into a live, multi-language web blog — no build step, no database,
no JavaScript framework, and **zero external dependencies** (stdlib only).

It is a complete standalone program: `mdweb new` scaffolds a demo site, `mdweb run`
serves it as a realtime blog. You just edit Markdown and refresh your browser.

```text
$ mdweb new ./my-blog
created demo site at ./my-blog
  run:  mdweb run ./my-blog

$ mdweb run ./my-blog
mdweb: serving site at http://127.0.0.1:8080/
       docs: ./my-blog
press Ctrl-C to stop
```

---

## Features

- **Doc-driven site structure** — the `doc/` directory tree is rendered as category
  levels automatically (`posts/` → `/posts/`, `notes/` → `/notes/`, …).
- **Layout slots** — a `_layout/` directory holds `header`, `footer`, `side` and
  `inject` fragments; the `inject` slot is the natural place for analytics /
  statistics JS.
- **Configurable metadata** — global site config via `site.toml`; per-article
  metadata via frontmatter. Templates + parameters make it easy to customise.
- **Multi-language** — one site, many languages, selected by filename suffix such as
  `hello.zh.md`. A default language and optional language-prefixed URLs.
- **Article metadata** — created/updated dates, author, tags, custom `meta` map.
- **`mdweb new`** — generate a demo doc + template site.
- **`mdweb run`** — serve any doc directory live; uses the system default theme unless
  you pass `--template` or set `theme` in `site.toml`.
- **Small markdown renderer** — headings, paragraphs, fenced code blocks, blockquotes,
  ordered/unordered (nested) lists, HR, inline emphasis/code/links/images/strikethrough,
  and raw HTML pass-through. No external crates.

## Building

Requires a stable Rust toolchain.

```bash
cargo build --release
./target/release/mdweb --help
```

## Quick start

```bash
# 1. create a demo site (docs + a copy of the default template)
mdweb new my-blog

# 2. serve it (default port 8080, or pick your own)
mdweb run my-blog --port 8080

# 3. open http://127.0.0.1:8080/ and start editing Markdown files
```

## Command line

```
mdweb <VERSION> - a static blog engine written in pure Rust.

USAGE:
    mdweb run   [PATH] [--host HOST] [--port PORT] [--template DIR]
    mdweb new   <PATH>

COMMANDS:
    new <PATH>   Create a demo site (docs + template) at PATH.
    run          Serve a doc directory as a realtime web blog. PATH defaults
                 to the current directory. Uses the system default template
                 unless --template DIR or site.toml [theme].

OPTIONS:
    --host <H>      Bind host (default 127.0.0.1)
    --port <P>      Port (default 8080)
    --template <D>  Use a template directory instead of the default theme.
    -h, --help      Show this help.
    -V, --version   Show version.
```

## Site layout

```
my-blog/
├── site.toml              # global site configuration (TOML)
├── _index.md              # homepage content (frontmatter + markdown)
├── about.md               # a normal page (layout: "page" renders with page.html)
├── _layout/               # doc-level layout slots override the theme partials
│   ├── header.html
│   ├── footer.html
│   ├── side.html
│   └── inject.html        # put analytics / statistics JS here
├── _static/               # extra static assets served under /static/
├── posts/
│   ├── _index.md          # category page for /posts/
│   ├── hello-world.md     # article for the default language
│   └── hello-world.zh.md  # the same article in another language
├── notes/
│   ├── _index.md
│   └── tips.md
└── template/              # site-local template theme (see Themes)
    ├── base.html
    ├── index.html
    ├── category.html
    ├── article.html
    ├── page.html
    ├── 404.html
    └── partials/
        ├── header.html
        ├── footer.html
        ├── side.html
        └── inject.html
```

Notes:

- `_index.md` in a directory becomes that category's index page.
- A directory's `_index.md` title/summary/description configure the category.
- Files in `_layout/` shadow the theme's `partials/` with the same name.
- `_static/` files are served at `/static/<path>`.

## Configuration (`site.toml`)

```toml
title = "My Blog"            # site title (fallback for every language)
base_url = "http://localhost:8080"
author = "Jane Doe"
language = "en"              # default language (unprefixed URLs)
languages = ["en", "zh"]     # enabled languages; other languages are ignored
theme = "template"           # "default" (built-in) or a directory name like "template"

[lang.en]                    # per-language overrides
title = "My Blog"
description = "A demo site built with mdweb."
keywords = "blog, rust"

[lang.zh]
title = "我的博客"
description = "使用 mdweb 构建的演示站点，支持多语言。"

[meta]                       # arbitrary metadata exposed to templates as config.meta
description = "A demo site"

[params]                     # arbitrary parameters exposed to templates as config.params
github = "https://github.com/conkayyan/mdweb-rs"

# Friend links rendered in the sidebar (target="_blank"). Each [[friend_links]]
# entry becomes { name, url } in the friend_links ctx array.
[[friend_links]]
name = "mdweb"
url = "https://github.com/conkayyan/mdweb-rs"

[[friend_links]]
name = "Rust"
url = "https://www.rust-lang.org/"
```

## Frontmatter

Each Markdown file may start with a YAML-style `---` block. Supported keys:

```yaml
---
title: "Hello World"     # page title
date: "2024-01-15"       # creation date (quoted strings recommended)
updated: "2024-06-01"    # last update date
author: "Jane Doe"
tags: ["mdweb", "rust"]
summary: "One-line description."
layout: "page"           # "article" (default) or "page"; page renders with page.html
draft: false             # true hides the article
meta:                    # arbitrary map exposed to templates as article.meta
  description: "Longer description"
---
```

A TOML-style `+++` block is also accepted.

## Themes

Themes are directories of templates. A site-local `template/` directory overrides the
built-in default theme. `mdweb new` copies the default theme so you can edit it freely.

Resolution order for a slot/partial name:

1. `_layout/<name>.html` from the doc directory (highest priority)
2. `<theme>/partials/<name>.html`
3. the built-in default partial

### Template syntax

- **Output**: `{{ expr }}` or `{{ expr | safe }}` (the `safe` filter skips HTML escaping).
- **Blocks**: `{% block name %} ... {% endblock name %}` — overridden by child pages.
- **Extends**: `{% extends "base.html" %}`.
- **Conditionals**: `{% if expr %} ... {% else %} ... {% endif %}`.
- **Loops**: `{% for x in xs %} ... {% endfor %}` — with `x_index` as the zero-based index.
- **Comments**: `{# ... #}`.

### Context variables

Globally available:

| Variable | Description |
| --- | --- |
| `config.title` / `config.base_url` / `config.author` / `config.language` | site config |
| `config.languages` | list of language codes |
| `config.meta` / `config.params` | arbitrary maps from `site.toml` |
| `site.title` / `site.lang` | site title and current language |
| `title` | current page/site title |
| `description` / `keywords` | current language description/keywords |
| `lang` | current language code |
| `languages` | list of `{ code, title, url }` for the language switcher |
| `categories` | category tree (nested `{ title, url, children }`) |
| `home_url` | URL of the home page for the current language |
| `current_url` | current request URL |
| `current_year` | current year (for copyright footers) |
| `header` / `side` / `footer` / `inject` | rendered layout slots |

Per-template:

- `index.html` → `home` `{ content, articles: [...] }`
- `category.html` → `category` `{ title, slug, url, description, content, articles, children }`
- `article.html` / `page.html` → `article` `{ title, lang, url, date, updated, author, tags, content, meta, fields, ... }`
- `404.html` → `page` `{ title: "Not Found" }`

## Multi-language URLs

The default language is served at unprefixed paths (`/posts/hello-world/`); other
languages get a prefix (`/zh/posts/hello-world/`). The language switcher uses
`languages[].url`.
