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

use super::common::{approx_text_width, escape_text, fit_node, palette_color, shape_path};

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
    /// Continuous horizontal coordinate (TD/BT) or vertical coordinate
    /// (LR/RL) of the node's center. Replaces the old integer `slot`
    /// ranking so singleton layers can inherit their parent's absolute
    /// position instead of being clamped back to slot 0.
    x_pos: f64,
    /// Within-layer rank offset kept constant across barycenter passes
    /// so multi-node layers never collapse onto the same x.
    bias: f64,
    fill: String,
    /// Cached box width for this label so layout and SVG agree.
    w: f64,
    /// Cached box height.
    h: f64,
}

struct Edge {
    from: usize,
    to: usize,
    label: String,
    style: u8, // 0 solid arrow, 1 plain, 2 dotted, 3 thick
    /// Side of the source the edge exits from (0=top, 1=right,
    /// 2=bottom, 3=left). Computed in `layout` and consumed by
    /// `edge_svg` so the load-balancing pass can override the
    /// natural pick without edge_svg re-deriving it.
    s_side: u8,
    /// Side of the target the edge arrives at. Same lifecycle as
    /// `s_side`.
    e_side: u8,
}

/// Pieces of an edge SVG, kept separate so the renderer can paint
/// them in different z-orders: the line goes down first, then the
/// nodes (which sit on top of the line ends), then the arrowhead
/// (which needs to be on top of the node's stroke band so the tip
/// isn't visually clipped — see the comment in [`Self::edge_svg`]).
/// Labels stay grouped with their edge so the existing ordering
/// between line, arrowhead, and label is preserved where it matters.
struct EdgeParts {
    line: String,
    arrowhead: Option<String>,
    label: Option<String>,
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
    /// Per-layer max width (TD/BT) or max height (LR/RL). Siblings
    /// share the same band so a tall diamond next to a short rect
    /// still renders in the same row.
    layer_max_extent: Vec<f64>,
    /// Total y offset where each layer starts (TD/BT) or x offset
    /// (LR/RL). Pre-computed in `layout` and reused in `pos_of` so
    /// both placement and edge routing agree on the layer geometry.
    layer_y: Vec<f64>,
    /// For each node, the edge indices grouped by which side of the
    /// node they anchor to (0=top, 1=right, 2=bottom, 3=left). Used by
    /// edge_svg to spread parallel edges along the side so they don't
    /// overlap at the same midpoint.
    side_edges: Vec<[Vec<usize>; 4]>,
}

impl Diagram {
    fn parse(src: &str) -> Diagram {
        let mut d = Diagram {
            dir: Dir::Td,
            nodes: Vec::new(),
            edges: Vec::new(),
            subs: Vec::new(),
            idx: HashMap::new(),
            layer_max_extent: Vec::new(),
            layer_y: Vec::new(),
            side_edges: Vec::new(),
        };
        let mut sub_stack: Vec<usize> = Vec::new();
        for raw in src.lines() {
            let mut line = super::common::strip_comments(raw).trim().to_string();
            // strip trailing comma that sometimes ends node decls
            if line.ends_with(',') {
                line.pop();
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
                    layer_max_extent: Vec::new(),
                    layer_y: Vec::new(),
                    side_edges: Vec::new(),
                };
            }
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
        let (w, h) = fit_node(&label);
        // Apply a slightly larger default for non-rectangle shapes (which
        // need breathing room around their visual silhouette).
        let pad = match shape {
            2 => 24.0, // circle
            3 => 24.0, // diamond
            _ => 0.0,
        };
        self.nodes.push(Node {
            label,
            shape,
            layer: 0,
            x_pos: 0.0,
            bias: 0.0,
            fill: palette_color(i).to_string(),
            w: w + pad,
            h: h + pad,
        });
        self.idx.insert(id, i);
        if let Some(&cup) = subs.last() {
            self.subs[cup].nodes.push(i);
        }
        i
    }

