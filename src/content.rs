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
    pub const BASE: &str = include_str!("../template/default/base.html");
    pub const INDEX: &str = include_str!("../template/default/index.html");
    pub const CATEGORY: &str = include_str!("../template/default/category.html");
    pub const ARTICLE: &str = include_str!("../template/default/article.html");
    pub const PAGE: &str = include_str!("../template/default/page.html");
    pub const SEARCH: &str = include_str!("../template/default/search.html");
    pub const NOT_FOUND: &str = include_str!("../template/default/404.html");
    pub const PARTIAL_HEADER: &str = include_str!("../template/default/layout/header.html");
    pub const PARTIAL_FOOTER: &str = include_str!("../template/default/layout/footer.html");
    pub const PARTIAL_SIDE: &str = include_str!("../template/default/layout/side.html");
    pub const PARTIAL_INJECT: &str = include_str!("../template/default/layout/inject.html");
    pub const PARTIAL_CAT_NODE: &str = include_str!("../template/default/layout/_cat_node.html");
    pub const PARTIAL_NAV_NODE: &str = include_str!("../template/default/layout/_nav_node.html");
    pub const PAGE_SECTION: &str = include_str!("../template/default/page_section.html");
    pub const STYLE: &str = include_str!("../static/style.css");
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
    pub meta: Value,
    pub extra: Value,
    pub translations: Vec<Value>,
    pub prev: Option<(String, String)>,
    pub next: Option<(String, String)>,
    pub draft: bool,
    pub sort_ts: i64,
}

