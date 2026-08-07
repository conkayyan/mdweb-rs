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
theme = "default"           # name of a directory under template/; leave empty for built-in default

[lang.en]                    # per-language overrides
title = "My Blog"
display_name = "English"     # label shown in the language dropdown
description = "A demo site built with mdweb."
keywords = "blog, rust"

[lang.zh]
title = "我的博客"
display_name = "简体中文"
description = "使用 mdweb 构建的演示站点，支持多语言。"
keywords = "博客, rust"

[i18n.zh]                    # UI string overrides; missing keys fall back to English
home           = "首页"
categories     = "分类"
recent_posts   = "最近文章"
friend_links   = "友情链接"
no_posts       = "暂无文章。"
read_in        = "其他语言："
published      = "发布于："
updated        = "更新于："
author         = "作者："
prev           = "上一篇"
next           = "下一篇"
not_found      = "页面未找到"
not_found_desc = "您访问的页面不存在。"

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

### Multi-language dropdown

The header shows a language switcher dropdown. Each language's label is set via
`[lang.<code>].display_name`; if unset, the raw code (`zh`, `en`, …) is shown.

```toml
[lang.zh]
title = "我的博客"
display_name = "简体中文"
```

### UI strings (i18n)

Default templates ship English strings and look up labels via the `t.*` context
keys. Override them per language under `[i18n.<code>]`:

```toml
[i18n.zh]
categories   = "分类"
recent_posts = "最近文章"
friend_links = "友情链接"
no_posts     = "暂无文章。"
```

Available keys: `home`, `categories`, `recent_posts`, `friend_links`, `no_posts`,
`read_in`, `published`, `updated`, `author`, `prev`, `next`, `not_found`,
`not_found_desc`. Missing keys fall back to English, then to a built-in default,
then to the key string itself.

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

A theme is a directory of templates under `template/<name>/`. Set `theme = "<name>"`
in `site.toml` (or leave it unset / empty to use the built-in `default`).
`mdweb new` writes `template/default/` so you can edit the active theme in place
or duplicate it under a new name to switch.

Resolution order for a slot/partial name:

1. `_layout/<name>.html` from the doc directory (highest priority)
2. `template/<theme>/partials/<name>.html`
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
| `languages` | list of `{ code, display_name, url, active }` for the language switcher |
| `t` | UI string map (keys: `home`, `categories`, `recent_posts`, …) |
| `current_lang_display_name` | display name for the current language (e.g. button label) |
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

**Breaking changes (v0.2.0):** `languages[].title` (which confusingly returned the
*site title*, not a label) is replaced by `languages[].display_name`. The `is_zh`
boolean context key is removed; use the `t.*` keys instead. The `lang_meta` map
and `lang_active` flag are also removed — use `lang` for the current language
code, `current_lang_display_name` for the language button label.
