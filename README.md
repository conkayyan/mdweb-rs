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

- **Doc-driven site structure** — the `content/` directory tree maps to web
  routes: `posts/<category>/` → `/posts/<category>/`,
  `pages/<section>/` → `/pages/<section>/`.
- **Layout slots** — a `template/<theme>/layout/` directory holds `header`,
  `footer`, `side` and `inject` fragments; the `inject` slot is the natural
  place for analytics / statistics JS.
- **Built-in analytics** — set an `[analytics.google]` or `[analytics.baidu]`
  block in `site.toml` with a non-empty `id` to auto-inject Google Analytics
  (`gtag.js`) or Baidu Tongji into the page `<head>` — no template edits
  required.
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

## Configuration via environment

The `run` subcommand reads two optional environment variables. They are
useful in container / system-service deployments where passing command-line
flags is awkward:

| Variable      | Equivalent CLI flag | Default     |
| ------------- | ------------------- | ----------- |
| `MDWEB_HOST`  | `--host`            | `127.0.0.1` |
| `MDWEB_PORT`  | `--port`            | `8080`      |

Precedence is **CLI flag > environment variable > default**, matching the
order most container runtimes expect. The example below binds the server
to all interfaces on port 8080 (the typical Docker case):

```bash
MDWEB_HOST=0.0.0.0 MDWEB_PORT=8080 mdweb run ./my-blog
```

## Docker

A pre-built image is published to GitHub Container Registry on every
tagged release.

```bash
# Run with a local ./site directory mounted read-only:
docker run -d --name mdweb -p 8080:8080 \
  -v "$PWD/site:/app/site:ro" \
  ghcr.io/conkayyan/mdweb:latest
```

Or with docker-compose (single service, see [`docker-compose.yaml`](docker-compose.yaml)):

```bash
docker compose up -d
# open http://127.0.0.1:8080/
```

To build the image locally:

```bash
docker build -t mdweb .
docker run --rm -p 8080:8080 -v "$PWD/site:/app/site:ro" mdweb
```

