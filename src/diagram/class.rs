//! Mermaid `classDiagram` → SVG renderer.
//!
//! Supported syntax:
//! ```text
//! classDiagram
//!   class Animal {
//     +name: string
//!     +age: int
//!     +makeSound() void
//!   }
//!   class Dog {
//     +breed: string
//!   }
//!   Animal <|-- Dog
//!   Dog *-- Owner : has
//! ```
//!
//! Class boxes are stacked vertically with the same column layout
//! Mermaid uses. Relationships are anchored to the side midpoint
//! facing the other class. Returns `None` when no classes are found.

use super::common::{approx_text_width, escape_text, palette_color, shape_path};
use std::collections::HashMap;

/// Render a Mermaid classDiagram source to SVG. `None` if no classes.
pub fn render(src: &str) -> Option<String> {
    let mut title = String::new();
    let mut classes: Vec<ClassDef> = Vec::new();
    let mut rels: Vec<Relation> = Vec::new();
    let mut idx: HashMap<String, usize> = HashMap::new();

    let mut in_class: Option<usize> = None;
    for raw in src.lines() {
        let line = super::common::strip_comments(raw).trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("classdiagram") {
            continue;
        }
        if lower.starts_with("title") {
            title = line[5..].trim().to_string();
            continue;
        }
        // Class block: `class Name {` or `class Name` (single line).
        if lower.starts_with("class ") || lower.starts_with("class\t") {
            let rest = line["class".len()..].trim();
            if rest.ends_with('{') {
                let name = rest[..rest.len() - 1].trim().to_string();
                if !idx.contains_key(&name) {
                    classes.push(ClassDef::new(name.clone()));
                    idx.insert(name.clone(), classes.len() - 1);
                }
                in_class = idx.get(&name).copied();
                continue;
            } else {
                let name = rest.trim().to_string();
                if !name.is_empty() && !idx.contains_key(&name) {
                    classes.push(ClassDef::new(name.clone()));
                    idx.insert(name.clone(), classes.len() - 1);
                }
                in_class = idx.get(&name).copied();
                continue;
            }
        }
        if line == "}" {
            in_class = None;
            continue;
        }
        if let Some(ci) = in_class {
            // Member inside a class block.
            classes[ci].members.push(parse_member(line));
            continue;
        }
        // Relationship line.
        if let Some(rel) = parse_relation(line, &idx) {
            rels.push(rel);
        }
    }
    if classes.is_empty() {
        return None;
    }

    layout(&mut classes, &rels);
    let mut out = String::new();
    let (mw, mh) = dimensions(&classes, title.is_empty());
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         width=\"{mw:.0}\" height=\"{mh:.0}\" \
         viewBox=\"0 0 {mw:.0} {mh:.0}\" \
         style=\"max-width:100%;height:auto;\" \
         role=\"img\" aria-label=\"class diagram\">"
    ));
    if !title.is_empty() {
        out.push_str(&format!(
            "<text x=\"{:.0}\" y=\"28\" font-size=\"16\" font-weight=\"600\" \
             font-family=\"sans-serif, Noto Sans CJK SC, Microsoft YaHei, PingFang SC, Hiragino Sans GB, Source Han Sans SC, WenQuanYi Micro Hei\" fill=\"#24292f\">{}</text>",
            mw / 2.0,
            escape_text(&title)
        ));
    }
    // Edges first so boxes overlap them at the boundary.
    for r in &rels {
        if let Some(s) = render_relation(r, &classes) {
            out.push_str(&s);
        }
    }
    for (i, c) in classes.iter().enumerate() {
        out.push_str(&render_class(c, i));
    }
    out.push_str("</svg>");
    Some(out)
}

