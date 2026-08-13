//! A tiny, dependency-free LaTeX **drawing** → inline SVG renderer.
//!
//! Companion to [`crate::tex`], which typesets formulas. This module draws the
//! three LaTeX environments that are graphics rather than math:
//!
//! - `\begin{picture}` — `\put`, `\line`, `\vector`, `\circle`, `\circle*`,
//!   `\multiput`, `\framebox`, `\unitlength`;
//! - `\xymatrix` — xy-pic commutative diagrams: grid cells (`&` column,
//!   `\\` row) joined by `\ar[r]^f` / `\ar[d]_g` arrows;
//! - `\begin{tikzpicture}` — a pragmatic TikZ subset: `\draw`, `\fill`,
//!   `\filldraw`, `\path`, `\node`, `\coordinate`, `\def`, `\foreach`;
//!   path operations `--`, `-|`, `|-`, `.. controls … and … ..`, `rectangle`,
//!   `circle`, `arc`, `grid`, `plot`, `cycle`; coordinates `(x,y)`, `(name)`,
//!   `(30:1.5)` polar, `+(…)`, `++(…)`, `($(A)!t!(B)$)`; and the common
//!   option keys (`->`, `>=stealth`, `thin`, `line width=1.2pt`, `dashed`,
//!   `color=blue`, `fill=red!20`, `domain=0:2`, `samples=`, `pos=`,
//!   `above`/`below left`/`anchor=`, `inner sep=`).
//!
//! Conventions follow [`crate::tex`], not [`crate::diagram`]: every length is
//! in `em` of the surrounding text, strokes default to `currentColor` (so a
//! drawing inherits the page's text colour, dark themes included), and labels
//! are typeset by the math engine through [`crate::tex::typeset`] so they
//! share the site's math font stack.
//!
//! Anything the parser does not understand is skipped rather than fatal, so a
//! drawing with one unsupported construct still renders. [`render`] returns
//! `None` only when nothing at all was drawn, and the caller
//! ([`crate::markdown`]) then falls back to a code block or to the math
//! renderer.

use crate::tex::Typeset;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Whether `src` looks like a drawing environment rather than a formula.
/// Cheap enough to run on every `$$…$$` block before the math renderer.
pub fn is_drawing(src: &str) -> bool {
    let s = src.trim_start();
    s.contains("\\begin{picture}") || s.contains("\\begin{tikzpicture}") || s.contains("\\xymatrix")
}

/// Render a LaTeX drawing environment to inline SVG. `None` when the source
/// is not a drawing, or when it produced no ink at all.
pub fn render(src: &str) -> Option<String> {
    let s = src.trim();
    let (canvas, kind) = if s.contains("\\begin{picture}") {
        (picture::render(s)?, Kind::Picture)
    } else if s.contains("\\begin{tikzpicture}") {
        (tikz::render(s)?, Kind::Tikz)
    } else if s.contains("\\xymatrix") {
        (xypic::render(s)?, Kind::XyPic)
    } else {
        return None;
    };
    canvas.to_svg(kind, s)
}

/// Which front end produced a canvas — only used for the accessible label.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Picture,
    XyPic,
    Tikz,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Picture => "LaTeX picture",
            Kind::XyPic => "commutative diagram",
            Kind::Tikz => "TikZ drawing",
        }
    }
}

// ---------------------------------------------------------------------------
// Units
// ---------------------------------------------------------------------------

/// 1 cm of TikZ user space in `em`. At the browser default of 16 px per em,
/// 1 cm = 96/2.54 px ≈ 37.8 px ≈ 2.36 em.
pub(crate) const EM_PER_CM: f64 = 2.3622;
/// 1 TeX point in `em` (1 em = 12 pt at the default text size).
pub(crate) const EM_PER_PT: f64 = 1.0 / 12.0;
/// 1 TeX point of `\unitlength` in `em` for the `picture` environment.
///
/// LaTeX sets `picture` beside 10 pt body text, where 1 em = 10 pt, so a
/// `\unitlength=1pt` figure must render at 0.1 em — not the 1/12 em that
/// TikZ's centimetre mapping assumes at the browser default of 16 px/em.
/// Without this every line in a `picture` comes out ~20 % shorter than the
/// same figure in a LaTeX document.
pub(crate) const PIC_EM_PER_PT: f64 = 0.1;
/// The `picture` backend's cm → em factor on its 10 pt basis: 1 cm =
/// 2835/100 TeX points (72.27 pt/in), so `\unitlength=1cm` stays in step
/// with `\unitlength=1pt`, and `\unitlength=1em` round-trips exactly.
pub(crate) const PIC_EM_PER_CM: f64 = 28.45274 * PIC_EM_PER_PT;
/// One `\xymatrix` grid step in `em`. Kept generous relative to a single
/// entry's width so the **arrows** between entries are visibly longer than
/// the node labels — xy-pic diagrams read badly when the connecting lines are
/// shorter than the symbols they join. Entry font size and stroke widths are
/// untouched; only the spacing grows.
pub(crate) const EM_PER_XY_CELL: f64 = 4.0;
/// Blank margin left around the drawing's bounding box.
const PAD: f64 = 0.25;
/// Default gap between a label and the point it is attached to.
pub(crate) const LABEL_SEP: f64 = 0.12;
/// Default `inner sep` for a TikZ node label, in `em`. 0.3333 em is TikZ's
/// own default padding; it is the gap wedged between the label's ink and
/// the anchor point by [`Anchor::origin_offset`].
pub(crate) const DEFAULT_INNER_SEP: f64 = 0.333;

