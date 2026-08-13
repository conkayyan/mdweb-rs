//! The LaTeX `picture` environment.
//!
//! `\put(x,y){obj}` places an object at a point measured in `\unitlength`
//! (1 pt unless the source says otherwise). Supported objects are `\line`,
//! `\vector`, `\circle`, `\circle*`, `\framebox`, `\makebox`, `\dashbox` and
//! plain text or math, plus the `\multiput` and `\qbezier` shorthands.
//!
//! Note LaTeX's slope convention: `\line(dx,dy){len}` takes `len` as the
//! **horizontal** extent, except for a vertical line (`dx = 0`) where it is
//! the vertical one.
//!
//! Sizing follows LaTeX's own typesetting: `\unitlength=1pt` means a TeX
//! point, and LaTeX sets a 10 pt body (1 em = 10 pt), so the default unit
//! and any `pt` length map to `1/10 em` — see
//! [`super::PIC_EM_PER_PT`].

use super::expr::{eval, eval_in, Vars};
use super::scan::Scanner;
use super::{
    label, width_from_pt, Anchor, Arrow, ArrowKind, Canvas, Color, Item, PathOp, Pt, Stroke,
    PIC_EM_PER_CM, PIC_EM_PER_PT,
};

struct Picture {
    canvas: Canvas,
    /// `\unitlength` in em.
    unit: f64,
    width: f64,
    vars: Vars,
}

pub(crate) fn render(src: &str) -> Option<Canvas> {
    let mut p = Picture {
        canvas: Canvas::default(),
        unit: PIC_EM_PER_PT,
        width: width_from_pt(0.4),
        vars: Vars::new(),
    };
    p.unit = unitlength(src).unwrap_or(PIC_EM_PER_PT);
    p.run(src);
    if p.canvas.is_empty() {
        None
    } else {
        Some(p.canvas)
    }
}

/// `\setlength{\unitlength}{1mm}` or `\unitlength=1mm`, in em.
///
/// Lengths are TeX physical lengths evaluated on the `picture` backend's
/// own basis ([`super::PIC_EM_PER_CM`]), so `1pt` lands on exactly
/// [`super::PIC_EM_PER_PT`] and `1em` stays exactly one em.
fn unitlength(src: &str) -> Option<f64> {
    let rest = if let Some(i) = src.find("\\setlength{\\unitlength}") {
        let mut s = Scanner::new(&src[i + "\\setlength{\\unitlength}".len()..]);
        s.group()?
    } else if let Some(i) = src.find("\\unitlength") {
        let tail = &src[i + "\\unitlength".len()..];
        let tail = tail.trim_start().trim_start_matches('=');
        tail.lines().next()?.to_string()
    } else {
        return None;
    };
    // `eval_in` normalises lengths to centimetres on the picture basis.
    let cm = eval_in(&rest, &Vars::new(), PIC_EM_PER_CM)?;
    if cm > 0.0 {
        Some(cm * PIC_EM_PER_CM)
    } else {
        None
    }
}

impl Picture {
    fn pt(&self, x: f64, y: f64) -> Pt {
        (x * self.unit, y * self.unit)
    }

    fn stroke(&self) -> Stroke {
        Stroke {
            width: self.width,
            ..Stroke::default()
        }
    }

