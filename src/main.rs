mod config;
mod content;
mod markdown;
mod parse;
mod render;
mod server;
mod template;
mod value;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use content::theme_files;
use content::Site;

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
                 to the current directory. Uses the system default template
                 unless --template DIR or site.toml [theme].

OPTIONS:
    --host <H>      Bind host (default 127.0.0.1)
    --port <P>      Port (default 8080)
    --template <D>  Use a template directory instead of the default theme.
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
        Ok(site) => match server::serve(site, &host, port) {
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
        ("about.md", ABOUT_MD),
        ("posts/_index.md", POSTS_INDEX_MD),
        ("posts/_index.zh.md", POSTS_INDEX_ZH_MD),
        ("posts/hello-world.md", HELLO_MD),
        ("posts/hello-world.zh.md", HELLO_ZH_MD),
        ("notes/_index.md", NOTES_INDEX_MD),
        ("notes/_index.zh.md", NOTES_INDEX_ZH_MD),
        ("notes/tips.md", TIPS_MD),
        ("_layout/header.html", LAYOUT_HEADER),
        ("_layout/footer.html", LAYOUT_FOOTER),
        ("_layout/side.html", LAYOUT_SIDE),
        ("_layout/inject.html", LAYOUT_INJECT),
        ("_static/style.css", theme_files::STYLE),
    ];
    for (rel, content) in files {
        let p = dir.join(rel);
        if let Err(e) = write_file(&p, content) {
            eprintln!("error writing {}: {e}", p.display());
            return 1;
        }
    }

    // copy the default template so the user can customise it
    let tpl_files: Vec<(&str, &str)> = vec![
        ("template/base.html", theme_files::BASE),
        ("template/index.html", theme_files::INDEX),
        ("template/category.html", theme_files::CATEGORY),
        ("template/article.html", theme_files::ARTICLE),
        ("template/page.html", theme_files::PAGE),
        ("template/404.html", theme_files::NOT_FOUND),
        ("template/partials/header.html", theme_files::PARTIAL_HEADER),
        ("template/partials/footer.html", theme_files::PARTIAL_FOOTER),
        ("template/partials/side.html", theme_files::PARTIAL_SIDE),
        ("template/partials/inject.html", theme_files::PARTIAL_INJECT),
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
theme = "template"

[lang.en]
title = "My Blog"
description = "A demo site built with mdweb."

[lang.zh]
title = "我的博客"
description = "使用 mdweb 构建的演示站点，支持多语言。"
"#;

const INDEX_MD: &str = r#"---
title: "Welcome"
layout: "index"
---

Welcome to a blog powered by **mdweb**, a static blog engine written in pure Rust.
"#;

const ABOUT_MD: &str = r#"---
title: "About"
---

This demo shows how mdweb renders a doc directory into a blog.

- Markdown files become pages.
- Directories become categories.
- File names like `hello.zh.md` become languages.
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
date: "2024-01-15"
updated: "2024-06-01"
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
date: "2024-01-15"
updated: "2024-06-01"
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

const NOTES_INDEX_ZH_MD: &str = r#"---
title: "笔记"
summary: "随手记录的小笔记。"
---
"#;

const TIPS_MD: &str = r#"---
title: "A Few Tips"
date: "2024-03-02"
tags: ["tips"]
---

- Use double quotes around dates in frontmatter.
- Name files like `foo.en.md` / `foo.zh.md` for translations.
- Drop custom partials into `_layout/` to override the theme.
"#;

const LAYOUT_HEADER: &str = r##"<header class="site-header">
  <div class="container header-inner">
    <a class="brand" href="{{ home_url }}">
      <span class="brand-mark" aria-hidden="true">M</span>
      <span class="brand-name">{{ title }}</span>
    </a>
    <nav class="primary-nav" aria-label="Primary">
      <a class="nav-link{% if home_active %} is-active{% endif %}" href="{{ home_url }}">Home</a>
      {% for c in categories %}
      <a class="nav-link{% if c.active %} is-active{% endif %}" href="{{ c.url }}">{{ c.title }}</a>
      {% endfor %}
    </nav>
    <nav class="langs" aria-label="Languages">
      {% for l in languages %}
      <a class="lang-link{% if l.active %} is-active{% endif %}" href="{{ l.url }}" title="{{ l.title }}">{{ l.code }}</a>
      {% endfor %}
    </nav>
  </div>
</header>
"##;

const LAYOUT_FOOTER: &str = r##"<footer class="site-footer">
  <p>Powered by <a href="https://github.com/conkayyan/mdweb-rs">mdweb</a> · © {{ current_year }} {{ title }}</p>
</footer>
"##;

const LAYOUT_SIDE: &str = r##"<nav class="category-nav">
  <h3>{% if is_zh %}分类{% else %}Categories{% endif %}</h3>
  {% if categories %}
  <ul>
  {% for c in categories %}
    <li>
      <a href="{{ c.url }}"{% if c.active %} class="is-active"{% endif %}>{{ c.title }}</a>
      {% if c.children %}
      <ul>
      {% for ch in c.children %}<li><a href="{{ ch.url }}"{% if ch.active %} class="is-active"{% endif %}>{{ ch.title }}</a></li>{% endfor %}
      </ul>
      {% endif %}
    </li>
  {% endfor %}
  </ul>
  {% else %}
  <p>{% if is_zh %}暂无分类{% else %}No categories yet.{% endif %}</p>
  {% endif %}
</nav>
"##;

const LAYOUT_INJECT: &str = "<!-- put your analytics / statistic JS snippet here -->\n";