    fn layout(&mut self) {
        let n_nodes = self.nodes.len();
        if n_nodes == 0 {
            return;
        }
        // longest-path layering
        let mut depth = vec![0usize; n_nodes];
        for _ in 0..n_nodes {
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
        // adjacency
        let parents: Vec<Vec<usize>> = {
            let mut p = vec![Vec::new(); n_nodes];
            for e in &self.edges {
                p[e.to].push(e.from);
            }
            p
        };
        let children: Vec<Vec<usize>> = {
            let mut c = vec![Vec::new(); n_nodes];
            for e in &self.edges {
                c[e.from].push(e.to);
            }
            c
        };
        // subtree sizes (descendant count, including self) — needed by
        // the balance pass to choose which nodes can be pushed deeper.
        let mut subtree = vec![1usize; n_nodes];
        let mut topo: Vec<usize> = (0..n_nodes).collect();
        topo.sort_by_key(|&i| std::cmp::Reverse(depth[i]));
        for &i in &topo {
            for &p in &parents[i] {
                subtree[p] += subtree[i];
            }
        }
        // balance pass: pure longest-path leaves layer widths uneven
        // (e.g. layer 3 ends up with two nodes while layer 4 has one),
        // so push smaller-subtree nodes one layer deeper whenever it
        // does not violate the edge-ordering constraint. For the
        // user-reported 7-node graph this is exactly what moves D
        // from layer 3 down to layer 4 to share it with F, matching
        // Mermaid's `dagre` layout where the singleton layer above
        // holds E (the branchy parent) instead.
        let max_layer_initial = *depth.iter().max().unwrap_or(&0);
        for layer in 0..max_layer_initial {
            let cur_size = depth.iter().filter(|&&d| d == layer).count();
            let next_size = depth.iter().filter(|&&d| d == layer + 1).count();
            if cur_size <= next_size {
                continue;
            }
            let mut candidates: Vec<usize> = (0..n_nodes).filter(|&i| depth[i] == layer).collect();
            // sort by subtree size ascending — the most "branchless"
            // node is the safest candidate to push deeper.
            candidates.sort_by_key(|&i| subtree[i]);
            for &ni in &candidates {
                let cur_size = depth.iter().filter(|&&d| d == layer).count();
                let next_size = depth.iter().filter(|&&d| d == layer + 1).count();
                if cur_size <= next_size {
                    break;
                }
                // pushing from `layer` to `layer + 1` is safe iff every
                // child of `ni` is at least at `layer + 2`, so that
                // depth[child] > depth[ni] still holds.
                let can_push = children[ni].iter().all(|&c| depth[c] >= layer + 2);
                if !can_push {
                    continue;
                }
                // Don't push a node down if it has a sibling at the
                // same layer — separating siblings forces the parent's
                // edge to one child to cross through the other child's
                // row (e.g. B has two children C and D; pushing D to
                // a deeper row leaves C alone on the middle row and
                // B→D has to zig-zag across C's bbox to reach D).
                // Keeping siblings together is what makes the parent
                // fan out cleanly to the left and right.
                let has_sibling = parents[ni].iter().any(|&p| {
                    children[p]
                        .iter()
                        .any(|&c| c != ni && depth[c] == depth[ni])
                });
                if has_sibling {
                    continue;
                }
                depth[ni] = layer + 1;
            }
        }
        // write back the final depth and rebuild by_layer
        let mut by_layer: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, node) in self.nodes.iter_mut().enumerate() {
            node.layer = depth[i];
            node.x_pos = 0.0;
            node.bias = 0.0;
            by_layer.entry(depth[i]).or_default().push(i);
        }
        let max_layer = *depth.iter().max().unwrap_or(&0);

        // Pre-compute per-layer max extent (height for TD/BT, width for
        // LR/RL). All siblings in a layer share the same band so a
        // tall diamond next to a short rect still sits in the same row.
        let mut layer_max_extent = vec![0.0_f64; max_layer + 1];
        match self.dir {
            Dir::Td | Dir::Bt => {
                for n in &self.nodes {
                    if n.h > layer_max_extent[n.layer] {
                        layer_max_extent[n.layer] = n.h;
                    }
                }
            }
            Dir::Lr | Dir::Rl => {
                for n in &self.nodes {
                    if n.w > layer_max_extent[n.layer] {
                        layer_max_extent[n.layer] = n.w;
                    }
                }
            }
        }
        self.layer_max_extent = layer_max_extent;
        // `layer_y` (the start offset of every layer) is deliberately NOT
        // built here. The x-position passes below (bias, barycenter,
        // collision, hub/upward alignment, swap) read only `x_pos`, `w`
        // and `h` — never `pos_of`, which needs `layer_y`. So the
        // inter-layer gaps can be derived AFTER those passes decide where
        // every node sits horizontally. `rebuild_layer_y` sizes each gap
        // from the widest edge crossing it: an edge whose endpoints are
        // far apart cross-flow needs proportional along-flow room, or its
        // cubic bezier is squeezed into a tight, corner-like curve.
        // bias: a node whose parent sits in the layer immediately above
        // (e.g. F under E) inherits its parent's x and only gets a small
        // lateral nudge to avoid overlapping with that parent. Nodes
        // whose parent is further up (e.g. D, whose parent C is two
        // layers above after the balance pass pushed it down) get the
        // full rank-based spread so they fan out to the side.
        let step = 240.0_f64;
        for layer in 0..=max_layer {
            if let Some(ns) = by_layer.get(&layer) {
                let mut sorted: Vec<usize> = ns.clone();
                // sort by parent x (so children of left-leaning parents
                // stay left), with subtree size as tiebreaker so the
                // branchy node stays closer to center.
                sorted.sort_by(|&a, &b| {
                    let ax = if parents[a].is_empty() {
                        0.0
                    } else {
                        parents[a].iter().map(|&p| self.nodes[p].x_pos).sum::<f64>()
                            / parents[a].len() as f64
                    };
                    let bx = if parents[b].is_empty() {
                        0.0
                    } else {
                        parents[b].iter().map(|&p| self.nodes[p].x_pos).sum::<f64>()
                            / parents[b].len() as f64
                    };
                    // Sort keys, in priority order:
                    // 1. parent x — children stay on the side of their
                    //    parent (so a left-leaning parent's chain
                    //    doesn't sprawl right).
                    // 2. subtree size DESCENDING — bigger subtrees get
                    //    the closer rank (rank 0 / -0.5), smaller
                    //    subtrees fan to the edge.
                    // 3. label alphabetical — a deterministic,
                    //    source-order-INDEPENDENT tiebreaker so the
                    //    layout doesn't flip when a user reorders the
                    //    是/否 edges in their Mermaid source.
                    ax.partial_cmp(&bx)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(subtree[b].cmp(&subtree[a]))
                        .then(self.nodes[a].label.cmp(&self.nodes[b].label))
                });
                let count = sorted.len() as f64;
                for (i, &ni) in sorted.iter().enumerate() {
                    let rank = i as f64 - (count - 1.0) / 2.0;
                    // parent layer distance: how many layers between the
                    // closest parent and `ni`. `1` means directly below.
                    // Roots (no parents) use a large sentinel so they
                    // fall into the rank-based branch with bias = 0 for
                    // singletons (otherwise their bias would equal
                    // `nudge` and shift A, B, C apart from each other).
                    let parent_layer_diff = if parents[ni].is_empty() {
                        usize::MAX
                    } else {
                        let min_parent = parents[ni]
                            .iter()
                            .map(|&p| depth[p])
                            .min()
                            .unwrap_or(depth[ni]);
                        depth[ni] - min_parent
                    };
                    // Direct children (parent in immediately previous
                    // layer) inherit their parent's x exactly so the
                    // chain A→B→C stays vertically aligned. Pushed or
                    // displaced nodes use the full rank-based spread
                    // so D gets fanned out to the side after the
                    // balance pass.
                    self.nodes[ni].bias = if parent_layer_diff == 1 {
                        0.0
                    } else {
                        rank * step
                    };
                    self.nodes[ni].x_pos = self.nodes[ni].bias;
                }
            }
        }
        // Barycenter: downward passes only. Each node's new x is its
        // bias plus an aggregate of its parents' x — that way a
        // singleton layer inherits its only parent's absolute
        // position rather than being clamped back to 0 (the failure
        // mode of the old integer-slot layout, which stacked F
        // directly under D).
        //
        // For nodes with several parents (typical of a hub like
        // `纯静态页面`, fed from D, E, and F) the mean would land G
        // halfway between the leftmost and rightmost parent — visually
        // orphaned under neither of them. Anchoring to the rightmost
        // parent (`max`) puts the hub directly under the right branch
        // (the F column in our 7-node example), which is what the
        // user expects when they say `G 应该在 F 正下方`. Single-parent
        // nodes use the only parent, identical to the old behaviour.
        for _round in 0..4 {
            for layer in 1..=max_layer {
                if let Some(ns) = by_layer.get(&layer) {
                    for &ni in ns {
                        if parents[ni].is_empty() {
                            continue;
                        }
                        let agg = if parents[ni].len() == 1 {
                            self.nodes[parents[ni][0]].x_pos
                        } else {
                            parents[ni]
                                .iter()
                                .map(|&p| self.nodes[p].x_pos)
                                .fold(f64::NEG_INFINITY, f64::max)
                        };
                        self.nodes[ni].x_pos = self.nodes[ni].bias + agg;
                    }
                }
            }
        }

        // Detect collisions in TD/BT modes after barycenter — two nodes
        // in the same layer can end up at the same x if one has many
        // parents pulling it left and the other has many pulling it
        // right. Push them apart by half their combined widths so the
        // rendered boxes don't visually overlap.
        let h_gap = 32.0_f64;
        for layer in 0..=max_layer {
            if let Some(ns) = by_layer.get(&layer) {
                if ns.len() < 2 {
                    continue;
                }
                // A single left-to-right pass does NOT suffice: pushing
                // the left member of a pair left can re-collide it with
                // its own left neighbour, so one `windows(2)` sweep
                // leaves tight nodes only ~0–25 px apart instead of the
                // required half-widths + gap (visible as overlapping
                // boxes, e.g. seven siblings fanned out of one hub).
                // Re-run until no pair needs nudging — each pass is
                // monotone (a re-collision always reduces the overlap),
                // and the `n-1` bound guarantees a wide fan resolves.
                let mut order: Vec<usize> = ns.clone();
                // Sort by x_pos first so we walk left-to-right when
                // pushing siblings apart. The `label` tiebreaker is
                // critical: without it the sort is stable on equal
                // x_pos, which preserves whatever order `by_layer`
                // returned — and that order comes from a HashMap, so
                // it's an artifact of edge insertion order. The user
                // sees this as: reordering 是/否 edges in their source
                // swaps which branch lands on the left side. Sorting
                // by label alphabetically makes the push deterministic
                // regardless of source order.
                order.sort_by(|&a, &b| {
                    self.nodes[a]
                        .x_pos
                        .partial_cmp(&self.nodes[b].x_pos)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(self.nodes[a].label.cmp(&self.nodes[b].label))
                });
                for _ in 0..=ns.len() {
                    let mut any = false;
                    for w in order.windows(2) {
                        let a = w[0];
                        let b = w[1];
                        let aw = self.nodes[a].w;
                        let bw = self.nodes[b].w;
                        let needed = (aw + bw) / 2.0 + h_gap;
                        let cur = self.nodes[b].x_pos - self.nodes[a].x_pos;
                        if cur < needed {
                            let push = (needed - cur) / 2.0;
                            self.nodes[a].x_pos -= push;
                            self.nodes[b].x_pos += push;
                            any = true;
                        }
                    }
                    if !any {
                        break;
                    }
                }
            }
        }

        // Post-collision hub alignment: a hub node (one with several
        // parents) was placed by the barycenter above using the
        // *pre-collision* x of its parents. If a sibling collision in
        // any parent's layer pushed the rightmost parent sideways
        // (e.g. F got nudged from 0 to +4.3 in the 7-node example
        // because D and F share layer 4), the hub inherits the
        // stale position and ends up offset from its visual
        // anchor — `G` lands 4.3 px left of `F` even though the
        // user expects `G 应该在 F 正下方`. Re-snap the hub to the
        // rightmost parent's *final* position so the alignment
        // survives collision nudging.
        for ni in 0..n_nodes {
            if parents[ni].len() < 2 {
                continue;
            }
            let rightmost = parents[ni]
                .iter()
                .map(|&p| self.nodes[p].x_pos)
                .fold(f64::NEG_INFINITY, f64::max);
            self.nodes[ni].x_pos = self.nodes[ni].bias + rightmost;
        }

        // Upward alignment pass: when a node has 2+ children that
        // landed on the *same* side of the parent (both left or both
        // right), nudge the parent so it sits above its children's
        // stack instead of under its own parent. Without this the
        // `E → F → G` branch in the 7-node example keeps E directly
        // under C (centre column) while F and G end up slightly
        // right of E — the user reads it as "E didn't move". When
        // children's spread is small (both on the same side), snap
        // the parent to the rightmost child so the whole branch
        // shifts together as a visual unit. We do this *after* the
        // hub alignment so the parent sees the hub's final position
        // (G → F → E collapses into a single right-side stack).
        for layer in (1..=max_layer).rev() {
            if let Some(ns) = by_layer.get(&layer) {
                for &ni in ns {
                    if children[ni].len() < 2 {
                        continue;
                    }
                    let child_xs: Vec<f64> =
                        children[ni].iter().map(|&c| self.nodes[c].x_pos).collect();
                    let leftmost = child_xs.iter().cloned().fold(f64::INFINITY, f64::min);
                    let rightmost = child_xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    if rightmost - leftmost < 60.0 {
                        self.nodes[ni].x_pos = self.nodes[ni].bias + rightmost;
                    }
                }
            }
        }

        // Aesthetic swap pass: for each parent with exactly two
        // children sitting on opposite sides, try swapping them and
        // keep whichever arrangement leaves the smaller total
        // horizontal edge span across both subtrees. This breaks the
        // dependency on source order — yes/no (是/否) labels are
        // bound to edges, so swapping the children moves the labels
        // too, and the algorithm picks the layout that gives the
        // tightest downstream geometry. Descendants shift with their
        // root so the relative geometry inside each subtree is
        // preserved.
        for ni in 0..n_nodes {
            let ch = children[ni].clone();
            if ch.len() != 2 {
                continue;
            }
            let (c1, c2) = (ch[0], ch[1]);
            if depth[c1] != depth[c2] {
                continue;
            }
            let parent_x = self.nodes[ni].x_pos;
            let c1_x = self.nodes[c1].x_pos;
            let c2_x = self.nodes[c2].x_pos;
            let dx1 = c1_x - parent_x;
            let dx2 = c2_x - parent_x;
            if dx1.signum() * dx2.signum() >= 0.0 {
                continue;
            }
            // Sum of |child x - parent x| over each node in the two
            // subtrees, where each subtree is hypothetically rooted
            // at `root_x`. Pre-compute the contribution per node so
            // we don't need to borrow `self.nodes` while mutating.
            let score = |root: usize, root_x: f64, snapshot: &[f64]| -> f64 {
                let shift = root_x - snapshot[root];
                let mut s = 0.0_f64;
                let mut stack: Vec<(usize, f64)> = vec![(root, shift)];
                while let Some((cur, cur_shift)) = stack.pop() {
                    for &p in &parents[cur] {
                        let reference = if p == ni { parent_x } else { snapshot[p] };
                        s += (snapshot[cur] + cur_shift - reference).abs();
                    }
                    for &gc in &children[cur] {
                        stack.push((gc, cur_shift));
                    }
                }
                s
            };
            let snapshot: Vec<f64> = self.nodes.iter().map(|n| n.x_pos).collect();
            let cur_score = score(c1, c1_x, &snapshot) + score(c2, c2_x, &snapshot);
            let shift_c1 = c2_x - c1_x;
            let shift_c2 = c1_x - c2_x;
            let mut stack: Vec<usize> = vec![c1];
            while let Some(cur) = stack.pop() {
                self.nodes[cur].x_pos += shift_c1;
                for &gc in &children[cur] {
                    stack.push(gc);
                }
            }
            let mut stack: Vec<usize> = vec![c2];
            while let Some(cur) = stack.pop() {
                self.nodes[cur].x_pos += shift_c2;
                for &gc in &children[cur] {
                    stack.push(gc);
                }
            }
            let swap_score = score(c1, c2_x, &snapshot) + score(c2, c1_x, &snapshot);
            // Decide whether to keep the swap:
            // - Strictly better (swap_score < cur_score): keep it.
            // - Tied (swap_score == cur_score): apply the canonical
            //   alphabetical rule so the result doesn't depend on
            //   which child appeared first in the Mermaid source.
            //   We want label[c1] (the LEFT child) to come first
            //   alphabetically; if it doesn't, the swap is the
            //   canonical arrangement.
            // - Strictly worse: revert.
            // Canonical "LEFT child has the smaller label" rule. The
            // swap code above puts c1 where c2 was and vice versa, so
            // whoever is on LEFT after the swap depends on which side
            // c1 sat on before. If c1 was LEFT, the swap takes c2 to
            // LEFT — we want c2's label to be smaller. If c1 was
            // RIGHT, the swap takes c1 to LEFT — we want c1's label
            // to be smaller. Both clauses are equivalent to the
            // XOR below.
            let c1_left = c1_x < c2_x;
            let c1_smaller = self.nodes[c1].label.as_str() < self.nodes[c2].label.as_str();
            let canonical_wants_swap = c1_left != c1_smaller;
            let keep = if swap_score < cur_score {
                true
            } else if (swap_score - cur_score).abs() < 1e-9 {
                canonical_wants_swap
            } else {
                false
            };
            if !keep {
                let mut stack: Vec<usize> = vec![c1];
                while let Some(cur) = stack.pop() {
                    self.nodes[cur].x_pos -= shift_c1;
                    for &gc in &children[cur] {
                        stack.push(gc);
                    }
                }
                let mut stack: Vec<usize> = vec![c2];
                while let Some(cur) = stack.pop() {
                    self.nodes[cur].x_pos -= shift_c2;
                    for &gc in &children[cur] {
                        stack.push(gc);
                    }
                }
            }
        }

        // normalize: shift every x_pos so the leftmost (TD/BT) or
        // topmost (LR/RL) node sits at GX (resp. GY) from the origin.
        // Without this the viewBox would start at a negative offset and
        // some browsers clip the left edge.
        // Use each node's own w/h so the offset matches what we draw.
        let gx = 28.0_f64;
        let gy = match self.dir {
            Dir::Td | Dir::Bt => 28.0_f64,
            Dir::Lr | Dir::Rl => 80.0_f64,
        };
        match self.dir {
            Dir::Td | Dir::Bt => {
                // Use max half-width on the leftmost side so labels that
                // extend past the bbox (CJK) don't clip the left edge.
                let (min_x, max_hw) = self
                    .nodes
                    .iter()
                    .fold((f64::INFINITY, 0.0_f64), |(mx, hw), n| {
                        (mx.min(n.x_pos), hw.max(n.w / 2.0))
                    });
                let pad = gx - (min_x - max_hw);
                for n in &mut self.nodes {
                    n.x_pos += pad;
                }
            }
            Dir::Lr | Dir::Rl => {
                let (min_y, max_hh) = self
                    .nodes
                    .iter()
                    .fold((f64::INFINITY, 0.0_f64), |(my, hh), n| {
                        (my.min(n.x_pos), hh.max(n.h / 2.0))
                    });
                let pad = gy - (min_y - max_hh);
                for n in &mut self.nodes {
                    n.x_pos += pad;
                }
            }
        }
        // Now that every node has its final cross-flow position, derive
        // the per-layer start offsets. Edge-behaving gaps need the real
        // x/y spread between connected nodes, which is exactly what the
        // by-layer passes above produced.
        self.rebuild_layer_y();
        // precompute, for each node, which edges attach to which side
        // (0=top, 1=right, 2=bottom, 3=left) — used by edge_svg to
        // fan parallel edges out along the side instead of stacking
        // them on top of each other at the midpoint.
        // Keep `source_side` and `target_side` per edge as separate
        // vectors so the redistribution pass below can move edges
        // between target sides without losing track of the source.
        let mut source_side = vec![0u8; self.edges.len()];
        let mut target_side = vec![0u8; self.edges.len()];
        for (ei, e) in self.edges.iter().enumerate() {
            let from = &self.nodes[e.from];
            let to = &self.nodes[e.to];
            let (x1, y1) = self.pos_of(from);
            let (x2, y2) = self.pos_of(to);
            // Pass the *other* node's center to anchor_side — we want
            // the direction from `from` to `to`, so the offset must
            // come from `to`'s own half-dimensions.
            source_side[ei] = self.anchor_side(from, x2 + to.w / 2.0, y2 + to.h / 2.0) as u8;
            target_side[ei] = self.anchor_side(to, x1 + from.w / 2.0, y1 + from.h / 2.0) as u8;
        }
        // Load-balancing pass: when ≥ 3 edges all naturally pick the
        // same target side (typical for a hub like `纯静态页面` with
        // several incoming edges), spread them across neighbouring
        // sides so each edge enters the side that actually faces its
        // source. We sort the cluster by horizontal displacement
        // (most negative first, most positive last) and assign:
        //   • most-negative dx → left  (source is left of target)
        //   • middle            → right (closest to vertical, so the
        //                           curve can bend around obstacles
        //                           without looping back on itself)
        //   • most-positive dx  → top  (sits on the natural side; the
        //                           edge with the largest horizontal
        //                           component naturally drops down to
        //                           the top with a clean curve)
        // Only fires when the cluster spans both sides of the target
        // (mixed dx signs) — otherwise the edges all genuinely come
        // from one direction and we should leave them stacked on the
        // natural side.
        for node_idx in 0..self.nodes.len() {
            let incoming: Vec<usize> = (0..self.edges.len())
                .filter(|&ei| self.edges[ei].to == node_idx)
                .collect();
            if incoming.len() != 3 {
                // The left/right/top assignment below is hand-tuned
                // for the 3-edge hub case. Clusters of other sizes
                // skip the pass and rely on the natural pick.
                continue;
            }
            // Group by target side.
            let mut by_side: [Vec<usize>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
            for &ei in &incoming {
                by_side[target_side[ei] as usize].push(ei);
            }
            // Only act if all 3 edges cluster on a single side.
            let dominant = by_side.iter().position(|v| v.len() == 3);
            let dominant = match dominant {
                Some(s) => s,
                None => continue,
            };
            // Score each edge by horizontal displacement from the
            // target's centre to its source's centre: negative = source
            // is left of target, positive = source is right of target.
            let mut with_dx: Vec<(usize, f64)> = by_side[dominant]
                .iter()
                .map(|&ei| {
                    let e = &self.edges[ei];
                    let from = &self.nodes[e.from];
                    let to = &self.nodes[node_idx];
                    let (xf, _) = self.pos_of(from);
                    let (xt, _) = self.pos_of(to);
                    let from_cx = xf + from.w / 2.0;
                    let to_cx = xt + to.w / 2.0;
                    (ei, from_cx - to_cx)
                })
                .collect();
            with_dx.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            // Only redistribute if the cluster has sources on BOTH
            // sides of the target — otherwise the edges all genuinely
            // come from one direction and we should leave them stacked
            // on the natural side. `>= 0.0` (not strict `> 0.0`) so we
            // still redistribute when the rightmost source sits exactly
            // at the target's x; the strict check collapsed the pass
            // after the right-column alignment pulled E and F in line
            // with G, leaving three edges all stacked on G's top edge
            // instead of the expected left/right/top distribution.
            // Use a small epsilon (1e-6) instead of literal 0.0:
            // E→G has dx=0 exactly when E and G share the same x,
            // but floating-point arithmetic on `x_pos` can land at
            // `2.84e-14` instead of 0.0, so a strict `>= 0.0` would
            // skip the redistribution and leave three edges stacked
            // on the top side. The earlier comment "≥ 0.0 not strict
            // > 0.0" was correct in intent but the tolerance was
            // implicit (assumed exact zero) — write it down.
            //
            // Earlier this pass also redistributed to LEFT/RIGHT/TOP,
            // but that forced F→G (F is left of G) onto RIGHT, making
            // the curve sweep across the entire G box. Cleaner: keep
            // all 3 on TOP and rely on the per-side rank fan-out
            // (rank 0 = leftmost slot, rank 2 = rightmost slot) to
            // spread them. The side-normal end_tan fix in
            // bezier_segment makes each entry a clean vertical
            // arrival.
            if with_dx[0].1 < 0.0 && with_dx[2].1 >= -1e-6 {
                // Keep all 3 on TOP — per-side rank fan-out (now
                // sorted by source x) handles the lateral spread.
            }
        }
        // Stamp the (possibly redistributed) sides onto the edges
        // themselves so edge_svg doesn't have to re-derive them — if
        // it called `anchor_side` it would get the pre-redistribution
        // natural side, defeating the load-balancing pass.
        for (ei, e) in self.edges.iter_mut().enumerate() {
            e.s_side = source_side[ei];
            e.e_side = target_side[ei];
        }
        // Now fold source_side and (possibly updated) target_side
        // into the per-node side index used by edge_svg.
        let mut side_edges: Vec<[Vec<usize>; 4]> =
            self.nodes.iter().map(|_| Default::default()).collect();
        for (ei, e) in self.edges.iter().enumerate() {
            side_edges[e.from][source_side[ei] as usize].push(ei);
            side_edges[e.to][target_side[ei] as usize].push(ei);
        }
        // Sort each side's edge list by source x so the rank assigned
        // by `anchor_with_offset` correlates with where the source
        // actually sits, not with edge enumeration order. Without
        // this, the 3 edges to 纯静态页面 spread their TOP-side fan-out
        // by edge order (E→G → LEFT slot, F→G → RIGHT slot) even
        // though E is the closest to G's center. The result is a
        // crossing: E→G sweeps from E's right vertex all the way to
        // G's top-LEFT corner instead of staying near the centre.
        let cx_of = |ei: usize| -> f64 {
            let n = &self.nodes[self.edges[ei].from];
            let (x, _) = self.pos_of(n);
            x + n.w / 2.0
        };
        for node_side in side_edges.iter_mut() {
            for bucket in node_side.iter_mut() {
                if bucket.len() > 1 {
                    bucket.sort_by(|&a, &b| {
                        cx_of(a)
                            .partial_cmp(&cx_of(b))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
            }
        }
        self.side_edges = side_edges;
    }

    /// Rebuild the per-layer start offsets (`layer_y`) with inter-layer
    /// gaps that are computed from the layout geometry instead of a
    /// fixed constant. Each boundary between two adjacent layers gets a
    /// gap large enough to give the widest edge crossing it room to bend
    /// smoothly: an edge whose endpoints are far apart cross-flow needs
    /// proportional along-flow room, otherwise its cubic bezier is
    /// squeezed into a tight, corner-like curve. This is what turns the
    /// old fixed `gy` — small enough that long diagonal connectors
    /// rendered as two near-straight segments meeting at a sharp angle —
    /// into a gap that grows with the curves it must host.
    ///
    /// Must run AFTER every x-position pass (bias, barycenter, collision,
    /// swaps, normalize) because it reads each node's final `x_pos`; the
    /// by-layer passes never touch `layer_y`, so there is no ordering
    /// hazard in deferring it.
    fn rebuild_layer_y(&mut self) {
        let n_layers = self.layer_max_extent.len();
        let base_gap = match self.dir {
            Dir::Td | Dir::Bt => 28.0_f64,
            Dir::Lr | Dir::Rl => 80.0_f64,
        };
        if n_layers <= 1 {
            self.layer_y = vec![base_gap; n_layers];
            return;
        }
        // Minimum gap an edge may require. Kept modest for Td/Bt so short
        // diagrams stay compact; LR/RL is wider to seat edge labels.
        // Per boundary gap. `boundary gap[l]` separates layer `l` from
        // layer `l + 1`. Start from the base, then inflate each boundary
        // by the widest single-hop edge crossing it. Long edges that hop
        // several layers at once don't tell us which individual boundary
        // needs the space, so they don't inflate any of them — the tight
        // single-hop edges are the ones that visually need it most.
        let mut gap = vec![base_gap; n_layers - 1];
        for e in &self.edges {
            let a = self.nodes[e.from].layer;
            let b = self.nodes[e.to].layer;
            // Only single-hop edges (endpoints on adjacent layers) drive
            // the gap — they are the ones whose curve is confined to one
            // strip and which visibly need the room.
            if a + 1 != b && a != b + 1 {
                continue;
            }
            // In Td/Bt the cross-flow axis is x; in Lr/Rl it is y. Both
            // live in `x_pos` (renamed for clarity but shared). The gap
            // an edge needs scales with how far apart its endpoints are
            // *laterally*: a vertical parent->child chain can ride a slim
            // gap, while a long sideways span demands a tall one.
            let lateral = (self.nodes[e.from].x_pos - self.nodes[e.to].x_pos).abs();
            if lateral < 1e-9 {
                continue;
            }
            // The bezier control-handle projection along the flow axis is
            // `off = max(len*0.4, 20)`, whose along-flow component is
            // capped so the curve has room for its bend plus a little
            // margin. A wider gap also lets a dense fan's curves fan out
            // (overlap scales with how compressed the channel is), so a
            // higher lateral ratio costs nothing for sparse graphs but
            // opens up tight hubs. Ceiling guards against a pathological
            // wide fan exploding the diagram.
            let needed = (lateral * 0.5).clamp(base_gap, 260.0);
            let boundary = a.min(b);
            if needed > gap[boundary] {
                gap[boundary] = needed;
            }
        }
        let mut layer_y = Vec::with_capacity(n_layers);
        let mut cursor = base_gap;
        for (l, g) in gap.iter().copied().enumerate() {
            layer_y.push(cursor);
            cursor += self.layer_max_extent[l] + g;
        }
        layer_y.push(cursor);
        self.layer_y = layer_y;
    }

    /// Which side of `n` (0=top, 1=right, 2=bottom, 3=left) an edge
    /// heading toward `(tx, ty)` should anchor to. Pure layout — does
    /// not know anything about parallel edges; that fan-out happens
    /// later in `anchor_with_offset` once `side_edges` is built.
    ///
    /// We use the *ratio* of horizontal displacement to total
    /// displacement instead of the absolute `|dx| > |dy|` test. The
    /// pure-magnitude test forces every near-vertical edge to share
    /// the bottom side, which makes two diagonally fanned children
    /// (one going down-right, one going down-left) start at the same
    /// bottom edge and cross each other near the source vertex — the
    /// exact bug behind the user-reported `E→F` / `E→G` crossing in
    /// the 7-node example. Routing the left-leaning edge out of the
    /// left side avoids the crossover entirely.
    fn anchor_side(&self, n: &Node, tx: f64, ty: f64) -> usize {
        let (x, y) = self.pos_of(n);
        let cx = x + n.w / 2.0;
        let cy = y + n.h / 2.0;
        let dx = tx - cx;
        let dy = ty - cy;
        let total = dx.abs() + dy.abs();
        if total > 0.0 {
            // 0.7 means horizontal displacement is below ~70 % of the
            // total — the target is closer to straight up/down than
            // to straight left/right, so the natural anchor is the
            // top/bottom side. Above 0.7 the target is clearly
            // sideways and left/right is correct. The threshold sits
            // above the C→D ratio (0.43, both endpoints pick top/
            // bottom) and above the D→G ratio (0.63, both endpoints
            // pick top/bottom — the user expects D→G to leave D's
            // bottom-middle, not its right-middle). 0.75 (not 0.7)
            // keeps a mostly-horizontal fan edge like B→I (h_ratio
            // 0.72, B far left of the hub) on the TOP side with its
            // siblings instead of diverting it to a sideways entry
            // that threads the channel and grazes a neighbour.
            let h_ratio = dx.abs() / total;
            if h_ratio > 0.75 {
                if dx < 0.0 {
                    3
                } else {
                    1
                }
            } else if dy < 0.0 {
                0
            } else {
                2
            }
        } else {
            2
        }
    }

    /// Anchor point on the chosen `side` of `n`, offset along that side
    /// by `rank` of `count` so multiple parallel edges don't collide.
    /// `rank` is 0-indexed so the spread is symmetric around the
    /// midpoint.
    ///
    /// Diamonds get a different treatment from rectangles: a rect's
    /// bottom (side 2) is an actual edge so spreading along x keeps
    /// the anchor on the shape. A diamond's "bottom" is a single
    /// vertex, so spreading along the y of that vertex would place
    /// anchors *below* the diamond in empty space — and two
    /// diagonally fanned edges (one down-right, one down-left) would
    /// then start in the air, with their bezier control points
    /// pulling the curves toward each other, producing an immediate
    /// crossover (the user-reported `E→F` / `E→G` bug).
    ///
    /// Fix: for diamonds on side 0/2, distribute the anchors along
    /// the two adjacent edges (e.g. bottom-LEFT and bottom-RIGHT)
    /// instead of along the y of the bottom vertex. Rank 0 lands on
    /// the bottom-LEFT edge near the left vertex, the last rank on
    /// the bottom-RIGHT edge near the right vertex, and any middle
    // rank at the bottom vertex itself. Each anchor now sits ON the
    /// diamond silhouette, so two diverging edges leave the shape on
    /// physically separate edges and never cross near the source.
    fn anchor_with_offset(&self, n: &Node, side: usize, rank: usize, count: usize) -> (f64, f64) {
        let (x, y) = self.pos_of(n);
        let cx = x + n.w / 2.0;
        let cy = y + n.h / 2.0;
        let is_diamond = n.shape == 3;
        // For diamonds on a vertical side (top/bottom), use the
        // edge-spreading scheme above. For horizontal sides
        // (left/right) the standard offset along the y of the
        // midpoint still keeps anchors on the diamond edge, so the
        // simple rect-style spread is fine there.
        if is_diamond && (side == 0 || side == 2) {
            if count <= 1 {
                if side == 0 {
                    (cx, y)
                } else {
                    (cx, y + n.h)
                }
            } else {
                // `frac` measures how far from the centre rank we
                // are, normalised to [0, 1]. Ranks below the centre
                // go onto the LEFT edge (for side 2, that's the
                // bottom-LEFT edge; for side 0, the top-LEFT edge),
                // ranks above go to the RIGHT edge. The further from
                // centre, the closer to the side vertex we land.
                let mid = (count - 1) as f64 / 2.0;
                let r = rank as f64;
                let frac = ((r - mid).abs() / mid).clamp(0.0, 1.0);
                // Halfway along the diagonal edge from the apex
                // (top/bottom vertex) to the corresponding side
                // vertex keeps the anchor on the diamond silhouette
                // while leaving enough room for the bezier to
                // diverge.
                let edge_pos = match side {
                    0 => ((cx, y), (x, cy), (x + n.w, cy)), // apex, left, right
                    _ => ((cx, y + n.h), (x, cy), (x + n.w, cy)),
                };
                if r < mid {
                    // LEFT edge: from apex toward left vertex.
                    let ax = edge_pos.0 .0 + frac * (edge_pos.1 .0 - edge_pos.0 .0);
                    let ay = edge_pos.0 .1 + frac * (edge_pos.1 .1 - edge_pos.0 .1);
                    (ax, ay)
                } else if r > mid {
                    // RIGHT edge: from apex toward right vertex.
                    let ax = edge_pos.0 .0 + frac * (edge_pos.2 .0 - edge_pos.0 .0);
                    let ay = edge_pos.0 .1 + frac * (edge_pos.2 .1 - edge_pos.0 .1);
                    (ax, ay)
                } else {
                    edge_pos.0
                }
            }
        } else {
            let spread_frac = if is_diamond { 0.28 } else { 0.7 };
            let offset = if count > 1 {
                let side_len = match side {
                    0 | 2 => n.w,
                    _ => n.h,
                };
                // Dense fan sides: floor the spread so parallel edges
                // keep a ~20 px step even when the side itself is
                // narrow, and clamp it to the side length so the
                // anchors never float past the node's corners (an
                // arrowhead drawn in mid-air beside the box looks
                // broken). A 6-edge fan into a 70 px box with the
                // standard 70 px-wide spread crowds the curves into
                // 8-10 px mid-path gaps; pushing the anchors to the
                // full side opens the fan. Circles are exempt — an
                // anchor beyond the disc silhouette would float in
                // empty air next to the stroke.
                let min_span = if n.shape == 2 {
                    0.0
                } else {
                    20.0 * (count - 1) as f64
                };
                let span = (side_len * spread_frac).max(min_span).min(side_len);
                let step = span / (count - 1) as f64;
                -span / 2.0 + rank as f64 * step
            } else {
                0.0
            };
            match side {
                // Circles are bounded by a box that includes a 24 px
                // pad around the visible disc, so anchoring at the
                // bbox edge leaves an 11 px gap between the line tip
                // and the circle stroke. Snap each anchor onto the
                // actual circle silhouette instead.
                0 if n.shape == 2 => (cx + offset, cy - Self::circle_r(n)),
                1 if n.shape == 2 => (cx + Self::circle_r(n), cy + offset),
                2 if n.shape == 2 => (cx + offset, cy + Self::circle_r(n)),
                3 if n.shape == 2 => (cx - Self::circle_r(n), cy + offset),
                0 => (cx + offset, y),
                1 => (x + n.w, cy + offset),
                2 => (cx + offset, y + n.h),
                _ => (x, cy + offset),
            }
        }
    }

    /// Visible radius of a circle node — the bbox includes a 24 px
    /// shape pad, so `n.w / 2` would be 11 px wider than the actual
    /// disc. Mirrors the formula in `shape_path` for shape 2 so the
    /// anchors line up with what the SVG draws.
    fn circle_r(n: &Node) -> f64 {
        (n.h / 2.0).min(n.w / 2.0) + 6.0
    }

    fn pos_of(&self, n: &Node) -> (f64, f64) {
        // x_pos is the node's center along the perpendicular axis;
        // pos_of returns the top-left corner so shape paths anchor
        // the same way they did before the slot-to-x_pos rewrite.
        // The layer's start uses the per-layer max extent so siblings
        // line up even when their heights differ.
        match self.dir {
            Dir::Td | Dir::Bt => (n.x_pos - n.w / 2.0, self.layer_y[n.layer]),
            Dir::Lr | Dir::Rl => (self.layer_y[n.layer], n.x_pos - n.h / 2.0),
        }
    }

    /// Path data for the arrowhead placed at the tip of an edge. Sized so
    /// it visually balances a 1.6 px stroke.
    fn arrowhead_path(&self, px: f64, py: f64, angle: f64) -> String {
        let (s, c) = (angle.sin(), angle.cos());
        let back = 9.0;
        let side = 5.0;
        let bx = px - back * c;
        let by = py - back * s;
        let p1x = bx + side * (-s);
        let p1y = by + side * c;
        let p2x = bx + side * s;
        let p2y = by - side * c;
        format!("M {px:.1} {py:.1} L {p1x:.1} {p1y:.1} L {p2x:.1} {p2y:.1} Z")
    }

    /// One cubic-bezier segment between two points. Used directly when
    /// an edge has a single span, or chained together with multiple
    /// `C` commands when an edge needs via-points to dodge obstacles.
    ///
    /// The two control points ride along *different* tangents so the
    /// curve has visible bend at both ends instead of degenerating
    /// into a straight line:
    ///
    /// - `start_tan` (a unit vector) controls the tangent at the
    ///   source. For the first segment of an edge this is the side
    ///   normal — e.g. C's left vertex exits going LEFT, even though
    ///   the line itself heads down-LEFT toward D.
    /// - The end tangent is always aligned with the line direction
    ///   (`ex-sx, ey-sy`) so the arrowhead sits flush with the final
    ///   tangent. This is the missing piece the previous S-curve
    ///   version got wrong: it forced vertical tangents for TD, so
    ///   the curve arrived at D's top going straight DOWN while the
    ///   arrow (computed from the line angle) pointed down-LEFT.
    ///
    /// When the start tangent matches the line direction (e.g.
    /// straight vertical edges where source side 2 = bottom and the
    /// line goes straight down), both control points lie on the
    /// start→end line and the bezier collapses to a straight line —
    /// which is what you want for a vertical edge anyway.
    fn bezier_segment(
        &self,
        dir: Dir,
        sx: f64,
        sy: f64,
        ex: f64,
        ey: f64,
        start_tan: (f64, f64),
        end_tan: Option<(f64, f64)>,
        force_curve: bool,
    ) -> String {
        let dx = ex - sx;
        let dy = ey - sy;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            return format!("C {sx:.1} {sy:.1} {ex:.1} {ey:.1} {ex:.1} {ey:.1}");
        }
        // Collinear shortcut: when the start tangent is already aligned
        // with the line direction (the typical case for rect anchors
        // whose side normal matches the line, or for any chain whose
        // previous segment ended on the same heading), the bezier
        // collapses to a straight line. Emitting `L` instead of `C`
        // removes the S-bend that `c1`/`c2` would otherwise carve out
        // of the control polygon — without this fix, A→B and B→C
        // render as `M 207.2 64 C 207.2 84 207.2 72 207.2 92`, where
        // the actual curve is degenerate but the source looks like a
        // kink. Threshold `0.95` is loose enough to fire on every
        // dot-aligned edge in the test corpus (rect bottom → straight
        // down has dot=1.0, diamond vertex → off-axis gets up to 0.85).
        let ux = dx / len;
        let uy = dy / len;
        let s_dot = start_tan.0 * ux + start_tan.1 * uy;
        // The straight-line shortcut must also respect the END tangent
        // (the side normal the arrowhead aims along): a corner arrival
        // like `M 469 L 594` hits the hub ~27° off the side normal while
        // the arrow points dead-perpendicular — the line's tail then
        // skews away from the arrow and forks at the junction.
        let e_dot = match end_tan {
            Some(t) => t.0 * ux + t.1 * uy,
            None => 1.0,
        };
        // force_curve is set on multi-segment obstacle paths: every
        // chain of vias must stay continuous (each segment shares its
        // end tangent with the next segment's start tangent), so we
        // never collapse a piece to a flat `L` — otherwise the corner
        // at the shared via reappears (two `L`s meeting at an angle).
        // Single-segment edges keep the shortcut for straight runs.
        if !force_curve && s_dot.abs() > 0.95 {
            // A line within a hair of the side normal is a genuinely
            // straight run straight into the side (e.g. a vertical
            // A→B). Everything else arrives off the arrow and is
            // re-routed by `edge_svg`'s `entry_run` splice (given our
            // tangents this branch is only reached for aligned legs).
            if e_dot > 0.995 {
                return format!("L {ex:.1} {ey:.1}");
            }
        }
        // Offset is 40% of the segment length so the curve has a
        // visible bow without overshooting into adjacent nodes.
        // Cap it by the along-flow distance between the endpoints:
        // a long fan edge (B→hub) can reach `off ≈ len*0.4 ≈ 170px`
        // while the two layers are only ~160px apart, which places
        // c2 *above* the source level and carves the curve into an
        // S-bend. All fan edges then collapse toward the same
        // mid-path region and touch. Keeping the handles inside the
        // layer band (`along*0.75`) yields a monotone bow instead.
        let along = match dir {
            Dir::Td => ey - sy,
            Dir::Bt => sy - ey,
            Dir::Lr => ex - sx,
            Dir::Rl => sx - ex,
        };
        let off = if along > 0.0 {
            (len * 0.4).max(20.0).min(along * 0.4)
        } else {
            (len * 0.4).max(20.0)
        };
        let c1x = sx + off * start_tan.0;
        let c1y = sy + off * start_tan.1;
        // End control point: when the caller specifies an end tangent
        // (e.g. the inward normal of a side anchor — for a right-side
        // anchor that's `(-1, 0)`), place c2 so the curve's end tangent
        // is end_tan. This produces a clean horizontal entry into the
        // side instead of the S-bow you'd get from placing c2 along
        // the line direction. For a near-vertical edge to a right-side
        // anchor, line-direction c2 sits above the target — the curve
        // sweeps up past F, then dives back down. End-tangent c2 sits
        // to the right of the target at the same y, so the curve
        // arrives going leftward into the right edge.
        let (c2x, c2y) = match end_tan {
            Some(t) => (ex - off * t.0, ey - off * t.1),
            None => (ex - off * ux, ey - off * uy),
        };
        format!("C {c1x:.1} {c1y:.1} {c2x:.1} {c2y:.1} {ex:.1} {ey:.1}")
    }

    /// Routing waypoints for an edge: `[start, …via…, end]`. A via
    /// point is inserted when the midpoint of the edge falls inside
    /// another node's bounding box (which would otherwise make the
    /// bezier curve sweep straight through that node). The via is
    /// placed perpendicular to the line by enough to clear the
    /// obstacle, picking the side that gives the most clearance.
    fn edge_waypoints(&self, e: &Edge, sx: f64, sy: f64, ex: f64, ey: f64) -> Vec<(f64, f64)> {
        let mut waypoints = vec![(sx, sy)];
        // Sample the bezier curve at multiple `t` values rather than
        // checking only the midpoint — an edge whose straight line
        // misses every node can still bow through one once the side
        // tangents at the anchors pull the curve sideways (E→G in
        // the 7-node graph passes its midpoint test but its start
        // tangent sends it well past F's right edge, then back).
        // We use 11 evenly spaced samples and report the first
        // obstacle encountered; this catches the common cases
        // without paying for a full polyline intersection test.
        const N_SAMPLES: usize = 11;
        let sample_t = |t: f64| -> (f64, f64) {
            let u = 1.0 - t;
            // Control points are needed to evaluate the actual curve,
            // not the straight segment. We reconstruct the same
            // tangents edge_svg uses so the sample follows the path
            // that will actually be drawn.
            let from = &self.nodes[e.from];
            let to = &self.nodes[e.to];
            let (x1, y1) = self.pos_of(from);
            let (_x2, _y2) = self.pos_of(to);
            let from_cx = x1 + from.w / 2.0;
            let from_cy = y1 + from.h / 2.0;
            let s_dx = sx - from_cx;
            let s_dy = sy - from_cy;
            let s_dist = (s_dx * s_dx + s_dy * s_dy).sqrt().max(1e-9);
            let out_tan = (s_dx / s_dist, s_dy / s_dist);
            let ldx = ex - sx;
            let ldy = ey - sy;
            let llen = (ldx * ldx + ldy * ldy).sqrt().max(1e-9);
            let line_tan = (ldx / llen, ldy / llen);
            let line_weight = if from.shape == 3 { 0.65 } else { 0.0 };
            let mx = out_tan.0 * (1.0 - line_weight) + line_tan.0 * line_weight;
            let my = out_tan.1 * (1.0 - line_weight) + line_tan.1 * line_weight;
            let mm = (mx * mx + my * my).sqrt().max(1e-9);
            let s_tan = (mx / mm, my / mm);
            let len = ((ex - sx) * (ex - sx) + (ey - sy) * (ey - sy)).sqrt();
            // Same control-offset cap as `bezier_segment` so the sample
            // follows the curve that is actually drawn — otherwise a
            // long diagonal fan edge would be tested for bowing with an
            // uncapped `off` while the renderer's capped handles keep
            // the curve tight, over-reporting obstacles.
            let along = match self.dir {
                Dir::Td => ey - sy,
                Dir::Bt => sy - ey,
                Dir::Lr => ex - sx,
                Dir::Rl => sx - ex,
            };
            let off = if along > 0.0 {
                (len * 0.4).max(20.0).min(along * 0.4)
            } else {
                (len * 0.4).max(20.0)
            };
            let c1x = sx + off * s_tan.0;
            let c1y = sy + off * s_tan.1;
            let c2x = ex - off * (ldx / llen);
            let c2y = ey - off * (ldy / llen);
            let bx =
                u * u * u * sx + 3.0 * u * u * t * c1x + 3.0 * u * t * t * c2x + t * t * t * ex;
            let by =
                u * u * u * sy + 3.0 * u * u * t * c1y + 3.0 * u * t * t * c2y + t * t * t * ey;
            (bx, by)
        };
        let mut obstacle: Option<(f64, f64, f64, f64)> = None;
        for (i, n) in self.nodes.iter().enumerate() {
            if i == e.from || i == e.to {
                continue;
            }
            let (nx, ny) = self.pos_of(n);
            let n_right = nx + n.w;
            let n_bottom = ny + n.h;
            for k in 1..N_SAMPLES {
                let t = k as f64 / N_SAMPLES as f64;
                let (px, py) = sample_t(t);
                if px >= nx && px <= n_right && py >= ny && py <= n_bottom {
                    obstacle = Some((nx, ny, n.w, n.h));
                    break;
                }
            }
            if obstacle.is_some() {
                break;
            }
        }
        if let Some((ox, oy, ow, oh)) = obstacle {
            let ocx = ox + ow / 2.0;
            let ocy = oy + oh / 2.0;
            let dx = ex - sx;
            let dy = ey - sy;
            let len = (dx * dx + dy * dy).sqrt();
            if len > 0.0 {
                // Two perpendicular directions. Offset by a moderate
                // amount (half the longer side) and prefer the side
                // that puts the via *outside* the obstacle's bbox;
                // break the distance tie in favor of the farther side
                // so the curve bends away from the node rather than
                // grazing it.
                let perp_a = (-dy / len, dx / len);
                let perp_b = (dy / len, -dx / len);
                let offset = (ow.max(oh)) * 0.5;
                let via_a = (
                    (sx + ex) / 2.0 + perp_a.0 * offset,
                    (sy + ey) / 2.0 + perp_a.1 * offset,
                );
                let via_b = (
                    (sx + ex) / 2.0 + perp_b.0 * offset,
                    (sy + ey) / 2.0 + perp_b.1 * offset,
                );
                let clears = |v: (f64, f64)| -> bool {
                    (v.0 - ocx).abs() > ow / 2.0 || (v.1 - ocy).abs() > oh / 2.0
                };
                let dist_a = (via_a.0 - ocx).powi(2) + (via_a.1 - ocy).powi(2);
                let dist_b = (via_b.0 - ocx).powi(2) + (via_b.1 - ocy).powi(2);
                let via = match (clears(via_a), clears(via_b)) {
                    (true, false) => via_a,
                    (false, true) => via_b,
                    (true, true) => {
                        if dist_a >= dist_b {
                            via_a
                        } else {
                            via_b
                        }
                    }
                    // neither clears (rare — only when the edge is
                    // exactly colinear with the obstacle); fall back
                    // to the farther side so we at least bend away.
                    (false, false) => {
                        if dist_a >= dist_b {
                            via_a
                        } else {
                            via_b
                        }
                    }
                };
                waypoints.push(via);
            }
        }
        waypoints.push((ex, ey));
        waypoints
    }

    fn to_svg(&self) -> String {
        let (mw, mh) = self.dimensions();
        let mut out = String::new();
        // Subgraph labels are rendered INSIDE the box (baseline at
        // y0 + 14) — they don't extend above y=0 any more, so no
        // viewBox upward expansion is needed. We still widen the
        // right edge if the label is wider than the box would allow,
        // since the text starts at `x0` with no anchor centering.
        let vb_x = 0.0_f64;
        let vb_y = 0.0_f64;
        let mut vb_w = mw;
        let vb_h = mh;
        for sub in &self.subs {
            if sub.nodes.is_empty() {
                continue;
            }
            let (x0, _y0, _x1, _y1) = self.sub_bbox(sub);
            let label_w = approx_text_width(&sub.name);
            let right = x0 + label_w + 4.0;
            if right > vb_w {
                vb_w = right;
            }
        }
        // Set explicit width/height equal to viewBox so the SVG renders
        // at its natural CSS-pixel size instead of being stretched to
        // fill the parent container (which made the diagram look huge
        // in many Markdown viewers — the 12px font in viewBox units
        // would become ~33 CSS px when the viewBox was scaled 2–3× up
        // to match the article width).
        out.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" \
             width=\"{vb_w:.0}\" height=\"{vb_h:.0}\" \
             viewBox=\"{vb_x:.0} {vb_y:.0} {vb_w:.0} {vb_h:.0}\" \
             style=\"max-width:100%;height:auto;\" \
             role=\"img\" aria-label=\"flowchart diagram\">"
        ));
        for sub in &self.subs {
            let (x0, y0, x1, y1) = self.sub_bbox(sub);
            out.push_str(&format!(
                "<rect x=\"{x0:.0}\" y=\"{y0:.0}\" width=\"{:.0}\" height=\"{:.0}\" rx=\"8\" \
                 fill=\"#f6f8fa\" stroke=\"#d0d7de\" stroke-dasharray=\"5 3\"/>",
                x1 - x0,
                y1 - y0
            ));
            // Label is horizontally centred but anchored near the top of the
            // box (baseline at y0 + 16 leaves ~6 px above the cap line
            // for the 13 px font). Horizontal centring uses
            // `text-anchor="middle"` so the text grows symmetrically
            // around the box midpoint regardless of label length.
            let cx = x0 + (x1 - x0) / 2.0;
            let cy = y0 + 16.0;
            out.push_str(&format!(
                "<text x=\"{cx:.0}\" y=\"{cy:.0}\" font-size=\"13\" \
                 text-anchor=\"middle\" \
                 font-family=\"sans-serif, Noto Sans CJK SC, Microsoft YaHei, PingFang SC, Hiragino Sans GB, Source Han Sans SC, WenQuanYi Micro Hei\" fill=\"#57606a\">{}</text>",
                escape_text(&sub.name)
            ));
        }
        // Pre-compute edge geometry once: lines are emitted first
        // (under the nodes), then nodes, then arrowheads on top of
        // the nodes — so the arrowhead's tip isn't covered by the
        // target rect's 1.5 px stroke band. Labels stay grouped with
        // their edge so they render in the same z-order as before.
        let edge_parts: Vec<EdgeParts> = self
            .edges
            .iter()
            .enumerate()
            .map(|(ei, e)| self.edge_svg(e, ei))
            .collect();
        for p in &edge_parts {
            out.push_str(&p.line);
            if let Some(l) = &p.label {
                out.push_str(l);
            }
        }
        for n in &self.nodes {
            out.push_str(&self.node_svg(n));
        }
        for p in &edge_parts {
            if let Some(h) = &p.arrowhead {
                out.push_str(h);
            }
        }
        out.push_str("</svg>");
        out
    }

    fn dimensions(&self) -> (f64, f64) {
        if self.nodes.is_empty() {
            return (120.0, 120.0);
        }
        let gx = 28.0_f64;
        let gy = match self.dir {
            Dir::Td | Dir::Bt => 28.0_f64,
            Dir::Lr | Dir::Rl => 80.0_f64,
        };
        match self.dir {
            Dir::Td | Dir::Bt => {
                // `n.x_pos + n.w / 2.0` is already the right edge of
                // each node (post-normalization the leftmost node's
                // left edge sits at `gx`). Adding `gx` gives the right
                // padding; previously the formula also added `max_hw`,
                // which double-counted the widest node's half-width
                // and inflated the viewBox by ~60 px for no reason.
                let max_x = self
                    .nodes
                    .iter()
                    .map(|n| n.x_pos + n.w / 2.0)
                    .fold(f64::NEG_INFINITY, f64::max);
                let max_layer = self.nodes.iter().map(|n| n.layer).max().unwrap_or(0);
                // Bottom of the deepest layer = layer_y + its max extent + bottom padding.
                let last_y = self.layer_y[max_layer] + self.layer_max_extent[max_layer] + gy;
                (max_x + gx, last_y)
            }
            Dir::Lr | Dir::Rl => {
                // Same correction on the vertical axis: `n.x_pos + n.h / 2.0`
                // is already the bottom edge of each node.
                let max_y = self
                    .nodes
                    .iter()
                    .map(|n| n.x_pos + n.h / 2.0)
                    .fold(f64::NEG_INFINITY, f64::max);
                let max_layer = self.nodes.iter().map(|n| n.layer).max().unwrap_or(0);
                let last_x = self.layer_y[max_layer] + self.layer_max_extent[max_layer] + gy;
                (last_x + gx, max_y + gy)
            }
        }
    }

    fn node_svg(&self, n: &Node) -> String {
        let (x, y) = self.pos_of(n);
        let cx = x + n.w / 2.0;
        let cy = y + n.h / 2.0;
        // Multi-line labels need <tspan> rows — SVG <text> ignores \n.
        let max_chars = match n.shape {
            // diamond/circle have less horizontal room; cut sooner.
            2 | 3 => 9,
            _ => 14,
        };
        let lines = super::common::wrap_lines(&n.label, max_chars);
        let line_h = 14.0_f64;
        // Centre the wrapped block on the box: baseline of the first
        // line sits above `cy`, each subsequent line is one line_h
        // lower. The parent <text> gets `y={first_baseline}` so SVG
        // doesn't default to y=0 (which would put every label at the
        // top of the canvas, invisible behind later shapes).
        let total = lines.len() as f64 * line_h;
        let first_baseline = cy - total / 2.0 + line_h * 0.75;
        let mut tspans = String::new();
        for (i, line) in lines.iter().enumerate() {
            let dy = if i == 0 { 0.0 } else { line_h };
            // Escape first (so the inner <tspan> tags we add below
            // stay intact), then split into ASCII/CJK runs so
            // renderers that don't do automatic font-family fallback
            // (e.g. ImageMagick's librsvg) still pick a CJK font for
            // CJK glyphs.
            let rendered = super::common::render_text_spans(&escape_text(line));
            tspans.push_str(&format!(
                "<tspan x=\"{cx:.1}\" dy=\"{dy:.2}\">{}</tspan>",
                rendered
            ));
        }
        let path = shape_path(n.shape, x, y, n.w, n.h);
        let mut s = String::new();
        if n.shape == 2 {
            // Circle (`A((text))`): `shape_path` returns the full
            // `<circle ... />` tag, so we cannot append `fill`/`stroke`
            // after it the way we do for `<path d="..."/>` — that
            // would emit `<circle ... /> fill="..."/>` which the SVG
            // parser reads as a closed circle followed by orphan
            // attribute text (rendered as literal characters and
            // giving the circle the default black fill instead of
            // `n.fill`). Emit the circle inline instead.
            let cx = x + n.w / 2.0;
            let cy = y + n.h / 2.0;
            let r = (n.h / 2.0).min(n.w / 2.0) + 6.0;
            s.push_str(&format!(
                "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{r:.1}\" fill=\"{}\" \
                 stroke=\"#24292f\" stroke-width=\"1.5\"/>",
                n.fill
            ));
        } else {
            s.push_str(&format!(
                "<path d=\"{path}\" fill=\"{}\" stroke=\"#24292f\" stroke-width=\"1.5\"/>",
                n.fill
            ));
        }
        s.push_str(&format!(
            "<text x=\"{cx:.1}\" y=\"{first_baseline:.2}\" font-size=\"12\" text-anchor=\"middle\" \
             font-family=\"sans-serif, Noto Sans CJK SC, Microsoft YaHei, PingFang SC, Hiragino Sans GB, Source Han Sans SC, WenQuanYi Micro Hei\" fill=\"#24292f\">{}</text>",
            tspans
        ));
        s
    }

    fn edge_svg(&self, e: &Edge, edge_idx: usize) -> EdgeParts {
        let from = &self.nodes[e.from];
        let to = &self.nodes[e.to];
        let (x1, y1) = self.pos_of(from);
        let (_x2, _y2) = self.pos_of(to);
        // Anchor to the boundary side facing the other node so edges
        // never dive through a node's interior (rect uses side midpoint;
        // diamond uses the matching vertex). When several edges share
        // the same side of the same node (e.g. E→F and E→G both leave
        // E through its bottom vertex, or D→G, E→G, F→G all arrive at
        // G through its top) the rank/count from `side_edges` fans them
        // out along that side so the line segments don't overlap.
        //
        // The sides themselves come from the per-edge fields populated
        // by `layout` — re-deriving them here via `anchor_side` would
        // undo the redistribution pass that spreads clustered incoming
        // edges across multiple sides.
        let s_side = e.s_side as usize;
        let e_side = e.e_side as usize;
        let s_edges = &self.side_edges[e.from][s_side];
        let e_edges = &self.side_edges[e.to][e_side];
        let s_rank = s_edges.iter().position(|&i| i == edge_idx).unwrap_or(0);
        let e_rank = e_edges.iter().position(|&i| i == edge_idx).unwrap_or(0);
        let (sx, sy) = self.anchor_with_offset(from, s_side, s_rank, s_edges.len());
        let (ex, ey) = self.anchor_with_offset(to, e_side, e_rank, e_edges.len());
        let (stroke, dash, arrow) = match e.style {
            0 => ("#24292f", "none", true),
            1 => ("#24292f", "none", false),
            2 => ("#24292f", "6 4", true),
            _ => ("#8250df", "none", true),
        };
        // Compute routing waypoints: when an obstacle (a non-endpoint
        // node whose bbox straddles the edge midpoint) would otherwise
        // sit under the curve, `edge_waypoints` inserts a perpendicular
        // via-point so the bezier routes around it instead of slicing
        // straight through. Multi-segment paths are stitched with `C`
        // commands after the initial `M` so the curve stays smooth.
        let mut waypoints = self.edge_waypoints(e, sx, sy, ex, ey);
        // The first segment's start tangent is a blend of the
        // outward direction from the source (centre → anchor) and
        // the line direction (anchor → target). For rect anchors
        // the outward direction already points along the side
        // normal, so the full weight works. For diamond anchors
        // the outward direction can be almost pure horizontal or
        // vertical (e.g. E→G leaves E's right vertex going RIGHT,
        // but the target sits down-right), which used to push the
        // first control point out to x=321 — the curve would sweep
        // far sideways before bending back toward G, crossing
        // through F. Blending 35 % toward the line direction keeps
        // a clean exit out of the vertex while still bending the
        // curve toward the target instead of perpendicular to it.
        let from_cx = x1 + from.w / 2.0;
        let from_cy = y1 + from.h / 2.0;
        let s_dx = sx - from_cx;
        let s_dy = sy - from_cy;
        let s_dist = (s_dx * s_dx + s_dy * s_dy).sqrt().max(1e-9);
        let out_tan = (s_dx / s_dist, s_dy / s_dist);
        let ldx = ex - sx;
        let ldy = ey - sy;
        let llen = (ldx * ldx + ldy * ldy).sqrt().max(1e-9);
        let line_tan = (ldx / llen, ldy / llen);
        // Weight: how strongly the start tangent follows the line
        // direction versus the outward (side-normal / vertex) direction.
        // Higher weight yields a smoother diagonal curve with less
        // overshoot past the source, at the cost of a less pronounced
        // "exit-from-side" feel. Diamond weight 0.65 keeps the curve
        // visibly anchored to the vertex (no straight degenerate
        // bezier); rect weight 0.5 turns D→G from a visible L-bend
        // (pure-down then sharp right) into a clean diagonal without
        // going so far that the collinear shortcut collapses it to `L`.
        // The collinear threshold at 0.95 in bezier_segment would
        // fire for line_weight ≥ ~0.8 on diamonds (their out_tan is
        // nearly perpendicular to line_tan, so a 0.85 blend lands
        // within 1° of the line direction and the dot product
        // exceeds the threshold).
        let line_weight = if from.shape == 3 { 0.65 } else { 0.5 };
        let s_tan = {
            let mx = out_tan.0 * (1.0 - line_weight) + line_tan.0 * line_weight;
            let my = out_tan.1 * (1.0 - line_weight) + line_tan.1 * line_weight;
            let m = (mx * mx + my * my).sqrt().max(1e-9);
            (mx / m, my / m)
        };
        // End-tangent of the curve: the inward normal of the SIDE
        // the edge enters — not the offset anchor. Parallel edges
        // fan out along a side (e.g. 纯静态页面 has three incoming
        // edges along its top), so the anchor point no longer sits
        // at the side midpoint; computing the normal from the
        // offset point gives a diagonal direction that swings the
        // curve to the side. The side normal is a fixed axis-aligned
        // vector (top=down, right=left, bottom=up, left=right), and
        // that's what we want the curve's final heading to be: a
        // clean horizontal or vertical arrival instead of a skew.
        let end_tan = match e_side {
            0 => (0.0, 1.0),  // top:    going down
            1 => (-1.0, 0.0), // right:  going left
            2 => (0.0, -1.0), // bottom: going up
            3 => (1.0, 0.0),  // left:   going right
            _ => (0.0, 1.0),
        };
        // Every arrow gets a straight aligned entry: the final stroke into a
        // node must run along the side normal for at least the arrowhead
        // plus margin, or the line's tail is still sweeping diagonally
        // as it slides under the arrowhead and visibly forks off it. A
        // plain single bezier cannot guarantee this — its far start
        // control keeps pulling the curve sideways within arrowhead
        // range even when `off` is large (the fan's children still
        // forked up to 12.6°). So, for EVERY edge whose final leg is not
        // already a straight perpendicular run (`aligned`), splice a
        // waypoint on the side normal: the last leg becomes a dedicated
        // straight run of ENTRY_RUN px into the node, and the arrow
        // sits at the end of a flat vertical/horizontal stroke.
        // Already-aligned legs (a vertical `L` straight down to the
        // arrow) pass through untouched. `entry_run` is used in the
        // multi-segment builder below to keep the spliced run perfectly
        // straight and hand it off with C¹ continuity.
        const ENTRY_RUN: f64 = 14.0;
        let (tnx, tny) = end_tan;
        let wp_len = waypoints.len();
        let (plx, ply) = waypoints[wp_len - 2];
        let (lex, ley) = (ex - plx, ey - ply);
        let leg_len = (lex * lex + ley * ley).sqrt().max(1e-9);
        let aligned = (lex * tnx + ley * tny) / leg_len;
        let entry_run = if aligned < 0.995 {
            waypoints.insert(wp_len - 1, (ex - ENTRY_RUN * tnx, ey - ENTRY_RUN * tny));
            true
        } else {
            false
        };
        let d = if waypoints.len() == 2 {
            // Single-segment path: keep the natural exit direction
            // (anchor → outward) so the curve leaves the node cleanly,
            // and use the side-normal end tangent so the curve arrives
            // along the inbound normal of the target anchor.
            format!(
                "M {sx:.1} {sy:.1} {}",
                self.bezier_segment(self.dir, sx, sy, ex, ey, s_tan, Some(end_tan), false)
            )
        } else {
            // Obstacle-routed path: the first segment runs from the
            // anchor to a via point that sits perpendicular to the
            // obstacle. If we kept the natural anchor-based tangent
            // here it can point AWAY from the via (e.g. E→G exits
            // E going RIGHT but the via is down-RIGHT), forcing the
            // curve to loop back on itself before reaching the via.
            // Override the first segment's tangent with the direction
            // to the via so the curve sweeps toward the obstacle
            // directly instead of bulging out and returning.
            // The LAST segment uses the side-normal end tangent so the
            // final approach into the target anchor is smooth.
            let mut d = format!("M {sx:.1} {sy:.1}");
            let segs: Vec<_> = waypoints.windows(2).collect();
            let last_i = segs.len() - 1;
            // Through-tangent at an interior via point: the direction
            // from the previous waypoint to the next. Each interior
            // via is shared by two segments — the previous segment
            // ends along this tangent and the next segment starts
            // along the SAME tangent, so the curve passes through the
            // via with C¹ continuity instead of a sharp corner (two
            // straight `L` pieces meeting at an angle).
            let through = |k: usize| -> (f64, f64) {
                let (ax, ay) = waypoints[k - 1];
                let (bx, by) = waypoints[k + 1];
                let tdx = bx - ax;
                let tdy = by - ay;
                let tl = (tdx * tdx + tdy * tdy).sqrt().max(1e-9);
                (tdx / tl, tdy / tl)
            };
            for (i, w) in segs.iter().enumerate() {
                d.push(' ');
                let sdx = w[1].0 - w[0].0;
                let sdy = w[1].1 - w[0].1;
                let slen = (sdx * sdx + sdy * sdy).sqrt().max(1e-9);
                let seg_tan = (sdx / slen, sdy / slen);
                // First piece leaves the anchor heading toward the
                // first via (seg_tan equals the via direction here,
                // so no loop-back). Later pieces inherit the shared
                // through-tangent from `through(i)`.
                let seg_start = if i == 0 || (entry_run && i == last_i) {
                    // First piece leaves the anchor heading toward the
                    // first via (seg_tan equals the via direction here,
                    // so no loop-back). The spliced entry run starts
                    // along its OWN direction so the final leg stays a
                    // perfectly straight perpendicular shot at the arrow.
                    seg_tan
                } else {
                    through(i)
                };
                let seg_end = if i == last_i {
                    Some(end_tan)
                } else if entry_run && i + 1 == last_i {
                    // End the piece just before the spliced run along the
                    // side normal too, so it hands off to the straight
                    // entry run with C¹ continuity (no corner at the
                    // splice point).
                    Some(end_tan)
                } else {
                    Some(through(i + 1))
                };
                d.push_str(&self.bezier_segment(
                    self.dir, w[0].0, w[0].1, w[1].0, w[1].1, seg_start, seg_end, true,
                ));
            }
            d
        };
        let line = format!(
            "<path d=\"{d}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"1.6\" \
             stroke-dasharray=\"{dash}\"/>"
        );
        // Arrowhead is emitted separately (see `to_svg`) so it sits
        // on top of the target node's stroke band. With the node drawn
        // AFTER the edge, the 1.5 px stroke covers the bottom 0.75 px
        // of the arrowhead tip — making the connection look like it's
        // "blocked" by the box edge. Pulling the arrowhead out of the
        // edge group restores the visible tip.
        let arrowhead = if arrow {
            // Aim the arrow along the curve's actual end tangent. Both
            // single-segment paths and obstacle-routed paths end their
            // final piece with the side-normal `end_tan`, so the arrow
            // always points INTO the side the edge enters — never the
            // overall start→end heading (e.g. E→G ends on a heading of
            // ~113° but its overall direction is ~143°, so the old code
            // rendered an arrowhead pointing the wrong way at G).
            let (hx, hy) = (ex - end_tan.0, ey - end_tan.1);
            let ang = (ey - hy).atan2(ex - hx);
            let head = self.arrowhead_path(ex, ey, ang);
            Some(format!("<path d=\"{head}\" fill=\"{stroke}\"/>"))
        } else {
            None
        };
        if !e.label.is_empty() {
            // Padding kept tight (2 px each side) so the label
            // rect doesn't bury the line/arrowhead on short edges.
            // Earlier 12 px of padding made the rect wider than the
            // gap between source and target, hiding both the line
            // ends and the arrow tip under the white label fill.
            let bw = approx_text_width(&e.label) + 4.0;
            let bh = 16.0_f64;
            // Default label center = midpoint of the edge. For
            // horizontal LR/RL edges we lift the label ABOVE the
            // line (by 18 px) so the source→target span stays clear
            // — the user can see both the line coming out of the
            // source and the arrowhead going into the target.
            let mut tx = (sx + ex) / 2.0;
            let mut ty = (sy + ey) / 2.0 - bh / 2.0;
            let dx = ex - sx;
            let dy = ey - sy;
            let len = (dx * dx + dy * dy).sqrt().max(1e-9);
            let ux = dx / len;
            let uy = dy / len;
            if ux.abs() > uy.abs() && matches!(self.dir, Dir::Lr | Dir::Rl) {
                // Horizontal LR edge: sit above the midpoint.
                ty = sy.min(ey) - bh - 2.0;
            }
            // If the label would land inside a containing subgraph
            // box, slide it along the edge so it sits in the OUTSIDE
            // half — between the box boundary and the target node.
            // Then clamp the center to stay inside the source→target
            // span so the line and arrowhead stay visible on both
            // sides of the label.
            for sub in &self.subs {
                let (bx0, by0, bx1, by1) = self.sub_bbox(sub);
                let lx = tx - bw / 2.0;
                let rx = tx + bw / 2.0;
                let ly = ty;
                let lry = ty + bh;
                let overlaps = lx < bx1 && rx > bx0 && ly < by1 && lry > by0;
                if !overlaps {
                    continue;
                }
                if ux.abs() > uy.abs() {
                    // Horizontal edge — translate `tx` past the
                    // closer vertical side of the box in the
                    // direction of travel, then clamp so the rect
                    // stays within the source→target span.
                    let bound = if ux > 0.0 { bx1 } else { bx0 };
                    let sign = ux.signum();
                    tx = bound + sign * (bw / 2.0);
                    let lo = sx.min(ex) + bw / 2.0;
                    let hi = sx.max(ex) - bw / 2.0;
                    if hi > lo {
                        tx = tx.clamp(lo, hi);
                    }
                } else if uy.abs() > 0.0 {
                    // Vertical edge — translate `ty` past the closer
                    // horizontal side of the box in the direction of
                    // travel, then clamp.
                    let bound = if uy > 0.0 { by1 } else { by0 };
                    let sign = uy.signum();
                    ty = bound + sign * (bh / 2.0);
                    let lo = sy.min(ey) + bh / 2.0;
                    let hi = sy.max(ey) - bh / 2.0;
                    if hi > lo {
                        ty = ty.clamp(lo, hi);
                    }
                }
            }
            let mut label = String::new();
            label.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{bw:.1}\" height=\"{bh:.1}\" rx=\"3\" \
                 fill=\"#fff\" fill-opacity=\"0.85\" stroke=\"#d0d7de\" stroke-width=\"0.5\"/>",
                tx - bw / 2.0,
                ty
            ));
            label.push_str(&format!(
                "<text x=\"{tx:.1}\" y=\"{:.1}\" font-size=\"10.5\" text-anchor=\"middle\" \
                 font-family=\"sans-serif, Noto Sans CJK SC, Microsoft YaHei, PingFang SC, Hiragino Sans GB, Source Han Sans SC, WenQuanYi Micro Hei\" fill=\"#24292f\">{}</text>",
                ty + bh - 4.0,
                escape_text(&e.label)
            ));
            return EdgeParts {
                line,
                arrowhead,
                label: Some(label),
            };
        }
        EdgeParts {
            line,
            arrowhead,
            label: None,
        }
    }

    fn sub_bbox(&self, sub: &Sub) -> (f64, f64, f64, f64) {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for &ni in &sub.nodes {
            let (x, y) = self.pos_of(&self.nodes[ni]);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x + self.nodes[ni].w);
            max_y = max_y.max(y + self.nodes[ni].h);
        }
        if min_x.is_infinite() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        // For the vertical extent, scan the WHOLE diagram (not just
        // the contained nodes) so the box stays vertically centred
        // when the subgraph wraps only a subset of columns — e.g.
        // an LR layout where C and D sit outside the subgraph. The
        // contained nodes' own band centres around y=90, so using
        // their extent alone happens to centre too, but a band that
        // shifts (e.g. with a third contained node) would float
        // off-axis. Anchoring to the diagram centre makes this
        // robust.
        let mut dmin_y = f64::INFINITY;
        let mut dmax_y = f64::NEG_INFINITY;
        for n in &self.nodes {
            let (_, y) = self.pos_of(n);
            dmin_y = dmin_y.min(y);
            dmax_y = dmax_y.max(y + n.h);
        }
        let pad_x = 20.0;
        let pad_y = 4.0;
        (min_x - pad_x, dmin_y - pad_y, max_x + pad_x, dmax_y + pad_y)
    }
}

