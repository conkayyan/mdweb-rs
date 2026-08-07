# mdweb

**mdweb** 是一个用纯 Rust 编写的静态博客引擎。它将一个目录下的 Markdown
文档渲染成实时更新的多语言博客——无需构建步骤、无需数据库、无需任何前端框架，
并且**不依赖任何外部 crate**（仅使用标准库）。

它是一个完整的独立程序：`mdweb new` 生成演示站点，`mdweb run` 将其作为实时博客
启动。你只需要编辑 Markdown 文件并刷新浏览器。

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

## 功能特性

- **文档即站点结构** — `doc/` 目录树会自动渲染为分类层级（`posts/` → `/posts/`，
  `notes/` → `/notes/`，以此类推）。
- **布局插槽** — `_layout/` 目录存放 `header`、`footer`、`side`、`inject` 片段；
  `inject` 插槽天然适合放置统计/分析 JS 代码。
- **可配置元信息** — 站点全局配置通过 `site.toml`，文章元信息通过 frontmatter。
  模板 + 参数模式，方便 DIY 定制。
- **多语言支持** — 一个站点多种语言，通过文件名后缀识别，例如 `hello.zh.md`。
  支持默认语言与可选的语言前缀 URL。
- **文章元信息** — 创建/更新时间、作者、标签、自定义 `meta` 映射。
- **`mdweb new`** — 一键生成演示站点（doc + template）。
- **`mdweb run`** — 实时启动任意 doc 目录；除非传入 `--template` 或在 `site.toml`
  中配置 `theme`，否则使用系统内置默认模板。
- **内置 Markdown 渲染器** — 标题、段落、围栏代码块、引用、有序/无序（可嵌套）列表、
  分隔线、行内强调/代码/链接/图片/删除线，并支持原始 HTML 透传。零外部依赖。

## 编译构建

需要稳定的 Rust 工具链。

```bash
cargo build --release
./target/release/mdweb --help
```

## 快速开始

```bash
# 1. 创建演示站点（doc + 一份默认模板的副本）
mdweb new my-blog

# 2. 启动服务（默认端口 8080，也可自定义）
mdweb run my-blog --port 8080

# 3. 打开 http://127.0.0.1:8080/ ，然后开始编辑 Markdown 文件
```

## 命令行

```
mdweb <VERSION> - 一个用纯 Rust 编写的静态博客引擎。

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

## 站点目录结构

```
my-blog/
├── site.toml              # 全局站点配置（TOML）
├── _index.md              # 首页内容（frontmatter + markdown）
├── about.md               # 普通页面（layout: "page" 时使用 page.html 渲染）
├── _layout/               # 文档级布局插槽，优先级高于主题 partials
│   ├── header.html
│   ├── footer.html
│   ├── side.html
│   └── inject.html        # 在此放置统计 / 分析 JS 代码
├── _static/               # 额外静态资源，通过 /static/ 提供
├── posts/
│   ├── _index.md          # /posts/ 分类页
│   ├── hello-world.md     # 默认语言的文章
│   └── hello-world.zh.md  # 同一篇文章的其他语言版本
├── notes/
│   ├── _index.md
│   └── tips.md
└── template/              # 站点本地模板主题（见“主题”一节）
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

说明：

- 目录下的 `_index.md` 即该分类的索引页。
- 目录 `_index.md` 的 title/summary/description 用于配置分类信息。
- `_layout/` 下的文件会覆盖主题 `partials/` 中同名文件。
- `_static/` 中的文件通过 `/static/<path>` 对外提供。

## 配置（`site.toml`）

```toml
title = "My Blog"            # 站点标题（所有语言的兜底值）
base_url = "http://localhost:8080"
author = "Jane Doe"
language = "en"              # 默认语言（无前缀 URL）
languages = ["en", "zh"]     # 启用的语言列表；未列出的语言将被忽略
theme = "default"           # template/ 下的子目录名；留空等价于内置默认

[lang.en]                    # 各语言的覆盖项
title = "My Blog"
display_name = "English"     # 语言下拉菜单中显示的标签
description = "A demo site built with mdweb."
keywords = "blog, rust"

[lang.zh]
title = "我的博客"
display_name = "简体中文"
description = "使用 mdweb 构建的演示站点，支持多语言。"
keywords = "博客, rust"

[i18n.zh]                    # 界面文案覆盖；缺失键回退到英文
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

[meta]                       # 任意元信息，模板中以 config.meta 访问
description = "A demo site"

[params]                     # 任意参数，模板中以 config.params 访问
github = "https://github.com/conkayyan/mdweb-rs"

# 友情链接（渲染在 sidebar，target="_blank"）。每条 [[friend_links]]
# 在 friend_links 数组里对应一个 { name, url }。
[[friend_links]]
name = "mdweb"
url = "https://github.com/conkayyan/mdweb-rs"

[[friend_links]]
name = "Rust"
url = "https://www.rust-lang.org/zh/"
```

### 多语言下拉菜单

