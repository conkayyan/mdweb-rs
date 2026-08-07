/// A small, dependency-free CommonMark-ish markdown renderer. Supports the
/// subset typically used in blog articles: headings, paragraphs, fenced code,
/// blockquote, ordered/unordered (nested) lists, HR, and inline emphasis,
/// code, links/images and strikethrough. Raw HTML passes through unchanged.

pub fn render(source: &str) -> String {
    let lines: Vec<String> = source.lines().map(|l| l.trim_end().to_string()).collect();
    render_lines(&lines)
}

fn render_lines(lines: &[String]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        // fenced code
        if let Some(info) = fence_info(line) {
            i += 1;
            let mut code = Vec::new();
            while i < lines.len() && fence_info(&lines[i]).is_none() {
                code.push(lines[i].clone());
                i += 1;
            }
            if i < lines.len() {
                i += 1; // closing fence
            }
            out.push_str("<pre><code class=\"language-");
            out.push_str(&escape_html(&trimmed_word(&info)));
            out.push_str("\">");
            out.push_str(&escape_html(&code.join("\n")));
            out.push_str("</code></pre>\n");
            continue;
        }
        // thematic break
        if is_hr(line) {
            out.push_str("<hr>\n");
            i += 1;
            continue;
        }
        // headings
        if let Some(level) = heading_level(line) {
            let text = line.trim_start_matches('#').trim();
            out.push_str(&format!(
                "<h{level}>{}</h{level}>\n",
                inline(text)
            ));
            i += 1;
            continue;
        }
        // blockquote
        if line.trim_start().starts_with('>') {
            let mut q = Vec::new();
            while i < lines.len() {
                let l = lines[i].trim_start();
                if l.starts_with('>') {
                    let content = l.trim_start_matches('>').trim_start().to_string();
                    q.push(content);
                    i += 1;
                } else if lines[i].trim().is_empty() {
                    q.push(String::new());
                    i += 1;
                } else {
                    break;
                }
            }
            out.push_str("<blockquote>");
            out.push_str(&render_lines(&q));
            out.push_str("</blockquote>\n");
            continue;
        }
        // lists
        if list_marker(line).is_some() {
            let indent = indent_of(line);
            let (items, next) = collect_list(lines, i, indent);
            let ordered = list_marker(line) == Some(true);
            let tag = if ordered { "ol" } else { "ul" };
            out.push_str(&format!("<{tag}>"));
            for item in &items {
                out.push_str("<li>");
                out.push_str(&render_item(item));
                out.push_str("</li>");
            }
            out.push_str(&format!("</{tag}>\n"));
            i = next;
            continue;
        }
        // paragraph
        let mut para = Vec::new();
        while i < lines.len() {
            let l = &lines[i];
            if l.trim().is_empty()
                || fence_info(l).is_some()
                || is_hr(l)
                || heading_level(l).is_some()
                || l.trim_start().starts_with('>')
                || list_marker(l).is_some()
            {
                break;
            }
            para.push(l.clone());
            i += 1;
        }
        let text = para.join(" ").trim().to_string();
        if !text.is_empty() {
            out.push_str("<p>");
            out.push_str(&inline(&text));
            out.push_str("</p>\n");
        }
    }
    out
}

fn trimmed_word(s: &str) -> String {
    s.split_whitespace().next().unwrap_or("").to_string()
}

fn render_item(lines: &[String]) -> String {
    let non_empty: Vec<&str> = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if non_empty.len() <= 1 {
        let text = non_empty.join(" ");
        return inline(&text);
    }
    render_lines(lines)
}

fn fence_info(line: &str) -> Option<String> {
    let t = line.trim_start();
    if let Some(r) = t.strip_prefix("```") {
        Some(r.trim().to_string())
    } else if let Some(r) = t.strip_prefix("~~~") {
        Some(r.trim().to_string())
    } else {
        None
    }
}

fn is_hr(line: &str) -> bool {
    let t = line.trim().to_string();
    let chars: Vec<char> = t.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() < 3 {
        return false;
    }
    let first = chars[0];
    (first == '-' || first == '*' || first == '_') && chars.iter().all(|c| *c == first)
}

fn heading_level(line: &str) -> Option<usize> {
    let t = line.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &t[hashes..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some(hashes)
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

/// Returns Some(true)=ordered, Some(false)=unordered, None=not a list line.
fn list_marker(line: &str) -> Option<bool> {
    let t = line.trim_start();
    if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
        return Some(false);
    }
    if let Some(d0) = t.chars().next().map(|c| c.is_ascii_digit()) {
        if d0 && t.find(|c| c == '.' || c == ')').is_some() {
            return Some(true);
        }
    }
    None
}

fn marker_body(line: &str, ordered: bool) -> String {
    let t = line.trim_start();
    if ordered {
        if let Some(idx) = t.find(|c| c == '.' || c == ')') {
            return t[idx + 1..].trim_start().to_string();
        }
        t.to_string()
    } else {
        t.trim_start_matches(['-', '*', '+']).trim_start().to_string()
    }
}

fn collect_list(lines: &[String], start: usize, indent: usize) -> (Vec<Vec<String>>, usize) {
    let mut items: Vec<Vec<String>> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let line = &lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        let il = indent_of(line);
        let ordered = list_marker(line);
        if il < indent || ordered.is_none() {
            break;
        }
        if il != indent {
            break;
        }
        let ordered = ordered == Some(true);
        let mut item = Vec::new();
        let body = marker_body(line, ordered);
        if !body.is_empty() {
            item.push(body);
        }
        i += 1;
        // continuations / nested content
        while i < lines.len() {
            let nl = &lines[i];
            if nl.trim().is_empty() {
                break;
            }
            let ni = indent_of(nl);
            if ni < indent {
                break;
            }
            if ni == indent && list_marker(nl).is_some() {
                break;
            }
            item.push(nl.clone());
            i += 1;
        }
        items.push(item);
    }
    (items, i)
}

