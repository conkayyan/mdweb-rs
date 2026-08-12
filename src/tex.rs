//! A tiny, dependency-free LaTeX math → inline **SVG** renderer.
//!
//! [`crate::markdown`](crate::markdown) math spans and blocks are turned into
//! baseline-aligned `<svg>` elements with **no client-side JavaScript, no
//! third-party library, and no dependence on the browser's MathML engine**.
//!
//! Pipeline: tokenise → recursive-descent parse into a semantic AST (the
//! single source of truth) → TeX-style box layout (HBox row with atom-class
//! spacing, fractions, radicals, scripts, limits, matrices, stretchy
//! delimiters) → SVG emission. The same AST also produces Presentation
//! MathML, which is embedded in the SVG's `<desc>` for accessibility.
//!
//! Layout is expressed in `em` units of the surrounding text size, and every
//! `<text>`/`<rect>` is emitted with explicit `font-size`/coordinates, so the
//! character **size proportions** (script scaling 0.7 → 0.5, enlarged display
//! big operations, fraction bar geometry, stretchy delimiters) are decided by
//! this renderer and match TeX's own output rather than the reader's font.
//!
//! The parser understands a pragmatic TeX subset:
//!
//! - auto-italic identifiers and digit runs (`x`, `123`);
//! - operators and relations (`+ - = \times \cdot \leq …`);
//! - Greek letters and many common symbols (`\alpha … \sum \infty …`);
//! - delimiters with `\left … \right`;
//! - `\frac`, `\over`, `\sqrt`, `\sqrt[n]`, `\text{}`, `\operatorname{}`,
//!   `\overset`;
//! - accents and variants: `\vec \hat \bar \overline \dot \ddot \tilde
//!   \widehat \widetilde`, `\mathbb \mathcal \mathsf \mathtt \mathit
//!   \mathbf \mathfrak`;
//! - superscript/subscript with `^{…}` / `_{…}`, `\limits` / `\nolimits`;
//! - simple `\begin{matrix} … \end{matrix}` arrays (`&` column, `\\` row);
//! - named math functions (`\sin \log \lim …`).
//!
//! Anything the parser does not understand degrades gracefully to upright
//! literal source text rather than crashing.
//!
//! **Drawing environments** (`\begin{picture}`, `\xymatrix`,
//! `\begin{tikzpicture}`) are not handled here — they are dispatched to
//! [`crate::drawing`], which has its own canvas / path / label pipeline
//! and shares only the math label typesetter.

/// Whether a `$…$` span is mathematical: non-empty, no spaces hugging the
/// delimiters (so currency amounts like `$ 5$` are left as literal text).
pub fn is_math_span(inner: &str) -> bool {
    !inner.is_empty() && !inner.starts_with(' ') && !inner.ends_with(' ')
}

/// Convert inline LaTeX math (`$…$` content) into a baseline-aligned SVG.
pub fn render(src: &str) -> String {
    svg_doc(src, false)
}

/// Convert a display (block) formula (`$$…$$` content) into an SVG.
pub fn render_block(src: &str) -> String {
    svg_doc(src, true)
}

/// The layout records text START positions only, so a box width can
/// underestimate the actual right edge of glyphs (e.g. digits are ~0.55 em
/// wide each in math fonts but the layout uses 0.5). Add a small right margin
/// so glyphs at the trailing edge don't get clipped against the viewBox.
const RIGHT_MARGIN: f64 = 0.15;

/// A laid-out formula: metrics in `em` of the surrounding text, with the
/// baseline at y = 0 and y pointing **up**.
///
/// This is the seam [`crate::drawing`] uses to place math labels inside a
/// drawing: measure with [`typeset`], then paint with [`Typeset::emit_at`].
#[derive(Debug, Clone)]
pub(crate) struct Typeset {
    /// Advance width including [`RIGHT_MARGIN`].
    pub(crate) w: f64,
    /// Height above the baseline.
    pub(crate) h: f64,
    /// Depth below the baseline.
    pub(crate) d: f64,
    ink: Vec<Ink>,
}

impl Typeset {
    pub(crate) fn empty() -> Typeset {
        Typeset {
            w: 0.0,
            h: 0.0,
            d: 0.0,
            ink: Vec::new(),
        }
    }

    fn from_box(b: LBox) -> Typeset {
        Typeset {
            w: b.w + RIGHT_MARGIN,
            h: b.h,
            d: b.d,
            ink: b.ink,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.ink.is_empty()
    }

    /// Ink bounding box with the baseline at y = 0, y up. `None` when empty.
    pub(crate) fn ink_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut x0 = f64::INFINITY;
        let mut y0 = f64::INFINITY;
        let mut x1 = f64::NEG_INFINITY;
        let mut y1 = f64::NEG_INFINITY;
        for ink in &self.ink {
            let (ix0, iy0, ix1, iy1) = match ink {
                Ink::T { x, y, s, .. } => (*x, *y, *x + 0.5 * *s, *y + 0.8 * *s),
                Ink::R { x0, y0, x1, y1 } => (*x0, *y0, *x1, *y1),
                Ink::P { x, y, sf, .. } | Ink::F { x, y, sf, .. } => {
                    (*x, *y, *x + *sf, *y + *sf)
                }
            };
            x0 = x0.min(ix0);
            y0 = y0.min(iy0);
            x1 = x1.max(ix1);
            y1 = y1.max(iy1);
        }
        (x0.is_finite() && y0.is_finite()).then_some((x0, y0, x1, y1))
    }

    /// Emit the `<text>`/`<rect>`/`<path>` fragments (no `<svg>` wrapper) so
    /// that the formula's baseline origin lands at `(x_off, y_base)` in the
    /// caller's **y-down** coordinate system.
    pub(crate) fn emit_at(&self, x_off: f64, y_base: f64) -> String {
        let mut parts = String::new();
        for ink in &self.ink {
            match ink {
                Ink::T {
                    x,
                    y,
                    s,
                    t,
                    italic,
                    bold,
                } => {
                    parts.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" font-size=\"{}\"{} text-anchor=\"start\">{}</text>",
                    fmt(x_off + x),
                    fmt(y_base - y),
                    fmt(*s),
                    if *bold {
                        " font-weight=\"bold\""
                    } else if *italic {
                        " font-style=\"italic\""
                    } else {
                        ""
                    },
                    esc(t)
                ));
                }
                Ink::R { x0, y0, x1, y1 } => {
                    parts.push_str(&format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                        fmt(x_off + x0),
                        fmt(y_base - y1),
                        fmt(x1 - x0),
                        fmt(y1 - y0)
                    ));
                }
                Ink::P { x, y, sf, sw, d } => {
                    // Design paths are in up-positive space (baseline at
                    // y = 0); SVG's y axis points down, so the path is
                    // mirrored with a negative y scale. Stroke weight is
                    // uniform because both axes scale by |sf|.
                    parts.push_str(&format!(
                        "<path d=\"{d}\" transform=\"translate({x} {y}) scale({s} -{s})\" \
fill=\"none\" stroke=\"currentColor\" stroke-width=\"{w}\" stroke-linecap=\"round\" \
stroke-linejoin=\"round\"/>",
                        x = fmt(x_off + x),
                        y = fmt(y_base - *y),
                        s = fmt(*sf),
                        w = fmt(*sw / *sf)
                    ));
                }
                Ink::F { x, y, sf, d } => {
                    parts.push_str(&format!(
                        "<path d=\"{d}\" transform=\"translate({x} {y}) scale({s} -{s})\" \
fill=\"currentColor\"/>",
                        x = fmt(x_off + x),
                        y = fmt(y_base - *y),
                        s = fmt(*sf)
                    ));
                }
            }
        }
        parts
    }
}

/// Parse and lay out a formula. Empty input yields [`Typeset::empty`].
pub(crate) fn typeset(src: &str, display: bool) -> Typeset {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return Typeset::empty();
    }
    let mut p = TexParser::new(trimmed, display);
    let node = p.parse_row();
    Typeset::from_box(layout(&node, display))
}

fn svg_doc(src: &str, display: bool) -> String {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut p = TexParser::new(trimmed, display);
    let node = p.parse_row();
    let t = Typeset::from_box(layout(&node, display));
    let w = fmt(t.w);
    let total = fmt(t.h + t.d);
    let va = if display {
        String::new()
    } else {
        format!(" style=\"vertical-align:-{}em;overflow:visible\"", fmt(t.d))
    };
    let aria = esc_attr(trimmed);
    let parts = t.emit_at(0.0, t.h);
    let mml = to_mathml(&node, display);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" aria-label=\"{aria}\" \
viewBox=\"0 0 {w} {total}\" width=\"{w}em\" height=\"{total}em\"{va} \
font-family=\"STIX Two Math, Latin Modern Math, Cambria Math, Noto Sans Math, \
MathJax_Main, Georgia, serif\"><title>{aria}</title><desc>{mml}</desc>{parts}</svg>"
    )
}

/// Round to at most 3 decimals, dropping trailing zeros.
fn fmt(x: f64) -> String {
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

// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Ord,
    Op,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
}

#[derive(Debug, Clone)]
struct Sym {
    t: String,
    italic: bool,
    bold: bool,
    cls: Class,
    /// Sum-like big operators move their limits under/over in display style.
    movable: bool,
}

/// A diacritic drawn over its base (`\vec`, `\hat`, `\bar`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccentKind {
    Vec,
    Hat,
    Bar,
    Overline,
    Dot,
    Ddot,
    Tilde,
    Widehat,
    Widetilde,
}

#[derive(Debug, Clone)]
enum Node {
    Row(Vec<Node>),
    Frac(Box<Node>, Box<Node>),
    /// `\cfrac` — display-style fraction with extra clearance around the bar.
    CFrac(Box<Node>, Box<Node>),
    /// `\boxed{…}` — a rule box drawn around its content.
    Boxed(Box<Node>),
    Accent {
        base: Box<Node>,
        kind: AccentKind,
    },
    Sqrt {
        rad: Box<Node>,
        deg: Option<Box<Node>>,
    },
    Script {
        base: Box<Node>,
        sub: Option<Box<Node>>,
        sup: Option<Box<Node>>,
    },
    OverUnder {
        base: Box<Node>,
        below: Option<Box<Node>>,
        above: Option<Box<Node>>,
    },
    Sym(Sym),
    Fence {
        open: char,
        inner: Box<Node>,
        close: char,
    },
    Matrix {
        rows: Vec<Vec<Node>>,
        open: char,
        close: char,
        /// `align` environments align columns on the `&` (right-aligned
        /// relation column) instead of centring every cell.
        align: bool,
    },
    Space(f64),
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct TexParser<'a> {
    src: &'a str,
    chars: Vec<char>,
    pos: usize,
    display: bool,
    /// Pending `\limits` / `\nolimits` directive for the next script.
    limit_mode: Option<bool>,
}

