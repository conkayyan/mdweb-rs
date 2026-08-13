//! Graphviz `digraph` / `graph` (DOT language) → SVG renderer.
//!
//! Supported syntax:
//! ```text
//! digraph G {
//   rankdir=LR;
//   A -> B;
//   B -> C;
//   "complex name" -> D [label="next"];
// }
// ```
//!
//! Edge operators:
//! - `->` (directed), `--` (undirected)
//!
//! Node attributes (just `label="…"` for now) are picked up from
//! `[label="…"]` blocks on either side of an edge or on a standalone
//! node declaration. The renderer reuses the same Sugiyama-ish
//! layout the Mermaid flowchart uses: longest-path layering, balance
//! pass, then downward barycenter sweeps.
//!
//! Nodes render as Graphviz's default ellipse (white fill, black
//! outline); edges are straight lines clipped to the ellipse outline,
//! with a `<marker>` arrowhead on directed edges. This matches the
//! canonical reference output:
//!
//! ```text
//! digraph G { A -> B; B -> C; B -> D }
//! ```
//!
//! → ellipse nodes `A` over `B` with `C` and `D` fanned out below,
//! straight `A→B` / `B→C` / `B→D` lines with triangular arrowheads.
//!
//! Returns `None` when no nodes are found.

use super::common::{approx_text_width, escape_text, fit_node, FONT_FAMILY};
use std::collections::HashMap;

/// Gap between successive layers along the rank axis (y for TB, x for
/// LR). TB uses a wide gap so diagonal fan edges (`B -> C`, `B -> D`)
/// get a long, clearly visible run instead of a ~30 px stub that hides
/// under the node strokes — matching the reference layout where B and C
/// sit ~100 px apart centre-to-centre. LR keeps the modest value.
const RANK_GAP: fn(Dir) -> f64 = |dir| match dir {
    Dir::TBlr => 64.0,
    Dir::Lr => 40.0,
};

/// Minimum horizontal separation enforced between the centres of two
/// same-layer nodes (beyond their half-widths). Small so a fan of
/// siblings sits tightly beneath the shared source like the reference,
/// instead of being pushed 32 px apart into a wide, sparse diagram.
const SIBLING_GAP: f64 = 10.0;

