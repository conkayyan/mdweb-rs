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
    ("reading_time_seconds", "sec read"),
    ("prev", "Previous"),
    ("next", "Next"),
    ("prev_page", "< Previous"),
    ("next_page", "Next >"),
    ("not_found", "Not Found"),
    ("not_found_desc", "The page you're looking for doesn't exist."),
    ("back_home", "Back home"),
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

/// Route rules for well-known, non-content URL paths. Each field is the URL
/// slug (path segment or file name) for a built-in route; configuring one
/// renames the route everywhere — server matching, generated links, feeds and
/// the template `routes` map. Defaults preserve the classic mdweb layout.
#[derive(Debug, Clone)]
pub struct Routes {
    /// Search page: `/search` (or `/<lang>/search`). Default `"search"`.
    pub search: String,
    /// Tag index and tag listings: `/tags/` and `/tags/<tag>/`. Default
    /// `"tags"`.
    pub tags: String,
    /// Per-language RSS feed: `/rss.xml` (or `/<lang>/rss.xml`). Default
    /// `"rss.xml"`.
    pub rss: String,
    /// Site-wide XML sitemap at the site root: `/sitemap.xml`. Default
    /// `"sitemap.xml"`.
    pub sitemap: String,
    /// Client-side search index at the site root: `/search.json`. Default
    /// `"search.json"`.
    pub search_index: String,
    /// Static assets served from `template/<theme>/static/`: `/static/…`.
    /// Default `"static"`.
    pub static_dir: String,
    /// URL prefix for the blog container `content/posts/`: `/posts/…`.
    /// Content still lives on disk under `content/posts/`; only the URL
    /// prefix changes. Default `"posts"`.
    pub posts: String,
    /// URL prefix for the pages container `content/pages/`: `/pages/…`.
    /// Content still lives on disk under `content/pages/`; only the URL
    /// prefix changes. Default `"pages"`.
    pub pages: String,
}

impl Default for Routes {
    fn default() -> Self {
        Routes {
            search: "search".into(),
            tags: "tags".into(),
            rss: "rss.xml".into(),
            sitemap: "sitemap.xml".into(),
            search_index: "search.json".into(),
            static_dir: "static".into(),
            posts: "posts".into(),
            pages: "pages".into(),
        }
    }
}

impl Routes {
    /// Translate an on-disk content container segment (`posts` / `pages`)
    /// into its configured URL prefix. Any other segment passes through
    /// unchanged, so non-container paths (root pages, `notes/`, …) are never
    /// touched.
    pub fn prefix_url(&self, seg: &str) -> String {
        match seg {
            "posts" => self.posts.clone(),
            "pages" => self.pages.clone(),
            other => other.to_string(),
        }
    }

    /// Inverse of `prefix_url`: map a URL prefix back to the on-disk content
    /// container directory name.
    pub fn prefix_disk(&self, seg: &str) -> String {
        if seg == self.posts {
            "posts".to_string()
        } else if seg == self.pages {
            "pages".to_string()
        } else {
            seg.to_string()
        }
    }
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
    /// Traffic analytics providers. A provider block is enabled whenever its
    /// `id` is set; the rendered snippet is injected into the page `<head>`
    /// before the user-editable `layout/inject.html` content.
    pub analytics: AnalyticsConfig,
    /// Route rules for well-known, non-content URLs.
    pub routes: Routes,
    /// Response header hardening.
    pub security: SecurityConfig,
    pub extra: Value,
}

/// Analytics providers parsed from the optional `[analytics.*]` tables.
/// Each provider is independently optional; setting `id` enables its snippet.
#[derive(Debug, Clone, Default)]
pub struct AnalyticsConfig {
    /// Google Analytics 4 (`gtag.js`) — measurement id like `G-XXXXXXX`.
    pub google: Option<AnalyticsProvider>,
    /// Baidu Tongji — site id like the long hash from the bm.supported panel.
    pub baidu: Option<AnalyticsProvider>,
}

