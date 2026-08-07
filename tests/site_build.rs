//! End-to-end integration tests that exercise `Site::build` + the rendering
//! pipeline through the public crate API. Lives outside `src/` so it can only
//! touch `pub` items — anything it needs must be re-exported.

use mdweb::content::Site;
use std::path::{Path, PathBuf};

fn tempdir(prefix: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "mdweb-it-{prefix}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, body).unwrap();
}

fn build_site_from_str(toml: &str) -> Site {
    let dir = tempdir("build");
    write(&dir, "site.toml", toml);
    Site::build(&dir, None).expect("build")
}

#[test]
fn translation_entries_carry_display_name() {
    let dir = tempdir("tr");
    write(
        &dir,
        "site.toml",
        r#"
        languages = ["en", "zh"]

        [lang.zh]
        display_name = "简体中文"
        "#,
    );
    write(&dir, "hello.md", "---\ntitle: Hello\n---\nbody\n");
    write(&dir, "hello.zh.md", "---\ntitle: 你好\n---\nbody\n");

    let site = Site::build(&dir, None).expect("build");
    let hello_en = site
        .articles
        .iter()
        .find(|a| a.lang == "en" && a.slug == "hello")
        .expect("en article");
    assert_eq!(hello_en.translations.len(), 1);
    let tr = hello_en.translations[0].as_map().expect("translation map");
    assert_eq!(tr.get("lang").and_then(|v| v.as_str()), Some("zh"));
    assert_eq!(
        tr.get("display_name").and_then(|v| v.as_str()),
        Some("简体中文")
    );
}

#[test]
fn home_content_resolves_per_language() {
    let dir = tempdir("home");
    write(&dir, "site.toml", r#"languages = ["en", "zh"]"#);
    write(
        &dir,
        "_index.md",
        "---\ntitle: Home\nlayout: index\n---\nEnglish body\n",
    );
    write(
        &dir,
        "_index.zh.md",
        "---\ntitle: 首页\nlayout: index\n---\n中文内容\n",
    );

    let site = Site::build(&dir, None).expect("build");
    assert_eq!(
        site.home_content.get("en").map(String::as_str),
        Some("<p>English body</p>\n")
    );
    assert_eq!(
        site.home_content.get("zh").map(String::as_str),
        Some("<p>中文内容</p>\n")
    );

    let en = site.home_value("en");
    let en = en.as_map().expect("en map");
    assert_eq!(en.get("content").and_then(|v| v.as_str()), Some("<p>English body</p>\n"));
    let zh = site.home_value("zh");
    let zh = zh.as_map().expect("zh map");
    assert_eq!(zh.get("content").and_then(|v| v.as_str()), Some("<p>中文内容</p>\n"));
}

#[test]
fn render_context_exposes_t_and_current_lang_display_name() {
    let site = build_site_from_str(
        r#"
        languages = ["en", "zh"]

        [lang.zh]
        display_name = "简体中文"

        [i18n.zh]
        categories = "分类"
        "#,
    );
    let html = mdweb::render::render_home(&site, "zh").expect("render");
    assert!(html.contains("分类"), "Chinese label should appear in /zh/ home page");
    assert!(html.contains("简体中文"), "current language display name should appear");
}

#[test]
fn languages_have_display_name_not_title() {
    let site = build_site_from_str(
        r#"
        languages = ["en", "zh"]

        [lang.zh]
        display_name = "简体中文"
        "#,
    );
    let html = mdweb::render::render_home(&site, "en").expect("render");
    // Dropdown should expose the Chinese label configured via [lang.zh].
    assert!(html.contains("简体中文"));
    // Old `l.title` / `languages[].title` shape should not appear.
    assert!(!html.contains("l.title") && !html.contains("lang-title"));
}

#[test]
fn empty_theme_falls_back_to_embedded_default() {
    let site = build_site_from_str("");
    let html = mdweb::render::render_home(&site, "en").expect("render");
    // No template/<name>/ dir exists, but the engine should still produce
    // a complete page from the embedded default theme (base.html shell).
    assert!(html.contains("<main"), "embedded default theme should render main shell");
}

#[test]
fn custom_theme_in_template_dir_is_picked_up() {
    let dir = tempdir("theme");
    write(&dir, "site.toml", r#"theme = "alt""#);
    // Only override `base.html` — keep using embedded defaults for the rest.
    write(
        &dir,
        "template/alt/base.html",
        r##"<!doctype html><html><head><title>{{ title }}</title></head>
<body><main>{{ home.content | safe }}</main><footer>CUSTOM</footer></body></html>"##,
    );
    write(&dir, "_index.md", "---\nlayout: index\n---\nbody\n");

    let site = Site::build(&dir, None).expect("build");
    let html = mdweb::render::render_home(&site, "en").expect("render");
    assert!(
        html.contains("CUSTOM"),
        "footer from template/alt/base.html should override the embedded default"
    );
}

#[test]
fn missing_custom_theme_falls_back_to_embedded() {
    let dir = tempdir("missing");
    write(&dir, "site.toml", r#"theme = "nope""#);
    write(&dir, "_index.md", "---\nlayout: index\n---\nbody\n");

    let site = Site::build(&dir, None).expect("build");
    let html = mdweb::render::render_home(&site, "en").expect("render");
    assert!(html.contains("<main"), "embedded fallback should still render");
}