# mdweb

**mdweb** 是一个用纯 Rust 编写的静态博客引擎。它将一个目录下的 Markdown
文档渲染成实时更新的多语言博客——无需构建步骤、无需数据库、无需任何前端框架，
并且**不依赖任何外部 crate**（仅使用标准库）。

它是一个完整的独立程序：`mdweb create` 生成演示站点，`mdweb new` 在已有站点中
创建新的 page 或 post，`mdweb run` 将其作为实时博客启动。你只需要编辑 Markdown
文件并刷新浏览器。

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

## 功能特性

- **文档即站点结构** — `doc/` 目录树会自动渲染为分类层级（`posts/` → `/posts/`，
  `notes/` → `/notes/`，以此类推）。
- **布局插槽** — `template/<theme>/layout/` 目录存放 `header`、`footer`、
  `side`、`inject` 片段；
  `inject` 插槽天然适合放置统计/分析 JS 代码。
- **可配置元信息** — 站点全局配置通过 `site.toml`，文章元信息通过 frontmatter。
  模板 + 参数模式，方便 DIY 定制。
- **多语言支持** — 一个站点多种语言，通过文件名后缀识别，例如 `hello.zh.md`。
  支持默认语言与可选的语言前缀 URL。
- **文章元信息** — 创建/更新时间、作者、标签、自定义 `meta` 映射。
- **`mdweb create`** — 一键生成演示站点（doc + template + samples）。
- **`mdweb new`** — 在已有站点中创建新的 page 或 post。
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
mdweb create my-blog

# 2. 启动服务（默认端口 8080，也可自定义）
mdweb run my-blog --port 8080

# 3. 打开 http://127.0.0.1:8080/ ，然后开始编辑 Markdown 文件
```

## 命令行

```
mdweb <VERSION> - 一个用纯 Rust 编写的静态博客引擎。

USAGE:
    mdweb create <PATH>
    mdweb new    <TYPE> <NAME> <SITE_PATH>
    mdweb run    [PATH] [--host HOST] [--port PORT] [--template DIR]

COMMANDS:
    create <PATH>              在 PATH 处搭建演示站点（docs + template + samples）。
    new <TYPE> <NAME> <PATH>   在已有站点中创建单个 page 或 post。
                               TYPE = page | post。
                               若 PATH 是站点根（含 site.toml），post 默认落到
                               content/posts/，page 默认落到 content/pages/；
                               否则文件直接放在 PATH/NAME.md。
                               NAME 中可包含 '/'，用于在子目录中创建文件。
    run                        将一个 doc 目录以实时博客形式启动。PATH 默认
                               为当前目录。除非传入 --template DIR 或在
                               site.toml 中设置 [theme]，否则使用系统默认模板。

OPTIONS:
    --host <H>      Bind host (default 127.0.0.1)
    --port <P>      Port (default 8080)
    --template <D>  Use a template directory instead of the default theme.
    -h, --help      Show this help.
    -V, --version   Show version.
```

## 创建 page 和 post

`mdweb new` 会在已有站点中创建单个 page 或 post。它需要三个参数：类型（`page`
或 `post`）、文件名，以及站点路径（也可以是该站点内的子目录）。

```bash
# 1. 搭建站点
mdweb create ./my-blog
mdweb run ./my-blog

# 2. 新增一篇文章（PATH 是站点根，文件落在 content/posts/）
mdweb new post hello-world ./my-blog
# → ./my-blog/content/posts/hello-world.md

# 3. 新增一个页面（PATH 是站点根，文件落在 content/pages/）
mdweb new page about ./my-blog
# → ./my-blog/content/pages/about.md

# 4. 在已有分类下新增文章
mdweb new post my-post ./my-blog/content/posts/web
# → ./my-blog/content/posts/web/my-post.md

# 5. 在子目录中新增页面（父目录会自动创建）
mdweb new page contact ./my-blog/content/pages/info
# → ./my-blog/content/pages/info/contact.md

# 6. NAME 中可包含 '/' 表示子目录
mdweb new post tips/shortcuts ./my-blog
# → ./my-blog/content/posts/tips/shortcuts.md
```

说明：

- 不写 `.md` 后缀会自动补上。
- 目标文件已存在时直接报错，不会覆盖。
- 文件内容来自 `samples/page.md` 与 `samples/post.md`（由 `mdweb create`
  写入）。两份样例都是带注释的完整 reference，复制后修改 frontmatter 与
  正文即可。
- Frontmatter 中的 `# ...` 注释出现在样例里，是合法的 YAML：解析器会跳过
  这些注释，所以文件开箱即用即可正确渲染。

