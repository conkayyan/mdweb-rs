use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::markdown;
use crate::parse::parse_frontmatter;
use crate::render::tree_has_active;
use crate::template::Engine;
use crate::value::Value;

/// Embedded default theme files.
pub mod theme_files {
    pub const BASE: &str = include_str!("../template/base.html");
    pub const INDEX: &str = include_str!("../template/index.html");
    pub const CATEGORY: &str = include_str!("../template/category.html");
    pub const ARTICLE: &str = include_str!("../template/article.html");
    pub const PAGE: &str = include_str!("../template/page.html");
    pub const NOT_FOUND: &str = include_str!("../template/404.html");
    pub const PARTIAL_HEADER: &str = include_str!("../template/partials/header.html");
    pub const PARTIAL_FOOTER: &str = include_str!("../template/partials/footer.html");
    pub const PARTIAL_SIDE: &str = include_str!("../template/partials/side.html");
    pub const PARTIAL_INJECT: &str = include_str!("../template/partials/inject.html");
    pub const PARTIAL_CAT_NODE: &str = include_str!("../template/partials/_cat_node.html");
    pub const STYLE: &str = include_str!("../static_default/style.css");
}

/// Layout partials from the doc's `_layout/` directory.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub header: Option<String>,
    pub footer: Option<String>,
    pub side: Option<String>,
    pub inject: Option<String>,
}

impl Layout {
    pub fn load(doc_root: &Path) -> Layout {
        let mut l = Layout::default();
        let p = doc_root.join("_layout").join("header.html");
        if let Ok(s) = std::fs::read_to_string(&p) {
            l.header = Some(s);
        }
        let p = doc_root.join("_layout").join("footer.html");
        if let Ok(s) = std::fs::read_to_string(&p) {
            l.footer = Some(s);
        }
        let p = doc_root.join("_layout").join("side.html");
        if let Ok(s) = std::fs::read_to_string(&p) {
            l.side = Some(s);
        }
        let p = doc_root.join("_layout").join("inject.html");
        if let Ok(s) = std::fs::read_to_string(&p) {
            l.inject = Some(s);
        }
        l
    }
}

/// A rendered article/page in one language.
#[derive(Debug, Clone)]
pub struct Article {
    pub slug: String,
    pub path: Vec<String>,
    pub lang: String,
    pub url: String,
    pub title: String,
    pub date: Option<String>,
    pub date_iso: Option<String>,
    pub updated: Option<String>,
    pub updated_iso: Option<String>,
    pub author: String,
    pub tags: Vec<String>,
    pub summary: String,
    pub content: String,
    pub layout: String,
    pub meta: Value,
    pub extra: Value,
    pub translations: Vec<Value>,
    pub prev: Option<(String, String)>,
    pub next: Option<(String, String)>,
    pub draft: bool,
    pub sort_ts: i64,
}

impl Article {
    /// Serialise to a template value.
    pub fn to_value(&self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("slug".into(), Value::str(&self.slug));
        m.insert("url".into(), Value::str(&self.url));
        m.insert("title".into(), Value::str(&self.title));
        m.insert("date".into(), opt_str(self.date.as_deref()));
        m.insert("date_iso".into(), opt_str(self.date_iso.as_deref()));
        m.insert("updated".into(), opt_str(self.updated.as_deref()));
        m.insert("updated_iso".into(), opt_str(self.updated_iso.as_deref()));
        m.insert("author".into(), Value::str(&self.author));
        m.insert(
            "tags".into(),
            Value::Arr(self.tags.iter().cloned().map(Value::str).collect()),
        );
        m.insert("summary".into(), Value::str(&self.summary));
        m.insert("content".into(), Value::str(&self.content));
        m.insert("meta".into(), self.meta.clone());
        m.insert("extra".into(), self.extra.clone());
        m.insert("translations".into(), Value::Arr(self.translations.clone()));
        m.insert("prev".into(), opt_link(self.prev.as_ref()));
        m.insert("next".into(), opt_link(self.next.as_ref()));
        m.insert("sort_ts".into(), Value::int(self.sort_ts));
        Value::Map(m)
    }
}

