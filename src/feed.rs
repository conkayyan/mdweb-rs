//! Search index, RSS feed, and sitemap rendering.
//!
//! The search index is served as a JSON document listing each article's title,
//! URL, summary, tags, and plain-text body so the sidebar's client-side
//! search box can match against it without a server round-trip.
//!
//! The RSS feed is a per-language RSS 2.0 document over the most recent
//! non-page articles. One feed per language is rendered from the same shared
//! helper so all `<link rel="alternate" type="application/rss+xml">`
//! references can point at consistent URLs.
//!
//! `search_results` runs the matching logic on the server side so the
//! `/search?q=...` page can render without round-tripping through JSON.

use crate::content::{Article, Site};

/// Strip HTML tags from a markdown-rendered string. Used to keep the search
/// index small (no `<p>` / `<strong>` tokens for the browser to chew on).
pub fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
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

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn article_text(article: &Article) -> String {
    collapse_ws(&strip_html(&article.content))
}

/// JSON search index for all languages. The shape is
/// `[{title,url,lang,summary,tags,date,text}, ...]` — kept intentionally
/// simple so the client-side script can iterate it cheaply.
pub fn search_index_json(site: &Site) -> String {
    let mut out = String::with_capacity(site.articles.len() * 200);
    out.push('[');
    for (i, a) in site.articles.iter().enumerate() {
        if a.draft {
            continue;
        }
        if i > 0 {
            out.push(',');
        }
        let title = json_str(&a.title);
        let url = json_str(&a.url);
        let lang = json_str(&a.lang);
        let summary = json_str(&a.summary);
        let date = json_str(a.date.as_deref().unwrap_or(""));
        let date_iso = json_str(a.date_iso.as_deref().unwrap_or(""));
        let text = json_str(&truncate(&article_text(a), 4000));
        let tags = a
            .tags
            .iter()
            .map(|t| json_str(t))
            .collect::<Vec<_>>()
            .join(",");
        out.push('{');
        out.push_str(&format!(
            "\"title\":{title},\"url\":{url},\"lang\":{lang},\"summary\":{summary},\
             \"tags\":[{tags}],\"date\":{date},\"date_iso\":{date_iso},\"text\":{text}"
        ));
        out.push('}');
    }
    out.push(']');
    out
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out = String::with_capacity(max + 4);
    for (i, c) in s.chars().enumerate() {
        if i >= max {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// One matched article. The server-side `search_results` returns a sorted
/// `Vec<&Article>` and `render_search` converts each one to a `Value`.
pub struct SearchHit<'a> {
    pub article: &'a Article,
    /// Lowercased copy of the haystack the query matched against — kept so
    /// the template can render a snippet showing where the hit landed.
    pub haystack: String,
}

/// Server-side search: matches `query` against title/summary/tags/body for
/// every non-draft article. Empty / whitespace-only queries return `[]`.
/// Hits in the request's language sort first; ties broken by `sort_ts` desc.
pub fn search_results<'a>(site: &'a Site, query: &str, lang: &str) -> Vec<SearchHit<'a>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let terms: Vec<&str> = q.split_whitespace().collect();
    let mut hits: Vec<SearchHit<'a>> = Vec::new();
    for a in &site.articles {
        if a.draft {
            continue;
        }
        let haystack = format!(
            "{} {} {} {}",
            a.title.to_lowercase(),
            a.summary.to_lowercase(),
            a.tags.iter().map(|t| t.to_lowercase()).collect::<Vec<_>>().join(" "),
            article_text(a).to_lowercase()
        );
        if terms.iter().all(|t| haystack.contains(*t)) {
            hits.push(SearchHit { article: a, haystack });
        }
    }
    hits.sort_by(|x, y| {
        let xm = if x.article.lang == lang { 0 } else { 1 };
        let ym = if y.article.lang == lang { 0 } else { 1 };
        if xm != ym {
            return xm.cmp(&ym);
        }
        y.article.sort_ts.cmp(&x.article.sort_ts)
    });
    hits
}

