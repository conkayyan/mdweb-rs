use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use mdweb::content::theme_files;
use mdweb::content::Site;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = env::args().collect();
    let code = run(&args);
    std::process::exit(code);
}

fn run(args: &[String]) -> i32 {
    if args.len() < 2 {
        print_help();
        return if args.len() == 1 { 0 } else { 1 };
    }
    match args[1].as_str() {
        "create" => cmd_create(&args[2..]),
        "new" => cmd_new(&args[2..]),
        "run" => cmd_run(&args[2..]),
        "-h" | "--help" | "help" => {
            print_help();
            0
        }
        "-V" | "--version" | "version" => {
            println!("mdweb {VERSION}");
            0
        }
        other => {
            eprintln!("unknown command: {other}");
            print_help();
            1
        }
    }
}

fn print_help() {
    println!(
        "mdweb {VERSION} - a static blog engine written in pure Rust.

USAGE:
    mdweb create <PATH>
    mdweb new    <TYPE> <NAME> <SITE_PATH>
    mdweb run    [PATH] [--host HOST] [--port PORT] [--template DIR]

COMMANDS:
    create <PATH>             Scaffold a demo site (docs + template + samples) at PATH.
    new <TYPE> <NAME> <PATH>  Create a new page or post in an existing site.
                              TYPE = page | post.
                              If PATH is the site root (has site.toml), post
                              defaults to content/posts/, page defaults to
                              content/pages/. Otherwise the file is placed at
                              PATH/NAME.md. NAME may contain '/' for sub-directories.
    run                       Serve a doc directory as a realtime web blog. PATH
                              defaults to the current directory. Loads theme =
                              <name> from template/<name>/ unless --template DIR
                              is given.

OPTIONS:
    --host <H>      Bind host (default 127.0.0.1)
    --port <P>      Port (default 8080)
    --template <D>  Use a template directory instead of the theme from site.toml.
    -h, --help      Show this help.
    -V, --version   Show version.
"
    );
}

/// Parse `--key value` style options.
fn parse_run_flags(args: &[String]) -> (Option<PathBuf>, String, u16, Option<PathBuf>) {
    let mut doc = None;
    let mut host = "127.0.0.1".to_string();
    let mut port = 8080u16;
    let mut tpl = None;

    let mut i = 0;
    let mut positional: Vec<String> = Vec::new();
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--host" => {
                if let Some(v) = args.get(i + 1) {
                    host = v.clone();
                    i += 1;
                }
            }
            "--port" => {
                if let Some(v) = args.get(i + 1) {
                    port = v.parse().unwrap_or(8080);
                    i += 1;
                }
            }
            "--template" => {
                if let Some(v) = args.get(i + 1) {
                    tpl = Some(PathBuf::from(v));
                    i += 1;
                }
            }
            "-h" | "--help" => {}
            _ if a.starts_with('-') => {}
            _ => positional.push(a.clone()),
        }
        i += 1;
    }
    if let Some(d) = positional.get(0) {
        doc = Some(PathBuf::from(d));
    }
    (doc, host, port, tpl)
}