fn opt_str(s: Option<&str>) -> Value {
    match s {
        Some(v) => Value::str(v),
        None => Value::Null,
    }
}

fn opt_link(l: Option<&(String, String)>) -> Value {
    match l {
        Some((t, u)) => Value::Map(BTreeMap::from([
            ("title".into(), Value::str(t)),
            ("url".into(), Value::str(u)),
        ])),
        None => Value::Null,
    }
}

/// A category node derived from the doc directory tree.
#[derive(Debug, Clone)]
pub struct Category {
    pub slug: String,
    pub path: Vec<String>,
    pub urls: BTreeMap<String, String>,
    pub titles: BTreeMap<String, String>,
    pub descriptions: BTreeMap<String, String>,
    pub contents: BTreeMap<String, String>,
    pub children: Vec<Category>,
}

impl Category {
    fn nav_value(&self, lang: &str) -> Value {
        Value::Map(BTreeMap::from([
            (
                "title".into(),
                Value::str(self.titles.get(lang).cloned().unwrap_or_else(|| self.slug.clone())),
            ),
            (
                "url".into(),
                Value::str(self.urls.get(lang).cloned().unwrap_or_else(|| "#".into())),
            ),
        ]))
    }
}

struct RawArticle {
    path: Vec<String>,
    slug: String,
    lang: String,
    _file: PathBuf,
    fm: BTreeMap<String, Value>,
    html: String,
    mtime: Option<i64>,
}

impl RawArticle {
    fn field(&self, name: &str) -> Option<&str> {
        self.fm.get(name).and_then(|v| v.as_str())
    }
}

/// A built, renderable site.
pub struct Site {
    pub config: Config,
    pub doc_root: PathBuf,
    pub languages: Vec<String>,
    pub default_lang: String,
    pub tree: Vec<Category>,
    pub articles: Vec<Article>,
    pub home_content: BTreeMap<String, String>,
    pub engine: Engine,
    /// Whether the embedded default static CSS is a valid fallback.
    pub engine_embedded: bool,
}

impl Site {
    pub fn build(
        doc_root: &Path,
        template_override: Option<PathBuf>,
    ) -> Result<Site, String> {
        let doc_root = doc_root
            .canonicalize()
            .unwrap_or_else(|_| doc_root.to_path_buf());
        let config = Config::load(&doc_root);
        let languages = config.resolved_languages();
        let default_lang = if languages.contains(&config.language) {
            config.language.clone()
        } else {
            languages[0].clone()
        };
        let layout = Layout::load(&doc_root);
        let (engine, engine_embedded) =
            load_engine(&config, &doc_root, &layout, template_override)?;

        let mut list: Vec<(String, PathBuf)> = Vec::new();
        walk_doc(&doc_root, &doc_root, &mut list);

        let mut indices: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let mut raws: Vec<RawArticle> = Vec::new();

        for (rel_str, abs) in list {
            if rel_str.ends_with(".md") {
                let source = std::fs::read_to_string(&abs).map_err(|e| e.to_string())?;
                let (fm, body) = parse_frontmatter(&source);
                if fm.get("draft").and_then(|v| v.as_bool()) == Some(true) {
                    continue;
                }
                let (dir, fname) = match rel_str.rfind('/') {
                    Some(i) => (rel_str[..i].to_string(), rel_str[i + 1..].to_string()),
                    None => (String::new(), rel_str.clone()),
                };
                let stem = fname.strip_suffix(".md").unwrap_or(&fname).to_string();
                let (base, lang) = parse_name(&stem, &languages, &default_lang);
                let html = markdown::render(&body);
                let mtime = std::fs::metadata(&abs)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);
                if base == "_index" {
                    indices
                        .entry(dir.clone())
                        .or_default()
                        .push(Value::Map(BTreeMap::from([
                            ("lang".into(), Value::str(&lang)),
                            ("fields".into(), Value::Map(fm)),
                            ("html".into(), Value::str(&html)),
                        ])));
                } else {
                    raws.push(RawArticle {
                        path: split_dir(&dir),
                        slug: base,
                        lang,
                        _file: abs,
                        fm,
                        html,
                        mtime,
                    });
                }
            }
        }

