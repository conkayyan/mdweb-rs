//! A pragmatic TikZ subset for inline diagrams.
//!
//! The implementation is a small recursive-descent parser over an em canvas
//! (y up). It is deliberately permissive: anything not understood is skipped
//! to the next `;`, so a drawing with one unsupported construct still renders
//! the rest. The supported surface is shaped by the doc examples plus the
//! common idioms:
//!
//! - **Top-level commands**: `\draw`, `\fill`, `\filldraw`, `\path`, `\node`,
//!   `\coordinate`, `\def`, `\foreach`, `\tikzset` (ignored).
//! - **Path connects**: `--`, `-|`, `|-`, `.. controls A and B ..`.
//! - **Path closers**: `rectangle`, `circle`, `arc`, `grid`, `cycle`, `plot`.
//! - **Coordinates**: `(x,y)`, `(name)`, `(θ:r)`, `+(dx,dy)`, `++(dx,dy)`,
//!   `($(A)!t!(B)$)`.
//! - **Nodes**: inline `node[opts]{…}` inside a path, plus standalone
//!   `\node[opts] (name) at (c) {…}` and `\coordinate[label=…] (name) at (c);`.
//! - **Options**: `->`, `>=stealth`, `thin`/`thick`/`line width=`,
//!   `dashed`/`dotted`, `color=`/`draw=`/`fill=` (with `name!pct` mixing),
//!   `domain=`/`samples=` for `plot`, `inner sep=`/`pos=`/`midway`/`above`/
//!   `below`/`left`/`right`/`anchor=` for nodes.
//! - **Macros**: `\def\r{1.8}` (numeric substitution) and `\foreach \i in
//!   {…}` (lists and `{a,b,…,c}` ranges).

use super::expr::{eval, Vars};
use super::scan::Scanner;
use super::{
    label, width_from_pt, Anchor, Arrow, ArrowKind, Canvas, Color, Dash, Item, PathOp, Pt, Stroke,
    DEFAULT_INNER_SEP, EM_PER_CM, EM_PER_PT,
};
use std::collections::HashMap;

/// Lower bound for `samples` to skip a path expression that would otherwise
/// render as a single segment.
const SAMPLES_MIN: usize = 2;
/// Upper bound: pathologically high `samples=` would freeze the build.
const SAMPLES_MAX: usize = 400;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub(crate) fn render(src: &str) -> Option<Canvas> {
    let (body, env_opts) = body(src)?;
    let mut env = Env::default();
    env.defaults = Options::from_list(&env_opts);
    env.run(&body);
    if env.canvas.is_empty() {
        None
    } else {
        Some(env.canvas)
    }
}

