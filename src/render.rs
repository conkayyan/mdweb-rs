use std::collections::BTreeMap;

use crate::content::{Article, Category, Site};
use crate::value::Value;

/// Build the shared context for a language.
fn base_ctx(site: &Site, lang: &str, current_url: &str) -> Value {
    let config = &site.config;
    let mut languages: Vec<Value> = Vec::new();
    for l in &site.languages {
        languages.push(Value::Map(BTreeMap::from([
            ("code".to_string(), Value::str(l)),
            ("title".to_string(), Value::str(&config.title_for(l))),
            ("url".to_string(), Value::str(&config.lang_prefix(l))),
        ])));
    }
    let mut config_langs: Vec<Value> = Vec::new();
    for l in &config.languages {
        config_langs.push(Value::str(l));
    }
    let categories = site.category_tree_value(&site.tree, lang);
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
        ("languages".to_string(), Value::Arr(languages)),
        ("home_url".to_string(), Value::str(&config.lang_prefix(lang))),
        ("current_url".to_string(), Value::str(current_url)),
        ("categories".to_string(), categories),
        ("current_year".to_string(), Value::str(&year)),
    ]))
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
    let name = if engine.has(&format!("slot::{slot}")) {
        format!("slot::{slot}")
    } else if engine.has(&format!("partials/{slot}.html")) {
        format!("partials/{slot}.html")
    } else {
        return Value::Null;
    };
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
    template_name: &str,
    payload_key: &str,
    payload: Value,
) -> Result<Value, String> {
    let mut ctx = base_ctx(site, lang, current_url);
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

pub fn render_home(site: &Site, lang: &str) -> Result<String, String> {
    let url = site.config.lang_prefix(lang);
    let payload = Site::home_value(site, lang);
    let ctx = with_layout(site, lang, &url, "index.html", "home", payload)?;
    render_with_context(site, "index.html", &ctx)
}

pub fn render_article(site: &Site, lang: &str, article: &Article) -> Result<String, String> {
    let template = if article.layout == "page" {
        "page.html"
    } else {
        "article.html"
    };
    let ctx = with_layout(
        site,
        lang,
        &article.url,
        template,
        "article",
        article.to_value(),
    )?;
    render_with_context(site, template, &ctx)
}

pub fn render_category(site: &Site, lang: &str, cat: &Category) -> Result<String, String> {
    let url = cat.urls.get(lang).cloned().unwrap_or_default();
    let payload = Site::category_value(site, cat, lang);
    let ctx = with_layout(site, lang, &url, "category.html", "category", payload)?;
    render_with_context(site, "category.html", &ctx)
}

pub fn render_not_found(site: &Site, lang: &str) -> Result<String, String> {
    let payload = Value::Map(BTreeMap::from([(
        "title".to_string(),
        Value::str("Not Found"),
    )]));
    let ctx = with_layout(site, lang, "", "404.html", "page", payload)?;
    render_with_context(site, "404.html", &ctx)
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