        let mut home_content: BTreeMap<String, String> = BTreeMap::new();
        if let Some(entries) = indices.get("") {
            for entry in entries {
                let lang = entry
                    .path("lang")
                    .and_then(|l| l.as_str())
                    .unwrap_or(&default_lang)
                    .to_string();
                if let Some(html) = entry.path("html").map(|h| h.render()) {
                    home_content.insert(lang, html);
                }
            }
        }

        let mut all_paths: HashSet<Vec<String>> = HashSet::new();
        for k in indices.keys() {
            for pre in prefixes(&split_dir(k)) {
                all_paths.insert(pre);
            }
        }
        for ra in &raws {
            for pre in prefixes(&ra.path) {
                all_paths.insert(pre);
            }
        }
        let tree = build_tree(&all_paths, &indices, &config, &default_lang);

        let mut groups: BTreeMap<String, Vec<&RawArticle>> = BTreeMap::new();
        for ra in &raws {
            let key = format!("{}|{}", ra.path.join("/"), ra.slug);
            groups.entry(key).or_default().push(ra);
        }

        let mut articles = Vec::new();
        for group in groups.values() {
            for ra in group.iter() {
                let translations: Vec<Value> = group
                    .iter()
                    .filter(|o| o.lang != ra.lang)
                    .map(|o| {
                        Value::Map(BTreeMap::from([
                            ("lang".to_string(), Value::str(&o.lang)),
                            (
                                "title".to_string(),
                                Value::str(o.field("title").unwrap_or(&o.slug)),
                            ),
                            (
                                "url".to_string(),
                                Value::str(&url_for(
                                    &config,
                                    &default_lang,
                                    &o.path,
                                    Some(&o.slug),
                                    &o.lang,
                                )),
                            ),
                            (
                                "display_name".to_string(),
                                Value::str(&config.display_name_for(&o.lang)),
                            ),
                        ]))
                    })
                    .collect();

                let (date, date_iso, dts) = parse_date(ra.field("date"));
                let (updated, updated_iso, uts) = parse_date(ra.field("updated"));
                let sort_ts = dts.or(uts).or(ra.mtime).unwrap_or(0);
                let author = ra
                    .field("author")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| config.author.clone());
                let summary = ra
                    .field("summary")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| make_summary(&ra.html));
                let layout = ra
                    .field("layout")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        if ra.path.is_empty() {
                            "page".to_string()
                        } else {
                            "article".to_string()
                        }
                    });
                let title = ra
                    .field("title")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| ra.slug.clone());
                let tags: Vec<String> = ra
                    .fm
                    .get("tags")
                    .and_then(|v| v.as_arr())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                articles.push(Article {
                    slug: ra.slug.clone(),
                    path: ra.path.clone(),
                    lang: ra.lang.clone(),
                    url: url_for(&config, &default_lang, &ra.path, Some(&ra.slug), &ra.lang),
                    title,
                    date,
                    date_iso,
                    updated,
                    updated_iso,
                    author,
                    tags,
                    summary,
                    content: ra.html.clone(),
                    layout,
                    meta: ra.fm.get("meta").cloned().unwrap_or_else(Value::map),
                    extra: Value::Map(
                        ra.fm
                            .iter()
                            .filter(|(k, _)| {
                                !matches!(
                                    k.as_str(),
                                    "title"
                                        | "date"
                                        | "updated"
                                        | "author"
                                        | "tags"
                                        | "draft"
                                        | "layout"
                                        | "summary"
                                        | "meta"
                                )
                            })
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect(),
                    ),
                    translations,
                    prev: None,
                    next: None,
                    draft: ra
                        .fm
                        .get("draft")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    sort_ts,
                });
            }
        }

        articles.sort_by_key(|a| std::cmp::Reverse(a.sort_ts));
        compute_navigation(&mut articles);

        Ok(Site {
            config,
            doc_root,
            languages,
            default_lang,
            tree,
            articles,
            home_content,
            engine,
            engine_embedded,
        })
    }

    pub fn title_for(&self, lang: &str) -> String {
        self.config.title_for(lang)
    }

    pub fn home_value(&self, lang: &str) -> Value {
        let mut arts: Vec<Value> = self
            .articles
            .iter()
            .filter(|a| a.lang == lang && !a.draft)
            .map(|a| a.to_value())
            .collect();
        arts.sort_by_key(|a| {
            std::cmp::Reverse(a.path("sort_ts").and_then(|v| v.as_int()).unwrap_or(0))
        });
        let content = self
            .home_content
            .get(lang)
            .or_else(|| self.home_content.get(&self.default_lang))
            .map(|s| Value::str(s))
            .unwrap_or(Value::Null);
        Value::Map(BTreeMap::from([
            ("content".to_string(), content),
            ("articles".to_string(), Value::Arr(arts)),
        ]))
    }

    pub fn category_value(&self, cat: &Category, lang: &str) -> Value {
        let mut arts: Vec<Value> = self
            .articles
            .iter()
            .filter(|a| a.lang == lang && !a.draft && starts_with(&a.path, &cat.path))
            .map(|a| a.to_value())
            .collect();
        arts.sort_by_key(|a| {
            std::cmp::Reverse(a.path("sort_ts").and_then(|v| v.as_int()).unwrap_or(0))
        });
        let children: Vec<Value> = cat.children.iter().map(|c| c.nav_value(lang)).collect();
        Value::Map(BTreeMap::from([
            (
                "title".to_string(),
                Value::str(cat.titles.get(lang).cloned().unwrap_or_else(|| cat.slug.clone())),
            ),
            ("slug".to_string(), Value::str(&cat.slug)),
            (
                "url".to_string(),
                Value::str(cat.urls.get(lang).cloned().unwrap_or_default()),
            ),
            (
                "description".to_string(),
                opt_str(cat.descriptions.get(lang).map(|s| s.as_str())),
            ),
            (
                "content".to_string(),
                opt_str(cat.contents.get(lang).map(|s| s.as_str())),
            ),
            ("articles".to_string(), Value::Arr(arts)),
            ("children".to_string(), Value::Arr(children)),
        ]))
    }

    pub fn category_tree_value(&self, cats: &[Category], lang: &str, current_url: &str) -> Value {
        let mut out = Vec::new();
        for c in cats {
            let children_val = self.category_tree_value(&c.children, lang, current_url);
            let url = c.urls.get(lang).cloned().unwrap_or_else(|| "#".to_string());
            let active = url == current_url;
            let descendant_active = active || tree_has_active(&children_val);
            let m = BTreeMap::from([
                (
                    "title".to_string(),
                    Value::str(c.titles.get(lang).cloned().unwrap_or_else(|| c.slug.clone())),
                ),
                ("url".to_string(), Value::str(&url)),
                ("active".to_string(), Value::Bool(active)),
                (
                    "descendant_active".to_string(),
                    Value::Bool(descendant_active),
                ),
                ("has_children".to_string(), Value::Bool(!c.children.is_empty())),
                ("children".to_string(), children_val),
            ]);
            out.push(Value::Map(m));
        }
        Value::Arr(out)
    }

    /// Raw file under the doc root at a URL-relative path.
    pub fn resolve_file(&self, rel: &str) -> Option<PathBuf> {
        let p = self.doc_root.join(rel);
        if p.is_file() {
            Some(p)
        } else {
            None
        }
    }
}

