//! The `_image/` convention: images live next to the documents that use them,
//! in an `_image/` sub-directory of any `content/` directory.
//!
//! Markdown references them by ordinary relative filesystem path, so opening
//! the `.md` in an editor shows the image. At render time the path is resolved
//! against the document's own directory and turned into a site URL with the
//! `_image` segment dropped:
//!
//! ```text
//! content/pages/_image/a.png   ←  ![x](_image/a.png)  in content/pages/x.md
//!                              →  <img src="/pages/a.png">
//! ```
//!
//! The mapping is one-to-one, so the server can invert it: `/pages/a.png` is
//! served from `content/pages/_image/a.png`. That only holds while `_image` is
//! the file's *direct* parent, so `_image/sub/a.png` is deliberately left
//! alone — put a second `_image/` inside `sub/` instead.

use std::path::{Path, PathBuf};

use crate::config::Routes;

/// The marker directory name. Single source of truth for both directions.
pub const DIR: &str = "_image";

/// Resolve one markdown/HTML `src` against `article_dir` (a content-relative
/// directory such as `posts/guide`, or `""` for the content root) and return
/// the site URL to use instead. The URL's first segment is run through
/// `routes.prefix_url`, so a renamed `posts`/`pages` route shows up in image
/// URLs too.
///
/// Returns `None` — meaning "leave the src untouched" — for anything that
/// isn't a relative path into an `_image/` directory: absolute URLs, schemes,
/// site-absolute paths, paths without an `_image` segment, and `../` chains
/// that would escape `content/`.
pub fn to_url(src: &str, article_dir: &str, routes: &Routes) -> Option<String> {
    let path = src.split(['?', '#']).next().unwrap_or("");
    if path.is_empty() || path.starts_with('/') || has_scheme(path) {
        return None;
    }
    let mut segs: Vec<&str> = if article_dir.is_empty() {
        Vec::new()
    } else {
        article_dir.split('/').collect()
    };
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                // Escapes content/ — leave it alone and let the author notice.
                segs.pop()?;
            }
            s => segs.push(s),
        }
    }
    // `_image` must be the file's direct parent, or the mapping isn't
    // invertible and the server could not find the file again.
    let parent = segs.len().checked_sub(2)?;
    if segs[parent] != DIR {
        return None;
    }
    segs.remove(parent);
    // The URL's first segment mirrors the configured posts/pages route.
    let out: Vec<String> = segs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if i == 0 {
                routes.prefix_url(s)
            } else {
                (*s).to_string()
            }
        })
        .collect();
    Some(format!("/{}", out.join("/")))
}

/// Rewrite the `src` of every `<img>` tag in rendered HTML.
///
/// Runs over the output of `markdown::render`, so it covers both `![](…)`
/// syntax and raw HTML `<img>` tags that markdown passes through verbatim.
/// Code spans and fenced blocks are already escaped to `&lt;img`, so examples
/// inside them are not touched.
pub fn rewrite_img_srcs(html: &str, article_dir: &str, routes: &Routes) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find("<img") {
        out.push_str(&rest[..at]);
        let tag_len = match rest[at..].find('>') {
            Some(end) => end + 1,
            None => break,
        };
        out.push_str(&rewrite_tag(&rest[at..at + tag_len], article_dir, routes));
        rest = &rest[at + tag_len..];
    }
    out.push_str(rest);
    out
}

/// Resolve a URL-relative path under `root`, refusing anything that escapes it.
///
/// Shared by the raw-file and image routes: segment-level rejection stops
/// textual traversal, and the canonical prefix check stops symlinks pointing
/// out of the tree.
pub fn contained(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.starts_with('/') {
        return None;
    }
    if rel.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return None;
    }
    let base = root.canonicalize().ok()?;
    let p = base.join(rel).canonicalize().ok()?;
    if p.starts_with(&base) && p.is_file() {
        Some(p)
    } else {
        None
    }
}

/// True for `https:`, `data:`, `mailto:` … and protocol-relative `//host/x`.
fn has_scheme(s: &str) -> bool {
    if s.starts_with("//") {
        return true;
    }
    match s.find(':') {
        Some(i) if i > 0 => s[..i]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')),
        _ => false,
    }
}

