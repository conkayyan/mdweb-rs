use std::collections::BTreeMap;
use std::path::Path;

use crate::parse::parse_toml;
use crate::value::Value;

/// Per-language metadata from `[lang.<code>]`.
#[derive(Debug, Clone, Default)]
pub struct LangMeta {
    pub title: Option<String>,
    pub description: Option<String>,
    pub keywords: Option<String>,
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
    pub extra: Value,
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
}