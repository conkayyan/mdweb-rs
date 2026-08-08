use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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
    if dir.is_dir() {
        eprintln!("error: target directory already exists: {}", dir.display());
        return 1;
    }
    if dir.exists() {
        eprintln!("error: target path already exists: {}", dir.display());
        return 1;
    }

    // Wire everything from the embedded default site (the `site/` directory
    // compiled into the binary at build time): configuration, content,
    // samples, theme templates and static assets.
    for (rel, content) in mdweb::site::all() {
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
        "page" => {
            match mdweb::site::get("samples/page.md") {
                Some(sample) => cmd_new_one(name, &site, "page", sample),
                None => {
                    eprintln!("error: embedded sample page missing");
                    1
                }
            }
        }
        "post" => {
            match mdweb::site::get("samples/post.md") {
                Some(sample) => cmd_new_one(name, &site, "post", sample),
                None => {
                    eprintln!("error: embedded sample post missing");
                    1
                }
            }
        }
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