fn cmd_run(args: &[String]) -> i32 {
    let (doc, host, port, tpl) = parse_run_flags(args);
    let doc = doc.unwrap_or_else(|| PathBuf::from("."));
    if !doc.is_dir() {
        eprintln!("error: doc directory not found: {}", doc.display());
        return 1;
    }
    match Site::build(&doc, tpl) {
        Ok(site) => match mdweb::server::serve(site, &host, port) {
            Ok(_) => 0,
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, content).map_err(|e| e.to_string())
}

fn cmd_create(args: &[String]) -> i32 {
    let Some(dir) = args.first() else {
        eprintln!("usage: mdweb create <PATH>");
        return 1;
    };
    let dir = PathBuf::from(dir);

    let files: Vec<(&str, &str)> = vec![
        ("site.toml", SITE_TOML),
        ("content/_index.md", INDEX_MD),
        ("content/_index.zh.md", INDEX_ZH_MD),
        // About lives at the site root: `content/about.md` → `/about/`. It
        // gets its own flat nav link in the header, separate from the Pages
        // dropdown.
        ("content/about.md", ABOUT_MD),
        ("content/about.zh.md", ABOUT_ZH_MD),
        ("content/pages/_index.md", PAGES_INDEX_MD),
        ("content/pages/_index.zh.md", PAGES_INDEX_ZH_MD),
        ("content/pages/docs/_index.md", DOCS_INDEX_MD),
        ("content/pages/docs/_index.zh.md", DOCS_INDEX_ZH_MD),
        ("content/pages/docs/guide/_index.md", GUIDE_INDEX_MD),
        ("content/pages/docs/guide/_index.zh.md", GUIDE_INDEX_ZH_MD),
        ("content/pages/docs/guide/intro.md", INTRO_MD),
        ("content/pages/docs/guide/intro.zh.md", INTRO_ZH_MD),
        ("content/pages/docs/guide/advanced/_index.md", ADVANCED_INDEX_MD),
        ("content/pages/docs/guide/advanced/_index.zh.md", ADVANCED_INDEX_ZH_MD),
        ("content/pages/docs/guide/advanced/configuration.md", CONFIGURATION_MD),
        ("content/pages/docs/guide/advanced/configuration.zh.md", CONFIGURATION_ZH_MD),
        ("content/posts/_index.md", POSTS_INDEX_MD),
        ("content/posts/_index.zh.md", POSTS_INDEX_ZH_MD),
        ("content/posts/hello-world.md", HELLO_MD),
        ("content/posts/hello-world.zh.md", HELLO_ZH_MD),
        ("content/posts/web/_index.md", WEB_INDEX_MD),
        ("content/posts/web/_index.zh.md", WEB_INDEX_ZH_MD),
        ("content/posts/web/frontend/_index.md", FRONTEND_INDEX_MD),
        ("content/posts/web/frontend/_index.zh.md", FRONTEND_INDEX_ZH_MD),
        ("content/posts/web/frontend/react.md", REACT_MD),
        ("content/posts/web/frontend/react.zh.md", REACT_ZH_MD),
        ("content/notes/_index.md", NOTES_INDEX_MD),
        ("content/notes/_index.zh.md", NOTES_INDEX_ZH_MD),
        ("content/notes/tips.md", TIPS_MD),
        ("content/notes/tips.zh.md", TIPS_ZH_MD),
        // Tutorial posts (EN + ZH) — enough to exercise home + category
        // pagination at the default `home_limit = 5`.
        ("content/posts/installing-mdweb.md", TUTORIAL_POSTS[0].1),
        ("content/posts/installing-mdweb.zh.md", TUTORIAL_POSTS[1].1),
        ("content/posts/writing-your-first-post.md", TUTORIAL_POSTS[2].1),
        ("content/posts/writing-your-first-post.zh.md", TUTORIAL_POSTS[3].1),
        ("content/posts/adding-pages.md", TUTORIAL_POSTS[4].1),
        ("content/posts/adding-pages.zh.md", TUTORIAL_POSTS[5].1),
        ("content/posts/customising-the-theme.md", TUTORIAL_POSTS[6].1),
        ("content/posts/customising-the-theme.zh.md", TUTORIAL_POSTS[7].1),
        ("content/posts/syndication-and-seo.md", TUTORIAL_POSTS[8].1),
        ("content/posts/syndication-and-seo.zh.md", TUTORIAL_POSTS[9].1),
        ("samples/page.md", SAMPLE_PAGE_MD),
        ("samples/post.md", SAMPLE_POST_MD),
        ("template/default/static/style.css", theme_files::STYLE),
    ];
    for (rel, content) in files {
        let p = dir.join(rel);
        if let Err(e) = write_file(&p, content) {
            eprintln!("error writing {}: {e}", p.display());
            return 1;
        }
    }

    // copy the default theme into template/default/ so the user can customise it
    let tpl_files: Vec<(&str, &str)> = vec![
        ("template/default/base.html", theme_files::BASE),
        ("template/default/index.html", theme_files::INDEX),
        ("template/default/category.html", theme_files::CATEGORY),
        ("template/default/article.html", theme_files::ARTICLE),
        ("template/default/page.html", theme_files::PAGE),
        ("template/default/page_section.html", theme_files::PAGE_SECTION),
        ("template/default/search.html", theme_files::SEARCH),
        ("template/default/tag.html", theme_files::TAG),
        ("template/default/tags.html", theme_files::TAGS),
        ("template/default/404.html", theme_files::NOT_FOUND),
        ("template/default/layout/header.html", theme_files::PARTIAL_HEADER),
        ("template/default/layout/footer.html", theme_files::PARTIAL_FOOTER),
        ("template/default/layout/side.html", theme_files::PARTIAL_SIDE),
        ("template/default/layout/inject.html", theme_files::PARTIAL_INJECT),
        ("template/default/layout/_cat_node.html", theme_files::PARTIAL_CAT_NODE),
        ("template/default/layout/_nav_node.html", theme_files::PARTIAL_NAV_NODE),
    ];
    for (rel, content) in tpl_files {
        let p = dir.join(rel);
        if let Err(e) = write_file(&p, content) {
            eprintln!("error writing {}: {e}", p.display());
            return 1;
        }
    }

    println!("created demo site at {}", dir.display());
    println!("  run:  mdweb run {}", dir.display());
    0
}

/// `mdweb new <TYPE> <NAME> <SITE_PATH>` — create a new page or post.
fn cmd_new(args: &[String]) -> i32 {
    if args.len() < 3 {
        eprintln!("usage: mdweb new <page|post> <NAME> <SITE_PATH>");
        return 1;
    }
    let kind = args[0].as_str();
    let name = args[1].trim();
    let site = PathBuf::from(&args[2]);

    if name.is_empty() {
        eprintln!("error: NAME must not be empty");
        return 1;
    }
    // When the site root is given (has site.toml), require it to exist.
    // Otherwise the path is a storage sub-directory and will be created
    // alongside the target file via write_file().
    if site.join("site.toml").is_file() && !site.is_dir() {
        eprintln!("error: site directory not found: {}", site.display());
        return 1;
    }

    match kind {
        "page" => cmd_new_one(name, &site, "page", SAMPLE_PAGE_MD),
        "post" => cmd_new_one(name, &site, "post", SAMPLE_POST_MD),
        other => {
            eprintln!("error: unknown type '{other}' (expected 'page' or 'post')");
            eprintln!("usage: mdweb new <page|post> <NAME> <SITE_PATH>");
            1
        }
    }
}

/// Resolve the destination file path for `new`. When `<SITE_PATH>` is the site
/// root (has `site.toml`), posts default to `content/posts/` and pages default
/// to `content/pages/`. Otherwise the file is placed directly at
/// `<SITE_PATH>/<NAME>.md` (the user has already chosen the destination).
fn target_path(site: &Path, kind: &str, name: &str) -> PathBuf {
    let file_name = if name.ends_with(".md") {
        name.to_string()
    } else {
        format!("{name}.md")
    };
    let prefix = match kind {
        "post" if site.join("site.toml").is_file() => "content/posts",
        "page" if site.join("site.toml").is_file() => "content/pages",
        _ => "",
    };
    if prefix.is_empty() {
        site.join(file_name)
    } else {
        site.join(prefix).join(file_name)
    }
}

fn cmd_new_one(name: &str, site: &Path, kind: &str, sample: &str) -> i32 {
    let target = target_path(site, kind, name);
    if target.exists() {
        eprintln!("error: file already exists: {}", target.display());
        return 1;
    }
    if let Err(e) = write_file(&target, sample) {
        eprintln!("error writing {}: {e}", target.display());
        return 1;
    }
    println!("created {kind}: {}", target.display());
    0
}

const SITE_TOML: &str = r#"title = "My Blog"
base_url = "http://localhost:8080"
author = "Jane Doe"
language = "en"
languages = ["en", "zh"]
theme = "default"

# Syndication: toggle visibility of the RSS feed and sitemap links in the
# footer. Both default to `true`; flip to `false` to hide.
show_rss = true
show_sitemap = true

# Listing limits. Set `0` to disable pagination for that listing.
home_limit = 5       # articles per page on /
category_limit = 5   # articles per page in category landings
pages_limit = 50     # pages per page in a directory landing
tags_limit = 10      # articles per page on a /tags/<tag>/ landing

# Tag cloud: show the tag widget in the sidebar (true) or hide it (false).
show_tag_cloud = true
tag_cloud_limit = 0   # max tags in the sidebar cloud; 0 = show all

[lang.en]
title = "My Blog"
display_name = "English"
description = "A demo site built with mdweb."
keywords = "blog, rust"

[lang.zh]
title = "我的博客"
display_name = "简体中文"
description = "使用 mdweb 构建的演示站点，支持多语言。"
keywords = "博客, rust"

[i18n.zh]
home          = "首页"
breadcrumb_home = "首页"
categories    = "分类"
pages         = "页面"
subpages      = "本节页面"
subcategories = "子分类"
recent_posts  = "最近文章"
tags          = "标签"
tag_list      = "标签下的文章"
friend_links  = "友情链接"
no_posts      = "暂无文章。"
read_in       = "其他语言："
published     = "发布于："
updated       = "更新于："
author        = "作者："
reading_time  = "分钟阅读"
reading_time_seconds = "秒阅读"
prev          = "上一篇"
next          = "下一篇"
prev_page     = "< 上一页"
next_page     = "下一页 >"
not_found     = "页面未找到"
not_found_desc = "您访问的页面不存在。"
search        = "搜索"
search_placeholder = "搜索…"
search_no_results = "未找到相关文章。"
rss           = "RSS 订阅"
sitemap       = "站点地图"

# Friend links — rendered in the sidebar (target="_blank").
[[friend_links]]
name = "mdweb"
url = "https://github.com/conkayyan/mdweb-rs"

[[friend_links]]
name = "Rust"
url = "https://www.rust-lang.org/"
"#;

const INDEX_MD: &str = r#"---
title: "Welcome"
---

Welcome to a blog powered by **mdweb**, a static blog engine written in pure Rust.
"#;

const INDEX_ZH_MD: &str = r#"---
title: "欢迎"
---

欢迎访问由 **mdweb** 驱动的博客——一个用纯 Rust 编写的静态博客引擎。
"#;

const ABOUT_MD: &str = r#"---
title: "About"
summary: "What this demo site is about."
---

This demo shows how mdweb renders a doc directory into a blog.

- Markdown files under `posts/` become articles.
- Folders under `posts/` become categories.
- Markdown files anywhere else (e.g. `pages/`) become standalone pages,
  with nested folders for hierarchy.
- A Markdown file directly under `content/` (like this one) becomes a
  top-level page with a flat URL (`/about/`) and a flat nav link.
- File names like `hello.zh.md` become language variants.
"#;

const ABOUT_ZH_MD: &str = r#"---
title: "关于"
summary: "本演示站点的简介。"
---

本演示展示了 mdweb 如何把一个文档目录渲染成博客。

- `posts/` 下的 Markdown 文件是文章。
- `posts/` 的子文件夹是分类。
- 其它位置（如 `pages/`）的 Markdown 文件是独立页面，
  支持嵌套目录形成层级。
- 直接放在 `content/` 下的 Markdown 文件（如本页）是顶层页面，
  URL 平铺（`/about/`），导航上作为独立链接出现。
- 像 `hello.zh.md` 这样的文件名代表不同语言版本。
"#;

const POSTS_INDEX_MD: &str = r#"---
title: "Posts"
summary: "All articles go here."
---

Articles are grouped by date and category.
"#;

const POSTS_INDEX_ZH_MD: &str = r#"---
title: "文章"
summary: "所有文章都在这里。"
---

文章按日期和分类归档。
"#;

const PAGES_INDEX_MD: &str = r#"---
title: "Pages"
summary: "Static pages live here."
---

Pages are standalone documents (about, contact, etc.) that aren't part of
the chronological feed.
"#;

const PAGES_INDEX_ZH_MD: &str = r#"---
title: "页面"
summary: "静态页面放在这里。"
---

页面是独立的内容（关于、联系等），不在文章时间线中显示。
"#;


const HELLO_MD: &str = r#"---
title: "Hello World"
date: "2026-08-01"
updated: "2026-08-04"
author: "Jane Doe"
tags: ["mdweb", "rust"]
meta:
  description: "The first post of the demo site."
---

This is the first post. Write **markdown** and save; then hit refresh.

```rust
fn main() {
    println!("Hello, mdweb!");
}
```

> Comments and rich text are supported too.
"#;

const HELLO_ZH_MD: &str = r#"---
title: "你好，世界"
date: "2026-08-01"
updated: "2026-08-04"
author: "Jane Doe"
tags: ["mdweb", "rust"]
meta:
  description: "mdweb 演示站点的第一篇文章。"
---

这是第一篇文章。使用 **Markdown** 书写，保存后刷新即可看到。

```rust
fn main() {
    println!("你好，mdweb！");
}
```
"#;

const NOTES_INDEX_MD: &str = r#"---
title: "Notes"
summary: "Quick notes and snippets."
---
"#;

const WEB_INDEX_MD: &str = r#"---
title: "Web"
summary: "Anything browser-shaped."
---

Sub-category example: nested under Posts.
"#;

const WEB_INDEX_ZH_MD: &str = r#"---
title: "Web"
summary: "一切与浏览器相关的内容。"
---

子分类示例：嵌套在「文章」之下。
"#;

const FRONTEND_INDEX_MD: &str = r#"---
title: "Frontend"
summary: "UI, components, build tools."
---

Nested two levels deep — under Posts → Web → Frontend.
"#;

const FRONTEND_INDEX_ZH_MD: &str = r#"---
title: "前端"
summary: "界面、组件、构建工具。"
---

二级嵌套示例：文章 → Web → 前端。
"#;

const REACT_MD: &str = r#"---
title: "A React Note"
date: "2026-08-07"
tags: ["react", "web"]
---

Three levels deep: Posts → Web → Frontend → this article.
"#;

const REACT_ZH_MD: &str = r#"---
title: "一篇 React 笔记"
date: "2026-08-07"
tags: ["react", "web"]
---

三级嵌套示例：文章 → Web → 前端 → 本文。
"#;

const NOTES_INDEX_ZH_MD: &str = r#"---
title: "笔记"
summary: "随手记录的小笔记。"
---
"#;

const TIPS_MD: &str = r#"---
title: "A Few Tips"
date: "2026-08-05"
tags: ["tips"]
---

- Use double quotes around dates in frontmatter.
- Name files like `foo.en.md` / `foo.zh.md` for translations.
- Edit `template/default/layout/<slot>.html` to customize the slot
  fragments (header / footer / side / inject).
"#;

const TIPS_ZH_MD: &str = r#"---
title: "几条小贴士"
date: "2026-08-05"
tags: ["tips"]
---

- 日期字段在 frontmatter 中请用双引号包裹。
- 文件命名为 `foo.en.md` / `foo.zh.md` 即可作为不同语言版本。
- 编辑 `template/default/layout/<slot>.html` 即可定制插槽片段
  （header / footer / side / inject）。
"#;

// ---------- Tutorial posts to drive pagination on `/` and `/posts/` ----------
// Sorted newest-first in the rendered feed (`sort_ts` desc). With the default
// `home_limit = 5` / `category_limit = 5`, the latest five fill page 1 and
// the older ones spill into subsequent pages.
const TUTORIAL_POSTS: &[(&str, &str)] = &[
    (
        "content/posts/installing-mdweb.md",
        "---\n\
        title: \"Installing mdweb\"\n\
        date: \"2026-08-04\"\n\
        tags: [\"tutorial\", \"setup\"]\n\
        ---\n\
        \n\
        mdweb is a single static binary. Grab a release from GitHub or build\n\
        from source with `cargo install mdweb`. Once on your `$PATH`, run:\n\
        \n\
        ```bash\n\
        mdweb create my-blog\n\
        cd my-blog\n\
        mdweb run\n\
        ```\n\
        \n\
        Then open <http://127.0.0.1:8080> and you'll see the demo site.\n\
        ",
    ),
    (
        "content/posts/installing-mdweb.zh.md",
        "---\n\
        title: \"安装 mdweb\"\n\
        date: \"2026-08-04\"\n\
        tags: [\"tutorial\", \"setup\"]\n\
        ---\n\
        \n\
        mdweb 是单一静态二进制。从 GitHub 下载 release，或用\n\
        `cargo install mdweb` 从源码构建。加入 `$PATH` 后执行：\n\
        \n\
        ```bash\n\
        mdweb create my-blog\n\
        cd my-blog\n\
        mdweb run\n\
        ```\n\
        \n\
        打开 <http://127.0.0.1:8080> 即可看到演示站点。\n\
        ",
    ),
    (
        "content/posts/writing-your-first-post.md",
        "---\n\
        title: \"Writing your first post\"\n\
        date: \"2026-08-03\"\n\
        tags: [\"tutorial\", \"content\"]\n\
        ---\n\
        \n\
        Posts live under `content/posts/` as plain Markdown. The directory\n\
        becomes a category, subdirectories become subcategories. Filenames\n\
        like `hello.zh.md` register as a Chinese variant of `hello.md`.\n\
        \n\
        Frontmatter accepts `title`, `date`, `updated`, `author`, `tags`,\n\
        `summary`, and arbitrary `extra` keys exposed to templates.\n\
        ",
    ),
    (
        "content/posts/writing-your-first-post.zh.md",
        "---\n\
        title: \"撰写第一篇文章\"\n\
        date: \"2026-08-03\"\n\
        tags: [\"tutorial\", \"content\"]\n\
        ---\n\
        \n\
        文章放在 `content/posts/` 目录下，就是普通 Markdown。目录即分类，\n\
        子目录即子分类。形如 `hello.zh.md` 的文件名注册为 `hello.md` 的\n\
        中文版本。\n\
        \n\
        frontmatter 支持 `title`、`date`、`updated`、`author`、`tags`、\n\
        `summary`，以及任意 `extra` 字段（可在模板里取到）。\n\
        ",
    ),
    (
        "content/posts/adding-pages.md",
        "---\n\
        title: \"Adding static pages\"\n\
        date: \"2026-08-02\"\n\
        tags: [\"tutorial\", \"pages\"]\n\
        ---\n\
        \n\
        Anything outside `posts/` is a **page**: about, contact, docs,\n\
        tutorials. Nested folders under `pages/` form a hierarchy and get\n\
        an instant landing page listing their children.\n\
        \n\
        A top-level `.md` file like `content/about.md` becomes `/about/` —\n\
        perfect for one-off links that don't deserve their own section.\n\
        ",
    ),
    (
        "content/posts/adding-pages.zh.md",
        "---\n\
        title: \"添加静态页面\"\n\
        date: \"2026-08-02\"\n\
        tags: [\"tutorial\", \"pages\"]\n\
        ---\n\
        \n\
        `posts/` 之外的任何内容都是**页面**：关于、联系、文档、教程。\n\
        `pages/` 下嵌套的目录形成层级，并自动生成子页面列表的 landing。\n\
        \n\
        顶层的 `.md` 文件（如 `content/about.md`）会变成 `/about/`——\n\
        适合不需要独立分区的快捷链接。\n\
        ",
    ),
    (
        "content/posts/customising-the-theme.md",
        "---\n\
        title: \"Customising the theme\"\n\
        date: \"2026-07-30\"\n\
        tags: [\"tutorial\", \"theme\"]\n\
        ---\n\
        \n\
        Override individual files under `template/default/`. mdweb loads\n\
        anything you ship there on top of the embedded defaults, so a\n\
        single `layout/header.html` is enough to recolour the navigation.\n\
        \n\
        Slots available: `header`, `footer`, `side`, `inject`. Use\n\
        `inject.html` to add analytics snippets before `</head>`.\n\
        ",
    ),
    (
        "content/posts/customising-the-theme.zh.md",
        "---\n\
        title: \"自定义主题\"\n\
        date: \"2026-07-30\"\n\
        tags: [\"tutorial\", \"theme\"]\n\
        ---\n\
        \n\
        在 `template/default/` 下覆盖任意文件即可。mdweb 会先加载内嵌的\n\
        默认模板，再用你提供的文件覆盖，所以你只需替换 `layout/header.html`\n\
        就能重新着色导航栏。\n\
        \n\
        可用插槽：`header`、`footer`、`side`、`inject`。用 `inject.html`\n\
        在 `</head>` 之前注入统计脚本。\n\
        ",
    ),
    (
        "content/posts/syndication-and-seo.md",
        "---\n\
        title: \"RSS, sitemap and SEO\"\n\
        date: \"2026-07-28\"\n\
        tags: [\"tutorial\", \"seo\"]\n\
        ---\n\
        \n\
        mdweb generates `/rss.xml`, `/sitemap.xml`, and a `<link rel=alternate>`\n\
        in the document head. Toggle the footer links via `show_rss` and\n\
        `show_sitemap` in `site.toml`. The sitemap covers every page, post,\n\
        and category across all configured languages.\n\
        ",
    ),
    (
        "content/posts/syndication-and-seo.zh.md",
        "---\n\
        title: \"RSS、站点地图与 SEO\"\n\
        date: \"2026-07-28\"\n\
        tags: [\"tutorial\", \"seo\"]\n\
        ---\n\
        \n\
        mdweb 自动生成 `/rss.xml`、`/sitemap.xml`，并在 `<head>` 中放置\n\
        `<link rel=alternate>`。通过 `site.toml` 中的 `show_rss` 和\n\
        `show_sitemap` 控制底部链接显示。站点地图覆盖所有语言下的所有\n\
        页面、文章和分类。\n\
        ",
    ),
];

// ---------- Multi-level nested pages example (EN + ZH) ----------
// pages/docs/_index.md and pages/docs/guide/_index.md are landing pages
// (their _index.md frontmatter provides the section title and intro).
// The leaf .md files are regular pages. The nav dropdown renders them
// recursively: Pages → Docs → Guide → {Intro, Advanced → Configuration}.

const DOCS_INDEX_MD: &str = r#"---
title: "Docs"
summary: "Guides, tutorials, and references."
---

Welcome to the docs. Pages are organised hierarchically — pick a section
below, or browse the full tree from the nav.
"#;

const DOCS_INDEX_ZH_MD: &str = r#"---
title: "文档"
summary: "指南、教程与参考。"
---

欢迎来到文档中心。页面以层级方式组织——可在下方选择章节，
也可以通过顶部导航浏览完整结构。
"#;

const GUIDE_INDEX_MD: &str = r#"---
title: "Guide"
summary: "Step-by-step walkthroughs."
---

Start here if you're new to mdweb.
"#;

const GUIDE_INDEX_ZH_MD: &str = r#"---
title: "指南"
summary: "循序渐进的教程。"
---

如果你是 mdweb 新手，请从这里开始。
"#;

const INTRO_MD: &str = r#"---
title: "Introduction"
summary: "What mdweb is and how it fits your workflow."
---

mdweb turns a folder of Markdown into a static site. The layout is purely
directory-driven: `posts/` for blog articles, anything else for pages,
nested folders for hierarchy.
"#;

const INTRO_ZH_MD: &str = r#"---
title: "简介"
summary: "mdweb 是什么，适合怎样的工作流。"
---

mdweb 把一个 Markdown 文件夹变成静态站点。结构由目录决定：
`posts/` 放博客文章，其它位置放页面，嵌套目录构成层级。
"#;

const ADVANCED_INDEX_MD: &str = r#"---
title: "Advanced"
summary: "Configuration and edge cases."
---

Deeper topics once you're past the basics.
"#;

const ADVANCED_INDEX_ZH_MD: &str = r#"---
title: "进阶"
summary: "配置与边界情况。"
---

掌握基础之后的深入话题。
"#;

const CONFIGURATION_MD: &str = r#"---
title: "Configuration"
summary: "site.toml and per-language overrides."
---

Edit `site.toml` to set the title, base URL, and theme. Per-language
strings live under `[lang.<code>]` and `[i18n.<code>]`.
"#;

const CONFIGURATION_ZH_MD: &str = r#"---
title: "配置"
summary: "site.toml 与各语言覆盖。"
---

编辑 `site.toml` 来设置标题、基础 URL 与主题。各语言字符串放在
`[lang.<code>]` 与 `[i18n.<code>]` 之下。
"#;

/// Reference sample for a single page. Used by `mdweb new page` and written
/// to `samples/page.md` by `mdweb create`. Frontmatter comments are valid
/// YAML and are skipped by the parser.
///
/// Note: there is no `layout:` field. An article becomes a page by living
/// outside `posts/`; the directory itself decides the rendering template.
const SAMPLE_PAGE_MD: &str = r#"---
# Page title (required).
title: "Sample Page"

# One-line summary (optional). Shown in category listings and used as
# the default value for <meta name="description"> when meta.description
# is unset.
summary: "A one-line description of the page."

# Arbitrary metadata exposed to templates as page.meta (optional).
meta:
  description: "A longer description for SEO."
  keywords: "page, sample, mdweb"

# Extra fields are available in templates as page.<name>.
custom_field: "any value"
---

Replace this with your page content. Pages are rendered with
`page.html` from the active theme and are typically used for
static content like about, contact, etc.
"#;

/// Reference sample for a single post. Used by `mdweb new post` and written
/// to `samples/post.md` by `mdweb create`. Frontmatter comments are valid
/// YAML and are skipped by the parser.
const SAMPLE_POST_MD: &str = r#"---
# Post title (required).
title: "Sample Post"

# Creation date (quoted strings recommended).
date: "2026-08-08"

# Last update date (optional).
updated: "2026-08-08"

# Author (optional; falls back to site.toml's author).
author: "Author Name"

# Tags (optional array of strings).
tags: ["mdweb", "rust"]

# One-line summary (optional). Shown in category listings and used as
# the default value for <meta name="description"> when meta.description
# is unset.
summary: "A one-line description."

# Draft: true hides the post from listings and feeds.
draft: false

# Arbitrary metadata exposed to templates as post.meta (optional).
meta:
  description: "A longer description for SEO."
  keywords: "post, sample, mdweb"

# Extra fields are available in templates as post.<name>.
custom_field: "any value"
---

Replace this with your post content. Posts are rendered with
`article.html` from the active theme and appear in category
listings and chronological feeds.

```rust
fn main() {
    println!("Hello, mdweb!");
}
```
"#;