/// Stroke width in `em` for a TikZ line width given in points.
///
/// TikZ's default `thin` is 0.4 pt, which at 16 px text would be a 0.53 px
/// hairline — noticeably lighter than the ~1 px stems of the surrounding
/// text. Widths are scaled by 1.35 so a default line sits at 0.045 em and
/// optically matches [`crate::tex`]'s glyph stroke weight.
pub(crate) fn width_from_pt(pt: f64) -> f64 {
    (pt * EM_PER_PT * 1.35).max(0.015)
}

// ---------------------------------------------------------------------------
// Canvas data model — device independent, `em` units, y pointing up
// ---------------------------------------------------------------------------

pub(crate) type Pt = (f64, f64);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dash {
    Solid,
    Dashed,
    DenselyDashed,
    LooselyDashed,
    Dotted,
    DashDot,
}

impl Dash {
    fn array(self) -> Option<&'static str> {
        match self {
            Dash::Solid => None,
            Dash::Dashed => Some("0.25 0.15"),
            Dash::DenselyDashed => Some("0.2 0.08"),
            Dash::LooselyDashed => Some("0.3 0.26"),
            Dash::Dotted => Some("0.02 0.12"),
            Dash::DashDot => Some("0.25 0.12 0.02 0.12"),
        }
    }
}

/// A colour: `Current` follows the page text colour, which is what makes a
/// drawing readable in both light and dark themes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Color {
    Current,
    Rgb(u8, u8, u8),
}

impl Color {
    /// Resolve an xcolor name, optionally with a `name!pct` mix towards white.
    pub(crate) fn named(name: &str) -> Option<Color> {
        let (base, pct) = match name.split_once('!') {
            Some((b, p)) => (b.trim(), p.trim().parse::<f64>().ok()),
            None => (name.trim(), None),
        };
        let (r, g, b) = match base.to_ascii_lowercase().as_str() {
            "black" => (0, 0, 0),
            "white" => (255, 255, 255),
            "red" => (255, 0, 0),
            "green" => (0, 128, 0),
            "blue" => (0, 0, 255),
            "cyan" => (0, 255, 255),
            "magenta" => (255, 0, 255),
            "yellow" => (255, 255, 0),
            "orange" => (255, 128, 0),
            "purple" => (191, 0, 191),
            "violet" => (128, 0, 128),
            "brown" => (150, 75, 0),
            "olive" => (128, 128, 0),
            "teal" => (0, 128, 128),
            "lime" => (191, 255, 0),
            "pink" => (255, 191, 191),
            "gray" | "grey" => (128, 128, 128),
            "darkgray" | "darkgrey" => (64, 64, 64),
            "lightgray" | "lightgrey" => (191, 191, 191),
            _ => return None,
        };
        match pct {
            // `red!20` is 20 % red mixed into white. xcolor mixes in its own
            // colour space; a linear RGB blend is close enough for a page
            // background wash and keeps the module dependency-free.
            Some(p) => {
                let t = (p / 100.0).clamp(0.0, 1.0);
                let mix = |c: u8| (c as f64 * t + 255.0 * (1.0 - t)).round() as u8;
                Some(Color::Rgb(mix(r), mix(g), mix(b)))
            }
            None => Some(Color::Rgb(r, g, b)),
        }
    }

