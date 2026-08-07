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
    mdweb run   [PATH] [--host HOST] [--port PORT] [--template DIR]
    mdweb new   <PATH>

COMMANDS:
    new <PATH>   Create a demo site (docs + template) at PATH.
    run          Serve a doc directory as a realtime web blog. PATH defaults
                 to the current directory. Loads theme = <name> from
                 template/<name>/ unless --template DIR is given.

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

fn cmd_new(args: &[String]) -> i32 {
    let Some(dir) = args.first() else {
        eprintln!("usage: mdweb new <PATH>");
        return 1;
    };
    let dir = PathBuf::from(dir);

    let files: Vec<(&str, &str)> = vec![
        ("site.toml", SITE_TOML),
        ("_index.md", INDEX_MD),
        ("_index.zh.md", INDEX_ZH_MD),
        ("about.md", ABOUT_MD),
        ("about.zh.md", ABOUT_ZH_MD),
        ("posts/_index.md", POSTS_INDEX_MD),
        ("posts/_index.zh.md", POSTS_INDEX_ZH_MD),
        ("posts/hello-world.md", HELLO_MD),
        ("posts/hello-world.zh.md", HELLO_ZH_MD),
        ("posts/web/_index.md", WEB_INDEX_MD),
        ("posts/web/_index.zh.md", WEB_INDEX_ZH_MD),
        ("posts/web/frontend/_index.md", FRONTEND_INDEX_MD),
        ("posts/web/frontend/_index.zh.md", FRONTEND_INDEX_ZH_MD),
        ("posts/web/frontend/react.md", REACT_MD),
        ("posts/web/frontend/react.zh.md", REACT_ZH_MD),
        ("notes/_index.md", NOTES_INDEX_MD),
        ("notes/_index.zh.md", NOTES_INDEX_ZH_MD),
        ("notes/tips.md", TIPS_MD),
        ("notes/tips.zh.md", TIPS_ZH_MD),
        ("_layout/header.html", theme_files::PARTIAL_HEADER),
        ("_layout/footer.html", theme_files::PARTIAL_FOOTER),
        ("_layout/side.html", theme_files::PARTIAL_SIDE),
        ("_layout/inject.html", theme_files::PARTIAL_INJECT),
        ("_static/style.css", theme_files::STYLE),
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
- Drop custom partials into `_layout/` to override the theme.
"#;

const TIPS_ZH_MD: &str = r#"---
title: "几条小贴士"
date: "2026-08-05"
tags: ["tips"]
---

- 日期字段在 frontmatter 中请用双引号包裹。
- 文件命名为 `foo.en.md` / `foo.zh.md` 即可作为不同语言版本。
- 把自定义 partial 放到 `_layout/` 下即可覆盖默认主题。
"#;

