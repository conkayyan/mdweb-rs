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
        "content/_index.md",
        "---\ntitle: Home\n---\nEnglish body\n",
    );
    write(
        &dir,
        "content/_index.zh.md",
        "---\ntitle: 首页\n---\n中文内容\n",
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

    let en = site.home_value("en", 1);
    let en = en.as_map().expect("en map");
    assert_eq!(en.get("content").and_then(|v| v.as_str()), Some("<p>English body</p>\n"));
    let zh = site.home_value("zh", 1);
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
    let html = mdweb::render::render_home(&site, "zh", 1).expect("render");
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
    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
    // Dropdown should expose the Chinese label configured via [lang.zh].
    assert!(html.contains("简体中文"));
    // Old `l.title` / `languages[].title` shape should not appear.
    assert!(!html.contains("l.title") && !html.contains("lang-title"));
}

#[test]
fn empty_theme_falls_back_to_embedded_default() {
    let site = build_site_from_str("");
    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
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
    write(&dir, "content/_index.md", "---\n---\nbody\n");

    let site = Site::build(&dir, None).expect("build");
    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
    assert!(
        html.contains("CUSTOM"),
        "footer from template/alt/base.html should override the embedded default"
    );
}

#[test]
fn missing_custom_theme_falls_back_to_embedded() {
    let dir = tempdir("missing");
    write(&dir, "site.toml", r#"theme = "nope""#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");

    let site = Site::build(&dir, None).expect("build");
    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
    assert!(html.contains("<main"), "embedded fallback should still render");
}

#[test]
fn sidebar_renders_search_input() {
    let dir = tempdir("search");
    write(&dir, "site.toml", r#"title = "X""#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    write(
        &dir,
        "content/posts/hello.md",
        "---\ntitle: Hello\ndate: 2026-08-01\ntags: [rust]\n---\nbody\n",
    );

    let site = Site::build(&dir, None).expect("build");
    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
    assert!(
        html.contains("side-search-input"),
        "sidebar should expose the search input"
    );
    assert!(
        html.contains("application/rss+xml"),
        "base.html should advertise the RSS feed via <link rel=alternate>"
    );
}

#[test]
fn search_index_lists_articles_in_json() {
    let dir = tempdir("index");
    write(&dir, "site.toml", r#"title = "X""#);
    write(
        &dir,
        "content/posts/hello.md",
        "---\ntitle: Hello\ndate: 2026-08-01\ntags: [rust]\n---\nHello world\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let json = mdweb::feed::search_index_json(&site);
    assert!(json.starts_with('[') && json.ends_with(']'));
    assert!(json.contains("Hello world"), "article body should be indexed");
    assert!(json.contains("\"rust\""), "tags should be present in index");
}

#[test]
fn rss_xml_lists_recent_articles() {
    let dir = tempdir("rss");
    write(
        &dir,
        "site.toml",
        r#"title = "X"
author = "Jane"
base_url = "http://example.com""#,
    );
    write(
        &dir,
        "content/posts/hello.md",
        "---\ntitle: Hello\ndate: 2026-08-01\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let xml = mdweb::feed::rss_xml(&site, "en", 10).expect("rss");
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("<rss version=\"2.0\""));
    assert!(xml.contains("Hello</title>"));
    assert!(xml.contains("http://example.com"));
    assert!(xml.contains("application/rss+xml"));
    assert!(xml.contains("<pubDate>"));
}

#[test]
fn sitemap_lists_home_categories_and_articles() {
    let dir = tempdir("sitemap");
    write(
        &dir,
        "site.toml",
        r#"title = "X"
base_url = "http://example.com"
languages = ["en", "zh"]"#,
    );
    write(
        &dir,
        "content/posts/hello.md",
        "---\ntitle: Hello\ndate: 2026-08-01\n---\nbody\n",
    );
    write(
        &dir,
        "content/posts/hello.zh.md",
        "---\ntitle: 你好\ndate: 2026-08-01\n---\nbody\n",
    );
    write(
        &dir,
        "content/pages/about.md",
        "---\ntitle: About\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let xml = mdweb::feed::sitemap_xml(&site);
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("<urlset"));
    assert!(xml.contains("http://example.com/"), "home URL");
    assert!(
        xml.contains("http://example.com/zh/"),
        "default language prefix should be reachable at /zh/"
    );
    assert!(
        xml.contains("http://example.com/posts/hello/"),
        "english article URL"
    );
    assert!(
        xml.contains("http://example.com/zh/posts/hello/"),
        "chinese article URL"
    );
    assert!(
        xml.contains("http://example.com/pages/about/"),
        "page layout should also be in the sitemap"
    );
    assert!(xml.contains("<lastmod>2026-08-01"), "date becomes lastmod");
}

#[test]
fn search_page_lists_matching_articles() {
    let dir = tempdir("searchpg");
    write(&dir, "site.toml", r#"title = "X""#);
    write(
        &dir,
        "content/posts/rust-intro.md",
        "---\ntitle: Rust intro\ndate: 2026-08-01\ntags: [rust]\n---\nRust is fast.\n",
    );
    write(
        &dir,
        "content/posts/cooking.md",
        "---\ntitle: Cooking bread\ndate: 2026-08-02\ntags: [food]\n---\nSourdough tips.\n",
    );

    let site = Site::build(&dir, None).expect("build");
    let html = mdweb::render::render_search(&site, "en", "sourdough").expect("render");
    assert!(html.contains("Cooking bread"), "matching article should appear");
    // "Rust intro" still shows in the recent-nav sidebar; scope the check to
    // the search-result section so the test reflects actual filtering.
    assert!(
        !html.contains("search-result-title\">Rust intro"),
        "non-matching article should not appear in the result list"
    );
    assert!(html.contains("<form"), "search page should expose a search form");
    assert!(html.contains("name=\"q\""), "form should post q parameter");
    assert!(html.contains(">sourdough<"), "current query should be in summary");

    let empty = mdweb::render::render_search(&site, "en", "no-such-thing").expect("render");
    assert!(empty.contains("No matching posts"));

    let blank = mdweb::render::render_search(&site, "en", "   ").expect("render");
    assert!(!blank.contains("No matching posts"));
}

#[test]
fn search_results_can_be_queried_directly() {
    let dir = tempdir("searchhits");
    write(&dir, "site.toml", r#"title = "X""#);
    write(
        &dir,
        "content/posts/rust-intro.md",
        "---\ntitle: Rust intro\n---\nRust is fast.\n",
    );
    write(
        &dir,
        "content/posts/cooking.md",
        "---\ntitle: Cooking\n---\nBread and rust ovens.\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let rust = mdweb::feed::search_results(&site, "rust", "en");
    assert!(rust.iter().any(|h| h.article.slug == "rust-intro"));
    assert!(rust.iter().any(|h| h.article.slug == "cooking"));
    let none = mdweb::feed::search_results(&site, "   ", "en");
    assert!(none.is_empty());
}

#[test]
fn categories_only_include_posts_subfolders() {
    // Pages and other top-level sections (notes, docs, …) must NOT become
    // categories. Only directories under `posts/` participate.
    let dir = tempdir("catscope");
    write(&dir, "site.toml", r#"title = "X""#);
    write(
        &dir,
        "content/posts/hello.md",
        "---\ntitle: Hello\n---\nbody\n",
    );
    write(
        &dir,
        "content/posts/web/_index.md",
        "---\ntitle: Web\n---\nbody\n",
    );
    write(
        &dir,
        "content/posts/web/frontend/intro.md",
        "---\ntitle: Web intro\n---\nbody\n",
    );
    write(
        &dir,
        "content/pages/about.md",
        "---\ntitle: About\n---\nbody\n",
    );
    write(
        &dir,
        "content/notes/tip.md",
        "---\ntitle: Tip\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    // Top-level category must be `posts`, not `pages` or `notes`.
    let slugs: Vec<&str> = site.tree.iter().map(|c| c.slug.as_str()).collect();
    assert_eq!(slugs, vec!["posts"], "non-posts sections leak into tree");
    // Nested subcategories are preserved (web → frontend).
    let posts = &site.tree[0];
    let web = posts.children.iter().find(|c| c.slug == "web").expect("web");
    let child_slugs: Vec<&str> = web.children.iter().map(|c| c.slug.as_str()).collect();
    assert_eq!(child_slugs, vec!["frontend"], "intro lives two dirs down");
}

#[test]
fn home_feed_excludes_pages_and_notes() {
    let dir = tempdir("homex");
    write(&dir, "site.toml", r#"title = "X""#);
    write(
        &dir,
        "content/posts/hello.md",
        "---\ntitle: Hello\ndate: 2026-08-01\n---\nbody\n",
    );
    write(
        &dir,
        "content/pages/about.md",
        "---\ntitle: About\ndate: 2026-08-02\n---\nbody\n",
    );
    write(
        &dir,
        "content/notes/tip.md",
        "---\ntitle: Tip\ndate: 2026-08-03\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let home = site.home_value("en", 1);
    let home = home.as_map().expect("home map");
    let arts = home.get("articles").and_then(|v| v.as_arr()).expect("arr");
    let titles: Vec<String> = arts
        .iter()
        .filter_map(|v| v.as_map().and_then(|m| m.get("title")).and_then(|t| t.as_str()).map(String::from))
        .collect();
    assert!(titles.contains(&"Hello".to_string()), "post appears on home");
    assert!(!titles.contains(&"About".to_string()), "page leaks into home");
    assert!(!titles.contains(&"Tip".to_string()), "notes leak into home");
}

#[test]
fn pages_tree_is_recursive() {
    // pages/docs/guide/intro.md lives three folders under the page root.
    // The pages tree must surface Docs > Guide > Intro with the correct
    // recursive `children` shape, and intermediate directories must be
    // labelled from their `_index.md` frontmatter. The top-level `pages`
    // section falls back to its directory name when no `_index.md` exists.
    let dir = tempdir("pagestree");
    write(&dir, "site.toml", r#"title = "X""#);
    write(
        &dir,
        "content/pages/docs/_index.md",
        "---\ntitle: My Docs\n---\nbody\n",
    );
    write(
        &dir,
        "content/pages/docs/guide/_index.md",
        "---\ntitle: The Guide\n---\nbody\n",
    );
    write(
        &dir,
        "content/pages/docs/guide/intro.md",
        "---\ntitle: Intro\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let tree = site.pages_tree_value("en", "/");
    let arr = tree.as_arr().expect("array");
    assert_eq!(arr.len(), 1, "only `pages` is a top-level page section");
    let pages = arr[0].as_map().expect("map");
    // No `pages/_index.md` in this fixture, so the top label falls back to
    // the directory name.
    assert_eq!(pages.get("title").and_then(|v| v.as_str()), Some("pages"));
    let pages_children = pages.get("children").and_then(|v| v.as_arr()).expect("ch");
    assert_eq!(pages_children.len(), 1, "one child: docs");
    let docs = pages_children[0].as_map().expect("map");
    assert_eq!(docs.get("title").and_then(|v| v.as_str()), Some("My Docs"));
    let docs_children = docs.get("children").and_then(|v| v.as_arr()).expect("ch");
    assert_eq!(docs_children.len(), 1, "one grandchild: guide");
    let guide = docs_children[0].as_map().expect("map");
    assert_eq!(guide.get("title").and_then(|v| v.as_str()), Some("The Guide"));
    let guide_children = guide.get("children").and_then(|v| v.as_arr()).expect("ch");
    assert_eq!(guide_children.len(), 1, "one great-grandchild: intro");
    let intro = guide_children[0].as_map().expect("map");
    assert_eq!(intro.get("title").and_then(|v| v.as_str()), Some("Intro"));
    assert_eq!(intro.get("has_children").and_then(|v| v.as_bool()), Some(false));
}

#[test]
fn page_section_renders_with_children_list() {
    // /pages/docs/ should render the section landing template, listing its
    // direct subdirectories and pages.
    let dir = tempdir("section");
    write(&dir, "site.toml", r#"title = "X""#);
    write(
        &dir,
        "content/pages/_index.md",
        "---\ntitle: Pages\n---\nbody\n",
    );
    write(
        &dir,
        "content/pages/docs/_index.md",
        "---\ntitle: Docs\nsummary: All docs live here.\n---\nintro text\n",
    );
    write(
        &dir,
        "content/pages/docs/intro.md",
        "---\ntitle: Intro\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let html = mdweb::render::render_home(&site, "en", 1).expect("render home");
    // The home page renders the header which calls into the layout — verify
    // the section listing path through page_section_value.
    let payload = site
        .page_section_value(&["pages".to_string(), "docs".to_string()], "en", 1)
        .expect("section");
    let m = payload.as_map().expect("map");
    assert_eq!(m.get("title").and_then(|v| v.as_str()), Some("Docs"));
    assert_eq!(
        m.get("summary").and_then(|v| v.as_str()),
        Some("All docs live here.")
    );
    let kids = m.get("children").and_then(|v| v.as_arr()).expect("kids");
    let titles: Vec<&str> = kids
        .iter()
        .filter_map(|c| c.as_map().and_then(|m| m.get("title")).and_then(|t| t.as_str()))
        .collect();
    assert!(titles.contains(&"Intro"), "leaf page lists itself");
    // And the rendering call succeeds.
    let section_html =
        mdweb::render::render_section(&site, "en", &["pages".to_string(), "docs".to_string()], 1)
            .expect("render section");
    assert!(section_html.contains("Docs"));
    assert!(section_html.contains("Intro"));
    let _ = html; // keep the home render call exercised
}

#[test]
fn article_to_value_exposes_lang_field() {
    // Article::to_value must surface `lang` so templates can label
    // search results and language badges.
    let dir = tempdir("langfield");
    write(&dir, "site.toml", r#"languages = ["en", "zh"]"#);
    write(
        &dir,
        "content/posts/hello.md",
        "---\ntitle: Hello\n---\nbody\n",
    );
    write(
        &dir,
        "content/posts/hello.zh.md",
        "---\ntitle: 你好\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let en = site.articles.iter().find(|a| a.slug == "hello" && a.lang == "en").expect("en");
    let v = en.to_value();
    assert_eq!(v.as_map().and_then(|m| m.get("lang")).and_then(|l| l.as_str()), Some("en"));
}

#[test]
fn breadcrumbs_render_above_posts_and_pages() {
    // `posts/web/frontend/react.md` → posts > web > frontend > title (no home)
    // `pages/docs/guide/intro.md` → home > pages > docs > guide > title
    let dir = tempdir("crumbs");
    write(&dir, "site.toml", r#"title = "My Blog"
languages = ["en", "zh"]

[i18n.zh]
breadcrumb_home = "首页""#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    write(&dir, "content/posts/_index.md", "---\ntitle: Posts\n---\nbody\n");
    write(&dir, "content/posts/web/_index.md", "---\ntitle: Web\n---\nbody\n");
    write(
        &dir,
        "content/posts/web/frontend/_index.md",
        "---\ntitle: Frontend\n---\nbody\n",
    );
    write(
        &dir,
        "content/posts/web/frontend/react.md",
        "---\ntitle: A React Note\n---\nbody\n",
    );
    write(
        &dir,
        "content/pages/docs/_index.md",
        "---\ntitle: Docs\n---\nbody\n",
    );
    write(
        &dir,
        "content/pages/docs/guide/_index.md",
        "---\ntitle: Guide\n---\nbody\n",
    );
    write(
        &dir,
        "content/pages/docs/guide/intro.md",
        "---\ntitle: Intro\n---\nbody\n",
    );
    write(
        &dir,
        "content/pages/_index.md",
        "---\ntitle: Pages\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let post = site.articles.iter().find(|a| a.slug == "react").expect("post");
    let html = mdweb::render::render_article(&site, "en", post).expect("render post");
    assert!(html.contains("class=\"breadcrumb\""));
    // Articles drop the leading home crumb; the trail starts at "Posts".
    assert!(!html.contains("href=\"/\">Index"));
    assert!(html.contains("href=\"/posts/\">Posts"));
    assert!(html.contains("href=\"/posts/web/\">Web"));
    assert!(html.contains("href=\"/posts/web/frontend/\">Frontend"));
    assert!(html.contains(">A React Note<"), "current item is the title");
    // The current item is rendered as a <span> not an <a>, so no link.
    let last_span_start = html.rfind("class=\"breadcrumb\"").unwrap_or(0);
    let last_block = &html[last_span_start..];
    assert!(last_block.contains("<span>A React Note</span>"), "current item is a non-link span");

    let page = site.articles.iter().find(|a| a.slug == "intro").expect("page");
    let page_html = mdweb::render::render_article(&site, "en", page).expect("render page");
    // The `pages/` container is a transparent sibling of `posts/`: its
    // segment is dropped from the trail, so users see Index › docs › guide
    // instead of Index › pages › docs › guide.
    assert!(!page_html.contains("href=\"/pages/\">Pages"), "pages/ container hidden");
    assert!(page_html.contains("href=\"/pages/docs/\">Docs"));
    assert!(page_html.contains("href=\"/pages/docs/guide/\">Guide"));
    assert!(page_html.contains("<span>Intro</span>"));

    // ZH variant: home title is per-language, ancestor titles come from
    // the `_index.<lang>.md` frontmatter.
    write(&dir, "content/pages/docs/_index.zh.md", "---\ntitle: 文档\n---\nbody\n");
    write(&dir, "content/pages/docs/guide/_index.zh.md", "---\ntitle: 指南\n---\nbody\n");
    write(&dir, "content/pages/docs/guide/intro.zh.md", "---\ntitle: 简介\n---\nbody\n");
    write(&dir, "content/pages/_index.zh.md", "---\ntitle: 页面\n---\nbody\n");
    let site_zh = Site::build(&dir, None).expect("build zh");
    let intro_zh = site_zh.articles.iter().find(|a| a.slug == "intro" && a.lang == "zh").expect("intro zh");
    let html_zh = mdweb::render::render_article(&site_zh, "zh", intro_zh).expect("render zh");
    assert!(html_zh.contains("href=\"/zh/\">首页"), "home is /zh/首页");
    assert!(!html_zh.contains("href=\"/zh/pages/\">页面"), "pages/ container hidden in zh too");
    assert!(html_zh.contains("href=\"/zh/pages/docs/\">文档"));
    assert!(html_zh.contains("href=\"/zh/pages/docs/guide/\">指南"));
    assert!(html_zh.contains("<span>简介</span>"));
}

#[test]
fn listing_pagination_slices_and_pagers_appear() {
    // 7 posts under `posts/`, default `home_limit` = 5 → 2 pages. Page 1
    // holds the newest 5; page 2 holds the 2 oldest.
    let dir = tempdir("paging");
    write(&dir, "site.toml", r#"title = "X"
home_limit = 5"#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    let titles_p1 = ["P01", "P02", "P03", "P04", "P05"];
    let dates_p1 = ["2026-08-01", "2026-08-02", "2026-08-03", "2026-08-04", "2026-08-05"];
    for (i, t) in titles_p1.iter().enumerate() {
        write(
            &dir,
            &format!("content/posts/p{:02}.md", i + 1),
            &format!("---\ntitle: \"{t}\"\ndate: \"{}\"\n---\nbody\n", dates_p1[i]),
        );
    }
    let titles_p2 = ["P06", "P07"];
    let dates_p2 = ["2026-07-01", "2026-06-01"];
    for (i, t) in titles_p2.iter().enumerate() {
        write(
            &dir,
            &format!("content/posts/p{:02}.md", i + 6),
            &format!("---\ntitle: \"{t}\"\ndate: \"{}\"\n---\nbody\n", dates_p2[i]),
        );
    }
    let site = Site::build(&dir, None).expect("build");
    assert_eq!(site.config.home_limit, 5, "configured home_limit wins");
    assert_eq!(site.config.category_limit, 20, "default category_limit");
    assert_eq!(site.config.pages_limit, 50, "default pages_limit");

    // page=1 → newest 5
    let html1 = mdweb::render::render_home(&site, "en", 1).expect("p1");
    assert!(html1.contains("class=\"pagination\""));
    assert!(html1.contains(">1 / 2<"));
    assert!(html1.contains("pagination-next"), "page 1 shows next link");
    for t in titles_p1 {
        assert!(html1.contains(t), "page 1 contains {t}");
    }
    for t in titles_p2 {
        assert!(!html1.contains(&format!(">{t}<")), "page 1 hides {t}");
    }

    // page=2 → oldest 2
    let html2 = mdweb::render::render_home(&site, "en", 2).expect("p2");
    assert!(html2.contains(">2 / 2<"));
    assert!(html2.contains("pagination-prev"), "page 2 shows prev link");
    for t in titles_p2 {
        assert!(html2.contains(&format!(">{t}<")), "page 2 contains {t}");
    }

    // page > total_pages clamps to last page; no next link on the clamp.
    let html_last = mdweb::render::render_home(&site, "en", 99).expect("clamp");
    assert!(html_last.contains(">2 / 2<"));
    assert!(!html_last.contains("pagination-next"));

    // Single-page result: pagination block is hidden entirely.
    let dir_small = tempdir("paging-small");
    write(&dir_small, "site.toml", r#"title = "X""#);
    write(&dir_small, "content/_index.md", "---\n---\nbody\n");
    write(
        &dir_small,
        "content/posts/only.md",
        "---\ntitle: Only\ndate: 2026-08-01\n---\nbody\n",
    );
    let site_small = Site::build(&dir_small, None).expect("build small");
    let html_small = mdweb::render::render_home(&site_small, "en", 1).expect("small");
    assert!(!html_small.contains("class=\"pagination\""));
}

#[test]
fn directory_drives_template_selection() {
    // The `layout:` frontmatter field is gone. Template selection now lives
    // entirely in directory: posts/* uses article.html, everything else uses
    // page.html. Verify by inspecting the rendered output.
    let dir = tempdir("layoutdir");
    write(&dir, "site.toml", r#"title = "X""#);
    write(
        &dir,
        "content/posts/post.md",
        "---\ntitle: P\ndate: 2026-08-01\n---\nbody\n",
    );
    write(
        &dir,
        "content/pages/leaf.md",
        "---\ntitle: L\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let post_html = mdweb::render::render_article(
        &site,
        "en",
        site.articles.iter().find(|a| a.slug == "post").expect("post"),
    )
    .expect("post render");
    let page_html = mdweb::render::render_article(
        &site,
        "en",
        site.articles.iter().find(|a| a.slug == "leaf").expect("leaf"),
    )
    .expect("page render");
    // Different templates → different DOM. The article template emits the
    // post meta header (date, prev/next nav); the page template does not.
    assert!(
        post_html.contains("post-meta") || post_html.contains("article-meta"),
        "posts render with article.html (meta header)"
    );
    assert!(
        !page_html.contains("post-meta") && !page_html.contains("article-meta"),
        "pages render with page.html (no meta header)"
    );
}

#[test]
fn reading_minutes_estimates_cjk_and_words() {
    // 600 CJK chars at 300 cpm → 2 min; 600 English words at 200 wpm → 3 min.
    let dir = tempdir("reading");
    write(&dir, "site.toml", r#"title = "X""#);
    let cjk = "中".repeat(600);
    let words = "lorem ".repeat(600);
    write(
        &dir,
        "content/posts/long.md",
        &format!("---\ntitle: L\n---\n\n{cjk}\n\n{words}\n"),
    );
    write(
        &dir,
        "content/posts/short.md",
        "---\ntitle: S\n---\n\nhi\n",
    );
    write(
        &dir,
        "content/posts/empty.md",
        "---\ntitle: E\n---\n\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let long = site.articles.iter().find(|a| a.slug == "long").expect("long");
    let short = site.articles.iter().find(|a| a.slug == "short").expect("short");
    let empty = site.articles.iter().find(|a| a.slug == "empty").expect("empty");
    assert_eq!(long.reading_minutes(), 5, "600 CJK + 600 words → 5 min");
    assert_eq!(long.reading_seconds(), 300, "5 min in seconds");
    assert_eq!(short.reading_minutes(), 0, "sub-minute content has 0 minutes");
    assert_eq!(short.reading_seconds(), 1, "sub-minute content is finer-grained…");
    assert_eq!(empty.reading_minutes(), 0, "empty content reports 0");
    // to_value surfaces both counts.
    let v = short.to_value();
    assert!(v
        .as_map()
        .and_then(|m| m.get("reading_minutes"))
        .and_then(|x| x.as_int())
        .unwrap_or(-1)
        == 0);
    assert!(v
        .as_map()
        .and_then(|m| m.get("reading_seconds"))
        .and_then(|x| x.as_int())
        .unwrap_or(-1)
        == 1);
    // Long posts render "N min read"; short ones fall back to "N sec read".
    let long_html = mdweb::render::render_article(&site, "en", long).expect("render long");
    assert!(
        long_html.contains("reading-time\">5 min read"),
        "long content renders minute label"
    );
    let short_html = mdweb::render::render_article(&site, "en", short).expect("render short");
    assert!(
        short_html.contains("reading-time\">1 sec read"),
        "short content refines the estimate to seconds"
    );
    // Rendered output exposes the label for both post and page paths.
    let page_dir = tempdir("readingpg");
    write(&page_dir, "site.toml", r#"title = "X""#);
    write(
        &page_dir,
        "content/about.md",
        "---\ntitle: About\n---\n\nSome content here for the page.\n",
    );
    let site2 = Site::build(&page_dir, None).expect("build page");
    let about = site2.articles.iter().find(|a| a.slug == "about").expect("about");
    let html = mdweb::render::render_article(&site2, "en", about).expect("render page");
    assert!(html.contains("reading-time"), "pages surface reading-time block");
    assert!(
        html.contains("sec read"),
        "rendered output uses the i18n reading_time_seconds label"
    );
}

#[test]
fn show_rss_and_sitemap_toggle_footer_links() {
    use mdweb::config::Config;
    let dir_on = tempdir("toggleon");
    write(
        &dir_on,
        "site.toml",
        r#"title = "X"
show_rss = true
show_sitemap = true"#,
    );
    write(&dir_on, "content/_index.md", "---\n---\nbody\n");
    let site_on = Site::build(&dir_on, None).expect("build");
    assert!(site_on.config.show_rss);
    assert!(site_on.config.show_sitemap);
    let html_on = mdweb::render::render_home(&site_on, "en", 1).expect("render on");
    assert!(html_on.contains("footer-links"), "footer-links block visible when on");
    assert!(html_on.contains("<a href=\"/rss.xml\""), "RSS footer link visible when on");
    assert!(html_on.contains("<a href=\"/sitemap.xml\""), "sitemap footer link visible when on");

    let dir_off = tempdir("toggleoff");
    write(
        &dir_off,
        "site.toml",
        r#"title = "X"
show_rss = false
show_sitemap = false"#,
    );
    write(&dir_off, "content/_index.md", "---\n---\nbody\n");
    let site_off = Site::build(&dir_off, None).expect("build");
    assert!(!site_off.config.show_rss);
    assert!(!site_off.config.show_sitemap);
    let html_off = mdweb::render::render_home(&site_off, "en", 1).expect("render off");
    assert!(!html_off.contains("footer-links"), "footer-links block hidden when both off");
    assert!(!html_off.contains("<a href=\"/rss.xml\""), "RSS footer link hidden when off");
    assert!(!html_off.contains("<a href=\"/sitemap.xml\""), "sitemap footer link hidden when off");

    // Default = both on when unset.
    let cfg: Config = Default::default();
    assert!(cfg.show_rss);
    assert!(cfg.show_sitemap);
}

#[test]
fn sidebar_drops_rss_link() {
    // RSS was previously surfaced as a small link in the sidebar search
    // nav; it now lives only in the footer (and only when show_rss = true).
    let dir = tempdir("nosidebars");
    write(&dir, "site.toml", r#"title = "X""#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    write(
        &dir,
        "content/posts/hello.md",
        "---\ntitle: Hello\ndate: 2026-08-01\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
    assert!(
        !html.contains("rss-link"),
        "sidebar no longer exposes the rss-link class"
    );
    // The sidebar used to include a sidebar RSS link button; verify that
    // nothing in the sidebar still points at /rss.xml as a sidebar link
    // (vs. the <link rel=alternate> in <head>, which we keep).
    let sidebar_start = html.find("<aside class=\"sidebar\"").unwrap_or(0);
    let sidebar_end = html.find("</aside>").unwrap_or(html.len());
    let sidebar = &html[sidebar_start..sidebar_end];
    assert!(
        !sidebar.contains("/rss.xml"),
        "RSS must not appear as a sidebar link (footer-only now)"
    );
}
    #[test]
fn tag_cloud_toggle_and_clickable_article_tags() {
    let dir = tempdir("tags");
    write(&dir, "site.toml", r#"title = "X"
show_tag_cloud = true"#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    write(
        &dir,
        "content/posts/hello.md",
        "---\ntitle: Hello\ndate: 2026-08-01\ntags: [\"rust\", \"my tag\"]\n---\nbody\n",
    );

    let site = Site::build(&dir, None).expect("build");

    // Sidebar cloud renders tags as links (enabled by default / explicitly).
    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
    assert!(html.contains("tag-cloud-nav"), "tag cloud widget present when enabled");
    assert!(html.contains("href=\"/tags/rust/\""), "cloud tag links to its page");
    assert!(
        html.contains("href=\"/tags/my%20tag/\""),
        "tag names with spaces are percent-encoded"
    );
    assert!(
        html.contains("rust<span class=\"tag-count\">(1)</span>"),
        "cloud tags render as name(count)"
    );

    // Article tags are clickable links to their tag pages.
    let post = site.articles.iter().find(|a| a.slug == "hello").expect("post");
    let article = mdweb::render::render_article(&site, "en", post).expect("article");
    assert!(article.contains("href=\"/tags/rust/\""), "article tag links to its page");
    assert!(
        article.contains("href=\"/tags/my%20tag/\""),
        "spaced tag in an article is encoded too"
    );

    // show_tag_cloud = false hides the sidebar widget but keeps page tags.
    let dir_off = tempdir("tags-off");
    write(&dir_off, "site.toml", r#"title = "X"
show_tag_cloud = false"#);
    write(&dir_off, "content/_index.md", "---\n---\nbody\n");
    write(
        &dir_off,
        "content/posts/hello.md",
        "---\ntitle: Hello\ntags: [\"rust\"]\n---\nbody\n",
    );
    let site_off = Site::build(&dir_off, None).expect("build off");
    assert!(!site_off.config.show_tag_cloud);
    let off = mdweb::render::render_home(&site_off, "en", 1).expect("render off");
    assert!(!off.contains("tag-cloud-nav"), "cloud hidden when show_tag_cloud=false");
}

#[test]
fn tag_cloud_limit_truncates_sorted_by_count() {
    let dir = tempdir("cloudlimit");
    write(&dir, "site.toml", r#"title = "X"
tag_cloud_limit = 2"#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    write(
        &dir,
        "content/posts/a.md",
        "---\ntitle: A\ntags: [\"a\"]\n---\nbody\n",
    );
    write(
        &dir,
        "content/posts/b.md",
        "---\ntitle: B\ntags: [\"b\"]\n---\nbody\n",
    );
    // b now appears twice → count 2; a and c count 1 each.
    write(
        &dir,
        "content/posts/c.md",
        "---\ntitle: C\ntags: [\"b\", \"c\"]\n---\nbody\n",
    );

    let site = Site::build(&dir, None).expect("build");
    assert_eq!(site.config.tag_cloud_limit, 2);

    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
    assert!(html.contains(">(2)</span>"), "only tags from the limited cloud shown");

    // Sorted by count desc (b first), a/c tie-break by name, c cut off.
    let cloud = html
        .split("tag-cloud-nav")
        .nth(1)
        .expect("cloud section")
        .split("</ul>")
        .next()
        .expect("cloud list");
    let pos_b = cloud.find("tags/b/\"").expect("b in cloud");
    let pos_a = cloud.find("tags/a/\"").expect("a in cloud");
    assert!(pos_b < pos_a, "b (count 2) sorts before a (count 1)");
    assert!(!cloud.contains("tags/c/\""), "c truncated by tag_cloud_limit");
    assert!(
        cloud.contains(">b<span class=\"tag-count\">(2)</span>"),
        "top tag shows its count"
    );

    // The /tags/ index is *not* limited and shows every tag with a count.
    let idx = mdweb::render::render_tags_index(&site, "en").expect("tags index");
    assert!(idx.contains("href=\"/tags/c/\""), "index shows all tags");
    assert!(idx.contains("b<span class=\"tag-count\">(2)</span>"), "index counts per tag");
}

#[test]
fn tags_index_page_lists_all_tags() {
    let dir = tempdir("tagsindex");
    write(&dir, "site.toml", r#"title = "X"
languages = ["en", "zh"]"#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    write(
        &dir,
        "content/posts/rust.md",
        "---\ntitle: Rust\ndate: 2026-08-02\ntags: [\"rust\"]\n---\nbody\n",
    );
    write(
        &dir,
        "content/posts/rust.zh.md",
        "---\ntitle: Rust ZH\ndate: 2026-08-02\ntags: [\"rust\"]\n---\nbody\n",
    );
    write(
        &dir,
        "content/posts/rust-b.md",
        "---\ntitle: Rust B\ndate: 2026-08-01\ntags: [\"rust\", \"my tag\"]\n---\nbody\n",
    );
    write(
        &dir,
        "content/posts/rust-b.zh.md",
        "---\ntitle: Rust B ZH\ndate: 2026-08-01\ntags: [\"rust\", \"my tag\"]\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");

    // /tags/ lists every tag in the current language, linked + counted.
    let html = mdweb::render::render_tags_index(&site, "en").expect("tags index");
    assert!(html.contains("class=\"breadcrumb\""), "index has breadcrumbs");
    assert!(html.contains("href=\"/\">Index"), "breadcrumb starts at home");
    assert!(html.contains("<span>Tags</span>"), "current crumb is a span");
    assert!(html.contains("href=\"/tags/rust/\""), "rust tag link");
    assert!(html.contains("href=\"/tags/my%20tag/\""), "encoded spaced tag");
    assert!(html.contains(">rust<span"), "rust tag shows its count");

    // /zh/tags/ is language-scoped: only tags used by zh articles appear.
    let zh_html = mdweb::render::render_tags_index(&site, "zh").expect("zh tags index");
    assert!(zh_html.contains("href=\"/zh/tags/rust/\""), "zh tags link into /zh/tags/");
    assert!(!zh_html.contains("href=\"/tags/"), "unprefixed en tag URLs must not leak into zh");
}

#[test]
fn tag_page_lists_matching_articles_with_pagination() {
    let dir = tempdir("tagpage");
    write(&dir, "site.toml", r#"title = "X"
tags_limit = 2"#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    for i in 1..=5 {
        write(
            &dir,
            &format!("content/posts/p{i}.md"),
            &format!("---\ntitle: P{i}\ndate: 2026-08-0{i}\ntags: [\"rust\"]\n---\nbody\n"),
        );
    }
    write(
        &dir,
        "content/posts/food.md",
        "---\ntitle: Food\ndate: 2026-08-01\ntags: [\"food\"]\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    assert_eq!(site.config.tags_limit, 2, "configured tags_limit wins");

    let p1 = mdweb::render::render_tag(&site, "en", "rust", 1).expect("tag p1");
    assert!(p1.contains("class=\"breadcrumb\""), "tag page has breadcrumbs");
    assert!(p1.contains("href=\"/tags/\""), "Tags crumb points at the index");
    assert!(p1.contains(">P5<"), "newest article on page 1");
    assert!(!p1.contains("card-title\"><a href=\"/posts/p3/\">P3</a>"), "page 1 holds 2 of 5");
    assert!(p1.contains("pagination-next"), "page 1 shows next link");

    let p3 = mdweb::render::render_tag(&site, "en", "rust", 3).expect("tag p3");
    assert!(p3.contains("card-title\"><a href=\"/posts/p1/\">P1</a>"), "page 3 shows the oldest");
    assert!(p3.contains("pagination-prev"), "page 3 shows prev link");

    // Unknown tag renders as a 404 (Err) rather than an empty page.
    assert!(mdweb::render::render_tag(&site, "en", "nope", 1).is_err());

    // The "food" tag lists only its own article, not the rust ones.
    let food = mdweb::render::render_tag(&site, "en", "food", 1).expect("food");
    assert!(food.contains("card-title\"><a href=\"/posts/food/\">Food</a>"));
    assert!(!food.contains("card-title\"><a href=\"/posts/p1/\">P1</a>"));
    assert!(!food.contains("pagination-next"), "single match has no pager");
}

#[test]
fn category_page_shows_breadcrumbs() {
    let dir = tempdir("catcrumbs");
    write(&dir, "site.toml", r#"title = "X""#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    write(&dir, "content/posts/_index.md", "---\ntitle: Posts\n---\nbody\n");
    write(&dir, "content/posts/web/_index.md", "---\ntitle: Web\n---\nbody\n");
    write(
        &dir,
        "content/posts/web/intro.md",
        "---\ntitle: Intro\ntags: [\"web\"]\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");

    let posts = site.tree.iter().find(|c| c.slug == "posts").expect("posts cat");
    let html = mdweb::render::render_category(&site, "en", posts, 1).expect("render posts");
    assert!(html.contains("class=\"breadcrumb\""), "category list has breadcrumbs");
    assert!(!html.contains("href=\"/\">Index"), "posts crumbs drop the home node");
    assert!(!html.contains("href=\"/posts/\">Posts"), "terminal crumb is not a self-link");
    assert!(html.contains("<span>Posts</span>"), "current item is a non-link span");

    let web = posts.children.iter().find(|c| c.slug == "web").expect("web cat");
    let html2 = mdweb::render::render_category(&site, "en", web, 1).expect("render web");
    assert!(html2.contains("href=\"/posts/\">Posts"), "ancestor crumb");
    assert!(!html2.contains("href=\"/posts/web/\">Web"), "terminal crumb is not a self-link");
    assert!(html2.contains("<span>Web</span>"), "current category is a span");
}

#[test]
fn root_level_md_files_become_root_pages() {
    // `content/about.md` (directly under content/) is a top-level page with
    // an empty path and a flat URL `/about/`. It must NOT appear in the
    // recent posts list nor in any category — it's a page, not a post.
    let dir = tempdir("rootpg");
    write(&dir, "site.toml", r#"title = "X""#);
    write(
        &dir,
        "content/about.md",
        "---\ntitle: About\n---\nbody\n",
    );
    write(
        &dir,
        "content/about.zh.md",
        "---\ntitle: 关于\n---\nbody\n",
    );
    write(
        &dir,
        "content/posts/hello.md",
        "---\ntitle: Hello\ndate: 2026-08-01\n---\nbody\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let about_en = site
        .articles
        .iter()
        .find(|a| a.slug == "about" && a.lang == "en")
        .expect("en about");
    assert_eq!(about_en.url, "/about/");
    assert!(about_en.path.is_empty(), "top-level page has empty path");
    // Categories only contain `posts`.
    let slugs: Vec<&str> = site.tree.iter().map(|c| c.slug.as_str()).collect();
    assert_eq!(slugs, vec!["posts"], "about must not leak into categories");
    // Home feed excludes the page.
    let home = site.home_value("en", 1);
    let arts = home
        .as_map()
        .and_then(|m| m.get("articles"))
        .and_then(|v| v.as_arr())
        .expect("arr");
    let titles: Vec<String> = arts
        .iter()
        .filter_map(|v| {
            v.as_map()
                .and_then(|m| m.get("title"))
                .and_then(|t| t.as_str())
                .map(String::from)
        })
        .collect();
    assert!(titles.contains(&"Hello".to_string()));
    assert!(!titles.contains(&"About".to_string()));
    // Pages tree does NOT include root pages (they're flat nav links, not
    // hierarchical). Confirms there is no spurious `about/` entry nested
    // under an empty-path section.
    let tree = site.pages_tree_value("en", "/about/");
    let arr = tree.as_arr().expect("arr");
    assert!(arr.is_empty(), "no page section exists for an empty path");
}
#[test]
fn image_paths_are_rewritten_and_resolvable() {
    // Images live in an `_image/` dir beside the document. Markdown keeps a
    // relative filesystem path (so local preview works) and the rendered HTML
    // gets a site URL with the `_image` segment dropped.
    let dir = tempdir("img");
    write(&dir, "site.toml", r#"title = "X""#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    write(&dir, "content/pages/_image/logo.svg", "<svg/>");
    write(&dir, "content/posts/guide/_image/hero.svg", "<svg/>");
    write(
        &dir,
        "content/posts/guide/post.md",
        concat!(
            "---\ntitle: P\ndate: 2026-08-06\n---\n",
            "![H](_image/hero.svg)\n\n",
            "![L](../../pages/_image/logo.svg)\n\n",
            "<img src=\"_image/hero.svg\" width=\"160\">\n\n",
            "![S](/static/x.png)\n\n",
            "![E](https://example.com/_image/x.png)\n\n",
            "![N](../other/x.png)\n\n",
            "```html\n<img src=\"_image/hero.svg\">\n```\n",
        ),
    );
    let site = Site::build(&dir, None).expect("build");
    let post = site.articles.iter().find(|a| a.slug == "post").expect("post");
    let html = &post.content;

    assert!(html.contains("src=\"/posts/guide/hero.svg\""), "same-dir image");
    assert!(html.contains("src=\"/pages/logo.svg\""), "cross-dir via ../");
    assert!(
        html.contains("<img src=\"/posts/guide/hero.svg\" width=\"160\">"),
        "raw HTML img is rewritten and keeps its other attributes"
    );
    assert!(html.contains("src=\"/static/x.png\""), "absolute path untouched");
    assert!(
        html.contains("src=\"https://example.com/_image/x.png\""),
        "external URL untouched"
    );
    assert!(html.contains("src=\"../other/x.png\""), "no _image segment: untouched");
    assert!(
        html.contains("&lt;img src=\"_image/hero.svg\"&gt;"),
        "an <img> shown as a code example stays verbatim"
    );

    // The server can invert the mapping back to disk.
    assert!(site.resolve_image("posts/guide/hero.svg").is_some());
    assert!(site.resolve_image("pages/logo.svg").is_some());
    assert!(site.resolve_image("pages/missing.svg").is_none());
}

#[test]
fn file_routes_reject_traversal_outside_the_doc_root() {
    let dir = tempdir("traversal");
    write(&dir, "site.toml", r#"title = "X""#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    write(&dir, "content/pages/_image/a.svg", "<svg/>");
    let site = Site::build(&dir, None).expect("build");

    assert!(site.resolve_file("site.toml").is_some(), "in-tree file still served");
    assert!(site.resolve_file("../../../etc/passwd").is_none());
    assert!(site.resolve_file("content/../../etc/passwd").is_none());
    assert!(site.resolve_file("/etc/passwd").is_none());
    assert!(site.resolve_image("../../../etc/passwd").is_none());
}

#[test]
fn analytics_off_by_default() {
    let dir = tempdir("anoff");
    write(&dir, "site.toml", r#"title = "X""#);
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    let site = Site::build(&dir, None).expect("build");
    assert!(!site.config.analytics.is_enabled());
    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
    assert!(
        !html.contains("googletagmanager"),
        "no Google snippet without config: {html}"
    );
    assert!(
        !html.contains("hm.baidu.com"),
        "no Baidu snippet without config: {html}"
    );
}

#[test]
fn analytics_google_injects_snippet() {
    let dir = tempdir("an-google");
    write(
        &dir,
        "site.toml",
        r#"title = "X"
[analytics.google]
id = "G-TESTID42"
"#,
    );
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    let site = Site::build(&dir, None).expect("build");
    assert!(site.config.analytics.is_enabled());
    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
    assert!(html.contains("googletagmanager.com/gtag/js?id=G-TESTID42"));
    assert!(html.contains("gtag('config', 'G-TESTID42')"));
    // The snippet must live inside <head> alongside the user-editable inject slot.
    let head_end = html.find("</head>").expect("has </head>");
    let snippet_at = html.find("googletagmanager.com").expect("snippet present");
    assert!(snippet_at < head_end, "snippet lives inside <head>");
}

#[test]
fn analytics_baidu_injects_snippet() {
    let dir = tempdir("an-baidu");
    write(
        &dir,
        "site.toml",
        r#"title = "X"
[analytics.baidu]
id = "abcdef0123456789abcdef0123456789"
"#,
    );
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    let site = Site::build(&dir, None).expect("build");
    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
    assert!(html.contains("hm.baidu.com/hm.js?abcdef0123456789abcdef0123456789"));
    assert!(html.contains("_hmt = _hmt || []"));
}

#[test]
fn analytics_precedes_user_inject_slot() {
    // The analytics script must be inserted before any HTML the author
    // hand-wrote in `layout/inject.html` so both blocks coexist.
    let dir = tempdir("an-inject");
    write(
        &dir,
        "site.toml",
        r#"title = "X"
[analytics.google]
id = "G-FIRST"
"#,
    );
    write(&dir, "content/_index.md", "---\n---\nbody\n");
    write(
        &dir,
        "template/default/layout/inject.html",
        "<!-- user-inject-marker -->\n",
    );
    let site = Site::build(&dir, None).expect("build");
    let html = mdweb::render::render_home(&site, "en", 1).expect("render");
    let analytics_at = html
        .find("googletagmanager")
        .expect("analytics snippet present");
    let user_at = html
        .find("user-inject-marker")
        .expect("user inject slot rendered");
    assert!(
        analytics_at < user_at,
        "analytics snippet should precede user inject content"
    );
}
