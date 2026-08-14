//! Shared helpers for every diagram renderer: SVG text escaping,
//! shape geometry, and the small primitive palette (rounded rect,
//! diamond, ellipse, parallelogram, trapezoid, hexagon, cylinder).
//!
//! Renderers in this module build SVG by hand — these helpers cover
//! the parts that are identical across [`flowchart`], [`pie`],
//! [`gantt`], [`class`], and [`dot`].

/// SVG attribute/text escape. Always runs at the very last moment,
/// before the SVG fragment is handed to the renderer, so user input
/// (Cyrillic, CJK, ampersands in labels) survives intact.
pub fn escape_text(s: &str) -> String {
    crate::html::escape_attr(s)
}

/// One row of wrapped text — either a single line, or multiple `<tspan>`
/// elements (since SVG `<text>` ignores `\n`). Each tspan gets an `x`
/// (centre-aligned) and `dy` for line height.
pub fn tspans(text: &str, max_chars: usize) -> String {
    let lines = wrap_lines(text, max_chars);
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            out.push_str(&format!(
                "<tspan x=\"0\" dy=\"0\">{}</tspan>",
                escape_text(line)
            ));
        } else {
            out.push_str(&format!(
                "<tspan x=\"0\" dy=\"1.15em\">{}</tspan>",
                escape_text(line)
            ));
        }
    }
    if out.is_empty() {
        // empty <text> still needs a tspan to position correctly
        out.push_str("<tspan x=\"0\" dy=\"0\"></tspan>");
    }
    out
}

