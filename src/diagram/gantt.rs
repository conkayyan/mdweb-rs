//! Mermaid `gantt` chart → SVG renderer.
//!
//! Supported syntax:
//! ```text
//! gantt
//!     title   A Gantt Diagram
//!     dateFormat YYYY-MM-DD
//!     section Section
//!     A task          :a1, 2024-01-01, 30d
//!     Another task    :after a1, 20d
//!     Another one     :a3, 2024-01-01, 12d
//! ```
//!
//! Tasks render as horizontal bars against a date axis that runs along
//! the top. The `after <id>` shorthand is resolved at parse time so a
//! task with no explicit start date gets placed right after its
//! predecessor, and a right-angle dependency arrow is drawn between the
//! two bars. Each section is tinted with its own colour and bars carry
//! their label ("name (Nd)") when they are wide enough to hold it.
//!
//! Returns `None` when no tasks are found.

use super::common::{approx_text_width, escape_text, palette_color, FONT_FAMILY};

/// Render a Mermaid gantt source to SVG. `None` if no tasks are found.
pub fn render(src: &str) -> Option<String> {
    let mut title = String::new();
    let mut date_format = DateFormat::Iso;
    let mut sections: Vec<Section> = Vec::new();
    let mut current: Option<usize> = None;
    let mut id_to_idx: std::collections::HashMap<String, usize> = Default::default();
    let mut id_to_section: std::collections::HashMap<String, usize> = Default::default();

    for raw in src.lines() {
        let line = super::common::strip_comments(raw).trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("gantt") {
            let _ = rest; // `gantt` / `ganttDiagram` directive — ignored
        } else if lower.starts_with("title") {
            title = line[5..].trim().to_string();
        } else if lower.starts_with("dateformat") {
            let fmt = line["dateformat".len()..].trim().to_ascii_lowercase();
            date_format = match fmt.as_str() {
                "yyyymmdd" | "yyyy/mm/dd" => DateFormat::Iso,
                _ => DateFormat::Iso,
            };
        } else if lower.starts_with("section") {
            let name = line["section".len()..].trim().to_string();
            sections.push(Section {
                name,
                tasks: Vec::new(),
            });
            current = Some(sections.len() - 1);
        } else if current.is_some() {
            // task line: optional id and metadata
            let spec = line.to_string();
            let mut tags: Vec<String> = Vec::new();
            let mut status = Status::Active;
            // Find the LAST ` :` colon-block — Mermaid tags come at the
            // tail (`<task> :<id>, <start>, <duration> :tag1, tag2`).
            // Earlier colons belong to id/start/duration syntax.
            let spec = if let Some(colon) = spec.rfind(" :") {
                let tail = spec[colon + 2..].trim();
                let tail_lower = tail.to_ascii_lowercase();
                let is_tag_block = tail_lower
                    .split(',')
                    .map(|s| s.trim())
                    .all(|s| matches!(s, "done" | "active" | "crit" | "critical" | ""));
                if is_tag_block {
                    for raw_tag in tail.split(',') {
                        let tag = raw_tag.trim().to_ascii_lowercase();
                        if tag == "done" {
                            status = Status::Done;
                        } else if tag == "active" {
                            status = Status::Active;
                        } else if tag == "crit" || tag == "critical" {
                            tags.push("crit".to_string());
                        }
                    }
                    spec[..colon].trim().to_string()
                } else {
                    spec
                }
            } else {
                spec
            };
            let task = parse_task(&spec, date_format);
            if let Some(t) = task {
                let si = current.unwrap();
                let ti = sections[si].tasks.len();
                if let Some(ref id) = t.id {
                    id_to_idx.insert(id.clone(), ti);
                    id_to_section.insert(id.clone(), si);
                }
                sections[si].tasks.push(TaskRow {
                    label: t.label,
                    id: t.id,
                    start_date: t.start_date,
                    duration_days: t.duration_days,
                    after: t.after,
                    status,
                    critical: tags.contains(&"crit".to_string()),
                });
            }
        }
    }

    if sections.iter().all(|s| s.tasks.is_empty()) {
        return None;
    }

    // Resolve `after` placeholders by computing concrete start dates.
    for si in 0..sections.len() {
        for ti in 0..sections[si].tasks.len() {
            let after_id = sections[si].tasks[ti].after.clone();
            if let Some(after_id) = after_id {
                if let Some(&other_si) = id_to_section.get(&after_id) {
                    if let Some(&other_ti) = id_to_idx.get(&after_id) {
                        let other_end = sections[other_si].tasks[other_ti].end_date();
                        sections[si].tasks[ti].start_date = Some(other_end);
                        sections[si].tasks[ti].duration_days =
                            sections[si].tasks[ti].duration_days.max(1);
                    }
                }
            }
        }
    }

    let pad_x = 150.0_f64; // left gutter for task labels
    let pad_right = 56.0_f64;
    let date_band = 22.0_f64; // height of the top date-header row
    let grid_top = if title.is_empty() { 20.0 } else { 40.0 };
    let axis_label_y = grid_top + 16.0;
    let row_h = 34.0_f64;
    let bar_h = 20.0_f64;
    let section_gap = 22.0_f64;
    // Compute date range.
    let mut min_date = i64::MAX;
    let mut max_date = i64::MIN;
    for sec in &sections {
        for t in &sec.tasks {
            if let Some(s) = t.start_date {
                let e = s + t.duration_days;
                if s < min_date {
                    min_date = s;
                }
                if e > max_date {
                    max_date = e;
                }
            }
        }
    }
    if min_date == i64::MAX {
        return None;
    }
    let span_days = (max_date - min_date).max(7);
    // Tick density adapts to the schedule length so the date labels
    // remain readable: daily for short spans, then weekly, then monthly.
    let (days_per_px, date_stride) = if span_days <= 31 {
        (60.0_f64, 1_i64)
    } else if span_days <= 200 {
        (20.0, 7)
    } else {
        (10.0, 30)
    };
    let chart_w = span_days as f64 * days_per_px;

    // Row layout: a date band, then per section a header line and one
    // bar row per task, with air around each section (including a
    // trailing one for bottom breathing room).
    let mut total_h = date_band;
    for sec in &sections {
        if !sec.name.is_empty() {
            total_h += 22.0;
        }
        total_h += sec.tasks.len() as f64 * row_h;
        total_h += section_gap;
    }

    let w = pad_x + chart_w + pad_right;
    let h = grid_top + total_h + 24.0;
    let chart_bottom = grid_top + total_h;

    // Defs ids derive from the source so several charts per page never
    // share (and clobber) marker or filter definitions.
    let tag = digest(src);
    let arrow_id = format!("gantt-arrow-{tag}");
    let shadow_id = format!("gantt-shadow-{tag}");

    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         width=\"{w:.0}\" height=\"{h:.0}\" \
         viewBox=\"0 0 {w:.0} {h:.0}\" \
         style=\"max-width:100%;height:auto;\" \
         role=\"img\" aria-label=\"gantt diagram\">"
    ));
    out.push_str("<defs>");
    out.push_str(&format!(
        "<marker id=\"{arrow_id}\" viewBox=\"0 0 10 10\" refX=\"8\" refY=\"5\" \
         markerWidth=\"7\" markerHeight=\"7\" orient=\"auto\">\
         <path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"#57606a\"/></marker>"
    ));
    out.push_str(&format!(
        "<filter id=\"{shadow_id}\" x=\"-10%\" y=\"-20%\" width=\"120%\" height=\"150%\">\
         <feDropShadow dx=\"1\" dy=\"2\" stdDeviation=\"1.5\" flood-opacity=\"0.18\"/>\
         </filter>"
    ));
    out.push_str("</defs>");
    if !title.is_empty() {
        out.push_str(&format!(
            "<text x=\"{:.0}\" y=\"28\" font-size=\"18\" font-weight=\"600\" \
             text-anchor=\"middle\" font-family=\"{FONT_FAMILY}\" fill=\"#24292f\">{}</text>",
            w / 2.0,
            escape_text(&title)
        ));
    }

    // Vertical grid across the chart, with the date axis along the top.
    for day in 0..=span_days {
        let tx = pad_x + day as f64 * days_per_px;
        let major = day % date_stride == 0;
        out.push_str(&format!(
            "<line x1=\"{tx:.0}\" y1=\"{grid_top:.0}\" x2=\"{tx:.0}\" y2=\"{chart_bottom:.0}\" \
             stroke=\"{}\" stroke-width=\"1\"{}/>",
            if major { "#d0d7de" } else { "#eef1f4" },
            if major {
                ""
            } else {
                " stroke-dasharray=\"3 3\""
            },
        ));
        if major {
            out.push_str(&format!(
                "<text x=\"{tx:.0}\" y=\"{axis_label_y:.0}\" font-size=\"10\" \
                 text-anchor=\"middle\" font-family=\"{FONT_FAMILY}\" fill=\"#57606a\">{}</text>",
                format_short_date(min_date + day)
            ));
        }
    }

    // Tasks: one bar row per task, keyed by section colour.
    #[derive(Clone, Copy)]
    struct Placed {
        x1: f64,
        x2: f64,
        cy: f64,
    }
    let mut placed: Vec<Vec<Option<Placed>>> =
        sections.iter().map(|s| vec![None; s.tasks.len()]).collect();
    let mut row_y = grid_top + date_band;
    for (si, sec) in sections.iter().enumerate() {
        if !sec.name.is_empty() {
            out.push_str(&format!(
                "<text x=\"8\" y=\"{:.0}\" font-size=\"14\" font-weight=\"600\" \
                 font-family=\"{FONT_FAMILY}\" fill=\"#6e7781\">{}</text>",
                row_y + 16.0,
                escape_text(&sec.name)
            ));
            row_y += 22.0;
        }
        for (ti, t) in sec.tasks.iter().enumerate() {
            let bar_top = row_y + 7.0;
            let label_baseline = row_y + 17.0;
            // Left column label.
            out.push_str(&format!(
                "<text x=\"8\" y=\"{label_baseline:.0}\" font-size=\"13\" \
                 font-family=\"{FONT_FAMILY}\" fill=\"#24292f\">{}</text>",
                escape_text(&t.label)
            ));
            if let Some(start) = t.start_date {
                let x1 = pad_x + (start - min_date) as f64 * days_per_px;
                let x2 = pad_x + (start - min_date + t.duration_days) as f64 * days_per_px;
                let color = if t.critical {
                    "#8250df"
                } else {
                    palette_color(si)
                };
                let bar_w = (x2 - x1).max(6.0);
                out.push_str(&format!(
                    "<rect x=\"{x1:.1}\" y=\"{bar_top:.1}\" width=\"{bar_w:.1}\" \
                     height=\"{bar_h:.1}\" rx=\"4\" fill=\"{color}\" \
                     filter=\"url(#{shadow_id})\"/>"
                ));
                // White overlay marks an `active` task (still in flight).
                if matches!(t.status, Status::Active) {
                    out.push_str(&format!(
                        "<rect x=\"{x1:.1}\" y=\"{bar_top:.1}\" width=\"{bar_w:.1}\" \
                         height=\"{bar_h:.1}\" rx=\"4\" fill=\"#ffffff\" fill-opacity=\"0.35\"/>"
                    ));
                }
                // In-bar "name (Nd)" when it fits.
                let inner = format!("{} ({}d)", t.label, t.duration_days);
                if approx_text_width(&inner) <= bar_w - 14.0 {
                    out.push_str(&format!(
                        "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"13\" font-weight=\"600\" \
                         text-anchor=\"middle\" font-family=\"{FONT_FAMILY}\" fill=\"#ffffff\">{}</text>",
                        x1 + bar_w / 2.0,
                        bar_top + bar_h / 2.0 + 4.0,
                        escape_text(&inner)
                    ));
                }
                placed[si][ti] = Some(Placed {
                    x1,
                    x2,
                    cy: bar_top + bar_h / 2.0,
                });
            }
            row_y += row_h;
        }
        row_y += section_gap;
    }

    // Dependency arrows: right-angle connector from the predecessor's
    // right edge to the dependent's left edge.
    for (si, sec) in sections.iter().enumerate() {
        for (ti, t) in sec.tasks.iter().enumerate() {
            let after_id = match t.after {
                Some(ref id) => id.clone(),
                None => continue,
            };
            let &other_si = match id_to_section.get(&after_id) {
                Some(s) => s,
                None => continue,
            };
            let &other_ti = match id_to_idx.get(&after_id) {
                Some(t) => t,
                None => continue,
            };
            let (src, dst) = match (placed[other_si][other_ti], placed[si][ti]) {
                (Some(s), Some(d)) => (s, d),
                _ => continue,
            };
            if (src.x2 - dst.x1).abs() < 0.5 && (src.cy - dst.cy).abs() < 0.5 {
                continue;
            }
            if (src.x2 - dst.x1).abs() < 0.5 {
                // Same column: straight vertical connector.
                out.push_str(&format!(
                    "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
                     stroke=\"#57606a\" stroke-width=\"1.5\" marker-end=\"url(#{arrow_id})\"/>",
                    src.x2, src.cy, dst.x1, dst.cy
                ));
            } else {
                let midx = (src.x2 + dst.x1) / 2.0;
                out.push_str(&format!(
                    "<path d=\"M {:.1} {:.1} L {midx:.1} {:.1} L {midx:.1} {:.1} \
                     L {:.1} {:.1}\" fill=\"none\" stroke=\"#57606a\" stroke-width=\"1.5\" \
                     marker-end=\"url(#{arrow_id})\"/>",
                    src.x2, src.cy, src.cy, dst.cy, dst.x1, dst.cy
                ));
            }
        }
    }

    out.push_str("</svg>");
    Some(out)
}

