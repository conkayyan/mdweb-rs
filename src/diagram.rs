//! A tiny, dependency-free Mermaid **flowchart** subset → SVG renderer.
//!
//! Draws `flowchart TD/LR/RL/BT` (and `graph …`) diagrams as inline SVG with
//! no JavaScript and no third-party library. Supported node shapes:
//!
//! - `A[text]` rectangle, `A(text)` rounded, `A((text))` circle,
//!   `A{text}` diamond, `A[/text/]` parallelogram, `A>text]` asymmetric.
//! - Edges: `-->`, `---`, `-.->` (dotted), `==>` (thick), optional labels
//!   via `|label|` or `-- label -->`.
//! - Comments `%% …`, `direction TD/LR/RL/BT`, and `subgraph`/`end`
//!   bounding boxes.
//!
//! Anything not understood is skipped, so a diagram with an unsupported
//! construct still produces a usable SVG. [`render`] returns `None` when no
//! nodes were parsed (the caller falls back to a code block).

use std::collections::HashMap;

/// Render a Mermaid flowchart source to SVG. `None` if no nodes are found.
pub fn render(src: &str) -> Option<String> {
    let mut d = Diagram::parse(src);
    if d.nodes.is_empty() {
        return None;
    }
    d.layout();
    Some(d.to_svg())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    Td,
    Lr,
    Rl,
    Bt,
}

struct Node {
    label: String,
    shape: u8,
    layer: usize,
    slot: usize,
    fill: String,
}

struct Edge {
    from: usize,
    to: usize,
    label: String,
    style: u8, // 0 solid arrow, 1 plain, 2 dotted, 3 thick
}

struct Sub {
    name: String,
    nodes: Vec<usize>,
}

struct Diagram {
    dir: Dir,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    subs: Vec<Sub>,
    idx: HashMap<String, usize>,
}

