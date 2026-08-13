//! Mermaid `pie` chart → SVG renderer.
//!
//! Supported syntax:
//! ```text
//! pie title Pets adopted
//!   "Dogs" : 386
//!   "Cats" : 85
//!   "Rats" : 15
//! ```
//!
//! Slices are drawn clockwise starting from 12 o'clock, matching what
//! Mermaid's pie renders. A colour-key legend runs down the right side
//! so each category name sits next to its slice colour. Small slices
//! (< 5%) get an outside connector line so the label doesn't crowd the
//! slice. Returns `None` when no data rows are found so the caller can
//! fall back to a code block.

use super::common::{escape_text, palette_color, FONT_FAMILY};

/// Render a Mermaid pie source to SVG. `None` if no data is found.
pub fn render(src: &str) -> Option<String> {
    let mut title = String::new();
    let mut rows: Vec<(String, f64)> = Vec::new();
    let mut total: f64 = 0.0;
    for raw in src.lines() {
        let line = super::common::strip_comments(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line
            .strip_prefix("pie")
            .or_else(|| line.strip_prefix("pieDiagram"))
        {
            // `pie title Pets…` — capture everything after `title`.
            let rest = rest.trim().trim_start_matches("title").trim();
            if !rest.is_empty() && title.is_empty() {
                title = rest.to_string();
            }
            continue;
        }
        if let Some(idx) = line.find(':') {
            let (label, value) = line.split_at(idx);
            let label = strip_quotes(label.trim()).to_string();
            let value: f64 = value[1..].trim().parse().unwrap_or(0.0);
            if !label.is_empty() && value > 0.0 {
                total += value;
                rows.push((label, value));
            }
        }
    }
    if rows.is_empty() || total <= 0.0 {
        return None;
    }

    // Layout: pie on the left, colour-key legend on the right. Width
    // and height grow with the number of legend rows.
    let r = 88.0_f64;
    let swatch = 14.0_f64;
    let gap = 8.0_f64;
    let row_h = 32.0_f64;
    let legend_col = 96.0_f64;
    let legend_gap = 36.0_f64;
    let top_pad = if title.is_empty() { 12.0 } else { 46.0 };
    let cy = top_pad + r;
    let cx = r + 20.0;
    let legend_x = cx + r + legend_gap;
    let legend_top = cy - r + 8.0;
    let legend_bottom = legend_top + row_h * rows.len() as f64 + swatch;
    let w = legend_x + swatch + gap + legend_col + 12.0;
    let h = legend_bottom.max(cy + r) + 12.0;

    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         width=\"{w:.0}\" height=\"{h:.0}\" \
         viewBox=\"0 0 {w:.0} {h:.0}\" \
         style=\"max-width:100%;height:auto;\" \
         role=\"img\" aria-label=\"pie diagram\">"
    ));
    if !title.is_empty() {
        out.push_str(&format!(
            "<text x=\"{:.0}\" y=\"28\" font-size=\"16\" font-weight=\"600\" \
             text-anchor=\"middle\" font-family=\"{FONT_FAMILY}\" fill=\"#24292f\">{}</text>",
            w / 2.0,
            escape_text(&title)
        ));
    }

    // Draw slices clockwise from 12 o'clock (-π/2).
    let mut angle = -std::f64::consts::FRAC_PI_2;
    for (i, (label, value)) in rows.iter().enumerate() {
        let frac = value / total;
        let sweep = frac * std::f64::consts::TAU;
        let a0 = angle;
        let a1 = angle + sweep;
        angle = a1;

        let x0 = cx + r * a0.cos();
        let y0 = cy + r * a0.sin();
        let x1 = cx + r * a1.cos();
        let y1 = cy + r * a1.sin();
        let large_arc = if sweep > std::f64::consts::PI { 1 } else { 0 };
        let color = palette_color(i);

        // Path: M cx,cy L x0,y0 A r,r 0 large 1 x1,y1 Z
        out.push_str(&format!(
            "<path d=\"M {cx:.1} {cy:.1} L {x0:.1} {y0:.1} \
             A {r:.1} {r:.1} 0 {large_arc} 1 {x1:.1} {y1:.1} Z\" \
             fill=\"{color}\" stroke=\"#fff\" stroke-width=\"1.5\"/>"
        ));

        // Label placement: inside slice if big enough; otherwise an
        // outside connector with the label pulled to the edge.
        let mid_a = (a0 + a1) / 2.0;
        let pct = frac * 100.0;
        let label_text = format!("{} {:.1}%", label, pct);
        if pct >= 5.0 {
            // Inside slice — the legend already names the category, so
            // the slice shows only the percentage. Text is white when
            // the slice is dark, otherwise the body text colour.
            let text_color = contrast_text(color);
            let lx = cx + (r * 0.62) * mid_a.cos();
            let ly = cy + (r * 0.62) * mid_a.sin() + 4.0;
            out.push_str(&format!(
                "<text x=\"{lx:.1}\" y=\"{ly:.1}\" font-size=\"12\" font-weight=\"600\" \
                 text-anchor=\"middle\" font-family=\"{FONT_FAMILY}\" fill=\"{text_color}\">{:.0}%</text>",
                pct
            ));
        } else {
            // Outside connector: leader line from arc to a label box.
            let leader_start = (cx + r * mid_a.cos(), cy + r * mid_a.sin());
            let side_x = if mid_a.cos() >= 0.0 { 1.0 } else { -1.0 };
            let elbow_x = cx + side_x * (r + 14.0);
            let elbow_y = cy + r * mid_a.sin();
            let label_x = elbow_x + side_x * 6.0;
            let text_anchor = if side_x > 0.0 { "start" } else { "end" };
            let text_color = "#24292f";
            out.push_str(&format!(
                "<polyline points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" \
                 fill=\"none\" stroke=\"#57606a\" stroke-width=\"1\"/>",
                leader_start.0, leader_start.1, elbow_x, elbow_y, label_x, elbow_y
            ));
            out.push_str(&format!(
                "<text x=\"{label_x:.1}\" y=\"{:.1}\" font-size=\"11\" \
                 text-anchor=\"{text_anchor}\" font-family=\"{FONT_FAMILY}\" fill=\"{text_color}\">{}</text>",
                elbow_y - 4.0,
                escape_text(&label_text)
            ));
        }
    }

    // Colour-key legend: one swatch + label row per category.
    for (i, (label, _value)) in rows.iter().enumerate() {
        let y = legend_top + i as f64 * row_h;
        let color = palette_color(i);
        out.push_str(&format!(
            "<rect x=\"{legend_x:.0}\" y=\"{y:.0}\" width=\"{swatch:.0}\" height=\"{swatch:.0}\" \
             rx=\"3\" fill=\"{color}\" stroke=\"#d0d7de\" stroke-width=\"1\"/>"
        ));
        out.push_str(&format!(
            "<text x=\"{:.0}\" y=\"{:.0}\" font-size=\"13\" \
             font-family=\"{FONT_FAMILY}\" fill=\"#24292f\">{}</text>",
            legend_x + swatch + gap,
            y + swatch - 3.0,
            escape_text(label)
        ));
    }
    out.push_str("</svg>");
    Some(out)
}