页眉会显示语言切换下拉菜单。每个语言的标签通过 `[lang.<code>].display_name`
设置；若未设置，则显示原始语言代码（`zh`、`en` 等）。

```toml
[lang.zh]
title = "我的博客"
display_name = "简体中文"
```

### 界面文案（i18n）

默认模板内置英文文案，并通过 `t.*` 上下文键查找标签。可在 `[i18n.<code>]`
中按语言覆盖：

```toml
[i18n.zh]
categories   = "分类"
recent_posts = "最近文章"
friend_links = "友情链接"
no_posts     = "暂无文章。"
```

可用键：`home`、`categories`、`recent_posts`、`friend_links`、`no_posts`、
`read_in`、`published`、`updated`、`author`、`prev`、`next`、`not_found`、
`not_found_desc`。缺失的键会回退到英文，再回退到内置默认值，最后回退到
键名字符串本身。

## Frontmatter

每个 Markdown 文件都可以以 YAML 风格的 `---` 块开头。支持的字段：

```yaml
---
title: "Hello World"     # 页面标题
date: "2024-01-15"       # 创建时间（建议用引号括起来）
updated: "2024-06-01"    # 最后更新时间
author: "Jane Doe"
tags: ["mdweb", "rust"]
summary: "一行简介。"
layout: "page"           # "article"（默认）或 "page"；page 使用 page.html 渲染
draft: false             # true 时隐藏该文章
meta:                    # 任意映射，模板中以 article.meta 访问
  description: "更长的描述"
---
```

同时支持 TOML 风格的 `+++` 块。

## 主题

主题是 `template/<name>/` 下的模板目录。在 `site.toml` 中通过 `theme = "<name>"`
切换（留空等价于使用内置 `default`）。`mdweb new` 会写出 `template/default/`，
你可以直接修改，或复制成 `template/<新名>/` 来切换。

模板解析顺序：

1. 文档目录里的 `_layout/<name>.html`（最高优先级）
2. `template/<theme>/partials/<name>.html`
3. 内置默认 partial（Themes）

主题即一组模板目录。站点本地 `template/` 目录会覆盖内置默认主题。`mdweb new`
会复制一份默认主题，方便你随意修改。

插槽 / partial 的解析优先级：

1. doc 目录下的 `_layout/<name>.html`（优先级最高）
2. `<theme>/partials/<name>.html`
3. 内置默认 partial

### 模板语法

- **输出**：`{{ expr }}` 或 `{{ expr | safe }}`（`safe` 过滤器跳过 HTML 转义）。
- **块**：`{% block name %} ... {% endblock name %}` — 可被子页面覆盖。
- **继承**：`{% extends "base.html" %}`。
- **条件**：`{% if expr %} ... {% else %} ... {% endif %}`。
- **循环**：`{% for x in xs %} ... {% endfor %}` — 可使用 `x_index` 获取从 0 开始的索引。
- **注释**：`{# ... #}`。

### 模板上下文变量

全局可用：

| 变量 | 说明 |
| --- | --- |
| `config.title` / `config.base_url` / `config.author` / `config.language` | 站点配置 |
| `config.languages` | 语言代码列表 |
| `config.meta` / `config.params` | `site.toml` 中的任意映射 |
| `site.title` / `site.lang` | 站点标题与当前语言 |
| `title` | 当前页面 / 站点标题 |
| `description` / `keywords` | 当前语言的描述 / 关键词 |
| `lang` | 当前语言代码 |
| `languages` | 语言切换列表：`{ code, display_name, url, active }` |
| `t` | 界面文案映射（键：`home`、`categories`、`recent_posts` 等） |
| `current_lang_display_name` | 当前语言的显示名（如按钮标签） |
| `categories` | 分类树（嵌套的 `{ title, url, children }`） |
| `home_url` | 当前语言首页的 URL |
| `current_url` | 当前请求 URL |
| `current_year` | 当前年份（用于版权页脚） |
| `header` / `side` / `footer` / `inject` | 渲染后的布局插槽 |

按模板区分：

- `index.html` → `home` `{ content, articles: [...] }`
- `category.html` → `category` `{ title, slug, url, description, content, articles, children }`
- `article.html` / `page.html` → `article` `{ title, lang, url, date, updated, author, tags, content, meta, fields, ... }`
- `404.html` → `page` `{ title: "Not Found" }`

## 多语言 URL

默认语言通过无前缀路径提供（`/posts/hello-world/`）；其他语言带前缀
（`/zh/posts/hello-world/`）。语言切换器使用 `languages[].url`。

**v0.2.0 不兼容变更：** `languages[].title`（实际返回的是*站点标题*而非语言
标签，容易产生误解）已被替换为 `languages[].display_name`。布尔上下文键
`is_zh` 已移除，请改用 `t.*` 键。`lang_meta` 映射与 `lang_active` 标志亦已
移除——当前语言代码请用 `lang`，语言按钮标签请用 `current_lang_display_name`。