The container honors `MDWEB_HOST` and `MDWEB_PORT` (see
[Configuration via environment](#configuration-via-environment) above).
The bundled `docker-compose.yaml` also sets these explicitly so you can
override at the deployment site without rebuilding the image.

## Releases / downloads

Pre-built binaries are attached to every tagged release on GitHub:

- [github.com/conkayyan/mdweb-rs/releases](https://github.com/conkayyan/mdweb-rs/releases)

| Target                                     | Archive                                                       |
| ------------------------------------------ | ------------------------------------------------------------- |
| `x86_64-unknown-linux-gnu` (Linux x86_64)  | `mdweb-v<tag>-x86_64-unknown-linux-gnu.tar.gz`                |
| `aarch64-unknown-linux-gnu` (Linux ARM64 — Raspberry Pi, AWS Graviton, …) | `mdweb-v<tag>-aarch64-unknown-linux-gnu.tar.gz` |
| `x86_64-apple-darwin` (Intel Mac)          | `mdweb-v<tag>-x86_64-apple-darwin.tar.gz`                     |
| `aarch64-apple-darwin` (Apple Silicon Mac) | `mdweb-v<tag>-aarch64-apple-darwin.tar.gz`                    |
| `x86_64-pc-windows-msvc` (Windows x86_64)  | `mdweb-v<tag>-x86_64-pc-windows-msvc.zip`                     |
| `aarch64-pc-windows-msvc` (Windows ARM64)  | `mdweb-v<tag>-aarch64-pc-windows-msvc.zip`                    |

Each archive contains a single binary (`mdweb` or `mdweb.exe`) plus a
SHA256 sidecar. Verify with `sha256sum -c <archive>.sha256` after
downloading.

## Command line

```
mdweb <VERSION> - a static blog engine written in pure Rust.

USAGE:
    mdweb create <PATH>
    mdweb new    <TYPE> <NAME> <SITE_PATH> [CATEGORY]
    mdweb run    [PATH] [--host HOST] [--port PORT] [--template DIR]

COMMANDS:
    create <PATH>              Scaffold a demo site (docs + template + samples) at PATH.
    new <TYPE> <NAME> <PATH>   Create a new page or post in an existing site.
                               TYPE = page | post.
                               If PATH is the site root (has site.toml), pages
                               land in content/pages/, posts in
                               content/posts/<CATEGORY>/. Posts are aggregated by
                               directory, so a bare content/posts/<NAME>.md has no
                               category page — pass a CATEGORY argument or a
                               `CATEGORY/NAME` name. Otherwise the file is placed
                               directly at PATH/NAME.md.
    run                        Serve a doc directory as a realtime web blog. PATH
                               defaults to the current directory. Uses the system
                               default template unless --template DIR or
                               site.toml [theme].

OPTIONS:
    --host <H>      Bind host (default 127.0.0.1, or $MDWEB_HOST)
    --port <P>      Port (default 8080, or $MDWEB_PORT)
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

# 2. add a post inside a category (posts are grouped by directory)
mdweb new post hello-world ./my-blog guide
# → ./my-blog/content/posts/guide/hello-world.md
mdweb new post guide/hello-world ./my-blog    # equivalent
# → ./my-blog/content/posts/guide/hello-world.md

# 3. add a page (PATH is the site root → file lands in content/pages/)
mdweb new page about ./my-blog
# → ./my-blog/content/pages/about.md

# 4. add a post inside an existing category directory
mdweb new post my-post ./my-blog/web
# → ./my-blog/content/posts/web/my-post.md

# 5. add a page inside a sub-directory (parent is auto-created)
mdweb new page contact ./my-blog/content/pages/info
# → ./my-blog/content/pages/info/contact.md

# 6. NAME may contain '/' for deeper sub-directories
mdweb new post tips/shortcuts ./my-blog
# → ./my-blog/content/posts/tips/shortcuts.md
```

Notes:

- The `.md` extension is added automatically if you omit it.
- A post at the site root needs a CATEGORY (or a `CATEGORY/NAME` name):
  without one the command fails, because the engine indexes posts from
  their directory.
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
│   │                       # (no `_index.md` here — the home page is just
│   │                       # the article stream; add one if you want
│   │                       # a hero intro per language)
│   ├── pages/             # everything that isn't a post goes here
│   │   │                   # (no `/pages/` landing — `pages/` is a
│   │   │                   # transparent container; sections and leaf
│   │   │                   # pages surface at their own URLs)
│   │   ├── about.md       # → /pages/about/      (leaf page)
│   │   └── docs/          # a page section
│   │       ├── _index.md
│   │       └── guide/intro.md
│   └── posts/             # posts → /posts/<category>/<slug>/
│       ├── _index.md      # category page for /posts/ (the chronological feed)
│       ├── guide/         # a category; posts live in a sub-directory
│       │   ├── _index.md  # the ordered beginner's path
│       │   ├── installing-mdweb.md
│       │   └── writing-your-first-post.md
│       ├── hello/         # another category
│       │   └── hello-world.md
│       └── theme/         # theming and template language posts
└── template/              # site-local template theme (see Themes)
    └── default/           # the active theme (theme = "default" in site.toml)
        ├── base.html
        ├── index.html
        ├── category.html
        ├── article.html
        ├── page.html
        ├── tag.html
        ├── tags.html
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
  `content/posts/guide/hello-world.md` → `/posts/guide/hello-world/`. The
  home page (`/`) is the article stream by default — drop in an optional
  `content/_index.md` (or `_index.<lang>.md`) if you want a hero intro
  above the list.
- Only two directories live at the top of `content/`: **`posts/`** and
  **`pages/`**. Everything else (including `about.md` and any custom page
  sections) belongs inside `pages/`.
- **Posts** (`posts/<category>/...`): the chronological feed. Each immediate
  sub-directory of `posts/` (`posts/guide/`, `posts/web/`, …) is a category
  and shows up in the header's **Categories** dropdown. A category's
  `_index.md` is its landing page; posts sit at
  `posts/<category>/<slug>.md`. A bare `posts/foo.md` (no category folder) is
  intentionally unlisted.
- **Pages** (`pages/...`): everything that isn't a post. `pages/` has no
  landing page of its own — `/pages/` returns 404 — but the header
  surfaces every first-level child of `pages/` as **its own top-level
  nav entry**. A sub-directory becomes a dropdown (auto-walking the full
  multi-level subtree under it); a `.md` leaf becomes a flat link:
  - a sub-directory like `pages/docs/` → a `Docs` dropdown whose items
    walk the full tree under it;
  - a `.md` leaf like `pages/about.md` → a flat `About` link.
  Each section or leaf can host its own `_image/` directory for sibling
  images; cross-section references via relative paths still resolve
  symmetrically when the docs are opened locally in an editor. The
  template's `{% for s in page_sections %}` loop is the single source of
  truth for the layout — edit it to reorder, group, or drop entries.
  Pages support the same `date` / `updated` / `author` frontmatter as posts;
  a section's landing page lists its direct pages newest-first (by `date`,
  falling back to `updated`, then file mtime), like the post listings. Each
  listed page shows its date and summary (the frontmatter `summary`, or an
  auto-generated one — see `summary_length`).
- A top-level `content/_index.md` is **optional**. Without one, `/` is just
  the article stream; with one, its body renders as a hero block above
  the list. Other top-level `.md` files at the site root are ignored —
  put them under `pages/` instead.
- A directory's `_index.md` title/summary/description configure the section.
- Tags from any document's frontmatter feed a per-language tag cloud in the
  sidebar and `/tags/<tag>/` listing pages (see [Tag pages](#tag-pages)).
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

## Document images

Assets that belong to one document rather than to the theme live beside it,
in an `_image/` directory. Any directory under `content/` may have one:

```
content/posts/guide/
├── _image/
│   └── hero.svg        → /posts/guide/hero.svg
├── _index.md
└── embedding-images.md
```

Reference them with an ordinary **relative filesystem path**, so the file
resolves both in your editor's markdown preview and on the site — the
`_image` segment is dropped when the URL is generated:

```markdown
![Banner](_image/hero.svg)              → <img src="/posts/guide/hero.svg">
![Logo](../../pages/_image/logo.svg)    → <img src="/pages/logo.svg">
<img src="_image/hero.svg" width="160"> → raw HTML is rewritten too
```

Rules:

- The file must sit **directly** inside `_image/`; for sub-folders, give the
  sub-folder its own `_image/`.
- `../` may cross directories but must stay inside `content/`. Paths that
  climb out are left untouched and will 404.
- Absolute (`/static/…`), external (`https://…`) and `data:` URLs, and any
  relative path without an `_image` segment, are passed through unchanged.
- Images are language-neutral: `/posts/guide/hero.svg` and
  `/zh/posts/guide/hero.svg` serve the same file.
- Avoid spaces and `?` or `#` in filenames.

## Configuration (`site.toml`)

```toml
title = "My Blog"            # site title (fallback for every language)
base_url = "http://localhost:8080"
author = "Jane Doe"
language = "en"              # default language (unprefixed URLs)
languages = ["en", "zh"]     # enabled languages; other languages are ignored
theme = "default"           # name of a directory under template/; leave empty for built-in default

# Listing limits. Set `0` to disable pagination for that listing.
home_limit = 10      # articles per page on /
category_limit = 20  # articles per page in category landings
pages_limit = 50     # pages per page in a directory landing
tags_limit = 20      # articles per page on a /tags/<tag>/ landing
summary_length = 240 # max chars of an auto-generated summary (0 = keep all)

# Tag cloud: show the tag widget in the sidebar (true) or hide it (false).
show_tag_cloud = true

# Analytics — leave the section out (or set `id = ""`) to disable. Each
# enabled provider's tracker JS is auto-injected into the page <head>
# before any HTML from `template/<theme>/layout/inject.html`.
#
# [analytics.google]            # Google Analytics 4 (gtag.js)
# id = "G-XXXXXXXXXX"
#
# [analytics.baidu]             # Baidu Tongji (百度统计)
# id = "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"

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
`tags`, `tag_list`, `read_in`, `published`, `updated`, `author`,
`reading_time`, `reading_time_seconds`, `prev`, `next`,
`not_found`, `not_found_desc`, `menu`, `nav_all_posts`. Missing keys fall back to English, then to a
built-in default, then to the key string itself.

### Routes

The built-in, non-content routes are configurable via a `[routes]` table. Each
value is the URL slug (path segment or file name) for that route; changing it
renames the route **everywhere** — server matching, generated links, the
footer/theme templates, and the feed/sitemap/search-index URLs. Language-prefixed
variants keep working automatically (`/<lang>/search`, `/<lang>/rss.xml`, …).

```toml
[routes]
search       = "search"        # search page:          /search  (/<lang>/search)
tags         = "tags"          # tags index + listing: /tags/   (/<lang>/tags/<tag>/)
rss          = "rss.xml"       # RSS feed:             /rss.xml (/<lang>/rss.xml)
sitemap      = "sitemap.xml"   # XML sitemap:          /sitemap.xml
search_index = "search.json"   # client-side search:   /search.json
static       = "static"        # theme assets:         /static/...
posts        = "posts"         # blog container:       content/posts/... → /posts/...
pages        = "pages"         # pages container:      content/pages/... → /pages/...
```

The values above are the defaults, so the block may be omitted entirely. An
empty value falls back to its default; surrounding slashes are trimmed (so
`"/search/"` behaves like `"search"`). The theme templates can read the resolved
values from the `routes.*` context keys (e.g. `routes.static`,
`routes.search_index`) and the full URLs from `static_url`, `search_action`,
`rss_url`, `sitemap_url`.

`posts`/`pages` only rename the **URL prefix**: content stays on disk under
`content/posts/` and `content/pages/`, and everything derived from it — article
and category URLs, page-section landings, breadcrumbs, navigation, image URLs,
the RSS feed and the sitemap — follows the new prefix (`posts = "blog"` turns
`content/posts/guide/hello.md` into `/blog/guide/hello/`).

### Security headers

Every response carries `X-Content-Type-Options: nosniff`,
`X-Frame-Options: SAMEORIGIN` and `Referrer-Policy: no-referrer`, plus a
`Content-Security-Policy` balanced for the default themes and the built-in
analytics snippets. All of this is configurable under `[security]`:

```toml
[security]
enabled = false                        # turn off all extra headers
csp = "default-src 'self'"             # full policy override; "" omits the header
```

`enabled` defaults to `true`. Set `csp` to replace the built-in policy with
your own (for example to allow a self-hosted analytics origin); an empty value
sends no `Content-Security-Policy` header at all.

## Tag pages

Every document's frontmatter `tags` feeds a per-language tag index. The sidebar
shows a **tag cloud** (every tag weighted by how many documents carry it) that
is hidden entirely by setting `show_tag_cloud = false` in `site.toml`. Tags are
clickable **everywhere** — in the sidebar cloud, on article pages, on page
listings, and in search results — and link to the matching tag listing:

```text
/tags/              ← index of every tag (per language)
/tags/<tag>/        ← posts carrying <tag> (default language)
/<lang>/tags/<tag>/ ← other languages
```

The sidebar cloud shows at most `tag_cloud_limit` tags (`0` = show all), the
most-used tags first — each rendered as `name(count)` where `count` is how many
documents carry the tag. `/tags/` (and `/<lang>/tags/`) lists **every** tag in
the language in the same `name(count)` form, linked to the matching listing. A
tag listing page shows every document carrying
that tag, newest first, with breadcrumbs (`Index › Tags › <tag>`) and pagination
driven by `tags_limit` (`?page=N`). Tag names with spaces or punctuation are
percent-encoded in URLs (`my tag` → `/tags/my%20tag/`).

Category listings also show the same breadcrumb trail (`Index › Posts › Web`).

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
| `tags` | per-language tag cloud: `[{ name, url, count }, …]` (limited to `tag_cloud_limit`) |
| `tag_cloud_limit` | maximum sidebar tags to show (`0` = show all) |
| `show_tag_cloud` | whether the sidebar tag cloud widget is enabled |
| `home_url` | URL of the home page for the current language |
| `current_url` | current request URL |
| `current_year` | current year (for copyright footers) |
| `header` / `side` / `footer` / `inject` | rendered layout slots |

Per-template:

- `index.html` → `home` `{ content, articles: [...] }`
- `category.html` → `category` `{ title, slug, url, description, content, articles, children }`
- `tag.html` → `tag` `{ name, title, url, articles, pagination, total }`
- `tags.html` → `tags_index` `{ title, url, tags: [{ name, url, count }], total }`
- `article.html` / `page.html` → `article` `{ title, lang, url, date, updated, author, tags, tag_links, content, meta, fields, ... }`
- `404.html` → `page` `{ title: "Not Found" }`

## Multi-language URLs

The default language is served at unprefixed paths (`/posts/guide/hello-world/`); other
languages get a prefix (`/zh/posts/guide/hello-world/`). The language switcher uses
`languages[].url`.

**Breaking changes (v0.2.0):** `languages[].title` (which confusingly returned the
*site title*, not a label) is replaced by `languages[].display_name`. The `is_zh`
boolean context key is removed; use the `t.*` keys instead. The `lang_meta` map
and `lang_active` flag are also removed — use `lang` for the current language
code, `current_lang_display_name` for the language button label.

## License

[MIT](LICENSE) — Copyright (c) 2026 conkayyan.