fn load_engine(
    config: &Config,
    doc_root: &Path,
    layout: &Layout,
    override_dir: Option<PathBuf>,
) -> Result<(Engine, bool), String> {
    let mut engine = Engine::new();
    let theme_dir = override_dir;
    // Returns (whether the engine ended up with embedded defaults only).
    let embedded = if let Some(dir) = theme_dir {
        // --template override: load embedded first so any partials the user
        // doesn't ship still resolve; user templates override on top.
        load_embedded(&mut engine)?;
        load_dir_templates(&mut engine, &dir)?;
        false
    } else if config.theme != "default" && !config.theme.is_empty() {
        let cands = [doc_root.join(&config.theme), PathBuf::from(&config.theme)];
        if let Some(d) = cands.iter().find(|c| c.is_dir()) {
            // Load embedded first as a fallback (e.g. _cat_node.html), then
            // the user's theme on top so the user's templates win.
            load_embedded(&mut engine)?;
            load_dir_templates(&mut engine, d)?;
            false
        } else {
            eprintln!(
                "warning: template \"{}\" not found; using the built-in default",
                config.theme
            );
            load_embedded(&mut engine)?;
            true
        }
    } else {
        load_embedded(&mut engine)?;
        true
    };

    for (slot, src) in [
        ("header", layout.header.as_deref()),
        ("footer", layout.footer.as_deref()),
        ("side", layout.side.as_deref()),
        ("inject", layout.inject.as_deref()),
    ] {
        if let Some(s) = src {
            engine.add(&format!("slot::{slot}"), s)?;
        }
    }
    Ok((engine, embedded))
}