/// Default Content-Security-Policy. Balanced to keep the built-in themes and
/// the injected analytics snippets working while still blocking framing,
/// mixed http: resources, form-targeting and non-self script loading beyond
/// https: origins. `'unsafe-inline'` is unavoidable because markdown and the
/// theme partials are raw HTML by design.
const DEFAULT_CSP: &str = "default-src 'self'; base-uri 'self'; form-action 'self'; \
frame-ancestors 'self'; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'; \
script-src 'self' 'unsafe-inline' https:; connect-src 'self' https:; font-src 'self' data:";

/// Response header hardening, configured under the optional `[security]`
/// table in site.toml. `X-Content-Type-Options`, `X-Frame-Options` and
/// `Referrer-Policy` are always sent unless `enabled = false`.
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Master switch for the extra response headers (defaults to `true`).
    pub enabled: bool,
    /// Content-Security-Policy override. `None` uses `DEFAULT_CSP`; an empty
    /// string omits the header entirely.
    pub csp: Option<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        SecurityConfig {
            enabled: true,
            csp: None,
        }
    }
}

impl SecurityConfig {
    /// The CSP value to send, or an empty string to omit the header.
    pub fn csp_header(&self) -> String {
        if !self.enabled {
            return String::new();
        }
        self.csp.clone().unwrap_or_else(|| DEFAULT_CSP.to_string())
    }
}

/// A single analytics provider entry. Currently the only field is the
/// service-specific identifier; provider-specific extras (e.g. self-hosted
/// script URLs) can be added here without breaking existing configs.
#[derive(Debug, Clone)]
pub struct AnalyticsProvider {
    pub id: String,
}

impl AnalyticsProvider {
    fn from_value(v: &Value) -> Option<Self> {
        let m = v.as_map()?;
        let id = m.get("id").and_then(|x| x.as_str())?.trim().to_string();
        if id.is_empty() {
            return None;
        }
        Some(AnalyticsProvider { id })
    }
}

impl AnalyticsConfig {
    /// `true` when at least one provider block has a non-empty `id`. The
    /// template pipeline uses this to skip the analytics `<script>` block
    /// entirely when no provider is configured.
    pub fn is_enabled(&self) -> bool {
        self.google.is_some() || self.baidu.is_some()
    }

    /// Render the analytics `<script>` blocks for every enabled provider.
    /// Returns an empty string when no provider is configured so callers can
    /// splice the result into the `inject` slot unconditionally.
    pub fn snippets(&self) -> String {
        let mut out = String::new();
        if let Some(g) = &self.google {
            out.push_str(&google_snippet(&g.id));
        }
        if let Some(b) = &self.baidu {
            out.push_str(&baidu_snippet(&b.id));
        }
        out
    }
}

/// Google Analytics 4 snippet. `id` is the measurement id (e.g. `G-XXXXXXX`).
/// The async-loaded `gtag.js` pattern is Google's current recommendation.
fn google_snippet(id: &str) -> String {
    // The id is interpolated into an HTML attribute (`src=...?id=`) *and* a
    // JS string literal that the browser will parse as JS. We escape for the
    // HTML attribute (so `</script>` can't close the tag) and for a JS string
    // literal separately (`js_string_escape` handles backslash, quote, `< > &`
    // and newlines — a plain HTML escape would let a `'` or backslash break
    // out of the JS string).
    let attr_safe = html_attr_escape(id);
    let js_safe = js_string_escape(id);
    format!(
        "<!-- Google Analytics (mdweb) -->\n\
<script async src=\"https://www.googletagmanager.com/gtag/js?id={attr_safe}\"></script>\n\
<script>\n\
  window.dataLayer = window.dataLayer || [];\n\
  function gtag(){{dataLayer.push(arguments);}}\n\
  gtag('js', new Date());\n\
  gtag('config', \"{js_safe}\");\n\
</script>\n"
    )
}