impl Diagram {
    fn parse(src: &str) -> Diagram {
        let mut d = Diagram {
            dir: Dir::Td,
            nodes: Vec::new(),
            edges: Vec::new(),
            subs: Vec::new(),
            idx: HashMap::new(),
        };
        let mut sub_stack: Vec<usize> = Vec::new();
        for raw in src.lines() {
            let mut line = strip_comments(raw).trim();
            // strip trailing comma that sometimes ends node decls
            if line.ends_with(',') {
                line = &line[..line.len() - 1];
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let lower = line.to_ascii_lowercase();
            // Any `xxxDiagram` declaration other than flowchart/graph is a
            // diagram type we do not support (sequence, gantt, pie, state…).
            if lower.ends_with("diagram") || lower.ends_with("journey") || lower.ends_with("gantt")
            {
                if lower.starts_with("flowchart") || lower.starts_with("graph ") {
                    parse_dir_decl(&mut d, line);
                    continue;
                }
                // stop parsing: unsupported diagram type
                return Diagram {
                    dir: Dir::Td,
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    subs: Vec::new(),
                    idx: HashMap::new(),
                };
            }
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("flowchart ") || lower.starts_with("graph ") {
                parse_dir_decl(&mut d, line);
                continue;
            }
            if matches!(
                lower.as_str(),
                "direction td" | "direction tb" | "direction lr" | "direction rl" | "direction bt"
            ) {
                d.dir = match lower.as_str() {
                    "direction lr" => Dir::Lr,
                    "direction rl" => Dir::Rl,
                    "direction bt" => Dir::Bt,
                    _ => Dir::Td,
                };
                continue;
            }
            if let Some(rest) = line.strip_prefix("subgraph") {
                d.subs.push(Sub {
                    name: rest.trim().to_string(),
                    nodes: Vec::new(),
                });
                sub_stack.push(d.subs.len() - 1);
                continue;
            }
            if line == "end" {
                sub_stack.pop();
                continue;
            }
            if line.starts_with("classDef ") || line.starts_with("class ") {
                continue; // styling classes: ignored (nodes keep default fill)
            }
            if line.starts_with("style ") {
                continue;
            }
            if let Some(_rest) = line.strip_prefix("linkStyle ") {
                continue;
            }
            // edge or node statement
            if contains_edge_op(line) {
                parse_edge(&mut d, line, &sub_stack);
            } else {
                parse_node_line(&mut d, line, &sub_stack);
            }
        }
        d
    }

    fn add_node(&mut self, id: String, label: String, shape: u8, subs: &[usize]) -> usize {
        if let Some(&i) = self.idx.get(&id) {
            return i;
        }
        let i = self.nodes.len();
        let label = if label.is_empty() { id.clone() } else { label };
        self.nodes.push(Node {
            label,
            shape,
            layer: 0,
            slot: 0,
            fill: "#e7e9ee".to_string(),
        });
        self.idx.insert(id, i);
        if let Some(&cup) = subs.last() {
            self.subs[cup].nodes.push(i);
        }
        i
    }

    fn layout(&mut self) {
        // longest-path layering
        let mut depth = vec![0usize; self.nodes.len()];
        for _ in 0..self.nodes.len() {
            let mut changed = false;
            for e in &self.edges {
                let nd = depth[e.from] + 1;
                if nd > depth[e.to] {
                    depth[e.to] = nd;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut counts = vec![0usize; self.nodes.len().max(1)];
        for (n, node) in self.nodes.iter_mut().enumerate() {
            let d = depth[n];
            node.layer = d;
            node.slot = counts[d];
            counts[d] += 1;
        }
    }

    fn pos_of(&self, n: &Node, w: f64, h: f64, gx: f64, gy: f64) -> (f64, f64) {
        match self.dir {
            Dir::Td | Dir::Bt => (
                gx + n.slot as f64 * (w + gx),
                gy + n.layer as f64 * (h + gy),
            ),
            Dir::Lr | Dir::Rl => (
                gx + n.layer as f64 * (w + gx),
                gy + n.slot as f64 * (h + gy),
            ),
        }
    }

    fn to_svg(&self) -> String {
        const W: f64 = 140.0;
        const H: f64 = 42.0;
        const GX: f64 = 56.0;
        const GY: f64 = 64.0;
        let (mw, mh) = self.dimensions(W, H, GX, GY);
        let mut out = String::new();
        out.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {mw:.0} {mh:.0}\" \
             role=\"img\" aria-label=\"flowchart diagram\">"
        ));
        for sub in &self.subs {
            let (x0, y0, x1, y1) = self.sub_bbox(sub, W, H, GX, GY);
            out.push_str(&format!(
                "<rect x=\"{x0:.0}\" y=\"{y0:.0}\" width=\"{:.0}\" height=\"{:.0}\" rx=\"8\" \
                 fill=\"#f6f8fa\" stroke=\"#d0d7de\" stroke-dasharray=\"5 3\"/>",
                x1 - x0,
                y1 - y0
            ));
            out.push_str(&format!(
                "<text x=\"{x0:.0}\" y=\"{:.0}\" font-size=\"13\" \
                 font-family=\"sans-serif\" fill=\"#57606a\">{}</text>",
                y0 - 6.0,
                escape_text(&sub.name)
            ));
        }
        for e in &self.edges {
            out.push_str(&self.edge_svg(e, W, H, GX, GY));
        }
        for n in &self.nodes {
            out.push_str(&self.node_svg(n, W, H, GX, GY));
        }
        out.push_str("</svg>");
        out
    }

    fn dimensions(&self, w: f64, h: f64, gx: f64, gy: f64) -> (f64, f64) {
        let mut cols = 0usize;
        let mut rows = 0usize;
        for n in &self.nodes {
            match self.dir {
                Dir::Td | Dir::Bt => {
                    cols = cols.max(n.slot + 1);
                    rows = rows.max(n.layer + 1);
                }
                Dir::Lr | Dir::Rl => {
                    cols = cols.max(n.layer + 1);
                    rows = rows.max(n.slot + 1);
                }
            }
        }
        (cols as f64 * (w + gx) + gx, rows as f64 * (h + gy) + gy)
    }

    fn node_svg(&self, n: &Node, w: f64, h: f64, gx: f64, gy: f64) -> String {
        let (x, y) = self.pos_of(n, w, h, gx, gy);
        let path = shape_path(n.shape, x, y, w, h);
        let mut s = String::new();
        s.push_str(&format!(
            "<path d=\"{path}\" fill=\"{}\" stroke=\"#24292f\" stroke-width=\"1.5\"/>",
            n.fill
        ));
        s.push_str(&format!(
            "<text x=\"{:.0}\" y=\"{:.0}\" font-size=\"12\" text-anchor=\"middle\" \
             font-family=\"sans-serif\" fill=\"#24292f\">{}</text>",
            x + w / 2.0,
            y + h / 2.0 + 4.0,
            wrap_label(&n.label, 14)
        ));
        s
    }

    fn edge_svg(&self, e: &Edge, w: f64, h: f64, gx: f64, gy: f64) -> String {
        let from = &self.nodes[e.from];
        let to = &self.nodes[e.to];
        let (x1, y1) = self.pos_of(from, w, h, gx, gy);
        let (x2, y2) = self.pos_of(to, w, h, gx, gy);
        let (sx, sy, ex, ey) = match self.dir {
            Dir::Td | Dir::Bt => (x1 + w / 2.0, y1 + h, x2 + w / 2.0, y2),
            Dir::Lr | Dir::Rl => (x1 + w, y1 + h / 2.0, x2, y2 + h / 2.0),
        };
        let (stroke, dash, arrow) = match e.style {
            0 => ("#24292f", "none", true),
            1 => ("#24292f", "none", false),
            2 => ("#24292f", "6 4", true),
            _ => ("#8250df", "none", true),
        };
        let mut s = String::new();
        s.push_str(&format!(
            "<line x1=\"{sx:.1}\" y1=\"{sy:.1}\" x2=\"{ex:.1}\" y2=\"{ey:.1}\" \
             stroke=\"{stroke}\" stroke-width=\"1.6\" stroke-dasharray=\"{dash}\"/>"
        ));
        if arrow {
            let ang = (ey - sy).atan2(ex - sx);
            let hx = ex - 9.0 * ang.cos();
            let hy = ey - 9.0 * ang.sin();
            let p1x = hx + 4.0 * ang.cos() - 5.0 * ang.sin();
            let p1y = hy + 4.0 * ang.sin() + 5.0 * ang.cos();
            let p2x = hx + 4.0 * ang.cos() + 5.0 * ang.sin();
            let p2y = hy + 4.0 * ang.sin() - 5.0 * ang.cos();
            s.push_str(&format!(
                "<path d=\"M {ex:.1} {ey:.1} L {p1x:.1} {p1y:.1} L {p2x:.1} {p2y:.1} Z\" \
                 fill=\"{stroke}\"/>"
            ));
        }
        if !e.label.is_empty() {
            let (lx, ly) = ((sx + ex) / 2.0, (sy + ey) / 2.0 - 6.0);
            let bw = e.label.chars().count() as f64 * 6.5 + 10.0;
            let tx = (sx + ex) / 2.0;
            let ty = (sy + ey) / 2.0 - 4.0;
            s.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{bw:.1}\" height=\"15\" rx=\"3\" \
                 fill=\"#fff\"/>",
                tx - bw / 2.0,
                ty - 11.0
            ));
            let _ = (lx, ly);
            s.push_str(&format!(
                "<text x=\"{tx:.1}\" y=\"{ty:.1}\" font-size=\"10.5\" text-anchor=\"middle\" \
                 font-family=\"sans-serif\" fill=\"#24292f\">{}</text>",
                escape_text(&e.label)
            ));
        }
        s
    }

