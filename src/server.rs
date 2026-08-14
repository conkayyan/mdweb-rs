use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use crate::config::POSTS_DIR;
use crate::content::{theme_files, Category, Site};
use crate::feed;
use crate::image_path;
use crate::render;

/// Run the web server until the process is killed / interrupted.
pub fn serve(site: Site, host: &str, port: u16) -> Result<(), String> {
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr).map_err(|e| e.to_string())?;
    let site = Arc::new(site);
    println!("mdweb: serving site at http://{}/", addr);
    println!("       docs: {}", site.doc_root.display());
    for lang in &site.languages {
        println!(
            "       /{lang}/ => http://{}{}",
            addr,
            site.config.lang_prefix(lang)
        );
    }
    println!("press Ctrl-C to stop");

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let site = Arc::clone(&site);
        std::thread::spawn(move || {
            let _ = handle(stream, &site);
        });
    }
    Ok(())
}

fn handle(mut stream: TcpStream, site: &Arc<Site>) -> std::io::Result<()> {
    // Bound how long a connection may sit idle while we are still reading the
    // request head (or writing the response) so slowloris-style connections
    // can't hold a thread open indefinitely.
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    // Read the request head.
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 64 * 1024 {
            break;
        }
    }
    let head = String::from_utf8_lossy(&buf).to_string();
    let request_line = head.lines().next().unwrap_or("GET / HTTP/1.1");
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let csp = site.config.security.csp_header();
    if parts.len() < 2 {
        let _ = respond(
            &mut stream,
            &csp,
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            b"bad request",
        );
        return Ok(());
    }
    let method = parts[0];
    if method != "GET" && method != "HEAD" {
        let _ = respond(
            &mut stream,
            &csp,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method Not Allowed",
        );
        return Ok(());
    }
    let raw_target = parts[1];
    let path_part = raw_target.split('?').next().unwrap_or("/");
    let query = raw_target
        .split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("")
        .to_string();
    let path = percent_decode(path_part);

    // Detect the request's language so the 404 page renders in the
    // caller's tongue instead of always falling back to the site default.
    let lang = detect_lang_from_path(site, &path);

    match route(site, &path, &query) {
        Ok(resp) => {
            respond(&mut stream, &csp, 200, "OK", resp.content_type, &resp.body)?;
        }
        Err(_) => {
            let html = render::render_not_found(site, &lang)
                .unwrap_or_else(|_| "<h1>404 Not Found</h1>".to_string());
            respond(
                &mut stream,
                &csp,
                404,
                "Not Found",
                "text/html; charset=utf-8",
                html.as_bytes(),
            )?;
        }
    }
    Ok(())
}

/// A routed response: body bytes plus the Content-Type header to send.
struct Response {
    body: Vec<u8>,
    content_type: &'static str,
}

const HTML: &str = "text/html; charset=utf-8";

/// Split a request path into its segments, ignoring empty ones.
fn path_segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// Split path segments into `(language, remaining)` using the first-segment
/// language convention: the first segment is the language code when it matches
/// a configured language, otherwise the site default and the whole path.
fn split_lang<'a>(site: &Site, segs: &[&'a str]) -> (String, Vec<&'a str>) {
    match segs.first() {
        Some(first) if site.languages.iter().any(|l| l == first) => {
            if segs.len() == 1 {
                (first.to_string(), Vec::new())
            } else {
                (first.to_string(), segs[1..].to_vec())
            }
        }
        _ => (site.default_lang.clone(), segs.to_vec()),
    }
}

/// Detect the request language from a URL path so the 404 page renders in the
/// caller's tongue instead of always falling back to the site default.
fn detect_lang_from_path(site: &Site, path: &str) -> String {
    split_lang(site, &path_segments(path)).0
}

