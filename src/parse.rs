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
    let block = lines.join("\n");

    let fields = if delim == "+++" {
        parse_toml(&block)
            .ok()
            .and_then(|v| v.as_map().cloned())
            .unwrap_or_default()
    } else {
        parse_yaml(&block)
    };

    // Reconstruct the body (may contain the delimiter inside code? unlikely)
    let body = trimmed
        .lines()
        .skip(body_start)
        .collect::<Vec<_>>()
        .join("\n");
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
        while let Some(line) = self.peek() {
            let trimmed_start = line.trim_start();
            if trimmed_start.is_empty() || trimmed_start.starts_with('#') {
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
            if trimmed_start.starts_with('-') {
                break;
            }
            self.pos += 1;
            let trimmed = strip_yaml_comment(line.trim_start());
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
        while let Some(line) = self.peek() {
            let trimmed_start = line.trim_start();
            if trimmed_start.is_empty() || trimmed_start.starts_with('#') {
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
            if !trimmed_start.starts_with('-') {
                break;
            }
            self.pos += 1;
            let rest = strip_yaml_comment(&trimmed_start[1..]).trim();
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
    let mut array_section: Option<Vec<String>> = None;

    for raw_line in text.lines() {
        let line = strip_comment(raw_line).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("[[") {
            // Array of tables: each [[name]] appends a new map into the
            // array at `name`. Subsequent key = value lines fill that map.
            // Segments may nest arrays-of-tables: `[[nav]]` then
            // `[[nav.children]]` appends into the *last* `[[nav]]` entry.
            if !line.ends_with("]]") {
                return Err(format!("bad array header: {line}"));
            }
            let inner = &line[2..line.len() - 2];
            let parts: Vec<String> = inner
                .split('.')
                .map(|s| s.trim().trim_matches('"').to_string())
                .collect();
            push_array_entry(&mut root, &parts)
                .map_err(|e| format!("bad array header {inner}: {e}"))?;
            section = parts.clone();
            array_section = Some(parts);
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
            array_section = None;
            continue;
        }
        let Some(eq) = line.find('=') else {
            return Err(format!("bad line: {line}"));
        };
        let key = line[..eq].trim().trim_matches('"').to_string();
        let value = parse_scalar(line[eq + 1..].trim());
        if let Some(parts) = &array_section {
            // Append into the current array entry. `parts` may nest through
            // arrays-of-tables (e.g. `nav.children`), each of which is
            // addressed by its last element.
            if let Some(m) = array_section_map_mut(&mut root, parts) {
                m.insert(key, value);
            }
        } else {
            let mut path = section.clone();
            path.push(key);
            root.insert_path(&path.join("."), value);
        }
    }
    Ok(root)
}

/// Resolve the innermost map of an array-section path, descending through
/// nested arrays-of-tables by taking the last element of each array. Used to
/// fill in `key = value` lines under `[[nav]]` / `[[nav.children]]`.
fn array_section_map_mut<'a>(
    root: &'a mut Value,
    parts: &[String],
) -> Option<&'a mut BTreeMap<String, Value>> {
    fn walk<'a>(
        node: &'a mut Value,
        parts: &[String],
    ) -> Option<&'a mut BTreeMap<String, Value>> {
        if parts.is_empty() {
            // `node` is the array holding the entry's fields.
            let items = node.as_arr_mut()?;
            if items.is_empty() {
                items.push(Value::map());
            }
            return items.last_mut().and_then(map_mut);
        }
        let cur = descend_array(node);
        let m = map_mut(cur)?;
        let (head, tail) = parts.split_first().unwrap();
        let entry = m.entry(head.clone()).or_insert_with(Value::map);
        walk(entry, tail)
    }
    walk(root, parts)
}

/// When `node` is an array-of-tables, descend into its last element (the
/// "current" entry); otherwise return the node unchanged. Used while walking
/// an array-section path where every intermediate array is addressed by its
/// most recently pushed entry.
fn descend_array(node: &mut Value) -> &mut Value {
    match node {
        Value::Arr(items) => {
            if items.is_empty() {
                items.push(Value::map());
            }
            items.last_mut().unwrap()
        }
        other => other,
    }
}

/// Walk an array-of-tables header path and append a new (empty) map into the
/// array named by the final segment. Intermediate segments address the last
/// element of each array, so `[[nav.children]]` appends into the `children`
/// array of the most recently pushed `[[nav]]` entry.
fn push_array_entry(node: &mut Value, parts: &[String]) -> Result<(), String> {
    // If `node` is an array-of-tables, the remaining path lives inside its
    // last element (the "current" entry).
    let cur = descend_array(node);
    let m = map_mut(cur).ok_or_else(|| "expected a table".to_string())?;
    if parts.len() == 1 {
        let entry = m.entry(parts[0].clone()).or_insert_with(Value::map);
        if entry.as_arr_mut().is_none() {
            *entry = Value::Arr(Vec::new());
        }
        if let Value::Arr(items) = entry {
            items.push(Value::map());
        }
        Ok(())
    } else {
        let entry = m.entry(parts[0].clone()).or_insert_with(Value::map);
        push_array_entry(entry, &parts[1..])
    }
}

