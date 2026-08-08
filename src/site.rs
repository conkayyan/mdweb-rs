//! The default seed site, embedded at compile time from the `site/` directory
//! at the repo root by `build.rs`. `FILES` maps each relative path under
//! `site/` to its content so `mdweb create` can scaffold a fresh site without
//! shipping a single string constant.

include!(concat!(env!("OUT_DIR"), "/site_files.rs"));

/// Look up an embedded default-site file by its relative path (forward
/// slashes), e.g. `get("site.toml")` or `get("template/default/base.html")`.
pub fn get(rel: &str) -> Option<&'static str> {
    FILES.iter().find(|(p, _)| *p == rel).map(|(_, s)| *s)
}

/// All embedded files as `(relative_path, content)` pairs.
pub fn all() -> &'static [(&'static str, &'static str)] {
    FILES
}