# mdweb

**mdweb** is a static blog engine written in pure Rust. It renders a directory
of Markdown documents into a live, multi-language web blog — no build step, no database,
no JavaScript framework, and **zero external dependencies** (stdlib only).

It is a complete standalone program: `mdweb create` scaffolds a demo site, `mdweb new`
creates a new page or post in an existing site, `mdweb run` serves it as a realtime
blog. You just edit Markdown and refresh your browser.

```text
$ mdweb create ./my-blog
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
- **Layout slots** — a `template/<theme>/layout/` directory holds `header`,
  `footer`, `side` and `inject` fragments; the `inject` slot is the natural
  place for analytics / statistics JS.
- **Configurable metadata** — global site config via `site.toml`; per-article
  metadata via frontmatter. Templates + parameters make it easy to customise.
- **Multi-language** — one site, many languages, selected by filename suffix such as
  `hello.zh.md`. A default language and optional language-prefixed URLs.
- **Article metadata** — created/updated dates, author, tags, custom `meta` map.
- **`mdweb create`** — generate a demo doc + template site.
- **`mdweb new`** — create a new page or post in an existing site.
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
mdweb create my-blog

# 2. serve it (default port 8080, or pick your own)
mdweb run my-blog --port 8080

# 3. open http://127.0.0.1:8080/ and start editing Markdown files
```

## Command line

```
mdweb <VERSION> - a static blog engine written in pure Rust.

USAGE:
    mdweb create <PATH>
    mdweb new    <TYPE> <NAME> <SITE_PATH>
    mdweb run    [PATH] [--host HOST] [--port PORT] [--template DIR]

COMMANDS:
    create <PATH>              Scaffold a demo site (docs + template + samples) at PATH.
    new <TYPE> <NAME> <PATH>   Create a new page or post in an existing site.
                               TYPE = page | post.
                               If PATH is the site root (has site.toml), post
                               defaults to content/posts/, page defaults to
                               content/pages/. Otherwise the file is placed at
                               PATH/NAME.md. NAME may contain '/' for sub-directories.
    run                        Serve a doc directory as a realtime web blog. PATH
                               defaults to the current directory. Uses the system
                               default template unless --template DIR or
                               site.toml [theme].

OPTIONS:
    --host <H>      Bind host (default 127.0.0.1)
    --port <P>      Port (default 8080)
    --template <D>  Use a template directory instead of the default theme.
    -h, --help      Show this help.
    -V, --version   Show version.
```

## Creating pages and posts

`mdweb new` creates a single page or post inside an existing site. It needs
three arguments: the type (`page` or `post`), the file name, and the path
to the site (or a sub-directory within it).

```bash
# 1. scaffold a new site
mdweb create ./my-blog
mdweb run ./my-blog

# 2. add a post (PATH is the site root → file lands in content/posts/)
mdweb new post hello-world ./my-blog
# → ./my-blog/content/posts/hello-world.md

# 3. add a page (PATH is the site root → file lands in content/pages/)
mdweb new page about ./my-blog
# → ./my-blog/content/pages/about.md

# 4. add a post inside an existing category
mdweb new post my-post ./my-blog/content/posts/web
# → ./my-blog/content/posts/web/my-post.md

# 5. add a page inside a sub-directory (parent is auto-created)
mdweb new page contact ./my-blog/content/pages/info
# → ./my-blog/content/pages/info/contact.md

# 6. NAME may contain '/' for sub-directories
mdweb new post tips/shortcuts ./my-blog
# → ./my-blog/content/posts/tips/shortcuts.md
```

Notes:

- The `.md` extension is added automatically if you omit it.
- If the target file already exists, the command fails without overwriting.
- The content is taken from `samples/page.md` or `samples/post.md` (both
  included by `mdweb create`). Each sample is a complete, commented
  reference — copy the file and edit the frontmatter + body to taste.
- Frontmatter comments (`# ...`) are part of the bundled samples and are
  valid YAML — the parser skips them, so the file renders correctly out
  of the box.

## Site layout

```
my-blog/
├── site.toml              # global site configuration (TOML)
├── samples/               # commented reference samples for page / post
│   ├── page.md            # source for `mdweb new page`
│   └── post.md            # source for `mdweb new post`
├── content/               # everything you write — content path = web routing
│   ├── _index.md          # → /          (home page; one per language)
│   ├── _index.zh.md       # → /zh/
│   ├── pages/             # pages → /pages/<slug>/; not in the chronological feed
│   │   ├── _index.md
│   │   └── about.md
│   └── posts/             # posts → /posts/<slug>/; appear in feeds and listings
│       ├── _index.md      # category page for /posts/
│       ├── hello-world.md
│       ├── hello-world.zh.md
│       └── web/           # nested sub-categories
│           ├── _index.md
│           └── frontend/
│               ├── _index.md
│               └── react.md
└── template/              # site-local template theme (see Themes)
    └── default/           # the active theme (theme = "default" in site.toml)
        ├── base.html
        ├── index.html
        ├── category.html
        ├── article.html
        ├── page.html
        ├── 404.html
        ├── layout/        # slot fragments (header / footer / side / inject)
        │   ├── header.html
        │   ├── footer.html
        │   ├── side.html
        │   └── inject.html
        └── static/        # site-level static assets, served at /static/
            └── style.css
```

Notes:

- `content/` is a transparent container — its name does not appear in URLs.
  `/content/posts/hello-world.md` is served at `/posts/hello-world/`, and
  `content/_index.md` is served at `/`.
- `content/_index.md` is the home page; only `_index.md` and `_index.<lang>.md`
  are accepted at the top of `content/`. Other top-level files are ignored.
- `_index.md` in a sub-directory becomes that category's index page.
- A directory's `_index.md` title/summary/description configure the category.
- Slot fragments live in `template/<theme>/layout/` (header / footer / side /
  inject). Edit them in place to customize the theme.
- Files in `template/<theme>/static/` are served at `/static/<path>`.

## Static assets

Drop user-owned assets (CSS, images, fonts, favicons, …) into
`template/<theme>/static/`. The server maps `template/<theme>/static/<path>`
to the URL `/static/<path>` so any file under that directory is reachable
at the matching URL. Replacement themes get their own `static/` directory.

```
template/default/static/
├── style.css            → /static/style.css
├── favicon.ico          → /static/favicon.ico
└── images/
    ├── avatar.png       → /static/images/avatar.png
    └── hero.jpg         → /static/images/hero.jpg
```

### Referencing from HTML

Use absolute paths in templates — they're unambiguous and survive template
moves:

```html
<link rel="stylesheet" href="/static/style.css">
<img src="/static/images/avatar.png" alt="avatar">
```

### Referencing from CSS

CSS `url(...)` paths are resolved **by the browser** relative to the
CSS file's URL, not its filesystem path. Since `style.css` is served at
`/static/style.css`, its sibling images must live under `/static/`:

```css
/* template/default/static/style.css */
body {
  background-image: url("./images/bg.png");   /* → /static/images/bg.png  ✓ */
  background-image: url("images/bg.png");     /* → /static/images/bg.png  ✓ */
  background-image: url("../images/bg.png");  /* → /images/bg.png         ✗ */
}
```

Stick to paths that stay within `/static/` — `../` will leave the static
namespace and 404.

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

Resolution order for a slot fragment:

1. `template/<theme>/layout/<name>.html` from the doc directory (highest priority)
2. the built-in default partial

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
