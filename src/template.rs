//! A small dependency-free template engine with `{{ var | filter }}` output,
//! `{% if %}`, `{% for %}`, `{% block %}`/`{% extends %}` inheritance,
//! `{% include "name" %}` (recursive).
//!
//! Expressions accept a dotted path plus a filter pipeline:
//!
//! ```text
//! {{ article.title | truncate:20:… }}
//! {{ a.date_iso | date:"%Y年%m月%d日" }}
//! {{ article.summary | replace:"mdweb":"md" | upper }}
//! {% for a in recent | limit:3 %}
//! {% if a.reading_minutes > 3 and a.lang == "en" %}
//! ```
//!
//! `if` supports `== != < > <= >=`, `and`/`or`/`not`, string/number
//! literals and paths. Context values are the crate's `Value` type.
//!
//! Filters — strings/values: `safe`, `upper`, `lower`, `trim`, `length`,
//! `slice:start:len`, `truncate:n:suffix` (suffix optional, default `…`),
//! `replace:from:to`, `date:format`. Arrays: `length`, `limit:n`,
//! `offset:n`, `reverse`, `sort`/`sort:field`, `sort_desc`/`sort_desc:field`.

use std::collections::BTreeMap;

use crate::value::Value;

#[derive(Debug, Clone)]
pub enum Node {
    Text(String),
    /// `expr` is the full filter-chained expression (e.g. `a.title | upper`).
    Out {
        expr: String,
    },
    If {
        cond: String,
        then: Vec<Node>,
        els: Vec<Node>,
    },
    For {
        var: String,
        iter: String,
        body: Vec<Node>,
    },
    Block {
        name: String,
        body: Vec<Node>,
    },
    Include {
        name: String,
    },
}

#[derive(Debug, Clone)]
pub struct Template {
    pub nodes: Vec<Node>,
    pub extends: Option<String>,
}

#[derive(Debug)]
enum Lex {
    Text(String),
    Expr(String),
    If(String),
    For { var: String, iter: String },
    Block(String),
    Extends(String),
    Include(String),
    Else,
    EndIf,
    EndFor,
    EndBlock,
}

enum Frame {
    If {
        cond: String,
        then: Vec<Node>,
        els: Vec<Node>,
        saw_else: bool,
    },
    For {
        var: String,
        iter: String,
        body: Vec<Node>,
    },
    Block {
        name: String,
        body: Vec<Node>,
    },
}

pub struct Engine {
    templates: BTreeMap<String, Template>,
}

/// Maximum recursion depth for `{% include %}` to prevent stack overflow
/// from cyclic or self-referential partials.
const MAX_INCLUDE_DEPTH: usize = 32;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const MONTHS_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const WEEKDAYS_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

struct ParsedDate {
    y: u32,
    m: u32,
    d: u32,
    h: u32,
    min: u32,
    sec: u32,
}

impl ParsedDate {
    fn day_of_year(&self) -> u32 {
        let mut days = self.d;
        let mut mo = 1;
        while mo < self.m {
            days += crate::date::month_len(i64::from(self.y), mo);
            mo += 1;
        }
        days
    }

    /// ISO-8601 week number (`%V`) and its year (`%G`).
    fn iso_week(&self) -> (u32, u32) {
        let wd = crate::date::weekday(i64::from(self.y), self.m, self.d);
        let dow = (wd as u32 + 6) % 7 + 1; // 1 = Monday .. 7 = Sunday
        let mut y = self.y;
        let mut doy = self.day_of_year();
        let mut w = (doy - dow + 10) / 7;
        if w < 1 {
            y -= 1;
            doy += crate::date::days_in_year(i64::from(y));
            w = (doy - dow + 10) / 7;
        } else if w > 52 && doy - dow + 10 >= 7 * 53 {
            y += 1;
            w = 1;
        }
        (y, w)
    }
}