    fn run(&mut self, src: &str) {
        let mut s = Scanner::new(src);
        // Walk to `\begin{picture}` and drop its size/offset arguments.
        while !s.eof() {
            if s.peek() == Some('\\') {
                let save_cmd = s.command();
                if save_cmd.as_deref() == Some("begin") && s.group().as_deref() == Some("picture") {
                    s.paren();
                    s.paren();
                    break;
                }
            } else {
                s.bump();
            }
        }
        while !s.eof() {
            let Some(cmd) = s.command() else {
                s.bump();
                continue;
            };
            match cmd.as_str() {
                "end" => break,
                "put" => {
                    let at = self.coord(&mut s);
                    let body = s.group().unwrap_or_default();
                    if let Some(at) = at {
                        self.object(&body, at);
                    }
                }
                "multiput" => {
                    let at = self.coord(&mut s);
                    let step = self.coord(&mut s);
                    let n = s
                        .group()
                        .and_then(|g| eval(&g, &self.vars))
                        .unwrap_or(0.0)
                        .max(0.0)
                        .min(200.0) as usize;
                    let body = s.group().unwrap_or_default();
                    if let (Some(at), Some(step)) = (at, step) {
                        for k in 0..n {
                            let p = (at.0 + step.0 * k as f64, at.1 + step.1 * k as f64);
                            self.object(&body, p);
                        }
                    }
                }
                "qbezier" => {
                    s.bracket();
                    let (a, b, c) = (self.coord(&mut s), self.coord(&mut s), self.coord(&mut s));
                    if let (Some(a), Some(b), Some(c)) = (a, b, c) {
                        // Quadratic → cubic: the control points sit two
                        // thirds of the way to the quadratic control point.
                        let lift = |p: Pt, q: Pt| {
                            (p.0 + 2.0 / 3.0 * (q.0 - p.0), p.1 + 2.0 / 3.0 * (q.1 - p.1))
                        };
                        self.canvas.push(Item::Path {
                            ops: vec![
                                PathOp::Move(a),
                                PathOp::Bezier {
                                    c1: lift(a, b),
                                    c2: lift(c, b),
                                    to: c,
                                },
                            ],
                            stroke: Some(self.stroke()),
                            fill: None,
                            arrow: Arrow::default(),
                        });
                    }
                }
                "thicklines" => self.width = width_from_pt(0.8),
                "thinlines" => self.width = width_from_pt(0.4),
                "linethickness" => {
                    if let Some(g) = s.group() {
                        if let Some(cm) = eval_in(&g, &self.vars, PIC_EM_PER_CM) {
                            self.width = (cm * PIC_EM_PER_CM).max(0.015);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// `(x,y)` in `\unitlength`, converted to em.
    fn coord(&self, s: &mut Scanner) -> Option<Pt> {
        let body = s.paren()?;
        let (a, b) = body.split_once(',')?;
        let x = eval_plain(a, &self.vars)?;
        let y = eval_plain(b, &self.vars)?;
        Some(self.pt(x, y))
    }

    /// One `\put` payload placed with its reference point at `at`.
    fn object(&mut self, body: &str, at: Pt) {
        let mut s = Scanner::new(body);
        let Some(cmd) = s.command() else {
            self.text(body, at);
            return;
        };
        match cmd.as_str() {
            "line" | "vector" => {
                let Some(dir) = s.paren() else { return };
                let Some((dx, dy)) = dir.split_once(',') else {
                    return;
                };
                let (dx, dy) = (
                    eval_plain(dx, &self.vars).unwrap_or(0.0),
                    eval_plain(dy, &self.vars).unwrap_or(0.0),
                );
                let len = s
                    .group()
                    .and_then(|g| eval_plain(&g, &self.vars))
                    .unwrap_or(0.0);
                let to = if dx == 0.0 {
                    (at.0, at.1 + len * self.unit * dy.signum())
                } else {
                    (
                        at.0 + len * self.unit * dx.signum(),
                        at.1 + len * self.unit * dy / dx.abs(),
                    )
                };
                self.canvas.push(Item::Path {
                    ops: vec![PathOp::Move(at), PathOp::Line(to)],
                    stroke: Some(self.stroke()),
                    fill: None,
                    arrow: Arrow {
                        end: cmd == "vector",
                        kind: ArrowKind::To,
                        ..Arrow::default()
                    },
                });
            }
            "circle" => {
                let filled = s.eat('*');
                let d = s
                    .group()
                    .and_then(|g| eval_plain(&g, &self.vars))
                    .unwrap_or(0.0);
                let r = d * self.unit / 2.0;
                if r <= 0.0 {
                    return;
                }
                self.canvas.push(Item::Circle {
                    c: at,
                    r,
                    stroke: if filled { None } else { Some(self.stroke()) },
                    fill: if filled { Some(Color::Current) } else { None },
                });
            }
            "framebox" | "makebox" | "dashbox" => {
                if cmd == "dashbox" {
                    s.group();
                }
                let size = s.paren().and_then(|b| {
                    let (w, h) = b.split_once(',')?;
                    Some((
                        eval_plain(w, &self.vars)? * self.unit,
                        eval_plain(h, &self.vars)? * self.unit,
                    ))
                });
                s.bracket();
                let text = s.group().unwrap_or_default();
                if let Some((w, h)) = size {
                    if cmd != "makebox" {
                        let (x0, y0) = at;
                        self.canvas.push(Item::Path {
                            ops: vec![
                                PathOp::Move((x0, y0)),
                                PathOp::Line((x0 + w, y0)),
                                PathOp::Line((x0 + w, y0 + h)),
                                PathOp::Line((x0, y0 + h)),
                                PathOp::Close,
                            ],
                            stroke: Some(Stroke {
                                dash: if cmd == "dashbox" {
                                    super::Dash::Dashed
                                } else {
                                    super::Dash::Solid
                                },
                                ..self.stroke()
                            }),
                            fill: None,
                            arrow: Arrow::default(),
                        });
                    }
                    if let Some(ts) = label(&text) {
                        self.canvas.push(Item::Label {
                            at: (at.0 + w / 2.0, at.1 + h / 2.0),
                            anchor: Anchor::Center,
                            ts,
                            gap: 0.0,
                            color: Color::Current,
                        });
                    }
                } else {
                    self.text(&text, at);
                }
            }
            _ => self.text(body, at),
        }
    }

    /// Bare text or math: LaTeX pins its baseline's left end to the point.
    fn text(&mut self, body: &str, at: Pt) {
        if let Some(ts) = label(body) {
            self.canvas.push(Item::Label {
                at,
                anchor: Anchor::BaseWest,
                ts,
                gap: 0.0,
                color: Color::Current,
            });
        }
    }
}

/// `picture` numbers are multiples of `\unitlength`, so a bare `50` must stay
/// 50 — `eval`'s centimetre normalisation only applies to explicit units.
fn eval_plain(src: &str, vars: &Vars) -> Option<f64> {
    eval(src.trim(), vars)
}

#[cfg(test)]
mod tests {
    use super::super::{render as draw, Kind};
    use super::*;

    const JACKSON: &str = r"\begin{picture}(76,20)
\put(0,0){$A$}
\put(69,0){$B$}
\put(14,3){\line(1,0){50}}
\put(39,3){\vector(0,1){15}}
\put(14,3){\circle*{2}}
\put(64,3){\circle*{2}}
\end{picture}";

    #[test]
    fn renders_the_classic_picture_example() {
        let out = draw(JACKSON).expect("svg");
        assert!(out.contains("aria-label=\"LaTeX picture\""), "{out}");
        assert_eq!(out.matches("<circle").count(), 2, "two dots: {out}");
        assert!(
            out.contains("<text"),
            "the A and B labels are typeset: {out}"
        );
        // 76 pt wide at 1 pt = 1/10 em (LaTeX's 10 pt basis), plus stroke +
        // arrow + padding; labels are hugged to their ink so the canvas
        // carries no phantom margin.
        assert!(out.contains("width=\"7.9"), "sized in em: {out}");
    }

    #[test]
    fn filled_circle_has_no_stroke() {
        let c = render(r"\begin{picture}(4,4)\put(2,2){\circle*{2}}\end{picture}").expect("canvas");
        let out = c.to_svg(Kind::Picture, "src").expect("svg");
        assert!(out.contains("fill=\"currentColor\""), "{out}");
        assert!(!out.contains("stroke=\"currentColor\""), "{out}");
    }

    #[test]
    fn vector_draws_an_arrow_head() {
        let c = render(r"\begin{picture}(4,4)\put(0,0){\vector(0,1){15}}\end{picture}")
            .expect("canvas");
        let out = c.to_svg(Kind::Picture, "src").expect("svg");
        assert_eq!(out.matches("<path").count(), 2, "line plus tip: {out}");
    }

    #[test]
    fn line_length_is_the_horizontal_extent() {
        let c = render(r"\begin{picture}(60,10)\put(0,0){\line(2,1){50}}\end{picture}")
            .expect("canvas");
        let out = c.to_svg(Kind::Picture, "src").expect("svg");
        // 50 units across at 1/10 em, plus stroke/PAD shift.
        assert!(out.contains("L 5.273"), "horizontal run: {out}");
    }

    #[test]
    fn unitlength_scales_the_drawing() {
        let big = render(
            r"\setlength{\unitlength}{1mm}\begin{picture}(10,10)\put(0,0){\line(1,0){10}}\end{picture}",
        )
        .expect("canvas");
        let out = big.to_svg(Kind::Picture, "src").expect("svg");
        // 10 mm = 1 cm = 28.45 pt at the TeX point × 1/10 em = 2.845 em,
        // plus PAD.
        assert!(out.contains("L 3.118"), "1 cm run plus padding: {out}");
    }

    #[test]
    fn empty_picture_falls_back() {
        assert!(render(r"\begin{picture}(10,10)\end{picture}").is_none());
    }
}