/// Substitute the `src` value of a single `<img …>` tag, if it maps.
fn rewrite_tag(tag: &str, article_dir: &str, routes: &Routes) -> String {
    let Some((start, quote)) = find_src_value(tag) else {
        return tag.to_string();
    };
    let Some(len) = tag[start..].find(quote) else {
        return tag.to_string();
    };
    match to_url(&tag[start..start + len], article_dir, routes) {
        Some(url) => format!("{}{}{}", &tag[..start], url, &tag[start + len..]),
        None => tag.to_string(),
    }
}

/// Byte offset just past the opening quote of `src="…"`, plus that quote char.
fn find_src_value(tag: &str) -> Option<(usize, char)> {
    let mut from = 0;
    loop {
        let at = from + tag[from..].find("src=")?;
        // Must be a whole attribute name, not the tail of e.g. `data-src=`.
        let is_attr = tag[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
        let quote = tag[at + 4..].chars().next();
        if is_attr && matches!(quote, Some('"') | Some('\'')) {
            return Some((at + 5, quote?));
        }
        from = at + 4;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routes() -> Routes {
        Routes::default()
    }

    #[test]
    fn maps_sibling_image_dir() {
        assert_eq!(
            to_url("_image/a.png", "pages", &routes()).as_deref(),
            Some("/pages/a.png")
        );
        assert_eq!(
            to_url("./_image/a.png", "pages", &routes()).as_deref(),
            Some("/pages/a.png")
        );
    }

    #[test]
    fn maps_from_content_root() {
        assert_eq!(to_url("_image/a.png", "", &routes()).as_deref(), Some("/a.png"));
    }

    #[test]
    fn maps_across_directories() {
        assert_eq!(
            to_url("../../pages/_image/logo.svg", "posts/guide", &routes()).as_deref(),
            Some("/pages/logo.svg")
        );
    }

    #[test]
    fn leaves_paths_that_escape_content() {
        assert_eq!(to_url("../../../_image/a.png", "pages", &routes()), None);
    }

    #[test]
    fn leaves_paths_without_an_image_segment() {
        assert_eq!(to_url("../other/a.png", "pages", &routes()), None);
        assert_eq!(to_url("images/a.png", "pages", &routes()), None);
    }

    #[test]
    fn leaves_nested_image_subdirectories() {
        // Not invertible: the server would look under `sub/_image/`.
        assert_eq!(to_url("_image/sub/a.png", "pages", &routes()), None);
    }

    #[test]
    fn leaves_absolute_and_external_srcs() {
        assert_eq!(to_url("/static/hero.png", "pages", &routes()), None);
        assert_eq!(to_url("https://example.com/_image/a.png", "pages", &routes()), None);
        assert_eq!(to_url("//cdn.example.com/_image/a.png", "pages", &routes()), None);
        assert_eq!(to_url("data:image/svg+xml,<svg/>", "pages", &routes()), None);
    }

    #[test]
    fn rewrites_markdown_and_raw_html_tags() {
        let h = rewrite_img_srcs(
            "<p><img src=\"_image/a.png\" alt=\"A\" /></p>\n<img width='40' src='_image/b.svg'>",
            "posts/guide",
            &routes(),
        );
        assert!(h.contains("src=\"/posts/guide/a.png\""));
        assert!(h.contains("alt=\"A\""), "other attributes survive");
        assert!(h.contains("src='/posts/guide/b.svg'"));
        assert!(h.contains("width='40'"));
    }

    #[test]
    fn uses_configured_content_prefix() {
        let r = Routes {
            posts: "blog".into(),
            pages: "docs".into(),
            ..Routes::default()
        };
        assert_eq!(
            to_url("_image/hero.svg", "posts/guide", &r).as_deref(),
            Some("/blog/guide/hero.svg")
        );
        assert_eq!(
            to_url("../../pages/_image/logo.svg", "posts/guide", &r).as_deref(),
            Some("/docs/logo.svg")
        );
    }

    #[test]
    fn skips_escaped_tags_in_code_blocks() {
        let src = "<pre><code>&lt;img src=\"_image/a.png\"&gt;</code></pre>";
        assert_eq!(rewrite_img_srcs(src, "pages", &routes()), src);
    }

    #[test]
    fn contained_rejects_traversal() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(contained(root, "Cargo.toml").is_some());
        assert!(contained(root, "../Cargo.toml").is_none());
        assert!(contained(root, "src/../Cargo.toml").is_none());
        assert!(contained(root, "/etc/passwd").is_none());
        assert!(contained(root, "src").is_none(), "directories are not files");
    }
}