    fn sub_bbox(&self, sub: &Sub, w: f64, h: f64, gx: f64, gy: f64) -> (f64, f64, f64, f64) {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for &ni in &sub.nodes {
            let (x, y) = self.pos_of(&self.nodes[ni], w, h, gx, gy);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x + w);
            max_y = max_y.max(y + h);
        }
        if min_x.is_infinite() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        (min_x - 16.0, min_y - 30.0, max_x + 16.0, max_y + 16.0)
    }
}

fn shape_path(shape: u8, x: f64, y: f64, w: f64, h: f64) -> String {
    let x0 = x;
    let x1 = x + w;
    let y0 = y;
    let y1 = y + h;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    match shape {
        1 => {
            // rounded rect
            format!(
                "M {x0:.1} {y0:.1} h {w:.1} v {h:.1} h -{w:.1} z",
                w = w - 0.0,
                h = h - 0.0
            )
        }
        2 => format!("<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{r:.1}\" />", r = (h / 2.0).min(w / 2.0) + 6.0),
        3 => format!("M {cx:.1} {y0:.1} L {x1:.1} {cy:.1} L {cx:.1} {y1:.1} L {x0:.1} {cy:.1} Z"),
        5 => format!("M {x0:.1} {y0:.1} L {x1:.1} {y0:.1} L {x0:.1} {y1:.1} Z"),
        6 => format!("M {x0:.1} {y0:.1} L {l:.1} {y0:.1} L {x1:.1} {cy:.1} L {l:.1} {y1:.1} L {x0:.1} {y1:.1} Z", l = x1 - 20.0),
        _ => format!("M {x0:.1} {y0:.1} L {x1:.1} {y0:.1} L {x1:.1} {y1:.1} L {x0:.1} {y1:.1} Z"),
    }
}