fn load_embedded(engine: &mut Engine) -> Result<(), String> {
    engine.add_many(vec![
        ("base.html", theme_files::BASE.to_string()),
        ("index.html", theme_files::INDEX.to_string()),
        ("category.html", theme_files::CATEGORY.to_string()),
        ("article.html", theme_files::ARTICLE.to_string()),
        ("page.html", theme_files::PAGE.to_string()),
        ("404.html", theme_files::NOT_FOUND.to_string()),
        ("partials/header.html", theme_files::PARTIAL_HEADER.to_string()),
        ("partials/footer.html", theme_files::PARTIAL_FOOTER.to_string()),
        ("partials/side.html", theme_files::PARTIAL_SIDE.to_string()),
        ("partials/inject.html", theme_files::PARTIAL_INJECT.to_string()),
        ("partials/_cat_node.html", theme_files::PARTIAL_CAT_NODE.to_string()),
    ])?;
    Ok(())
}

fn load_dir_templates(engine: &mut Engine, dir: &Path) -> Result<(), String> {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    collect_templates(dir, dir, &mut files);
    if files.is_empty() {
        return Err(format!(
            "template directory {} has no *.html files",
            dir.display()
        ));
    }
    for (rel, path) in files {
        let src = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        engine.add(&rel, &src)?;
    }
    Ok(())
}

fn collect_templates(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_templates(&p, base, out);
        } else if p.extension().map(|x| x == "html").unwrap_or(false) {
            if let Ok(rel) = p.strip_prefix(base) {
                out.push((rel.to_string_lossy().replace('\\', "/"), p));
            }
        }
    }
}

fn walk_doc(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.')
            || name == "_layout"
            || name == "template"
            || name == "_static"
            || name == "static"
        {
            continue;
        }
        if p.is_dir() {
            walk_doc(&p, base, out);
        } else if let Ok(rel) = p.strip_prefix(base) {
            out.push((rel.to_string_lossy().replace('\\', "/"), p));
        }
    }
}

fn parse_name(stem: &str, languages: &[String], default: &str) -> (String, String) {
    for lang in languages {
        if let Some(base) = stem.strip_suffix(&format!(".{lang}")) {
            return (base.to_string(), lang.clone());
        }
    }
    (stem.to_string(), default.to_string())
}