/// Render a DOT-language graph source to SVG. `None` if no nodes.
pub fn render(src: &str) -> Option<String> {
    let mut dir = Dir::TBlr;
    let mut rankdir_set = false;
    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut idx: HashMap<String, usize> = HashMap::new();

    let mut chars = src.chars().peekable();
    let mut cur = String::new();
    let mut brace_depth = 0_i32;
    let mut body_start = 0usize; // index in `cur` where the graph body starts (after `digraph G {`)
    let mut in_quote = None::<char>;
    while let Some(c) = chars.next() {
        match (in_quote, c) {
            (Some(q), c) if c == q => {
                in_quote = None;
                cur.push(c);
                continue;
            }
            (Some(_), _) => {
                cur.push(c);
                continue;
            }
            (None, '"') => {
                in_quote = Some('"');
                cur.push(c);
                continue;
            }
            (None, '\'') if cur.trim().is_empty() => {
                in_quote = Some('\'');
                cur.push(c);
                continue;
            }
            _ => {}
        }
        match c {
            '{' => {
                brace_depth += 1;
                cur.push(c);
                if brace_depth == 1 {
                    // Mark where the body starts so each statement
                    // we flush later skips past the `digraph G {` prefix.
                    body_start = cur.len();
                }
            }
            '}' => {
                brace_depth -= 1;
                cur.push(c);
                if brace_depth <= 0 {
                    let stmt = cur[body_start..cur.len() - 1].to_string();
                    if !stmt.trim().is_empty() {
                        flush_statement(
                            &stmt,
                            &mut nodes,
                            &mut edges,
                            &mut idx,
                            &mut dir,
                            &mut rankdir_set,
                        );
                    }
                    cur.clear();
                    body_start = 0;
                }
            }
            ';' => {
                cur.push(c);
                if brace_depth == 1 {
                    let stmt = cur[body_start..].to_string();
                    if !stmt.trim().is_empty() {
                        flush_statement(
                            &stmt,
                            &mut nodes,
                            &mut edges,
                            &mut idx,
                            &mut dir,
                            &mut rankdir_set,
                        );
                    }
                    cur.truncate(body_start);
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur[body_start..].trim().is_empty() {
        flush_statement(
            &cur[body_start..],
            &mut nodes,
            &mut edges,
            &mut idx,
            &mut dir,
            &mut rankdir_set,
        );
    }

    if nodes.is_empty() {
        return None;
    }

    layout(&mut nodes, &edges, dir);
    let (mw, mh) = dimensions(&nodes, dir);
    let layer_y = layer_offsets(&nodes, dir);

    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         width=\"{mw:.0}\" height=\"{mh:.0}\" \
         viewBox=\"0 0 {mw:.0} {mh:.0}\" \
         style=\"max-width:100%;height:auto;\" \
         role=\"img\" aria-label=\"graph diagram\">"
    ));
    out.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>");
    out.push_str(
        "<defs><marker id=\"mdsvg-dot-arrow\" viewBox=\"0 0 10 10\" refX=\"10\" refY=\"5\" \
         markerWidth=\"6\" markerHeight=\"6\" orient=\"auto\"><path d=\"M 0 0 L 10 5 L 0 10 z\" \
         fill=\"#000000\"/></marker></defs>",
    );
    // Edges first so the node strokes sit on top of the line ends.
    for e in &edges {
        out.push_str(&render_edge(e, &nodes, &layer_y, dir));
    }
    for n in &nodes {
        out.push_str(&render_node(n, &layer_y, dir));
    }
    out.push_str("</svg>");
    Some(out)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    /// rankdir=LR (left-to-right)
    Lr,
    /// rankdir=TB / TD (top-to-bottom; default)
    TBlr,
}

struct Node {
    #[allow(dead_code)]
    id: String,
    label: String,
    /// x_pos: center along the perpendicular axis.
    x_pos: f64,
    layer: usize,
    w: f64,
    h: f64,
}

struct Edge {
    from: usize,
    to: usize,
    label: String,
    directed: bool,
}

fn flush_statement(
    stmt: &str,
    nodes: &mut Vec<Node>,
    edges: &mut Vec<Edge>,
    idx: &mut HashMap<String, usize>,
    dir: &mut Dir,
    rankdir_set: &mut bool,
) {
    let s = stmt.trim();
    if s.is_empty() {
        return;
    }
    if let Some(rest) = s.strip_prefix("rankdir") {
        let rest = rest
            .trim_start_matches('=')
            .trim()
            .trim_matches('"')
            .trim_matches('\'');
        let lower = rest.to_ascii_lowercase();
        if lower == "lr" || lower == "rl" {
            *dir = Dir::Lr;
        } else {
            *dir = Dir::TBlr;
        }
        *rankdir_set = true;
        return;
    }
    if s.starts_with('{') || s.starts_with('}') {
        return;
    }
    // Try graph-level keywords.
    let lower = s.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "graph g"
            | "digraph g"
            | "strict graph g"
            | "strict digraph g"
            | "graph"
            | "digraph"
            | "strict graph"
            | "strict digraph"
    ) {
        return;
    }
    if lower.starts_with("graph ") || lower.starts_with("digraph ") || lower.starts_with("strict") {
        return;
    }
    // Try an edge: split on `->` or `--`.
    let op = if s.contains("->") {
        Some("->")
    } else if s.contains("--") {
        Some("--")
    } else {
        None
    };
    if let Some(op) = op {
        // DOT allows chained edges: `A -> B -> C` ≡ `A -> B; B -> C;`.
        // Split on the operator and emit a hop per adjacent pair, so a
        // chain no longer drops every node after the second one. A
        // trailing `[label=…]` block rides on the hop it follows (as in
        // Graphviz, where edge attributes apply to the whole chain).
        let parts: Vec<&str> = s.split(op).collect();
        if parts.len() >= 2 {
            let mut ids = Vec::with_capacity(parts.len());
            let mut last_label = String::new();
            for part in &parts {
                let (id, label) = split_id_with_attrs(part);
                ids.push(id);
                if !label.is_empty() {
                    last_label = label;
                }
            }
            for w in ids.windows(2) {
                let from = ensure_node(nodes, idx, &w[0]);
                let to = ensure_node(nodes, idx, &w[1]);
                edges.push(Edge {
                    from,
                    to,
                    label: last_label.clone(),
                    directed: op == "->",
                });
            }
            return;
        }
    }
    // Single node declaration.
    let (id, label) = split_id_with_attrs(s);
    if !id.is_empty() {
        ensure_node(nodes, idx, &id);
        if !label.is_empty() {
            nodes[*idx.get(&id).unwrap()].label = label;
        }
    }
}

fn split_id(s: &str) -> String {
    let s = s.trim().trim_end_matches(';').trim();
    let end = s
        .find(|c: char| c == '[' || c == '{' || c.is_whitespace())
        .unwrap_or(s.len());
    let raw = s[..end].trim();
    trim_quotes(raw).to_string()
}

/// Split an id from its attribute block. The label is only the
/// explicit `[label="…"]` value — an unlabelled node or edge target
/// must not leak its id into the label slot (that used to put a
/// stray label box onto every edge).
fn split_id_with_attrs(s: &str) -> (String, String) {
    let s = s.trim().trim_end_matches(';').trim();
    if let Some(bracket) = s.find('[') {
        let head = &s[..bracket];
        let attrs = &s[bracket..];
        let id = split_id(head);
        let label = read_attr(attrs, "label");
        return (id, label);
    }
    let id = split_id(s);
    (id, String::new())
}

fn trim_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn read_attr(s: &str, key: &str) -> String {
    // Find `key="value"` or `key=value`.
    if let Some(idx) = s.find(key) {
        let after = &s[idx + key.len()..];
        let after = after.trim_start_matches('=').trim();
        if after.starts_with('"') {
            if let Some(end) = after[1..].find('"') {
                return after[1..1 + end].to_string();
            }
        } else if after.starts_with('\'') {
            if let Some(end) = after[1..].find('\'') {
                return after[1..1 + end].to_string();
            }
        } else {
            let end = after
                .find(|c: char| c == ',' || c == ']' || c.is_whitespace())
                .unwrap_or(after.len());
            return after[..end].to_string();
        }
    }
    String::new()
}

fn ensure_node(nodes: &mut Vec<Node>, idx: &mut HashMap<String, usize>, id: &str) -> usize {
    if let Some(&i) = idx.get(id) {
        return i;
    }
    let i = nodes.len();
    let label = id.to_string();
    let (w, h) = fit_node(&label);
    nodes.push(Node {
        id: id.to_string(),
        label,
        x_pos: 0.0,
        layer: 0,
        w,
        h,
    });
    idx.insert(id.to_string(), i);
    i
}

fn layout(nodes: &mut [Node], edges: &[Edge], _dir: Dir) {
    let n_nodes = nodes.len();
    // Longest-path layering. Edges directed both ways count for
    // both directions; for `--` we treat it as directed (it's still
    // a layer constraint).
    let mut depth = vec![0usize; n_nodes];
    for _ in 0..n_nodes {
        let mut changed = false;
        for e in edges {
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
    for (i, n) in nodes.iter_mut().enumerate() {
        n.layer = depth[i];
    }
    let parents: Vec<Vec<usize>> = {
        let mut p = vec![Vec::new(); n_nodes];
        for e in edges {
            p[e.to].push(e.from);
        }
        p
    };
    // Barycenter passes.
    for _round in 0..4 {
        for layer in 1..=*depth.iter().max().unwrap_or(&0) {
            for i in 0..nodes.len() {
                if nodes[i].layer != layer {
                    continue;
                }
                if parents[i].is_empty() {
                    continue;
                }
                let sum: f64 = parents[i].iter().map(|&p| nodes[p].x_pos).sum();
                let mean = sum / parents[i].len() as f64;
                nodes[i].x_pos = mean;
            }
        }
    }
    // Collision pass. A single left-to-right sweep is NOT enough: the
    // second window (`D,E`) re-collides the first pair `(C,D)` because
    // it pushes D back toward C (kids C and D ended up 20 px apart,
    // well inside their 70 px ellipses). So we sweep each layer
    // repeatedly, halving the overlap each round, until no adjacent
    // pair violates the separation. The order is fixed from the first
    // sort — collapses converge and no node overtakes a neighbour.
    let max_layer = *depth.iter().max().unwrap_or(&0);
    for layer in 0..=max_layer {
        let mut order: Vec<usize> = (0..n_nodes).filter(|&i| nodes[i].layer == layer).collect();
        order.sort_by(|&a, &b| {
            nodes[a]
                .x_pos
                .partial_cmp(&nodes[b].x_pos)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for _ in 0..64 {
            let mut changed = false;
            for w in order.windows(2) {
                let (a, b) = (w[0], w[1]);
                let needed = (nodes[a].w + nodes[b].w) / 2.0 + SIBLING_GAP;
                let cur = nodes[b].x_pos - nodes[a].x_pos;
                if cur < needed {
                    let push = (needed - cur) / 2.0;
                    nodes[a].x_pos -= push;
                    nodes[b].x_pos += push;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    let _ = parents;
    // Normalise to positive x.
    let (min_x, max_hw) = nodes.iter().fold((f64::INFINITY, 0.0_f64), |(mx, hw), n| {
        (mx.min(n.x_pos), hw.max(n.w / 2.0))
    });
    let pad = 32.0 - (min_x - max_hw);
    for n in nodes.iter_mut() {
        n.x_pos += pad;
    }
    let _ = max_layer;
}

fn dimensions(nodes: &[Node], dir: Dir) -> (f64, f64) {
    if nodes.is_empty() {
        return (120.0, 120.0);
    }
    let gx = 32.0_f64;
    let gy = RANK_GAP(dir);
    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    // Pre-compute per-layer max extent.
    let mut layer_ext = vec![0.0_f64; max_layer + 1];
    for n in nodes {
        let e = match dir {
            Dir::TBlr => n.h,
            Dir::Lr => n.w,
        };
        if e > layer_ext[n.layer] {
            layer_ext[n.layer] = e;
        }
    }
    // Layer start offsets.
    let mut layer_y = vec![0.0_f64; max_layer + 1];
    let mut cursor = gy;
    for l in 0..=max_layer {
        layer_y[l] = cursor;
        cursor += layer_ext[l] + gy;
    }
    let (max_x, max_hw) = nodes
        .iter()
        .fold((f64::NEG_INFINITY, 0.0_f64), |(mx, hw), n| {
            (mx.max(n.x_pos + n.w / 2.0), hw.max(n.w / 2.0))
        });
    let (max_y, max_hh) = nodes
        .iter()
        .fold((f64::NEG_INFINITY, 0.0_f64), |(my, hh), n| {
            (my.max(n.x_pos + n.h / 2.0), hh.max(n.h / 2.0))
        });
    match dir {
        Dir::TBlr => (
            max_x + gx + max_hw,
            layer_y[max_layer] + layer_ext[max_layer] + 12.0,
        ),
        Dir::Lr => (
            layer_y[max_layer] + layer_ext[max_layer] + 12.0 + gx,
            max_y + gy + max_hh,
        ),
    }
}

/// Start offset of each layer along the rank axis (y for TB, x for LR).
fn layer_offsets(nodes: &[Node], dir: Dir) -> Vec<f64> {
    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    let mut layer_ext = vec![0.0_f64; max_layer + 1];
    for n in nodes {
        let e = match dir {
            Dir::TBlr => n.h,
            Dir::Lr => n.w,
        };
        if e > layer_ext[n.layer] {
            layer_ext[n.layer] = e;
        }
    }
    let gy = RANK_GAP(dir);
    let mut layer_y = vec![0.0_f64; max_layer + 1];
    let mut cursor = gy;
    for l in 0..=max_layer {
        layer_y[l] = cursor;
        cursor += layer_ext[l] + gy;
    }
    layer_y
}

/// Centre of node `n` in SVG coordinates.
fn node_center(n: &Node, layer_y: &[f64], dir: Dir) -> (f64, f64) {
    match dir {
        Dir::TBlr => (n.x_pos, layer_y[n.layer] + n.h / 2.0),
        Dir::Lr => (layer_y[n.layer] + n.w / 2.0, n.x_pos),
    }
}

/// Where the ray from ellipse centre `(cx, cy)` with radii `(rx, ry)`
/// in direction `(dx, dy)` crosses the ellipse outline. Used to clip
/// edge endpoints to the node's visible boundary so the arrow tip
/// touches the stroke instead of floating at the centre.
fn ellipse_intersect(cx: f64, cy: f64, rx: f64, ry: f64, dx: f64, dy: f64) -> (f64, f64) {
    let scale = ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt();
    if scale <= 1e-9 {
        return (cx, cy);
    }
    let t = 1.0 / scale;
    (cx + dx * t, cy + dy * t)
}

/// Render one node as a Graphviz-default ellipse with a centred label.
fn render_node(n: &Node, layer_y: &[f64], dir: Dir) -> String {
    let (cx, cy) = node_center(n, layer_y, dir);
    let rx = n.w / 2.0;
    let ry = n.h / 2.0;
    // Baseline sits slightly below the centre so the cap line
    // balances the descent — ~0.35 of the ellipse radius.
    let baseline = cy + ry * 0.35;
    format!(
        "<ellipse cx=\"{cx:.1}\" cy=\"{cy:.1}\" rx=\"{rx:.1}\" ry=\"{ry:.1}\" \
         fill=\"#ffffff\" stroke=\"#000000\" stroke-width=\"1.5\"/>\
         <text x=\"{cx:.1}\" y=\"{baseline:.1}\" text-anchor=\"middle\" \
         font-family=\"{FONT_FAMILY}\" font-size=\"16\" fill=\"#000000\">{}</text>",
        escape_text(&n.label)
    )
}

/// Render one edge as a straight line clipped to the node outlines.
/// Directed edges carry a `<marker>` arrowhead at the target end.
fn render_edge(e: &Edge, nodes: &[Node], layer_y: &[f64], dir: Dir) -> String {
    let from = &nodes[e.from];
    let to = &nodes[e.to];
    let (fx, fy) = node_center(from, layer_y, dir);
    let (tx, ty) = node_center(to, layer_y, dir);
    let (dx, dy) = (tx - fx, ty - fy);
    let (sx, sy) = ellipse_intersect(fx, fy, from.w / 2.0, from.h / 2.0, dx, dy);
    let (ex, ey) = ellipse_intersect(tx, ty, to.w / 2.0, to.h / 2.0, -dx, -dy);
    let mut s = String::new();
    if e.directed {
        s.push_str(&format!(
            "<line x1=\"{sx:.1}\" y1=\"{sy:.1}\" x2=\"{ex:.1}\" y2=\"{ey:.1}\" \
             stroke=\"#000000\" stroke-width=\"1.5\" marker-end=\"url(#mdsvg-dot-arrow)\"/>"
        ));
    } else {
        s.push_str(&format!(
            "<line x1=\"{sx:.1}\" y1=\"{sy:.1}\" x2=\"{ex:.1}\" y2=\"{ey:.1}\" \
             stroke=\"#000000\" stroke-width=\"1.5\"/>"
        ));
    }
    if !e.label.is_empty() {
        let bw = approx_text_width(&e.label) + 12.0;
        let bh = 18.0;
        let mx = (sx + ex) / 2.0;
        let my = (sy + ey) / 2.0;
        s.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{bw:.1}\" height=\"{bh:.1}\" rx=\"3\" \
             fill=\"#ffffff\" stroke=\"#000000\" stroke-width=\"0.5\"/>",
            mx - bw / 2.0,
            my - bh / 2.0
        ));
        s.push_str(&format!(
            "<text x=\"{mx:.1}\" y=\"{:.1}\" font-size=\"12\" text-anchor=\"middle\" \
             font-family=\"{FONT_FAMILY}\" fill=\"#000000\">{}</text>",
            my + 4.0,
            escape_text(&e.label)
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn renders_digraph() {
        let out = render("digraph G { A -> B; B -> C }").expect("render");
        assert!(out.contains("A"));
        assert!(out.contains("B"));
        assert!(out.contains("C"));
        // Directed edges carry the marker arrowhead.
        assert!(
            out.contains("marker-end=\"url(#mdsvg-dot-arrow)\""),
            "directed edge needs marker arrowhead: {out}"
        );
    }

    #[test]
    fn renders_undirected_edges() {
        let out = render("graph G { A -- B; B -- C }").expect("render");
        assert!(out.contains("<line"));
        // undirected: no arrowhead marker
        assert!(!out.contains("marker-end"), "{out}");
    }

    #[test]
    fn reads_label_attr() {
        let out = render("digraph G { A -> B [label=\"next\"] }").expect("render");
        assert!(out.contains("next"));
    }

    #[test]
    fn chained_edges_create_every_hop() {
        // `A -> B -> C` is short for `A -> B; B -> C;` — the parser
        // used to drop every node after the second, so only A→B was
        // drawn and node C vanished entirely.
        let out = render("digraph G { A -> B -> C; B -> D }").expect("render");
        for label in ["A", "B", "C", "D"] {
            assert!(
                out.contains(&format!(">{label}</text>")),
                "node {label} text label expected in {out}"
            );
        }
        assert_eq!(out.matches("<ellipse ").count(), 4);
        // Three edges: A→B, B→C, B→D.
        let line_count = out.matches("<line x1=").count();
        assert_eq!(
            line_count, 3,
            "expected 3 <line> edges, got {line_count}: {out}"
        );
    }

    #[test]
    fn wide_fan_never_overlaps_siblings() {
        // Regression: the one-shot collision sweep re-collided children
        // (C at x=67 and D at x=87 with rx=35 overlap: 102 > 52). The
        // layout must keep every sibling ellipse disjoint — spreading
        // and lengthening the fan edges when it has to.
        let out = render("digraph G { A -> B -> C; B -> D; B -> E }").expect("render");
        // Parse every <ellipse> as (cx, cy, rx).
        let mut nodes = Vec::new();
        for raw in out.split("<ellipse ").skip(1) {
            let tag = raw.split("/>").next().unwrap_or("");
            let v: Vec<f64> = tag
                .split('"')
                .filter_map(|p| p.trim().parse::<f64>().ok())
                .collect();
            if v.len() >= 3 {
                nodes.push((v[0], v[1], v[2])); // cx, cy, rx
            }
        }
        assert_eq!(nodes.len(), 5, "expected A,B,C,D,E: {out}");
        // The three fan siblings share the bottom layer (same cy).
        let bottom_cy = nodes
            .iter()
            .map(|&(_, cy, _)| cy)
            .fold(f64::NEG_INFINITY, f64::max);
        let mut siblings: Vec<f64> = nodes
            .iter()
            .filter(|&&(_, cy, _)| (cy - bottom_cy).abs() < 1.0)
            .map(|&(cx, _, rx)| cx - rx)
            .collect();
        assert_eq!(siblings.len(), 3, "three fan children expected: {out}");
        siblings.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // Every adjacent sibling must be disjoint: right edge of the
        // left one stays left of the next one's left edge.
        for w in siblings.windows(2) {
            assert!(
                w[0] < w[1],
                "sibling ellipses must not overlap (edges at {:.0} and {:.0})",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn unlabelled_edges_get_no_label_box() {
        // Regression: the target id used to leak onto every edge as a
        // label, producing a stray `B` label box on `A -> B`.
        let out = render("digraph G { A -> B; B -> C }").expect("render");
        assert!(
            !out.contains("rx=\"3\""),
            "no label box should be drawn on unlabelled edges: {out}"
        );
    }

    #[test]
    fn rankdir_lr() {
        let out = render("digraph G { rankdir=LR; A -> B }").expect("render");
        assert!(out.contains("A"));
        assert!(out.contains("B"));
    }

    #[test]
    fn empty_dot_returns_none() {
        assert!(render("digraph G { }").is_none());
    }

    #[test]
    fn matches_reference_geometry() {
        // The reference output for `digraph G { A -> B; B -> C; B -> D }`:
        // ellipse nodes, straight black edges with marker arrowheads,
        // A above B, C and D fanned out below B.
        let out = render("digraph G { A -> B; B -> C; B -> D }").expect("render");
        assert!(out.contains("<ellipse"), "nodes must be ellipses: {out}");
        assert!(out.contains("<line"), "edges must be straight lines: {out}");
        assert!(
            out.contains("marker-end=\"url(#mdsvg-dot-arrow)\""),
            "directed edges need marker arrowheads: {out}"
        );
        // No rectangles beyond the background fill.
        assert!(out.matches("rx=\"3\"").count() == 0, "{out}");

        // Pull (cx, cy) out of each node's <ellipse> by finding the
        // text label that follows it.
        fn ellipse_cy(svg: &str, label: &str) -> f64 {
            let head = svg
                .split_once(&format!(">{label}</text>"))
                .map(|(h, _)| h)
                .unwrap_or("");
            let tail = head.rsplit_once("<ellipse ").map(|(_, t)| t).unwrap_or("");
            tail.split_whitespace()
                .nth(1)
                .and_then(|p| p.split('=').nth(1))
                .and_then(|v| v.trim_matches('"').parse::<f64>().ok())
                .unwrap_or(f64::NEG_INFINITY)
        }
        let (ay, by, cy, dy) = (
            ellipse_cy(&out, "A"),
            ellipse_cy(&out, "B"),
            ellipse_cy(&out, "C"),
            ellipse_cy(&out, "D"),
        );
        assert!(
            ay < by && by < cy && by < dy,
            "A({ay}) must sit above B({by}), and C({cy}) / D({dy}) below"
        );

        // A→B is the first edge and must be a vertical straight line:
        // both endpoints share the x of node A.
        let line = out
            .split_once("<line ")
            .map(|(_, t)| t.split("/>").next().unwrap_or(""))
            .unwrap_or("");
        let nums: Vec<f64> = line
            .split('"')
            .filter_map(|p| p.trim().parse::<f64>().ok())
            .collect();
        assert!(nums.len() >= 4, "A→B line params missing: {line}");
        assert!(
            (nums[0] - nums[2]).abs() < 1.0,
            "A→B should be vertical (x1={}, x2={}): {line}",
            nums[0],
            nums[2]
        );
        assert!(
            nums[3] > nums[1],
            "A→B should point downward (y1={}, y2={})",
            nums[1],
            nums[3]
        );

        // Fan edges must stay clearly visible: with a wide layer gap the
        // B→C and B→D runs each span ≥ 60 px. A too-tight gap collapses
        // them into ~30 px stubs that sit right against the node strokes
        // and read as "B->C is missing".
        let mut edge_lengths = Vec::new();
        for raw in out.split("<line ").skip(1) {
            let tag = raw.split("/>").next().unwrap_or("");
            let v: Vec<f64> = tag
                .split('"')
                .filter_map(|p| p.trim().parse::<f64>().ok())
                .collect();
            if v.len() >= 4 {
                edge_lengths.push(((v[0] - v[2]).powi(2) + (v[1] - v[3]).powi(2)).sqrt());
            }
        }
        assert_eq!(edge_lengths.len(), 3, "three edges expected: {out}");
        assert!(
            edge_lengths.iter().all(|&l| l >= 60.0),
            "every edge must span ≥ 60 px so the fan is visible, got {edge_lengths:?}"
        );
    }
}
