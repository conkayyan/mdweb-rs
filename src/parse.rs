use std::collections::BTreeMap;

use crate::value::Value;

/// Extract and parse an optional frontmatter block from a markdown source.
/// Returns (fields, body).
///
/// Delimiters: `---` (YAML-ish) or `+++` (TOML-ish). The block is parsed with
/// the small native parsers in this module.
pub fn parse_frontmatter(source: &str) -> (BTreeMap<String, Value>, String) {
    let trimmed = source.trim_start_matches('\u{feff}');
    let trimmed = trimmed.strip_prefix('\r').unwrap_or(trimmed);
    let trimmed = trimmed.strip_prefix('\n').unwrap_or(trimmed);

    let delim = if trimmed.starts_with("---\n") || trimmed.starts_with("---\r\n") {
        Some("---")
    } else if trimmed.starts_with("+++\n") || trimmed.starts_with("+++\r\n") {
        Some("+++")
    } else {
        None
    };
    let delim = match delim {
        Some(d) => d,
        None => return (BTreeMap::new(), source.to_string()),
    };

    let mut lines: Vec<&str> = Vec::new();
    let mut body_start = 0usize;
    let mut found = false;
    for (i, line) in trimmed.lines().enumerate() {
        if i == 0 {
            continue; // opening delimiter
        }
        if line.trim() == delim {
            found = true;
            body_start = i + 1;
            break;
        }
        lines.push(line);
    }
    if !found {
        return (BTreeMap::new(), source.to_string());
    }
    let mut block = lines.join("\n");
    // Re-add blank lines that lines() collapses
    block = block; // keep simple

    let fields = if delim == "+++" {
        parse_toml(&block).ok().and_then(|v| v.as_map().cloned()).unwrap_or_default()
    } else {
        parse_yaml(&block)
    };

    // Reconstruct the body (may contain the delimiter inside code? unlikely)
    let body = trimmed.lines().skip(body_start).collect::<Vec<_>>().join("\n");
    (fields, body)
}

/// Parse an indentation-based YAML-ish block into a map.
pub fn parse_yaml(text: &str) -> BTreeMap<String, Value> {
    let lines: Vec<&str> = text.lines().collect();
    let mut p = Cursor::new(lines);
    p.parse_map(0).unwrap_or_default()
}

struct Cursor<'a> {
    lines: Vec<&'a str>,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(lines: Vec<&'a str>) -> Self {
        Cursor { lines, pos: 0 }
    }

    fn peek(&self) -> Option<&'a str> {
        self.lines.get(self.pos).copied()
    }

    fn parse_map(&mut self, indent: usize) -> Option<BTreeMap<String, Value>> {
        let mut map = BTreeMap::new();
        loop {
            let line = match self.peek() {
                Some(l) => l,
                None => break,
            };
            if line.trim().is_empty() {
                self.pos += 1;
                continue;
            }
            let il = indent_of(line);
            if il < indent {
                break;
            }
            if il > indent {
                // Child content that did not belong to a key; stop.
                break;
            }
            if line.trim_start().starts_with('-') {
                break;
            }
            self.pos += 1;
            let trimmed = line.trim_start();
            let (key, rest) = split_key_value(trimmed);
            if key.is_empty() {
                continue;
            }
            let rest = rest.trim();
            if rest.is_empty() {
                // nested block
                // find next non-blank line
                let mut j = self.pos;
                while let Some(l) = self.lines.get(j) {
                    if !l.trim().is_empty() {
                        break;
                    }
                    j += 1;
                }
                if let Some(l) = self.lines.get(j) {
                    let child_indent = indent_of(l);
                    if child_indent > indent {
                        if l.trim_start().starts_with('-') {
                            let v = self.parse_list(child_indent);
                            map.insert(key, Value::Arr(v));
                        } else {
                            let m = self.parse_map(child_indent).unwrap_or_default();
                            map.insert(key, Value::Map(m));
                        }
                        continue;
                    }
                }
                map.insert(key, Value::Null);
            } else {
                map.insert(key, parse_scalar(rest));
            }
        }
        Some(map)
    }

    fn parse_list(&mut self, indent: usize) -> Vec<Value> {
        let mut list = Vec::new();
        loop {
            let line = match self.peek() {
                Some(l) => l,
                None => break,
            };
            if line.trim().is_empty() {
                self.pos += 1;
                continue;
            }
            let il = indent_of(line);
            if il < indent {
                break;
            }
            if il > indent {
                // unexpected deeper content; skip
                self.pos += 1;
                continue;
            }
            let trimmed = line.trim_start();
            if !trimmed.starts_with('-') {
                break;
            }
            self.pos += 1;
            let rest = trimmed[1..].trim();
            if rest.is_empty() {
                let mut j = self.pos;
                while let Some(l) = self.lines.get(j) {
                    if !l.trim().is_empty() {
                        break;
                    }
                    j += 1;
                }
                if let Some(l) = self.lines.get(j) {
                    let child_indent = indent_of(l);
                    if child_indent > indent {
                        if l.trim_start().starts_with('-') {
                            list.push(Value::Arr(self.parse_list(child_indent)));
                        } else {
                            list.push(Value::Map(self.parse_map(child_indent).unwrap_or_default()));
                        }
                        continue;
                    }
                }
                list.push(Value::Null);
            } else {
                list.push(parse_scalar(rest));
            }
        }
        list
    }
}