#[derive(Clone, Copy, PartialEq)]
enum DateFormat {
    Iso,
}

#[derive(Clone, Copy)]
enum Status {
    Active,
    Done,
}

struct Section {
    name: String,
    tasks: Vec<TaskRow>,
}

struct TaskRow {
    label: String,
    #[allow(dead_code)]
    id: Option<String>,
    start_date: Option<i64>,
    duration_days: i64,
    after: Option<String>,
    #[allow(dead_code)]
    status: Status,
    critical: bool,
}

impl TaskRow {
    fn end_date(&self) -> i64 {
        self.start_date.unwrap_or(0) + self.duration_days
    }
}

struct ParsedTask {
    label: String,
    id: Option<String>,
    start_date: Option<i64>,
    duration_days: i64,
    after: Option<String>,
}

fn parse_task(line: &str, _fmt: DateFormat) -> Option<ParsedTask> {
    // Format: `<label> :<id>, <start>, <duration>` or
    //         `<label> :<id>, after <other>, <duration>`.
    let mut label = line.trim().to_string();
    let mut id: Option<String> = None;
    let mut start_date: Option<i64> = None;
    let mut duration_days: i64 = 1;
    let mut after: Option<String> = None;
    // split first colon
    if let Some(colon) = label.find(':') {
        let (lab, rest) = label.split_at(colon);
        let rest = rest[1..].to_string();
        label = lab.trim().to_string();
        let mut parts = rest.split(',').map(str::trim);
        let first = parts.next().unwrap_or("");
        if !first.is_empty() && !looks_like_date(first) && first != "after" {
            id = Some(first.to_string());
            if let Some(s) = parts.next() {
                parse_start_or_after(s, &mut start_date, &mut after);
            }
        } else if first == "after" {
            // `:<id>, after <other>, <duration>`
            // We need the id before — already popped as first above,
            // but here first == "after" so the id was the part before
            // (already consumed as the section of the line). Recover
            // it: look at the label and split off an explicit `id` if
            // present in the colon-segment.
            if let Some(s) = parts.next() {
                after = Some(s.to_string());
            }
            if let Some(d) = parts.next() {
                duration_days = parse_duration(d);
            }
        } else {
            // No id: just start, duration (or start).
            parse_start_or_after(first, &mut start_date, &mut after);
        }
        if let Some(d) = parts.next() {
            if d != "after" {
                duration_days = parse_duration(d);
            } else if let Some(other) = parts.next() {
                after = Some(other.to_string());
                if let Some(dd) = parts.next() {
                    duration_days = parse_duration(dd);
                }
            }
        }
    }
    if label.is_empty() {
        return None;
    }
    Some(ParsedTask {
        label,
        id,
        start_date,
        duration_days,
        after,
    })
}

