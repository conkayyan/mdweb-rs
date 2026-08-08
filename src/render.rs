use std::collections::BTreeMap;

use crate::config::I18N_DEFAULTS;
use crate::content::{Article, Category, Site};
use crate::feed;
use crate::value::Value;

/// Build the shared context for a language.
fn base_ctx(site: &Site, lang: &str, current_url: &str, current_query: &str) -> Value {
    let config = &site.config;
    let home_url = config.lang_prefix(lang);
    let home_active = current_url == home_url;
    let mut languages: Vec<Value> = Vec::new();
    for l in &site.languages {
        let url = config.lang_prefix(l);
        languages.push(Value::Map(BTreeMap::from([
            ("code".to_string(), Value::str(l)),
            ("display_name".to_string(), Value::str(&config.display_name_for(l))),
            ("url".to_string(), Value::str(&url)),
            ("active".to_string(), Value::Bool(l == lang)),
        ])));
    }
    let mut config_langs: Vec<Value> = Vec::new();
    for l in &config.languages {
        config_langs.push(Value::str(l));
    }
    // `posts/` is a special directory, not a category: its sub-directories
    // are the first-level categories. Flatten the container node away so the
    // sidebar tree starts at the real categories.
    let cats_root = match site.tree.first() {
        Some(root) if root.slug == "posts" => &root.children,
        _ => &site.tree,
    };
    let categories = site.category_tree_value(cats_root, lang, current_url);
    let categories_active = tree_has_active(&categories);
    let pages = pages_value(&site.articles, lang, current_url);
    let pages_tree = site.pages_tree_value(lang, current_url);
    // Direct entries for the header nav. Each first-level child of
    // `content/pages/` is rendered as its own button — see `header.html`.
    // The unwrap helper strips the redundant `pages/` wrapper when its
    // only job is to wrap everything else.
    let page_sections = unwrapped_page_sections_value(&pages_tree);
    // Top-level pages: articles whose path is empty (e.g. `content/about.md`
    // → `/about/`). These show up as flat nav links alongside the page
    // section buttons, since they live at the site root.
    let root_pages = root_pages_value(&site.articles, lang, current_url);
    let recent = recent_value(&site.articles, lang, 5);
    let mut tag_cloud = site.tag_cloud_value(lang);
    if let Value::Arr(list) = &mut tag_cloud {
        if site.config.tag_cloud_limit > 0 {
            list.truncate(site.config.tag_cloud_limit);
        }
    }
    let friend_links: Vec<Value> = site
        .config
        .friend_links
        .iter()
        .map(|l| {
            Value::Map(BTreeMap::from([
                ("name".to_string(), Value::str(&l.name)),
                ("url".to_string(), Value::str(&l.url)),
            ]))
        })
        .collect();
    let rss_url = format!("{}rss.xml", config.lang_prefix(lang));
    let search_action = format!("{}search", config.lang_prefix(lang));
    let sitemap_url = "/sitemap.xml".to_string();
    let mut t = BTreeMap::new();
    for (key, _) in I18N_DEFAULTS {
        t.insert((*key).to_string(), Value::str(&config.t(key, lang)));
    }
    let year = current_year().to_string();
    Value::Map(BTreeMap::from([
        (
            "config".to_string(),
            Value::Map(BTreeMap::from([
                ("title".to_string(), Value::str(&config.title)),
                ("base_url".to_string(), Value::str(&config.base_url)),
                ("author".to_string(), Value::str(&config.author)),
                ("language".to_string(), Value::str(lang)),
                ("languages".to_string(), Value::Arr(config_langs)),
                ("theme".to_string(), Value::str(&config.theme)),
                ("meta".to_string(), config.meta.clone()),
                ("params".to_string(), config.params.clone()),
            ])),
        ),
        (
            "site".to_string(),
            Value::Map(BTreeMap::from([
                ("title".to_string(), Value::str(&site.title_for(lang))),
                ("lang".to_string(), Value::str(lang)),
                ("languages".to_string(), Value::Arr(languages.clone())),
            ])),
        ),
        ("title".to_string(), Value::str(&site.title_for(lang))),
        (
            "description".to_string(),
            opt_str(&config.description_for(lang)),
        ),
        ("keywords".to_string(), opt_str(&config.keywords_for(lang))),
        ("lang".to_string(), Value::str(lang)),
        (
            "current_lang_display_name".to_string(),
            Value::str(&config.display_name_for(lang)),
        ),
        ("languages".to_string(), Value::Arr(languages)),
        ("home_url".to_string(), Value::str(&home_url)),
        ("home_active".to_string(), Value::Bool(home_active)),
        ("current_url".to_string(), Value::str(current_url)),
        ("categories".to_string(), categories),
        ("categories_active".to_string(), Value::Bool(categories_active)),
        ("pages".to_string(), pages),
        ("pages_tree".to_string(), pages_tree),
        ("page_sections".to_string(), page_sections),
        ("root_pages".to_string(), root_pages),
        ("recent".to_string(), recent),
        ("tags".to_string(), tag_cloud),
        ("show_tag_cloud".to_string(), Value::Bool(config.show_tag_cloud)),
        ("friend_links".to_string(), Value::Arr(friend_links)),
        ("rss_url".to_string(), Value::str(&rss_url)),
        ("sitemap_url".to_string(), Value::str(&sitemap_url)),
        ("show_rss".to_string(), Value::Bool(config.show_rss)),
        ("show_sitemap".to_string(), Value::Bool(config.show_sitemap)),
        ("search_action".to_string(), Value::str(&search_action)),
        ("search_query".to_string(), Value::str(current_query)),
        ("current_year".to_string(), Value::str(&year)),
        ("t".to_string(), Value::Map(t)),
    ]))
}