/// The `\begin{tikzpicture}…\end{tikzpicture}` envelope. Returns the body
/// text plus the environment-level option list (the `[…]` right after the
/// `\begin`), which in TikZ applies to *every* path in the picture — e.g.
/// `\begin{tikzpicture}[domain=0:2]` gives every `plot` that domain unless
/// it overrides it.
fn body(src: &str) -> Option<(String, String)> {
    let begin = src.find("\\begin{tikzpicture}")?;
    let after = begin + "\\begin{tikzpicture}".len();
    let mut s = Scanner::new(&src[after..]);
    let env = s.bracket().unwrap_or_default();
    let head = s.group().unwrap_or_default();
    let rest = &src[after + s.pos()..];
    let end = rest.find("\\end{tikzpicture}")?;
    Some((format!("{head}{}", &rest[..end]), env))
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Env {
    canvas: Canvas,
    /// Stored named coordinates from `\coordinate` and `\node` (name) at.
    nodes: HashMap<String, Pt>,
    /// Numeric aliases set by `\def` and `\foreach`.
    vars: Vars,
    /// Picture-level options from the `[…]` after `\begin{tikzpicture}`.
    /// TikZ applies these to every path and node unless a command overrides
    /// the key — `[domain=0:2]` on the environment, for instance, is the
    /// default domain for every `plot` inside.
    defaults: Options,
    /// Last "current" point in a path — `(a) -- (b)` reads `b` as absolute,
    /// but `++(1,0)` reads it as relative to this.
    last_ref: Option<Pt>,
}

impl Env {
    /// Parse a command's `[…]` option list, overlaid on the picture-level
    /// defaults set by `[domain=0:2]` on the `\begin{tikzpicture}` line
    /// (with the command's own keys winning). Every command in the picture
    /// reads its options through here so environment defaults propagate —
    /// a `plot` whose own `[]` doesn't set `domain` inherits the picture's.
    fn options(&mut self, s: &mut Scanner) -> Options {
        let mut o = self.defaults.clone();
        if let Some(list) = s.bracket() {
            o.apply_list(&list);
        }
        o
    }

    /// Like [`options`], but seeded from an existing `Options` (the
    /// surrounding `\draw`'s). A `node` parked between two `--`s on a
    /// coloured path should land in the path's colour without having to
    /// repeat `color=` in its own `[]`. Domain, samples, and stroke width
    /// carry over too — anything that affects an inline label as well as
    /// a path-level key.
    fn options_from(s: &mut Scanner, base: &Options) -> Options {
        let mut o = base.clone();
        if let Some(list) = s.bracket() {
            o.apply_list(&list);
        }
        o
    }

    fn run(&mut self, src: &str) {
        let mut s = Scanner::new(src);
        while !s.eof() {
            let Some(cmd) = s.command() else {
                s.bump();
                continue;
            };
            match cmd.as_str() {
                "draw" | "fill" | "filldraw" | "path" => {
                    let opts = self.options(&mut s);
                    let ops = self.parse_path(&mut s, &opts);
                    self.finish_path(cmd.as_str(), ops, opts);
                    s.eat(';');
                }
                "node" => {
                    let opts = self.options(&mut s);
                    self.parse_node(&mut s, &opts);
                    s.eat(';');
                }
                "coordinate" => {
                    let opts = self.options(&mut s);
                    self.parse_coordinate(&mut s, &opts);
                    s.eat(';');
                }
                "def" => {
                    if let Some(name) = self.read_def_name(&mut s) {
                        if let Some(body) = s.group() {
                            if let Some(v) = eval(&body, &self.vars) {
                                self.vars.set(&name, v);
                            }
                        }
                    }
                }
                "foreach" => {
                    self.parse_foreach(&mut s);
                }
                // Setup / style / unknown — drop to the next `;`.
                "tikzset" | "pgfset" | "begin" | "end" => s.skip_statement(),
                _ => s.skip_statement(),
            }
        }
    }

    /// Render the path ops produced by `parse_path` as either a stroke, a
    /// fill, or both, with the chosen arrowheads.
    fn finish_path(&mut self, kind: &str, mut ops: Vec<PathOp>, opts: Options) {
        if ops.is_empty() {
            return;
        }
        let stroke = opts.stroke();
        let arrow = opts.arrow();
        let fill = match kind {
            "fill" | "filldraw" => Some(opts.fill().unwrap_or(Color::Current)),
            _ => opts.fill(),
        };
        // If a fill was requested on an open path, the SVG renderer will
        // auto-close; do nothing extra here.
        let _ = &mut ops;
        self.canvas.push(Item::Path {
            ops,
            stroke,
            fill,
            arrow,
        });
    }

    /// `\def\r{1.8}` — the name is the *next* token, not a group.
    fn read_def_name(&mut self, s: &mut Scanner) -> Option<String> {
        s.ws();
        let c = s.peek()?;
        if c == '\\' {
            s.bump();
            let name = s.word();
            if !name.is_empty() {
                Some(name)
            } else {
                Some(s.bump()?.to_string())
            }
        } else {
            Some(s.word())
        }
    }

    /// `\foreach \i in {1,2,...,5} { … }` and the `a,b,…,c` range shorthand.
    fn parse_foreach(&mut self, s: &mut Scanner) {
        let var = match self.read_def_name(s) {
            Some(v) => v,
            None => return,
        };
        // Skip the `in` keyword as a literal word.
        if !s.eat_str("in") {
            return;
        }
        s.ws();
        let Some(list) = s.group() else { return };
        let values = self.foreach_values(&list);
        s.ws();
        let Some(body) = s.group() else { return };
        for v in values {
            self.vars.set(&var, v);
            self.run(&body);
        }
    }

    fn foreach_values(&self, list: &str) -> Vec<f64> {
        let items = super::scan::split_top(list, ',');
        // `1,2,...,5` form: `split_top` gives `["1", "2,...,5"]`.
        if items.len() == 2 {
            if let Some((step_s, end_s)) = items[1].split_once(",...") {
                let a = eval(&items[0], &self.vars);
                let b = eval(step_s, &self.vars);
                let c = eval(end_s, &self.vars);
                if let (Some(a), Some(b), Some(c)) = (a, b, c) {
                    let step = b - a;
                    if step.abs() < 1e-9 {
                        return vec![a];
                    }
                    let mut out = Vec::new();
                    let mut x = a;
                    let limit = if step > 0.0 { c + 1e-9 } else { c - 1e-9 };
                    while (step > 0.0 && x <= limit) || (step < 0.0 && x >= limit) {
                        out.push(x);
                        x += step;
                        if out.len() > 1000 {
                            break;
                        }
                    }
                    return out;
                }
            }
        }
        items
            .iter()
            .filter_map(|s| eval(s, &self.vars))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Options {
    stroke: Option<Color>,
    fill: Option<Color>,
    /// Text colour for `node` / `coordinate` labels. A `color=` key sets
    /// `stroke` *and* `text` together (TikZ's own semantics); a `text=`
    /// key sets just the text colour for the current command. The two
    /// stay separate so `color=blue` on a `\draw` propagates to any
    /// inline `node` it owns, but a node that says `text=red` only
    /// recolours its label and leaves the surrounding path alone.
    text: Option<Color>,
    width: Option<f64>,
    dash: Option<Dash>,
    arrow_start: bool,
    arrow_end: bool,
    arrow_kind: Option<ArrowKind>,
    domain: Option<(f64, f64)>,
    samples: Option<usize>,
    inner_sep: Option<f64>,
    above: Option<f64>,
    below: Option<f64>,
    left: Option<f64>,
    right: Option<f64>,
    anchor: Option<Anchor>,
    label: Option<String>,
    /// Fraction along the path segment (0 = start, 1 = end) for inline
    /// `node[pos=…]{…}` labels. Only meaningful inside a path's
    /// `-- node … (coord)` clause.
    pos: Option<f64>,
    raw: Vec<(String, String)>,
}

impl Options {
    fn from_list(list: &str) -> Options {
        let mut o = Options::default();
        o.apply_list(list);
        o
    }

    fn apply_list(&mut self, list: &str) {
        for key in super::scan::split_top(list, ',') {
            self.apply(&key);
        }
    }

    fn apply(&mut self, k: &str) {
        let k = k.trim();
        if k.is_empty() {
            return;
        }
        let (name, val) = match k.split_once('=') {
            Some((n, v)) => (n.trim(), v.trim()),
            None => {
                self.apply_flag(k);
                return;
            }
        };
        match name {
            "color" | "draw" => {
                // TikZ's `color=` paints both stroke and text — keep the
                // two channels in lock-step so an inline `node` along a
                // coloured `\draw` lands in the same hue.
                let c = Color::named(val).or(Some(Color::Current));
                self.stroke = c;
                self.text = c;
            }
            "text" => self.text = Color::named(val),
            "fill" => self.fill = Color::named(val),
            "line width" | "line width=" => {
                if let Some(cm) = eval(val, &Vars::new()) {
                    self.width = Some((cm * EM_PER_CM).max(0.015));
                }
            }
            "dashed" => self.dash = Some(Dash::Dashed),
            "densely dashed" => self.dash = Some(Dash::DenselyDashed),
            "loosely dashed" => self.dash = Some(Dash::LooselyDashed),
            "dotted" => self.dash = Some(Dash::Dotted),
            "dash dot" => self.dash = Some(Dash::DashDot),
            "domain" => {
                if let Some((a, b)) = val.split_once(':') {
                    if let (Some(x), Some(y)) = (eval(a, &Vars::new()), eval(b, &Vars::new())) {
                        self.domain = Some((x, y));
                    }
                }
            }
            "samples" => {
                if let Some(n) = eval(val, &Vars::new()) {
                    self.samples = Some((n as usize).clamp(SAMPLES_MIN, SAMPLES_MAX));
                }
            }
            "inner sep" => {
                // A bare number is in points, TikZ's default unit for
                // dimensions (`inner sep=4` ≡ 4 pt).
                if let Some(pt) = eval(val, &Vars::new()) {
                    self.inner_sep = Some(pt * EM_PER_PT);
                }
            }
            "pos" => {
                // TikZ `pos=0.03` — fraction along the current path
                // segment. Only meaningful inside a `-- node[pos=…]{…}
                // (coord)` clause.
                if let Some(v) = eval(val, &Vars::new()) {
                    self.pos = Some(v.clamp(0.0, 1.0));
                }
            }
            "anchor" => self.anchor = Anchor::from_name(val),
            "label" => {
                // `label=below:$B$` — split the directional prefix from the
                // body so the math span is rendered as math, not text.
                if let Some((dir, body)) = val.split_once(':') {
                    self.apply_flag(dir.trim());
                    self.label = Some(body.trim().to_string());
                } else {
                    self.label = Some(val.to_string());
                    // TikZ defaults `label=foo` (no direction) to `above`,
                    // so the label sits on top of the coordinate.
                    if self.above.is_none()
                        && self.below.is_none()
                        && self.left.is_none()
                        && self.right.is_none()
                    {
                        self.above = Some(1.0);
                    }
                }
            }
            "above" => self.above = Some(1.0),
            "below" => self.below = Some(1.0),
            "left" => self.left = Some(1.0),
            "right" => self.right = Some(1.0),
            "above left" => {
                self.above = Some(1.0);
                self.left = Some(1.0);
            }
            "above right" => {
                self.above = Some(1.0);
                self.right = Some(1.0);
            }
            "below left" => {
                self.below = Some(1.0);
                self.left = Some(1.0);
            }
            "below right" => {
                self.below = Some(1.0);
                self.right = Some(1.0);
            }
            "->" => self.arrow_end = true,
            "<-" => self.arrow_start = true,
            "<->" => {
                self.arrow_start = true;
                self.arrow_end = true;
            }
            "xstep" | "ystep" | "step" => self.raw.push((name.to_string(), val.to_string())),
            _ => {}
        }
    }

    fn apply_flag(&mut self, k: &str) {
        match k {
            "thin" => self.width = Some(width_from_pt(0.4)),
            "thick" => self.width = Some(width_from_pt(0.8)),
            "very thin" => self.width = Some(width_from_pt(0.2)),
            "very thick" => self.width = Some(width_from_pt(1.2)),
            "ultra thin" => self.width = Some(width_from_pt(0.1)),
            "ultra thick" => self.width = Some(width_from_pt(1.6)),
            "dashed" => self.dash = Some(Dash::Dashed),
            "dotted" => self.dash = Some(Dash::Dotted),
            "densely dashed" => self.dash = Some(Dash::DenselyDashed),
            "loosely dashed" => self.dash = Some(Dash::LooselyDashed),
            "->" => self.arrow_end = true,
            "<-" => self.arrow_start = true,
            "<->" => {
                self.arrow_start = true;
                self.arrow_end = true;
            }
            "fill" => self.fill = Some(Color::Current),
            "draw" => self.stroke = Some(Color::Current),
            "above" => self.above = Some(1.0),
            "below" => self.below = Some(1.0),
            "left" => self.left = Some(1.0),
            "right" => self.right = Some(1.0),
            "above left" => {
                self.above = Some(1.0);
                self.left = Some(1.0);
            }
            "above right" => {
                self.above = Some(1.0);
                self.right = Some(1.0);
            }
            "below left" => {
                self.below = Some(1.0);
                self.left = Some(1.0);
            }
            "below right" => {
                self.below = Some(1.0);
                self.right = Some(1.0);
            }
            _ => {}
        }
    }

    fn stroke(&self) -> Option<Stroke> {
        let mut s = Stroke::default();
        if let Some(c) = self.stroke {
            s.color = c;
        }
        if let Some(w) = self.width {
            s.width = w;
        }
        if let Some(d) = self.dash {
            s.dash = d;
        }
        Some(s)
    }

    fn fill(&self) -> Option<Color> {
        self.fill
    }

    fn arrow(&self) -> Arrow {
        Arrow {
            start: self.arrow_start,
            end: self.arrow_end,
            kind: self.arrow_kind.unwrap_or(ArrowKind::To),
            scale: 1.0,
        }
    }
}

impl Anchor {
    fn from_name(s: &str) -> Option<Anchor> {
        Some(match s.trim() {
            "center" => Anchor::Center,
            "north" => Anchor::North,
            "south" => Anchor::South,
            "east" => Anchor::East,
            "west" => Anchor::West,
            "north east" | "north-east" => Anchor::NorthEast,
            "north west" | "north-west" => Anchor::NorthWest,
            "south east" | "south-east" => Anchor::SouthEast,
            "south west" | "south-west" => Anchor::SouthWest,
            "base" => Anchor::Base,
            "base west" => Anchor::BaseWest,
            "base east" => Anchor::BaseEast,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Coordinate parsing
// ---------------------------------------------------------------------------

/// Parse a coordinate into a canvas point. Recognises:
/// - `(x,y)` Cartesian; numeric or `\name`;
/// - `(name)` named coordinate, looked up in `env.nodes`;
/// - `(θ:r)` polar, both parts in TikZ centimetres;
/// - `+(dx,dy)` / `++(dx,dy)` relative to `env.last_ref`;
/// - `($(A)!t!(B)$)` linear interpolation between two named points.
fn parse_coord(s: &mut Scanner, env: &mut Env) -> Option<Pt> {
    s.ws();
    let base = env.last_ref.unwrap_or((0.0, 0.0));
    let mut rel = false;
    let mut update = false;
    if s.peek() == Some('+') {
        s.bump();
        rel = true;
        if s.peek() == Some('+') {
            s.bump();
            update = true;
        }
    }
    if s.peek() == Some('$') {
        return calc_coord(s, env).map(|p| {
            let p = if rel {
                (base.0 + p.0, base.1 + p.1)
            } else {
                p
            };
            if update {
                env.last_ref = Some(p);
            }
            p
        });
    }
    let Some(body) = s.paren() else {
        return None;
    };
    let p = coord_body(&body, env);
    let p = match (rel, p) {
        (true, Some(p)) => Some((base.0 + p.0, base.1 + p.1)),
        (false, Some(p)) => Some(p),
        _ => None,
    };
    if update {
        env.last_ref = p;
    }
    p
}

fn coord_body(body: &str, env: &Env) -> Option<Pt> {
    let b = body.trim();
    if b.is_empty() {
        return None;
    }
    // Named coordinate `(A)` — no comma, no colon.
    if !b.contains(',') && !b.contains(':') {
        let name = b.trim_start_matches('\\').to_string();
        if let Some(p) = env.nodes.get(&name) {
            return Some(*p);
        }
        if let Some(v) = eval(b, &env.vars) {
            return Some((cm_to_em(v), cm_to_em(v)));
        }
        return None;
    }
    if let Some((a, b)) = b.split_once(':') {
        let ang = eval(a, &env.vars)?;
        let r = cm_to_em(eval(b, &env.vars)?);
        let rad = ang.to_radians();
        return Some((r * rad.cos(), r * rad.sin()));
    }
    let (x, y) = b.split_once(',')?;
    Some((cm_to_em(eval(x, &env.vars)?), cm_to_em(eval(y, &env.vars)?)))
}

/// TikZ user units are centimetres (`expr` doc: "1 = 1 cm"); the canvas
/// and every emitted length are in `em` of the surrounding text. Scale
/// every coordinate-like value at its source so 1 cm renders as
/// [`EM_PER_CM`] em — without this a `(1.8,0)` base renders barely wider
/// than a label glyph and the whole drawing collapses to under half its
/// intended size.
fn cm_to_em(v: f64) -> f64 {
    v * EM_PER_CM
}

/// `($(A)!t!(B)$)` — `t` is a fraction between two points.
fn calc_coord(s: &mut Scanner, env: &Env) -> Option<Pt> {
    s.bump();
    let a = parse_coord_value(s, env)?;
    s.ws();
    if s.peek() != Some('!') {
        return None;
    }
    s.bump();
    let t = eval(&read_until_bang(s), &env.vars).unwrap_or(0.5);
    s.ws();
    if s.peek() != Some('!') {
        return None;
    }
    s.bump();
    let b = parse_coord_value(s, env)?;
    s.ws();
    if s.peek() != Some('$') {
        return None;
    }
    s.bump();
    Some((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t))
}

/// `(A)` inside a `($…$)` — fewer preconditions than `parse_coord`.
fn parse_coord_value(s: &mut Scanner, env: &Env) -> Option<Pt> {
    let body = s.paren()?;
    coord_body(&body, env)
}

/// Read a run of tokens up to the next `!` or `)`.
fn read_until_bang(s: &mut Scanner) -> String {
    let mut buf = String::new();
    while let Some(c) = s.peek() {
        if c == '!' || c == ')' {
            break;
        }
        buf.push(c);
        s.bump();
    }
    buf
}

// ---------------------------------------------------------------------------
// Path parsing
// ---------------------------------------------------------------------------

/// An inline `node[opts]{label}` parked between `--` and the segment's
/// trailing coord. The actual placement point depends on the endpoint,
/// which the queue holds until the closing coord arrives.
struct PendingInline {
    opts: Options,
    body: String,
    /// Fraction along `[start, end]` for placement. `pos=0.5` (the TikZ
    /// default for inline nodes) lives at the midpoint; `pos=0.03` lands
    /// near the start.
    pos_frac: f64,
    /// `Some(p)` when the inline node carried an explicit `at (coord)`
    /// — then `pos_frac` is irrelevant and we use this point instead.
    explicit_at: Option<Pt>,
}

impl Env {
    /// Walk the path tokens until `;` or EOF, yielding a sequence of ops.
    fn parse_path(&mut self, s: &mut Scanner, opts: &Options) -> Vec<PathOp> {
        let mut ops = Vec::new();
        let mut pos: Pt = (0.0, 0.0);
        let mut started = false;
        // `-- node[opts]{...}` consumes the connector but defers the line
        // segment itself to the trailing coord. Set when the next coord
        // we see should become a Line rather than a bare pos update.
        let mut pending_line = false;
        // Inline nodes between a `--` and the segment's terminating coord.
        // We queue them because their actual placement depends on the
        // endpoint: a bare `node[…] {label}` defaults to the segment
        // midpoint, while `node[pos=…]{label}` uses the fraction. The
        // labels stay parked here until the closing coord arrives, then
        // we drain the queue and place each along the resolved segment.
        let mut pending_inline: Vec<PendingInline> = Vec::new();

        while !s.eof() {
            s.ws();
            match s.peek() {
                None | Some(';') => break,
                Some('-') | Some('.') => {
                    // `-- node[...]{...} (coord)` — the connector followed
                    // by one or more inline node labels. Park the nodes
                    // onto `pending_inline`; the actual placement waits
                    // until the segment's terminating coord is parsed,
                    // at which point each label is dropped at its
                    // `pos=` fraction along the resolved segment.
                    if s.starts_with("--") {
                        let save = s.pos();
                        s.advance(2);
                        s.ws();
                        if s.starts_with("node") {
                            s.advance(4);
                            // A node belongs to a path: its `[]` overlays
                            // the path's keys, so `color=blue` on the
                            // `\draw` re-tints a `node[…]` it owns
                            // without the user repeating it.
                            let o = Self::options_from(s, opts);
                            self.queue_inline_node(s, pos, o, &mut pending_inline);
                            // The next `--` (without a trailing coord yet)
                            // extends the same segment — the label we just
                            // queued still belongs to the upcoming line.
                            pending_line = true;
                            continue;
                        } else {
                            s.set_pos(save);
                        }
                    }
                    pending_line = false;
                    if let Some((to, ops2)) = self.read_connect(s, pos, started) {
                        ops.extend(ops2);
                        pos = to;
                        continue;
                    }
                    // read_connect already consumed the connector (`--`,
                    // `..`, `-|`, `|-`) and a partial coord before
                    // returning None. The path is unrecoverable from here
                    // — anything we bump past is mis-aligned. Hand control
                    // back to the caller; it will either hit `;` / EOF or
                    // dispatch on a fresh peek.
                    break;
                }
                Some('+') | Some('(') => {
                    if let Some(p) = parse_coord(s, self) {
                        let seg_start = pos;
                        if pending_line && started {
                            // Deferred line from `-- node[...]{...}` —
                            // emit it now and update the running point.
                            ops.push(PathOp::Line(p));
                            pending_line = false;
                        } else if !started {
                            ops.push(PathOp::Move(p));
                            started = true;
                        }
                        // Drain queued inline nodes that were parked
                        // between the previous `--` and this coord. Each
                        // one is placed at its `pos=` fraction along the
                        // segment, or at its explicit `at (coord)`.
                        let dx = p.0 - seg_start.0;
                        let dy = p.1 - seg_start.1;
                        for node in pending_inline.drain(..) {
                            let place = match node.explicit_at {
                                Some(at) => at,
                                None => (
                                    seg_start.0 + dx * node.pos_frac,
                                    seg_start.1 + dy * node.pos_frac,
                                ),
                            };
                            self.place_label(&node.opts, place, &node.body);
                        }
                        pos = p;
                        continue;
                    }
                    s.bump();
                }
                Some('\\') => {
                    if let Some(c) = s.command() {
                        match c.as_str() {
                            "node" => {
                                let o = Self::options_from(s, opts);
                                self.parse_inline_node(s, &o, pos);
                                continue;
                            }
                            "coordinate" => {
                                let o = Self::options_from(s, opts);
                                self.parse_coordinate(s, &o);
                                continue;
                            }
                            "plot" => {
                                let o = Self::options_from(s, opts);
                                let new = self.parse_plot(s, &o);
                                if !new.is_empty() {
                                    if let Some(p) = self.append_plot(&mut ops, new) {
                                        pos = p;
                                    }
                                    started = true;
                                }
                                continue;
                            }
                            _ => {
                                s.skip_statement();
                                continue;
                            }
                        }
                    }
                    s.bump();
                }
                Some('c') => {
                    if s.eat_str("cycle") {
                        if started {
                            ops.push(PathOp::Close);
                            started = false;
                        }
                        continue;
                    }
                    if s.eat_str("controls") {
                        if let Some((c1, c2)) = self.consume_controls(s) {
                            if let Some(p) = parse_coord(s, self) {
                                ops.push(PathOp::Bezier { c1, c2, to: p });
                                pos = p;
                                continue;
                            }
                        }
                        continue;
                    }
                    if s.eat_str("circle") {
                        if let Some(body) = s.paren() {
                            let r = cm_to_em(eval(&body, &self.vars).unwrap_or(0.0));
                            if r > 0.0 {
                                self.canvas.push(Item::Circle {
                                    c: pos,
                                    r,
                                    stroke: opts.stroke(),
                                    fill: opts.fill(),
                                });
                            }
                        }
                        continue;
                    }
                    s.bump();
                }
                Some('r') => {
                    if s.eat_str("rectangle") {
                        if let Some(p) = parse_coord(s, self) {
                            let (x0, x1) = min_max(pos.0, p.0);
                            let (y0, y1) = min_max(pos.1, p.1);
                            ops.push(PathOp::Move((x0, y0)));
                            ops.push(PathOp::Line((x1, y0)));
                            ops.push(PathOp::Line((x1, y1)));
                            ops.push(PathOp::Line((x0, y1)));
                            ops.push(PathOp::Close);
                            pos = p;
                            started = true;
                            continue;
                        }
                    }
                    s.bump();
                }
                Some('a') => {
                    if s.eat_str("arc") {
                        if let Some(body) = s.paren() {
                            let parts: Vec<&str> = body.split(',').collect();
                            let (a0, a1) = if parts.len() >= 2 {
                                (
                                    eval(parts[0], &self.vars).unwrap_or(0.0),
                                    eval(parts[1], &self.vars).unwrap_or(0.0),
                                )
                            } else {
                                (0.0, 0.0)
                            };
let r = if parts.len() >= 3 {
                                cm_to_em(eval(
                                    parts[2],
                                    &self.vars,
                                ).unwrap_or(0.5))
                            } else {
                                cm_to_em(0.5)
                            };
                            if r > 0.0 {
                                self.canvas.push(Item::Arc {
                                    c: pos,
                                    rx: r,
                                    ry: r,
                                    a0,
                                    a1,
                                    stroke: opts.stroke().unwrap_or_default(),
                                });
                            }
                        }
                        continue;
                    }
                    s.bump();
                }
                Some('g') => {
                    if s.eat_str("grid") {
                        if let Some(p) = parse_coord(s, self) {
                            let step = grid_step(opts);
                            self.canvas.push(Item::Grid {
                                p0: pos,
                                p1: p,
                                step,
                                stroke: opts.stroke().unwrap_or_default(),
                            });
                            // `(-0.1,-0.1) grid (2.1,2.1)` reads the grid's
                            // start as a plain coordinate, which the walker
                            // recorded as a lone Move — the grid owns that
                            // point now, so drop the dangling Move instead
                            // of emitting an empty stub path.
                            while matches!(ops.last(), Some(PathOp::Move(_))) {
                                ops.pop();
                            }
                            started = false;
                            continue;
                        }
                    }
                    s.bump();
                }
                Some('n') => {
                    if s.eat_str("node") {
                        let o = Self::options_from(s, opts);
                        // A bare `node[…] {label}` after `--` belongs to
                        // the upcoming segment; otherwise it is placed
                        // immediately at the current path position.
                        if pending_line {
                            self.queue_inline_node(s, pos, o, &mut pending_inline);
                        } else {
                            self.parse_inline_node(s, &o, pos);
                        }
                        continue;
                    }
                    s.bump();
                }
                Some('p') => {
                    if s.eat_str("plot") {
                        let o = self.options(s);
                        let new = self.parse_plot(s, &o);
                        if !new.is_empty() {
                            if let Some(p) = self.append_plot(&mut ops, new) {
                                pos = p;
                            }
                            started = true;
                        }
                        continue;
                    }
                    s.bump();
                }
                _ => {
                    s.bump();
                }
            }
        }
        ops
    }

    /// Read the `at (coord)` (optional) and `{body}` of an inline node,
    /// then push a `PendingInline` onto the queue. Called only when the
    /// caller has just seen a `--` connector — the actual placement
    /// waits until the segment's terminating coord is parsed.
    fn queue_inline_node(
        &mut self,
        s: &mut Scanner,
        _seg_start: Pt,
        opts: Options,
        queue: &mut Vec<PendingInline>,
    ) {
        s.ws();
        let explicit_at = if s.eat_str("at") {
            s.ws();
            parse_coord(s, self)
        } else {
            None
        };
        s.ws();
        let body = s.group().unwrap_or_default();
        queue.push(PendingInline {
            pos_frac: opts.pos.unwrap_or(0.5),
            opts,
            body,
            explicit_at,
        });
    }

    /// `--`, `-|`, `|-`, or `.. controls A and B ..`. Returns the new
    /// position and the path ops to emit.
    fn read_connect(
        &mut self,
        s: &mut Scanner,
        pos: Pt,
        _started: bool,
    ) -> Option<(Pt, Vec<PathOp>)> {
        if s.eat_str("..") {
            let (c1, c2) = self.consume_controls(s)?;
            if let Some(p) = parse_coord(s, self) {
                return Some((
                    p,
                    vec![PathOp::Bezier { c1, c2, to: p }],
                ));
            }
            return None;
        }
        if s.eat_str("--") {
            if let Some(p) = parse_coord(s, self) {
                return Some((p, vec![PathOp::Line(p)]));
            }
            return None;
        }
        if s.eat_str("-|") {
            if let Some(p) = parse_coord(s, self) {
                let mid = (p.0, pos.1);
                return Some((p, vec![PathOp::Line(mid), PathOp::Line(p)]));
            }
            return None;
        }
        if s.eat_str("|-") {
            if let Some(p) = parse_coord(s, self) {
                let mid = (pos.0, p.1);
                return Some((p, vec![PathOp::Line(mid), PathOp::Line(p)]));
            }
            return None;
        }
        None
    }

    /// `.. controls A and B ..` — return the two control points in canvas
    /// coordinates, taking the current path position as the implicit anchor.
    fn consume_controls(&mut self, s: &mut Scanner) -> Option<(Pt, Pt)> {
        let a = s.paren().and_then(|b| coord_body(&b, &self))?;
        s.ws();
        if !s.eat_str("and") {
            return None;
        }
        let b = s.paren().and_then(|b2| coord_body(&b2, &self))?;
        s.ws();
        if !s.eat_str("..") {
            return None;
        }
        Some((a, b))
    }

    /// `plot (\x,{expr})` with the surrounding options.
    fn parse_plot(&mut self, s: &mut Scanner, opts: &Options) -> Vec<PathOp> {
        let Some(body) = s.paren() else { return Vec::new() };
        let domain = opts.domain.unwrap_or((0.0, 1.0));
        // TikZ's default `samples` is 25, not 50 — a `plot` with no explicit
        // `samples=` must discretise as coarsely as TikZ does.
        let samples = opts.samples.unwrap_or(25);
        let inner = body.trim().strip_prefix('(').unwrap_or(&body);
        let inner = inner.strip_suffix(')').unwrap_or(inner).trim();
        let parts = super::scan::split_top(inner, ',');
        if parts.len() != 2 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut vars = self.vars.clone();
        for i in 0..=samples {
            let t = i as f64 / samples as f64;
            let x = domain.0 + (domain.1 - domain.0) * t;
            vars.set("x", x);
            let x_v = eval(&parts[0], &vars);
            let y = eval(&parts[1], &vars);
            if let (Some(x), Some(y)) = (x_v, y) {
                let p = (cm_to_em(x), cm_to_em(y));
                if i == 0 {
                    out.push(PathOp::Move(p));
                } else {
                    out.push(PathOp::Line(p));
                }
            }
        }
        out
    }

    /// Fold `parse_plot`'s trace into `ops`. A plot already begins with its
    /// own `Move`, so when it opens an un-started path we adopt that Move
    /// instead of emitting a duplicate `M … M …` stub. Returns the trace's
    /// endpoint, or `None` when the plot produced no points.
    fn append_plot(&mut self, ops: &mut Vec<PathOp>, new: Vec<PathOp>) -> Option<Pt> {
        if new.is_empty() {
            return None;
        }
        // Guard against a plot trace that omits its leading Move.
        if !matches!(new.first(), Some(PathOp::Move(_))) {
            if let Some(p) = new.first() {
                ops.push(PathOp::Move(point_of(p)));
            }
        }
        let end = new.last().map(point_of);
        ops.extend(new);
        end
    }

    /// Standalone `\node[opts] (name) at (coord) {label}`.
    fn parse_node(&mut self, s: &mut Scanner, opts: &Options) {
        let name = s.paren();
        s.ws();
        if s.eat_str("at") {
            // nothing — the next paren is the coordinate.
        }
        let at = parse_coord(s, self);
        let text = s.group().unwrap_or_default();
        if let (Some(nm), Some(p)) = (name, at) {
            self.nodes.insert(nm.trim().to_string(), p);
        }
        if let Some(p) = at {
            self.place_label(opts, p, &text);
        }
    }

    /// `\coordinate[opts] (name) at (c);` — register the point, optionally
    /// with a `label=above:$X$` style key.
    fn parse_coordinate(&mut self, s: &mut Scanner, opts: &Options) {
        let name = s.paren();
        s.ws();
        if s.eat_str("at") {
            // fall through to the paren below.
        }
        let at = s.paren().and_then(|b| coord_body(&b, self));
        if let (Some(nm), Some(p)) = (name, at) {
            self.nodes.insert(nm.trim().to_string(), p);
        }
        if let (Some(p), Some(label)) = (at, &opts.label) {
            self.place_label(opts, p, label);
        }
    }

    /// `node[opts]{label}` in the middle of a path.
    fn parse_inline_node(&mut self, s: &mut Scanner, opts: &Options, pos: Pt) {
        s.ws();
        if s.eat_str("at") {
            if let Some(p) = parse_coord(s, self) {
                let text = s.group().unwrap_or_default();
                self.place_label(opts, p, &text);
                return;
            }
        }
        let text = s.group().unwrap_or_default();
        self.place_label(opts, pos, &text);
    }

    /// Position a label relative to `at` according to `opts`.
    fn place_label(&mut self, opts: &Options, at: Pt, text: &str) {
        let Some(ts) = label(text) else { return };
        let anchor = opts.anchor.unwrap_or_else(|| node_anchor(opts));
        let gap = opts.inner_sep.unwrap_or(DEFAULT_INNER_SEP);
        // `text` overrides the inherited path colour; a missing `text`
        // falls back to the path stroke (so `color=blue` propagates);
        // nothing overrides → `Current`, which means the page's text
        // colour and therefore reads in dark mode too.
        let color = opts.text.or(opts.stroke).unwrap_or(Color::Current);
        self.canvas.push(Item::Label {
            at,
            anchor,
            ts,
            gap,
            color,
        });
    }
}

fn point_of(op: &PathOp) -> Pt {
    match op {
        PathOp::Move(p) | PathOp::Line(p) => *p,
        PathOp::Bezier { to, .. } => *to,
        PathOp::Close => (0.0, 0.0),
    }
}

fn min_max(a: f64, b: f64) -> (f64, f64) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Pick the anchor for a label based on `above`/`below`/`left`/`right` keys.
fn node_anchor(opts: &Options) -> Anchor {
    let y = opts.above.unwrap_or(0.0) - opts.below.unwrap_or(0.0);
    let x = opts.right.unwrap_or(0.0) - opts.left.unwrap_or(0.0);
    // `f64::signum` returns 1.0 for +0.0, so classify against zero
    // explicitly: a bare `above`/`below`/`right` must resolve to the plain
    // North/South/East anchor, not drift into a corner.
    let sx = if x < 0.0 { -1 } else if x > 0.0 { 1 } else { 0 };
    let sy = if y < 0.0 { -1 } else if y > 0.0 { 1 } else { 0 };
    match (sx, sy) {
        (1, 1) => Anchor::NorthEast,
        (-1, 1) => Anchor::NorthWest,
        (1, -1) => Anchor::SouthEast,
        (-1, -1) => Anchor::SouthWest,
        (1, 0) => Anchor::East,
        (-1, 0) => Anchor::West,
        (0, 1) => Anchor::North,
        (0, -1) => Anchor::South,
        _ => Anchor::Center,
    }
}

fn grid_step(opts: &Options) -> Pt {
    // TikZ's default grid step is 1 cm — the same unit as every coordinate,
    // so the default must be scaled to `em` exactly like `(x,y)` is. A bare
    // `(1.0, 1.0)` would draw a 1-em-spaced grid, ~2.36× too dense.
    let mut step = (EM_PER_CM, EM_PER_CM);
    for (k, v) in &opts.raw {
        match k.as_str() {
            "xstep" => step.0 = cm_to_em(eval(v, &Vars::new()).unwrap_or(1.0)),
            "ystep" => step.1 = cm_to_em(eval(v, &Vars::new()).unwrap_or(1.0)),
            "step" => {
                let s = cm_to_em(eval(v, &Vars::new()).unwrap_or(1.0));
                step = (s, s);
            }
            _ => {}
        }
    }
    step
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::render as draw;

    const TRIANGLE: &str = r"\begin{tikzpicture}
\def\r{1.8}
\coordinate[label=below:$B$] (B) at (-\r,0);
\coordinate[label=above:$C$] (C) at (\r,0);
\coordinate[label=left:$A$]  (A) at (0,\r);
\draw[thin] (A) -- node[above]{$c$} (B) -- (C) -- node[right]{$b$} (A);
\end{tikzpicture}";

    const AXES: &str = r"\begin{tikzpicture}[domain=0:2]
\draw[->] (-0.2,0) -- (4.2,0) node[right] {$x$};
\draw[->] (0,-0.2) -- (0,3.2) node[above] {$y$};
\draw[color=blue] plot (\x,{sin(\x r)}) node[right] {$y=\sin x$};
\draw[gray, dotted] (0,0) grid (3.5,2);
\end{tikzpicture}";

    #[test]
    fn triangle_renders() {
        let out = draw(TRIANGLE).expect("svg");
        assert!(out.contains("aria-label=\"TikZ drawing\""), "{out}");
        assert!(out.contains("<text"), "labels are typeset: {out}");
    }

    #[test]
    fn axes_render_with_plot_and_grid() {
        let out = draw(AXES).expect("svg");
        assert!(out.contains("aria-label=\"TikZ drawing\""), "{out}");
        assert!(
            out.matches("<path").count() >= 4,
            "axes + plot + grid: {out}"
        );
    }

    #[test]
    fn def_supplies_a_numeric_macro() {
        let out = draw(
            r"\begin{tikzpicture}\def\r{2}\draw (0,0) -- (\r,1);\end{tikzpicture}",
        )
        .expect("svg");
        // The macro resolves to 2, so the path ends at (2, 1) cm →
        // 2 · EM_PER_CM em — the bbox shift moves the path to
        // `L 4.997 0.272`.
        assert!(out.contains("L 4.997 0.272"), "the macro resolves: {out}");
    }

    #[test]
    fn macro_works_inside_an_arithmetic_expression() {
        let out = draw(
            r"\begin{tikzpicture}\def\r{2}\draw (0,0) -- (0.5*\r,1);\end{tikzpicture}",
        )
        .expect("svg");
        // 0.5 * 2 = 1, so the path ends at (1, 1) cm → 1 · EM_PER_CM em
        // → `L 2.635 0.272`.
        assert!(out.contains("L 2.635"), "0.5*\\r resolves: {out}");
    }

    #[test]
    fn coordinate_macro_resolves_and_registers() {
        let out = draw(
            r"\begin{tikzpicture}\def\r{2}\coordinate (B) at (-\r,0);\coordinate (C) at (\r,0);\draw (B) -- (C);\end{tikzpicture}",
        )
        .expect("svg");
        // B at x=-2 cm → -2 · EM_PER_CM → 0.272 (after the bbox shift).
        // C at x=+2 cm → +2 · EM_PER_CM → 9.721.
        // If `\r` failed to resolve, both collapse to x=0.
        assert!(
            out.contains("M 0.272") && out.contains("L 9.721"),
            "B and C resolve to distinct points: {out}"
        );
    }

    #[test]
    fn full_triangle_with_inline_alpha_node() {
        let src = r"\begin{tikzpicture}\small
\def\r{1.8}
\coordinate[label=$A$] (A) at (0.5*\r,0.8*\r);
\coordinate[label=below:$B$] (B) at (-\r,0);
\coordinate[label=below:$C$] (C) at (\r,0);
\draw[thin] (A) -- node[above] {$c$}
   node[pos=0.03,below,inner sep=4] {$\alpha$}
   (B) -- (C) -- node[right] {$b$} (A);
\end{tikzpicture}";
        let out = draw(src).expect("svg");
        // Source has 4 named points → the path is M + 3 L's. If the
        // `-- node c node α (B)` chain swallowed the line to B, we'd
        // see M + 2 L's instead.
        let d = out
            .split("path d=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap_or("");
        let l_count = d.matches(" L ").count();
        assert!(
            l_count >= 3,
            "expected 3 line segments in the triangle, got {l_count}: {d}"
        );
    }

    #[test]
    fn inline_node_lands_at_segment_midpoint_and_pos_fraction() {
        // The triangle: A (top), B (left), C (right). Three inline
        // labels on its edges:
        //   `-- node[above]{c}`            → midpoint of AB
        //   `-- node[pos=0.03,below]{α}`   → 3 % along AB from A
        //   `-- node[right]{b}`            → midpoint of CA
        // Before the deferred-placement fix all three were placed at
        // the segment start (A), so `c` and `b` overlapped with the
        // `A` vertex label. The test pins distance ratios — anchor
        // offsets (above/below/right) shift the *baseline*, but the
        // placement point's relative position along the segment is
        // what deferred placement controls.
        let src = r"\begin{tikzpicture}\small
\def\r{1.8}
\coordinate[label=$A$] (A) at (0.5*\r,0.8*\r);
\coordinate[label=below:$B$] (B) at (-\r,0);
\coordinate[label=below:$C$] (C) at (\r,0);
\draw[thin] (A) -- node[above] {$c$}
   node[pos=0.03,below,inner sep=4] {$\alpha$}
   (B) -- (C) -- node[right] {$b$} (A);
\end{tikzpicture}";
        let out = draw(src).expect("svg");

        // Extract vertex coordinates from the `<path d="…">` — the
        // triangle is M, L, L, L (closed). The first three distinct
        // points are A, B, C in path order; the closing L back to A
        // shares A's coordinates.
        let d = out
            .split("path d=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap_or("");
        let coords: Vec<f64> = d
            .split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<f64>().ok())
            .collect();
        // M + 3 L's = 8 numbers: xA yA xB yB xC yC xA yA.
        assert!(
            coords.len() >= 8,
            "expected at least 8 numbers in path, got {}: {d}",
            coords.len()
        );
        let (ax, ay) = (coords[0], coords[1]);
        let (bx, by) = (coords[2], coords[3]);
        let (cx, cy) = (coords[4], coords[5]);

        // Helper: pull (x, y) of a `<text …>label</text>` span.
        let text_xy = |label: &str| -> (f64, f64) {
            let needle = format!(">{label}</text>");
            let tail = out.split(&needle).next().unwrap_or("");
            let last_open = tail.rfind("<text").unwrap_or(0);
            let chunk = &tail[last_open..];
            let x = chunk
                .split("x=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(f64::NAN);
            let y = chunk
                .split("y=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(f64::NAN);
            (x, y)
        };

        let (cx_label, cy_label) = text_xy("c");
        let (ax_label, ay_label) = text_xy("α");
        let (bx_label, by_label) = text_xy("b");

        // Project a point onto the AB segment and return the parameter
        // t in [0, 1] (0 = at A, 1 = at B). Anchor offsets shift the
        // rendered baseline by ~(0, +h+LABEL_SEP), which is partly
        // perpendicular to AB and partly along it — so the projection
        // tolerates some drift, but it is far more robust than raw
        // distance when the anchor is `above`/`below`/`right`.
        let t_ab = |x: f64, y: f64| -> f64 {
            let dx = bx - ax;
            let dy = by - ay;
            let denom = dx * dx + dy * dy;
            if denom == 0.0 {
                return 0.0;
            }
            ((x - ax) * dx + (y - ay) * dy) / denom
        };

        // `c` (default pos=0.5) sits at the midpoint of AB — its
        // projection onto AB should be ~0.5.
        let t_c = t_ab(cx_label, cy_label);
        assert!(
            (t_c - 0.5).abs() < 0.25,
            "c should project near AB midpoint (t=0.5); \
             got t={t_c}; A=({ax},{ay}) B=({bx},{by})"
        );
        // And it must not be bunched at A.
        assert!(
            t_c > 0.2,
            "c landed near A (t={t_c}); deferred placement is broken"
        );

        // `α` (pos=0.03) hugs A — projection near 0.
        let t_alpha = t_ab(ax_label, ay_label);
        assert!(
            t_alpha < 0.35,
            "α should hug A (pos=0.03 along AB); \
             got t={t_alpha}; A=({ax},{ay}) B=({bx},{by})"
        );

        // `b` (default pos=0.5) sits at the midpoint of CA, anchored
        // `right` — so its baseline origin is `inner sep` past the midpoint
        // and its ink is vertically centred on the midpoint. The old
        // projection test no longer applies: the `right` gap pushes the
        // origin's projection back along CA, while the ink, not the em
        // box, is what sits on the midpoint.
        let mid_ca_x = (cx + ax) / 2.0;
        let mid_ca_y = (cy + ay) / 2.0;
        // The label starts to the right of the midpoint (right side of CA).
        assert!(
            bx_label > mid_ca_x + 0.2,
            "b should sit to the right of CA's midpoint; \
             label origin x={bx_label}, midpoint x={mid_ca_x}"
        );
        // And its ink centre rides the midpoint's y — the ink spans
        // ~0.8 em above the baseline in SVG y-down, so centre = y-0.4.
        let ink_y = by_label - 0.4;
        assert!(
            (ink_y - mid_ca_y).abs() < 0.1,
            "b's ink centre should sit on CA's midpoint; \
             ink centre y={ink_y}, midpoint y={mid_ca_y}; \
             C=({cx},{cy}) A=({ax},{ay})"
        );
    }

    #[test]
    fn foreach_iterates_over_a_range() {
        let out = draw(
            r"\begin{tikzpicture}
\foreach \i in {1,2,...,4} {\draw (0,0) -- (\i,0);}
\end{tikzpicture}",
        )
        .expect("svg");
        // Coordinates are scaled cm → em, so \i = 1 → 1 · EM_PER_CM
        // and \i = 4 → 4 · EM_PER_CM.
        assert!(out.contains("L 2.635"), "first: {out}");
        assert!(out.contains("L 9.721"), "last: {out}");
    }

    #[test]
    fn empty_tikz_falls_back() {
        assert!(draw(r"\begin{tikzpicture}\end{tikzpicture}").is_none());
    }
}