/// Baidu Tongji snippet. `id` is the long hash from the bm.supported panel.
fn baidu_snippet(id: &str) -> String {
    let safe = js_string_escape(id);
    format!(
        "<!-- Baidu Tongji (mdweb) -->\n\
<script>\n\
  var _hmt = _hmt || [];\n\
  (function() {{\n\
    var hm = document.createElement(\"script\");\n\
    hm.src = \"https://hm.baidu.com/hm.js?{safe}\";\n\
    var s = document.getElementsByTagName(\"script\")[0];\n\
    s.parentNode.insertBefore(hm, s);\n\
  }})();\n\
</script>\n"
    )
}

/// Escape a string for safe use inside an HTML attribute value (double-quoted).
fn html_attr_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape a string for safe use inside a JS double-quoted string literal.
fn js_string_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            _ => out.push(c),
        }
    }
    out
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
            analytics: AnalyticsConfig::default(),
            routes: Routes::default(),
            security: SecurityConfig::default(),
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
        if let Some(am) = m.get("analytics").and_then(|v| v.as_map()) {
            cfg.analytics.google = am
                .get("google")
                .and_then(AnalyticsProvider::from_value);
            cfg.analytics.baidu = am
                .get("baidu")
                .and_then(AnalyticsProvider::from_value);
        }
        if let Some(sm) = m.get("security").and_then(|v| v.as_map()) {
            if let Some(b) = sm.get("enabled").and_then(|v| v.as_bool()) {
                cfg.security.enabled = b;
            }
            if let Some(s) = sm.get("csp").and_then(|v| v.as_str()) {
                cfg.security.csp = Some(s.to_string());
            }
        }
        if let Some(rm) = m.get("routes").and_then(|v| v.as_map()) {
            // Each value is a URL slug; trim stray whitespace and slashes so
            // `"/search/"` behaves identically to `"search"`.
            let slug = |k: &str| {
                rm.get(k)
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().trim_matches('/').to_string())
                    .filter(|s| !s.is_empty())
            };
            if let Some(s) = slug("search") {
                cfg.routes.search = s;
            }
            if let Some(s) = slug("tags") {
                cfg.routes.tags = s;
            }
            if let Some(s) = slug("rss") {
                cfg.routes.rss = s;
            }
            if let Some(s) = slug("sitemap") {
                cfg.routes.sitemap = s;
            }
            if let Some(s) = slug("search_index") {
                cfg.routes.search_index = s;
            }
            if let Some(s) = slug("static") {
                cfg.routes.static_dir = s;
            }
            if let Some(s) = slug("posts") {
                cfg.routes.posts = s;
            }
            if let Some(s) = slug("pages") {
                cfg.routes.pages = s;
            }
        }
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
    /// language, `/<code>/tags/` otherwise. Follows the configured `routes.tags`.
    pub fn tag_index_url(&self, lang: &str) -> String {
        format!("{}{}/", self.lang_prefix(lang), self.routes.tags)
    }

    /// URL for a single tag listing page in a language.
    pub fn tag_url(&self, lang: &str, name: &str) -> String {
        format!(
            "{}{}/{}/",
            self.lang_prefix(lang),
            self.routes.tags,
            crate::content::percent_encode(name)
        )
    }

    /// URL of the search page for a language: `/search` for the default
    /// language, `/<code>/search` otherwise.
    pub fn search_url(&self, lang: &str) -> String {
        format!("{}{}", self.lang_prefix(lang), self.routes.search)
    }

    /// URL of the per-language RSS feed: `/rss.xml` for the default language,
    /// `/<code>/rss.xml` otherwise.
    pub fn rss_url(&self, lang: &str) -> String {
        format!("{}{}", self.lang_prefix(lang), self.routes.rss)
    }

    /// URL of the site-wide XML sitemap. Serves every language, so it lives at
    /// the root regardless of the current language.
    pub fn sitemap_url(&self) -> String {
        format!("/{}", self.routes.sitemap)
    }

    /// URL of the client-side search index. Spans all languages.
    pub fn search_index_url(&self) -> String {
        format!("/{}", self.routes.search_index)
    }

    /// URL prefix of the theme's static directory, e.g. `/static/`.
    pub fn static_url(&self) -> String {
        format!("/{}/", self.routes.static_dir)
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

    #[test]
    fn analytics_off_when_section_missing() {
        let cfg = parse(r#"title = "X""#);
        assert!(!cfg.analytics.is_enabled());
        assert_eq!(cfg.analytics.snippets(), "");
    }

    #[test]
    fn analytics_google_parsed() {
        let cfg = parse(
            r#"
[analytics.google]
id = "G-ABC123"
"#,
        );
        assert!(cfg.analytics.is_enabled());
        let g = cfg.analytics.google.clone().expect("google present");
        assert_eq!(g.id, "G-ABC123");
        let s = cfg.analytics.snippets();
        assert!(s.contains("G-ABC123"), "snippet embeds id: {s}");
        assert!(s.contains("googletagmanager.com/gtag/js"), "uses gtag.js");
    }

    #[test]
    fn analytics_baidu_parsed() {
        let cfg = parse(
            r#"
[analytics.baidu]
id = "e4d2c4f3a1b2c3d4e5f6a7b8c9d0e1f2"
"#,
        );
        assert!(cfg.analytics.is_enabled());
        assert!(cfg.analytics.google.is_none());
        let b = cfg.analytics.baidu.clone().expect("baidu present");
        assert_eq!(b.id, "e4d2c4f3a1b2c3d4e5f6a7b8c9d0e1f2");
        let s = cfg.analytics.snippets();
        assert!(s.contains("hm.baidu.com/hm.js?e4d2c4f3a1b2c3d4e5f6a7b8c9d0e1f2"));
    }

    #[test]
    fn analytics_both_providers() {
        let cfg = parse(
            r#"
[analytics.google]
id = "G-1"

[analytics.baidu]
id = "B-1"
"#,
        );
        let s = cfg.analytics.snippets();
        assert!(s.contains("googletagmanager"));
        assert!(s.contains("hm.baidu.com"));
    }

    #[test]
    fn analytics_empty_id_is_ignored() {
        let cfg = parse(
            r#"
[analytics.google]
id = ""
"#,
        );
        assert!(!cfg.analytics.is_enabled());
        assert!(cfg.analytics.google.is_none());
    }

    #[test]
    fn analytics_id_with_special_chars_is_escaped() {
        let cfg = parse(
            r#"
[analytics.google]
id = "</script><script>alert(1)</script>"
"#,
        );
        let s = cfg.analytics.snippets();
        assert!(
            !s.contains("</script><script>alert(1)</script>"),
            "raw id must not leak into HTML attribute: {s}"
        );
        assert!(s.contains("&lt;/script&gt;"));
    }

    #[test]
    fn analytics_id_cannot_break_out_of_js_string() {
        // A single quote or backslash must not close the surrounding JS
        // string literal in `gtag('config', "…)");`.
        let cfg = parse(
            r#"
[analytics.google]
id = "G-1\\\");alert(1);//"
"#,
        );
        let s = cfg.analytics.snippets();
        assert!(
            !s.contains("G-1\");alert(1);//"),
            "raw quote must be escaped inside the JS string: {s}"
        );
        assert!(s.contains("\\\""), "double quote must be backslash-escaped: {s}");
        assert_eq!(s.matches("\";alert(1)").count(), 0, "no breakout: {s}");
    }

    #[test]
    fn routes_default_to_classic_layout() {
        let cfg = parse(r#"title = "X""#);
        assert_eq!(cfg.routes.search, "search");
        assert_eq!(cfg.routes.tags, "tags");
        assert_eq!(cfg.routes.rss, "rss.xml");
        assert_eq!(cfg.routes.sitemap, "sitemap.xml");
        assert_eq!(cfg.routes.search_index, "search.json");
        assert_eq!(cfg.routes.static_dir, "static");
        assert_eq!(cfg.routes.posts, "posts");
        assert_eq!(cfg.routes.pages, "pages");
        assert_eq!(cfg.search_url("en"), "/search");
        assert_eq!(cfg.search_url("zh"), "/zh/search");
        assert_eq!(cfg.rss_url("en"), "/rss.xml");
        assert_eq!(cfg.rss_url("zh"), "/zh/rss.xml");
        assert_eq!(cfg.sitemap_url(), "/sitemap.xml");
        assert_eq!(cfg.search_index_url(), "/search.json");
        assert_eq!(cfg.static_url(), "/static/");
        assert_eq!(cfg.tag_index_url("en"), "/tags/");
        assert_eq!(cfg.tag_url("zh", "rust"), "/zh/tags/rust/");
    }

    #[test]
    fn routes_can_be_customised() {
        let cfg = parse(
            r#"
languages = ["en", "zh"]

[routes]
search = "find"
tags = "topics"
rss = "feed.xml"
sitemap = "site.xml"
search_index = "lookup.json"
static = "assets"
posts = "blog"
pages = "docs"
"#,
        );
        assert_eq!(cfg.routes.search, "find");
        assert_eq!(cfg.routes.tags, "topics");
        assert_eq!(cfg.routes.rss, "feed.xml");
        assert_eq!(cfg.routes.sitemap, "site.xml");
        assert_eq!(cfg.routes.search_index, "lookup.json");
        assert_eq!(cfg.routes.static_dir, "assets");
        assert_eq!(cfg.routes.posts, "blog");
        assert_eq!(cfg.routes.pages, "docs");
        // Content container translation both directions.
        assert_eq!(cfg.routes.prefix_url("posts"), "blog");
        assert_eq!(cfg.routes.prefix_url("pages"), "docs");
        assert_eq!(cfg.routes.prefix_url("notes"), "notes");
        assert_eq!(cfg.routes.prefix_disk("blog"), "posts");
        assert_eq!(cfg.routes.prefix_disk("docs"), "pages");
        assert_eq!(cfg.routes.prefix_disk("notes"), "notes");
        assert_eq!(cfg.search_url("en"), "/find");
        assert_eq!(cfg.search_url("zh"), "/zh/find");
        assert_eq!(cfg.rss_url("en"), "/feed.xml");
        assert_eq!(cfg.rss_url("zh"), "/zh/feed.xml");
        assert_eq!(cfg.sitemap_url(), "/site.xml");
        assert_eq!(cfg.search_index_url(), "/lookup.json");
        assert_eq!(cfg.static_url(), "/assets/");
        assert_eq!(cfg.tag_index_url("en"), "/topics/");
        assert_eq!(cfg.tag_url("zh", "a b"), "/zh/topics/a%20b/");
    }

    #[test]
    fn routes_ignore_empty_and_slash_padded_values() {
        let cfg = parse(
            r#"
[routes]
search = ""
tags = "/topics/"
rss = "   "
sitemap = "//site.xml"
"#,
        );
        // Empty values fall back to the default; slashes are trimmed.
        assert_eq!(cfg.routes.search, "search");
        assert_eq!(cfg.routes.tags, "topics");
        assert_eq!(cfg.routes.rss, "rss.xml");
        assert_eq!(cfg.routes.sitemap, "site.xml");
        assert_eq!(cfg.tag_index_url("en"), "/topics/");
    }

    #[test]
    fn security_defaults_to_default_csp() {
        let cfg = parse(r#"title = "X""#);
        assert!(cfg.security.enabled);
        let csp = cfg.security.csp_header();
        assert!(csp.contains("default-src 'self'"), "{csp}");
        assert!(csp.contains("frame-ancestors 'self'"), "{csp}");
        assert!(!csp.is_empty());
    }

    #[test]
    fn security_csp_can_be_overridden() {
        let cfg = parse(
            r#"
[security]
csp = "default-src 'none'"
"#,
        );
        assert_eq!(cfg.security.csp_header(), "default-src 'none'");
    }

    #[test]
    fn security_headers_can_be_disabled() {
        let cfg = parse(
            r#"
[security]
enabled = false
"#,
        );
        assert!(!cfg.security.enabled);
        assert_eq!(cfg.security.csp_header(), "");
    }
}