fn map_mut(v: &mut Value) -> Option<&mut BTreeMap<String, Value>> {
    match v {
        Value::Map(m) => Some(m),
        _ => None,
    }
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

/// Strip a `# comment` suffix from a YAML line. Honours both single and double
/// quoted strings so a literal `#` inside a value is preserved.
fn strip_yaml_comment(line: &str) -> &str {
    let mut in_str = false;
    let mut in_single = false;
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'"' && !in_single {
            in_str = !in_str;
        } else if b == b'\'' && !in_str {
            in_single = !in_single;
        } else if b == b'#' && !in_str && !in_single {
            // Only treat # as a comment when preceded by whitespace or at
            // start of line — bare `key:#value` is rare in YAML keys but
            // skipping the whitespace check keeps things lenient.
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
    // Lenient TOML-style separator: `key = value` works in a `---` block too
    // (it only applies when no `:` was found, so `url: /p?x=1` keeps working).
    if let Some(idx) = line.find('=') {
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
    fn yaml_accepts_toml_style_equals_separator() {
        let fm = parse_yaml("aliases = [\"about-us\", \"contact\"]\n");
        let aliases = fm.get("aliases").unwrap().as_arr().unwrap();
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0].as_str(), Some("about-us"));
        assert_eq!(aliases[1].as_str(), Some("contact"));
        // A colon value containing `=` is untouched.
        let fm = parse_yaml("url: /p?x=1\n");
        assert_eq!(fm.get("url").unwrap().as_str(), Some("/p?x=1"));
    }

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
        assert_eq!(
            lang.get("en")
                .unwrap()
                .as_map()
                .unwrap()
                .get("title")
                .unwrap()
                .as_str(),
            Some("English")
        );
    }

    #[test]
    fn parses_nested_array_of_tables() {
        let t = r#"
        [[nav]]
        title = "Docs"
        [[nav.children]]
        title = "Guide"
        url = "/guide/"
        [[nav.children]]
        title = "API"
        url = "/api/"
        [[nav]]
        title = "Blog"
        url = "/posts/"
        "#;
        let v = parse_toml(t).unwrap();
        let m = v.as_map().unwrap();
        let nav = m.get("nav").unwrap().as_arr().unwrap();
        assert_eq!(nav.len(), 2);
        let first = nav[0].as_map().unwrap();
        assert_eq!(first.get("title").unwrap().as_str(), Some("Docs"));
        let children = first.get("children").unwrap().as_arr().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(
            children[1]
                .as_map()
                .unwrap()
                .get("url")
                .unwrap()
                .as_str(),
            Some("/api/")
        );
        let second = nav[1].as_map().unwrap();
        assert_eq!(second.get("title").unwrap().as_str(), Some("Blog"));
        assert!(second.get("children").is_none());
    }

    #[test]
    fn yaml_skips_comment_lines() {
        let src = "# header comment\ntitle: \"Hello\"\n# trailing comment\ndate: \"2024-01-01\"\n";
        let fm = parse_yaml(src);
        assert_eq!(fm.get("title").unwrap().as_str(), Some("Hello"));
        assert_eq!(fm.get("date").unwrap().as_str(), Some("2024-01-01"));
        assert_eq!(fm.len(), 2);
    }

    #[test]
    fn yaml_skips_inline_comments() {
        let src = "title: \"Hello\" # inline\ntags: [a, b] # tail\n";
        let fm = parse_yaml(src);
        assert_eq!(fm.get("title").unwrap().as_str(), Some("Hello"));
        let tags = fm.get("tags").unwrap().as_arr().unwrap();
        assert_eq!(tags.len(), 2);
    }

    #[test]
    fn yaml_preserves_hash_inside_quoted_string() {
        let src = "title: \"Hello # world\"\n";
        let fm = parse_yaml(src);
        assert_eq!(fm.get("title").unwrap().as_str(), Some("Hello # world"));
    }

    #[test]
    fn frontmatter_with_comments_renders() {
        let src = "---\n# comment\ntitle: \"Sample\"\n# another\n---\n\nbody";
        let (fm, body) = parse_frontmatter(src);
        assert_eq!(fm.get("title").unwrap().as_str(), Some("Sample"));
        assert_eq!(body.trim(), "body");
    }
}