/// Parse `2026-08-02`, `2026-08-02T12:30:45Z` or `2026-08-02 12:30`.
fn parse_date_value(s: &str) -> Option<ParsedDate> {
    let s = s.trim();
    let s = s
        .strip_suffix('Z')
        .unwrap_or(s)
        .strip_suffix('z')
        .unwrap_or(s);
    let (date_part, time_part) = match s.find('T').or_else(|| s.find(' ')) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let mut dp = date_part.split('-');
    let y = dp.next()?.parse::<u32>().ok()?;
    let m = dp.next()?.parse::<u32>().ok()?;
    let d = dp.next()?.parse::<u32>().ok()?;
    if !(1..=12).contains(&m) || d == 0 || d > crate::date::month_len(i64::from(y), m) {
        return None;
    }
    let h = match time_part.split(':').next() {
        Some(v) if !v.is_empty() => v.parse::<u32>().ok()?,
        _ => 0,
    };
    let min = time_part
        .split(':')
        .nth(1)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let sec = time_part
        .split(':')
        .nth(2)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    Some(ParsedDate {
        y,
        m,
        d,
        h,
        min,
        sec,
    })
}

/// Format a date string with `%`-tokens (`%Y %y %m %e %d %H %I %M %S %p
/// %a %A %b %B %j %w %u %V %g %G %%`). Returns `None` when the input isn't a
/// parseable date so templates can fall back to the raw string.
fn date_format(s: &str, fmt: &str) -> Option<String> {
    let d = parse_date_value(s)?;
    let mut out = String::new();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let code = chars.get(i).copied().unwrap_or('%');
        match code {
            'Y' => out.push_str(&format!("{:04}", d.y)),
            'y' => out.push_str(&format!("{:02}", d.y % 100)),
            'm' => out.push_str(&format!("{:02}", d.m)),
            'e' => out.push_str(&format!("{}", d.d)),
            'd' => out.push_str(&format!("{:02}", d.d)),
            'H' => out.push_str(&format!("{:02}", d.h)),
            'I' => {
                if d.h % 12 == 0 {
                    out.push_str("12");
                } else {
                    out.push_str(&format!("{:02}", d.h % 12));
                }
            }
            'M' => out.push_str(&format!("{:02}", d.min)),
            'S' => out.push_str(&format!("{:02}", d.sec)),
            'p' => out.push_str(if d.h < 12 { "AM" } else { "PM" }),
            'a' => out.push_str(WEEKDAYS_SHORT[crate::date::weekday(i64::from(d.y), d.m, d.d)]),
            'A' => out.push_str(WEEKDAYS[crate::date::weekday(i64::from(d.y), d.m, d.d)]),
            'b' => out.push_str(MONTHS_SHORT[(d.m - 1) as usize]),
            'B' => out.push_str(MONTHS[(d.m - 1) as usize]),
            'j' => out.push_str(&format!("{:03}", d.day_of_year())),
            'w' => out.push_str(&format!("{}", crate::date::weekday(i64::from(d.y), d.m, d.d))),
            'u' => {
                let w = crate::date::weekday(i64::from(d.y), d.m, d.d);
                out.push_str(&format!("{}", if w == 0 { 7 } else { w }));
            }
            'V' => {
                let (_, w) = d.iso_week();
                out.push_str(&format!("{:02}", w));
            }
            'g' => {
                let (y, _) = d.iso_week();
                out.push_str(&format!("{:02}", y % 100));
            }
            'G' => {
                let (y, _) = d.iso_week();
                out.push_str(&format!("{:04}", y));
            }
            '%' => out.push('%'),
            _ => {
                out.push('%');
                out.push(code);
            }
        }
        i += 1;
    }
    Some(out)
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Engine {
        Engine {
            templates: BTreeMap::new(),
        }
    }

    /// Register a template by name, parsing its source.
    pub fn add(&mut self, name: &str, source: &str) -> Result<(), String> {
        let mut nodes = Vec::new();
        let mut extends = None;
        let mut stack = Vec::new();
        for lex in lex(source)? {
            build(lex, &mut nodes, &mut extends, &mut stack)?;
        }
        let tpl = Template { nodes, extends };
        self.templates.insert(name.to_string(), tpl);
        Ok(())
    }

    /// Register several templates at once.
    pub fn add_many(&mut self, items: Vec<(&str, String)>) -> Result<(), String> {
        for (name, src) in items {
            self.add(name, &src)?;
        }
        Ok(())
    }

    pub fn has(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }

    /// Resolve the extends chain (leaf first, root base last).
    fn chain(&self, name: &str, mut acc: Vec<String>) -> Vec<String> {
        if acc.contains(&name.to_string()) {
            return acc;
        }
        acc.push(name.to_string());
        if let Some(t) = self.templates.get(name) {
            if let Some(base) = &t.extends {
                if self.templates.contains_key(base) {
                    return self.chain(base, acc);
                }
            }
        }
        acc
    }

    fn collect_blocks(&self, name: &str) -> Vec<(String, Vec<Node>)> {
        let mut out = Vec::new();
        if let Some(t) = self.templates.get(name) {
            for n in &t.nodes {
                if let Node::Block { name: bn, body } = n {
                    out.push((bn.clone(), body.clone()));
                }
            }
        }
        out
    }

    /// Render a named template with the given context.
    pub fn render(&self, name: &str, ctx: &Value) -> Result<String, String> {
        self.render_with_depth(name, ctx, 0)
    }

    fn render_with_depth(&self, name: &str, ctx: &Value, depth: usize) -> Result<String, String> {
        if depth > MAX_INCLUDE_DEPTH {
            return Err(format!(
                "include depth exceeded ({MAX_INCLUDE_DEPTH}): probable cycle starting at \"{name}\""
            ));
        }
        if !self.templates.contains_key(name) {
            return Err(format!("template not found: {name}"));
        }
        let chain = self.chain(name, Vec::new());
        let root = chain.last().cloned().unwrap_or_else(|| name.to_string());

        // Walk base -> leaf so leaf overrides win.
        let mut blocks: BTreeMap<String, Vec<Node>> = BTreeMap::new();
        for t in chain.iter().rev() {
            for (bn, body) in self.collect_blocks(t) {
                blocks.insert(bn, body);
            }
        }
        let t = self.templates.get(&root).unwrap();
        let mut out = String::new();
        self.render_nodes(&t.nodes, ctx, &blocks, depth, &mut out)?;
        Ok(out)
    }

    fn render_nodes(
        &self,
        nodes: &[Node],
        ctx: &Value,
        blocks: &BTreeMap<String, Vec<Node>>,
        depth: usize,
        out: &mut String,
    ) -> Result<(), String> {
        for n in nodes {
            match n {
                Node::Text(t) => out.push_str(t),
                Node::Out { expr } => {
                    let (value, safe) = eval_expr(ctx, expr);
                    let render = value.render();
                    if safe {
                        out.push_str(&render);
                    } else {
                        out.push_str(&escape_html(&render));
                    }
                }
                Node::If { cond, then, els } => {
                    if eval_cond(ctx, cond) {
                        self.render_nodes(then, ctx, blocks, depth, out)?;
                    } else {
                        self.render_nodes(els, ctx, blocks, depth, out)?;
                    }
                }
                Node::For { var, iter, body } => {
                    let (value, _) = eval_expr(ctx, iter);
                    let arr = match value {
                        Value::Arr(a) => a,
                        _ => continue,
                    };
                    for (i, item) in arr.iter().enumerate() {
                        if let Value::Map(base) = ctx {
                            let mut child = base.clone();
                            child.insert(var.clone(), item.clone());
                            child.insert(format!("{var}_index"), Value::int(i as i64));
                            child.insert(format!("{var}_length"), Value::int(arr.len() as i64));
                            let child_ctx = Value::Map(child);
                            self.render_nodes(body, &child_ctx, blocks, depth, out)?;
                        } else {
                            self.render_nodes(body, &Value::Null, blocks, depth, out)?;
                        }
                    }
                }
                Node::Block { name, body } => {
                    if let Some(override_body) = blocks.get(name) {
                        self.render_nodes(override_body, ctx, blocks, depth, out)?;
                    } else {
                        self.render_nodes(body, ctx, blocks, depth, out)?;
                    }
                }
                Node::Include { name } => {
                    let partial = self.render_with_depth(name, ctx, depth + 1)?;
                    out.push_str(&partial);
                }
            }
        }
        Ok(())
    }
}