fn strip_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Choose white or near-black label text for legibility on a coloured
/// slice. The palette stays inside a narrow hue/saturation range, so
/// a simple lightness threshold is sufficient.
fn contrast_text(color: &str) -> &'static str {
    // Approximate by parsing the hex code (palette is `#[0-9a-f]{6}`).
    if let Some(hex) = color.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
            let luma = 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64;
            if luma < 150.0 {
                "#ffffff"
            } else {
                "#24292f"
            }
        } else {
            "#ffffff"
        }
    } else {
        "#ffffff"
    }
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn renders_pie_with_title() {
        let out =
            render("pie title Pets adopted\n  \"Dogs\" : 386\n  \"Cats\" : 85\n  \"Rats\" : 15")
                .expect("should render");
        assert!(out.contains("Pets adopted"));
        assert!(out.contains("Dogs"));
        assert!(out.contains("Cats"));
        assert!(out.contains("<path"));
    }

    #[test]
    fn renders_pie_without_title() {
        let out = render("pie\n  \"A\" : 50\n  \"B\" : 50").expect("render");
        assert!(out.contains("A"));
        assert!(out.contains("B"));
    }

    #[test]
    fn small_slices_get_outside_label() {
        let out = render("pie title Mix\n  \"Big\" : 99\n  \"Tiny\" : 1").expect("render");
        // The leader polyline only fires for outside labels — the
        // small-slice branch should produce one.
        assert!(
            out.contains("<polyline"),
            "expected leader for tiny slice: {out}"
        );
    }

    #[test]
    fn empty_pie_returns_none() {
        assert!(render("pie title Empty").is_none());
        assert!(render("not a pie at all").is_none());
    }

    #[test]
    fn legend_lists_every_category_with_a_swatch() {
        let out =
            render("pie title Pets adopted\n  \"Dogs\" : 386\n  \"Cats\" : 85").expect("render");
        // Each category appears once, to the right of a colour swatch.
        assert_eq!(out.matches("Dogs").count(), 1);
        assert_eq!(out.matches("Cats").count(), 1);
        assert_eq!(out.matches("<rect").count(), 2, "one swatch per category");
        // The legend is drawn after the last slice, right of the pie centre.
        let last_slice = out.rfind("<path").unwrap();
        let legend = &out[last_slice..];
        assert!(
            legend.contains("Dogs") && legend.contains("<rect"),
            "legend after slices"
        );
    }
}