fn split_dir(dir: &str) -> Vec<String> {
    if dir.is_empty() {
        return Vec::new();
    }
    dir.split('/').map(|s| s.to_string()).collect()
}

fn prefixes(p: &[String]) -> Vec<Vec<String>> {
    (1..=p.len()).map(|i| p[..i].to_vec()).collect()
}

fn starts_with(hay: &[String], needle: &[String]) -> bool {
    hay.len() >= needle.len() && hay[..needle.len()] == needle[..]
}

fn url_for(
    config: &Config,
    _default: &str,
    path: &[String],
    slug: Option<&String>,
    lang: &str,
) -> String {
    let mut pieces: Vec<String> = path.to_vec();
    if let Some(s) = slug {
        pieces.push(s.clone());
    }
    let joined = pieces.join("/");
    let prefix = config.lang_prefix(lang);
    if prefix == "/" {
        if joined.is_empty() {
            "/".to_string()
        } else {
            format!("/{joined}/")
        }
    } else if joined.is_empty() {
        prefix
    } else {
        format!("{prefix}{joined}/")
    }
}

/// Returns (display, iso, unix_ts).
fn parse_date(raw: Option<&str>) -> (Option<String>, Option<String>, Option<i64>) {
    let raw = match raw {
        Some(r) if !r.is_empty() => r,
        _ => return (None, None, None),
    };
    let date_part = raw
        .split('T')
        .next()
        .unwrap_or(raw)
        .split_whitespace()
        .next()
        .unwrap_or(raw);
    if let Some(ts) = parse_epoch(date_part) {
        let iso = date_part.replace('/', "-");
        return (Some(iso.clone()), Some(iso), Some(ts));
    }
    (Some(raw.to_string()), Some(raw.to_string()), None)
}

fn parse_epoch(date_part: &str) -> Option<i64> {
    let mut parts = date_part.split('-');
    let y = parts.next()?.parse::<i32>().ok()?;
    let m = parts.next()?.parse::<u32>().ok()?;
    let d = parts.next()?.parse::<u32>().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(date_to_epoch(y, m, d))
}