impl Article {
    /// Estimated reading time in minutes. Counts CJK characters at ~300
    /// chars/min and whitespace-separated tokens in the rest at ~200 wpm;
    /// rounds up. Returns `0` for empty content so callers can omit the
    /// display with a simple truthy check.
    pub fn reading_minutes(&self) -> i64 {
        let mut cjk: usize = 0;
        let mut other = String::new();
        let mut in_tag = false;
        for c in self.content.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => {
                    let cp = c as u32;
                    if (0x4E00..=0x9FFF).contains(&cp)        // CJK Unified Ideographs
                        || (0x3040..=0x309F).contains(&cp)    // Hiragana
                        || (0x30A0..=0x30FF).contains(&cp)    // Katakana
                    {
                        cjk += 1;
                    } else {
                        other.push(c);
                    }
                }
                _ => {}
            }
        }
        let words = other.split_whitespace().count();
        if cjk == 0 && words == 0 {
            return 0;
        }
        let minutes = (cjk as f64 / 300.0) + (words as f64 / 200.0);
        minutes.ceil() as i64
    }

    /// Serialise to a template value.
    pub fn to_value(&self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("slug".into(), Value::str(&self.slug));
        m.insert("url".into(), Value::str(&self.url));
        m.insert("title".into(), Value::str(&self.title));
        m.insert("lang".into(), Value::str(&self.lang));
        m.insert("date".into(), opt_str(self.date.as_deref()));
        m.insert("date_iso".into(), opt_str(self.date_iso.as_deref()));
        m.insert("updated".into(), opt_str(self.updated.as_deref()));
        m.insert("updated_iso".into(), opt_str(self.updated_iso.as_deref()));
        m.insert("author".into(), Value::str(&self.author));
        m.insert("reading_minutes".into(), Value::int(self.reading_minutes()));
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
    pub theme: String,
    pub tree: Vec<Category>,
    pub articles: Vec<Article>,
    pub home_content: BTreeMap<String, String>,
    /// Per-directory `_index.md` frontmatter (key = joined dir path).
    /// Used by the page-tree builder to label directory nodes.
    pub indices: BTreeMap<String, Vec<Value>>,
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
        let theme_name = if config.theme.is_empty() {
            "default".to_string()
        } else {
            config.theme.clone()
        };
        let (engine, engine_embedded) =
            load_engine(&config, &doc_root, &theme_name, template_override)?;

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
        // Categories are only the directories under `posts/`. Other top-level
        // sections (`pages/`, `notes/`, …) exit the category tree entirely;
        // they have their own navigation surface.
        let posts_paths: HashSet<Vec<String>> = all_paths
            .iter()
            .filter(|p| p.first().map(|s| s.as_str()) == Some("posts"))
            .cloned()
            .collect();
        let tree = build_tree(&posts_paths, &indices, &config, &default_lang);

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
                // The `layout:` frontmatter field was removed: every article's
                // role is now derived from its directory. Anything under
                // `posts/` is an article; everything else is a page.
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
            theme: theme_name.to_string(),
            tree,
            articles,
            home_content,
            indices,
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
            // Only blog posts surface on the home feed: pages (anything not
            // under `posts/`) belong in the pages tree, not the home stream.
            .filter(|a| {
                a.lang == lang
                    && !a.draft
                    && a.path.first().map(|s| s.as_str()) == Some("posts")
            })
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

    /// Build the navigation tree for non-post (page) content. Everything that
    /// isn't under `posts/` (and isn't the home `_index.md`) becomes a leaf or
    /// directory node, organised by its on-disk path. Intermediate directories
    /// are labelled from their `_index.md` frontmatter (`title`) when present
    /// and remain clickable: each directory renders a landing page that lists
    /// its direct children.
    pub fn pages_tree_value(&self, lang: &str, current_url: &str) -> Value {
        // 1. Filter pages for the current language: everything that isn't under
        //    `posts/` is a page.
        let pages: Vec<&Article> = self
            .articles
            .iter()
            .filter(|a| {
                a.lang == lang
                    && a.path.first().map(|s| s.as_str()) != Some("posts")
            })
            .collect();

        // 2. Collect every directory prefix that any page lives under.
        let mut all_dirs: std::collections::BTreeSet<Vec<String>> =
            std::collections::BTreeSet::new();
        for p in &pages {
            for pre in prefixes(&p.path) {
                all_dirs.insert(pre);
            }
        }

        // 3. Top-level entries are the longest path-segments that have length 1.
        let tops: Vec<Vec<String>> = all_dirs
            .iter()
            .filter(|p| p.len() == 1)
            .cloned()
            .collect();

        let mut out = Vec::new();
        for top in &tops {
            out.push(build_pages_node(
                top,
                &all_dirs,
                &pages,
                &self.indices,
                &self.config,
                &self.default_lang,
                lang,
                current_url,
            ));
        }
        Value::Arr(out)
    }

    /// Payload for a directory landing page (e.g. `/pages/docs/`). Looks up
    /// `_index.md` metadata in the requested language (falling back to the
    /// default language) and lists direct children: subdirectory links first,
    /// then sibling pages.
    pub fn page_section_value(&self, dir_path: &[String], lang: &str) -> Option<Value> {
        let key = dir_path.join("/");
        let entry = self.indices.get(&key)?;
        let entry = entry
            .iter()
            .find(|v| v.path("lang").and_then(|l| l.as_str()) == Some(lang))
            .or_else(|| {
                self.indices
                    .get(&key)
                    .and_then(|v| v.first())
            })?;
        let fm = entry
            .path("fields")
            .and_then(|f| f.as_map().cloned())
            .unwrap_or_default();
        let title = fm
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                dir_path
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "Section".to_string())
            });
        let summary = fm
            .get("summary")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let html = entry
            .path("html")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Children = subdirectories first (by name), then leaf pages (by title).
        let mut children: Vec<Value> = Vec::new();
        let key_for_match = dir_path.join("/");
        let subdirs: std::collections::BTreeSet<Vec<String>> = self
            .indices
            .keys()
            .filter_map(|k| {
                let parts = split_dir(k);
                if parts.len() == dir_path.len() + 1
                    && parts[..dir_path.len()] == dir_path[..]
                {
                    Some(parts)
                } else {
                    None
                }
            })
            .collect();
        for sub in &subdirs {
            let sub_key = sub.join("/");
            let sub_title = self
                .indices
                .get(&sub_key)
                .and_then(|items| {
                    items
                        .iter()
                        .find(|v| v.path("lang").and_then(|l| l.as_str()) == Some(lang))
                        .or_else(|| items.first())
                })
                .and_then(|it| it.path("fields"))
                .and_then(|f| f.as_map())
                .and_then(|m| m.get("title"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    sub.last().cloned().unwrap_or_else(|| "Section".to_string())
                });
            let url = url_for(&self.config, &self.default_lang, sub, None, lang);
            children.push(Value::Map(BTreeMap::from([
                ("title".to_string(), Value::str(&sub_title)),
                ("url".to_string(), Value::str(&url)),
                ("is_section".to_string(), Value::Bool(true)),
            ])));
        }
        // Also surface pages that live directly in this directory.
        let mut leafs: Vec<&Article> = self
            .articles
            .iter()
            .filter(|a| {
                a.lang == lang
                    && a.path == dir_path
                    && a.path.first().map(|s| s.as_str()) != Some("posts")
            })
            .collect();
        leafs.sort_by(|a, b| a.title.cmp(&b.title));
        for a in leafs {
            children.push(Value::Map(BTreeMap::from([
                ("title".to_string(), Value::str(&a.title)),
                ("url".to_string(), Value::str(&a.url)),
                ("is_section".to_string(), Value::Bool(false)),
            ])));
        }
        let _ = key_for_match; // silence unused warning if any

        let url = url_for(&self.config, &self.default_lang, dir_path, None, lang);
        Some(Value::Map(BTreeMap::from([
            ("title".to_string(), Value::str(&title)),
            ("url".to_string(), Value::str(&url)),
            ("summary".to_string(), opt_str(Some(&summary))),
            ("content".to_string(), opt_str(Some(&html))),
            ("children".to_string(), Value::Arr(children)),
        ])))
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
    _config: &Config,
    doc_root: &Path,
    theme_name: &str,
    override_dir: Option<PathBuf>,
) -> Result<(Engine, bool), String> {
    let mut engine = Engine::new();
    // Returns (whether the engine ended up with embedded defaults only).
    let embedded = if let Some(dir) = override_dir {
        // --template override: load embedded first so any partials the user
        // doesn't ship still resolve; user templates override on top.
        load_embedded(&mut engine)?;
        load_dir_templates(&mut engine, &dir)?;
        false
    } else {
        let theme_dir = doc_root.join("template").join(theme_name);
        if theme_dir.is_dir() {
            load_embedded(&mut engine)?;
            load_dir_templates(&mut engine, &theme_dir)?;
            false
        } else if theme_name == "default" {
            load_embedded(&mut engine)?;
            true
        } else {
            eprintln!(
                "warning: template/{theme_name} not found; using the built-in default"
            );
            load_embedded(&mut engine)?;
            true
        }
    };

    Ok((engine, embedded))
}

