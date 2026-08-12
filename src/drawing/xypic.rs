//! xy-pic commutative diagrams: the `\xymatrix` subset.
//!
//! Entries sit on a grid (`&` starts a column, `\\` a row) and are joined by
//! `\ar[dir]` arrows, where `dir` is a run of `r`/`l`/`u`/`d` steps — `[r]` is
//! one column right, `[dr]` one down and one right, `[rr]` two right.
//!
//! Arrow labels follow xy-pic's side convention: `^` sits on the left of the
//! travel direction (above a rightward arrow), `_` on the right, and `|` is
//! placed on the line itself.

use super::scan::Scanner;
use super::{
    label, math_label, Anchor, Arrow, ArrowKind, Canvas, Color, Item, PathOp, Pt, Stroke,
    EM_PER_XY_CELL, LABEL_SEP,
};
use crate::tex::Typeset;

/// Gap between an entry and the arrows touching it.
const ENTRY_PAD: f64 = 0.22;
/// Distance from an arrow to its `^` / `_` label.
const SIDE_SEP: f64 = 0.26;

struct Entry {
    row: usize,
    col: usize,
    ts: Option<Typeset>,
    arrows: Vec<Arr>,
}

struct Arr {
    /// Column and row deltas, row growing downwards.
    dcol: i64,
    drow: i64,
    /// (side, body): +1 above/left, -1 below/right, 0 on the line.
    labels: Vec<(i32, String)>,
}

pub(crate) fn render(src: &str) -> Option<Canvas> {
    let body = matrix_body(src)?;
    let entries = parse(&body);
    if entries.is_empty() {
        return None;
    }
    let mut canvas = Canvas::default();

    // Entry radii drive how far arrows stop short of their endpoints.
    let radius = |e: &Entry| match &e.ts {
        Some(ts) => (ts.w / 2.0).max((ts.h + ts.d) / 2.0) + ENTRY_PAD,
        None => ENTRY_PAD,
    };
    let centre = |row: usize, col: usize| -> Pt {
        (
            col as f64 * EM_PER_XY_CELL,
            -(row as f64) * EM_PER_XY_CELL,
        )
    };

    for e in &entries {
        if let Some(ts) = &e.ts {
            canvas.push(Item::Label {
                at: centre(e.row, e.col),
                anchor: Anchor::Center,
                ts: ts.clone(),
                gap: 0.0,
                color: Color::Current,
            });
        }
    }

    for e in &entries {
        for a in &e.arrows {
            let (tr, tc) = (e.row as i64 + a.drow, e.col as i64 + a.dcol);
            if tr < 0 || tc < 0 {
                continue;
            }
            let from = centre(e.row, e.col);
            let to = centre(tr as usize, tc as usize);
            let (dx, dy) = (to.0 - from.0, to.1 - from.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len <= 0.0 {
                continue;
            }
            let (ux, uy) = (dx / len, dy / len);
            let r0 = radius(e);
            let r1 = entries
                .iter()
                .find(|x| x.row as i64 == tr && x.col as i64 == tc)
                .map(radius)
                .unwrap_or(ENTRY_PAD);
            if r0 + r1 >= len {
                continue;
            }
            let p0 = (from.0 + ux * r0, from.1 + uy * r0);
            let p1 = (to.0 - ux * r1, to.1 - uy * r1);
            canvas.push(Item::Path {
                ops: vec![PathOp::Move(p0), PathOp::Line(p1)],
                stroke: Some(Stroke::default()),
                fill: None,
                arrow: Arrow {
                    end: true,
                    kind: ArrowKind::To,
                    ..Arrow::default()
                },
            });
            let mid = ((p0.0 + p1.0) / 2.0, (p0.1 + p1.1) / 2.0);
            // Rotating the direction a quarter turn counter-clockwise gives
            // xy-pic's `^` side: above a rightward arrow, right of a
            // downward one.
            let (nx, ny) = (-uy, ux);
            for (side, body) in &a.labels {
                let Some(ts) = math_label(body) else { continue };
                let off = match side {
                    1 => SIDE_SEP,
                    -1 => -SIDE_SEP,
                    _ => 0.0,
                };
                canvas.push(Item::Label {
                    at: (mid.0 + nx * off, mid.1 + ny * off),
                    anchor: anchor_for(nx * off, ny * off),
                    ts,
                    gap: 0.0,
                    color: Color::Current,
                });
            }
        }
    }

    if canvas.is_empty() {
        None
    } else {
        Some(canvas)
    }
}

/// Pin the label's near edge to the arrow so it never sits on the line.
/// Canvas is y-up, so `oy > 0` means *above* the arrow (North on screen).
/// The pin offset is along the arrow's perpendicular: `ox > 0` means the
/// pin sits to the right of the arrow, and we want the label to sit on the
/// same side as the pin — `Anchor::East` puts the label's left edge just
/// past the pin, leaving a `LABEL_SEP` gap from the arrow.
fn anchor_for(ox: f64, oy: f64) -> Anchor {
    if ox.abs() > oy.abs() {
        if ox > 0.0 {
            Anchor::East
        } else {
            Anchor::West
        }
    } else if oy > LABEL_SEP {
        Anchor::North
    } else if oy < -LABEL_SEP {
        Anchor::South
    } else {
        Anchor::Center
    }
}

/// The `{…}` body of `\xymatrix`, skipping its `@…` layout options.
fn matrix_body(src: &str) -> Option<String> {
    let i = src.find("\\xymatrix")?;
    let mut s = Scanner::new(&src[i + "\\xymatrix".len()..]);
    while matches!(s.peek(), Some(c) if c == '@' || c == '=' || c.is_alphanumeric() || c == '.' || c == '+' || c == '-') {
        s.bump();
    }
    s.group()
}

fn parse(body: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for (row, line) in super::scan::split_top(body, '\u{1}')
        .first()
        .map(|_| split_rows(body))
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        for (col, cell) in super::scan::split_top(&line, '&').into_iter().enumerate() {
            let e = parse_cell(row, col, &cell);
            if e.ts.is_some() || !e.arrows.is_empty() {
                out.push(e);
            }
        }
    }
    out
}