/// True if any node in a category tree (as built by `Site::category_tree_value`)
/// has `active == true`. Used to decide whether a sidebar tree branch should
/// auto-expand.
pub(crate) fn tree_has_active(v: &Value) -> bool {
    if let Value::Arr(items) = v {
        for item in items {
            if let Value::Map(m) = item {
                if matches!(m.get("active"), Some(Value::Bool(true))) {
                    return true;
                }
                if let Some(children) = m.get("children") {
                    if tree_has_active(children) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Single-page navigation entries: articles whose path doesn't live under
/// `posts/`. The `pages_tree` value supersedes this for the new header nav,
/// but we keep it for backward compatibility with custom themes.
fn pages_value(articles: &[Article], lang: &str, current_url: &str) -> Value {
    let mut out = Vec::new();
    for a in articles {
        if a.lang == lang && a.path.first().map(|s| s.as_str()) != Some("posts") {
            out.push(Value::Map(BTreeMap::from([
                ("title".to_string(), Value::str(&a.title)),
                ("url".to_string(), Value::str(&a.url)),
                ("active".to_string(), Value::Bool(a.url == current_url)),
            ])));
        }
    }
    Value::Arr(out)
}

/// Top-level entries for the header's **Pages** dropdown: every direct
/// child of `content/pages/` (sub-directories and standalone `.md`
/// leaves). When `pages_tree` is dominated by a single root node that
/// just wraps everything else (which is the common case — `pages/`
/// contains the whole non-post tree), drop the wrapper so the header
/// doesn't render its own title twice when the template iterates the
/// first-level items. Otherwise surface the top-level nodes as-is, so
/// authors can still put siblings of `pages/` at the `content/` root
/// if they really need to.
fn unwrapped_page_sections_value(pages_tree: &Value) -> Value {
    let items = match pages_tree {
        Value::Arr(a) => a,
        _ => return Value::Arr(Vec::new()),
    };
    if items.len() == 1 {
        if let Some(Value::Map(m)) = items.first() {
            if matches!(m.get("has_children"), Some(Value::Bool(true))) {
                if let Some(Value::Arr(children)) = m.get("children") {
                    if !children.is_empty() {
                        return Value::Arr(children.clone());
                    }
                }
            }
        }
    }
    Value::Arr(items.clone())
}

/// Top-level pages — articles whose path is empty (e.g. `content/about.md`
/// lives directly under `content/`, so its URL is `/about/`). These get a
/// dedicated nav slot as flat links, since they don't belong under any
/// section's hierarchy.
fn root_pages_value(articles: &[Article], lang: &str, current_url: &str) -> Value {
    let mut out = Vec::new();
    for a in articles {
        if a.lang == lang && a.path.is_empty() {
            out.push(Value::Map(BTreeMap::from([
                ("title".to_string(), Value::str(&a.title)),
                ("url".to_string(), Value::str(&a.url)),
                ("active".to_string(), Value::Bool(a.url == current_url)),
            ])));
        }
    }
    Value::Arr(out)
}

/// Most recent blog posts (sorted by `sort_ts` desc). Only articles under
/// `posts/` qualify — pages and other top-level sections are excluded.
fn recent_value(articles: &[Article], lang: &str, limit: usize) -> Value {
    let mut filtered: Vec<&Article> = articles
        .iter()
        .filter(|a| {
            a.lang == lang
                && a.path.first().map(|s| s.as_str()) == Some("posts")
        })
        .collect();
    filtered.sort_by_key(|a| std::cmp::Reverse(a.sort_ts));
    let mut out = Vec::new();
    for a in filtered.into_iter().take(limit) {
        let date = a.date.as_deref().unwrap_or("");
        out.push(Value::Map(BTreeMap::from([
            ("title".to_string(), Value::str(&a.title)),
            ("url".to_string(), Value::str(&a.url)),
            ("date".to_string(), opt_str(date)),
        ])));
    }
    Value::Arr(out)
}

fn opt_str(s: &str) -> Value {
    if s.is_empty() {
        Value::Null
    } else {
        Value::str(s)
    }
}

/// Render a layout slot with doc-partial precedence over theme-partial.
fn render_slot(site: &Site, ctx: &Value, slot: &str) -> Value {
    let engine = &site.engine;
    let name = format!("layout/{slot}.html");
    if !engine.has(&name) {
        return Value::Null;
    }
    match engine.render(&name, ctx) {
        Ok(html) => Value::str(html),
        Err(e) => {
            eprintln!("warning: render {slot}: {e}");
            Value::Null
        }
    }
}

fn with_layout(
    site: &Site,
    lang: &str,
    current_url: &str,
    _template_name: &str,
    payload_key: &str,
    payload: Value,
) -> Result<Value, String> {
    // Sidebar pre-fills the search box with the current query so editing
    // and resubmitting the form feels natural.
    let current_query = payload
        .as_map()
        .and_then(|m| m.get("query"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut ctx = base_ctx(site, lang, current_url, &current_query);
    let headers = render_slot(site, &ctx, "header");
    let side = render_slot(site, &ctx, "side");
    let footer = render_slot(site, &ctx, "footer");
    let inject = render_slot(site, &ctx, "inject");
    if let Value::Map(map) = &mut ctx {
        map.insert("header".to_string(), headers);
        map.insert("side".to_string(), side);
        map.insert("footer".to_string(), footer);
        map.insert("inject".to_string(), inject);
        map.insert(payload_key.to_string(), payload);
    }
    Ok(ctx)
}

fn render_with_context(site: &Site, template_name: &str, ctx: &Value) -> Result<String, String> {
    site.engine.render(template_name, ctx)
}

pub fn render_home(site: &Site, lang: &str, page: usize) -> Result<String, String> {
    let url = site.config.lang_prefix(lang);
    let payload = Site::home_value(site, lang, page);
    let ctx = with_layout(site, lang, &url, "index.html", "home", payload)?;
    render_with_context(site, "index.html", &ctx)
}

pub fn render_article(site: &Site, lang: &str, article: &Article) -> Result<String, String> {
    // Directory-driven: posts/* → article template, anything else → page.
    let template = if article.path.first().map(|s| s.as_str()) == Some("posts") {
        "article.html"
    } else {
        "page.html"
    };
    let breadcrumbs = site.breadcrumbs(&article.path, lang, &article.title);
    let payload = article.to_value();
    let ctx = with_layout(
        site,
        lang,
        &article.url,
        template,
        "article",
        payload,
    )?;
    // Inject breadcrumbs at top-level so the template can iterate
    // `{% for item in breadcrumbs %}`.
    let mut ctx = ctx;
    if let Value::Map(m) = &mut ctx {
        m.insert("breadcrumbs".to_string(), Value::Arr(breadcrumbs));
    }
    render_with_context(site, template, &ctx)
}

pub fn render_category(
    site: &Site,
    lang: &str,
    cat: &Category,
    page: usize,
) -> Result<String, String> {
    let url = cat.urls.get(lang).cloned().unwrap_or_default();
    let payload = Site::category_value(site, cat, lang, page);
    let title = payload
        .as_map()
        .and_then(|m| m.get("title").and_then(|v| v.as_str()))
        .unwrap_or(&cat.slug)
        .to_string();
    let breadcrumbs = site.breadcrumbs(&cat.path, lang, &title);
    let ctx = with_layout(site, lang, &url, "category.html", "category", payload)?;
    let mut ctx = ctx;
    if let Value::Map(m) = &mut ctx {
        m.insert("breadcrumbs".to_string(), Value::Arr(breadcrumbs));
    }
    render_with_context(site, "category.html", &ctx)
}

/// Render a tag landing page (`/tags/<tag>/`): the list of every document
/// carrying the tag, with breadcrumbs Index › Tags › <tag>.
pub fn render_tag(site: &Site, lang: &str, name: &str, page: usize) -> Result<String, String> {
    let payload = match site.tag_value(name, lang, page) {
        Some(v) => v,
        None => return Err("not found".into()),
    };
    let url = payload
        .as_map()
        .and_then(|m| m.get("url").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let breadcrumbs = site.tag_breadcrumbs(name, lang);
    let ctx = with_layout(site, lang, &url, "tag.html", "tag", payload)?;
    let mut ctx = ctx;
    if let Value::Map(m) = &mut ctx {
        m.insert("breadcrumbs".to_string(), Value::Arr(breadcrumbs));
    }
    render_with_context(site, "tag.html", &ctx)
}

/// Render the `/tags/` index page listing every tag for the language.
pub fn render_tags_index(site: &Site, lang: &str) -> Result<String, String> {
    let payload = site.tags_index_value(lang);
    let url = payload
        .as_map()
        .and_then(|m| m.get("url").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let breadcrumbs = site.tags_index_breadcrumbs(lang);
    let ctx = with_layout(site, lang, &url, "tags.html", "tags_index", payload)?;
    let mut ctx = ctx;
    if let Value::Map(m) = &mut ctx {
        m.insert("breadcrumbs".to_string(), Value::Arr(breadcrumbs));
    }
    render_with_context(site, "tags.html", &ctx)
}

/// Render a directory landing page (e.g. `/pages/docs/`) using
/// `page_section.html`. Falls back to 404 if the path has no `_index.md`.
pub fn render_section(
    site: &Site,
    lang: &str,
    dir_path: &[String],
    page: usize,
) -> Result<String, String> {
    let payload = match site.page_section_value(dir_path, lang, page) {
        Some(v) => v,
        None => return Err("not found".into()),
    };
    let url = payload
        .as_map()
        .and_then(|m| m.get("url").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let title = payload
        .as_map()
        .and_then(|m| m.get("title").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let breadcrumbs = site.breadcrumbs(dir_path, lang, &title);
    let ctx = with_layout(site, lang, &url, "page_section.html", "section", payload)?;
    let mut ctx = ctx;
    if let Value::Map(m) = &mut ctx {
        m.insert("breadcrumbs".to_string(), Value::Arr(breadcrumbs));
    }
    render_with_context(site, "page_section.html", &ctx)
}

pub fn render_not_found(site: &Site, lang: &str) -> Result<String, String> {
    let payload = Value::Map(BTreeMap::from([(
        "title".to_string(),
        Value::str("Not Found"),
    )]));
    let ctx = with_layout(site, lang, "", "404.html", "page", payload)?;
    render_with_context(site, "404.html", &ctx)
}

pub fn render_search(site: &Site, lang: &str, query: &str) -> Result<String, String> {
    let hits = feed::search_results(site, query, lang);
    let mut articles: Vec<Value> = Vec::with_capacity(hits.len());
    for h in &hits {
        let mut v = h.article.to_value();
        if let Value::Map(m) = &mut v {
            // The active language's hits surface first; tag the rest with
            // their language code so the template can render a small badge.
            m.insert(
                "current_lang".to_string(),
                Value::Bool(h.article.lang == lang),
            );
        }
        articles.push(v);
    }
    let payload = Value::Map(BTreeMap::from([
        ("query".to_string(), Value::str(query)),
        ("query_trimmed".to_string(), Value::str(query.trim())),
        ("count".to_string(), Value::int(articles.len() as i64)),
        // The template engine has no expression comparisons, so resolve the
        // pluralisation here. Templates just print `search.count_word`.
        (
            "count_word".to_string(),
            Value::str(if articles.len() == 1 { "match" } else { "matches" }),
        ),
        ("articles".to_string(), Value::Arr(articles)),
    ]));
    // Use lang_prefix so the URL is stable per language and matches the
    // sidebar's `action="/<lang>/search"` shape.
    let url = if lang == site.default_lang {
        "/search".to_string()
    } else {
        format!("/{lang}/search")
    };
    let ctx = with_layout(site, lang, &url, "search.html", "search", payload)?;
    render_with_context(site, "search.html", &ctx)
}

fn current_year() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    days_to_year(now / 86400)
}

fn days_to_year(days: i64) -> i64 {
    fn leaps(v: i64) -> i64 {
        v / 4 - v / 100 + v / 400
    }
    fn start(y: i64) -> i64 {
        365 * (y - 1970) + leaps(y - 1) - leaps(1969)
    }
    let mut y = 1970 + days / 366;
    if days < start(y) {
        while days < start(y) {
            y -= 1;
        }
    } else {
        while days >= start(y + 1) {
            y += 1;
        }
    }
    y
}

