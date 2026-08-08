use std::collections::BTreeMap;
use std::path::Path;

use crate::parse::parse_toml;
use crate::value::Value;

/// Per-language metadata from `[lang.<code>]`.
#[derive(Debug, Clone, Default)]
pub struct LangMeta {
    pub title: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub keywords: Option<String>,
}

/// i18n keys and their English fallbacks. Single source of truth used both
/// for the lookup in `Config::t` and for the loop that builds the `t.*` map
/// exposed to templates.
pub(crate) const I18N_DEFAULTS: &[(&str, &str)] = &[
    ("home", "Home"),
    ("breadcrumb_home", "Index"),
    ("categories", "Categories"),
    ("pages", "Pages"),
    ("subpages", "Pages in this section"),
    ("subcategories", "Subcategories"),
    ("recent_posts", "Recent Posts"),
    ("tags", "Tags"),
    ("tag_list", "Posts tagged"),
    ("friend_links", "Friend Links"),
    ("no_posts", "No posts yet."),
    ("read_in", "Read in:"),
    ("published", "Published:"),
    ("updated", "Updated:"),
    ("author", "Author:"),
    ("reading_time", "min read"),
    ("prev", "Previous"),
    ("next", "Next"),
    ("prev_page", "< Previous"),
    ("next_page", "Next >"),
    ("not_found", "Not Found"),
    ("not_found_desc", "The page you're looking for doesn't exist."),
    ("search", "Search"),
    ("search_placeholder", "Search…"),
    ("search_no_results", "No matching posts."),
    ("rss", "RSS Feed"),
    ("sitemap", "Sitemap"),
];

fn builtin_default(key: &str) -> Option<&'static str> {
    I18N_DEFAULTS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

/// Site configuration from `site.toml`.
#[derive(Debug, Clone)]
pub struct Config {
    pub title: String,
    pub base_url: String,
    pub author: String,
    pub language: String,
    pub languages: Vec<String>,
    pub theme: String,
    pub params: Value,
    pub meta: Value,
    pub langs: BTreeMap<String, LangMeta>,
    pub i18n: BTreeMap<String, BTreeMap<String, String>>,
    /// Friend links parsed from `[[friend_links]]` tables in site.toml.
    pub friend_links: Vec<FriendLink>,
    /// Whether to surface the RSS feed link in templates. Default `true`.
    pub show_rss: bool,
    /// Whether to surface the sitemap link in templates. Default `true`.
    pub show_sitemap: bool,
    /// Articles shown per page on the home feed. `0` disables pagination
    /// (every article on one page). Default `10`.
    pub home_limit: usize,
    /// Articles shown per page in a category listing. Default `20`.
    pub category_limit: usize,
    /// Pages shown per page in a directory landing listing. Default `50`.
    pub pages_limit: usize,
    /// Whether to show the tag cloud widget in the sidebar. Default `true`.
    pub show_tag_cloud: bool,
    /// Articles shown per page on a tag listing page. `0` disables
    /// pagination (every match on one page). Default `20`.
    pub tags_limit: usize,
    /// Maximum number of tags shown in the sidebar tag cloud. `0` shows every
    /// tag. Default `0`.
    pub tag_cloud_limit: usize,
    pub extra: Value,
}

/// One entry under `[[friend_links]]`.
#[derive(Debug, Clone)]
pub struct FriendLink {
    pub name: String,
    pub url: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            title: "My Site".into(),
            base_url: String::new(),
            author: "Unknown".into(),
            language: "en".into(),
            languages: Vec::new(),
            theme: "default".into(),
            params: Value::map(),
            meta: Value::map(),
            langs: BTreeMap::new(),
            i18n: BTreeMap::new(),
            friend_links: Vec::new(),
            show_rss: true,
            show_sitemap: true,
            home_limit: 10,
            category_limit: 20,
            pages_limit: 50,
            show_tag_cloud: true,
            tags_limit: 20,
            tag_cloud_limit: 0,
            extra: Value::map(),
        }
    }
}