## 站点目录结构

```
my-blog/
├── site.toml              # 全局站点配置（TOML）
├── samples/               # 带注释的 page / post 参考样例
│   ├── page.md            # `mdweb new page` 的素材
│   └── post.md            # `mdweb new post` 的素材
├── content/               # 所有写作内容都在这里——路径即路由
│   ├── _index.md          # → /          （首页，每个语言一个）
│   ├── _index.zh.md       # → /zh/
│   ├── pages/             # 页面 → /pages/<slug>/；不进时间线
│   │   ├── _index.md
│   │   └── about.md
│   └── posts/             # 文章 → /posts/<slug>/；出现在列表与 Feed 中
│       ├── _index.md      # /posts/ 分类页
│       ├── hello-world.md
│       ├── hello-world.zh.md
│       └── web/           # 多级子分类
│           ├── _index.md
│           └── frontend/
│               ├── _index.md
│               └── react.md
└── template/              # 站点本地模板主题（见“主题”一节）
    └── default/           # 当前主题（site.toml 中 theme = "default"）
├── base.html
    ├── index.html
    ├── category.html
    ├── article.html
    ├── page.html
    ├── tag.html
    ├── tags.html
    ├── 404.html
        ├── layout/        # 插槽片段（header / footer / side / inject）
        │   ├── header.html
        │   ├── footer.html
        │   ├── side.html
        │   └── inject.html
        └── static/        # 站点级静态资源，通过 /static/ 提供
            └── style.css
```

说明：

- `content/` 是透明容器，不出现在 URL 中。
  `/content/posts/hello-world.md` 对外的访问路径是 `/posts/hello-world/`；
  `content/_index.md` 对外是 `/`。
- `content/_index.md` 是首页；`content/` 顶层只接受 `_index.md` 与
  `_index.<lang>.md`，其他顶层文件会被忽略。
- 目录下的 `_index.md` 即该分类的索引页。
- 目录 `_index.md` 的 title/summary/description 用于配置分类信息。
- 任意文档 frontmatter 里的 `tags` 会生成每种语言的标签云（侧栏）与
  `/tags/<tag>/` 标签列表页（见「标签页面」一节）。
- 插槽片段在 `template/<theme>/layout/`（header / footer / side / inject），
  直接编辑即可定制主题。
- `template/<theme>/static/` 中的文件通过 `/static/<path>` 对外提供。

## 静态资源

把站点自有的资源（CSS、图片、字体、favicon 等）放进 `template/<theme>/static/`。
服务器会把 `template/<theme>/static/<path>` 映射到 `/static/<path>`，
目录下任何文件都能直接访问。换主题时新主题也有自己的 `static/` 目录。

```
template/default/static/
├── style.css            → /static/style.css
├── favicon.ico          → /static/favicon.ico
└── images/
    ├── avatar.png       → /static/images/avatar.png
    └── hero.jpg         → /static/images/hero.jpg
```

### 在 HTML 中引用

模板里建议用绝对路径——不会因为模板移动而出错：

```html
<link rel="stylesheet" href="/static/style.css">
<img src="/static/images/avatar.png" alt="avatar">
```

### 在 CSS 中引用

CSS 的 `url(...)` 是**由浏览器**按 CSS 文件的 URL 来解析相对路径的，
而不是文件系统路径。因为 `style.css` 暴露在 `/static/style.css`，
它的同级图片必须放在 `/static/` 之下：

```css
/* template/default/static/style.css */
body {
  background-image: url("./images/bg.png");   /* → /static/images/bg.png  ✓ */
  background-image: url("images/bg.png");     /* → /static/images/bg.png  ✓ */
  background-image: url("../images/bg.png");  /* → /images/bg.png         ✗ */
}
```

保持在 `/static/` 命名空间内——`../` 会跳出静态目录，导致 404。

## 配置（`site.toml`）