/// RSS 2.0 XML for a single language. Sorted newest first. Includes up to
/// `limit` items (pass `usize::MAX` for everything).
pub fn rss_xml(site: &Site, lang: &str, limit: usize) -> Result<String, String> {
    let mut arts: Vec<&Article> = site
        .articles
        .iter()
        .filter(|a| a.lang == lang && a.layout != "page" && !a.draft)
        .collect();
    arts.sort_by_key(|a| std::cmp::Reverse(a.sort_ts));
    arts.truncate(limit);

    let title = site.title_for(lang);
    let desc = site.config.description_for(lang);
    let base = site.config.base_url.trim_end_matches('/').to_string();
    let feed_url = format!("{}{}/rss.xml", base, site.config.lang_prefix(lang).trim_end_matches('/'));

    let mut out = String::with_capacity(2048 + arts.len() * 512);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">\n");
    out.push_str("  <channel>\n");
    out.push_str(&format!("    <title>{}</title>\n", xml_escape(&title)));
    out.push_str(&format!("    <link>{}</link>\n", xml_escape(&base)));
    out.push_str(&format!(
        "    <description>{}</description>\n",
        xml_escape(&desc)
    ));
    if !site.config.author.is_empty() {
        out.push_str(&format!(
            "    <managingEditor>{}</managingEditor>\n",
            xml_escape(&site.config.author)
        ));
    }
    out.push_str(&format!(
        "    <atom:link href=\"{}\" rel=\"self\" type=\"application/rss+xml\" />\n",
        xml_escape(&feed_url)
    ));

    let lang_attr = if lang.is_empty() { String::new() } else { format!(" xml:lang=\"{lang}\"") };
    for a in arts {
        let link = format!("{base}{}", a.url);
        let pub_date = a
            .date_iso
            .as_deref()
            .map(rfc822_from_iso)
            .unwrap_or_default();
        out.push_str("    <item>\n");
        out.push_str(&format!(
            "      <title{lang_attr}>{}</title>\n",
            xml_escape(&a.title)
        ));
        out.push_str(&format!("      <link>{}</link>\n", xml_escape(&link)));
        let guid = link.clone();
        out.push_str(&format!(
            "      <guid isPermaLink=\"true\">{}</guid>\n",
            xml_escape(&guid)
        ));
        if !pub_date.is_empty() {
            out.push_str(&format!("      <pubDate>{}</pubDate>\n", pub_date));
        }
        if !a.author.is_empty() {
            out.push_str(&format!(
                "      <author>{}</author>\n",
                xml_escape(&a.author)
            ));
        }
        for t in &a.tags {
            out.push_str(&format!("      <category>{}</category>\n", xml_escape(t)));
        }
        let body = if a.summary.is_empty() {
            truncate(&collapse_ws(&strip_html(&a.content)), 1200)
        } else {
            a.summary.clone()
        };
        out.push_str(&format!(
            "      <description>{}</description>\n",
            xml_escape(&body)
        ));
        out.push_str("    </item>\n");
    }
    out.push_str("  </channel>\n");
    out.push_str("</rss>\n");
    Ok(out)
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// Convert an ISO-8601 date (YYYY-MM-DD or full ISO timestamp) to an
/// RFC 822 string suitable for `<pubDate>`. Falls back to the raw string
/// if it cannot be parsed.
fn rfc822_from_iso(iso: &str) -> String {
    let (date_part, time_part) = match iso.split_once('T') {
        Some((d, t)) => (d, t.trim_end_matches('Z')),
        None => (iso, "00:00:00"),
    };
    let mut parts = date_part.split('-');
    let y = match parts.next() {
        Some(s) => match s.parse::<i32>() {
            Ok(v) => v,
            Err(_) => return iso.to_string(),
        },
        None => return iso.to_string(),
    };
    let m = match parts.next() {
        Some(s) => match s.parse::<u32>() {
            Ok(v) => v,
            Err(_) => return iso.to_string(),
        },
        None => return iso.to_string(),
    };
    let d = match parts.next() {
        Some(s) => match s.parse::<u32>() {
            Ok(v) => v,
            Err(_) => return iso.to_string(),
        },
        None => return iso.to_string(),
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return iso.to_string();
    }
    let time = if time_part.is_empty() { "00:00:00" } else { time_part };
    format!("{y:04}-{m:02}-{d:02}T{time}Z")
}