/// Minimal TOML-subset parser used for `site.toml`.
pub fn parse_toml(text: &str) -> Result<Value, String> {
    let mut root = Value::map();
    let mut section: Vec<String> = Vec::new();

    for raw_line in text.lines() {
        let line = strip_comment(raw_line).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') {
                return Err(format!("bad table header: {line}"));
            }
            let inner = &line[1..line.len() - 1];
            section = inner
                .split('.')
                .map(|s| s.trim().trim_matches('"').to_string())
                .collect();
            continue;
        }
        let Some(eq) = line.find('=') else {
            return Err(format!("bad line: {line}"));
        };
        let key = line[..eq].trim().trim_matches('"').to_string();
        let value = parse_scalar(line[eq + 1..].trim());
        let mut path = section.clone();
        path.push(key);
        root.insert_path(&path.join("."), value);
    }
    Ok(root)
}

fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'"' {
            in_str = !in_str;
        } else if b == b'#' && !in_str {
            return &line[..i];
        }
    }
    line
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

fn split_key_value(line: &str) -> (String, String) {
    let line = line.trim_start();
    // keys may be quoted: "my key": value
    if let Some(stripped) = line.strip_prefix('"') {
        if let Some(end) = stripped.find('"') {
            let key = &stripped[..end];
            let rest = &stripped[end + 1..];
            let rest = rest.strip_prefix(':').unwrap_or(rest);
            return (key.to_string(), rest.trim().to_string());
        }
    }
    if let Some(idx) = line.find(':') {
        return (line[..idx].trim().to_string(), line[idx + 1..].to_string());
    }
    (line.to_string(), String::new())
}

/// Parse a scalar string into a Value.
pub fn parse_scalar(s: &str) -> Value {
    let s = s.trim();
    if s.is_empty() {
        return Value::Null;
    }
    if let Some(rest) = s.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return Value::Str(unescape(&rest[..end]));
        }
    }
    if let Some(rest) = s.strip_prefix('\'') {
        if let Some(end) = rest.find('\'') {
            return Value::Str(rest[..end].to_string());
        }
    }
    match s {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        "null" | "~" => return Value::Null,
        _ => {}
    }
    if let Some(arr) = parse_array(s) {
        return Value::Arr(arr);
    }
    if let Ok(i) = s.parse::<i64>() {
        return Value::Int(i);
    }
    Value::Str(s.to_string())
}

fn parse_array(s: &str) -> Option<Vec<Value>> {
    if !s.starts_with('[') || !s.ends_with(']') {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    let items = split_commas(inner);
    Some(items.into_iter().map(|i| parse_scalar(&i)).collect())
}

fn split_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_str = !in_str;
                cur.push(c);
            }
            ',' if !in_str => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('r') => out.push('\r'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_frontmatter() {
        let src = "---\ntitle: Hello World\ndate: 2024-01-01\nauthor: Jane\ntags: [rust, web]\n---\n\n# Body\ncontent here";
        let (fm, body) = parse_frontmatter(src);
        assert_eq!(fm.get("title").unwrap().as_str(), Some("Hello World"));
        assert!(body.contains("# Body"));
    }

    #[test]
    fn parses_nested_map() {
        let src = "title: Hi\nmeta:\n  description: A site\n  keywords: a, b\n";
        let fm = parse_yaml(src);
        let meta = fm.get("meta").unwrap().as_map().unwrap();
        assert_eq!(meta.get("description").unwrap().as_str(), Some("A site"));
    }

    #[test]
    fn parses_toml() {
        let t = "title = \"Blog\"\n[lang.en]\ntitle = \"English\"\n";
        let v = parse_toml(t).unwrap();
        let lang = v.as_map().unwrap().get("lang").unwrap().as_map().unwrap();
        assert_eq!(lang.get("en").unwrap().as_map().unwrap().get("title").unwrap().as_str(), Some("English"));
    }
}
