use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use crate::content::{theme_files, Category, Site};
use crate::feed;
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
    if parts.len() < 2 {
        let _ = respond(&mut stream, 400, "Bad Request", "text/plain; charset=utf-8", b"bad request");
        return Ok(());
    }
    let method = parts[0];
    if method != "GET" && method != "HEAD" {
        let _ = respond(
            &mut stream,
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

    match route(site, &path, &query) {
        Ok(resp) => {
            respond(&mut stream, 200, "OK", resp.content_type, &resp.body)?;
        }
        Err(_) => {
            let html = render::render_not_found(site, &site.default_lang)
                .unwrap_or_else(|_| "<h1>404 Not Found</h1>".to_string());
            respond(
                &mut stream,
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

/// Route a request path and return the response (body + content-type).
fn route(site: &Arc<Site>, path: &str, query: &str) -> Result<Response, String> {
    let clean = path.trim_matches('/');
    let segs: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();

    // Static assets: doc `_static/<rel>` then embedded theme defaults.
    if !segs.is_empty() && segs[0] == "static" {
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
    if segs.len() == 1 && segs[0] == "search.json" {
        let body = feed::search_index_json(site);
        return Ok(Response {
            body: body.into_bytes(),
            content_type: "application/json; charset=utf-8",
        });
    }

    // RSS feed for the default language at /rss.xml; for other languages at
    // /<lang>/rss.xml. Detect both shapes after stripping the lang prefix
    // below.
    let rss_request = segs.last().copied() == Some("rss.xml");

    // Determine language (explicit prefix, or default for unprefixed paths).
    let (lang, rest): (String, Vec<&str>) = match segs.first() {
        Some(first) if site.languages.iter().any(|l| l == first) => {
            if segs.len() == 1 {
                (first.to_string(), Vec::new())
            } else {
                (first.to_string(), segs[1..].to_vec())
            }
        }
        _ => (site.default_lang.clone(), segs.clone()),
    };

    if rest.is_empty() {
        return render::render_home(site, &lang).map(|h| Response {
            body: h.into_bytes(),
            content_type: HTML,
        });
    }

    // RSS feed: must come before category/article resolution so it isn't
    // swallowed by a category whose slug happens to be "rss".
    if rss_request && rest == ["rss.xml"] {
        let body = feed::rss_xml(site, &lang, 50)
            .map_err(|e| format!("rss: {e}"))?;
        return Ok(Response {
            body: body.into_bytes(),
            content_type: "application/rss+xml; charset=utf-8",
        });
    }

    // Search page: /search?q=... (or /<lang>/search?q=...).
    if rest == ["search"] {
        let q = parse_form_query(query, "q");
        return render::render_search(site, &lang, &q).map(|h| Response {
            body: h.into_bytes(),
            content_type: HTML,
        });
    }

    if let Some(cat) = find_category(site, &rest.join("/")) {
        return render::render_category(site, &lang, cat).map(|h| Response {
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

    // Raw files under the doc tree (images, markdown source, ...).
    let rel = rest.join("/");
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
    let p = site.doc_root.join("_static").join(rel);
    if let Ok(data) = std::fs::read(&p) {
        return Some(data);
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
    rest: &[&str],
) -> Option<&'a crate::content::Article> {
    let last = rest.last().copied().unwrap_or("");
    let path_parts = &rest[..rest.len().saturating_sub(1)];
    site.articles.iter().find(|a| {
        a.lang == lang
            && a.slug == last
            && a.path.len() == path_parts.len()
            && a.path.iter().zip(path_parts.iter()).all(|(x, y)| x == y)
    })
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
    status: u16,
    reason: &str,
    ctype: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\nServer: mdweb\r\n\r\n",
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
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
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