/// Greedy word-wrap with a hard cap on characters per line. Returns
/// individual lines (no `\n` markers) — caller decides how to emit
/// them as `<tspan>`s.
pub fn wrap_lines(s: &str, max: usize) -> Vec<String> {
    if s.is_empty() {
        return vec![String::new()];
    }
    if max == 0 {
        return vec![s.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut cur_len = 0usize;
    // Split on ASCII spaces; CJK / Cyrillic runs through unchanged.
    for word in s.split_whitespace() {
        let wlen = word.chars().count();
        if cur_len == 0 {
            current.push_str(word);
            cur_len = wlen;
        } else if cur_len + 1 + wlen <= max {
            current.push(' ');
            current.push_str(word);
            cur_len += 1 + wlen;
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            cur_len = wlen;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Font stack emitted into every `<text>` element. `sans-serif` is
/// the generic family that the browser/viewer falls back to when no
/// named font is found — the named families afterwards cover the
/// common CJK installations on Linux (Noto, WenQuanYi), Windows
/// (Microsoft YaHei, SimSun), and macOS (PingFang SC, Hiragino
/// Sans GB). Without these fallbacks, viewers that map `sans-serif`
/// to a font lacking CJK coverage (e.g. headless ImageMagick with
/// the default font config) silently drop the CJK glyphs and only
/// ASCII labels stay visible.
///
/// Names are unquoted because the value lives inside a double-quoted
/// SVG attribute (`font-family="..."`); escaping inner quotes with
/// `&quot;` works but renders the font attribute unreadable, and
/// switching the attribute to single quotes per site is uglier. CSS
/// font-name parsing tolerates unquoted multi-word names without
/// spaces that look like identifiers; the spaces here are accepted
/// by every modern browser and by librsvg / ImageMagick.
pub const FONT_FAMILY: &str = "sans-serif, Noto Sans CJK SC, Microsoft YaHei, PingFang SC, \
     Hiragino Sans GB, Source Han Sans SC, WenQuanYi Micro Hei";

/// Default node padding around label text. Tight on both axes so the
/// diagram stays compact when scaled to fit a page column — the
/// 12 px font in viewBox units already reads as 12 CSS px once the
/// SVG carries explicit width/height.
pub const NODE_PAD_X: f64 = 12.0;
/// Default node padding on top/bottom.
pub const NODE_PAD_Y: f64 = 8.0;

/// CJK-capable font stack. Listed first so SVG `<tspan>`s that
/// contain only CJK glyphs can pick the right face even when the
/// surrounding render context (e.g. ImageMagick's librsvg) doesn't
/// process the parent `font-family` fallback chain. Browsers fall
/// back to `sans-serif` for Latin chars automatically.
pub const CJK_FONT: &str = "Noto Sans CJK SC, Microsoft YaHei, PingFang SC, Hiragino Sans GB, \
     Source Han Sans SC, WenQuanYi Micro Hei";

/// Split `text` into runs of ASCII vs. CJK characters and emit each
/// run as its own `<tspan>` with an explicit font-family. The CJK
/// runs get [`CJK_FONT`] so a renderer that doesn't do font-family
/// fallback still draws the glyphs. ASCII runs get `sans-serif`.
/// Caller must escape the result of this function — it embeds the
/// raw chars inside the tspan tags.
pub fn render_text_spans(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut run = String::new();
    let mut run_is_cjk: Option<bool> = None;
    let flush = |run: &mut String, run_is_cjk: &mut Option<bool>, out: &mut String| {
        if run.is_empty() {
            return;
        }
        let family = if run_is_cjk.unwrap_or(false) {
            CJK_FONT
        } else {
            "sans-serif"
        };
        out.push_str(&format!(
            "<tspan font-family=\"{}\">{}</tspan>",
            family, run
        ));
        run.clear();
        *run_is_cjk = None;
    };
    for c in text.chars() {
        let cjk = !c.is_ascii() && c != ' ';
        if let Some(prev) = run_is_cjk {
            if prev != cjk {
                flush(&mut run, &mut run_is_cjk, &mut out);
            }
        }
        run_is_cjk = Some(cjk);
        run.push(c);
    }
    flush(&mut run, &mut run_is_cjk, &mut out);
    out
}

/// Estimate the width of `text` rendered in a 12 px sans-serif font
/// (matches what the SVG `<text>` element ends up using). Roughly
/// 6.6 px per ASCII char, 13.2 px per CJK glyph — so the box widths
/// stay close to what a browser would actually lay out.
pub fn approx_text_width(text: &str) -> f64 {
    let mut w: f64 = 0.0;
    for c in text.chars() {
        if c.is_ascii() {
            w += 6.6;
        } else {
            // CJK / wide characters
            w += 13.2;
        }
    }
    if w < 2.0 {
        2.0
    } else {
        w
    }
}

/// Default node box size that fits `label` comfortably — width adapts
/// to text length, height adapts to wrapped-line count.
pub fn fit_node(label: &str) -> (f64, f64) {
    let max_chars = 14;
    let lines = wrap_lines(label, max_chars);
    let max_w = lines
        .iter()
        .map(|l| approx_text_width(l))
        .fold(0.0_f64, f64::max);
    let line_h = 14.0_f64;
    let h = (lines.len() as f64 * line_h + NODE_PAD_Y * 2.0).max(36.0);
    let w = (max_w + NODE_PAD_X * 2.0).max(70.0);
    (w, h)
}

/// SVG `path d=` for a flowchart node shape. Shape codes are the
/// same convention used by [`flowchart`]:
/// 0 rect, 1 rounded rect, 2 circle, 3 diamond, 4 parallelogram,
/// 5 trapezoid, 6 asymmetric.
pub fn shape_path(shape: u8, x: f64, y: f64, w: f64, h: f64) -> String {
    let x0 = x;
    let x1 = x + w;
    let y0 = y;
    let y1 = y + h;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    match shape {
        1 => {
            // rounded rect (Mermaid's stadium / rounded)
            let r = 8.0_f64.min(w / 2.0).min(h / 2.0);
            format!(
                "M {x0:.1} {y0_add_r:.1} A {r:.1} {r:.1} 0 0 1 {x0_add_r:.1} {y0:.1} L {x1_sub_r:.1} {y0:.1} \
                 A {r:.1} {r:.1} 0 0 1 {x1:.1} {y0_add_r:.1} L {x1:.1} {y1_sub_r:.1} \
                 A {r:.1} {r:.1} 0 0 1 {x1_sub_r:.1} {y1:.1} L {x0_add_r:.1} {y1:.1} \
                 A {r:.1} {r:.1} 0 0 1 {x0:.1} {y1_sub_r:.1} Z",
                y0_add_r = y0 + r,
                x0_add_r = x0 + r,
                x1_sub_r = x1 - r,
                y1_sub_r = y1 - r,
            )
        }
        2 => format!(
            "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{r:.1}\" />",
            r = (h / 2.0).min(w / 2.0) + 6.0
        ),
        3 => format!("M {cx:.1} {y0:.1} L {x1:.1} {cy:.1} L {cx:.1} {y1:.1} L {x0:.1} {cy:.1} Z"),
        4 => {
            // parallelogram (slanted)
            let s = 14.0_f64;
            format!(
                "M {x0_add_s:.1} {y0:.1} L {x1:.1} {y0:.1} L {x1_sub_s:.1} {y1:.1} L {x0:.1} {y1:.1} Z",
                x0_add_s = x0 + s,
                x1_sub_s = x1 - s
            )
        }
        5 => format!("M {x0:.1} {y0:.1} L {x1:.1} {y0:.1} L {x0:.1} {y1:.1} Z"),
        6 => {
            // asymmetric (flag)
            let l = x1 - 20.0;
            format!(
                "M {x0:.1} {y0:.1} L {l:.1} {y0:.1} L {x1:.1} {cy:.1} L {l:.1} {y1:.1} L {x0:.1} {y1:.1} Z"
            )
        }
        _ => format!("M {x0:.1} {y0:.1} L {x1:.1} {y0:.1} L {x1:.1} {y1:.1} L {x0:.1} {y1:.1} Z"),
    }
}

/// Hex stroke colour catalogue. Used by [`pie`], [`gantt`], and
/// [`class`] for deterministic, accessible colours.
pub const PALETTE: &[&str] = &[
    "#5b8def", "#f5a623", "#7ed321", "#bd10e0", "#ff6b6b", "#4ecdc4", "#f7d046", "#9b59b6",
    "#1abc9c", "#e67e22",
];

/// Pick a colour from [`PALETTE`] by index (wraps).
pub fn palette_color(i: usize) -> &'static str {
    PALETTE[i % PALETTE.len()]
}

/// Strip a leading `%` and trailing `,` from a Mermaid source line.
pub fn strip_comments(line: &str) -> &str {
    if let Some(i) = line.find("%%") {
        &line[..i]
    } else {
        line
    }
}