/// Route a request path and return the response (body + content-type).
fn route(site: &Arc<Site>, path: &str, query: &str) -> Result<Response, String> {
    let segs = path_segments(path);
    let routes = &site.config.routes;

    // Static assets: doc `template/<theme>/static/<rel>` then embedded theme
    // defaults.
    if !segs.is_empty() && segs[0] == routes.static_dir {
        if segs.len() > 1 {
            let rest = segs[1..].join("/");
            if let Some(body) = serve_static(site, &rest) {
                return Ok(Response {
                    body,
                    content_type: content_type(&rest),
                });
            }
        }
        return Err("not found".into());
    }

    // Search index: unprefixed, spans all languages.
    if segs.len() == 1 && segs[0] == routes.search_index {
        let body = feed::search_index_json(site);
        return Ok(Response {
            body: body.into_bytes(),
            content_type: "application/json; charset=utf-8",
        });
    }

    // Sitemap: unprefixed, spans all languages.
    if segs.len() == 1 && segs[0] == routes.sitemap {
        let body = feed::sitemap_xml(site);
        return Ok(Response {
            body: body.into_bytes(),
            content_type: "application/xml; charset=utf-8",
        });
    }

    // RSS feed for the default language at `routes.rss`; for other languages
    // at `/<lang>/routes.rss`. Detect both shapes after stripping the lang
    // prefix below.
    let rss_request = segs.last().copied() == Some(routes.rss.as_str());

    // Determine language (explicit prefix, or default for unprefixed paths).
    let (lang, rest): (String, Vec<&str>) = split_lang(site, &segs);

    // ?page=N applies to home, category, and page-section listings.
    let page = parse_form_query(query, "page")
        .parse::<usize>()
        .ok()
        .filter(|&n| n > 0)
        .unwrap_or(1);

    if rest.is_empty() {
        return render::render_home(site, &lang, page).map(|h| Response {
            body: h.into_bytes(),
            content_type: HTML,
        });
    }

    // RSS feed: must come before category/article resolution so it isn't
    // swallowed by a category whose slug happens to be the configured rss name.
    if rss_request && rest == [routes.rss.as_str()] {
        let body = feed::rss_xml(site, &lang, 50).map_err(|e| format!("rss: {e}"))?;
        return Ok(Response {
            body: body.into_bytes(),
            content_type: "application/rss+xml; charset=utf-8",
        });
    }

    // Search page: /search?q=... (or /<lang>/search?q=...).
    if rest == [routes.search.as_str()] {
        let q = parse_form_query(query, "q");
        return render::render_search(site, &lang, &q).map(|h| Response {
            body: h.into_bytes(),
            content_type: HTML,
        });
    }

    // Tags index: {routes.tags}/ (or /<lang>/{routes.tags}/) lists every tag
    // in the language.
    if rest.len() == 1 && rest[0] == routes.tags.as_str() {
        return render::render_tags_index(site, &lang).map(|h| Response {
            body: h.into_bytes(),
            content_type: HTML,
        });
    }

    // Tag listing: {routes.tags}/<tag>/ (or /<lang>/{routes.tags}/<tag>/).
    // Must come before category/section resolution so a tag name can't
    // collide with a dir.
    if rest.len() == 2 && rest[0] == routes.tags.as_str() {
        let name = percent_decode(rest[1]);
        return render::render_tag(site, &lang, &name, page).map(|h| Response {
            body: h.into_bytes(),
            content_type: HTML,
        });
    }

    // Aliases: custom pretty URLs defined in frontmatter (`aliases =
    // ["about-us"]` → `/zh/about-us/`). An alias replaces the whole on-disk
    // path, so it must be matched against the raw URL before the disk-prefix
    // mapping below.
    let alias_key = rest.join("/");
    if let Some(a) = find_alias(site, &lang, &alias_key) {
        return render::render_article(site, &lang, a).map(|h| Response {
            body: h.into_bytes(),
            content_type: HTML,
        });
    }

    // Content routes match against on-disk paths, so map the URL container
    // prefix back to its directory name first (e.g. a `routes.posts = "blog"`
    // prefix → `posts`). Components under a renamed prefix are translated by
    // the first segment only; `images` and raw-file resolution keep working
    // because `resolve_image`/`resolve_file` invert their own prefix.
    let mut rest: Vec<String> = rest.iter().map(|s| s.to_string()).collect();
    if let Some(first) = rest.first_mut() {
        *first = routes.prefix_disk(first);
    }

    if let Some(cat) = find_category(site, &rest.join("/")) {
        return render::render_category(site, &lang, cat, page).map(|h| Response {
            body: h.into_bytes(),
            content_type: HTML,
        });
    }

    // Directory landing pages: only consider non-`posts` paths. Category
    // lookups already handled `posts/*`; here we resolve `_index.md`-bearing
    // directories under e.g. `pages/`, `notes/`, …
    if let Some(dir) = find_section(site, &rest) {
        return render::render_section(site, &lang, &dir, page).map(|h| Response {
            body: h.into_bytes(),
            content_type: HTML,
        });
    }

    if let Some(a) = find_article(site, &lang, &rest) {
        return render::render_article(site, &lang, a).map(|h| Response {
            body: h.into_bytes(),
            content_type: HTML,
        });
    }

    // Images: `/pages/a.png` is served from `content/pages/_image/a.png`.
    // Comes after the content matchers so a document always wins a tie.
    let rel = rest.join("/");
    if let Some(p) = site.resolve_image(&rel) {
        if let Ok(data) = std::fs::read(&p) {
            return Ok(Response {
                body: data,
                content_type: content_type(&rel),
            });
        }
    }

    // Raw files under the doc tree (images, markdown source, ...).
    if let Some(p) = site.resolve_file(&rel) {
        if let Ok(data) = std::fs::read(&p) {
            return Ok(Response {
                body: data,
                content_type: content_type(&rel),
            });
        }
    }
    Err("not found".into())
}