fn date_to_epoch(y: i32, m: u32, d: u32) -> i64 {
    let my = if m <= 2 { y - 1 } else { y };
    let mut days = 365 * (i64::from(my) - 1970);
    days += (i64::from(my) - 1969) / 4;
    days -= (i64::from(my) - 1901) / 100;
    days += (i64::from(my) - 1601) / 400;
    let leap = if is_leap(y) && m > 2 { 1 } else { 0 };
    let md: i64 = match m {
        1 => 0,
        2 => 31,
        3 => 59,
        4 => 90,
        5 => 120,
        6 => 151,
        7 => 181,
        8 => 212,
        9 => 243,
        10 => 273,
        11 => 304,
        12 => 334,
        _ => 0,
    };
    days + md + leap + i64::from(d) - 1
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn compute_navigation(articles: &mut Vec<Article>) {
    let old = articles.clone();
    let mut groups: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (i, a) in old.iter().enumerate() {
        groups
            .entry((a.lang.clone(), a.path.join("/")))
            .or_default()
            .push(i);
    }
    for idxs in groups.values() {
        let mut s = idxs.clone();
        s.sort_by_key(|&k| old[k].sort_ts);
        for (k, &i) in s.iter().enumerate() {
            if k > 0 {
                let j = s[k - 1];
                articles[i].prev = Some((old[j].title.clone(), old[j].url.clone()));
            }
            if k + 1 < s.len() {
                let j = s[k + 1];
                articles[i].next = Some((old[j].title.clone(), old[j].url.clone()));
            }
        }
    }
}

fn make_summary(html: &str) -> String {
    let txt = strip_tags(html);
    let words: Vec<&str> = txt.split_whitespace().collect();
    let total: usize = words.iter().map(|w| w.chars().count()).sum();
    if total <= 240 {
        return words.join(" ");
    }
    let mut out = String::new();
    let mut count = 0;
    for w in words {
        count += w.chars().count() + 1;
        if count > 240 {
            break;
        }
        out.push_str(w);
        out.push(' ');
    }
    out.trim().to_string() + "…"
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn build_tree(
    all_paths: &HashSet<Vec<String>>,
    indices: &BTreeMap<String, Vec<Value>>,
    config: &Config,
    default_lang: &str,
) -> Vec<Category> {
    let mut tops: Vec<Vec<String>> = all_paths
        .iter()
        .filter(|p| p.len() == 1)
        .cloned()
        .collect();
    tops.sort();
    tops.iter()
        .map(|p| build_node(p, all_paths, indices, config, default_lang))
        .collect()
}

fn build_node(
    path: &[String],
    all_paths: &HashSet<Vec<String>>,
    indices: &BTreeMap<String, Vec<Value>>,
    config: &Config,
    default_lang: &str,
) -> Category {
    let slug = path.last().cloned().unwrap_or_default();
    let key = path.join("/");
    let mut titles = BTreeMap::new();
    let mut descriptions = BTreeMap::new();
    let mut contents = BTreeMap::new();
    let mut urls = BTreeMap::new();
    let langs = config.resolved_languages();
    for lang in &langs {
        if let Some(items) = indices.get(&key) {
            if let Some(it) = items.iter().find(|v| {
                v.path("lang").and_then(|l| l.as_str()) == Some(lang.as_str())
            }) {
                let fm = it
                    .path("fields")
                    .and_then(|f| f.as_map().cloned())
                    .unwrap_or_default();
                if let Some(t) = fm.get("title").and_then(|v| v.as_str()) {
                    titles.insert(lang.clone(), t.to_string());
                }
                if let Some(d) = fm.get("summary").and_then(|v| v.as_str()) {
                    descriptions.insert(lang.clone(), d.to_string());
                }
                if let Some(h) = it.path("html").and_then(|v| v.as_str()) {
                    contents.insert(lang.clone(), h.to_string());
                }
            }
        }
        urls.insert(
            lang.clone(),
            url_for(config, default_lang, path, None, lang),
        );
    }
    let mut child_paths: Vec<Vec<String>> = all_paths
        .iter()
        .filter(|k| k.len() == path.len() + 1 && k[..path.len()] == path[..])
        .cloned()
        .collect();
    child_paths.sort();
    let children = child_paths
        .iter()
        .map(|c| build_node(c, all_paths, indices, config, default_lang))
        .collect();
    Category {
        slug,
        path: path.to_vec(),
        urls,
        titles,
        descriptions,
        contents,
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write(dir: &PathBuf, rel: &str, body: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn translation_entries_carry_display_name() {
        let dir = std::env::temp_dir().join(format!(
            "mdweb-content-tr-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        write(
            &dir,
            "site.toml",
            r#"
            languages = ["en", "zh"]

            [lang.zh]
            display_name = "简体中文"
            "#,
        );
        write(
            &dir,
            "hello.md",
            "---\ntitle: Hello\n---\nbody\n",
        );
        write(
            &dir,
            "hello.zh.md",
            "---\ntitle: 你好\n---\nbody\n",
        );

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
        let dir = std::env::temp_dir().join(format!(
            "mdweb-content-home-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
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

        let en_val = site.home_value("en");
        let en = en_val.as_map().expect("en map");
        assert_eq!(en.get("content").and_then(|v| v.as_str()), Some("<p>English body</p>\n"));
        let zh_val = site.home_value("zh");
        let zh = zh_val.as_map().expect("zh map");
        assert_eq!(zh.get("content").and_then(|v| v.as_str()), Some("<p>中文内容</p>\n"));
    }
}
