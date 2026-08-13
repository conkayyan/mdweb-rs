---
title: "Configuration, RSS and SEO"
date: "2026-08-08"
updated: "2026-08-09"
tags: ["tutorial", "config", "seo"]
---

Most of mdweb's behaviour is controlled from a single `site.toml` at the
project root. Set the title, base URL, author, default language and theme:

```toml
title = "My Blog"
base_url = "http://localhost:8080"
author = "Jane Doe"
language = "en"
languages = ["en", "zh"]
theme = "default"
```

## Per-language strings

Localised titles and descriptions live under `[lang.<code>]`, and UI
strings under `[i18n.<code>]`. Each configured language also gets its own
URL prefix (`/zh/…` for `zh`):

```toml
[lang.zh]
title = "我的博客"
display_name = "简体中文"
description = "使用 mdweb 构建的演示站点，支持多语言。"
keywords = "博客, rust"

[i18n.zh]
home     = "首页"
categories = "分类"
```

## Routing rules

The `[routes]` table renames the well-known routes. Renaming one updates
the server, generated links, footer/theme templates and the
feed/sitemap/search-index URLs all at once:

```toml
[routes]
search = "search"
rss    = "rss.xml"
posts  = "posts"
pages  = "pages"
```

## RSS, sitemap and SEO

mdweb generates `/rss.xml`, `/sitemap.xml`, and a `<link rel=alternate>`
in the document head. Toggle the footer links via `show_rss` and
`show_sitemap` in `site.toml`. The sitemap covers every page, post,
and category across all configured languages.

## Listing limits

`home_limit`, `category_limit`, `pages_limit` and `tags_limit` control
pagination for the corresponding listings; set any of them to `0` to
disable pagination. `show_tag_cloud` toggles the sidebar tag widget.

## Analytics and security

Set an `id` under `[analytics.google]` or `[analytics.baidu]` to inject
the matching tracker into the `<head>`. Security headers
(`X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, CSP) are
on by default and can be tuned under `[security]`.