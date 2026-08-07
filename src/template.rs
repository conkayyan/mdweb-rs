//! A small dependency-free template engine with `{{ var }}` output,
//! `{% if %}`, `{% for %}`, `{% block %}`/`{% extends %}` inheritance, and a
//! single `| safe` filter. Context values are the crate's `Value` type.

use std::collections::BTreeMap;

use crate::value::Value;

#[derive(Debug, Clone)]
pub enum Node {
    Text(String),
    Out { expr: String, safe: bool },
    If { cond: String, then: Vec<Node>, els: Vec<Node> },
    For { var: String, iter: String, body: Vec<Node> },
    Block { name: String, body: Vec<Node> },
}

#[derive(Debug, Clone)]
pub struct Template {
    pub nodes: Vec<Node>,
    pub extends: Option<String>,
}

#[derive(Debug)]
enum Lex {
    Text(String),
    Expr { expr: String, safe: bool },
    If(String),
    For { var: String, iter: String },
    Block(String),
    Extends(String),
    Else,
    EndIf,
    EndFor,
    EndBlock,
}

enum Frame {
    If { cond: String, then: Vec<Node>, els: Vec<Node>, saw_else: bool },
    For { var: String, iter: String, body: Vec<Node> },
    Block { name: String, body: Vec<Node> },
}

pub struct Engine {
    templates: BTreeMap<String, Template>,
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
        if !self.templates.contains_key(name) {
            return Err(format!("template not found: {name}"));
        }
        let chain = self.chain(name, Vec::new());
        let root = chain
            .last()
            .cloned()
            .unwrap_or_else(|| name.to_string());

        // Walk base -> leaf so leaf overrides win.
        let mut blocks: BTreeMap<String, Vec<Node>> = BTreeMap::new();
        for t in chain.iter().rev() {
            for (bn, body) in self.collect_blocks(t) {
                blocks.insert(bn, body);
            }
        }
        let t = self.templates.get(&root).unwrap();
        let mut out = String::new();
        render_nodes(&t.nodes, ctx, &blocks, &mut out)?;
        Ok(out)
    }
}

fn render_nodes(
    nodes: &[Node],
    ctx: &Value,
    blocks: &BTreeMap<String, Vec<Node>>,
    out: &mut String,
) -> Result<(), String> {
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(t),
            Node::Out { expr, safe } => {
                let val = ctx.path(expr).cloned().unwrap_or(Value::Null);
                // prevent deep value drop
                let render = val.render();
                if *safe {
                    out.push_str(&render);
                } else {
                    out.push_str(&escape_html(&render));
                }
            }
            Node::If { cond, then, els } => {
                let val = ctx.path(cond).cloned().unwrap_or(Value::Null);
                if val.truthy() {
                    render_nodes(then, ctx, blocks, out)?;
                } else {
                    render_nodes(els, ctx, blocks, out)?;
                }
            }
            Node::For { var, iter, body } => {
                let val = ctx.path(iter).cloned().unwrap_or(Value::Null);
                let arr = match val {
                    Value::Arr(a) => a,
                    _ => continue,
                };
                for (i, item) in arr.iter().enumerate() {
                    if let Value::Map(base) = ctx {
                        let mut child = base.clone();
                        child.insert(var.clone(), item.clone());
                        child.insert(
                            format!("{var}_index"),
                            Value::int(i as i64),
                        );
                        let child_ctx = Value::Map(child);
                        render_nodes(body, &child_ctx, blocks, out)?;
                    } else {
                        render_nodes(body, &Value::Null, blocks, out)?;
                    }
                }
            }
            Node::Block { name, body } => {
                if let Some(override_body) = blocks.get(name) {
                    render_nodes(override_body, ctx, blocks, out)?;
                } else {
                    render_nodes(body, ctx, blocks, out)?;
                }
            }
        }
    }
    Ok(())
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
        let mut parts = content.split('|');
        let expr = parts.next().unwrap_or("").trim().to_string();
        let safe = parts.any(|f| f.trim() == "safe");
        Lex::Expr { expr, safe }
    } else {
        // directive
        if content.starts_with("endblock") {
            Lex::EndBlock
        } else if content.starts_with("endif") {
            Lex::EndIf
        } else if let Some(_) = content.strip_prefix("endfor") {
            Lex::EndFor
        } else if let Some(_) = content.strip_prefix("else") {
            Lex::Else
        } else if let Some(cond) = content.strip_prefix("if") {
            Lex::If(cond.trim().to_string())
        } else if let Some(name) = content.strip_prefix("block") {
            Lex::Block(unquote(name.trim()))
        } else if let Some(base) = content.strip_prefix("extends") {
            Lex::Extends(unquote(base.trim()))
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
        None => Lex::For { var: body.to_string(), iter: body.to_string() },
    }
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
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
        Lex::Expr { expr, safe } => push_list(nodes, Node::Out { expr, safe }, stack),
        Lex::Extends(base) => {
            if extends.is_none() {
                *extends = Some(base);
            }
            Ok(())
        }
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
            let Frame::If { cond, then, els, .. } = frame else {
                return Err("mismatched endif".into());
            };
            push_list(nodes, Node::If { cond, then, els }, stack)
        }
        Lex::For { var, iter } => {
            stack.push(Frame::For { var, iter, body: Vec::new() });
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
            stack.push(Frame::Block { name, body: Vec::new() });
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

fn push_list(
    nodes: &mut Vec<Node>,
    node: Node,
    stack: &mut Vec<Frame>,
) -> Result<(), String> {
    if let Some(top) = stack.last_mut() {
        match top {
            Frame::If { then, els, saw_else, .. } => {
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
            ("base.html", include_str!("../template/base.html")),
            ("index.html", include_str!("../template/index.html")),
            ("category.html", include_str!("../template/category.html")),
            ("article.html", include_str!("../template/article.html")),
            ("page.html", include_str!("../template/page.html")),
            ("404.html", include_str!("../template/404.html")),
        ];
        for (name, src) in files {
            let mut e = Engine::new();
            if let Err(err) = e.add(name, src) {
                panic!("{name} failed: {err}");
            }
        }
    }
}