#[derive(Clone)]
struct ClassDef {
    name: String,
    members: Vec<Member>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl ClassDef {
    fn new(name: String) -> Self {
        ClassDef {
            name,
            members: Vec::new(),
            x: 0.0,
            y: 0.0,
            w: 160.0,
            h: 60.0,
        }
    }
}

#[derive(Clone)]
struct Member {
    vis: Visibility,
    name: String,
    /// `true` if this is a method (has `()`).
    is_method: bool,
    /// Optional return type as Mermaid writes it (`: Type`).
    ty: String,
}

#[derive(Clone, Copy)]
enum Visibility {
    Public,
    Private,
    Protected,
    Package,
}

#[derive(Clone, Copy, PartialEq)]
enum RelKind {
    Inheritance, // <|--
    Composition, // *--
    Aggregation, // o--
    Association, // --> or --
    Dependency,  // ..>  or ..
    Realization, // ..|>
    Dashed,      // ..
}

#[derive(Clone)]
struct Relation {
    from: usize,
    to: usize,
    kind: RelKind,
    label: String,
}

fn parse_member(line: &str) -> Member {
    let mut s = line.trim().to_string();
    let mut vis = Visibility::Public;
    if let Some(c) = s.chars().next() {
        match c {
            '+' => {
                vis = Visibility::Public;
                s = s[1..].trim().to_string();
            }
            '-' => {
                vis = Visibility::Private;
                s = s[1..].trim().to_string();
            }
            '#' => {
                vis = Visibility::Protected;
                s = s[1..].trim().to_string();
            }
            '~' => {
                vis = Visibility::Package;
                s = s[1..].trim().to_string();
            }
            _ => {}
        }
    }
    let is_method = s.contains('(');
    let mut ty = String::new();
    if let Some(idx) = s.find(':') {
        let (name, rest) = s.split_at(idx);
        let rest_owned = rest[1..].trim().to_string();
        let name_owned = name.trim().to_string();
        s = name_owned;
        ty = rest_owned;
    }
    if is_method {
        // strip everything from `(` onward including any return type
        if let Some(idx) = s.find('(') {
            s = s[..idx].trim().to_string();
        }
    }
    Member {
        vis,
        name: s,
        is_method,
        ty,
    }
}

fn parse_relation(line: &str, idx: &HashMap<String, usize>) -> Option<Relation> {
    // Try each known operator in priority order (longest match wins).
    let candidates: &[(&str, RelKind)] = &[
        ("<|--", RelKind::Inheritance),
        ("*--", RelKind::Composition),
        ("o--", RelKind::Aggregation),
        ("..|>", RelKind::Realization),
        ("..>", RelKind::Dependency),
        ("..", RelKind::Dashed),
        ("-->", RelKind::Association),
        ("--", RelKind::Association),
    ];
    let mut best: Option<(usize, &str, RelKind)> = None;
    for (op, kind) in candidates {
        if let Some(i) = line.find(op) {
            if best.map_or(true, |(bi, _, _)| {
                i < bi || (i == bi && op.len() > best.unwrap().1.len())
            }) {
                best = Some((i, op, *kind));
            }
        }
    }
    let (i, op, kind) = best?;
    let left = line[..i].trim();
    let right_with_label = line[i + op.len()..].trim();
    let left = left.split_whitespace().next().unwrap_or("").to_string();
    let (right, label) = split_label(right_with_label);
    let from = *idx.get(&left)?;
    let to = *idx.get(&right)?;
    Some(Relation {
        from,
        to,
        kind,
        label,
    })
}

fn split_label(s: &str) -> (String, String) {
    // `Name : label` or `Name` only.
    if let Some(idx) = s.find(" : ") {
        (s[..idx].trim().to_string(), s[idx + 3..].trim().to_string())
    } else {
        (
            s.split_whitespace().next().unwrap_or("").to_string(),
            String::new(),
        )
    }
}

/// Lay the classes out in a vertical stack; use a single column for
/// now (good enough for the common case of 2–6 classes).
fn layout(classes: &mut [ClassDef], rels: &[Relation]) {
    let pad_x = 12.0_f64;
    let pad_y = 16.0_f64;
    let title_h = 22.0_f64;
    let member_h = 15.0_f64;
    for c in classes.iter_mut() {
        let mut max_w = approx_text_width(&c.name) + pad_x * 2.0;
        for m in &c.members {
            let t = format!(
                "{}{}",
                m.name,
                if m.ty.is_empty() {
                    String::new()
                } else {
                    format!(": {}", m.ty)
                }
            );
            max_w = max_w.max(approx_text_width(&t) + pad_x * 2.0 + 12.0);
        }
        c.w = max_w.max(120.0);
        let h = title_h + (c.members.len().max(1) as f64) * member_h + pad_y;
        c.h = h;
    }
    // Initial vertical stack at x=40.
    let x = 40.0_f64;
    let mut y = 36.0_f64;
    for c in classes.iter_mut() {
        c.x = x;
        c.y = y;
        y += c.h + 24.0;
    }
    // If there are relationships, prefer to place `to` to the right
    // of `from` so the inheritance arrow goes left-to-right.
    let _ = rels;
}

fn dimensions(classes: &[ClassDef], has_title: bool) -> (f64, f64) {
    let mut w = 0.0_f64;
    let mut h = 0.0_f64;
    for c in classes {
        if c.x + c.w > w {
            w = c.x + c.w;
        }
        if c.y + c.h > h {
            h = c.y + c.h;
        }
    }
    (w + 32.0, h + (if has_title { 32.0 } else { 16.0 }))
}

fn render_class(c: &ClassDef, idx: usize) -> String {
    let mut s = String::new();
    let title_h = 24.0_f64;
    let member_h = 16.0_f64;
    let color = palette_color(idx);
    // Title row.
    s.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
         fill=\"{color}\" stroke=\"#24292f\" stroke-width=\"1.2\"/>",
        c.x,
        c.y,
        c.w,
        title_h,
        color = color
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"13\" font-weight=\"600\" \
         text-anchor=\"middle\" font-family=\"sans-serif, Noto Sans CJK SC, Microsoft YaHei, PingFang SC, Hiragino Sans GB, Source Han Sans SC, WenQuanYi Micro Hei\" fill=\"#ffffff\">{}</text>",
        c.x + c.w / 2.0,
        c.y + title_h - 7.0,
        escape_text(&c.name)
    ));
    // Members list.
    for (i, m) in c.members.iter().enumerate() {
        let y = c.y + title_h + i as f64 * member_h + 12.0;
        let glyph = match m.vis {
            Visibility::Public => "+",
            Visibility::Private => "−",
            Visibility::Protected => "#",
            Visibility::Package => "~",
        };
        let kind_glyph = if m.is_method { "()" } else { "" };
        let line = format!(
            "{}{}{}{}",
            glyph,
            m.name,
            kind_glyph,
            if m.ty.is_empty() {
                String::new()
            } else {
                format!(": {}", m.ty)
            }
        );
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{y:.1}\" font-size=\"11\" \
             font-family=\"monospace\" fill=\"#24292f\">{}</text>",
            c.x + 10.0,
            escape_text(&line)
        ));
    }
    // Box outline (drawn after so the row dividers don't break it).
    s.push_str(&shape_path(0, c.x, c.y, c.w, c.h));
    s.push_str(&format!(
        " fill=\"#fff\" stroke=\"#24292f\" stroke-width=\"1.2\"/>"
    ));
    s
}