/// Unit-vector tangent for an edge leaving a node on `side`:

/// Strip the outermost `["…"]` / `['…']` pair from a node label so the
fn strip_outer_quotes(s: &str) -> String {
    let bytes = s.as_bytes();
    let first = bytes.first().copied();
    let last = bytes.last().copied();
    match (first, last) {
        (Some(b'"'), Some(b'"')) => s[1..s.len() - 1].to_string(),
        (Some(b'\''), Some(b'\'')) => s[1..s.len() - 1].to_string(),
        _ => s.to_string(),
    }
}

fn contains_edge_op(line: &str) -> bool {
    line.contains("-->") || line.contains("---") || line.contains("==>") || line.contains("-.->")
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
            // `rfind("))")` returns the byte index of the FIRST `)` of
            // the closing `))`; the label spans the gap between the
            // opening `((` and that index. `+1` would shift into the
            // closing paren and produce a trailing `)` in the label
            // — e.g. `A((渲染))` previously rendered as `渲染)`.
            let end = rest.rfind("))").unwrap_or(rest.len() - 1);
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
    let label = strip_outer_quotes(label.trim());
    Some((id, label, shape))
}

/// Try to consume a Mermaid edge token starting at byte offset `i`.
/// Returns `(bytes_consumed, style, inline_label)`. Handles the four
/// standard operators (`-->`, `---`, `==>`, `-.->`) plus the inline-label
/// form `-- label -->`, `-- label ---`, `-- label ==>`, `-- label -.->`.
fn detect_edge_token(line: &str, i: usize) -> Option<(usize, u8, String)> {
    let len = line.len();
    if i >= len {
        return None;
    }
    // Standard operators take priority over the `--` inline label form so
    // `-->`, `---`, `==>` and `-.->` are still recognised as before.
    if line[i..].starts_with("==>") {
        return Some((3, 3, String::new()));
    }
    // Mermaid's dotted arrow is "-.->" (4 bytes: dash, dot, dash,
    // greater-than). Consuming only "-.-" leaves the trailing ">"
    // for the next token — which the shape splitter then treats as
    // an asymmetric-shape opener, eating the real target node.
    if line[i..].starts_with("-.->") {
        return Some((4, 2, String::new()));
    }
    if line[i..].starts_with("---") {
        return Some((3, 1, String::new()));
    }
    if line[i..].starts_with("-->") {
        return Some((3, 0, String::new()));
    }
    // Inline label form: `--` then optional whitespace then label text
    // then a trailing edge operator. The label may contain dashes (e.g.
    // `-- foo-bar -->`), only the closing operator terminates it.
    if line[i..].starts_with("--") {
        let after = i + 2;
        if after >= len {
            return None;
        }
        let bytes = line.as_bytes();
        let mut j = after;
        // Skip leading whitespace after `--` (typical shape is `-- `).
        while j < len && bytes[j] == b' ' {
            j += 1;
        }
        if j >= len {
            return None;
        }
        let mut k = j;
        while k < len {
            // Detect a closing operator that starts right after a label run.
            let close_run = k > j && close_edge_at(line, k).is_some();
            if close_run {
                let (op_len, style) = close_edge_at(line, k).unwrap();
                let label = line[j..k].trim().to_string();
                return Some((k - i + op_len, style, label));
            }
            // Spaces before a closing operator still belong to the label
            // region; only the operator itself terminates it.
            if bytes[k] == b' ' {
                let mut m = k;
                while m < len && bytes[m] == b' ' {
                    m += 1;
                }
                if let Some((op_len, style)) = close_edge_at(line, m) {
                    let label = line[j..m].trim().to_string();
                    return Some((m - i + op_len, style, label));
                }
            }
            k = next_boundary(line, k);
        }
    }
    None
}