impl Config {
    pub fn load(doc_root: &Path) -> Config {
        let path = doc_root.join("site.toml");
        if !path.exists() {
            return Config::default();
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("warning: cannot read {}: {e}", path.display());
                return Config::default();
            }
        };
        match parse_toml(&text) {
            Ok(root) => Config::from_value(&root),
            Err(e) => {
                eprintln!("warning: {}: {e}", path.display());
                Config::default()
            }
        }
    }

    fn from_value(v: &Value) -> Config {
        let m = v.as_map().cloned().unwrap_or_default();
        let get_str = |k: &str| m.get(k).and_then(|v| v.as_str()).map(|s| s.to_string());
        let mut cfg = Config::default();
        if let Some(s) = get_str("title") {
            cfg.title = s;
        }
        if let Some(s) = get_str("base_url") {
            cfg.base_url = s;
        }
        if let Some(s) = get_str("author") {
            cfg.author = s;
        }
        if let Some(s) = get_str("language") {
            cfg.language = s;
        }
        if let Some(s) = get_str("theme") {
            cfg.theme = s;
        }
        if let Some(arr) = m.get("languages").and_then(|v| v.as_arr()) {
            cfg.languages = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        cfg.params = m.get("params").cloned().unwrap_or_else(Value::map);
        cfg.meta = m.get("meta").cloned().unwrap_or_else(Value::map);
        if let Some(b) = m.get("show_rss").and_then(|v| v.as_bool()) {
            cfg.show_rss = b;
        }
        if let Some(b) = m.get("show_sitemap").and_then(|v| v.as_bool()) {
            cfg.show_sitemap = b;
        }
        if let Some(n) = m.get("home_limit").and_then(|v| v.as_int()) {
            if n >= 0 {
                cfg.home_limit = n as usize;
            }
        }
        if let Some(n) = m.get("category_limit").and_then(|v| v.as_int()) {
            if n >= 0 {
                cfg.category_limit = n as usize;
            }
        }
        if let Some(n) = m.get("pages_limit").and_then(|v| v.as_int()) {
            if n >= 0 {
                cfg.pages_limit = n as usize;
            }
        }
        if let Some(b) = m.get("show_tag_cloud").and_then(|v| v.as_bool()) {
            cfg.show_tag_cloud = b;
        }
        if let Some(n) = m.get("tags_limit").and_then(|v| v.as_int()) {
            if n >= 0 {
                cfg.tags_limit = n as usize;
            }
        }
        if let Some(n) = m.get("tag_cloud_limit").and_then(|v| v.as_int()) {
            if n >= 0 {
                cfg.tag_cloud_limit = n as usize;
            }
        }
        if let Some(Value::Arr(items)) = m.get("friend_links") {
            for item in items {
                if let Value::Map(entry) = item {
                    let name = entry
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let url = entry
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !name.is_empty() && !url.is_empty() {
                        cfg.friend_links.push(FriendLink { name, url });
                    }
                }
            }
        }
        cfg.extra = if let Some(Value::Map(extra)) = m.get("extra") {
            Value::Map(extra.clone())
        } else {
            Value::map()
        };
        if let Some(lang_map) = m.get("lang").and_then(|v| v.as_map()) {
            for (code, meta) in lang_map {
                let mm = meta.as_map().cloned().unwrap_or_default();
                cfg.langs.insert(
                    code.clone(),
                    LangMeta {
                        title: mm.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        display_name: mm
                            .get("display_name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        description: mm
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        keywords: mm
                            .get("keywords")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    },
                );
            }
        }
        if let Some(i18n_map) = m.get("i18n").and_then(|v| v.as_map()) {
            for (lang, body) in i18n_map {
                let Some(bm) = body.as_map() else { continue };
                let mut table = BTreeMap::new();
                for (k, v) in bm {
                    if let Some(s) = v.as_str() {
                        table.insert(k.clone(), s.to_string());
                    }
                }
                cfg.i18n.insert(lang.clone(), table);
            }
        }
        cfg
    }

    /// Resolved ordered language list.
    pub fn resolved_languages(&self) -> Vec<String> {
        let mut out = if self.languages.is_empty() {
            vec![self.language.clone()]
        } else {
            self.languages.clone()
        };
        for l in &out.clone() {
            if !out.contains(l) {
                out.push(l.clone());
            }
        }
        out
    }

    pub fn title_for(&self, lang: &str) -> String {
        self.langs
            .get(lang)
            .and_then(|m| m.title.clone())
            .unwrap_or_else(|| self.title.clone())
    }

    pub fn description_for(&self, lang: &str) -> String {
        if let Some(d) = self.langs.get(lang).and_then(|m| m.description.clone()) {
            return d;
        }
        if let Some(Value::Str(s)) = self.meta.as_map().and_then(|m| m.get("description")) {
            return s.clone();
        }
        String::new()
    }

    pub fn keywords_for(&self, lang: &str) -> String {
        if let Some(k) = self.langs.get(lang).and_then(|m| m.keywords.clone()) {
            return k;
        }
        if let Some(Value::Str(s)) = self.meta.as_map().and_then(|m| m.get("keywords")) {
            return s.clone();
        }
        String::new()
    }

    /// URL prefix for a language: "/" for default, "/<code>/" otherwise.
    pub fn lang_prefix(&self, lang: &str) -> String {
        if lang == self.language {
            "/".to_string()
        } else {
            format!("/{lang}/")
        }
    }

    /// Display name for a language code. Falls back to the raw code when unset.
    pub fn display_name_for(&self, lang: &str) -> String {
        self.langs
            .get(lang)
            .and_then(|m| m.display_name.clone())
            .unwrap_or_else(|| lang.to_string())
    }

    /// URL prefix for a language's tag listings: `/tags/` for the default
    /// language, `/<code>/tags/` otherwise.
    pub fn tag_index_url(&self, lang: &str) -> String {
        format!("{}tags/", self.lang_prefix(lang))
    }

    /// URL for a single tag listing page in a language.
    pub fn tag_url(&self, lang: &str, name: &str) -> String {
        format!(
            "{}tags/{}/",
            self.lang_prefix(lang),
            crate::content::percent_encode(name)
        )
    }

    /// Resolve a UI string: current lang → English → built-in default → key itself.
    pub fn t(&self, key: &str, lang: &str) -> String {
        if let Some(s) = self.i18n.get(lang).and_then(|t| t.get(key)) {
            return s.clone();
        }
        if lang != "en" {
            if let Some(s) = self.i18n.get("en").and_then(|t| t.get(key)) {
                return s.clone();
            }
        }
        if let Some(default) = builtin_default(key) {
            return default.to_string();
        }
        key.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Config {
        let v = crate::parse::parse_toml(text).expect("parse");
        Config::from_value(&v)
    }

    #[test]
    fn display_name_from_lang_meta() {
        let cfg = parse(
            r#"
            languages = ["en", "zh"]

            [lang.en]
            display_name = "English"

            [lang.zh]
            display_name = "简体中文"
        "#,
        );
        assert_eq!(cfg.display_name_for("en"), "English");
        assert_eq!(cfg.display_name_for("zh"), "简体中文");
    }

    #[test]
    fn display_name_falls_back_to_code() {
        let cfg = parse(r#"languages = ["en", "fr"]"#);
        assert_eq!(cfg.display_name_for("en"), "en");
        assert_eq!(cfg.display_name_for("fr"), "fr");
    }

    #[test]
    fn t_resolves_current_lang() {
        let cfg = parse(
            r#"
            languages = ["en", "zh"]

            [i18n.en]
            categories = "Categories"

            [i18n.zh]
            categories = "分类"
        "#,
        );
        assert_eq!(cfg.t("categories", "zh"), "分类");
        assert_eq!(cfg.t("categories", "en"), "Categories");
    }

    #[test]
    fn t_falls_back_to_english() {
        let cfg = parse(
            r#"
            languages = ["en", "fr"]

            [i18n.en]
            categories = "Categories"
        "#,
        );
        assert_eq!(cfg.t("categories", "fr"), "Categories");
    }

    #[test]
    fn t_uses_builtin_default_when_no_english() {
        let cfg = parse(r#"languages = ["fr"]"#);
        // "home" has a built-in default; falls through English (unset) to the default.
        assert_eq!(cfg.t("home", "fr"), "Home");
    }

    #[test]
    fn t_returns_key_for_unknown_when_no_default() {
        let cfg = parse(r#"languages = ["en"]"#);
        assert_eq!(cfg.t("totally_custom_key", "en"), "totally_custom_key");
    }
}