fn render_relation(r: &Relation, classes: &[ClassDef]) -> Option<String> {
    let from = classes.get(r.from)?;
    let to = classes.get(r.to)?;
    let (x1, y1) = mid_side(from, to);
    let (x2, y2) = mid_side(to, from);
    let stroke = "#24292f";
    let dash = match r.kind {
        RelKind::Dependency | RelKind::Dashed | RelKind::Realization => "6 4",
        _ => "none",
    };
    let arrow_at_end = matches!(
        r.kind,
        RelKind::Inheritance | RelKind::Association | RelKind::Dependency | RelKind::Realization
    );
    let arrow_at_start = matches!(r.kind, RelKind::Composition | RelKind::Aggregation);
    let mut s = String::new();
    s.push_str(&format!(
        "<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" \
         stroke=\"{stroke}\" stroke-width=\"1.4\" stroke-dasharray=\"{dash}\"/>"
    ));
    if arrow_at_end {
        let ang = (y2 - y1).atan2(x2 - x1);
        s.push_str(&arrowhead(x2, y2, ang, false, r.kind));
    }
    if arrow_at_start {
        let ang = (y1 - y2).atan2(x1 - x2);
        s.push_str(&arrowhead(x1, y1, ang, true, r.kind));
    }
    if !r.label.is_empty() {
        let bw = approx_text_width(&r.label) + 12.0;
        let bh = 16.0_f64;
        let tx = (x1 + x2) / 2.0;
        let ty = (y1 + y2) / 2.0;
        s.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{bw:.1}\" height=\"{bh:.1}\" rx=\"3\" \
             fill=\"#fff\" stroke=\"#d0d7de\" stroke-width=\"0.5\"/>",
            tx - bw / 2.0,
            ty - bh / 2.0
        ));
        s.push_str(&format!(
            "<text x=\"{tx:.1}\" y=\"{:.1}\" font-size=\"10.5\" text-anchor=\"middle\" \
             font-family=\"sans-serif, Noto Sans CJK SC, Microsoft YaHei, PingFang SC, Hiragino Sans GB, Source Han Sans SC, WenQuanYi Micro Hei\" fill=\"#24292f\">{}</text>",
            ty + 4.0,
            escape_text(&r.label)
        ));
    }
    Some(s)
}