fn serve_static(site: &Arc<Site>, rel: &str) -> Option<Vec<u8>> {
    // Files live in `template/<theme>/static/` on disk regardless of the
    // configured URL prefix; only the route changes.
    let theme = if site.theme.is_empty() {
        "default"
    } else {
        site.theme.as_str()
    };
    let dir = site.doc_root.join("template").join(theme).join("static");
    // `contained` rejects `..`, `.`, empty and absolute segments and, after
    // canonicalizing both paths, refuses anything that resolves outside
    // `dir` — so `/static/../../../site.toml` cannot escape the static
    // directory. (None just means "not on disk under `dir`"; the embedded
    // style.css fallback below still applies.)
    if let Some(p) = image_path::contained(&dir, rel) {
        if let Ok(data) = std::fs::read(&p) {
            return Some(data);
        }
    }
    if site.engine_embedded && rel == "style.css" {
        return Some(theme_files::STYLE.as_bytes().to_vec());
    }
    None
}

fn find_category<'a>(site: &'a Site, key: &str) -> Option<&'a Category> {
    fn walk<'a>(cats: &'a [Category], key: &str) -> Option<&'a Category> {
        for c in cats {
            if c.path.join("/") == key {
                return Some(c);
            }
            if let Some(f) = walk(&c.children, key) {
                return Some(f);
            }
        }
        None
    }
    walk(&site.tree, key)
}

fn find_article<'a>(
    site: &'a Site,
    lang: &str,
    rest: &[String],
) -> Option<&'a crate::content::Article> {
    let last = rest.last().map(|s| s.as_str()).unwrap_or("");
    let path_parts = &rest[..rest.len().saturating_sub(1)];
    site.articles.iter().find(|a| {
        a.lang == lang
            && a.slug == last
            && a.path.len() == path_parts.len()
            && a.path.iter().zip(path_parts.iter()).all(|(x, y)| x == y)
    })
}

/// Match a request path (already stripped of the language prefix) against any
/// document's frontmatter `aliases` list. An alias is an exact whole-path
/// match — `aliases = ["about-us"]` matches `/zh/about-us/` but not
/// `/zh/about-us/x/`.
fn find_alias<'a>(
    site: &'a Site,
    lang: &str,
    key: &str,
) -> Option<&'a crate::content::Article> {
    site.articles
        .iter()
        .find(|a| a.lang == lang && a.aliases.iter().any(|al| al == key))
}

