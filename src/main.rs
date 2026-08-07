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
        ("content/pages/_index.md", PAGES_INDEX_MD),
        ("content/pages/_index.zh.md", PAGES_INDEX_ZH_MD),
        ("content/pages/about.md", ABOUT_MD),
        ("content/pages/about.zh.md", ABOUT_ZH_MD),
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
        ("samples/page.md", SAMPLE_PAGE_MD),
        ("samples/post.md", SAMPLE_POST_MD),
        ("template/default/layout/header.html", theme_files::PARTIAL_HEADER),
        ("template/default/layout/footer.html", theme_files::PARTIAL_FOOTER),
        ("template/default/layout/side.html", theme_files::PARTIAL_SIDE),
        ("template/default/layout/inject.html", theme_files::PARTIAL_INJECT),
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
        ("template/default/search.html", theme_files::SEARCH),
        ("template/default/404.html", theme_files::NOT_FOUND),
        ("template/default/partials/header.html", theme_files::PARTIAL_HEADER),
        ("template/default/partials/footer.html", theme_files::PARTIAL_FOOTER),
        ("template/default/partials/side.html", theme_files::PARTIAL_SIDE),
        ("template/default/partials/inject.html", theme_files::PARTIAL_INJECT),
        ("template/default/partials/_cat_node.html", theme_files::PARTIAL_CAT_NODE),
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
categories    = "分类"
recent_posts  = "最近文章"
friend_links  = "友情链接"
no_posts      = "暂无文章。"
read_in       = "其他语言："
published     = "发布于："
updated       = "更新于："
author        = "作者："
prev          = "上一篇"
next          = "下一篇"
not_found     = "页面未找到"
not_found_desc = "您访问的页面不存在。"
search        = "搜索"
search_placeholder = "搜索…"
search_no_results = "未找到相关文章。"
rss           = "RSS 订阅"

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
layout: "index"
---

Welcome to a blog powered by **mdweb**, a static blog engine written in pure Rust.
"#;

const INDEX_ZH_MD: &str = r#"---
title: "欢迎"
layout: "index"
---

欢迎访问由 **mdweb** 驱动的博客——一个用纯 Rust 编写的静态博客引擎。
"#;

const ABOUT_MD: &str = r#"---
title: "About"
layout: "page"
---

This demo shows how mdweb renders a doc directory into a blog.

- Markdown files become pages.
- Directories become categories.
- File names like `hello.zh.md` become languages.
"#;

const ABOUT_ZH_MD: &str = r#"---
title: "关于"
layout: "page"
---

本演示展示了 mdweb 如何把一个文档目录渲染成博客。

- Markdown 文件变成页面。
- 目录变成分类。
- 像 `hello.zh.md` 这样的文件名代表不同语言。
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
- Drop custom partials into `template/default/layout/` to override the theme.
"#;

const TIPS_ZH_MD: &str = r#"---
title: "几条小贴士"
date: "2026-08-05"
tags: ["tips"]
---

- 日期字段在 frontmatter 中请用双引号包裹。
- 文件命名为 `foo.en.md` / `foo.zh.md` 即可作为不同语言版本。
- 把自定义 partial 放到 `template/<theme>/layout/` 下即可覆盖默认主题。
"#;

/// Reference sample for a single page. Used by `mdweb new page` and written
/// to `samples/page.md` by `mdweb create`. Frontmatter comments are valid
/// YAML and are skipped by the parser.
const SAMPLE_PAGE_MD: &str = r#"---
# Page title (required).
title: "Sample Page"

# Layout: "page" renders with page.html from the active theme.
layout: "page"

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