```toml
title = "My Blog"            # 站点标题（所有语言的兜底值）
base_url = "http://localhost:8080"
author = "Jane Doe"
language = "en"              # 默认语言（无前缀 URL）
languages = ["en", "zh"]     # 启用的语言列表；未列出的语言将被忽略
theme = "default"           # template/ 下的子目录名；留空等价于内置默认

# 列表分页。设为 `0` 可对该列表禁用分页。
home_limit = 10      # 首页每页文章数（/）
category_limit = 20  # 分类页每页文章数
pages_limit = 50     # 目录落地页每页数量
tags_limit = 20      # 标签页每页文章数（/tags/<标签>/）

# 标签云：是否在侧边栏显示标签小部件（true）或隐藏（false）。
show_tag_cloud = true
tag_cloud_limit = 0   # 侧边栏标签云最多显示几个标签；0 = 全部显示

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

可用键：`home`、`categories`、`recent_posts`、`tags`、`tag_list`、
`friend_links`、`no_posts`、`read_in`、`published`、`updated`、`author`、
`reading_time`、`reading_time_seconds`、
`prev`、`next`、`not_found`、`not_found_desc`。缺失的键会回退到英文，再回退到
内置默认值，最后回退到键名字符串本身。

## 标签页面

每篇文档 frontmatter 里的 `tags` 会生成每种语言的标签云。侧边栏显示一个**标签云**（每个标签按其被多少篇文档使用加权），在 `site.toml` 中设置
`show_tag_cloud = false` 即可完全隐藏。标签**处处可点击**——侧边栏云、文章页、
页面列表、搜索结果里都是如此——每个链接指向对应的标签列表：

```text
/tags/              ← 全部标签索引（按语言）
/tags/<标签>/        ← 默认语言的该标签列表
/<lang>/tags/<标签>/ ← 其它语言
```

`tag_cloud_limit` 控制侧边栏云最多显示几个标签（`0` = 全部显示），按使用次数倒序，
每个标签以 `名称(次数)` 形式渲染。`/tags/`（及 `/<lang>/tags/`）列出当前语言下的
**所有**标签，同样以 `名称(次数)` 形式显示，链接到对应列表页。
标签列表页显示带有该标签的所有文档，按时间倒序，带有面包屑
（`首页 › 标签 › <标签>`）以及由 `tags_limit` 控制的分页（`?page=N`）。
含空格或标点的标签名在 URL 中会被百分号转义（`my tag` → `/tags/my%20tag/`）。

分类列表页同样带有面包屑（`首页 › 文章 › Web`）。

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

1. 文档目录里的 `template/<theme>/layout/<name>.html`（最高优先级）
2. 内置默认 partial（Themes）

主题即一组模板目录。站点本地 `template/` 目录会覆盖内置默认主题。`mdweb create`
会复制一份默认主题，方便你随意修改。

插槽 / partial 的解析优先级：

1. doc 目录下的 `template/<theme>/layout/<name>.html`（优先级最高）
2. 内置默认 partial

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
| `tags` | 当前语言的标签云：`[{ name, url, count }, …]`（数量上限由 `tag_cloud_limit` 控制） |
| `tag_cloud_limit` | 侧边栏标签云最多显示的标签数（`0` = 全部显示） |
| `show_tag_cloud` | 是否启用侧边栏标签云小部件 |
| `home_url` | 当前语言首页的 URL |
| `current_url` | 当前请求 URL |
| `current_year` | 当前年份（用于版权页脚） |
| `header` / `side` / `footer` / `inject` | 渲染后的布局插槽 |

按模板区分：

- `index.html` → `home` `{ content, articles: [...] }`
- `category.html` → `category` `{ title, slug, url, description, content, articles, children }`
- `tag.html` → `tag` `{ name, title, url, articles, pagination, total }`
- `tags.html` → `tags_index` `{ title, url, tags: [{ name, url, count }], total }`
- `article.html` / `page.html` → `article` `{ title, lang, url, date, updated, author, tags, tag_links, content, meta, fields, ... }`
- `404.html` → `page` `{ title: "Not Found" }`

## 多语言 URL

默认语言通过无前缀路径提供（`/posts/hello-world/`）；其他语言带前缀
（`/zh/posts/hello-world/`）。语言切换器使用 `languages[].url`。

**v0.2.0 不兼容变更：** `languages[].title`（实际返回的是*站点标题*而非语言
标签，容易产生误解）已被替换为 `languages[].display_name`。布尔上下文键
`is_zh` 已移除，请改用 `t.*` 键。`lang_meta` 映射与 `lang_active` 标志亦已
移除——当前语言代码请用 `lang`，语言按钮标签请用 `current_lang_display_name`。