/// Length + style of whichever standard edge operator starts the given
/// substring, or `None` when none matches.
fn close_edge_at(line: &str, k: usize) -> Option<(usize, u8)> {
    if line[k..].starts_with("==>") {
        Some((3, 3))
    } else if line[k..].starts_with("-.->") {
        Some((4, 2))
    } else if line[k..].starts_with("---") {
        Some((3, 1))
    } else if line[k..].starts_with("-->") {
        Some((3, 0))
    } else {
        None
    }
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
        // edge operator (with optional `-- label -->` inline form)
        let (op_end, style, inline_label) = match detect_edge_token(line, i) {
            Some((n, st, lbl)) => (i + n, st, lbl),
            None => (i, 0, String::new()),
        };
        if op_end > i {
            i = op_end;
            // inline label captured from the `-- label -->` form already
            // wins; otherwise fall through to the `|label|` / `-- label --`
            // forms so the existing syntax keeps working.
            let mut label = inline_label;
            while i < len && line.as_bytes()[i] == b' ' {
                i += 1;
            }
            if label.is_empty() && i < len && line.as_bytes()[i] == b'|' {
                if let Some(rel) = line[i + 1..].find('|') {
                    label = line[i + 1..i + 1 + rel].to_string();
                    i += rel + 2;
                }
            } else if label.is_empty() {
                // possible -- label -- form: consume a word that is followed
                // by another edge operator (not a node id)
                let j = i;
                while i < len
                    && !line[i..].starts_with("-->")
                    && !line[i..].starts_with("---")
                    && !line[i..].starts_with("==>")
                    && !line[i..].starts_with("-.->")
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
                        || line[i..].starts_with("-.->")
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
                        s_side: 0,
                        e_side: 0,
                    });
                }
                from = Some(ni);
            }
            continue;
        }
        // node segment: consume until next edge op (including `--` which
        // opens an inline-label edge).
        let j = i;
        let mut k = i;
        while k < len
            && !line[k..].starts_with("-->")
            && !line[k..].starts_with("---")
            && !line[k..].starts_with("==>")
            && !line[k..].starts_with("-.->")
            && !(k > j
                && line.as_bytes().get(k) == Some(&b'-')
                && line.as_bytes().get(k + 1) == Some(&b'-'))
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
        && !line[k..].starts_with("-.->")
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

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn renders_flowchart_td() {
        let out = render("flowchart TD\n  A[Start] --> B{Decision}\n  B --> C[End]")
            .expect("should render");
        assert!(out.starts_with("<svg"));
        assert!(out.contains("Start"));
        assert!(out.contains("Decision"));
        assert!(out.contains("<path "));
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
        // Glyphs are split into ASCII/CJK tspans so they render even
        // when the surrounding renderer doesn't do font-family
        // fallback — assert both pieces are present.
        assert!(out.contains("编写"));
        assert!(out.contains("Markdown"));
    }

    #[test]
    fn long_label_uses_tspans() {
        // Long label that exceeds 14 chars must be split into <tspan>
        // rows — SVG <text> doesn't honour \n, so the old wrap_label
        // implementation silently dropped the rest of the label.
        let out = render("flowchart TD\n  A[编写很长的多字符中文标签用于测试换行] --> B")
            .expect("should render");
        assert!(out.contains("<tspan"), "long label must use tspans: {out}");
    }

    #[test]
    fn unknown_syntax_degrades_empty() {
        assert!(render("sequenceDiagram\n A->>B: hi").is_none());
    }

    #[test]
    fn inline_label_arrow_dashed() {
        // The user-reported case: a 7-node flowchart whose four edges carry
        // 中文 labels via the `-- label -->` Mermaid syntax.
        let src = "graph TD\n\
                   \x20 A[写出新文章] --> B[撰写 Markdown]\n\
                   \x20 B --> C{包含公式?}\n\
                   \x20 C -- 是 --> D[加入行内公式]\n\
                   \x20 C -- 否 --> E{包含图表?}\n\
                   \x20 E -- 是 --> F[加入流程图]\n\
                   \x20 E -- 否 --> G[纯静态页面]\n\
                   \x20 D --> G\n\
                   \x20 F --> G\n";
        let svg = render(src).expect("should render");
        let has = |needle: &str| svg.contains(needle);
        // Labels are now split into ASCII/CJK runs across separate
        // `<tspan>` elements, so check each piece individually.
        for label in &[
            "写出新文章",
            "撰写",
            "Markdown",
            "包含公式",
            "加入行内公式",
            "包含图表",
            "加入流程图",
            "纯静态页面",
            "是",
            "否",
        ] {
            assert!(has(label), "missing {label} in SVG:\n{svg}");
        }
        // 8 edges in the diagram — each renders as a `<path>` with
        // `stroke-width="1.6"` and `fill="none"`. (Node shapes use
        // `1.5`; arrowheads have no stroke.) Obstacle-routed edges use
        // two `C` segments stitched with via-points, so we can't count
        // by ` C ` markers anymore — count by the edge's own
        // stroke-width marker instead.
        let edge_count = svg.matches("stroke-width=\"1.6\"").count();
        assert_eq!(
            edge_count, 8,
            "edge count wrong ({} edges):\n{svg}",
            edge_count
        );
        // And the labels should have lost the surrounding quotes the user
        // typed — `["加入行内公式"]` should render as a bare string.
        assert!(!svg.contains("\"加入行内公式\""));
        assert!(!svg.contains("\"加入流程图\""));
        assert!(!svg.contains("\"纯静态页面\""));
    }

    #[test]
    fn arrowheads_painted_after_nodes() {
        // The arrowhead's tip lands exactly on the target rect's edge,
        // where the 1.5 px stroke band covers ~0.75 px of the tip.
        // Emitting arrowheads AFTER the rects keeps the tip visible.
        // Regression test for: "纯静态页面" right-side arrow
        // connection almost blocked by the rect's stroke.
        let src = "graph TD\n  A[Start] --> B[End]";
        let svg = render(src).expect("render");
        // Pick the End-node rect (the second 1.5-stroke rect, which
        // starts at y=92, lower than the Start rect at y=28).
        let end_rect_pos = svg
            .find("<path d=\"M 28.0 92.0 L 98.0 92.0")
            .expect("rect for End should exist");
        // The arrowhead's tip sits at (63, 92) — the End rect's top
        // edge midpoint. It must come AFTER the rect in the SVG so
        // its tip isn't covered by the rect's stroke band.
        let arrowhead_pos = svg
            .rfind("M 63.0 92.0 L 58.0 83.0 L 68.0 83.0 Z")
            .expect("arrowhead for A → B should exist");
        assert!(
            arrowhead_pos > end_rect_pos,
            "arrowhead should be emitted after the rect, \
             but rect at {end_rect_pos} came after arrowhead at {arrowhead_pos}:\n{svg}"
        );
    }

    #[test]
    fn inline_label_no_spaces() {
        // `--label-->` with no surrounding whitespace should also work.
        let out = render("graph TD\n  A-->B\n  A--否-->C\n  C-->D[end]").expect("render");
        assert!(out.contains("<path "));
        // label `否` rendered as text inside the edge label box
        assert!(out.contains("否"), "missing inline label: {out}");
        assert!(out.contains("end"));
    }

    #[test]
    fn obstacle_routed_edges_have_no_corners() {
        // The 7-node graph forces E→G and F→G through an obstacle via.
        // Before the through-tangent fix these multi-segment edges
        // degraded to `M … L … L …` — two straight pieces meeting at a
        // sharp angle at the via. An edge path must be a continuous
        // curve: either a single straight `L` run (collinear anchors)
        // or an all-`C` curve chain, never a `C` stitched to an `L`.
        let src = "graph TD\n\
                   \x20 A[写出新文章] --> B[撰写 Markdown]\n\
                   \x20 B --> C{包含公式?}\n\
                   \x20 C -- 是 --> D[加入行内公式]\n\
                   \x20 C -- 否 --> E{包含图表?}\n\
                   \x20 E -- 是 --> F[加入流程图]\n\
                   \x20 E -- 否 --> G[纯静态页面]\n\
                   \x20 D --> G\n\
                   \x20 F --> G\n";
        let svg = render(src).expect("should render");
        // Collect every path element's `d` attribute.
        let mut attrs = svg.as_str();
        while let Some(start) = attrs.find("<path ") {
            attrs = &attrs[start..];
            let (_d_start, rest) = attrs[1..].split_once("d=\"M ").expect("path has d");
            let end = rest.find("\"").expect("d terminates");
            attrs = &rest[end..];
            let d = format!("M {}", &rest[..end]);
            // Arrowheads are <path fill=...> with `M … L … L … Z` — skip.
            if d.contains(" Z") || d.contains("z") {
                continue;
            }
            let has_c = d.contains(" C ");
            let has_l = d.contains(" L ");
            assert!(
                !(has_c && has_l),
                "edge path mixes curve and line (corner at a via): {d}"
            );
        }
    }

    #[test]
    fn boxed_label_quotes_stripped() {
        // `["text"]` form must drop the surrounding quotes in the rendered
        // SVG (Mermaid uses quoting only to allow brackets/letters in
        // labels — the quotes themselves aren't part of the visible text).
        let out = render("graph TD\n  A[\"plain quote\"] --> B[end]").expect("render");
        assert!(out.contains("plain quote"), "label missing text: {out}");
        assert!(
            !out.contains("\"plain quote\""),
            "label still quoted: {out}"
        );
    }
}