/// Match a request path against any `_index.md` directory in the doc tree,
/// excluding `posts/*` (those are categories). Returns the matched directory
/// path segments when found.
fn find_section(site: &Site, rest: &[String]) -> Option<Vec<String>> {
    if rest.is_empty() {
        return None;
    }
    let dir: Vec<String> = rest.to_vec();
    let key = dir.join("/");
    if site.indices.contains_key(&key) && dir.first().map(|s| s.as_str()) != Some(POSTS_DIR) {
        Some(dir)
    } else {
        None
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(v) = hex2(bytes[i + 1], bytes[i + 2]) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex2(a: u8, b: u8) -> Option<u8> {
    let hi = hexv(a)?;
    let lo = hexv(b)?;
    Some((hi << 4) | lo)
}

fn hexv(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Pull a single field out of an x-www-form-urlencoded query string.
fn parse_form_query(query: &str, key: &str) -> String {
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        if percent_decode(k) == key {
            return percent_decode(v);
        }
    }
    String::new()
}

fn respond(
    stream: &mut TcpStream,
    csp: &str,
    status: u16,
    reason: &str,
    ctype: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let csp_line = if csp.is_empty() {
        String::new()
    } else {
        format!("Content-Security-Policy: {csp}\r\n")
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\nServer: mdweb\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: SAMEORIGIN\r\nReferrer-Policy: no-referrer\r\n{csp_line}\r\n",
        body.len(),
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn content_type(path: &str) -> &'static str {
    use std::path::Path;
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "json" => "application/json",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "apng" => "image/apng",
        "jpg" | "jpeg" | "jfif" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "jxl" => "image/jxl",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn write(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, body).unwrap();
    }

    fn tempdir(prefix: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "mdweb-srv-{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn build() -> Site {
        let dir = tempdir("alias");
        write(&dir, "site.toml", r#"languages = ["en", "zh"]"#);
        write(
            &dir,
            "content/pages/about.md",
            "---\ntitle: About\n---\nALIAS-BODY\n",
        );
        write(
            &dir,
            "content/pages/about.zh.md",
            "---\ntitle: 关于\n---\n中文正文\n",
        );
        write(
            &dir,
            "content/posts/hello.md",
            "---\ntitle: Hello\ndate: 2026-08-01\n---\nHELLO-BODY\n",
        );
        write(
            &dir,
            "content/posts/hello.zh.md",
            "---\ntitle: 你好\naliases = [\"hello-zh\"]\ndate: 2026-08-01\n---\n你好正文\n",
        );
        Site::build(&dir, None).expect("build")
    }

    fn body_of(resp: Result<Response, String>) -> String {
        String::from_utf8(resp.expect("ok response").body).unwrap()
    }

    #[test]
    fn aliases_route_and_keep_slug_url() {
        let site = Arc::new(build());

        // Page without aliases keeps its slug URL.
        assert!(route(&site, "/pages/about/", "").is_ok());

        // Canonical URL of the aliased zh post is its first alias.
        let hello_zh = site
            .articles
            .iter()
            .find(|a| a.slug == "hello" && a.lang == "zh")
            .expect("zh post");
        assert_eq!(hello_zh.url, "/zh/hello-zh/");
        assert_eq!(hello_zh.aliases, vec!["hello-zh".to_string()]);
        // The English post (no alias) keeps its slug URL.
        let hello_en = site
            .articles
            .iter()
            .find(|a| a.slug == "hello" && a.lang == "en")
            .expect("en post");
        assert_eq!(hello_en.url, "/posts/hello/");

        // Alias path serves the document.
        assert!(body_of(route(&site, "/zh/hello-zh/", "")).contains("你好正文"));
        // The original slug path still works alongside the alias.
        assert!(body_of(route(&site, "/zh/posts/hello/", "")).contains("你好正文"));
        // Alias without the zh prefix must not resolve (wrong language).
        assert!(route(&site, "/hello-zh/", "").is_err());
        // A partial match is not an alias hit.
        assert!(route(&site, "/zh/hello-zh/extra/", "").is_err());
    }
}