fn load_embedded(engine: &mut Engine) -> Result<(), String> {
    engine.add_many(vec![
        ("base.html", theme_files::BASE.to_string()),
        ("index.html", theme_files::INDEX.to_string()),
        ("category.html", theme_files::CATEGORY.to_string()),
        ("article.html", theme_files::ARTICLE.to_string()),
        ("page.html", theme_files::PAGE.to_string()),
        ("search.html", theme_files::SEARCH.to_string()),
        ("404.html", theme_files::NOT_FOUND.to_string()),
        ("layout/header.html", theme_files::PARTIAL_HEADER.to_string()),
        ("layout/footer.html", theme_files::PARTIAL_FOOTER.to_string()),
        ("layout/side.html", theme_files::PARTIAL_SIDE.to_string()),
        ("layout/inject.html", theme_files::PARTIAL_INJECT.to_string()),
        ("layout/_cat_node.html", theme_files::PARTIAL_CAT_NODE.to_string()),
        ("layout/_nav_node.html", theme_files::PARTIAL_NAV_NODE.to_string()),
        ("page_section.html", theme_files::PAGE_SECTION.to_string()),
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
            || name == "template"
            || name == "static"
            || name == "samples"
        {
            continue;
        }
        if p.is_dir() {
            if name == "content" {
                // content/ is a transparent container: recurse into it but
                // strip the prefix so URLs / categories are not prefixed with
                // `content/`. content/ itself is not a category.
                walk_content(&p, base, out);
            } else {
                walk_doc(&p, base, out);
            }
        } else if let Ok(rel) = p.strip_prefix(base) {
            out.push((rel.to_string_lossy().replace('\\', "/"), p));
        }
    }
}

/// Walk inside the `content/` container. Strips the leading `content/` from
/// every emitted path so discovery is otherwise identical to `walk_doc`.
///
/// At the top level of `content/` we accept:
///
/// - `_index.md` / `_index.<lang>.md` — the home page variants
/// - any other `*.md` file — these become top-level pages (e.g.
///   `content/about.md` → `/about/`). Directory sections like `posts/`,
///   `pages/`, `notes/` are still the recommended way to organise content,
///   but root-level pages are useful for one-off pages like About.
fn walk_content(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            walk_content(&p, base, out);
        } else if let Ok(rel) = p.strip_prefix(base) {
            let stripped = rel
                .to_string_lossy()
                .replace('\\', "/")
                .strip_prefix("content/")
                .unwrap_or(&rel.to_string_lossy())
                .to_string();
            out.push((stripped, p));
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

/// Recursive helper that builds a single node of the pages navigation tree.
fn build_pages_node(
    path: &[String],
    all_dirs: &std::collections::BTreeSet<Vec<String>>,
    pages: &[&Article],
    indices: &BTreeMap<String, Vec<Value>>,
    config: &Config,
    default_lang: &str,
    lang: &str,
    current_url: &str,
) -> Value {
    // Title: prefer `_index.md` frontmatter for this dir in the requested
    // language, falling back to the directory slug.
    let key = path.join("/");
    let title = indices
        .get(&key)
        .and_then(|items| {
            items
                .iter()
                .find(|v| v.path("lang").and_then(|l| l.as_str()) == Some(lang))
                .or_else(|| items.first())
        })
        .and_then(|it| it.path("fields"))
        .and_then(|f| f.as_map())
        .and_then(|m| m.get("title"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.last().cloned().unwrap_or_default());

    let url = url_for(config, default_lang, path, None, lang);

    // Children: subdirectories (sorted by name) + leaf articles at this dir.
    let mut children_vals: Vec<Value> = Vec::new();
    let sub_dirs: Vec<Vec<String>> = all_dirs
        .iter()
        .filter(|k| k.len() == path.len() + 1 && k[..path.len()] == path[..])
        .cloned()
        .collect();
    for sub in &sub_dirs {
        children_vals.push(build_pages_node(
            sub,
            all_dirs,
            pages,
            indices,
            config,
            default_lang,
            lang,
            current_url,
        ));
    }
    let mut leaves: Vec<&&Article> = pages
        .iter()
        .filter(|p| p.path == path)
        .collect();
    leaves.sort_by(|a, b| a.title.cmp(&b.title));
    for art in leaves {
        children_vals.push(Value::Map(BTreeMap::from([
            ("title".to_string(), Value::str(&art.title)),
            ("url".to_string(), Value::str(&art.url)),
            ("active".to_string(), Value::Bool(art.url == current_url)),
            ("descendant_active".to_string(), Value::Bool(art.url == current_url)),
            ("has_children".to_string(), Value::Bool(false)),
            ("children".to_string(), Value::Arr(vec![])),
            ("is_section".to_string(), Value::Bool(false)),
        ])));
    }

    let active = url == current_url;
    let descendant_active = active || {
        let mut found = false;
        for child in &children_vals {
            if let Value::Map(m) = child {
                if matches!(m.get("active"), Some(Value::Bool(true)))
                    || matches!(m.get("descendant_active"), Some(Value::Bool(true)))
                {
                    found = true;
                    break;
                }
            }
        }
        found
    };

    Value::Map(BTreeMap::from([
        ("title".to_string(), Value::str(&title)),
        ("url".to_string(), Value::str(&url)),
        ("active".to_string(), Value::Bool(active)),
        ("descendant_active".to_string(), Value::Bool(descendant_active)),
        ("has_children".to_string(), Value::Bool(!children_vals.is_empty())),
        ("children".to_string(), Value::Arr(children_vals)),
        ("is_section".to_string(), Value::Bool(true)),
    ]))
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