impl<'a> TexParser<'a> {
    fn new(src: &'a str, display: bool) -> TexParser<'a> {
        TexParser {
            src,
            chars: src.chars().collect(),
            pos: 0,
            display,
            limit_mode: None,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn eat_ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn rest_is(&self, pat: &str) -> bool {
        self.src[self.pos..].starts_with(pat)
    }

    /// `{ … }` group — called with `pos` just past the opening `{`.
    fn parse_group_body(&mut self) -> Node {
        let mut depth = 1;
        let inner = self.take_raw(&mut depth);
        let mut t = TexParser::new(inner.trim(), self.display);
        t.parse_row()
    }

    /// `{ … }` group, command names stripped — for `\text`, `\operatorname`,
    /// environment names.
    fn parse_group_body_plain(&mut self) -> String {
        let mut depth = 1;
        self.take_raw(&mut depth).replace('\\', "")
    }

    fn take_raw(&mut self, depth: &mut usize) -> String {
        let mut out = String::new();
        while let Some(c) = self.next() {
            match c {
                '{' => *depth += 1,
                '}' => {
                    *depth -= 1;
                    if *depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            if *depth > 0 {
                out.push(c);
            }
        }
        out
    }

    fn parse_row(&mut self) -> Node {
        Node::Row(self.parse_item_list())
    }

    fn parse_item_list(&mut self) -> Vec<Node> {
        let mut items: Vec<Node> = Vec::new();
        loop {
            self.eat_ws();
            let c = match self.peek() {
                None => break,
                Some('}') => break,
                Some('&') => break,
                Some('\\') if self.rest_is("\\\\") => break,
                Some('\\') if self.rest_is("\\end") => break,
                Some('\\') if self.rest_is("\\right") => break,
                Some(c) => c,
            };
            self.pos += 1;
            match c {
                '{' => items.push(self.parse_group_body()),
                '^' | '_' => {
                    let is_sup = c == '^';
                    self.eat_ws();
                    let arg = if self.peek() == Some('{') {
                        self.pos += 1;
                        Some(self.parse_group_body())
                    } else if let Some(x) = self.peek() {
                        if x.is_whitespace() {
                            None
                        } else {
                            self.pos += 1;
                            Some(normal_atom(self, x))
                        }
                    } else {
                        None
                    };
                    let forced = self.limit_mode.take();
                    let node = self.attach_script(&mut items, is_sup, arg, forced);
                    items.push(node);
                }
                '\\' => {
                    let r = &self.src[self.pos..];
                    let infix_over = r.starts_with("over")
                        && !r[4..]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_alphabetic());
                    if infix_over {
                        self.pos += 4;
                        self.eat_ws();
                        let num = std::mem::take(&mut items);
                        let den = self.parse_item_list();
                        let n = Node::Row(num);
                        let d = Node::Row(den);
                        items.push(Node::Frac(Box::new(n), Box::new(d)));
                        break;
                    } else if let Some(h) = self.parse_command() {
                        if !matches!(h, Node::Row(ref v) if v.is_empty()) {
                            items.push(h);
                        }
                    }
                }
                '~' => items.push(Node::Space(0.333)),
                c => items.push(normal_atom(self, c)),
            }
        }
        items
    }

    /// Attach a `^`/`_` script to the last item (a sibling in a row), or to a
    /// phantom base when the row is empty.
    fn attach_script(
        &mut self,
        items: &mut Vec<Node>,
        is_sup: bool,
        arg: Option<Node>,
        forced: Option<bool>,
    ) -> Node {
        let mut base = items.pop();
        // A big op may carry both `_a^b` and `^b_a`; the second script merges
        // into the pending Script/OverUnder built by the first.
        let mut sub: Option<Box<Node>> = None;
        let mut sup: Option<Box<Node>> = None;
        match base.take() {
            Some(Node::Script {
                base: sb,
                sub: s,
                sup: u,
            }) => {
                base = Some(*sb);
                sub = s;
                sup = u;
            }
            Some(Node::OverUnder {
                base: ob,
                below: l,
                above: a,
            }) => {
                base = Some(*ob);
                sub = l;
                sup = a;
            }
            other => base = other,
        }
        let prev_is_op = base
            .as_ref()
            .map(|b| match b {
                Node::Sym(s) => s.cls == Class::Op && s.movable,
                _ => false,
            })
            .unwrap_or(false);
        let under_over = match forced {
            Some(v) => v,
            None => self.display && prev_is_op,
        };
        // The new script slots into whichever position is still free; a
        // pending script from the first pass stays in its original slot.
        let phantom = base.take().unwrap_or(Node::Row(Vec::new()));
        if under_over {
            let (below, above) = if is_sup {
                (sub, arg.map(Box::new))
            } else {
                (arg.map(Box::new), sup)
            };
            Node::OverUnder {
                base: Box::new(phantom),
                below,
                above,
            }
        } else {
            let (subp, supp) = if is_sup {
                (sub, arg.map(Box::new))
            } else {
                (arg.map(Box::new), sup)
            };
            Node::Script {
                base: Box::new(phantom),
                sub: subp,
                sup: supp,
            }
        }
    }

    fn parse_command(&mut self) -> Option<Node> {
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphabetic() {
                name.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        if name.is_empty() {
            let c = self.next()?; // single-char control like \, \;
            let w = match c {
                ',' => 0.167,
                ':' | ';' => 0.222,
                '!' => 0.167,
                ' ' => 0.333,
                _ => 0.333,
            };
            return Some(Node::Space(w));
        }
        Some(self.dispatch(name))
    }

    fn dispatch(&mut self, name: String) -> Node {
        let n = name.as_str();
        match n {
            "begin" => {
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let env = self.parse_group_body_plain();
                self.parse_matrix(&env)
            }
            "end" => Node::Row(Vec::new()),
            "frac" | "dfrac" | "tfrac" => {
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let num = self.parse_group_body();
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let den = self.parse_group_body();
                Node::Frac(Box::new(num), Box::new(den))
            }
            "cfrac" => {
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let num = self.parse_group_body();
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let den = self.parse_group_body();
                Node::CFrac(Box::new(num), Box::new(den))
            }
            "boxed" => {
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let inner = self.parse_group_body();
                Node::Boxed(Box::new(inner))
            }
            "vec" | "hat" | "bar" | "overline" | "dot" | "ddot" | "tilde" | "widehat"
            | "widetilde" => {
                let kind = match n {
                    "vec" => AccentKind::Vec,
                    "hat" => AccentKind::Hat,
                    "bar" => AccentKind::Bar,
                    "overline" => AccentKind::Overline,
                    "dot" => AccentKind::Dot,
                    "ddot" => AccentKind::Ddot,
                    "tilde" => AccentKind::Tilde,
                    "widehat" => AccentKind::Widehat,
                    _ => AccentKind::Widetilde,
                };
                self.eat_ws();
                let base = if self.peek() == Some('{') {
                    self.pos += 1;
                    self.parse_group_body()
                } else if let Some(c) = self.next() {
                    normal_atom(self, c)
                } else {
                    Node::Row(Vec::new())
                };
                Node::Accent {
                    base: Box::new(base),
                    kind,
                }
            }
            "sqrt" => {
                self.eat_ws();
                let deg = if self.peek() == Some('[') {
                    self.pos += 1;
                    let mut idx = String::new();
                    while let Some(c) = self.peek() {
                        if c == ']' {
                            break;
                        }
                        idx.push(c);
                        self.pos += 1;
                    }
                    if self.peek() == Some(']') {
                        self.pos += 1;
                    }
                    let mut t = TexParser::new(idx.trim(), self.display);
                    Some(Box::new(t.parse_row()))
                } else {
                    None
                };
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let rad = self.parse_group_body();
                Node::Sqrt {
                    rad: Box::new(rad),
                    deg,
                }
            }
            "text" | "mbox" | "textrm" | "textnormal" | "mathrm" => {
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let inner = self.parse_group_body_plain();
                Node::Sym(Sym {
                    t: inner,
                    italic: false,
                    bold: false,
                    cls: Class::Ord,
                    movable: false,
                })
            }
            "operatorname" => {
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let inner = self.parse_group_body_plain();
                Node::Sym(Sym {
                    t: inner,
                    italic: false,
                    bold: false,
                    cls: Class::Ord,
                    movable: false,
                })
            }
            "overset" | "stackrel" => {
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let top = self.parse_group_body();
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let base = self.parse_group_body();
                Node::OverUnder {
                    base: Box::new(base),
                    below: None,
                    above: Some(Box::new(top)),
                }
            }
            "left" => {
                self.eat_ws();
                let open = self.next().unwrap_or('(');
                let body = self.parse_row();
                let close = if self.rest_is("\\right") {
                    self.pos += 6;
                    self.eat_ws();
                    self.next().unwrap_or(')')
                } else {
                    ')'
                };
                Node::Fence {
                    open: if matches!(open, '(' | '[' | '{' | '|' | '/' | '.') {
                        open
                    } else {
                        '('
                    },
                    inner: Box::new(body),
                    close: if matches!(close, ')' | ']' | '}' | '|' | '/' | '.') {
                        close
                    } else {
                        ')'
                    },
                }
            }
            "right" => Node::Row(Vec::new()),
            "limits" => {
                self.limit_mode = Some(true);
                Node::Row(Vec::new())
            }
            "nolimits" => {
                self.limit_mode = Some(false);
                Node::Row(Vec::new())
            }
            "displaystyle" | "textstyle" => Node::Row(Vec::new()),
            "qquad" => Node::Space(2.0),
            "quad" => Node::Space(1.0),
            "," | ";" | "!" | ":" => Node::Space(sym_space(n)),
            n if MATHVARIANTS.contains(&n) => {
                let bold = n == "mathbf";
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let inner = self.parse_group_body_plain();
                Node::Sym(Sym {
                    t: inner,
                    italic: false,
                    bold,
                    cls: Class::Ord,
                    movable: false,
                })
            }
            n if MATH_FUNCS.contains(&n) => Node::Sym(Sym {
                t: n.to_string(),
                italic: false,
                bold: false,
                cls: Class::Ord,
                movable: false,
            }),
            n if n.len() == 1 => Node::Sym(Sym {
                t: n.to_string(),
                italic: true,
                bold: false,
                cls: Class::Ord,
                movable: false,
            }),
            _ => {
                if let Some(u) = greek(n) {
                    Node::Sym(Sym {
                        t: u.to_string(),
                        italic: true,
                        bold: false,
                        cls: Class::Ord,
                        movable: false,
                    })
                } else if let Some(ch) = look(n) {
                    Node::Sym(Sym {
                        t: ch.clone(),
                        italic: false,
                        bold: false,
                        cls: class_of(&ch),
                        movable: movable_op(&ch),
                    })
                } else if let Some(big) = bigop(n) {
                    Node::Sym(Sym {
                        t: big.clone(),
                        italic: false,
                        bold: false,
                        cls: Class::Op,
                        movable: movable_op(&big),
                    })
                } else {
                    Node::Sym(Sym {
                        t: format!("\\{n}"),
                        italic: false,
                        bold: false,
                        cls: Class::Ord,
                        movable: false,
                    })
                }
            }
        }
    }

    fn parse_matrix(&mut self, env: &str) -> Node {
        let (open, close, align) = match env {
            "pmatrix" => ('(', ')', false),
            "bmatrix" => ('[', ']', false),
            "Bmatrix" => ('{', '}', false),
            "vmatrix" => ('|', '|', false),
            "Vmatrix" => ('‖', '‖', false),
            "align" | "align*" | "aligned" | "aligned*" | "eqnarray" | "eqnarray*" => {
                ('.', '.', true)
            }
            _ => ('.', '.', false),
        };
        let mut rows: Vec<Vec<Node>> = Vec::new();
        let mut cells: Vec<Node> = Vec::new();
        let mut cell = Vec::new();
        loop {
            self.eat_ws();
            if self.rest_is("\\end") {
                self.pos += 4;
                self.eat_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                }
                let _ = self.parse_group_body_plain();
                break;
            }
            if self.rest_is("\\\\") {
                self.pos += 2;
                cells.push(Node::Row(cell));
                cell = Vec::new();
                rows.push(std::mem::take(&mut cells));
                continue;
            }
            match self.peek() {
                None => break,
                Some('&') => {
                    self.pos += 1;
                    cells.push(Node::Row(cell));
                    cell = Vec::new();
                }
                Some(_) => {
                    let frag = self.parse_row();
                    cell.push(frag);
                }
            }
        }
        if !cell.is_empty() || !cells.is_empty() {
            cells.push(Node::Row(cell));
            rows.push(std::mem::take(&mut cells));
        }
        Node::Matrix {
            rows,
            open,
            close,
            align,
        }
    }
}

fn sym_space(n: &str) -> f64 {
    match n {
        "," => 0.167,
        ":" | ";" => 0.222,
        "!" => 0.167,
        _ => 0.167,
    }
}

/// A single non-control character in a formula. Letter runs are split into
/// per-letter italic atoms (TeX keeps `dx` as two close italic variables).
fn normal_atom(p: &mut TexParser<'_>, c: char) -> Node {
    if c.is_ascii_digit() {
        let mut n = String::new();
        n.push(c);
        while let Some(d) = p.peek() {
            if d.is_ascii_digit() {
                n.push(d);
                p.pos += 1;
            } else {
                break;
            }
        }
        Node::Sym(Sym {
            t: n,
            italic: false,
            bold: false,
            cls: Class::Ord,
            movable: false,
        })
    } else if c.is_ascii_alphabetic() {
        let mut name = String::new();
        name.push(c);
        while let Some(d) = p.peek() {
            if d.is_ascii_alphabetic() {
                name.push(d);
                p.pos += 1;
            } else {
                break;
            }
        }
        let letters: Vec<Node> = name
            .chars()
            .map(|ch| {
                Node::Sym(Sym {
                    t: ch.to_string(),
                    italic: true,
                    bold: false,
                    cls: Class::Ord,
                    movable: false,
                })
            })
            .collect();
        if letters.len() == 1 {
            letters.into_iter().next().unwrap()
        } else {
            Node::Row(letters)
        }
    } else if c == '-' {
        // The ASCII hyphen-minus is too short and sits too low to read as a
        // math binary minus; map it to the proper U+2212 MINUS SIGN so the
        // glyph picks up the wide minus bar from a math font.
        Node::Sym(Sym {
            t: "−".to_string(),
            italic: false,
            bold: false,
            cls: Class::Bin,
            movable: false,
        })
    } else {
        let t = c.to_string();
        Node::Sym(Sym {
            t,
            italic: false,
            bold: false,
            cls: class_of(&c.to_string()),
            movable: false,
        })
    }
}

// ---------------------------------------------------------------------------
// LBox layout (TeX-style metrics, all numbers in em of the surrounding text)
// ---------------------------------------------------------------------------

/// Lift of the math axis above the baseline.
const AXIS: f64 = 0.25;
/// Default rule (fraction bar / overline) thickness.
const RULE: f64 = 0.04;
/// Script size factor.
const SCR: f64 = 0.7;
/// Script-of-script size factor is SCR*SCR ≈ 0.5 (handled by nesting).
/// Constant visual stroke width (em of the surrounding text) for the
/// path-drawn math glyphs: big operators, stretchy delimiters, the radical.
/// Matches the ~0.05em stroke of ordinary text digits at 1em.
const SW: f64 = 0.06;

#[derive(Debug, Clone)]
struct LBox {
    w: f64,
    h: f64,
    d: f64,
    ink: Vec<Ink>,
}

#[derive(Debug, Clone)]
enum Ink {
    /// `y` is the glyph baseline (up-positive); `s` the font size in em.
    T {
        x: f64,
        y: f64,
        s: f64,
        t: String,
        italic: bool,
        bold: bool,
    },
    /// Horizontal rule from (x0, y0) to (x1, y1), y up-positive.
    R { x0: f64, y0: f64, x1: f64, y1: f64 },
    /// Centreline stroke path (`d` in the glyph's own design box, baseline at
    /// y = 0) positioned with `translate(x, y) scale(sf)`. The visual stroke
    /// width `sw` is constant regardless of `sf`, which keeps big operators,
    /// stretchy delimiters and the radical optically weight-matched to the
    /// surrounding text instead of thickening with the glyph size.
    P {
        x: f64,
        y: f64,
        sf: f64,
        sw: f64,
        d: String,
    },
    /// Filled glyph outline (`d` a closed path in the font's own design box,
    /// baseline at y = 0) positioned with `translate(x, y) scale(sf)`. Used
    /// for the radical, which is a solid shape rather than a constant-width
    /// centreline stroke.
    F { x: f64, y: f64, sf: f64, d: String },
}

impl LBox {
    fn empty() -> LBox {
        LBox {
            w: 0.0,
            h: 0.0,
            d: 0.0,
            ink: Vec::new(),
        }
    }

    /// Translate every ink particle by (x, y) (y up-positive).
    fn place(&mut self, x: f64, y: f64) {
        for ink in &mut self.ink {
            match ink {
                Ink::T { x: px, y: py, .. } => {
                    *px += x;
                    *py += y;
                }
                Ink::R { x0, y0, x1, y1, .. } => {
                    *x0 += x;
                    *x1 += x;
                    *y0 += y;
                    *y1 += y;
                }
                Ink::P { x: px, y: py, .. } => {
                    *px += x;
                    *py += y;
                }
                Ink::F { x: px, y: py, .. } => {
                    *px += x;
                    *py += y;
                }
            }
        }
    }
}

/// Scale a box (widths, heights, ink) by a factor — used for scripts.
fn scale_box(b: &mut LBox, k: f64) {
    b.w *= k;
    b.h *= k;
    b.d *= k;
    for ink in &mut b.ink {
        match ink {
            Ink::T { x, y, s, .. } => {
                *x *= k;
                *y *= k;
                *s *= k;
            }
            Ink::R { x0, y0, x1, y1 } => {
                *x0 *= k;
                *y0 *= k;
                *x1 *= k;
                *y1 *= k;
            }
            Ink::P { x, y, sf, .. } => {
                *x *= k;
                *y *= k;
                *sf *= k;
            }
            Ink::F { x, y, sf, .. } => {
                *x *= k;
                *y *= k;
                *sf *= k;
            }
        }
    }
}

fn class_of(t: &str) -> Class {
    let ch = t.chars().next().unwrap_or(' ');
    match ch {
        '(' | '[' | '{' | '|' | '‖' | '⌊' | '⌈' => Class::Open,
        ')' | ']' | '}' => Class::Close,
        ',' | ';' | ':' | '!' | '?' => Class::Punct,
        '+' | '-' | '−' | '×' | '÷' | '±' | '∓' | '∗' | '∘' | '∙' | '⋆' | '∧' | '∨' | '∪' | '∩'
        | '∖' | '⊕' | '⊗' | '⋅' => Class::Bin,
        '=' | '<' | '>' | '≤' | '≥' | '≈' | '≠' | '≡' | '∼' | '≃' | '≅' | '≪' | '≫' | '∝' | '⊥'
        | '∥' | '∣' | '∈' | '∉' | '∋' | '⊂' | '⊃' | '⊆' | '⊇' | '→' | '←' | '↔' | '⇒' | '⇐'
        | '⇔' | '↦' | '↑' | '↓' => Class::Rel,
        '∑' | '∏' | '∐' | '⋃' | '⋂' | '⨁' | '⨂' | '∫' | '∬' | '∭' | '∮' => {
            Class::Op
        }
        _ => Class::Ord,
    }
}

fn movable_op(t: &str) -> bool {
    matches!(t, "∑" | "∏" | "∐" | "⋃" | "⋂" | "⨁" | "⨂")
}

/// Approximate advance width (in em) of a character in a serif math font.
fn unit_w(c: char) -> f64 {
    match c {
        'i' | 'j' | 'l' | 'ℓ' => 0.28,
        'f' | 't' | 'r' => 0.40,
        'm' | 'w' | 'M' | 'W' => 0.82,
        'A'..='Z' => 0.66,
        'a'..='z' => 0.54,
        '0'..='9' => 0.50,
        '∫' => 0.9,
        '∬' => 1.15,
        '∭' => 1.35,
        '∮' => 1.0,
        '∑' | '∏' | '∐' => 1.1,
        '⋃' | '⋂' | '⨁' | '⨂' => 1.0,
        '(' | ')' | '[' | ']' => 0.33,
        '|' => 0.25,
        '‖' => 0.5,
        '+' | '×' | '÷' | '−' | '±' | '∓' | '∗' | '∧' | '∨' | '∪' | '∩' | '⊕' | '⊗' => {
            0.55
        }
        '=' | '≡' | '≈' | '≤' | '≥' | '≠' | '≃' | '≅' | '∼' | '∝' | '⊂' | '⊃' | '⊆' | '⊇' => {
            0.8
        }
        '<' | '>' | '≪' | '≫' => 0.72,
        '∈' | '∉' | '∋' => 0.72,
        '→' | '←' | '↔' | '⇒' | '⇐' | '⇔' | '↦' => 0.95,
        '↑' | '↓' => 0.6,
        '∞' => 0.7,
        '∂' | '∇' | '△' | '∠' => 0.62,
        '∘' => 0.5,
        '⋅' => 0.35,
        '∙' | '⋆' => 0.5,
        ',' => 0.18,
        ';' => 0.30,
        ':' => 0.28,
        '!' | '?' => 0.30,
        '.' => 0.20,
        '/' => 0.55,
        '%' => 0.7,
        '…' => 0.9,
        '⋯' => 0.9,
        '⋮' => 0.6,
        '⋱' => 0.75,
        _ => 0.60,
    }
}

/// Sum of per-character widths; multi-char text runs count each letter.
fn text_w(t: &str) -> f64 {
    t.chars().map(unit_w).sum()
}

/// ASCII-ish letter/symbol box: standard character ascent/descent.
fn glyph_h(t: &str) -> f64 {
    if t.chars().count() <= 1 && matches!(t, "(" | ")" | "[" | "]" | "|" | "‖") {
        0.9
    } else {
        0.72
    }
}
fn glyph_d(_t: &str) -> f64 {
    0.15
}

/// Metric data for big/integral operators and stretchy delimiters.
struct GlyphShape {
    asc: f64,
    desc: f64,
}

fn shape(c: char) -> GlyphShape {
    match c {
        '∫' | '∬' | '∭' | '∮' => GlyphShape {
            asc: 0.75,
            desc: 0.25,
        },
        '∑' | '∏' | '∐' | '⋃' | '⋂' | '⨁' | '⨂' => GlyphShape {
            asc: 0.80,
            desc: 0.30,
        },
        '(' | ')' | '[' | ']' => GlyphShape {
            asc: 0.90,
            desc: 0.30,
        },
        '|' => GlyphShape {
            asc: 0.80,
            desc: 0.20,
        },
        '‖' => GlyphShape {
            asc: 0.80,
            desc: 0.20,
        },
        '√' => GlyphShape {
            asc: 0.78,
            desc: 0.22,
        },
        _ => GlyphShape {
            asc: 0.72,
            desc: 0.18,
        },
    }
}

impl Sym {
    fn boxed(&self, display: bool) -> LBox {
        let t = &self.t;
        let mut b = LBox {
            w: text_w(t),
            h: glyph_h(t),
            d: glyph_d(t),
            ink: Vec::new(),
        };
        if self.cls == Class::Op && t.chars().count() == 1 {
            let ch = t.chars().next().unwrap();
            let (w, h, d) = bigop_size(ch, display);
            if let Some((dpath, asc, desc, dw)) = bigop_path(ch) {
                let sf = (h + d) / (asc + desc);
                let y = h - asc * sf;
                b.w = dw * sf;
                b.h = h;
                b.d = d;
                b.ink.push(Ink::P {
                    x: 0.0,
                    y,
                    sf,
                    sw: SW,
                    d: dpath,
                });
            } else {
                let sh = shape(ch);
                let sf = (h + d) / (sh.asc + sh.desc);
                let y = h - sh.asc * sf;
                b.w = w;
                b.h = h;
                b.d = d;
                b.ink.push(Ink::T {
                    x: 0.0,
                    y,
                    s: sf,
                    t: t.clone(),
                    italic: false,
                    bold: false,
                });
            }
        } else {
            b.ink.push(Ink::T {
                x: 0.0,
                y: 0.0,
                s: 1.0,
                t: t.clone(),
                italic: self.italic,
                bold: self.bold,
            });
        }
        b
    }
}

fn bigop_size(ch: char, display: bool) -> (f64, f64, f64) {
    match ch {
        '∫' | '∬' | '∭' | '∮' => {
            if display {
                // 2.35em tall, symmetric about the axis.
                let total = 2.35;
                let h = AXIS + total / 2.0;
                (text_w(&ch.to_string()) * 1.1, h, total - h)
            } else {
                (text_w(&ch.to_string()), 1.1, 0.4)
            }
        }
        _ => {
            // sum-like
            if display {
                (text_w(&ch.to_string()) * 1.7, 1.7, 0.6)
            } else {
                (text_w(&ch.to_string()), 1.05, 0.35)
            }
        }
    }
}

/// Centreline path + design box for a stretchy delimiter. `asc`/`desc` are
/// the design ascent/descent (baseline at y = 0, up-positive) and `dw` is the
/// width the delimiter occupies at scale 1 (used as the fence slot width).
fn delim_path(c: char) -> Option<(&'static str, f64, f64, f64)> {
    match c {
        '(' => Some((
            "M 0.30,0.88 C 0.16,0.84 0.09,0.66 0.09,0.31 C 0.09,-0.04 0.16,-0.22 0.30,-0.28",
            0.90, 0.30, 0.40,
        )),
        ')' => Some((
            "M 0.18,0.88 C 0.32,0.84 0.39,0.66 0.39,0.31 C 0.39,-0.04 0.32,-0.22 0.18,-0.28",
            0.90, 0.30, 0.40,
        )),
        '[' => Some(("M 0.42,0.88 L 0.14,0.88 V -0.28 L 0.42,-0.28", 0.90, 0.30, 0.40)),
        ']' => Some(("M 0.14,0.88 L 0.42,0.88 V -0.28 L 0.14,-0.28", 0.90, 0.30, 0.40)),
        '{' => Some((
            "M 0.34,0.88 C 0.12,0.88 0.06,0.74 0.06,0.58 C 0.06,0.40 0.22,0.34 0.20,0.14 \
             C 0.18,-0.06 0.06,-0.10 0.06,-0.28 C 0.06,-0.44 0.12,-0.58 0.34,-0.60",
            0.90, 0.62, 0.40,
        )),
        '}' => Some((
            "M 0.06,0.88 C 0.28,0.88 0.34,0.74 0.34,0.58 C 0.34,0.40 0.18,0.34 0.20,0.14 \
             C 0.22,-0.06 0.34,-0.10 0.34,-0.28 C 0.34,-0.44 0.28,-0.58 0.06,-0.60",
            0.90, 0.62, 0.40,
        )),
        '|' => Some(("M 0.25,0.80 L 0.25,-0.20", 0.80, 0.20, 0.28)),
        '‖' => Some(("M 0.20,0.80 L 0.20,-0.20 M 0.70,0.80 L 0.70,-0.20", 0.80, 0.20, 0.90)),
        _ => None,
    }
}

/// One integral sign as a centreline path, offset horizontally by `x`.
fn int_path(x: f64) -> String {
    format!(
        "M {a},0.76 C {b},0.76 {c},0.70 {c},0.64 C {c},0.34 {d},0.04 {e},-0.14 \
         C {f},-0.34 {g},-0.40 {h},-0.32 C {i},-0.27 {i},-0.22 {j},-0.18",
        a = x + 0.48,
        b = x + 0.40,
        c = x + 0.36,
        d = x + 0.34,
        e = x + 0.32,
        f = x + 0.28,
        g = x + 0.20,
        h = x + 0.14,
        i = x + 0.10,
        j = x + 0.13,
    )
}

/// Centreline path + design box for a big operator (baseline at y = 0).
fn bigop_path(c: char) -> Option<(String, f64, f64, f64)> {
    const SUM: (&str, f64, f64, f64) = (
        "M 0.10,0.78 L 0.72,0.78 M 0.72,-0.26 L 0.10,-0.26 \
         M 0.10,0.78 L 0.40,0.25 L 0.10,-0.26",
        0.80,
        0.30,
        0.82,
    );
    const PROD: (&str, f64, f64, f64) = (
        "M 0.10,0.78 L 0.72,0.78 M 0.10,0.78 L 0.10,-0.26 M 0.72,0.78 L 0.72,-0.26",
        0.80,
        0.30,
        0.82,
    );
    const COPROD: (&str, f64, f64, f64) = (
        "M 0.10,0.78 L 0.10,-0.26 M 0.10,-0.26 L 0.72,-0.26 M 0.72,-0.26 L 0.72,0.78",
        0.80,
        0.30,
        0.82,
    );
    const UN: (&str, f64, f64, f64) = (
        "M 0.10,0.78 L 0.10,-0.06 C 0.10,-0.26 0.72,-0.26 0.72,-0.06 L 0.72,0.78",
        0.80,
        0.30,
        0.82,
    );
    const INTER: (&str, f64, f64, f64) = (
        "M 0.10,-0.26 L 0.10,0.06 C 0.10,0.26 0.72,0.26 0.72,0.06 L 0.72,-0.26",
        0.80,
        0.30,
        0.82,
    );
    const DIRSUM: (&str, f64, f64, f64) = (
        "M 0.41,0.62 L 0.41,-0.30 M 0.02,0.16 L 0.80,0.16 M 0.41,0.62 A 0.46 0.46 0 1 0 0.409,-0.30",
        0.80,
        0.30,
        0.82,
    );
    const TENSOR: (&str, f64, f64, f64) = (
        "M 0.08,0.42 L 0.74,-0.10 M 0.08,-0.10 L 0.74,0.42 M 0.41,0.62 A 0.46 0.46 0 1 0 0.409,-0.30",
        0.80,
        0.30,
        0.82,
    );
    match c {
        '∑' => Some((SUM.0.to_string(), SUM.1, SUM.2, SUM.3)),
        '∏' => Some((PROD.0.to_string(), PROD.1, PROD.2, PROD.3)),
        '∐' => Some((COPROD.0.to_string(), COPROD.1, COPROD.2, COPROD.3)),
        '⋃' => Some((UN.0.to_string(), UN.1, UN.2, UN.3)),
        '⋂' => Some((INTER.0.to_string(), INTER.1, INTER.2, INTER.3)),
        '⨁' => Some((DIRSUM.0.to_string(), DIRSUM.1, DIRSUM.2, DIRSUM.3)),
        '⨂' => Some((TENSOR.0.to_string(), TENSOR.1, TENSOR.2, TENSOR.3)),
        '∫' => Some((int_path(0.0), 0.76, 0.36, 0.60)),
        '∬' => Some((
            format!("{} {}", int_path(0.0), int_path(0.40)),
            0.76,
            0.36,
            0.88,
        )),
        '∭' => Some((
            format!("{} {} {}", int_path(0.0), int_path(0.36), int_path(0.72)),
            0.76,
            0.36,
            1.20,
        )),
        '∮' => Some((
            format!(
                "{} M 0.22,0.58 A 0.14 0.14 0 1 1 0.218,0.58",
                int_path(0.0)
            ),
            0.76,
            0.36,
            0.60,
        )),
        _ => None,
    }
}

/// Build a stretched delimiter as a constant-stroke-width path centred on
/// `centre` with total height `height`. Returns the ink and the slot width it
/// occupies.
fn stretch_delim(ch: char, centre: f64, height: f64) -> Option<(Ink, f64)> {
    let (d, asc, desc, dw) = delim_path(ch)?;
    let sf = height / (asc + desc);
    let y = centre - (asc - desc) / 2.0 * sf;
    let ink = Ink::P {
        x: 0.0,
        y,
        sf,
        sw: SW,
        d: d.to_string(),
    };
    Some((ink, dw * sf))
}

fn layout(node: &Node, display: bool) -> LBox {
    match node {
        Node::Row(children) => layout_row(children, display),
        Node::Frac(n, d) => layout_frac(n, d, display),
        Node::Sqrt { rad, deg } => layout_sqrt(rad, deg.as_deref(), display),
        Node::Script { base, sub, sup } => {
            layout_script(base, sub.as_deref(), sup.as_deref(), display)
        }
        Node::OverUnder { base, below, above } => {
            layout_overunder(base, below.as_deref(), above.as_deref(), display)
        }
        Node::Sym(s) => s.boxed(display),
        Node::Fence { open, inner, close } => layout_fence(*open, inner, *close, display),
        Node::Matrix {
            rows,
            open,
            close,
            align,
        } => layout_matrix(rows, *open, *close, *align, display),
        Node::CFrac(n, d) => layout_cfrac(n, d),
        Node::Boxed(inner) => layout_boxed(inner, display),
        Node::Accent { base, kind } => layout_accent(base, *kind, display),
        Node::Space(w) => LBox {
            w: *w,
            h: 0.0,
            d: 0.0,
            ink: Vec::new(),
        },
    }
}

fn node_class(n: &Node) -> Class {
    match n {
        Node::Sym(s) => s.cls,
        Node::Row(v) => v.last().map(node_class).unwrap_or(Class::Ord),
        Node::Fence { open, .. } => {
            if *open == '.' {
                Class::Ord
            } else {
                Class::Open
            }
        }
        Node::Frac(..) | Node::Sqrt { .. } => Class::Ord,
        Node::CFrac(..) | Node::Boxed(..) | Node::Accent { .. } => Class::Ord,
        Node::Script { base, .. } | Node::OverUnder { base, .. } => node_class(base),
        _ => Class::Ord,
    }
}

/// TeX spacing (in mu) between adjacent atom classes — appendix-G style.
/// Around a binary operator (`Bin`) TeX inserts \medmuskip (4 mu); the table
/// uses 2 mu for Ord/Bin which reads as too tight against a wide MINUS SIGN,
/// so Ord→Bin and Bin→Ord are bumped to 4 mu to match Bin→Bin/Bin→Rel.
fn space_mu(prev: Class, next: Class) -> f64 {
    const T: [[f64; 7]; 7] = [
        //            Ord  Op  Bin  Rel  Open Close Punct
        /* Ord */
        [0.0, 1.0, 4.0, 3.0, 0.0, 0.0, 0.0],
        /* Op  */ [1.0, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0],
        /* Bin */ [4.0, 2.0, 4.0, 4.0, 0.0, 4.0, 4.0],
        /* Rel */ [3.0, 3.0, 4.0, 4.0, 0.0, 0.0, 0.0],
        /* Open*/ [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        /* Close*/ [0.0, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0],
        /* Punct*/ [0.0, 1.0, 2.0, 1.0, 0.0, 0.0, 0.0],
    ];
    T[prev as usize][next as usize] / 18.0
}

fn layout_row(children: &[Node], display: bool) -> LBox {
    let mut out = LBox::empty();
    let mut prev: Option<Class> = None;
    for child in children {
        if let Some(p) = prev {
            let sp = space_mu(p, node_class(child));
            if sp > 0.0 {
                out.w += sp;
            }
        }
        let mut b = layout(child, display);
        let x = out.w;
        b.place(x, 0.0);
        out.w += b.w;
        out.h = out.h.max(b.h);
        out.d = out.d.max(b.d);
        out.ink.extend(b.ink);
        prev = Some(node_class(child));
    }
    out
}

fn layout_frac(n: &Node, d: &Node, display: bool) -> LBox {
    let mut num = layout(n, display);
    let mut den = layout(d, display);
    let bar_top = AXIS + RULE / 2.0;
    let bar_bot = AXIS - RULE / 2.0;
    let bn = bar_top + 0.07 + num.d; // numerator baseline (up-positive)
    let bd = bar_bot - 0.10 - den.h; // denominator baseline
    let w = num.w.max(den.w);
    // The denominator is placed at math-y = bd (negative), so the deepest
    // ink below the outer baseline sits at `bd - den.d`. The descent is
    // therefore `den.d - bd` (bd is negative, so this grows with both the
    // denominator's own descent and how far below the bar it sits). Using
    // `den.h` here — the denominator's ascent above its own baseline —
    // undercounts when the denominator itself descends deeply (nested
    // \cfrac, tall under-limits of a big op) and overcounts for a simple
    // \frac{1}{2}.
    let mut out = LBox {
        w,
        h: bn + num.h,
        d: den.d - bd,
        ink: Vec::new(),
    };
    out.ink.push(Ink::R {
        x0: 0.0,
        y0: bar_bot,
        x1: w,
        y1: bar_top,
    });
    num.place((w - num.w) / 2.0, bn);
    den.place((w - den.w) / 2.0, bd);
    out.ink.extend(num.ink);
    out.ink.extend(den.ink);
    out
}

/// `\cfrac` — a display-style fraction: the numerator/denominator stay at full
/// size with extra clearance above and below the bar (continued fractions).
fn layout_cfrac(n: &Node, d: &Node) -> LBox {
    let mut num = layout(n, true);
    let mut den = layout(d, true);
    let bar_top = AXIS + RULE / 2.0;
    let bar_bot = AXIS - RULE / 2.0;
    let bn = bar_top + 0.17 + num.d; // numerator baseline (up-positive)
    let bd = bar_bot - 0.17 - den.h; // denominator baseline
    let w = num.w.max(den.w);
    // See `layout_frac` — descent must track the denominator's own descent
    // (`den.d`) once placed at `bd`, otherwise deep nested \cfrac chains
    // overflow the SVG viewBox at the bottom.
    let mut out = LBox {
        w,
        h: bn + num.h,
        d: den.d - bd,
        ink: Vec::new(),
    };
    out.ink.push(Ink::R {
        x0: 0.0,
        y0: bar_bot,
        x1: w,
        y1: bar_top,
    });
    num.place((w - num.w) / 2.0, bn);
    den.place((w - den.w) / 2.0, bd);
    out.ink.extend(num.ink);
    out.ink.extend(den.ink);
    out
}

/// Actual visual extent (top, bottom in y-up em) of a box's ink. Text glyphs
/// extend above their design h by ~0.8 × font_size in real ascent, so the
/// design h underestimates the visual top for cursive or full-height glyphs
/// (∞, ∑, ∏, capitals, …). Pen strokes painted as `Ink::R` lie in their own
/// rectangle and need no ascent correction.
fn visual_extent(ink: &[Ink]) -> (f64, f64) {
    let mut top = 0.0f64;
    let mut bottom = 0.0f64;
    for i in ink {
        match i {
            Ink::T { y, s, .. } => {
                top = top.max(*y + 0.8 * s);
                bottom = bottom.min(*y - 0.2 * s);
            }
            Ink::R { y0, y1, .. } => {
                top = top.max(*y1);
                bottom = bottom.min(*y0);
            }
            _ => {}
        }
    }
    (top, bottom)
}

/// `\boxed{…}` — a rule box drawn around the content with a small padding.
fn layout_boxed(inner: &Node, display: bool) -> LBox {
    let mut b = layout(inner, display);
    let pad = 0.09;
    let rule = 0.05;
    // Match the `rule` clearance that already exists between the top/bottom
    // rects and the viewBox edges on the left/right sides too — otherwise the
    // left and right strokes render flush against the viewBox boundary and
    // browsers can clip them on render.
    let margin = rule;
    // Text glyphs extend above their design h by ~0.8 × font_size in real
    // ascent, so the design h underestimates the visual top. Push the frame
    // out by `gap = 3 × rule` so the rule sits well outside the glyph's
    // visual top, leaving clear breathing room above (and to the right of)
    // the formula. The right side already gets an extra `pad` via `x1`; the
    // top mirrors that with an extra `pad` added to `y1` so the four sides
    // contribute the same structural padding.
    let (vis_top, vis_bot) = visual_extent(&b.ink);
    let gap = 3.0 * rule;
    let eff_h = vis_top.max(b.h) + gap;
    let eff_d = (-vis_bot).max(b.d) + gap;
    // The frame is offset by `margin` on every side so the rule sits inside
    // the SVG's viewBox with breathing room rather than touching the edge.
    let x0 = margin;
    let x1 = margin + b.w + 2.0 * pad + gap;
    let y0 = -(eff_d + pad);
    let y1 = eff_h + 2.0 * pad;
    let mut out = LBox {
        w: x1 + margin,
        h: eff_h + 2.0 * pad + rule,
        d: eff_d + pad + rule,
        ink: Vec::new(),
    };
    out.ink.push(Ink::R {
        x0,
        y0: y1 - rule,
        x1,
        y1,
    });
    out.ink.push(Ink::R {
        x0,
        y0,
        x1,
        y1: y0 + rule,
    });
    out.ink.push(Ink::R {
        x0,
        y0: y0 + rule,
        x1: x0 + rule,
        y1: y1 - rule,
    });
    out.ink.push(Ink::R {
        x0: x1 - rule,
        y0: y0 + rule,
        x1,
        y1: y1 - rule,
    });
    b.place(margin + pad, pad);
    out.ink.extend(b.ink);
    out
}

/// Reference line (y-up, em) where an accent mark's centre sits over `base`.
/// Lowercase x-height letters keep the mark low; caps and compound bases raise it.
fn accent_ascent(base: &Node, b: &LBox) -> f64 {
    let real = match base {
        Node::Sym(s) if s.t.chars().count() == 1 => {
            let c = s.t.chars().next().unwrap();
            if c.is_ascii_lowercase() && !"bdfhklt".contains(c) {
                0.50
            } else {
                0.72
            }
        }
        _ => b.h,
    };
    (real + 0.22).max(0.68)
}

/// Vector of stroke paths drawing the accent mark centred at `(cx, cy)`.
fn accent_ink(kind: AccentKind, cx: f64, cy: f64, hw: f64) -> Vec<Ink> {
    let sw = 0.045;
    let p = |d: String| Ink::P {
        x: 0.0,
        y: 0.0,
        sf: 1.0,
        sw,
        d,
    };
    match kind {
        AccentKind::Vec => {
            let x2 = cx + hw;
            vec![p(format!(
                "M {:.3},{:.3} L {:.3},{:.3} M {:.3},{:.3} L {:.3},{:.3} L {:.3},{:.3}",
                cx - hw,
                cy,
                x2,
                cy,
                x2 - 0.09,
                cy - 0.055,
                x2,
                cy,
                x2 - 0.09,
                cy + 0.055
            ))]
        }
        AccentKind::Hat => vec![p(format!(
            "M {:.3},{:.3} L {:.3},{:.3} L {:.3},{:.3}",
            cx - 0.8 * hw,
            cy - 0.10,
            cx,
            cy + 0.06,
            cx + 0.8 * hw,
            cy - 0.10
        ))],
        AccentKind::Bar => vec![p(format!(
            "M {:.3},{:.3} L {:.3},{:.3}",
            cx - 0.9 * hw,
            cy,
            cx + 0.9 * hw,
            cy
        ))],
        AccentKind::Overline => vec![p(format!(
            "M {:.3},{:.3} L {:.3},{:.3}",
            cx - hw,
            cy,
            cx + hw,
            cy
        ))],
        AccentKind::Dot => vec![p(format!(
            "M {:.3},{:.3} L {:.3},{:.3}",
            cx,
            cy,
            cx + 0.001,
            cy
        ))],
        AccentKind::Ddot => vec![p(format!(
            "M {:.3},{:.3} L {:.3},{:.3} M {:.3},{:.3} L {:.3},{:.3}",
            cx - 0.055,
            cy,
            cx - 0.054,
            cy,
            cx + 0.055,
            cy,
            cx + 0.056,
            cy
        ))],
        AccentKind::Tilde | AccentKind::Widetilde => {
            let w = if kind == AccentKind::Widetilde {
                hw
            } else {
                0.7 * hw
            };
            vec![p(format!(
                "M {:.3},{:.3} Q {:.3},{:.3} {:.3},{:.3} Q {:.3},{:.3} {:.3},{:.3}",
                cx - w,
                cy,
                cx - 0.5 * w,
                cy + 0.06,
                cx,
                cy,
                cx + 0.5 * w,
                cy - 0.06,
                cx + w,
                cy
            ))]
        }
        AccentKind::Widehat => vec![p(format!(
            "M {:.3},{:.3} L {:.3},{:.3} L {:.3},{:.3}",
            cx - hw,
            cy - 0.10,
            cx,
            cy + 0.06,
            cx + hw,
            cy - 0.10
        ))],
    }
}

fn layout_accent(base: &Node, kind: AccentKind, display: bool) -> LBox {
    let b = layout(base, display);
    let cy = accent_ascent(base, &b);
    let hw = (b.w * 0.5).max(0.12);
    let mut out = LBox {
        w: b.w,
        h: (cy + 0.12).max(b.h),
        d: b.d,
        ink: Vec::new(),
    };
    let mut placed = b.clone();
    placed.place(0.0, 0.0);
    out.ink.extend(placed.ink);
    out.ink.extend(accent_ink(kind, b.w * 0.5, cy, hw));
    out
}

fn layout_sqrt(rad: &Node, deg: Option<&Node>, display: bool) -> LBox {
    let mut r = layout(rad, display);
    let bar_y = r.h + 0.15;
    let sf = (bar_y + 0.17) / 1.0; // radical glyph spans top 0.8 to hook -0.2
    // Radical `√` (U+221A) design box: x 0.072..0.853, y -0.200..0.800.
    let gw = 0.853 * sf;
    let mut out = LBox {
        w: gw + r.w,
        h: bar_y + RULE + 0.02,
        d: r.d.max(0.17),
        ink: Vec::new(),
    };
    out.ink.push(Ink::F {
        x: 0.0,
        y: bar_y + RULE - 0.8 * sf,
        sf,
        d: "M0.095 0.178Q0.089 0.178 0.081 0.186T0.072 0.200T0.103 0.230T0.169 0.280\
             T0.207 0.309Q0.209 0.311 0.212 0.311H0.213Q0.219 0.311 0.227 0.294T0.281 0.177\
             Q0.300 0.134 0.312 0.108L0.397 -0.077Q0.398 -0.077 0.501 0.136T0.707 0.565\
             T0.814 0.786Q0.820 0.800 0.834 0.800Q0.841 0.800 0.846 0.794T0.853 0.782\
             V0.776L0.620 0.293L0.385 -0.193Q0.381 -0.200 0.366 -0.200Q0.357 -0.200 0.354 \
             -0.197Q0.352 -0.195 0.256 0.015L0.160 0.225L0.144 0.214Q0.129 0.202 0.113 \
             0.190T0.095 0.178Z"
            .to_string(),
    });
    out.ink.push(Ink::R {
        x0: gw,
        y0: bar_y,
        x1: gw + r.w,
        y1: bar_y + RULE,
    });
    if let Some(d) = deg {
        let mut db = layout(d, display);
        scale_box(&mut db, 0.5);
        db.place(0.0, bar_y - 0.72 * sf + 0.15);
        out.ink.extend(db.ink);
    }
    r.place(gw, 0.0);
    out.ink.extend(r.ink);
    out
}

/// True when `n` is (or single-element-wraps) a path-drawn big operator.
fn is_bigop(n: &Node) -> bool {
    match n {
        Node::Sym(s) => matches!(
            s.t.chars().next().unwrap_or(' '),
            '∫' | '∬' | '∭' | '∮' | '∑' | '∏' | '∐' | '⋃' | '⋂' | '⨁' | '⨂'
        ),
        Node::Row(v) => v.len() == 1 && is_bigop(&v[0]),
        _ => false,
    }
}

fn layout_script(base: &Node, sub: Option<&Node>, sup: Option<&Node>, display: bool) -> LBox {
    let b = layout(base, display);
    let bigop = is_bigop(base);
    let mut su = sup.map(|n| {
        let mut x = layout(n, display);
        scale_box(&mut x, SCR);
        x
    });
    let sd = sub.map(|n| {
        let mut x = layout(n, display);
        scale_box(&mut x, SCR);
        x
    });
    // Side limits of a big operator hug its design top/bottom (a plain script
    // drops by a fixed amount instead).
    let sup_d = su.as_ref().map(|x| x.d).unwrap_or(0.0);
    let sup_h = su.as_ref().map(|x| x.h).unwrap_or(0.0);
    // The generic box ascent (0.72) overestimates real glyph tops (~0.5), so a
    // small *negative* drop keeps the exponent near the base's top-right corner.
    let sup_y = if bigop { b.h - sup_h + sup_d } else { b.h - 0.05 };
    let mut out = LBox {
        w: b.w,
        h: b.h,
        d: b.d,
        ink: Vec::new(),
    };
    if let Some(s) = &mut su {
        let x = b.w + 0.01;
        s.place(x, sup_y);
        out.w = b.w + 0.01 + s.w;
        out.h = out.h.max(sup_y + s.h);
        out.ink.extend(s.ink.clone());
    }
    if let Some(s) = sd {
        let x = b.w + 0.01;
        let sub_y = if bigop {
            -b.d
        } else {
            -(s.h + 0.08)
        };
        let mut placed = s;
        placed.place(x, sub_y);
        out.w = out.w.max(b.w + 0.01 + placed.w);
        out.d = out.d.max(-sub_y + placed.d);
        out.ink.extend(placed.ink);
    }
    // base ink emitted first
    out.ink.splice(0..0, b.ink);
    out
}

fn layout_overunder(
    base: &Node,
    below: Option<&Node>,
    above: Option<&Node>,
    display: bool,
) -> LBox {
    let b = layout(base, display);
    let mut ab = above.map(|n| {
        let mut x = layout(n, display);
        scale_box(&mut x, SCR);
        x
    });
    let bl = below.map(|n| {
        let mut x = layout(n, display);
        scale_box(&mut x, SCR);
        x
    });
    let mut out = LBox {
        w: b.w,
        h: b.h,
        d: b.d,
        ink: Vec::new(),
    };
    if let Some(a) = &mut ab {
        let x = (b.w - a.w) / 2.0;
        let y = b.h + a.d + 0.20;
        a.place(x, y);
        out.h = y + a.h;
        out.w = out.w.max(a.w);
        out.ink.extend(a.ink.clone());
    }
    if let Some(l) = bl {
        let x = (b.w - l.w) / 2.0;
        let y = -(b.d + l.h + 0.20);
        let mut placed = l;
        placed.place(x, y);
        out.d = out.d.max(-y + placed.d);
        out.w = out.w.max(placed.w);
        out.ink.extend(placed.ink);
    }
    out.ink.splice(0..0, b.ink);
    out
}

fn layout_fence(open: char, inner: &Node, close: char, display: bool) -> LBox {
    let body = layout(inner, display);
    let h = body.h;
    let d = body.d;
    let height = (h + d).max(1.0);
    let centre = (h - d) / 2.0;
    let mut out = LBox {
        w: 0.0,
        h,
        d,
        ink: Vec::new(),
    };
    let mut wl = 0.0;
    let ext = centre + height / 2.0;
    let extd = height / 2.0 - centre;
    if open != '.' {
        if let Some((ink, w)) = stretch_delim(open, centre, height) {
            wl = w + 0.05;
            out.h = out.h.max(ext);
            out.d = out.d.max(extd);
            out.ink.push(ink);
        }
    }
    let bx = wl;
    let mut shifted = body;
    shifted.place(bx, 0.0);
    out.w = bx + shifted.w;
    out.ink.extend(shifted.ink);
    if close != '.' {
        if let Some((ink, w)) = stretch_delim(close, centre, height) {
            let mut placed = ink;
            if let Ink::P { x, .. } = &mut placed {
                *x += out.w + 0.05;
            }
            out.w += 0.05 + w;
            out.h = out.h.max(ext);
            out.d = out.d.max(extd);
            out.ink.push(placed);
        }
    }
    out
}

fn layout_matrix(rows: &[Vec<Node>], open: char, close: char, align: bool, display: bool) -> LBox {
    let mut grid: Vec<Vec<LBox>> = Vec::new();
    let mut cols = 0;
    for row in rows {
        cols = cols.max(row.len());
    }
    if cols == 0 {
        return LBox::empty();
    }
    let mut col_w = vec![0.0_f64; cols];
    let mut max_h: Vec<f64> = Vec::new();
    let mut max_d: Vec<f64> = Vec::new();
    for row in rows {
        let mut cells = Vec::new();
        let mut rh: f64 = 0.0;
        let mut rd: f64 = 0.0;
        for c in row {
            let b = layout(c, display);
            rh = rh.max(b.h);
            rd = rd.max(b.d);
            cells.push(b);
        }
        for (i, b) in cells.iter().enumerate() {
            let i = i.min(cols - 1);
            col_w[i] = col_w[i].max(b.w);
        }
        max_h.push(rh);
        max_d.push(rd);
        grid.push(cells);
    }
    let gap = 0.7;
    let rowgap = 0.35;
    let inner_w: f64 = col_w.iter().sum::<f64>() + gap * (cols.saturating_sub(1)) as f64;
    let n = rows.len();
    // Row baselines, stacking bottom-to-top from the first row's baseline.
    let mut y = vec![0.0_f64; n];
    for i in 1..n {
        y[i] = y[i - 1] - (max_d[i - 1] + rowgap + max_h[i]);
    }
    let top = max_h[0];
    let bottom = -(y[n - 1] - max_d[n - 1]);
    let height = (top + bottom).max(1.0);
    let centre = (top - bottom) / 2.0;
    // In TeX, atoms in a row align on the math axis (AXIS above baseline),
    // not on the baseline itself. For ordinary atoms the axis sits at AXIS
    // above the baseline; for a matrix it sits at the visual centre. Shift
    // the matrix up so its centre lands on the row's axis line, which keeps
    // `A_{m,n} = (matrix)` vertically centred with the matrix body.
    let shift = AXIS - centre;
    let mut out = LBox {
        w: 0.0,
        h: top + shift,
        d: bottom - shift,
        ink: Vec::new(),
    };
    let mut x_off = 0.0;
    if open != '.' {
        if let Some((mut ink, w)) = stretch_delim(open, centre, height) {
            // Lift the opening paren to match the shifted content.
            if let Ink::P { y, .. } = &mut ink {
                *y += shift;
            }
            out.ink.push(ink);
            x_off = w + 0.05;
        }
    }
    for (ri, cells) in grid.iter().enumerate() {
        let mut x = x_off;
        for (ci, b) in cells.iter().enumerate() {
            let cw = col_w[ci.min(cols - 1)];
            let mut placed = b.clone();
            // `align` environments line the cells up on the `&`: columns run
            // right-aligned / left-aligned alternately (TeX's rl columns).
            let dx = if align {
                if ci % 2 == 0 {
                    cw - b.w
                } else {
                    0.0
                }
            } else {
                (cw - b.w) / 2.0
            };
            placed.place(x + dx, y[ri] + shift);
            out.ink.extend(placed.ink);
            x += cw + gap;
        }
    }
    let mut right = x_off + inner_w;
    if close != '.' {
        if let Some((ink, w)) = stretch_delim(close, centre, height) {
            let mut placed = ink;
            // Shift the closing paren up to match the rest of the matrix.
            if let Ink::P { y, .. } = &mut placed {
                *y += shift;
            }
            if let Ink::P { x, .. } = &mut placed {
                *x += right + 0.05;
            }
            out.ink.push(placed);
            right += 0.05 + w;
        }
    }
    out.w = right;
    out
}

// ---------------------------------------------------------------------------
// MathML (accessibility) generation
// ---------------------------------------------------------------------------

fn to_mathml(node: &Node, display: bool) -> String {
    let body = mathml_frag(node);
    let disp = if display { "block" } else { "inline" };
    format!("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"{disp}\">{body}</math>")
}

fn mathml_frag(node: &Node) -> String {
    match node {
        Node::Row(children) => children.iter().map(mathml_frag).collect(),
        Node::Frac(n, d) => format!(
            "<mfrac>{}{}</mfrac>",
            wrap(&mathml_frag(n)),
            wrap(&mathml_frag(d))
        ),
        Node::CFrac(n, d) => format!(
            "<mfrac>{}{}</mfrac>",
            wrap(&mathml_frag(n)),
            wrap(&mathml_frag(d))
        ),
        Node::Boxed(inner) => format!(
            "<menclose notation=\"box\">{}</menclose>",
            wrap(&mathml_frag(inner))
        ),
        Node::Accent { base, kind } => format!(
            "<mover accent=\"true\">{}{}</mover>",
            wrap(&mathml_frag(base)),
            format!("<mo>{}</mo>", esc(&accent_mo(*kind)))
        ),
        Node::Sqrt {
            rad,
            deg: Some(idx),
        } => format!(
            "<mroot>{}{}</mroot>",
            wrap(&mathml_frag(rad)),
            wrap(&mathml_frag(idx))
        ),
        Node::Sqrt { rad, deg: None } => {
            format!("<msqrt>{}</msqrt>", wrap(&mathml_frag(rad)))
        }
        Node::Script { base, sub, sup } => {
            let b = wrap(&mathml_frag(base));
            match (sub, sup) {
                (None, Some(s)) => format!("<msup>{b}{}</msup>", wrap(&mathml_frag(s))),
                (Some(s), None) => format!("<msub>{b}{}</msub>", wrap(&mathml_frag(s))),
                (Some(s), Some(u)) => format!(
                    "<msubsup>{b}{}{}</msubsup>",
                    wrap(&mathml_frag(s)),
                    wrap(&mathml_frag(u))
                ),
                (None, None) => b,
            }
        }
        Node::OverUnder { base, below, above } => {
            let b = wrap(&mathml_frag(base));
            match (below, above) {
                (Some(l), None) => {
                    format!("<munder>{b}{}</munder>", wrap(&mathml_frag(l)))
                }
                (None, Some(a)) => format!("<mover>{b}{}</mover>", wrap(&mathml_frag(a))),
                (Some(l), Some(a)) => format!(
                    "<munderover>{b}{}{}</munderover>",
                    wrap(&mathml_frag(l)),
                    wrap(&mathml_frag(a))
                ),
                (None, None) => b,
            }
        }
        Node::Sym(s) => sym_mathml(s),
        Node::Fence { open, inner, close } => {
            let mid = mathml_frag(inner);
            let mut out = String::new();
            if *open != '.' {
                out.push_str(&format!(
                    "<mo stretchy=\"true\">{}</mo>",
                    esc(&open.to_string())
                ));
            }
            out.push_str(&mid);
            if *close != '.' {
                out.push_str(&format!(
                    "<mo stretchy=\"true\">{}</mo>",
                    esc(&close.to_string())
                ));
            }
            out
        }
        Node::Matrix {
            rows,
            open,
            close,
            ..
        } => {
            let mut out = String::new();
            if *open != '.' {
                out.push_str(&format!(
                    "<mo stretchy=\"true\">{}</mo>",
                    esc(&open.to_string())
                ));
            }
            out.push_str("<mtable>");
            for row in rows {
                if row.is_empty() {
                    continue;
                }
                out.push_str("<mtr>");
                for (i, c) in row.iter().enumerate() {
                    if i > 0 {
                        out.push_str("</mtd>");
                    }
                    out.push_str("<mtd>");
                    out.push_str(&mathml_frag(c));
                }
                out.push_str("</mtd></mtr>");
            }
            out.push_str("</mtable>");
            if *close != '.' {
                out.push_str(&format!(
                    "<mo stretchy=\"true\">{}</mo>",
                    esc(&close.to_string())
                ));
            }
            out
        }
        Node::Space(w) => format!("<mspace width=\"{}\"/>", fmt(*w)),
    }
}

fn sym_mathml(s: &Sym) -> String {
    let t = &s.t;
    if t.chars().all(|c| c.is_ascii_digit()) {
        return format!("<mn>{}</mn>", esc(t));
    }
    if s.italic {
        return t
            .chars()
            .map(|c| format!("<mi mathvariant=\"italic\">{}</mi>", esc(&c.to_string())))
            .collect();
    }
    match s.cls {
        Class::Op => {
            let ch = t.chars().next().unwrap_or(' ');
            let (att, mov) = if movable_op(t) {
                (
                    "largeop=\"true\" symmetric=\"true\" movablelimits=\"true\"",
                    "",
                )
            } else if bigop_char(ch) {
                (
                    "largeop=\"true\" symmetric=\"true\" movablelimits=\"false\"",
                    "",
                )
            } else {
                ("", "")
            };
            format!(
                "<mo {}{}>{}</mo>",
                att,
                if mov.is_empty() { "" } else { mov },
                esc(t)
            )
        }
        Class::Bin | Class::Rel | Class::Open | Class::Close | Class::Punct => {
            format!("<mo>{}</mo>", esc(t))
        }
        Class::Ord => format!("<mi mathvariant=\"normal\">{}</mi>", esc(t)),
    }
}

fn bigop_char(ch: char) -> bool {
    matches!(ch, '∫' | '∬' | '∭' | '∮')
}

/// Wrap a fragment in `<mrow>` when it holds more than one top-level element.
fn wrap(frag: &str) -> String {
    if top_level_count(frag) > 1 {
        format!("<mrow>{frag}</mrow>")
    } else {
        frag.to_string()
    }
}

fn top_level_count(html: &str) -> usize {
    let mut count = 0;
    let mut depth: usize = 0;
    let mut i = 0;
    let b = html.as_bytes();
    while i < b.len() {
        if b[i] == b'<' {
            let close = html[i..].find('>').map(|p| i + p).unwrap_or(b.len() - 1);
            let tag = &html[i + 1..close];
            let closing = tag.starts_with('/');
            let self_closing = tag.ends_with('/');
            let comment = tag.starts_with('!');
            if depth == 0 && !closing && !self_closing && !comment {
                count += 1;
            }
            if closing {
                depth = depth.saturating_sub(1);
            } else if !self_closing && !comment {
                depth += 1;
            }
            i = close + 1;
        } else {
            let start = i;
            while i < b.len() && b[i] != b'<' {
                i += 1;
            }
            if depth == 0 && !html[start..i].trim().is_empty() {
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Symbol tables
// ---------------------------------------------------------------------------

fn bigop(name: &str) -> Option<String> {
    match name {
        "sum" => Some("∑".to_string()),
        "prod" => Some("∏".to_string()),
        "coprod" => Some("∐".to_string()),
        "bigcup" => Some("⋃".to_string()),
        "bigcap" => Some("⋂".to_string()),
        "bigoplus" => Some("⨁".to_string()),
        "bigotimes" => Some("⨂".to_string()),
        "int" => Some("∫".to_string()),
        "iint" => Some("∬".to_string()),
        "iiint" => Some("∭".to_string()),
        "oint" => Some("∮".to_string()),
        _ => None,
    }
}

fn greek(name: &str) -> Option<char> {
    const GREEK: &[(&str, char)] = &[
        ("alpha", 'α'),
        ("beta", 'β'),
        ("gamma", 'γ'),
        ("delta", 'δ'),
        ("epsilon", 'ϵ'),
        ("varepsilon", 'ε'),
        ("zeta", 'ζ'),
        ("eta", 'η'),
        ("theta", 'θ'),
        ("vartheta", 'ϑ'),
        ("iota", 'ι'),
        ("kappa", 'κ'),
        ("lambda", 'λ'),
        ("mu", 'μ'),
        ("nu", 'ν'),
        ("xi", 'ξ'),
        ("omicron", 'ο'),
        ("pi", 'π'),
        ("varpi", 'ϖ'),
        ("rho", 'ρ'),
        ("varrho", 'ϱ'),
        ("sigma", 'σ'),
        ("varsigma", 'ς'),
        ("tau", 'τ'),
        ("upsilon", 'υ'),
        ("phi", 'ϕ'),
        ("varphi", 'φ'),
        ("chi", 'χ'),
        ("psi", 'ψ'),
        ("omega", 'ω'),
        ("Gamma", 'Γ'),
        ("Delta", 'Δ'),
        ("Theta", 'Θ'),
        ("Lambda", 'Λ'),
        ("Xi", 'Ξ'),
        ("Pi", 'Π'),
        ("Sigma", 'Σ'),
        ("Upsilon", 'Υ'),
        ("Phi", 'Φ'),
        ("Psi", 'Ψ'),
        ("Omega", 'Ω'),
        ("digamma", 'ϝ'),
        ("varkappa", 'ϰ'),
    ];
    GREEK.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

fn look(name: &str) -> Option<String> {
    const DIRECT: &[(&str, &str)] = &[
        ("cdot", "⋅"),
        ("times", "×"),
        ("div", "÷"),
        ("pm", "±"),
        ("mp", "∓"),
        ("ast", "∗"),
        ("circ", "∘"),
        ("bullet", "∙"),
        ("colon", ":"),
        ("neq", "≠"),
        ("ne", "≠"),
        ("leq", "≤"),
        ("le", "≤"),
        ("geq", "≥"),
        ("ge", "≥"),
        ("approx", "≈"),
        ("equiv", "≡"),
        ("propto", "∝"),
        ("sim", "∼"),
        ("simeq", "≃"),
        ("cong", "≅"),
        ("ll", "≪"),
        ("gg", "≫"),
        ("perp", "⊥"),
        ("parallel", "∥"),
        ("mid", "∣"),
        ("in", "∈"),
        ("notin", "∉"),
        ("ni", "∋"),
        ("subset", "⊂"),
        ("supset", "⊃"),
        ("subseteq", "⊆"),
        ("supseteq", "⊇"),
        ("cup", "∪"),
        ("cap", "∩"),
        ("setminus", "∖"),
        ("oplus", "⊕"),
        ("otimes", "⊗"),
        ("to", "→"),
        ("rightarrow", "→"),
        ("leftarrow", "←"),
        ("leftrightarrow", "↔"),
        ("uparrow", "↑"),
        ("downarrow", "↓"),
        ("mapsto", "↦"),
        ("Rightarrow", "⇒"),
        ("implies", "⇒"),
        ("Leftarrow", "⇐"),
        ("Leftrightarrow", "⇔"),
        ("land", "∧"),
        ("wedge", "∧"),
        ("lor", "∨"),
        ("vee", "∨"),
        ("neg", "¬"),
        ("not", "¬"),
        ("forall", "∀"),
        ("exists", "∃"),
        ("emptyset", "∅"),
        ("infty", "∞"),
        ("partial", "∂"),
        ("nabla", "∇"),
        ("prime", "′"),
        ("degree", "°"),
        ("angle", "∠"),
        ("triangle", "△"),
        ("ldots", "…"),
        ("dots", "…"),
        ("cdots", "⋯"),
        ("vdots", "⋮"),
        ("ddots", "⋱"),
        ("star", "⋆"),
        ("checkmark", "✓"),
        ("hbar", "ℏ"),
        ("ell", "ℓ"),
        ("Re", "ℜ"),
        ("Im", "ℑ"),
        ("aleph", "ℵ"),
        ("wp", "℘"),
        ("top", "⊤"),
        ("bot", "⊥"),
    ];
    DIRECT
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

const MATH_FUNCS: [&str; 18] = [
    "sin", "cos", "tan", "cot", "sec", "csc", "sinh", "cosh", "tanh", "coth", "arcsin", "arccos",
    "arctan", "log", "ln", "exp", "lim", "det",
];
const MATHVARIANTS: [&str; 7] = ["mathbf", "mathbb", "mathcal", "mathsf", "mathtt", "mathit", "mathfrak"];

/// Combining char used in the MathML `<mover>` for each accent kind.
fn accent_mo(kind: AccentKind) -> &'static str {
    match kind {
        AccentKind::Vec => "⃗",
        AccentKind::Hat => "̂",
        AccentKind::Bar => "̄",
        AccentKind::Overline => "̅",
        AccentKind::Dot => "̇",
        AccentKind::Ddot => "̈",
        AccentKind::Tilde | AccentKind::Widetilde => "̃",
        AccentKind::Widehat => "̂",
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn esc(s: &str) -> String {
    escape_html(s)
}

fn esc_attr(s: &str) -> String {
    escape_html(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_identifiers_and_digits() {
        let out = render("x");
        assert!(out.starts_with("<svg "), "output must be an SVG: {out}");
        assert!(out.contains("aria-label=\"x\""));
        assert!(out.contains("<text x=\"0\" y=\"0.72\" font-size=\"1\" font-style=\"italic\" text-anchor=\"start\">x</text>"));
        let sub = render("x^2");
        assert!(sub.contains(
            "<text x=\"0.55\" y=\"0.504\" font-size=\"0.7\" text-anchor=\"start\">2</text>"
        ));
    }

    #[test]
    fn renders_fraction_with_bar() {
        let out = render("\\frac{a}{b}");
        assert!(
            out.contains("<desc><math"),
            "MathML a11y must be embedded: {out}"
        );
        assert!(out.contains(
            "<mfrac><mi mathvariant=\"italic\">a</mi><mi mathvariant=\"italic\">b</mi></mfrac>"
        ));
        assert!(
            out.contains("<rect x=\"0\" y=\"0.94\" width=\"0.54\" height=\"0.04\"/>"),
            "fraction bar: {out}"
        );
    }

    #[test]
    fn renders_sqrt_and_mroot() {
        let out = render("\\sqrt{x}");
        assert!(out.contains("<msqrt>"));
        assert!(
            out.contains("<path d=\"M0.095 0.178Q0.089 0.178"),
            "radical is drawn as a filled U+221A glyph: {out}"
        );
        let n = render("\\sqrt[n]{x}");
        assert!(n.contains("<mroot>"));
    }

    #[test]
    fn renders_sum_with_limits() {
        let inline = render("\\sum_{i=1}^{n} i");
        assert!(
            inline.contains("<msubsup>"),
            "inline `\\sum` uses side limits: {inline}"
        );
        let block = render_block("\\sum_{i=1}^{n} i");
        assert!(
            block.contains("<munderover>"),
            "display `\\sum` uses under/over: {block}"
        );
        let sub_only = render_block("\\sum_{i} x");
        assert!(
            sub_only.contains("<munder>"),
            "display sub-only: {sub_only}"
        );
        let sup_only = render_block("\\sum^{n} x");
        assert!(sup_only.contains("<mover>"), "display sup-only: {sup_only}");
    }

    #[test]
    fn integral_stays_side_limits() {
        let block = render_block("\\int_0^{\\infty}");
        assert!(
            block.contains("<msubsup>"),
            "`\\int` is non-movable: {block}"
        );
    }

    #[test]
    fn renders_overset_and_forced_limits() {
        let out = render("\\gamma \\overset{\\text{def}}{=}");
        assert!(out.contains("<mover><mo>=</mo>"));
        let lim = render("\\lim\\limits_{n \\to \\infty}");
        assert!(
            lim.contains("<munder>"),
            "forced `\\limits` stacks below: {lim}"
        );
    }

    #[test]
    fn renders_matrix() {
        let out = render("\\begin{pmatrix} 1 & 2 \\\\ 3 & 4 \\end{pmatrix}");
        assert!(out.contains("<mtable>"));
        assert!(out.contains("<mtr><mtd><mn>1</mn></mtd><mtd><mn>2</mn></mtd></mtr>"));
        assert!(out.contains("</mtable>"));
        assert!(out.contains("stretchy=\"true\">(</mo>"));
    }

    #[test]
    fn renders_boxed() {
        let out = render("\\boxed{x+y}");
        assert!(
            out.contains("<menclose notation=\"box\">"),
            "boxed content must be marked as menclose: {out}"
        );
        assert!(
            out.contains("</menclose>"),
            "boxed must close the menclose element: {out}"
        );
    }

    #[test]
    fn boxed_frame_stays_inside_viewbox() {
        let out = render_block("\\boxed{ \\int\\limits_{-\\infty}^{\\infty} e^{-x^2} \\, dx = \\sqrt{\\pi} }");
        for attr in out.match_indices("<rect") {
            let rect = &out[attr.0..out[attr.0..].find("/>").unwrap() + attr.0 + 2];
            let x = rect
                .split("x=\"").nth(1).and_then(|s| s.split('"').next())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap();
            let w = rect
                .split("width=\"").nth(1).and_then(|s| s.split('"').next())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap();
            assert!(x >= -0.001, "frame rect left edge must not be negative: {rect}");
            assert!(
                x + w <= 8.35,
                "frame rect must stay within viewBox width: {rect}"
            );
        }
        let w = out
            .split("viewBox=\"0 0 ")
            .nth(1)
            .and_then(|s| s.split(' ').next())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap();
        assert!((w - 8.523).abs() < 0.01, "viewBox width: {w}");
    }

    #[test]
    fn boxed_glyphs_stay_inside_frame() {
        // The frame must lie *outside* every glyph's visual top/bottom, so
        // tall upper-limit glyphs (e.g. \int\limits upper limit) don't poke
        // through the box's top edge.
        let out = render_block("\\boxed{ \\int\\limits_{-\\infty}^{\\infty} e^{-x^2} \\, dx = \\sqrt{\\pi} }");
        let vb_h: f64 = out
            .split("viewBox=\"0 0 ").nth(1)
            .and_then(|s| s.split('"').next())
            .and_then(|s| s.split(' ').nth(1))
            .and_then(|s| s.parse().ok())
            .unwrap();
        for m in out.match_indices("<text ") {
            let tag_end = m.0 + out[m.0..].find('>').unwrap() + 1;
            let tag = &out[m.0..tag_end];
            let y_svg = match tag.split("y=\"").nth(1).and_then(|s| s.split('"').next()).and_then(|s| s.parse::<f64>().ok()) {
                Some(v) => v,
                None => continue,
            };
            let s = match tag.split("font-size=\"").nth(1).and_then(|s| s.split('"').next()).and_then(|s| s.parse::<f64>().ok()) {
                Some(v) => v,
                None => continue,
            };
            let vis_top = y_svg - 0.8 * s;
            let vis_bot = y_svg + 0.2 * s;
            assert!(
                vis_top >= 0.149,
                "glyph visual top ({vis_top}) must be at or below the box top edge (0.15): \
                 {tag}\nfull svg: {out}"
            );
            assert!(
                vis_top >= 0.18,
                "glyph visual top ({vis_top}) should leave at least 0.18 of breathing room \
                 above the box top edge (was tightened for the increased padding): \
                 {tag}\nfull svg: {out}"
            );
            assert!(
                vis_bot <= vb_h - 0.149,
                "glyph visual bottom ({vis_bot}) must be above the box bottom edge ({vb_h}): \
                 {tag}\nfull svg: {out}"
            );
        }
    }

    #[test]
    fn renders_align_rows() {
        let out = render(
            "\\begin{align*} y &= x^2 \\\\ z &= x + 1 \\end{align*}",
        );
        assert!(out.contains("<mtable>"), "align maps to an mtable: {out}");
        assert!(
            out.contains("<mtr><mtd><mi mathvariant=\"italic\">y</mi></mtd>"),
            "first align row, first cell: {out}"
        );
        assert!(
            out.matches("<mtr>").count() == 2,
            "one <mtr> per align row: {out}"
        );
    }

    #[test]
    fn renders_cfrac() {
        let out = render("\\cfrac{1}{1+\\cfrac{1}{2}}");
        assert!(
            out.matches("<mfrac>").count() == 2,
            "each \\cfrac becomes an mfrac: {out}"
        );
    }

    #[test]
    fn deep_cfrac_viewbox_holds_all_ink() {
        // A 5-level continued fraction: each level adds its own denominator
        // height, so the total descent is several em. The viewBox height
        // (b.h + b.d) must be at least as tall as the deepest ink pixel,
        // otherwise the bottom of the formula gets clipped against the
        // viewBox edge.
        let out = render_block(
            "e = 2 + \\cfrac{1}{1 + \\cfrac{1}{2 + \\cfrac{2}{3 + \\cfrac{3}{4 + \\cfrac{4}{\\ldots}}}}}",
        );
        let vb_h: f64 = out
            .split("viewBox=\"0 0 ")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .and_then(|s| s.split(' ').nth(1))
            .and_then(|s| s.parse().ok())
            .unwrap();
        let mut max_y = 0.0_f64;
        for m in out.match_indices(" y=\"") {
            let rest = &out[m.0 + 4..];
            if let Some(end) = rest.find('"') {
                if let Ok(v) = rest[..end].parse::<f64>() {
                    if v > max_y {
                        max_y = v;
                    }
                }
            }
        }
        assert!(
            max_y <= vb_h + 0.001,
            "deepest ink y={max_y} must be inside viewBox height {vb_h} ({out})"
        );
        // The viewBox must also leave at least a small margin of breathing
        // room below the deepest ink (no flush-against-the-edge clipping).
        assert!(
            max_y + 0.05 <= vb_h,
            "viewBox ({vb_h}) must leave ≥0.05em below deepest ink y={max_y}"
        );
    }

    #[test]
    fn renders_fences_stretchy() {
        let out = render("\\left( \\frac{1}{2} \\right)");
        assert!(out.contains("stretchy=\"true\">(</mo>"));
        assert!(out.contains("stretchy=\"true\">)</mo>"));
        // invisible delimiters `\left.` / `\right.` emit no fence
        let dot = render("\\left. x \\right.");
        assert!(!dot.contains("stretchy=\"true\">(</mo>"));
    }

    #[test]
    fn display_block_sets_display_attr_and_no_vertical_align() {
        let block = render_block("\\frac{1}{2}");
        assert!(block.contains("display=\"block\""));
        assert!(!block.contains("vertical-align:-"));
        let inline = render("\\frac{1}{2}");
        assert!(inline.contains("display=\"inline\""));
        assert!(inline.contains("vertical-align:-"));
    }

    #[test]
    fn empty_input_renders_nothing() {
        assert_eq!(render(""), "");
        assert_eq!(render("   "), "");
        assert_eq!(render_block(""), "");
    }

    #[test]
    fn degrades_unknown_command() {
        let out = render("\\madeupcommand{x}");
        assert!(
            out.contains("\\madeupcommand"),
            "unknown command kept literally: {out}"
        );
    }

    #[test]
    fn escapes_markup_in_aria_label() {
        let out = render("<script>&");
        assert!(!out.contains("<script>"));
        assert!(out.contains("&lt;script&gt;&amp;"));
    }

    #[test]
    fn renders_accent_marks() {
        let vec = render("\\vec{R}");
        assert!(
            vec.contains("<mover accent=\"true\"><mi mathvariant=\"italic\">R</mi><mo>⃗</mo></mover>"),
            "vec mathml: {vec}"
        );
        assert!(vec.contains("<path"), "vec draws a mark: {vec}");
        let hat = render("\\hat{x}");
        assert!(hat.contains("<mover accent=\"true\"><mi mathvariant=\"italic\">x</mi><mo>̂</mo></mover>"));
        let over = render("\\overline{AB}");
        assert!(over.contains("<mover accent=\"true\"><mrow>"));
        let sub = render("\\vec{R}_0");
        assert!(sub.contains("<msub>"));
    }

    #[test]
    fn renders_fraktur_variant() {
        let out = render("\\mathfrak{m}");
        assert!(
            out.contains("<text x=\"0\" y=\"0.72\" font-size=\"1\" text-anchor=\"start\">m</text>"),
            "fraktur maps to plain letter: {out}"
        );
        assert!(!out.contains("font-style=\"italic\""));
        let many = render("\\mathfrak{moments}");
        assert!(many.contains(">moments<"));
    }
}