    fn attr(self) -> String {
        match self {
            Color::Current => "currentColor".to_string(),
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Stroke {
    pub(crate) color: Color,
    pub(crate) width: f64,
    pub(crate) dash: Dash,
}

impl Default for Stroke {
    fn default() -> Stroke {
        Stroke {
            color: Color::Current,
            width: width_from_pt(0.4),
            dash: Dash::Solid,
        }
    }
}

/// Arrow tip shape. `To` is TeX's thin `\to` tip, `Stealth` the filled dart.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Stealth / Latex are reserved for future arrow style parsing.
pub(crate) enum ArrowKind {
    To,
    Stealth,
    Latex,
}

#[derive(Clone, Copy)]
pub(crate) struct Arrow {
    pub(crate) start: bool,
    pub(crate) end: bool,
    pub(crate) kind: ArrowKind,
    pub(crate) scale: f64,
}

impl Default for Arrow {
    fn default() -> Arrow {
        Arrow {
            start: false,
            end: false,
            kind: ArrowKind::To,
            scale: 1.0,
        }
    }
}

impl Arrow {
    fn any(&self) -> bool {
        self.start || self.end
    }

    /// (length, half-width) of the tip in `em`.
    fn size(&self) -> (f64, f64) {
        let (l, hw) = match self.kind {
            ArrowKind::To => (0.24, 0.075),
            ArrowKind::Stealth => (0.26, 0.1),
            ArrowKind::Latex => (0.22, 0.11),
        };
        (l * self.scale, hw * self.scale)
    }
}

/// Which point of a label box is pinned to its anchor coordinate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Anchor {
    Center,
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
    Base,
    BaseWest,
    BaseEast,
}

impl Anchor {
    /// Offset from the anchor point to the label's baseline origin (its
    /// left edge on the baseline), in canvas coordinates (y up). The
    /// label's bbox then covers `[dx..dx+w] × [dy-d..dy+h]`, so a positive
    /// `dy` puts the glyph above the anchor (North), negative puts it
    /// below (South); positive `dx` puts the glyph to the right (East).
    ///
    /// `ink` is the label's ink bounding box with the baseline at y = 0
    /// (see [`crate::tex::Typeset::ink_bounds`]). Directional anchors pin
    /// an **ink** edge `gap` from the anchor point — `gap` is TikZ's
    /// `inner sep` — and centre the label optically on the perpendicular
    /// axis, on the visible ink rather than the full em box. Centering on
    /// the ink is what keeps a `node[right]` label sitting on a slanted
    /// side's midpoint instead of floating a half-descent higher; pinning
    /// the ink with a gap is what keeps an `above` label clear of the line
    /// it is attached to. `Center` and `Base` anchors ignore `gap`.
    ///
    /// Direction names match screen orientation: `North` is up on the
    /// page, `South` is down, `East` is right, `West` is left.
    fn origin_offset(self, w: f64, ink: (f64, f64, f64, f64), gap: f64) -> Pt {
        let (_ix0, iy0, _ix1, iy1) = ink;
        // Vertical centre of the visible ink (baseline at y = 0). For a
        // capital letter that is ~0.4 em above the baseline while the em
        // box's own centre is only ~(h-d)/2 — so aligning the box centre
        // with a point puts the glyph visibly above it.
        let icy = (iy0 + iy1) / 2.0;
        let (dx, dy) = match self {
            Anchor::Center => (-w / 2.0, -icy),
            Anchor::North => (-w / 2.0, gap - iy0),
            Anchor::South => (-w / 2.0, -gap - iy1),
            Anchor::East => (gap, -icy),
            Anchor::West => (-w - gap, -icy),
            Anchor::NorthEast => (gap, gap - iy0),
            Anchor::NorthWest => (-w - gap, gap - iy0),
            Anchor::SouthEast => (gap, -gap - iy1),
            Anchor::SouthWest => (-w - gap, -gap - iy1),
            Anchor::Base => (-w / 2.0, 0.0),
            Anchor::BaseWest => (0.0, 0.0),
            Anchor::BaseEast => (-w, 0.0),
        };
        (dx, dy)
    }
}

/// Resolve a label's baseline origin offset from its anchor, gap and ink.
pub(crate) fn label_offset(anchor: Anchor, ts: &Typeset, gap: f64) -> Pt {
    let ink = ts.ink_bounds().unwrap_or((0.0, -ts.d, ts.w, ts.h));
    anchor.origin_offset(ts.w, ink, gap)
}

#[derive(Clone)]
pub(crate) enum PathOp {
    Move(Pt),
    Line(Pt),
    Bezier { c1: Pt, c2: Pt, to: Pt },
    Close,
}

#[derive(Clone)]
pub(crate) enum Item {
    Path {
        ops: Vec<PathOp>,
        stroke: Option<Stroke>,
        fill: Option<Color>,
        arrow: Arrow,
    },
    Circle {
        c: Pt,
        r: f64,
        stroke: Option<Stroke>,
        fill: Option<Color>,
    },
    /// Elliptical arc from `a0` to `a1` (degrees, counter-clockwise).
    Arc {
        c: Pt,
        rx: f64,
        ry: f64,
        a0: f64,
        a1: f64,
        stroke: Stroke,
    },
    Grid {
        p0: Pt,
        p1: Pt,
        step: Pt,
        stroke: Stroke,
    },
    Label {
        at: Pt,
        anchor: Anchor,
        ts: Typeset,
        /// Separation between the label's ink and the anchor point, in
        /// `em`. Only directional anchors apply it; `Center`/`Base`
        /// anchors ignore it (their callers pre-offset the anchor).
        gap: f64,
        /// Ink colour. `Current` — the default — lets the label follow the
        /// page text colour like every other stroke here; a TikZ
        /// `color=blue` on the owning path sets it explicitly.
        color: Color,
    },
}

#[derive(Default, Clone)]
pub(crate) struct Canvas {
    pub(crate) items: Vec<Item>,
}

// ---------------------------------------------------------------------------
// Bounding box
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct BBox {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl BBox {
    fn new() -> BBox {
        BBox {
            x0: f64::INFINITY,
            y0: f64::INFINITY,
            x1: f64::NEG_INFINITY,
            y1: f64::NEG_INFINITY,
        }
    }

    fn is_empty(&self) -> bool {
        self.x0 > self.x1 || self.y0 > self.y1
    }

    fn add(&mut self, p: Pt) {
        self.x0 = self.x0.min(p.0);
        self.y0 = self.y0.min(p.1);
        self.x1 = self.x1.max(p.0);
        self.y1 = self.y1.max(p.1);
    }

    fn add_disc(&mut self, c: Pt, rx: f64, ry: f64) {
        self.add((c.0 - rx, c.1 - ry));
        self.add((c.0 + rx, c.1 + ry));
    }

    fn grow(&mut self, m: f64) {
        if !self.is_empty() {
            self.x0 -= m;
            self.y0 -= m;
            self.x1 += m;
            self.y1 += m;
        }
    }
}

impl Canvas {
    pub(crate) fn push(&mut self, item: Item) {
        self.items.push(item);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn bbox(&self) -> BBox {
        let mut bb = BBox::new();
        for item in &self.items {
            match item {
                Item::Path {
                    ops, stroke, arrow, ..
                } => {
                    let mut sub = BBox::new();
                    for op in ops {
                        match op {
                            PathOp::Move(p) | PathOp::Line(p) => sub.add(*p),
                            PathOp::Bezier { c1, c2, to } => {
                                // Control points bound the curve; a tight hull
                                // is not worth the arithmetic here.
                                sub.add(*c1);
                                sub.add(*c2);
                                sub.add(*to);
                            }
                            PathOp::Close => {}
                        }
                    }
                    if !sub.is_empty() {
                        let half = stroke.map(|s| s.width / 2.0).unwrap_or(0.0);
                        let tip = if arrow.any() { arrow.size().0 } else { 0.0 };
                        sub.grow(half + tip);
                        bb.add((sub.x0, sub.y0));
                        bb.add((sub.x1, sub.y1));
                    }
                }
                Item::Circle { c, r, stroke, .. } => {
                    let half = stroke.map(|s| s.width / 2.0).unwrap_or(0.0);
                    bb.add_disc(*c, r + half, r + half);
                }
                Item::Arc {
                    c, rx, ry, stroke, ..
                } => {
                    // Bounding the full ellipse over-estimates a short arc,
                    // but never clips it.
                    bb.add_disc(*c, rx + stroke.width / 2.0, ry + stroke.width / 2.0);
                }
                Item::Grid { p0, p1, stroke, .. } => {
                    let half = stroke.width / 2.0;
                    bb.add((p0.0.min(p1.0) - half, p0.1.min(p1.1) - half));
                    bb.add((p0.0.max(p1.0) + half, p0.1.max(p1.1) + half));
                }
                Item::Label {
                    at,
                    anchor,
                    ts,
                    gap,
                    ..
                } => {
                    let (dx, dy) = label_offset(*anchor, ts, *gap);
                    let ox = at.0 + dx;
                    let oy = at.1 + dy;
                    // Hug the label's *actual ink* horizontally (the em box's
                    // advance width includes tex's RIGHT_MARGIN guard, which
                    // would leave phantom blank space at a drawing's edge).
                    // Vertically keep the full em box so descenders are never
                    // clipped.
                    let (x0, x1) = match ts.ink_bounds() {
                        Some((ix0, _, ix1, _)) => (ox + ix0, ox + ix1),
                        None => (ox, ox + ts.w),
                    };
                    bb.add((x0, oy - ts.d));
                    bb.add((x1, oy + ts.h));
                }
            }
        }
        bb
    }
}

// ---------------------------------------------------------------------------
// SVG emission
// ---------------------------------------------------------------------------

impl Canvas {
    /// Convert to a standalone `<svg>`. `None` when the canvas has no ink.
    pub(crate) fn to_svg(&self, kind: Kind, source: &str) -> Option<String> {
        if self.items.is_empty() {
            return None;
        }
        let mut bb = self.bbox();
        if bb.is_empty() {
            return None;
        }
        bb.grow(PAD);
        let w = bb.x1 - bb.x0;
        let h = bb.y1 - bb.y0;
        if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
            return None;
        }
        // Canvas y points up; SVG's points down.
        let tx = |x: f64| x - bb.x0;
        let ty = |y: f64| bb.y1 - y;

        let mut parts = String::new();
        for item in &self.items {
            match item {
                Item::Path {
                    ops,
                    stroke,
                    fill,
                    arrow,
                } => {
                    let mut d = String::new();
                    for op in ops {
                        match op {
                            PathOp::Move(p) => {
                                d.push_str(&format!("M {} {}", fmt(tx(p.0)), fmt(ty(p.1))));
                            }
                            PathOp::Line(p) => {
                                d.push_str(&format!(" L {} {}", fmt(tx(p.0)), fmt(ty(p.1))));
                            }
                            PathOp::Bezier { c1, c2, to } => {
                                d.push_str(&format!(
                                    " C {} {} {} {} {} {}",
                                    fmt(tx(c1.0)),
                                    fmt(ty(c1.1)),
                                    fmt(tx(c2.0)),
                                    fmt(ty(c2.1)),
                                    fmt(tx(to.0)),
                                    fmt(ty(to.1))
                                ));
                            }
                            PathOp::Close => d.push_str(" Z"),
                        }
                    }
                    if d.is_empty() {
                        continue;
                    }
                    parts.push_str(&format!(
                        "<path d=\"{d}\" fill=\"{}\"{}/>",
                        fill.map(|c| c.attr()).unwrap_or_else(|| "none".into()),
                        stroke.map(stroke_attrs).unwrap_or_default()
                    ));
                    if arrow.any() {
                        let pts = path_points(ops);
                        if pts.len() >= 2 {
                            if arrow.end {
                                let n = pts.len();
                                parts.push_str(&arrow_head(
                                    pts[n - 2],
                                    pts[n - 1],
                                    arrow,
                                    stroke,
                                    &tx,
                                    &ty,
                                ));
                            }
                            if arrow.start {
                                parts
                                    .push_str(&arrow_head(pts[1], pts[0], arrow, stroke, &tx, &ty));
                            }
                        }
                    }
                }
                Item::Circle { c, r, stroke, fill } => {
                    parts.push_str(&format!(
                        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"{}/>",
                        fmt(tx(c.0)),
                        fmt(ty(c.1)),
                        fmt(*r),
                        fill.map(|c| c.attr()).unwrap_or_else(|| "none".into()),
                        stroke.map(stroke_attrs).unwrap_or_default()
                    ));
                }
                Item::Arc {
                    c,
                    rx,
                    ry,
                    a0,
                    a1,
                    stroke,
                } => {
                    let p0 = (
                        c.0 + rx * a0.to_radians().cos(),
                        c.1 + ry * a0.to_radians().sin(),
                    );
                    let p1 = (
                        c.0 + rx * a1.to_radians().cos(),
                        c.1 + ry * a1.to_radians().sin(),
                    );
                    let large = if (a1 - a0).abs() > 180.0 { 1 } else { 0 };
                    // Canvas angles run counter-clockwise; the y flip turns
                    // that into a clockwise sweep in SVG space.
                    let sweep = if a1 > a0 { 0 } else { 1 };
                    parts.push_str(&format!(
                        "<path d=\"M {} {} A {} {} 0 {large} {sweep} {} {}\" fill=\"none\"{}/>",
                        fmt(tx(p0.0)),
                        fmt(ty(p0.1)),
                        fmt(*rx),
                        fmt(*ry),
                        fmt(tx(p1.0)),
                        fmt(ty(p1.1)),
                        stroke_attrs(*stroke)
                    ));
                }
                Item::Grid {
                    p0,
                    p1,
                    step,
                    stroke,
                } => {
                    let (x0, x1) = (p0.0.min(p1.0), p0.0.max(p1.0));
                    let (y0, y1) = (p0.1.min(p1.1), p0.1.max(p1.1));
                    let mut d = String::new();
                    for x in axis_ticks(x0, x1, step.0) {
                        d.push_str(&format!(
                            "M {} {} L {} {}",
                            fmt(tx(x)),
                            fmt(ty(y0)),
                            fmt(tx(x)),
                            fmt(ty(y1))
                        ));
                    }
                    for y in axis_ticks(y0, y1, step.1) {
                        d.push_str(&format!(
                            "M {} {} L {} {}",
                            fmt(tx(x0)),
                            fmt(ty(y)),
                            fmt(tx(x1)),
                            fmt(ty(y))
                        ));
                    }
                    if !d.is_empty() {
                        parts.push_str(&format!(
                            "<path d=\"{d}\" fill=\"none\"{}/>",
                            stroke_attrs(*stroke)
                        ));
                    }
                }
                Item::Label {
                    at,
                    anchor,
                    ts,
                    gap,
                    color,
                } => {
                    let (dx, dy) = label_offset(*anchor, ts, *gap);
                    let ink = ts.emit_at(tx(at.0 + dx), ty(at.1 + dy));
                    match color {
                        Color::Current => parts.push_str(&ink),
                        // `color` re-points every `currentColor` inside the
                        // typeset ink (the stroked glyph paths); `fill`
                        // catches the plain `<text>` and `<rect>` runs, which
                        // carry no paint of their own.
                        c => parts.push_str(&format!(
                            "<g color=\"{c}\" fill=\"{c}\">{ink}</g>",
                            c = c.attr()
                        )),
                    }
                }
            }
        }

        let label = kind.label();
        Some(format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" aria-label=\"{label}\" \
viewBox=\"0 0 {w} {h}\" width=\"{w}em\" height=\"{h}em\" \
font-family=\"STIX Two Math, Latin Modern Math, Cambria Math, Noto Sans Math, \
MathJax_Main, Georgia, serif\"><title>{label}</title><desc>{src}</desc>{parts}</svg>",
            w = fmt(w),
            h = fmt(h),
            src = escape(source)
        ))
    }
}

fn stroke_attrs(s: Stroke) -> String {
    let dash = match s.dash.array() {
        Some(a) => format!(" stroke-dasharray=\"{a}\""),
        None => String::new(),
    };
    format!(
        " stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{dash}",
        s.color.attr(),
        fmt(s.width)
    )
}

/// The on-path vertices, used to aim arrow tips.
fn path_points(ops: &[PathOp]) -> Vec<Pt> {
    ops.iter()
        .filter_map(|op| match op {
            PathOp::Move(p) | PathOp::Line(p) => Some(*p),
            PathOp::Bezier { to, .. } => Some(*to),
            PathOp::Close => None,
        })
        .collect()
}

/// A filled tip at `to`, pointing along `from → to`.
fn arrow_head(
    from: Pt,
    to: Pt,
    arrow: &Arrow,
    stroke: &Option<Stroke>,
    tx: &dyn Fn(f64) -> f64,
    ty: &dyn Fn(f64) -> f64,
) -> String {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if dx == 0.0 && dy == 0.0 {
        return String::new();
    }
    let ang = dy.atan2(dx);
    let (len, half) = arrow.size();
    let base = (to.0 - len * ang.cos(), to.1 - len * ang.sin());
    let (nx, ny) = (-ang.sin(), ang.cos());
    let p1 = (base.0 + half * nx, base.1 + half * ny);
    let p2 = (base.0 - half * nx, base.1 - half * ny);
    let color = stroke.map(|s| s.color).unwrap_or(Color::Current);
    match arrow.kind {
        // `stealth` is notched at the back, which reads as a sharper dart.
        ArrowKind::Stealth => {
            let notch = (to.0 - len * 0.55 * ang.cos(), to.1 - len * 0.55 * ang.sin());
            format!(
                "<path d=\"M {} {} L {} {} L {} {} L {} {} Z\" fill=\"{}\"/>",
                fmt(tx(to.0)),
                fmt(ty(to.1)),
                fmt(tx(p1.0)),
                fmt(ty(p1.1)),
                fmt(tx(notch.0)),
                fmt(ty(notch.1)),
                fmt(tx(p2.0)),
                fmt(ty(p2.1)),
                color.attr()
            )
        }
        _ => format!(
            "<path d=\"M {} {} L {} {} L {} {} Z\" fill=\"{}\"/>",
            fmt(tx(to.0)),
            fmt(ty(to.1)),
            fmt(tx(p1.0)),
            fmt(ty(p1.1)),
            fmt(tx(p2.0)),
            fmt(ty(p2.1)),
            color.attr()
        ),
    }
}

/// Grid line positions from `lo` to `hi`, aligned on multiples of `step`.
fn axis_ticks(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    let mut out = Vec::new();
    if !(step.is_finite() && step > 0.0) || !lo.is_finite() || !hi.is_finite() {
        return out;
    }
    let n = ((hi - lo) / step).floor() as i64;
    // A pathological `step` must not spin the emitter; 400 lines is already
    // far past what reads on a page.
    if n < 0 || n > 400 {
        return out;
    }
    let first = (lo / step).ceil() * step;
    let mut v = first;
    while v <= hi + 1e-9 && out.len() <= 400 {
        out.push(v);
        v += step;
    }
    out
}

/// Round to at most 3 decimals, dropping trailing zeros.
pub(crate) fn fmt(x: f64) -> String {
    let v = (x * 1000.0).round() / 1000.0;
    if v == 0.0 {
        return "0".to_string();
    }
    let s = format!("{v:.3}");
    let t = s.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() {
        "0".to_string()
    } else {
        t.to_string()
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// Label typesetting
// ---------------------------------------------------------------------------

/// Typeset a label body: `$…$` runs go through the math engine, bare text is
/// set upright via `\text{…}`. Returns `None` for an empty body.
pub(crate) fn label(body: &str) -> Option<Typeset> {
    let b = body.trim();
    if b.is_empty() {
        return None;
    }
    // A body that is entirely one math span is the common case (`{$c$}`).
    let src = if b.starts_with('$') && b.ends_with('$') && b.len() > 2 {
        lift_primes(&b[1..b.len() - 1])
    } else if b.contains('$') {
        // Mixed text and math: splice the text runs into \text{…} groups so
        // the whole label is laid out by one pass of the math engine. Math
        // segments get prime-lifting; text segments stay literal so a `'`
        // inside `\text{it's}` doesn't accidentally become `^{\prime}`.
        let mut out = String::new();
        let mut math = false;
        for seg in b.split('$') {
            if math {
                out.push_str(&lift_primes(seg));
            } else if !seg.trim().is_empty() {
                out.push_str(&format!("\\text{{{seg}}}"));
            }
            math = !math;
        }
        out
    } else if b.starts_with('\\') {
        lift_primes(b)
    } else {
        format!("\\text{{{b}}}")
    };
    let ts = crate::tex::typeset(&src, false);
    if ts.is_empty() {
        None
    } else {
        Some(ts)
    }
}

/// Typeset a label body as a math expression. Used by xy-pic, which puts
/// its labels in TeX math mode by default — so `^f` becomes italic 𝑓 rather
/// than the upright `\text{f}` that `label` would produce. The body's own
/// `$…$` runs are still honoured; any remaining text runs are wrapped in
/// `\mathrm{…}` so they stay upright.
pub(crate) fn math_label(body: &str) -> Option<Typeset> {
    let b = body.trim();
    if b.is_empty() {
        return None;
    }
    let src = if b.starts_with('$') && b.ends_with('$') && b.len() > 2 {
        lift_primes(&b[1..b.len() - 1])
    } else if b.contains('$') {
        let mut out = String::new();
        let mut math = false;
        for seg in b.split('$') {
            if math {
                out.push_str(&lift_primes(seg));
            } else if !seg.trim().is_empty() {
                out.push_str(&format!("\\mathrm{{{seg}}}"));
            }
            math = !math;
        }
        out
    } else {
        lift_primes(b)
    };
    let ts = crate::tex::typeset(&src, false);
    if ts.is_empty() {
        None
    } else {
        Some(ts)
    }
}

/// Keep `f'` and `f''` close to the letter — but with a thin space so the
/// apostrophe doesn't kiss the preceding glyph. We do **not** promote the
/// prime to a superscript (`f^{\prime}`), which sits too high and reads as
/// a separate symbol; the math convention in this engine is to leave the
/// prime at the baseline next to its base.
fn lift_primes(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' {
            // Add a thin space before the prime so it doesn't overlap.
            out.push_str("\\,");
            let start = i;
            while i < chars.len() && chars[i] == '\'' {
                out.push(chars[i]);
                i += 1;
            }
            let _ = start;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Front ends (filled in by the following sections)
// ---------------------------------------------------------------------------

mod expr;
mod picture;
mod scan;
mod tikz;
mod xypic;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_source_is_not_a_drawing() {
        assert!(!is_drawing(""));
        assert!(render("").is_none());
        assert!(render("x^2 + 1").is_none());
    }

    #[test]
    fn sniffs_the_three_environments() {
        assert!(is_drawing("\\begin{picture}(1,1)\\end{picture}"));
        assert!(is_drawing(
            "\\begin{tikzpicture}\\draw (0,0)--(1,1);\\end{tikzpicture}"
        ));
        assert!(is_drawing("\\xymatrix{A & B}"));
        assert!(!is_drawing("\\begin{pmatrix} 1 & 2 \\end{pmatrix}"));
    }

    #[test]
    fn viewbox_holds_all_ink_with_padding() {
        let mut c = Canvas::default();
        c.push(Item::Path {
            ops: vec![PathOp::Move((0.0, 0.0)), PathOp::Line((2.0, 1.0))],
            stroke: Some(Stroke::default()),
            fill: None,
            arrow: Arrow::default(),
        });
        let out = c.to_svg(Kind::Tikz, "src").expect("svg");
        // 2 wide + stroke + 2 * PAD; tikz is *not* enlarged (only xy-pic is).
        assert!(
            out.contains("viewBox=\"0 0 2.545 1.545\""),
            "bbox grows by stroke and padding: {out}"
        );
        assert!(out.contains("width=\"2.545em\""), "em width: {out}");
    }

    #[test]
    fn dashed_stroke_emits_dasharray() {
        let mut c = Canvas::default();
        c.push(Item::Path {
            ops: vec![PathOp::Move((0.0, 0.0)), PathOp::Line((1.0, 0.0))],
            stroke: Some(Stroke {
                dash: Dash::Dashed,
                ..Stroke::default()
            }),
            fill: None,
            arrow: Arrow::default(),
        });
        let out = c.to_svg(Kind::Tikz, "src").expect("svg");
        assert!(out.contains("stroke-dasharray=\"0.25 0.15\""), "{out}");
    }

    #[test]
    fn color_mix_blends_towards_white() {
        assert_eq!(Color::named("red"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(Color::named("red!20"), Some(Color::Rgb(255, 204, 204)));
        assert_eq!(Color::named("nosuchcolor"), None);
    }

    #[test]
    fn arrow_tip_sits_at_the_path_end() {
        let mut c = Canvas::default();
        c.push(Item::Path {
            ops: vec![PathOp::Move((0.0, 0.0)), PathOp::Line((1.0, 0.0))],
            stroke: Some(Stroke::default()),
            fill: None,
            arrow: Arrow {
                end: true,
                ..Arrow::default()
            },
        });
        let out = c.to_svg(Kind::Tikz, "src").expect("svg");
        // Path end is at canvas x = 1; the tip triangle starts there
        // (along whatever y the bbox settled to).
        assert!(out.contains("L 1.513 0.513"), "tip at the endpoint: {out}");
        assert!(out.contains("fill=\"currentColor\"/>"), "{out}");
    }

    #[test]
    fn grid_ticks_align_on_multiples_of_step() {
        assert_eq!(axis_ticks(-0.1, 2.1, 1.0), vec![0.0, 1.0, 2.0]);
        assert!(axis_ticks(0.0, 1.0, 0.0).is_empty());
        assert!(axis_ticks(0.0, 1e9, 1e-9).is_empty(), "no runaway grids");
    }

    #[test]
    fn label_wraps_bare_text_but_not_math() {
        assert!(label("").is_none());
        let math = label("$\\alpha$").expect("math label");
        let text = label("A").expect("text label");
        assert!(math.w > 0.0 && text.w > 0.0);
    }

    #[test]
    fn debug_label_metrics() {
        for body in ["$A$", "$B$", "$C$", "$c$", "$\\alpha$", "$b$"] {
            let ts = label(body).expect("label");
            let svg = ts.emit_at(0.0, ts.h);
            let text_tags: Vec<&str> = svg.split("<text ").skip(1).collect();
            let mut ink: Vec<(f64, f64)> = Vec::new();
            for tag in text_tags {
                let x = tag
                    .split("x=\"")
                    .nth(1)
                    .and_then(|s| s.split('"').next())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(f64::NAN);
                let y = tag
                    .split("y=\"")
                    .nth(1)
                    .and_then(|s| s.split('"').next())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(f64::NAN);
                ink.push((x, ts.h - y));
            }
            let x0 = ink.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
            let x1 = ink.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
            let y0 = ink.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
            let y1 = ink.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
            eprintln!(
                "label {body}: w={} h={} d={} ink x[{},{}] y[{},{}]",
                ts.w, ts.h, ts.d, x0, y0, x1, y1
            );
        }
    }

    #[test]
    fn debug_canvas_items() {
        let src = r"\begin{tikzpicture}\small
\def\r{1.8}
\coordinate[label=$A$] (A) at (0.5*\r,0.8*\r);
\coordinate[label=below:$B$] (B) at (-\r,0);
\coordinate[label=below:$C$] (C) at (\r,0);
\draw[thin] (A) -- node[above] {$c$}
   node[pos=0.03,below,inner sep=4] {$\alpha$}
   (B) -- (C) -- node[right] {$b$} (A);
\end{tikzpicture}";
        let c = crate::drawing::tikz::render(src).expect("canvas");
        let mut path: Vec<(f64, f64)> = Vec::new();
        for it in &c.items {
            match it {
                Item::Label {
                    at,
                    anchor,
                    ts,
                    gap,
                    ..
                } => {
                    let (dx, dy) = label_offset(*anchor, ts, *gap);
                    eprintln!(
                        "LABEL at=({},{}) anchor={:?} gap={} w={} h={} d={} -> x0={} y0={} x1={} y1={}",
                        at.0,
                        at.1,
                        anchor,
                        gap,
                        ts.w,
                        ts.h,
                        ts.d,
                        at.0 + dx,
                        at.1 + dy - ts.d,
                        at.0 + dx + ts.w,
                        at.1 + dy + ts.h
                    );
                }
                Item::Path { ops, .. } => {
                    for op in ops {
                        if let PathOp::Move(p) | PathOp::Line(p) = op {
                            path.push(*p);
                        }
                    }
                }
                _ => {}
            }
        }
        eprintln!("PATH: {path:?}");
    }

    #[test]
    fn north_anchor_sits_above_the_point() {
        // Anchor::North places the label ink above the anchor (dy > 0) in
        // canvas y-up, horizontally centred, with `gap` clearing the
        // point.
        let (dx, dy) = Anchor::North.origin_offset(0.4, (0.0, 0.0, 0.5, 0.8), 0.3);
        assert!(dy > 0.0, "North dy > 0: {dy}");
        assert!(dx < 0.0, "North centres horizontally: {dx}");
    }

    #[test]
    fn south_anchor_sits_below_the_point() {
        let (_, dy) = Anchor::South.origin_offset(0.4, (0.0, 0.0, 0.5, 0.8), 0.3);
        assert!(dy < 0.0, "South dy < 0: {dy}");
    }

    #[test]
    fn east_anchor_sits_right_of_the_point() {
        let (dx, dy) = Anchor::East.origin_offset(0.4, (0.0, 0.0, 0.5, 0.8), 0.3);
        // `dx > 0` leaves `gap` between the point and the label.
        assert!(dx > 0.0, "East dx > 0: {dx}");
        assert!(dy < 0.0, "East centres on the ink: {dy}");
    }

    #[test]
    fn west_anchor_sits_left_of_the_point() {
        let (dx, dy) = Anchor::West.origin_offset(0.4, (0.0, 0.0, 0.5, 0.8), 0.3);
        // `dx ≤ -w-gap` puts the label entirely to the left of the point.
        assert!(dx <= -0.7, "West dx ≤ -w-gap: {dx}");
        assert!(dy < 0.0, "West centres on the ink: {dy}");
    }

    /// A directional anchor centres the label on the visible ink, not on
    /// the empty em box — otherwise a `right` label rides a half-descent
    /// above its point.
    #[test]
    fn ink_centering_keeps_labels_on_the_point() {
        let (_, dy_box) = Anchor::East.origin_offset(0.4, (0.0, -0.15, 0.5, 0.72), 0.0);
        let (_, dy_ink) = Anchor::East.origin_offset(0.4, (0.0, 0.0, 0.5, 0.8), 0.0);
        assert!(
            dy_ink < dy_box,
            "ink centering sits the baseline lower than box centering: \
             {dy_ink} vs {dy_box}"
        );
    }

    #[test]
    fn math_label_adds_thin_space_before_prime() {
        // The prime stays at the baseline (no superscript promotion),
        // but a thin space `\,` is inserted before it so it doesn't
        // kiss the preceding letter.
        let ts = math_label("g'").expect("prime");
        let svg = ts.emit_at(0.0, ts.h);
        // Two glyphs: the g, and the apostrophe.
        assert_eq!(svg.matches("<text").count(), 2, "g + ': {svg}");
        // The thin space widens the gap between g and ' vs `g'` literal.
        let spaced = ts.w;
        let bare_ts = math_label("g").expect("bare g");
        assert!(
            spaced > bare_ts.w,
            "spaced label is wider than bare g: {spaced} vs {}",
            bare_ts.w
        );
    }

    #[test]
    fn math_label_keeps_multiple_primes_at_baseline() {
        // Two and three primes stay at baseline, just with a leading
        // thin space — no superscript promotion.
        let ts2 = math_label("f''").expect("double prime");
        let svg2 = ts2.emit_at(0.0, ts2.h);
        assert_eq!(svg2.matches("<text").count(), 3, "f + 2': {svg2}");
        let ts3 = math_label("g'''").expect("triple prime");
        let svg3 = ts3.emit_at(0.0, ts3.h);
        assert_eq!(svg3.matches("<text").count(), 4, "g + 3': {svg3}");
    }

    #[test]
    fn label_lifts_primes_inside_math_segment_only() {
        // Math segments get prime spacing; text segments inside \text{…}
        // keep the literal apostrophe so `it's` doesn't become gibberish.
        let mixed = label("it's $f'$ end").expect("mixed");
        let svg = mixed.emit_at(0.0, mixed.h);
        // The whole label emits successfully without error.
        assert!(svg.contains("<text"), "has glyphs: {svg}");
    }
}
