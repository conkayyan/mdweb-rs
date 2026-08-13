//! Native SVG diagram renderer.
//!
//! Five Mermaid / Graphviz-style diagram families are supported:
//!
//! - [`flowchart`] — `flowchart TD|LR|RL|BT` / `graph …` with all the
//!   standard node shapes and edge operators.
//! - [`pie`] — `pie title …` with `label : value` rows.
//! - [`gantt`] — `gantt` with `dateFormat`, `section`, task bars and
//!   `after` dependencies.
//! - [`class`] — `classDiagram` with class headers, fields, methods,
//!   and UML relationship arrows.
//! - [`dot`] — Graphviz `digraph`/`graph` syntax with `->`/`--` edges
//!   and quoted/identifier nodes.
//! - [`plantuml`] — PlantUML `@startuml` class diagrams with bodies,
//!   stereotypes, cardinalities and relation labels.
//!
//! All renderers share the [`common`] helpers (text measurement,
//! shape paths, palette, escape). The public entry point is
//! [`render`] — it dispatches to the right submodule based on the
//! first non-blank line of the source. Returns `None` when no
//! renderer recognises the source so the caller can fall back to a
//! fenced code block.

pub mod class;
pub mod common;
pub mod dot;
pub mod flowchart;
pub mod gantt;
pub mod pie;
pub mod plantuml;

/// Dispatch a diagram source string to the right renderer. Returns
/// `None` when nothing recognised the syntax.
pub fn render(src: &str) -> Option<String> {
    let trimmed = src.trim_start();
    let first_line = trimmed.lines().next().unwrap_or("").trim();
    let first = first_line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    // `flowchart` / `graph` are flowchart directives — keep their
    // original casing in the renderer but inspect via the lowered form.
    if first == "flowchart" || first == "graph" {
        return flowchart::render(src);
    }
    match first.as_str() {
        "pie" => pie::render(src),
        "gantt" => gantt::render(src),
        "classdiagram" => class::render(src),
        "@startuml" => plantuml::render(src),
        "digraph" | "graph" | "strictdigraph" | "strictgraph" => dot::render(src),
        _ => None,
    }
}

/// Which renderer family `render` would dispatch to for `src`. Lets
/// `markdown.rs` pick the right fenced-code-block class without
/// running the full pipeline twice.
pub fn kind_of(src: &str) -> Option<&'static str> {
    let trimmed = src.trim_start();
    let first_line = trimmed.lines().next().unwrap_or("").trim();
    let first = first_line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match first.as_str() {
        "flowchart" | "graph" => Some("flowchart"),
        "pie" => Some("pie"),
        "gantt" => Some("gantt"),
        "classdiagram" => Some("class"),
        "@startuml" => Some("plantuml"),
        "digraph" | "strictdigraph" | "strictgraph" => Some("dot"),
        _ => None,
    }
}