fn contains_edge_op(line: &str) -> bool {
    line.contains("-->") || line.contains("---") || line.contains("==>") || line.contains("-.-")
}

fn parse_dir_decl(d: &mut Diagram, line: &str) {
    let rest = line
        .trim_start_matches("flowchart")
        .trim_start_matches("graph")
        .trim();
    d.dir = match rest.to_ascii_lowercase().as_str() {
        "lr" => Dir::Lr,
        "rl" => Dir::Rl,
        "bt" => Dir::Bt,
        _ => Dir::Td,
    };
}

fn parse_node_line(d: &mut Diagram, line: &str, subs: &[usize]) {
    if let Some((id, label, shape)) = split_node_spec(line) {
        if d.idx.contains_key(&id) {
            // re-declaration: keep first
            return;
        }
        d.add_node(id, label, shape, subs);
    } else if is_ident(line) {
        d.add_node(line.to_string(), line.to_string(), 0, subs);
    }
}

fn split_node_spec(s: &str) -> Option<(String, String, u8)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let open = s
        .char_indices()
        .find(|(_, c)| matches!(c, '[' | '(' | '{' | '>' | '/'))?
        .0;
    let id = s[..open].trim().to_string();
    if id.is_empty() {
        return None;
    }
    let rest = &s[open..];
    let (shape, label) = match rest.chars().next().unwrap() {
        '>' => {
            let end = rest.find(']').unwrap_or(rest.len());
            (6, rest[1..end].to_string())
        }
        '(' if rest.starts_with("((") => {
            let end = rest.rfind("))").map(|i| i + 1).unwrap_or(rest.len() - 1);
            (2, rest[2..end].to_string())
        }
        '(' => {
            let end = rest.rfind(')').unwrap_or(rest.len());
            (1, rest[1..end].to_string())
        }
        '{' => {
            let end = rest.rfind('}').unwrap_or(rest.len());
            (3, rest[1..end].to_string())
        }
        '/' => {
            let end = rest.rfind('/').unwrap_or(rest.len());
            (4, rest[1..end].to_string())
        }
        _ => {
            let end = rest.rfind(']').unwrap_or(rest.len());
            (0, rest[1..end].to_string())
        }
    };
    let label = label.trim().replace('\'', "");
    Some((id, label, shape))
}