/// Inline formatting: emphasis, strong, code, links, images, strike.
fn inline(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    loop {
        if rest.is_empty() {
            break;
        }
        let tokens: [(&str, TokenKind); 6] = [
            ("**", TokenKind::Strong),
            ("`", TokenKind::Code),
            ("![", TokenKind::Image),
            ("[", TokenKind::Link),
            ("~~", TokenKind::Strike),
            ("*", TokenKind::Italic),
        ];
        let mut best: Option<(usize, &str, TokenKind)> = None;
        for (tok, kind) in tokens.iter() {
            if let Some(idx) = rest.find(tok) {
                match &best {
                    None => {
                        best = Some((idx, *tok, *kind));
                    }
                    Some((bi, bt, _)) => {
                        if idx < *bi || (idx == *bi && *tok == "**") {
                            best = Some((idx, *tok, *kind));
                        }
                    }
                }
            }
        }
        let (idx, _tok, kind) = match best {
            Some(v) => v,
            None => {
                out.push_str(rest);
                break;
            }
        };
        out.push_str(&rest[..idx]);
        rest = &rest[idx..];
        match kind {
            TokenKind::Strong => {
                let t = rest.strip_prefix("**").unwrap();
                if let Some(cl) = t.find("**") {
                    out.push_str("<strong>");
                    out.push_str(&inline(&t[..cl]));
                    out.push_str("</strong>");
                    rest = &t[cl + 2..];
                } else {
                    out.push_str(rest);
                    break;
                }
            }
            TokenKind::Italic => {
                let t = rest.strip_prefix('*').unwrap();
                if let Some(cl) = t.find('*') {
                    out.push_str("<em>");
                    out.push_str(&inline(&t[..cl]));
                    out.push_str("</em>");
                    rest = &t[cl + 1..];
                } else {
                    out.push_str(rest);
                    break;
                }
            }
            TokenKind::Strike => {
                let t = rest.strip_prefix("~~").unwrap();
                if let Some(cl) = t.find("~~") {
                    out.push_str("<del>");
                    out.push_str(&inline(&t[..cl]));
                    out.push_str("</del>");
                    rest = &t[cl + 2..];
                } else {
                    out.push_str(rest);
                    break;
                }
            }
            TokenKind::Code => {
                let t = rest.strip_prefix('`').unwrap();
                let close = t.find('`');
                match close {
                    Some(cl) => {
                        out.push_str("<code>");
                        out.push_str(&escape_html(&t[..cl]));
                        out.push_str("</code>");
                        rest = &t[cl + 1..];
                    }
                    None => {
                        out.push_str("`");
                        out.push_str(t);
                        break;
                    }
                }
            }
            TokenKind::Image | TokenKind::Link => {
                let is_img = rest.starts_with("![");
                let inner = if is_img {
                    rest.strip_prefix("![").unwrap()
                } else {
                    rest.strip_prefix('[').unwrap()
                };
                let Some(close) = inner.find(']') else {
                    out.push_str(rest);
                    break;
                };
                let label = &inner[..close];
                let after = &inner[close + 1..];
                if let Some(rest_of_url) = after.strip_prefix('(') {
                    if let Some(close_paren) = rest_of_url.find(')') {
                        let url = &rest_of_url[..close_paren];
                        if is_img {
                            out.push_str(&format!(
                                "<img src=\"{}\" alt=\"{}\" />",
                                escape_attr(url),
                                escape_attr(label)
                            ));
                        } else {
                            out.push_str(&format!(
                                "<a href=\"{}\">{}</a>",
                                escape_attr(url),
                                inline(label)
                            ));
                        }
                        rest = &rest_of_url[close_paren + 1..];
                        continue;
                    }
                }
                out.push_str(rest);
                break;
            }
        }
    }
    out
}

#[derive(Clone, Copy)]
enum TokenKind {
    Strong,
    Italic,
    Strike,
    Code,
    Image,
    Link,
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_heading_and_emphasis() {
        let h = render("# Title\n\nSome **bold** and *italic* text.");
        assert!(h.contains("<h1>Title</h1>"));
        assert!(h.contains("<strong>bold</strong>"));
        assert!(h.contains("<em>italic</em>"));
    }

    #[test]
    fn renders_code_block() {
        let h = render("```rust\nfn main() {}\n```");
        assert!(h.contains("language-rust"));
        assert!(h.contains("fn main() {}"));
    }

    #[test]
    fn renders_link() {
        let h = render("[google](https://google.com)");
        assert!(h.contains("<a href=\"https://google.com\">google</a>"));
    }

    #[test]
    fn renders_list() {
        let h = render("- a\n- b");
        assert!(h.contains("<li>a</li>"));
        assert!(h.contains("<li>b</li>"));
    }
}
