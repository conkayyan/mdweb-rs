//! Shared HTML escaping helpers.
//!
//! Single source of truth for the `& < > "` escaping used across the
//! markdown renderer, template engine, math/diagram SVG emitters and the
//! analytics snippets. XML and JavaScript-string escaping are distinct
//! output contexts and live next to their consumers.

/// Escape `&`, `<` and `>` for safe inclusion in HTML text.
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape `&`, `<`, `>` and `"` — safe for double-quoted HTML attribute
/// values (also fine for HTML text).
pub fn escape_attr(s: &str) -> String {
    escape(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_covers_text_specials() {
        assert_eq!(escape("a&b<c>d"), "a&amp;b&lt;c&gt;d");
        assert_eq!(escape("no change"), "no change");
    }

    #[test]
    fn escape_attr_also_quotes() {
        assert_eq!(escape_attr("a\"b"), "a&quot;b");
    }
}