fn mid_side(c: &ClassDef, other: &ClassDef) -> (f64, f64) {
    let cx = c.x + c.w / 2.0;
    let cy = c.y + c.h / 2.0;
    let ox = other.x + other.w / 2.0;
    let oy = other.y + other.h / 2.0;
    if (ox - cx).abs() > (oy - cy).abs() {
        if ox > cx {
            (c.x + c.w, cy)
        } else {
            (c.x, cy)
        }
    } else if oy > cy {
        (cx, c.y + c.h)
    } else {
        (cx, c.y)
    }
}

fn arrowhead(px: f64, py: f64, angle: f64, hollow: bool, kind: RelKind) -> String {
    let (s, c) = (angle.sin(), angle.cos());
    let back = 10.0;
    let side = 6.0;
    let bx = px - back * c;
    let by = py - back * s;
    let p1x = bx + side * (-s);
    let p1y = by + side * c;
    let p2x = bx + side * s;
    let p2y = by - side * c;
    let fill = if hollow { "#ffffff" } else { "#24292f" };
    let stroke = "#24292f";
    // Triangular hollow head for inheritance/realization
    match kind {
        RelKind::Inheritance | RelKind::Realization => {
            format!(
                "<polygon points=\"{px:.1},{py:.1} {p1x:.1},{p1y:.1} {p2x:.1},{p2y:.1}\" \
                 fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"1\"/>"
            )
        }
        _ => {
            format!(
                "<polygon points=\"{px:.1},{py:.1} {p1x:.1},{p1y:.1} {p2x:.1},{p2y:.1}\" \
                 fill=\"{fill}\"/>"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn renders_class_diagram() {
        let src = "classDiagram\n  class Animal {\n    +name: string\n    +age: int\n  }\n  class Dog {\n    +bark() void\n  }\n  Animal <|-- Dog\n";
        let out = render(src).expect("render");
        assert!(out.contains("Animal"));
        assert!(out.contains("Dog"));
        assert!(out.contains("<line"));
    }

    #[test]
    fn renders_relation_label() {
        let src = "classDiagram\n  class A\n  class B\n  A --> B : uses\n";
        let out = render(src).expect("render");
        assert!(out.contains("uses"));
    }

    #[test]
    fn empty_class_diagram_returns_none() {
        assert!(render("classDiagram\n").is_none());
    }
}