fn escape_html(s: &str) -> String {
    crate::html::escape_attr(s)
}

/// Evaluate a full filter-chained expression. Returns the value and whether
/// the pipeline ended with the `safe` filter.
fn eval_expr(ctx: &Value, expr: &str) -> (Value, bool) {
    let mut parts = expr.split('|');
    let operand = parts.next().unwrap_or("").trim().to_string();
    let mut value = ctx.path(&operand).cloned().unwrap_or(Value::Null);
    if matches!(value, Value::Null) {
        value = eval_operand(ctx, &operand);
    }
    let mut safe = false;
    for f in parts {
        let f = f.trim();
        let (name, args) = match f.split_once(':') {
            Some((n, rest)) => (n, split_args(rest)),
            None => (f, Vec::new()),
        };
        apply_filter(name, &mut value, &args, &mut safe);
    }
    (value, safe)
}

fn apply_filter(name: &str, v: &mut Value, args: &[String], safe: &mut bool) {
    match name {
        "safe" => *safe = true,
        "upper" => {
            if let Value::Str(s) = v {
                *s = s.to_uppercase();
            }
        }
        "lower" => {
            if let Value::Str(s) = v {
                *s = s.to_lowercase();
            }
        }
        "trim" => {
            if let Value::Str(s) = v {
                *s = s.trim().to_string();
            }
        }
        "length" => {
            let n = match &*v {
                Value::Str(s) => s.chars().count() as i64,
                Value::Arr(a) => a.len() as i64,
                other => other.render().len() as i64,
            };
            *v = Value::int(n);
        }
        "slice" => {
            if let Value::Str(s) = v {
                let chars: Vec<char> = s.chars().collect();
                let start: usize = args.first().and_then(|x| x.parse().ok()).unwrap_or(0);
                let taken: Vec<char> = if args.len() > 1 {
                    let len: usize = args.get(1).and_then(|x| x.parse().ok()).unwrap_or(0);
                    chars.into_iter().skip(start).take(len).collect()
                } else {
                    chars.into_iter().skip(start).collect()
                };
                *s = taken.into_iter().collect();
            }
        }
        "truncate" => {
            if let Value::Str(s) = v {
                let max: usize = args.first().and_then(|x| x.parse().ok()).unwrap_or(0);
                let suffix = args.get(1).map(String::as_str).unwrap_or("…");
                if s.chars().count() > max {
                    let chars: Vec<char> = s.chars().take(max).collect();
                    *s = format!("{}{}", chars.into_iter().collect::<String>(), suffix);
                }
            }
        }
        "replace" => {
            if let Value::Str(s) = v {
                if args.len() >= 2 {
                    *s = s.replace(&args[0], &args[1]);
                }
            }
        }
        "date" => {
            if let Value::Str(s) = v {
                if let Some(fmt) = args.first() {
                    if let Some(fmt) = date_format(s, fmt) {
                        *s = fmt;
                    }
                }
            }
        }
        "limit" => {
            if let Value::Arr(a) = v {
                let n = args
                    .first()
                    .and_then(|x| x.parse::<usize>().ok())
                    .unwrap_or(0);
                a.truncate(n);
            }
        }
        "offset" => {
            if let Value::Arr(a) = v {
                let n = args
                    .first()
                    .and_then(|x| x.parse::<usize>().ok())
                    .unwrap_or(0);
                if n <= a.len() {
                    a.drain(..n);
                } else {
                    a.clear();
                }
            }
        }
        "reverse" => {
            if let Value::Arr(a) = v {
                a.reverse();
            }
        }
        "sort" => sort_slice(arr_mut(v), args.first().map(String::as_str), false),
        "sort_desc" => sort_slice(arr_mut(v), args.first().map(String::as_str), true),
        _ => {}
    }
}