fn parse_edge(d: &mut Diagram, line: &str, subs: &[usize]) {
    let len = line.len();
    let mut i = 0;
    let mut from: Option<usize> = None;
    while i < len {
        // skip whitespace
        while i < len && line.as_bytes()[i] == b' ' {
            i += 1;
        }
        if i >= len {
            break;
        }
        // edge operator?
        let (op_end, style) = if line[i..].starts_with("==>") {
            (i + 3, 3)
        } else if line[i..].starts_with("-.-") {
            (i + 3, 2)
        } else if line[i..].starts_with("---") {
            (i + 3, 1)
        } else if line[i..].starts_with("-->") {
            (i + 3, 0)
        } else {
            (i, 0)
        };
        if op_end > i {
            i = op_end;
            // optional label |...|
            let mut label = String::new();
            while i < len && line.as_bytes()[i] == b' ' {
                i += 1;
            }
            if i < len && line.as_bytes()[i] == b'|' {
                if let Some(rel) = line[i + 1..].find('|') {
                    label = line[i + 1..i + 1 + rel].to_string();
                    i += rel + 2;
                }
            } else {
                // possible -- label -- form: consume a word that is followed
                // by another edge operator (not a node id)
                let j = i;
                while i < len
                    && !line[i..].starts_with("-->")
                    && !line[i..].starts_with("---")
                    && !line[i..].starts_with("==>")
                    && !line[i..].starts_with("-.-")
                    && line.as_bytes()[i] != b' '
                {
                    i = next_boundary(line, i);
                }
                let word = &line[j..i];
                if !word.is_empty() && i < len {
                    // If followed by an edge op it was a label, else rewind
                    if line[i..].starts_with("-->")
                        || line[i..].starts_with("---")
                        || line[i..].starts_with("==>")
                        || line[i..].starts_with("-.-")
                    {
                        label = word.trim().to_string();
                    } else {
                        i = j; // treat as node start, let outer loop handle
                    }
                } else {
                    i = j;
                }
            }
            // resolve target node
            while i < len && line.as_bytes()[i] == b' ' {
                i += 1;
            }
            let seg_end = i;
            let (id, lbl, shape) = split_node_spec(&line[i..]).unwrap_or_else(|| {
                let end = next_edge_or_space(line, i, len);
                (line[seg_end..end].to_string(), String::new(), 0)
            });
            if !id.is_empty() && id != "|" {
                let ni = d.add_node(id, lbl, shape, subs);
                if let Some(fi) = from {
                    d.edges.push(Edge {
                        from: fi,
                        to: ni,
                        label,
                        style,
                    });
                }
                from = Some(ni);
            }
            continue;
        }
        // node segment: consume until next edge op
        let j = i;
        let mut k = i;
        while k < len
            && !line[k..].starts_with("-->")
            && !line[k..].starts_with("---")
            && !line[k..].starts_with("==>")
            && !line[k..].starts_with("-.-")
        {
            k = next_boundary(line, k);
        }
        let seg = line[j..k].trim();
        i = k;
        if let Some((id, label, shape)) = split_node_spec(seg) {
            let ni = d.add_node(id, label, shape, subs);
            from = Some(ni);
        } else if is_ident(seg) {
            let ni = d.add_node(seg.to_string(), seg.to_string(), 0, subs);
            from = Some(ni);
        }
    }
}

/// Advance `k` (a char boundary) to the next char boundary.
fn next_boundary(line: &str, k: usize) -> usize {
    k + line[k..].chars().next().map_or(1, char::len_utf8)
}

fn next_edge_or_space(line: &str, from: usize, len: usize) -> usize {
    let mut k = from;
    while k < len
        && !line[k..].starts_with("-->")
        && !line[k..].starts_with("---")
        && !line[k..].starts_with("==>")
        && !line[k..].starts_with("-.-")
        && line.as_bytes()[k] != b' '
    {
        k = next_boundary(line, k);
    }
    k
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':')
}

fn strip_comments(line: &str) -> &str {
    if let Some(i) = line.find("%%") {
        &line[..i]
    } else {
        line
    }
}

fn wrap_label(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return escape_text(s);
    }
    let mut out = String::new();
    let mut len = 0;
    for &ch in &chars {
        if len >= max && ch != ' ' {
            out.push('\n');
            len = 0;
        }
        out.push(ch);
        len += 1;
    }
    out
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_flowchart_td() {
        let out = render("flowchart TD\n  A[Start] --> B{Decision}\n  B --> C[End]")
            .expect("should render");
        assert!(out.starts_with("<svg"));
        assert!(out.contains("Start"));
        assert!(out.contains("Decision"));
        assert!(out.contains("<line "));
    }

    #[test]
    fn renders_graph_lr_with_labels() {
        let out = render("graph LR\n  A -->|yes| B\n  B -.-> C").expect("render");
        assert!(out.contains("yes"));
        assert!(out.contains("stroke-dasharray=\"6 4\""));
    }

    #[test]
    fn subgraph_and_comments() {
        let out =
            render("flowchart TD\n %% comment\n subgraph core\n A[one]\n B[two]\n end\n A --> B")
                .expect("render");
        assert!(out.contains("core"));
        assert!(out.contains("one"));
    }

    #[test]
    fn cjk_labels_do_not_panic() {
        let out = render("flowchart TD\n  A[编写 Markdown] --> B{能渲染了吗？}\n  B --> C[发布]")
            .expect("should render");
        assert!(out.starts_with("<svg"));
        assert!(out.contains("编写 Markdown"));
    }

    #[test]
    fn unknown_syntax_degrades_empty() {
        assert!(render("sequenceDiagram\n A->>B: hi").is_none());
    }
}