/// Split on top-level `\\` row separators.
fn split_rows(body: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            '\\' if depth <= 0 && chars.get(i + 1) == Some(&'\\') => {
                rows.push(cur.clone());
                cur.clear();
                i += 2;
                continue;
            }
            _ => {}
        }
        cur.push(c);
        i += 1;
    }
    if !cur.trim().is_empty() {
        rows.push(cur);
    }
    rows
}

fn parse_cell(row: usize, col: usize, cell: &str) -> Entry {
    let (head, rest) = match cell.find("\\ar") {
        Some(i) => (&cell[..i], &cell[i..]),
        None => (cell, ""),
    };
    let mut arrows = Vec::new();
    let mut s = Scanner::new(rest);
    while !s.eof() {
        let Some(cmd) = s.command() else {
            s.bump();
            continue;
        };
        if cmd != "ar" {
            continue;
        }
        let dir = s.bracket().unwrap_or_default();
        let mut a = Arr {
            dcol: 0,
            drow: 0,
            labels: Vec::new(),
        };
        for c in dir.chars() {
            match c {
                'r' => a.dcol += 1,
                'l' => a.dcol -= 1,
                'd' => a.drow += 1,
                'u' => a.drow -= 1,
                _ => {}
            }
        }
        if a.dcol == 0 && a.drow == 0 {
            continue;
        }
        // Labels attach directly after the direction: `^f`, `_{f'}`, `|x`.
        loop {
            let side = match s.peek() {
                Some('^') => 1,
                Some('_') => -1,
                Some('|') => 0,
                _ => break,
            };
            s.bump();
            let body = match s.group() {
                Some(g) => g,
                None => {
                    // A single token, e.g. `^f`.
                    let mut t = String::new();
                    if s.peek() == Some('\\') {
                        t.push('\\');
                        s.bump();
                        while matches!(s.peek(), Some(c) if c.is_alphabetic()) {
                            t.push(s.bump().unwrap());
                        }
                    } else if let Some(c) = s.bump() {
                        t.push(c);
                    }
                    t
                }
            };
            a.labels.push((side, body));
        }
        arrows.push(a);
    }
    Entry {
        row,
        col,
        ts: label(head.trim()),
        arrows,
    }
}

#[cfg(test)]
mod tests {
    use super::super::render as draw;
    use super::*;

    const SQUARE: &str = r"\xymatrix{
  A \ar[r]^f \ar[d]_g &
  B \ar[d]^{g'} \\
  D \ar[r]_{f'} &
  C
}";

    #[test]
    fn renders_the_commutative_square() {
        let out = draw(SQUARE).expect("svg");
        assert!(out.contains("aria-label=\"commutative diagram\""), "{out}");
        // Four entries, four arrow labels, four arrows (line + tip each).
        assert_eq!(out.matches("<path").count(), 8, "four arrows: {out}");
        assert!(out.contains("<text"), "entries are typeset: {out}");
    }

    #[test]
    fn caret_and_underscore_land_on_opposite_sides() {
        let c = render(r"\xymatrix{A \ar[r]^f & B}").expect("canvas");
        let up = c
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Label { at, .. } => Some(at.1),
                _ => None,
            })
            .fold(f64::MIN, f64::max);
        let c2 = render(r"\xymatrix{A \ar[r]_f & B}").expect("canvas");
        let down = c2
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Label { at, .. } => Some(at.1),
                _ => None,
            })
            .fold(f64::MAX, f64::min);
        assert!(up > 0.0, "^ sits above the arrow");
        assert!(down < 0.0, "_ sits below the arrow");
    }

    #[test]
    fn multi_step_directions_span_several_cells() {
        let c = render(r"\xymatrix{A \ar[rr] & B & C}").expect("canvas");
        let far = c.items.iter().any(|i| match i {
            Item::Path { ops, .. } => matches!(ops.last(), Some(PathOp::Line(p)) if p.0 > EM_PER_XY_CELL),
            _ => false,
        });
        assert!(far, "[rr] reaches the third column");
    }

    /// `anchor_for` reads canvas coordinates (y-up), so `oy > 0` must mean
    /// *above* the arrow (North on screen). The x branches map a pin offset
    /// to the side of the label that should sit just past it: a pin to the
    /// right of the arrow (`ox > 0`) calls for `Anchor::East` so the label's
    /// left edge lands a `LABEL_SEP` past the pin. When `Anchor::origin_offset`
    /// was flipped to match y-up, the two conventions disagreed and dropped
    /// xy-pic arrow labels back onto the wrong side of the arrow.
    #[test]
    fn anchor_for_uses_canvas_y_up() {
        assert!(matches!(anchor_for(0.0, 1.0), Anchor::North));
        assert!(matches!(anchor_for(0.0, LABEL_SEP * 2.0), Anchor::North));
        assert!(matches!(anchor_for(0.0, -1.0), Anchor::South));
        assert!(matches!(anchor_for(0.0, -LABEL_SEP * 2.0), Anchor::South));
        assert!(matches!(anchor_for(1.0, 0.0), Anchor::East));
        assert!(matches!(anchor_for(-1.0, 0.0), Anchor::West));
    }

    #[test]
    fn empty_matrix_falls_back() {
        assert!(render(r"\xymatrix{}").is_none());
    }
}