/// Borrow the array under `v`, replacing non-arrays with an empty one.
fn arr_mut(v: &mut Value) -> &mut Vec<Value> {
    if v.as_arr_mut().is_some() {
        return v.as_arr_mut().unwrap();
    }
    *v = Value::Arr(Vec::new());
    v.as_arr_mut().unwrap()
}

fn sort_slice(a: &mut [Value], field: Option<&str>, desc: bool) {
    a.sort_by(|x, y| {
        let kx = match field {
            Some(f) => x.path(f).cloned().unwrap_or_else(|| x.clone()),
            None => x.clone(),
        };
        let ky = match field {
            Some(f) => y.path(f).cloned().unwrap_or_else(|| y.clone()),
            None => y.clone(),
        };
        let ord = cmp_value(&kx, &ky);
        if desc {
            ord.reverse()
        } else {
            ord
        }
    });
}

fn cmp_value(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Int(x), Value::Str(y)) => match y.parse::<i64>() {
            Ok(yy) => x.cmp(&yy),
            Err(_) => x.to_string().cmp(y),
        },
        (Value::Str(x), Value::Int(y)) => match x.parse::<i64>() {
            Ok(xx) => xx.cmp(y),
            Err(_) => x.cmp(&y.to_string()),
        },
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,
        _ => a.render().cmp(&b.render()),
    }
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// Split filter arguments on `:` but keep `:` inside double-quoted strings,
/// so formats like `date:"%Y-%m-%d %H:%M"` stay whole.
fn split_args(rest: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    for c in rest.chars() {
        if c == '"' {
            in_q = !in_q;
            cur.push(c);
        } else if c == ':' && !in_q {
            args.push(unquote(&cur));
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    args.push(unquote(&cur));
    args
}

/// Evaluate a bare operand: a path, a quoted string, `true`/`false`,
/// `null`/`none` or an integer literal.
fn eval_operand(ctx: &Value, expr: &str) -> Value {
    let t = expr.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        return Value::str(unquote(t));
    }
    match t {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        "null" | "none" | "" => return Value::Null,
        _ => {}
    }
    if let Ok(i) = t.parse::<i64>() {
        return Value::int(i);
    }
    ctx.path(t).cloned().unwrap_or(Value::Null)
}

/// Evaluate a `{% if %}` condition. Supports `and`/`or`/`not` plus
/// comparisons on filter-chained paths or literals.
fn eval_cond(ctx: &Value, cond: &str) -> bool {
    for or_part in cond.split(" or ") {
        let mut all = true;
        for and_part in or_part.split(" and ") {
            if !eval_cond_term(ctx, and_part.trim()) {
                all = false;
                break;
            }
        }
        if all {
            return true;
        }
    }
    false
}

fn eval_cond_term(ctx: &Value, term: &str) -> bool {
    let term = term.trim();
    if let Some(rest) = term.strip_prefix("not ") {
        return !eval_cond_term(ctx, rest.trim());
    }
    if let Some((l, r, op)) = split_comparison(term) {
        let lv = eval_cond_operand(ctx, l.trim());
        let rv = eval_cond_operand(ctx, r.trim());
        return compare_values(lv, rv, op);
    }
    eval_cond_operand(ctx, term).truthy()
}

/// An operand in a comparison may itself carry filters (`name | length > 3`).
fn eval_cond_operand(ctx: &Value, t: &str) -> Value {
    eval_expr(ctx, t).0
}

/// Find the earliest comparison operator in a term. Picks the leftmost and
/// orders the two-char ops before single ones so `>=` wins over `>`.
fn split_comparison(term: &str) -> Option<(&str, &str, &str)> {
    let ops = [">=", "<=", "!=", "==", "<", ">"];
    let mut best: Option<(usize, &str)> = None;
    for op in ops {
        let mut search = 0;
        while let Some(i) = term[search..].find(op) {
            let i = search + i;
            match best {
                None => best = Some((i, op)),
                Some((bi, _)) if i < bi => best = Some((i, op)),
                Some((bi, bop)) if i == bi && op.len() > bop.len() => best = Some((i, op)),
                _ => {}
            }
            search = i + 1;
        }
    }
    best.map(|(i, op)| (&term[..i], &term[i + op.len()..], op))
}

fn compare_values(a: Value, b: Value, op: &str) -> bool {
    let ord = cmp_value(&a, &b);
    let eq = a == b;
    match op {
        "==" => eq,
        "!=" => !eq,
        "<" => ord == std::cmp::Ordering::Less,
        ">" => ord == std::cmp::Ordering::Greater,
        "<=" => ord != std::cmp::Ordering::Greater,
        ">=" => ord != std::cmp::Ordering::Less,
        _ => false,
    }
}

fn lex(source: &str) -> Result<Vec<Lex>, String> {
    let mut toks = Vec::new();
    let mut i = 0;
    while i < source.len() {
        let rest = &source[i..];
        let mut first: Option<(usize, &str)> = None;
        for marker in ["{{", "{%", "{#"] {
            if let Some(idx) = rest.find(marker) {
                match first {
                    None => first = Some((idx, marker)),
                    Some((bi, _)) => {
                        if idx < bi {
                            first = Some((idx, marker));
                        }
                    }
                }
            }
        }
        let (idx, marker) = match first {
            Some(v) => v,
            None => {
                if !rest.is_empty() {
                    toks.push(Lex::Text(rest.to_string()));
                }
                break;
            }
        };
        if idx > 0 {
            toks.push(Lex::Text(rest[..idx].to_string()));
        }
        let after = &rest[idx..];
        let (content, consumed) = match marker {
            "{{" => {
                let end = after.find("}}").ok_or("unterminated {{")?;
                let c = &after[2..end];
                (c, end + 2)
            }
            "{%" => {
                let end = after.find("%}").ok_or("unterminated {%")?;
                let c = &after[2..end];
                (c, end + 2)
            }
            _ => {
                let end = after.find("#}").ok_or("unterminated {#")?;
                ("", end + 2)
            }
        };
        if marker != "{#" {
            toks.push(parse_expr_token(marker, content.trim()));
        }
        i += idx + consumed;
    }
    Ok(toks)
}

fn parse_expr_token(marker: &str, content: &str) -> Lex {
    if marker == "{{" {
        Lex::Expr(content.to_string())
    } else {
        // directive
        if content.starts_with("endblock") {
            Lex::EndBlock
        } else if content.starts_with("endif") {
            Lex::EndIf
        } else if content.starts_with("endfor") {
            Lex::EndFor
        } else if content.starts_with("else") {
            Lex::Else
        } else if let Some(cond) = content.strip_prefix("if") {
            Lex::If(cond.trim().to_string())
        } else if let Some(name) = content.strip_prefix("block") {
            Lex::Block(unquote(name.trim()))
        } else if let Some(base) = content.strip_prefix("extends") {
            Lex::Extends(unquote(base.trim()))
        } else if let Some(name) = content.strip_prefix("include") {
            Lex::Include(unquote(name.trim()))
        } else if let Some(body) = content.strip_prefix("for") {
            parse_for(body)
        } else {
            Lex::Text(String::new()) // ignore unknown
        }
    }
}

fn parse_for(body: &str) -> Lex {
    let body = body.trim();
    match body.split_once(" in ") {
        Some((var, iter)) => Lex::For {
            var: var.trim().to_string(),
            iter: iter.trim().to_string(),
        },
        None => Lex::For {
            var: body.to_string(),
            iter: body.to_string(),
        },
    }
}

fn build(
    lex: Lex,
    nodes: &mut Vec<Node>,
    extends: &mut Option<String>,
    stack: &mut Vec<Frame>,
) -> Result<(), String> {
    match lex {
        Lex::Text(t) => push_list(nodes, Node::Text(t), stack),
        Lex::Expr(expr) => push_list(nodes, Node::Out { expr }, stack),
        Lex::Extends(base) => {
            if extends.is_none() {
                *extends = Some(base);
            }
            Ok(())
        }
        Lex::Include(name) => push_list(nodes, Node::Include { name }, stack),
        Lex::If(cond) => {
            stack.push(Frame::If {
                cond,
                then: Vec::new(),
                els: Vec::new(),
                saw_else: false,
            });
            Ok(())
        }
        Lex::Else => {
            match stack.last_mut() {
                Some(Frame::If { saw_else, .. }) if !*saw_else => {
                    *saw_else = true;
                }
                _ => return Err("unexpected else".into()),
            }
            Ok(())
        }
        Lex::EndIf => {
            let frame = stack.pop().ok_or("unexpected endif")?;
            let Frame::If {
                cond, then, els, ..
            } = frame
            else {
                return Err("mismatched endif".into());
            };
            push_list(nodes, Node::If { cond, then, els }, stack)
        }
        Lex::For { var, iter } => {
            stack.push(Frame::For {
                var,
                iter,
                body: Vec::new(),
            });
            Ok(())
        }
        Lex::EndFor => {
            let frame = stack.pop().ok_or("unexpected endfor")?;
            let Frame::For { var, iter, body } = frame else {
                return Err("mismatched endfor".into());
            };
            push_list(nodes, Node::For { var, iter, body }, stack)
        }
        Lex::Block(name) => {
            stack.push(Frame::Block {
                name,
                body: Vec::new(),
            });
            Ok(())
        }
        Lex::EndBlock => {
            let frame = stack.pop().ok_or("unexpected endblock")?;
            let Frame::Block { name, body } = frame else {
                return Err("mismatched endblock".into());
            };
            push_list(nodes, Node::Block { name, body }, stack)
        }
    }
}

fn push_list(nodes: &mut Vec<Node>, node: Node, stack: &mut [Frame]) -> Result<(), String> {
    if let Some(top) = stack.last_mut() {
        match top {
            Frame::If {
                then,
                els,
                saw_else,
                ..
            } => {
                if *saw_else {
                    els.push(node);
                } else {
                    then.push(node);
                }
            }
            Frame::For { body, .. } => body.push(node),
            Frame::Block { body, .. } => body.push(node),
        }
    } else {
        nodes.push(node);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_template_files_parse() {
        let files = [
            (
                "base.html",
                include_str!("../site/template/default/base.html"),
            ),
            (
                "index.html",
                include_str!("../site/template/default/index.html"),
            ),
            (
                "category.html",
                include_str!("../site/template/default/category.html"),
            ),
            (
                "article.html",
                include_str!("../site/template/default/article.html"),
            ),
            (
                "page.html",
                include_str!("../site/template/default/page.html"),
            ),
            (
                "404.html",
                include_str!("../site/template/default/404.html"),
            ),
            (
                "layout/header.html",
                include_str!("../site/template/default/layout/header.html"),
            ),
            (
                "layout/side.html",
                include_str!("../site/template/default/layout/side.html"),
            ),
        ];
        for (name, src) in files {
            let mut e = Engine::new();
            if let Err(err) = e.add(name, src) {
                panic!("{name} failed: {err}");
            }
        }
    }

    fn render_str(src: &str, ctx: &Value) -> String {
        let mut e = Engine::new();
        e.add("t", src).unwrap();
        e.render("t", ctx).unwrap()
    }

    #[test]
    fn filters_and_comparisons() {
        let ctx = Value::Map(BTreeMap::from([(
            "name".to_string(),
            Value::str("Hello World"),
        )]));
        assert_eq!(render_str("{{ name | truncate:5 }}", &ctx), "Hello…");
        assert_eq!(render_str("{{ name | truncate:5:... }}", &ctx), "Hello...");
        assert_eq!(render_str("{{ name | slice:0:5 }}", &ctx), "Hello");
        assert_eq!(render_str("{{ name | slice:6 }}", &ctx), "World");
        assert_eq!(
            render_str("{{ name | replace:\"World\":\"Rust\" }}", &ctx),
            "Hello Rust"
        );
        assert_eq!(render_str("{{ name | upper }}", &ctx), "HELLO WORLD");
        assert_eq!(render_str("{{ name | lower }}", &ctx), "hello world");
        assert_eq!(
            render_str(
                "{% if name | length > 10 %}big{% else %}small{% endif %}",
                &ctx
            ),
            "big"
        );
        assert_eq!(
            render_str("{% if name == \"Hello World\" %}eq{% endif %}", &ctx),
            "eq"
        );
        assert_eq!(
            render_str(
                "{% if name != \"x\" and name | length >= 5 %}ok{% endif %}",
                &ctx
            ),
            "ok"
        );
        assert_eq!(
            render_str("{% if not name == \"x\" or 1 > 2 %}ok{% endif %}", &ctx),
            "ok"
        );
        assert_eq!(
            render_str("{% if 3 <= 3 and 2 or 1 < 0 %}ok{% endif %}", &ctx),
            "ok"
        );
        assert_eq!(render_str("{% if 1 < 2 %}yes{% endif %}", &ctx), "yes");
    }

    #[test]
    fn array_loop_filters() {
        let ctx = Value::Map(BTreeMap::from([(
            "items".to_string(),
            Value::Arr(vec![
                Value::str("b"),
                Value::str("a"),
                Value::str("c"),
                Value::str("d"),
                Value::str("e"),
            ]),
        )]));
        assert_eq!(
            render_str(
                "{% for x in items | sort | limit:2 %}{{ x }}{% endfor %}",
                &ctx
            ),
            "ab"
        );
        assert_eq!(
            render_str(
                "{% for x in items | sort_desc | limit:1 %}{{ x }}{% endfor %}",
                &ctx
            ),
            "e"
        );
        assert_eq!(
            render_str("{% for x in items | offset:2 %}{{ x }}{% endfor %}", &ctx),
            "cde"
        );
        assert_eq!(
            render_str(
                "{% for x in items | reverse | limit:1 %}{{ x }}{% endfor %}",
                &ctx
            ),
            "e"
        );
        // sort:field orders map elements by a named key.
        let ctx2 = Value::Map(BTreeMap::from([(
            "rows".to_string(),
            Value::Arr(vec![
                Value::Map(BTreeMap::from([
                    ("title".to_string(), Value::str("B")),
                    ("n".to_string(), Value::int(2)),
                ])),
                Value::Map(BTreeMap::from([
                    ("title".to_string(), Value::str("A")),
                    ("n".to_string(), Value::int(1)),
                ])),
            ]),
        )]));
        assert_eq!(
            render_str(
                "{% for r in rows | sort:title %}{{ r.title }}{% endfor %}",
                &ctx2
            ),
            "AB"
        );
        assert_eq!(
            render_str(
                "{% for r in rows | sort_desc:n %}{{ r.title }}{% endfor %}",
                &ctx2
            ),
            "BA"
        );
    }

    #[test]
    fn date_filter() {
        let null = Value::Null;
        assert_eq!(
            render_str("{{ \"2026-08-02\" | date:\"%Y/%m/%d\" }}", &null),
            "2026/08/02"
        );
        assert_eq!(
            render_str(
                "{{ \"2026-08-02T14:30:05Z\" | date:\"%Y-%m-%d %H:%M\" }}",
                &null
            ),
            "2026-08-02 14:30"
        );
        assert_eq!(
            render_str("{{ \"2026-08-02\" | date:\"%A\" }}", &null),
            "Sunday"
        );
    }
}