fn parse_start_or_after(s: &str, start: &mut Option<i64>, after: &mut Option<String>) {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("after ") {
        *after = Some(rest.trim().to_string());
        return;
    }
    if let Some(days) = parse_date(s) {
        *start = Some(days);
    }
}

fn looks_like_date(s: &str) -> bool {
    // YYYY-MM-DD or YYYY/MM/DD
    let bytes = s.as_bytes();
    bytes.len() >= 8 && (bytes[4] == b'-' || bytes[4] == b'/')
}

fn parse_date(s: &str) -> Option<i64> {
    let s = s.trim().replace('/', "-");
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    Some(days_from_epoch(y, m, d))
}

fn parse_duration(s: &str) -> i64 {
    let s = s.trim();
    let mut chars = s.chars().peekable();
    let mut num = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
            chars.next();
        } else {
            break;
        }
    }
    let n: f64 = num.parse().unwrap_or(0.0);
    let suffix: String = chars.collect();
    let mult: f64 = match suffix.to_ascii_lowercase().as_str() {
        "d" => 1.0,
        "w" => 7.0,
        "m" => 30.0,
        "h" => 1.0 / 24.0,
        _ => 1.0,
    };
    ((n * mult).round() as i64).max(1)
}

/// Days since a fixed epoch (1970-01-01) using Howard Hinnant's
/// civil-from-days algorithm.
fn days_from_epoch(y: i64, m: i64, d: i64) -> i64 {
    let y_adj = if m <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = (y_adj - era * 400) as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn format_short_date(days: i64) -> String {
    // Inverse of days_from_epoch (Howard Hinnant's civil_from_days).
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as i64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// FNV-1a digest of the source, used to namespace `<defs>` ids so two
/// gantt charts on one page never share marker/filter definitions.
fn digest(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:08x}", h & 0xffff_ffff)
}

#[cfg(test)]
mod tests {
    use super::{days_from_epoch, format_short_date, parse_date, parse_duration, render};

    #[test]
    fn renders_simple_gantt() {
        let src = "gantt\n  title A timeline\n  dateFormat YYYY-MM-DD\n  section A\n  T1 :a1, 2024-01-01, 7d\n  T2 :a2, 2024-01-08, 5d\n";
        let out = render(src).expect("render");
        assert!(out.contains("A timeline"));
        assert!(out.contains("T1"));
        assert!(out.contains("T2"));
        assert!(out.contains("<rect"));
    }

    #[test]
    fn after_dependency_resolves_start() {
        let src = "gantt\n  dateFormat YYYY-MM-DD\n  section S\n  First  :f1, 2024-01-01, 5d\n  Second :s1, after f1, 3d\n";
        let out = render(src).expect("render");
        // Second should start on 2024-01-06 (5 days after start).
        assert!(out.contains("Second"));
        assert!(out.contains("<rect"));
    }

    #[test]
    fn duration_units() {
        assert_eq!(parse_duration("7d"), 7);
        assert_eq!(parse_duration("2w"), 14);
        assert_eq!(parse_duration("1m"), 30);
    }

    #[test]
    fn date_round_trip() {
        let d = parse_date("2024-01-15").unwrap();
        assert_eq!(format_short_date(d), "2024-01-15");
        // 1970-01-01 is day 0
        assert_eq!(days_from_epoch(1970, 1, 1), 0);
    }

    #[test]
    fn empty_gantt_returns_none() {
        assert!(render("gantt\n  dateFormat YYYY-MM-DD\n  section S\n").is_none());
    }

    #[test]
    fn after_dependency_draws_arrow_with_prefixed_defs() {
        let out = render(
            "gantt\n  title Project\n  section Design\n  Prototype :p1, 2026-08-01, 5d\n  UI        :u1, after p1, 2d\n",
        )
        .expect("render");
        // Defs ids derive from the source (not user labels).
        assert!(out.contains("<defs>"), "expected defs block");
        assert!(
            out.contains("gantt-arrow-") && out.contains("gantt-shadow-"),
            "ids should carry the gantt- prefix: {out}"
        );
        assert!(
            out.contains("marker-end=\"url(#gantt-arrow-"),
            "dependency arrow must reference the prefixed marker"
        );
        // The bar is wide enough to carry the in-bar label.
        assert!(out.contains("Prototype (5d)"));
    }

    #[test]
    fn single_section_gantt_uses_section_colour() {
        let out = render("gantt\n  section S\n  T1 :a1, 2024-01-01, 30d\n").expect("render");
        assert!(
            out.contains("#5b8def"),
            "first palette colour for first section"
        );
    }
}
