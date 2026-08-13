//! PlantUML `@startuml` class diagram → SVG renderer.
//!
//! Supported syntax (a pragmatic subset of the real PlantUML grammar):
//!
//! ```text
//! @startuml
//! interface Vehicle {
//!   + start()
//!   + stop()
//! }
//! abstract class AbstractCar {
//!   - model: String
//!   # speed: int
//!   + AbstractCar(model: String)
//!   + accelerate()
//!   + getModel(): String
//! }
//! class Sedan { + Sedan(model: String) }
//! class Engine { - horsePower: int }
//! Vehicle <|.. AbstractCar    ' realization
//! AbstractCar <|-- Sedan      ' inheritance
//! AbstractCar *-- Engine      ' composition
//! AbstractCar <--> Driver     ' bidirectional association
//! AbstractCar ..> Manufacturer : depends on
//! note right of Sedan : 轿车
//! @enduml
//! ```
//!
//! Class declarations (`class`, `interface`, `enum`, `entity`, `abstract`,
//! `abstract class`, `annotation`, …) with `{ … }` member bodies, `as Alias`,
//! `<<stereotype>>`, the `extends` / `implements` keywords, quoted
//! cardinalities, relation labels and attached `note right|left|top|bottom of X`
//! are all handled. Relationship semantics follow **real PlantUML** output
//! (verified against the reference implementation): for a line `A <|-- B` the
//! hollow triangle is drawn at `A` (the supertype), which is placed above `B`;
//! `A *-- B` puts the filled diamond at `A` (the whole).
//!
//! | Operator | Kind | Marker (side) |
//! | --- | --- | --- |
//! | `<|--` / `<|..` | extension / realization | hollow triangle (left) |
//! | `--|>` / `..|>` | extension / realization reversed | hollow triangle (right) |
//! | `*--` / `--*` | composition | filled diamond (glyph side) |
//! | `o--` / `--o` | aggregation | hollow diamond (glyph side) |
//! | `-->` / `..>` | dependency | filled arrow (right) |
//! | `<--` | reversed dependency | filled arrow (left) |
//! | `<-->` / `<..>` | bidirectional association | filled arrows (both) |
//! | `--` / `..` | association | none |
//!
//! `skinparam`, `hide`/`show`, `package`, `together`, `!define` directives,
//! multi-target `note over A, B`, `'` comments and bracketed inline styles
//! (`-[bold]->`) are skipped. Returns `None` when no classes or relations could
//! be parsed (the caller falls back to a fenced code block).

use super::common::{approx_text_width, escape_text, FONT_FAMILY};
use std::collections::HashMap;

/// Hard cap on the number of classes before the renderer gives up and the
/// caller falls back to a fenced code block (resource-cap discipline: bounded
/// work per untrusted input).
const MAX_CLASSES: usize = 250;
/// Hard cap on the number of relations.
const MAX_RELATIONS: usize = 600;

/// Render a PlantUML class diagram source to SVG. `None` when the source does
/// not contain any class declarations or relations.
pub fn render(src: &str) -> Option<String> {
    let (mut classes, mut relations, mut notes, title) = parse_src(src)?;
    layout(&mut classes, &relations, !title.is_empty());
    place_notes(&mut notes, &classes);
    // Notes hanging off the left edge or above the top row may go negative;
    // shift the whole drawing back into the first quadrant.
    let mut min_x = 0.0_f64;
    let mut min_y = 0.0_f64;
    for c in &classes {
        min_x = min_x.min(c.x);
        min_y = min_y.min(c.y);
    }
    for n in &notes {
        min_x = min_x.min(n.x);
        min_y = min_y.min(n.y);
    }
    if min_x < 0.0 || min_y < 0.0 {
        for c in classes.iter_mut() {
            c.x -= min_x;
            c.y -= min_y;
        }
        for n in notes.iter_mut() {
            n.x -= min_x;
            n.y -= min_y;
        }
    }
    assign_ports(&classes, &mut relations);
    route_horizontal(&classes, &mut relations);
    Some(assemble(&classes, &relations, &notes, &title, src))
}

/// Parse and resolve a PlantUML source into classes, relations, notes and the
/// optional title. `None` when nothing renderable could be extracted.
#[allow(clippy::type_complexity)]
fn parse_src(src: &str) -> Option<(Vec<Class>, Vec<Relation>, Vec<Note>, String)> {
    let mut title = String::new();
    let mut classes: Vec<Class> = Vec::new();
    let mut idx: HashMap<String, usize> = HashMap::new();
    let mut raw_rels: Vec<String> = Vec::new();
    let mut notes: Vec<Note> = Vec::new();
    let mut in_class: Option<usize> = None;

    for raw in src.lines() {
        let line = strip_comment(raw);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('@') || line.starts_with('!') {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("title") {
            title = line[5..].trim().to_string();
            continue;
        }
        if line == "}" {
            in_class = None;
            continue;
        }
        // Class member bodies are the only block that collects lines.
        if let Some(ci) = in_class {
            if let Some(m) = parse_member(line) {
                classes[ci].members.push(m);
            }
            continue;
        }
        // `note <side> of X : text` — attached to a class, rendered as a
        // sticky note next to it. Unparseable note forms (e.g. `note over A, B`)
        // are skipped by `is_skip_line`.
        if lower.starts_with("note ") || lower == "note" {
            if let Some(note) = parse_note(line, &idx) {
                notes.push(note);
            }
            continue;
        }
        if is_skip_line(&lower) {
            continue;
        }
        if let Some(decl) = parse_declaration(line) {
            let ci = register(&mut classes, &mut idx, &decl.id, &decl.name, &decl.stereo);
            if decl.block {
                in_class = Some(ci);
            }
            for (target, kind) in decl.ext_impl {
                let op = if kind == "implements" { "<|.." } else { "<|--" };
                raw_rels.push(format!("{target} {op} {}", decl.id));
            }
            continue;
        }
        // package / namespace / together openers, their closing `}` (skipped
        // above), and anything else is a potential relation line; unresolved
        // lines are ignored at resolution time.
        raw_rels.push(line.to_string());
    }

    if classes.len() > MAX_CLASSES {
        return None;
    }

    let mut relations: Vec<Relation> = Vec::new();
    for raw in &raw_rels {
        if relations.len() > MAX_RELATIONS {
            break;
        }
        if let Some(r) = resolve_relation(raw, &mut classes, &mut idx) {
            relations.push(r);
        }
    }
    if classes.is_empty() {
        return None;
    }

    Some((classes, relations, notes, title))
}

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Class {
    name: String,
    stereo: String,
    members: Vec<Member>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Class {
    fn new(name: String, stereo: String) -> Self {
        Class {
            name,
            stereo,
            members: Vec::new(),
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum MemberVis {
    Public,
    Private,
    Protected,
    Package,
}

#[derive(Clone)]
struct Member {
    vis: MemberVis,
    text: String,
}

/// Which side of its target class a `note … of X` sticks to.
#[derive(Clone, Copy, PartialEq)]
enum NoteSide {
    Left,
    Right,
    Top,
    Bottom,
}

/// The edge of a class box a relation connects to.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    fn is_horizontal(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }
}

#[derive(Clone)]
struct Note {
    side: NoteSide,
    /// Index into `classes` of the class the note belongs to.
    target: usize,
    text: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum LineKind {
    Solid,
    Dashed,
}

/// The marker drawn at one end of a relation.
#[derive(Clone, Copy)]
enum Marker {
    /// Hollow triangle (inheritance / realization). Apex touches the box edge.
    Triangle,
    /// Solid triangle (dependency direction arrow).
    Arrow,
    /// Hollow diamond (aggregation).
    Diamond,
    /// Solid diamond (composition).
    FilledDiamond,
}

impl Marker {
    /// How far the marker extends away from the box edge (the line starts
    /// there).
    fn depth(self) -> f64 {
        match self {
            Marker::Triangle | Marker::Arrow => 18.0,
            Marker::Diamond | Marker::FilledDiamond => 10.0,
        }
    }
}

#[derive(Clone)]
struct Relation {
    from: usize,
    to: usize,
    kind: LineKind,
    from_marker: Option<Marker>,
    to_marker: Option<Marker>,
    label: String,
    from_card: String,
    to_card: String,
    /// Connection polyline: `path[0]` on the `from` box edge, `path[last]` on
    /// the `to` box edge. Computed by [`assign_ports`] / [`route_horizontal`]
    /// after layout.
    path: Vec<(f64, f64)>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Strip a PlantUML line comment (`' …`) outside of double-quoted strings.
fn strip_comment(line: &str) -> String {
    let mut in_quote = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quote = !in_quote,
            '\'' if !in_quote => return line[..i].to_string(),
            _ => {}
        }
    }
    line.to_string()
}

/// Lines that are configuration, notes, or other non-class constructs.
fn is_skip_line(lower: &str) -> bool {
    let first = lower.split_whitespace().next().unwrap_or("");
    matches!(
        first,
        "skinparam"
            | "hide"
            | "show"
            | "note"
            | "page"
            | "scale"
            | "set"
            | "left"
            | "right"
            | "participant"
            | "actor"
            | "activate"
            | "deactivate"
            | "autonumber"
            | "loop"
            | "alt"
            | "else"
            | "opt"
            | "group"
            | "end"
            | "ref"
            | "salience"
    )
}

struct Decl {
    id: String,
    name: String,
    stereo: String,
    block: bool,
    /// `(target, "extends" | "implements")` pairs from the declaration line.
    ext_impl: Vec<(String, String)>,
}

/// Parse a class declaration line (`class A`, `interface B {`,
/// `abstract class C as D <<stereotype>>`, `class E extends F`, …). Returns
/// `None` when the line is not a declaration.
fn parse_declaration(line: &str) -> Option<Decl> {
    let mut s = line.trim().to_string();
    // A leading visibility marker may prefix the keyword (`-class Foo {}`).
    if let Some(c) = s.chars().next() {
        if matches!(c, '+' | '-' | '#' | '~') {
            s = s[1..].trim_start().to_string();
        }
    }
    // Key words, longest first. `abstract` alone is an abstract class.
    const KEYWORDS: &[&str] = &[
        "abstract class",
        "abstract",
        "interface",
        "annotation",
        "class",
        "enum",
        "entity",
        "struct",
        "record",
        "exception",
        "protocol",
        "dataclass",
        "metaclass",
    ];
    let mut rest = None;
    for kw in KEYWORDS {
        if let Some(r) = s.strip_prefix(kw) {
            if r.is_empty() || r.starts_with(' ') || r.starts_with('"') || r.starts_with('{') {
                rest = Some(r.trim_start());
                break;
            }
        }
    }
    let mut rest = rest?.to_string();
    if rest.is_empty() {
        return None;
    }

    let block = rest.contains('{');

    let mut stereo = String::new();
    rest = strip_stereotype_into(&rest, &mut stereo);

    let (name_region, targets) = split_ext_impl(&rest);
    let name_region = cut_annotations(name_region);
    let (name, alias) = parse_name_alias(&name_region);
    let id = if alias.is_empty() {
        name.clone()
    } else {
        alias
    };
    if id.is_empty() {
        return None;
    }

    let mut ext_impl = Vec::new();
    if let Some((ts, kind)) = targets {
        for t in ts.split(',') {
            let t = unquote(
                t.trim()
                    .trim_start_matches('{')
                    .trim_end_matches('}')
                    .trim(),
            );
            if t.is_empty() {
                continue;
            }
            ext_impl.push((t, kind.to_string()));
        }
    }

    Some(Decl {
        id,
        name,
        stereo,
        block,
        ext_impl,
    })
}

/// Remove a `<<…>>` stereotype at the end of a declaration region and return
/// it separately.
fn strip_stereotype_into(s: &str, out: &mut String) -> String {
    match (s.find("<<"), s.find(">>")) {
        (Some(lo), Some(hi)) if lo < hi => {
            *out = s[lo + 2..hi].trim().to_string();
            let mut t = String::with_capacity(s.len());
            t.push_str(&s[..lo]);
            t.push_str(&s[hi + 2..]);
            t
        }
        _ => s.to_string(),
    }
}

/// Split a declaration region into (name region, (`targets`, kind)) where the
/// keyword is `implements` or `extends` and kind is `"implements"` /
/// `"extends"`.
fn split_ext_impl(s: &str) -> (String, Option<(&str, &str)>) {
    if let Some(i) = word_index(s, "implements") {
        (
            s[..i].to_string(),
            Some((&s[i + "implements".len()..], "implements")),
        )
    } else if let Some(i) = word_index(s, "extends") {
        (
            s[..i].to_string(),
            Some((&s[i + "extends".len()..], "extends")),
        )
    } else {
        (s.to_string(), None)
    }
}

/// Cut colour / `$tag` annotations (`class Foo #red`, `class $C1`) off the
/// name region of a declaration.
fn cut_annotations(s: String) -> String {
    let cut = ["##", " #", " $"].iter().filter_map(|d| s.find(d)).min();
    match cut {
        Some(i) => s[..i].trim_end().to_string(),
        None => s,
    }
}

/// Split a declaration region into (display name, alias), handling quoted
/// display names (`class "My Class" as c`).
fn parse_name_alias(region: &str) -> (String, String) {
    let mut region = region;
    let mut alias = String::new();
    if let Some(i) = word_index(region, "as") {
        alias = unquote(region[i + 2..].trim());
        region = &region[..i];
    }
    let name = unquote(
        region
            .trim()
            .trim_end_matches('{')
            .trim_end_matches('}')
            .trim(),
    );
    (name, alias)
}

/// Index of a whole word `kw` inside `s` (preceded by start/whitespace and
/// followed by whitespace/`{`/`,`/`}`/end), outside double quotes. Byte-safe
/// across CJK and other multibyte identifiers.
fn word_index(s: &str, kw: &str) -> Option<usize> {
    let mut in_quote = false;
    let mut prev_ws = true;
    let mut i = 0usize;
    while i < s.len() {
        let c = s[i..].chars().next().unwrap();
        match c {
            '"' => in_quote = !in_quote,
            ' ' | '\t' => prev_ws = true,
            _ => {
                if prev_ws && !in_quote && s[i..].starts_with(kw) {
                    let after = i + kw.len();
                    let ok_after = after >= s.len()
                        || matches!(
                            s.as_bytes()[after],
                            b' ' | b'\t' | b'{' | b',' | b'}' | b'\r' | b'\n'
                        );
                    if ok_after {
                        return Some(i);
                    }
                }
                prev_ws = false;
            }
        }
        i += c.len_utf8();
    }
    None
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn parse_member(line: &str) -> Option<Member> {
    let mut s = line.trim().to_string();
    if s == "{" || s == "}" || s == ";" {
        return None;
    }
    // Strip {field},{method},{static},{abstract},{classifier} modifiers.
    for m in [
        "{field}",
        "{method}",
        "{static}",
        "{abstract}",
        "{classifier}",
    ] {
        while let Some(i) = s.to_ascii_lowercase().find(m) {
            s.replace_range(i..i + m.len(), " ");
        }
    }
    let s = s.trim().to_string();
    if s.is_empty() {
        return None;
    }
    // Separator rows in advanced class bodies are dropped.
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("--")
        || lower.starts_with("..")
        || lower.starts_with("==")
        || lower.starts_with("__")
    {
        return None;
    }
    let mut vis = MemberVis::Public;
    let mut t = s.as_str();
    if let Some(c) = t.chars().next() {
        match c {
            '+' => {
                vis = MemberVis::Public;
                t = t[1..].trim_start();
            }
            '-' => {
                vis = MemberVis::Private;
                t = t[1..].trim_start();
            }
            '#' => {
                vis = MemberVis::Protected;
                t = t[1..].trim_start();
            }
            '~' => {
                vis = MemberVis::Package;
                t = t[1..].trim_start();
            }
            '\\' => {
                t = &t[1..];
                if t.starts_with('~') {
                    vis = MemberVis::Package;
                    t = &t[1..];
                }
            }
            _ => {}
        }
    }
    let text = t.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(Member { vis, text })
}

/// Find the relation operator (a run of `-`/`.`, optionally framed by a
/// marker from `< > | * o`). Returns the span and the operator text.
fn find_operator(line: &str) -> Option<(usize, usize, &str)> {
    let bytes = line.as_bytes();
    let mut best: Option<(usize, usize, &str)> = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'-' || bytes[i] == b'.' {
            let run_start = i;
            let mut j = i;
            while j < bytes.len() && (bytes[j] == b'-' || bytes[j] == b'.') {
                j += 1;
            }
            let mut lo = run_start;
            let mut hi = j;
            while lo > 0 && matches!(bytes[lo - 1], b'<' | b'>' | b'|' | b'*' | b'o') {
                lo -= 1;
            }
            while hi < bytes.len() && matches!(bytes[hi], b'<' | b'>' | b'|' | b'*' | b'o') {
                hi += 1;
            }
            // Require either a doubled run (`--`) or a run wearing a marker
            // (`o-`, `..|>`), so a lone dash inside an identifier never fires.
            let acceptable = (j - run_start >= 2) || (lo < run_start || hi > j);
            if acceptable {
                let width = hi - lo;
                if best.is_none_or(|(blo, bhi, _)| {
                    width > bhi - blo || (width == bhi - blo && lo < blo)
                }) {
                    best = Some((lo, hi, &line[lo..hi]));
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    best
}

/// Decode an operator string into (line kind, marker at left, marker at right).
/// The two ends are decoded independently, so bidirectional operators like
/// `<-->` yield an arrow at both ends.
fn shape_of(op: &str) -> (LineKind, Option<Marker>, Option<Marker>) {
    let kind = if op.contains('.') {
        LineKind::Dashed
    } else {
        LineKind::Solid
    };
    let first = op
        .char_indices()
        .find(|(_, c)| *c == '-' || *c == '.')
        .map(|(i, _)| i)
        .unwrap_or(op.len());
    let last = op
        .char_indices()
        .rfind(|(_, c)| *c == '-' || *c == '.')
        .map(|(i, _)| i)
        .unwrap_or(0);
    let pre = &op[..first];
    let post = if last < op.len() { &op[last + 1..] } else { "" };
    let from = if pre.contains('|') {
        Some(Marker::Triangle)
    } else if pre.contains('*') {
        Some(Marker::FilledDiamond)
    } else if pre.contains('o') {
        Some(Marker::Diamond)
    } else if pre.contains('<') {
        Some(Marker::Arrow)
    } else {
        None
    };
    let to = if post.contains('|') {
        Some(Marker::Triangle)
    } else if post.contains('*') {
        Some(Marker::FilledDiamond)
    } else if post.contains('o') {
        Some(Marker::Diamond)
    } else if post.contains('>') {
        Some(Marker::Arrow)
    } else {
        None
    };
    (kind, from, to)
}

/// An endpoint of a relation: the class identifier plus an optional quoted
/// multiplicity (`Class01 "1"`, `"many" Class02`).
struct Endpoint {
    id: String,
    card: String,
}

fn parse_endpoint(s: &str) -> Endpoint {
    let s = s.trim();
    let mut card = String::new();
    let mut name = String::new();
    let mut rest = s;
    let mut first_quote = true;
    while let Some(q) = rest.find('"') {
        let after = &rest[q + 1..];
        match after.find('"') {
            Some(e) => {
                let inner = &after[..e];
                name.push_str(&rest[..q]);
                if first_quote {
                    card = inner.to_string();
                    first_quote = false;
                }
                name.push(' ');
                rest = &after[e + 1..];
            }
            None => break,
        }
    }
    name.push_str(rest);
    let name = name.trim();
    if name.is_empty() && !card.is_empty() {
        // The whole endpoint is a single quoted string → it is the name.
        Endpoint {
            id: card,
            card: String::new(),
        }
    } else {
        Endpoint {
            id: name.to_string(),
            card,
        }
    }
}

/// Split a right-hand side into (endpoint, label): the label follows the
/// first `:` outside double quotes.
fn split_label(s: &str) -> (&str, &str) {
    let mut in_quote = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_quote = !in_quote,
            ':' if !in_quote => return (s[..i].trim(), s[i + 1..].trim()),
            _ => {}
        }
    }
    (s.trim(), "")
}

/// Parse `note <side> of X : text` into a [`Note`] attached to `X`. `side` is
/// one of `right`/`left`/`top`/`bottom` (defaults to `right`); `over` degrades
/// to `top`. Returns `None` when the target class is unknown or the form does
/// not match.
fn parse_note(line: &str, idx: &HashMap<String, usize>) -> Option<Note> {
    let rest = line["note".len()..].trim_start();
    let (target_part, text) = split_label(rest);
    if text.is_empty() {
        return None;
    }
    let mut words = target_part.split_whitespace();
    let side = match words.next()? {
        "left" => NoteSide::Left,
        "top" | "over" => NoteSide::Top,
        "bottom" => NoteSide::Bottom,
        _ => NoteSide::Right,
    };
    let target = match words.next()? {
        "of" => words.next()?,
        t => t,
    };
    let target = *idx.get(target)?;
    Some(Note {
        side,
        target,
        text: text.to_string(),
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    })
}

fn resolve_relation(
    raw: &str,
    classes: &mut Vec<Class>,
    idx: &mut HashMap<String, usize>,
) -> Option<Relation> {
    let (lo, hi, op) = find_operator(raw)?;
    let (kind, from_marker, to_marker) = shape_of(op);
    let left_text = raw[..lo].trim();
    let (right_text, label) = split_label(&raw[hi..]);
    let left = parse_endpoint(left_text);
    let right = parse_endpoint(right_text);
    if left.id.is_empty() || right.id.is_empty() {
        return None;
    }
    let from = class_index(classes, idx, &left.id);
    let to = class_index(classes, idx, &right.id);
    if from == to {
        return None; // self-relation: skipped
    }
    Some(Relation {
        from,
        to,
        kind,
        from_marker,
        to_marker,
        label: label.to_string(),
        from_card: left.card,
        to_card: right.card,
        path: vec![(0.0, 0.0), (0.0, 0.0)],
    })
}

/// Get or auto-create the class with the given identifier (PlantUML creates
/// classes referenced by relations that were never declared).
fn class_index(classes: &mut Vec<Class>, idx: &mut HashMap<String, usize>, id: &str) -> usize {
    if let Some(&i) = idx.get(id) {
        return i;
    }
    classes.push(Class::new(id.to_string(), String::new()));
    idx.insert(id.to_string(), classes.len() - 1);
    classes.len() - 1
}

fn register(
    classes: &mut Vec<Class>,
    idx: &mut HashMap<String, usize>,
    id: &str,
    name: &str,
    stereo: &str,
) -> usize {
    if let Some(&i) = idx.get(id) {
        // A later declaration may carry a nicer display name / stereotype
        // than the token an auto-created class was named from.
        if !name.is_empty() {
            classes[i].name = name.to_string();
        }
        if !stereo.is_empty() {
            classes[i].stereo = stereo.to_string();
        }
        return i;
    }
    classes.push(Class::new(name.to_string(), stereo.to_string()));
    idx.insert(id.to_string(), classes.len() - 1);
    classes.len() - 1
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// Layered layout (Sugiyama-lite): supertypes above subtypes, wholes above
/// parts. Inheritance / realization (triangle) and composition / aggregation
/// (diamond) relations force a strict parent→child layer; dependency,
/// association and bidirectional edges only pin a node no higher than its
/// partner, so they never blow the diagram up into one column per relation.
/// Layers are ordered with a barycenter sweep and centred on a shared column
/// grid, which keeps the fan-out from a shared supertype compact and crossing
/// light. Isolated classes sit on the top row.
fn layout(classes: &mut [Class], relations: &[Relation], has_title: bool) {
    let n = classes.len();
    // Measure every box.
    for c in classes.iter_mut() {
        let (w, h) = box_size(c);
        c.w = w;
        c.h = h;
    }

    // Structural (parent→child) and weak (partner-pinning) edges.
    let mut strong: Vec<(usize, usize)> = Vec::new();
    let mut weak: Vec<(usize, usize)> = Vec::new();
    for r in relations {
        if let Some((p, c)) = structural_parent(r) {
            strong.push((p, c));
        } else {
            weak.push((r.from, r.to));
            if r.from_marker.is_none() && r.to_marker.is_none() {
                weak.push((r.to, r.from)); // plain association: undirected
            }
        }
    }

    // Longest-path layering on the structural edges.
    let mut layer = vec![0usize; n];
    for _ in 0..=n {
        let mut changed = false;
        for &(p, c) in &strong {
            if layer[c] < layer[p] + 1 {
                layer[c] = layer[p] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Weak edges pull partners up into the same row (never past it, once).
    for &(a, b) in &weak {
        if layer[b] < layer[a] {
            layer[b] = layer[a];
        }
    }

    // Group into layers, then reduce crossings with barycenter sweeps.
    let max_layer = layer.iter().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_layer + 1];
    for (i, &l) in layer.iter().enumerate() {
        layers[l].push(i);
    }
    for _ in 0..4 {
        for li in 1..layers.len() {
            let (head, tail) = layers.split_at_mut(li);
            sort_by_barycenter(&mut tail[0], &head[li - 1], relations);
        }
        for li in (0..layers.len() - 1).rev() {
            let (head, tail) = layers.split_at_mut(li + 1);
            sort_by_barycenter(&mut head[li], &tail[0], relations);
        }
    }

    // Hub centering: within each layer, the most connected nodes drift toward
    // the middle columns (each node's rank is blended toward the centre in
    // proportion to its degree), so a wide layer reads as one centred
    // composition instead of the hubs sitting on the far edges. Leaves and
    // layers without hubs keep the sweep's alignment.
    center_hubs(&mut layers, relations);

    // Centred column grid: each layer sits in a contiguous, centred slice of
    // the shared columns so edges stay near-vertical.
    let n_cols = layers.iter().map(|l| l.len()).max().unwrap_or(1);
    let mut col_w = vec![0.0_f64; n_cols];
    let mut row_h = vec![0.0_f64; layers.len()];
    for (li, l) in layers.iter().enumerate() {
        let start = (n_cols - l.len()) / 2;
        for (k, &ci) in l.iter().enumerate() {
            col_w[start + k] = col_w[start + k].max(classes[ci].w);
            row_h[li] = row_h[li].max(classes[ci].h);
        }
    }

    let pad_x = 24.0_f64;
    let col_gap = 56.0_f64;
    let mut col_x: Vec<f64> = Vec::with_capacity(n_cols);
    let mut cursor = pad_x;
    for w in &col_w {
        col_x.push(cursor);
        cursor += w + col_gap;
    }

    let row0_y = if has_title { 54.0 } else { 30.0 };
    let row_gap = 46.0_f64;
    let mut row_y: Vec<f64> = Vec::with_capacity(layers.len());
    let mut y = row0_y;
    for &h in &row_h {
        row_y.push(y);
        y += h + row_gap;
    }

    for (i, c) in classes.iter_mut().enumerate() {
        let li = layer[i];
        let k = layers[li].iter().position(|&x| x == i).unwrap_or(0);
        let col = (n_cols - layers[li].len()) / 2 + k;
        let cx = col_x[col] + col_w[col] / 2.0;
        c.x = cx - c.w / 2.0;
        c.y = row_y[li];
    }
}

/// For a structural relation, the (parent, child) pair the layout must honour.
/// Triangle markers mark the supertype, diamond markers the whole; the other
/// end is the subtype / part.
fn structural_parent(r: &Relation) -> Option<(usize, usize)> {
    use Marker::*;
    match (r.from_marker, r.to_marker) {
        (Some(Triangle), _) => Some((r.from, r.to)),
        (_, Some(Triangle)) => Some((r.to, r.from)),
        (Some(FilledDiamond | Diamond), _) => Some((r.from, r.to)),
        (_, Some(FilledDiamond | Diamond)) => Some((r.to, r.from)),
        _ => None,
    }
}

/// Stable-sort `layer` by the mean position of its neighbours in the adjacent
/// layer; nodes with no neighbours there keep their relative order.
fn sort_by_barycenter(layer: &mut [usize], adj: &[usize], relations: &[Relation]) {
    layer.sort_by(|&a, &b| {
        let ba = barycenter(a, adj, relations).unwrap_or(f64::INFINITY);
        let bb = barycenter(b, adj, relations).unwrap_or(f64::INFINITY);
        ba.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
    });
}

fn barycenter(node: usize, adj: &[usize], relations: &[Relation]) -> Option<f64> {
    let mut sum = 0.0_f64;
    let mut cnt = 0usize;
    for (i, &b) in adj.iter().enumerate() {
        if relations
            .iter()
            .any(|r| (r.from == node && r.to == b) || (r.from == b && r.to == node))
        {
            sum += i as f64;
            cnt += 1;
        }
    }
    if cnt == 0 {
        None
    } else {
        Some(sum / cnt as f64)
    }
}

/// Re-order each layer so the most connected members sit closest to the
/// layer's centre. A hill-climb swaps adjacent members whenever the member on
/// the far side of the layer's midpoint is more connected, so hubs migrate
/// toward the middle columns while equal-degree groups keep the barycenter
/// sweep's relative order.
fn center_hubs(layers: &mut [Vec<usize>], relations: &[Relation]) {
    if layers.len() < 2 {
        return;
    }
    let max_id = layers.iter().flatten().copied().max().unwrap_or(0);
    let mut degree = vec![0usize; max_id + 1];
    for r in relations {
        degree[r.from] += 1;
        degree[r.to] += 1;
    }
    for layer in layers.iter_mut() {
        let n = layer.len();
        if n < 3 {
            continue;
        }
        let mid = (n - 1) as f64 / 2.0;
        for _ in 0..n {
            let mut moved = false;
            for k in 0..n - 1 {
                let (a, b) = (layer[k], layer[k + 1]);
                let (da, db) = (degree[a] as f64, degree[b] as f64);
                let (fa, fb) = ((k as f64 - mid).abs(), ((k + 1) as f64 - mid).abs());
                if (db > da && fa < fb) || (da > db && fb < fa) {
                    layer.swap(k, k + 1);
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Port assignment and edge routing
// ---------------------------------------------------------------------------

/// Which side of `a` faces `b`. Structural relations (supertype/subtype or
/// whole/part) always connect vertically so the hierarchy reads top-down;
/// other relations use the mutual overlap: same-row partners connect
/// horizontally, stacked partners vertically, and the dominant axis decides
/// the remaining diagonal cases.
fn vertical_side(dy: f64) -> Side {
    if dy > 0.0 {
        Side::Bottom
    } else {
        Side::Top
    }
}

fn facing_side(a: &Class, b: &Class, structural: bool) -> Side {
    let dx = (b.x + b.w / 2.0) - (a.x + a.w / 2.0);
    let dy = (b.y + b.h / 2.0) - (a.y + a.h / 2.0);
    if structural {
        vertical_side(dy)
    } else {
        let x_overlap = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
        let y_overlap = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
        if y_overlap > 0.0 {
            if dx > 0.0 {
                Side::Right
            } else {
                Side::Left
            }
        } else if x_overlap > 0.0 || dy.abs() > dx.abs() {
            vertical_side(dy)
        } else if dx > 0.0 {
            Side::Right
        } else {
            Side::Left
        }
    }
}

/// The centre of the class at the other end of `r` from `ci`.
fn partner_center(ci: usize, r: &Relation, classes: &[Class]) -> (f64, f64) {
    let p = if r.from == ci { r.to } else { r.from };
    let c = &classes[p];
    (c.x + c.w / 2.0, c.y + c.h / 2.0)
}

/// The connection point `r` currently uses on class `ci`, if any.
fn port_on(r: &Relation, ci: usize) -> Option<(f64, f64)> {
    if r.from == ci {
        r.path.first().copied()
    } else if r.to == ci {
        r.path.last().copied()
    } else {
        None
    }
}

/// Assign every relation a distinct connection point on the facing edge of
/// each endpoint, spread so a class with many relations uses ports all around
/// the sides it faces instead of one shared midpoint. Ports on a side are
/// ordered by the partner's position along that side, so the fan out of a
/// class stays planar (no overlapping ports, no edges crossing each other
/// near the box).
fn assign_ports(classes: &[Class], relations: &mut [Relation]) {
    use std::cmp::Ordering;
    let mut groups: HashMap<(usize, Side), Vec<usize>> = HashMap::new();
    for (ri, r) in relations.iter().enumerate() {
        let structural = structural_parent(r).is_some();
        let sa = facing_side(&classes[r.from], &classes[r.to], structural);
        let sb = facing_side(&classes[r.to], &classes[r.from], structural);
        groups.entry((r.from, sa)).or_default().push(ri);
        groups.entry((r.to, sb)).or_default().push(ri);
    }
    for ((ci, side), members) in groups {
        let c = &classes[ci];
        let mut sorted = members;
        sorted.sort_by(|&x, &y| {
            let (ax, ay) = partner_center(ci, &relations[x], classes);
            let (bx, by) = partner_center(ci, &relations[y], classes);
            let (k1, k2) = if side.is_horizontal() { (ay, ax) } else { (ax, ay) };
            let (l1, l2) = if side.is_horizontal() { (by, bx) } else { (bx, by) };
            k1.partial_cmp(&l1)
                .unwrap_or(Ordering::Equal)
                .then_with(|| k2.partial_cmp(&l2).unwrap_or(Ordering::Equal))
        });
        let n = sorted.len() as f64;
        for (i, &ri) in sorted.iter().enumerate() {
            let f = (i as f64 + 1.0) / (n + 1.0);
            let port = match side {
                Side::Bottom => (c.x + c.w * f, c.y + c.h),
                Side::Top => (c.x + c.w * f, c.y),
                Side::Right => (c.x + c.w, c.y + c.h * f),
                Side::Left => (c.x, c.y + c.h * f),
            };
            let rel = &mut relations[ri];
            if rel.from == ci {
                rel.path[0] = port;
            } else {
                rel.path[1] = port;
            }
        }
    }
}

/// True when a port on class `ci` already sits within 4 px of height `y`.
fn port_occupied(ci: usize, y: f64, relations: &[Relation], ri: usize) -> bool {
    relations.iter().enumerate().any(|(j, r)| {
        j != ri && port_on(r, ci).is_some_and(|p| (p.1 - y).abs() < 4.0)
    })
}

/// The classes, current relation paths and endpoints involved in re-routing
/// one relation, threaded through the routing helpers.
struct RouteCtx<'a> {
    classes: &'a [Class],
    relations: &'a [Relation],
    ai: usize,
    bi: usize,
    ri: usize,
}

/// True when `path` runs through any class box (other than the endpoints) or
/// crosses any other relation's polyline.
fn path_blocked(path: &[(f64, f64)], ctx: &RouteCtx<'_>) -> bool {
    for k in 0..path.len().saturating_sub(1) {
        let p = path[k];
        let q = path[k + 1];
        for (i, c) in ctx.classes.iter().enumerate() {
            if i == ctx.ai || i == ctx.bi {
                continue;
            }
            if segment_hits_rect(p, q, (c.x, c.y, c.w, c.h)) {
                return true;
            }
        }
        for (j, r) in ctx.relations.iter().enumerate() {
            if j == ctx.ri {
                continue;
            }
            for m in 0..r.path.len().saturating_sub(1) {
                if segments_intersect(p, q, r.path[m], r.path[m + 1]) {
                    return true;
                }
            }
        }
    }
    false
}

/// Re-route horizontal relations so both ends sit on one shared horizontal y.
/// Returns the straightened path when a free band (clearing boxes, other
/// ports and other edges) exists.
fn straighten(
    a: &Class,
    b: &Class,
    p1: (f64, f64),
    p2: (f64, f64),
    ctx: &RouteCtx<'_>,
) -> Option<Vec<(f64, f64)>> {
    use std::cmp::Ordering;
    let lo = a.y.max(b.y) + 6.0;
    let hi = (a.y + a.h).min(b.y + b.h) - 6.0;
    if lo >= hi {
        return None;
    }
    let tcy = b.y + b.h / 2.0;
    let mut cands = vec![lo, hi, (lo + hi) / 2.0, tcy, p1.1, p2.1];
    cands.sort_by(|x, y| {
        (x - tcy)
            .abs()
            .partial_cmp(&(y - tcy).abs())
            .unwrap_or(Ordering::Equal)
    });
    let mut unique: Vec<f64> = Vec::with_capacity(cands.len());
    for y in cands {
        if !unique.iter().any(|u| (u - y).abs() < 1.0) {
            unique.push(y);
        }
    }
    for y in unique {
        if y < lo || y > hi {
            continue;
        }
        let path = vec![(p1.0, y), (p2.0, y)];
        if !path_blocked(&path, ctx)
            && !port_occupied(ctx.ai, y, ctx.relations, ctx.ri)
            && !port_occupied(ctx.bi, y, ctx.relations, ctx.ri)
        {
            return Some(path);
        }
    }
    None
}

/// Route a horizontal relation around the boxes blocking its straight path:
/// pick a free horizontal corridor inside the source's edge span, then drop
/// vertically onto the target edge when the corridor is outside the target's
/// span. Validated against boxes, other ports and other edges.
fn corridor_path(
    a: &Class,
    b: &Class,
    p2: (f64, f64),
    ctx: &RouteCtx<'_>,
) -> Option<Vec<(f64, f64)>> {
    use std::cmp::Ordering;
    let to_right = b.x + b.w / 2.0 >= a.x + a.w / 2.0;
    let xa = if to_right { a.x + a.w } else { a.x };
    let xb = if to_right { b.x } else { b.x + b.w };
    let lo_x = xa.min(xb);
    let hi_x = xa.max(xb);
    let tcy = b.y + b.h / 2.0;

    // Vertical bands blocked by boxes straddling the corridor, plus the ports
    // already claimed on either edge.
    let mut intervals: Vec<(f64, f64)> = Vec::new();
    for (i, c) in ctx.classes.iter().enumerate() {
        if i == ctx.ai || i == ctx.bi {
            continue;
        }
        if c.x + c.w > lo_x - 2.0 && c.x < hi_x + 2.0 {
            intervals.push((c.y - 2.0, c.y + c.h + 2.0));
        }
    }
    for (j, r) in ctx.relations.iter().enumerate() {
        if j == ctx.ri {
            continue;
        }
        if let Some(p) = port_on(r, ctx.ai) {
            intervals.push((p.1 - 4.0, p.1 + 4.0));
        }
        if let Some(p) = port_on(r, ctx.bi) {
            intervals.push((p.1 - 4.0, p.1 + 4.0));
        }
    }
    intervals.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(Ordering::Equal));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (s, e) in intervals {
        match merged.last_mut() {
            Some(last) if s <= last.1 => {
                if e > last.1 {
                    last.1 = e;
                }
            }
            _ => merged.push((s, e)),
        }
    }

    // Free corridors within the source edge's span.
    let top = a.y + 2.0;
    let bot = a.y + a.h - 2.0;
    let mut cands: Vec<f64> = Vec::new();
    let mut cursor = top;
    for (s, e) in &merged {
        if *s > cursor + 2.0 {
            gap_candidates(&mut cands, cursor, *s, tcy);
        }
        cursor = cursor.max(*e);
    }
    if bot > cursor + 2.0 {
        gap_candidates(&mut cands, cursor, bot, tcy);
    }
    cands.sort_by(|x, y| {
        (x - tcy)
            .abs()
            .partial_cmp(&(y - tcy).abs())
            .unwrap_or(Ordering::Equal)
    });

    for y in cands {
        if y < top || y > bot {
            continue;
        }
        let path = if y >= b.y + 2.0 && y <= b.y + b.h - 2.0 {
            vec![(xa, y), (xb, y)]
        } else {
            vec![(xa, y), (xb, y), (xb, p2.1)]
        };
        if !path_blocked(&path, ctx) && !port_occupied(ctx.ai, y, ctx.relations, ctx.ri) {
            return Some(path);
        }
    }
    None
}

/// Candidate corridor heights inside a free vertical band `[s, e]`.
fn gap_candidates(out: &mut Vec<f64>, s: f64, e: f64, tcy: f64) {
    for c in [s + 4.0, (s + e) / 2.0, e - 4.0, tcy.clamp(s, e)] {
        if c >= s && c <= e && !out.iter().any(|x| (x - c).abs() < 1.0) {
            out.push(c);
        }
    }
}

/// Re-route horizontal relations blocked by a box or another edge: straighten
/// onto a shared y, or detour through a free corridor. Repeated until the
/// layout stabilises so re-routing never reintroduces a crossing.
fn route_horizontal(classes: &[Class], relations: &mut [Relation]) {
    for _ in 0..4 {
        let mut changed = false;
        for ri in 0..relations.len() {
            let snapshot = relations[ri].path.clone();
            let mut best: Option<Vec<(f64, f64)>> = None;
            {
                let r = &relations[ri];
                if r.path.len() >= 2 {
                    let (ai, bi) = (r.from, r.to);
                    let a = &classes[ai];
                    let b = &classes[bi];
                    if facing_side(a, b, structural_parent(r).is_some()).is_horizontal() {
                        let ctx = RouteCtx {
                            classes,
                            relations: &*relations,
                            ai,
                            bi,
                            ri,
                        };
                        let p1 = r.path[0];
                        let p2 = *r.path.last().unwrap();
                        let straight = vec![p1, p2];
                        best = Some(
                            straighten(a, b, p1, p2, &ctx).unwrap_or_else(|| {
                                if !path_blocked(&straight, &ctx) {
                                    straight
                                } else {
                                    corridor_path(a, b, p2, &ctx).unwrap_or(straight)
                                }
                            }),
                        );
                    }
                }
            }
            if let Some(np) = best {
                if np != snapshot {
                    relations[ri].path = np;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// Liang–Barsky segment↔rect test; the rect is shrunk 0.5 px so lines grazing
/// a box outline are not treated as hits.
fn segment_hits_rect(p: (f64, f64), q: (f64, f64), rect: (f64, f64, f64, f64)) -> bool {
    let (x1, y1) = p;
    let (x2, y2) = q;
    let (rx, ry, rw, rh) = (rect.0 + 0.5, rect.1 + 0.5, rect.2 - 1.0, rect.3 - 1.0);
    if rw <= 0.0 || rh <= 0.0 {
        return false;
    }
    let (dx, dy) = (x2 - x1, y2 - y1);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    let p = [-dx, dx, -dy, dy];
    let q = [x1 - rx, rx + rw - x1, y1 - ry, ry + rh - y1];
    for k in 0..4 {
        if p[k] == 0.0 {
            if q[k] < 0.0 {
                return false;
            }
        } else {
            let t = q[k] / p[k];
            if p[k] < 0.0 {
                if t > t1 {
                    return false;
                }
                if t > t0 {
                    t0 = t;
                }
            } else {
                if t < t0 {
                    return false;
                }
                if t < t1 {
                    t1 = t;
                }
            }
        }
    }
    t0 <= t1
}

fn orient(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> f64 {
    (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
}

fn on_segment(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> bool {
    let eps = 0.5;
    cx >= ax.min(bx) - eps
        && cx <= ax.max(bx) + eps
        && cy >= ay.min(by) - eps
        && cy <= ay.max(by) + eps
}

fn segments_intersect(
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
) -> bool {
    let (ax, ay) = a;
    let (bx, by) = b;
    let (cx, cy) = c;
    let (dx, dy) = d;
    let o1 = orient(ax, ay, bx, by, cx, cy);
    let o2 = orient(ax, ay, bx, by, dx, dy);
    let o3 = orient(cx, cy, dx, dy, ax, ay);
    let o4 = orient(cx, cy, dx, dy, bx, by);
    let crossing = (o1 > 0.0 && o2 < 0.0 || o1 < 0.0 && o2 > 0.0)
        && (o3 > 0.0 && o4 < 0.0 || o3 < 0.0 && o4 > 0.0);
    if crossing {
        return true;
    }
    if o1 == 0.0 && on_segment(ax, ay, bx, by, cx, cy) {
        return true;
    }
    if o2 == 0.0 && on_segment(ax, ay, bx, by, dx, dy) {
        return true;
    }
    if o3 == 0.0 && on_segment(cx, cy, dx, dy, ax, ay) {
        return true;
    }
    if o4 == 0.0 && on_segment(cx, cy, dx, dy, bx, by) {
        return true;
    }
    false
}

/// Position each note beside its target class, sized from its wrapped text.
/// The requested side is tried first; when a neighbour class already occupies
/// that side (e.g. `note left of Sedan` next to `Wheel`), the note falls back
/// to the next free side so it never covers a class box.
/// Only the note's x/y/w/h and side change; the global shift for negative
/// coordinates happens after this, in `render`.
fn place_notes(notes: &mut [Note], classes: &[Class]) {
    const SIDES: [NoteSide; 4] = [
        NoteSide::Right,
        NoteSide::Left,
        NoteSide::Top,
        NoteSide::Bottom,
    ];
    for n in notes {
        let lines = note_lines(&n.text);
        n.w = lines
            .iter()
            .map(|l| measure(l, 11.0))
            .fold(24.0_f64, f64::max)
            + 24.0;
        n.h = lines.len() as f64 * 15.0 + 18.0;
        let preferred = n.side;
        let mut order = SIDES;
        let off = order.iter().position(|s| *s == preferred).unwrap_or(0);
        order.rotate_left(off);
        for &side in &order {
            n.side = side;
            place_note_at(n, classes);
            let clashes = classes
                .iter()
                .any(|c| rects_overlap((n.x, n.y, n.w, n.h), (c.x, c.y, c.w, c.h)));
            if !clashes {
                break;
            }
        }
    }
}

fn place_note_at(n: &mut Note, classes: &[Class]) {
    let c = &classes[n.target];
    match n.side {
        NoteSide::Right => {
            n.x = c.x + c.w + 18.0;
            n.y = c.y;
        }
        NoteSide::Left => {
            n.x = c.x - n.w - 18.0;
            n.y = c.y;
        }
        NoteSide::Top => {
            n.x = c.x + c.w / 2.0 - n.w / 2.0;
            n.y = c.y - n.h - 18.0;
        }
        NoteSide::Bottom => {
            n.x = c.x + c.w / 2.0 - n.w / 2.0;
            n.y = c.y + c.h + 18.0;
        }
    }
}

fn rects_overlap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// Wrap note text to a comfortable width (~200 px at 11 px), breaking long
/// words (CJK, URLs) by character when needed.
fn note_lines(text: &str) -> Vec<String> {
    let max_px = 200.0_f64;
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let cand = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if measure(&cand, 11.0) <= max_px || cur.is_empty() {
            cur = cand;
        } else {
            out.push(cur);
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    // Hard-break any single token that still overflows.
    let mut hard: Vec<String> = Vec::new();
    for l in out {
        if measure(&l, 11.0) <= max_px {
            hard.push(l);
            continue;
        }
        let mut cur = String::new();
        for ch in l.chars() {
            let cand = format!("{cur}{ch}");
            if measure(&cand, 11.0) <= max_px {
                cur = cand;
            } else {
                hard.push(cur);
                cur = ch.to_string();
            }
        }
        if !cur.is_empty() {
            hard.push(cur);
        }
    }
    hard
}

/// Content size of a class box: width from the widest label, height from the
/// member rows.
fn box_size(c: &Class) -> (f64, f64) {
    let mut max_w = measure(&c.name, 14.0).max(60.0);
    if !c.stereo.is_empty() {
        max_w = max_w.max(measure(&c.stereo, 10.5) + 16.0);
    }
    for m in &c.members {
        max_w = max_w.max(measure(&m.text, 11.0) + 22.0);
    }
    let w = (max_w + 16.0 * 2.0).max(100.0);
    let h = if c.members.is_empty() {
        40.0
    } else {
        42.0 + c.members.len() as f64 * 15.0
    };
    (w, h)
}

/// Estimated rendered width of `text` at `font_px` (scaled from the shared
/// 12 px heuristic).
fn measure(text: &str, font_px: f64) -> f64 {
    approx_text_width(text) * (font_px / 12.0)
}

// ---------------------------------------------------------------------------
// SVG assembly
// ---------------------------------------------------------------------------

fn assemble(
    classes: &[Class],
    relations: &[Relation],
    notes: &[Note],
    title: &str,
    src: &str,
) -> String {
    let (mw, mh) = dimensions(classes, notes, title.is_empty());
    let fid = format!("puml-{:08x}", fnv1a(src.as_bytes()));
    let mut o = String::new();
    o.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         width=\"{mw:.0}\" height=\"{mh:.0}\" viewBox=\"0 0 {mw:.0} {mh:.0}\" \
         style=\"max-width:100%;height:auto;\" \
         role=\"img\" aria-label=\"plantuml class diagram\">"
    ));
    o.push_str(&format!(
        "<defs><filter id=\"{fid}\" x=\"-20%\" y=\"-20%\" width=\"140%\" height=\"140%\">\
         <feDropShadow dx=\"2\" dy=\"2\" stdDeviation=\"2\" flood-opacity=\"0.15\"/>\
         </filter></defs>"
    ));
    o.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>");
    if !title.is_empty() {
        o.push_str(&format!(
            "<text x=\"{:.0}\" y=\"26\" font-size=\"16\" font-weight=\"600\" \
             font-family=\"{FONT_FAMILY}\" fill=\"#24292f\" text-anchor=\"middle\">{}</text>",
            mw / 2.0,
            escape_text(title)
        ));
    }
    // Relations first so box outlines overlap their marker tips.
    for r in relations {
        if r.from < classes.len() && r.to < classes.len() {
            o.push_str(&render_relation(r, classes));
        }
    }
    for c in classes {
        o.push_str(&render_class(c, &fid));
    }
    for n in notes {
        if let Some(c) = classes.get(n.target) {
            o.push_str(&render_note(n, c));
        }
    }
    o.push_str("</svg>");
    o
}

fn dimensions(classes: &[Class], notes: &[Note], no_title: bool) -> (f64, f64) {
    let mut w = 0.0_f64;
    let mut h = 0.0_f64;
    for c in classes {
        w = w.max(c.x + c.w);
        h = h.max(c.y + c.h);
    }
    for n in notes {
        w = w.max(n.x + n.w);
        h = h.max(n.y + n.h);
    }
    (w + 24.0, h + (if no_title { 20.0 } else { 44.0 }))
}

/// FNV-1a 32-bit hash, used only to namespace the SVG filter id.
fn fnv1a(data: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in data {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Class box drawing. Mirrors the targeted style: rounded corners, light
/// fill, dark outline, soft drop shadow.
fn render_class(c: &Class, fid: &str) -> String {
    let mut s = String::new();
    let name_band = if c.stereo.is_empty() { 26.0 } else { 32.0 };
    let title_h = name_band + 8.0;
    s.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"4\" \
         fill=\"#f9f9f9\" stroke=\"#333333\" stroke-width=\"1.5\" filter=\"url(#{fid})\"/>",
        c.x, c.y, c.w, c.h
    ));
    let cx = c.x + c.w / 2.0;
    if !c.stereo.is_empty() {
        s.push_str(&format!(
            "<text x=\"{cx:.1}\" y=\"{:.1}\" font-size=\"10.5\" font-style=\"italic\" \
             text-anchor=\"middle\" font-family=\"{FONT_FAMILY}\" fill=\"#6b7280\">{}</text>",
            c.y + 16.0,
            escape_text(&c.stereo)
        ));
    }
    s.push_str(&format!(
        "<text x=\"{cx:.1}\" y=\"{:.1}\" font-size=\"14\" font-weight=\"600\" \
         text-anchor=\"middle\" font-family=\"{FONT_FAMILY}\" fill=\"#1f2328\">{}</text>",
        c.y + name_band,
        escape_text(&c.name)
    ));
    if !c.members.is_empty() {
        s.push_str(&format!(
            "<line x1=\"{:.1}\" x2=\"{:.1}\" y1=\"{:.1}\" y2=\"{:.1}\" \
             stroke=\"#cccccc\" stroke-width=\"1\"/>",
            c.x + 0.5,
            c.x + c.w - 0.5,
            c.y + title_h,
            c.y + title_h
        ));
        for (i, m) in c.members.iter().enumerate() {
            let gy = match m.vis {
                MemberVis::Public => "+",
                MemberVis::Private => "−",
                MemberVis::Protected => "#",
                MemberVis::Package => "~",
            };
            let line = format!("{gy}{}", m.text);
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" \
                 font-family=\"monospace\" fill=\"#374151\">{}</text>",
                c.x + 9.0,
                c.y + title_h + 8.0 + i as f64 * 15.0,
                escape_text(&line)
            ));
        }
    }
    s
}

/// Emit the relation polyline: markers at both ends oriented along the first /
/// last segment, multiplicity labels beside each port, and the relationship
/// label on a small white plate at the middle of the path.
fn render_relation(r: &Relation, classes: &[Class]) -> String {
    let path = &r.path;
    if path.len() < 2 {
        return String::new();
    }
    let n = path.len();
    let (x1, y1) = path[0];
    let (x2, y2) = path[n - 1];

    let mut start = path[0];
    let mut end = path[n - 1];
    let mut o = String::new();
    let ang0 = (path[1].1 - path[0].1).atan2(path[1].0 - path[0].0);
    if let Some(m) = r.from_marker {
        o.push_str(&marker_svg(start, ang0, m));
        start = (start.0 + ang0.cos() * m.depth(), start.1 + ang0.sin() * m.depth());
    }
    let ang_last = (path[n - 1].1 - path[n - 2].1).atan2(path[n - 1].0 - path[n - 2].0);
    if let Some(m) = r.to_marker {
        o.push_str(&marker_svg(end, ang_last + std::f64::consts::PI, m));
        end = (end.0 - ang_last.cos() * m.depth(), end.1 - ang_last.sin() * m.depth());
    }

    let dash = match r.kind {
        LineKind::Dashed => "8 4",
        LineKind::Solid => "none",
    };
    // Draw each segment; the first starts past the source marker and the last
    // stops short of the target marker.
    for k in 0..n - 1 {
        let (mut ax, mut ay) = path[k];
        let (mut bx, mut by) = path[k + 1];
        if k == 0 {
            ax = start.0;
            ay = start.1;
        }
        if k == n - 2 {
            bx = end.0;
            by = end.1;
        }
        o.push_str(&format!(
            "<line x1=\"{ax:.1}\" y1=\"{ay:.1}\" x2=\"{bx:.1}\" y2=\"{by:.1}\" \
             stroke=\"#333333\" stroke-width=\"1.5\" stroke-dasharray=\"{dash}\"/>"
        ));
    }

    // Multiplicity labels hang beside each endpoint.
    if !r.from_card.is_empty() {
        o.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" text-anchor=\"end\" \
             font-family=\"{FONT_FAMILY}\" fill=\"#6b7280\">{}</text>",
            x1 - 8.0,
            y1 + 5.0,
            escape_text(&r.from_card)
        ));
    }
    if !r.to_card.is_empty() {
        o.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" \
             font-family=\"{FONT_FAMILY}\" fill=\"#6b7280\">{}</text>",
            x2 + 8.0,
            y2 - 5.0,
            escape_text(&r.to_card)
        ));
    }

    // Relationship label on a small white backing plate at the middle of the
    // polyline. Shifted perpendicular to the local segment until it clears
    // unrelated boxes (a label can otherwise land on a third class sitting
    // between the ends).
    if !r.label.is_empty() {
        let (mx, my, ang) = polyline_label(path);
        let bw = measure(&r.label, 10.5) + 12.0;
        let bh = 16.0_f64;
        let other: Vec<(f64, f64, f64, f64)> = classes
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != r.from && *i != r.to)
            .map(|(_, c)| (c.x, c.y, c.w, c.h))
            .collect();
        let (mx, my) = label_anchor(mx, my, bw, bh, ang, &other);
        o.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{bw:.1}\" height=\"{bh:.0}\" rx=\"3\" \
             fill=\"#ffffff\" stroke=\"#d0d7de\" stroke-width=\"0.6\"/>",
            mx - bw / 2.0,
            my - bh / 2.0
        ));
        o.push_str(&format!(
            "<text x=\"{mx:.1}\" y=\"{:.1}\" font-size=\"10.5\" text-anchor=\"middle\" \
             font-family=\"{FONT_FAMILY}\" fill=\"#24292f\">{}</text>",
            my + bh / 2.0 - 4.0,
            escape_text(&r.label)
        ));
    }
    o
}

/// Midpoint of a polyline by arc length, plus the direction of the segment it
/// sits on (used to nudge the label plate perpendicular to the edge).
fn polyline_label(path: &[(f64, f64)]) -> (f64, f64, f64) {
    let mut total = 0.0_f64;
    for s in path.windows(2) {
        total += ((s[1].0 - s[0].0).powi(2) + (s[1].1 - s[0].1).powi(2)).sqrt();
    }
    let half = total / 2.0;
    let mut acc = 0.0_f64;
    for s in path.windows(2) {
        let d = ((s[1].0 - s[0].0).powi(2) + (s[1].1 - s[0].1).powi(2)).sqrt();
        if acc + d >= half || d == 0.0 {
            let t = if d == 0.0 { 0.0 } else { (half - acc) / d };
            let x = s[0].0 + (s[1].0 - s[0].0) * t;
            let y = s[0].1 + (s[1].1 - s[0].1) * t;
            let ang = (s[1].1 - s[0].1).atan2(s[1].0 - s[0].0);
            return (x, y, ang);
        }
        acc += d;
    }
    (path[path.len() - 1].0, path[path.len() - 1].1, 0.0)
}

/// A sticky note beside a class: light-yellow rounded box with a short dashed
/// connector to the class edge.
fn render_note(n: &Note, c: &Class) -> String {
    let mut o = String::new();
    let (x1, y1, x2, y2) = match n.side {
        NoteSide::Right => (n.x, n.y + n.h / 2.0, c.x + c.w, n.y + n.h / 2.0),
        NoteSide::Left => (n.x + n.w, n.y + n.h / 2.0, c.x, n.y + n.h / 2.0),
        NoteSide::Top => (n.x + n.w / 2.0, n.y + n.h, n.x + n.w / 2.0, c.y),
        NoteSide::Bottom => (n.x + n.w / 2.0, n.y, n.x + n.w / 2.0, c.y + c.h),
    };
    o.push_str(&format!(
        "<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" \
         stroke=\"#d4a72c\" stroke-width=\"1\" stroke-dasharray=\"3 3\"/>"
    ));
    o.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"4\" \
         fill=\"#fff8d6\" stroke=\"#d4a72c\" stroke-width=\"1.2\"/>",
        n.x, n.y, n.w, n.h
    ));
    let lines = note_lines(&n.text);
    for (i, l) in lines.iter().enumerate() {
        o.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" \
             font-family=\"{FONT_FAMILY}\" fill=\"#5b4a00\">{}</text>",
            n.x + 12.0,
            n.y + 15.0 + i as f64 * 15.0,
            escape_text(l)
        ));
    }
    o
}

/// Anchor for a label plate centred at `(mx, my)`. Tries small nudges along
/// the edge's perpendicular until the plate clears every unrelated class box
/// (`others`); falls back to the raw midpoint when nothing is free.
fn label_anchor(
    mx: f64,
    my: f64,
    bw: f64,
    bh: f64,
    ang: f64,
    others: &[(f64, f64, f64, f64)],
) -> (f64, f64) {
    let px = (ang + std::f64::consts::FRAC_PI_2).cos();
    let py = (ang + std::f64::consts::FRAC_PI_2).sin();
    for step in [0.0_f64, 18.0, -18.0, 36.0, -36.0, 54.0, -54.0, 72.0, -72.0] {
        let (cx, cy) = (mx + px * step, my + py * step);
        let clear = others
            .iter()
            .all(|o| !rects_overlap((cx - bw / 2.0, cy - bh / 2.0, bw, bh), *o));
        if clear {
            return (cx, cy);
        }
    }
    (mx, my)
}

/// Emit a polygon marker at `(px,py)`. `away` is the angle pointing from the
/// box into the diagram; the marker's apex touches the box edge and its body
/// extends along `away`.
fn marker_svg(p: (f64, f64), away: f64, m: Marker) -> String {
    let (px, py) = p;
    let (sa, ca) = away.sin_cos();
    let depth = m.depth();
    let (s2, c2) = (-sa, ca); // perpendicular
    match m {
        Marker::Triangle | Marker::Arrow => {
            let bx = px + ca * depth;
            let by = py + sa * depth;
            let wx = s2 * 6.0;
            let wy = c2 * 6.0;
            let fill = if matches!(m, Marker::Arrow) {
                "#333333"
            } else {
                "#ffffff"
            };
            format!(
                "<polygon points=\"{px:.1},{py:.1} {:.1},{:.1} {:.1},{:.1}\" \
                 fill=\"{fill}\" stroke=\"#333333\" stroke-width=\"1.5\"/>",
                bx + wx,
                by + wy,
                bx - wx,
                by - wy
            )
        }
        Marker::Diamond | Marker::FilledDiamond => {
            let hx = ca * depth / 2.0;
            let hy = sa * depth / 2.0;
            let wx = s2 * 5.0;
            let wy = c2 * 5.0;
            let bx = px + ca * depth;
            let by = py + sa * depth;
            let fill = if matches!(m, Marker::FilledDiamond) {
                "#333333"
            } else {
                "#ffffff"
            };
            format!(
                "<polygon points=\"{px:.1},{py:.1} {:.1},{:.1} {bx:.1},{by:.1} {:.1},{:.1}\" \
                 fill=\"{fill}\" stroke=\"#333333\" stroke-width=\"1.5\"/>",
                px + hx + wx,
                py + hy + wy,
                px + hx - wx,
                py + hy - wy
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VEHICLE: &str = r#"@startuml
!define LookAndFeel
!define Shape

interface Vehicle {
  + start()
  + stop()
}

abstract class AbstractCar {
  - model: String
  # speed: int
  + AbstractCar(model: String)
  + accelerate()
  + getModel(): String
}

class Sedan {
  + Sedan(model: String)
  + openTrunk()
}

class SUV {
  + SUV(model: String)
  + enable4WD()
}

class Engine {
  - type: String
  - horsePower: int
  + Engine(type: String, hp: int)
  + start()
  + stop()
}

class Wheel {
  - size: int
  - brand: String
  + Wheel(size: int, brand: String)
}

class Tire {
  - type: String
  + Tire(type: String)
}

class Driver {
  - name: String
  + Driver(name: String)
  + drive(car: AbstractCar)
}

class Manufacturer {
  + buildCar(): AbstractCar
}

' Relations
Vehicle <|.. AbstractCar   ' 实现
AbstractCar <|-- Sedan     ' 继承
AbstractCar <|-- SUV       ' 继承
AbstractCar *-- Engine     ' 组合
AbstractCar o-- Wheel      ' 聚合
Wheel *-- Tire             ' 组合
AbstractCar <--> Driver    ' 双向关联
AbstractCar ..> Manufacturer : depends on

note right of Manufacturer : 制造工厂
note left of Sedan : 轿车
note right of SUV : 越野车
@enduml"#;

    fn render_ok(src: &str) -> String {
        render(src).expect("render should succeed")
    }

    /// Lay out and route the parsed classes/relations the same way `render`
    /// does, so a test can inspect the computed connection paths.
    fn layout_and_route(src: &str) -> (Vec<Class>, Vec<Relation>) {
        let (mut classes, mut relations, _notes, _title) = parse_src(src).expect("parse");
        layout(&mut classes, &relations, false);
        assign_ports(&classes, &mut relations);
        route_horizontal(&classes, &mut relations);
        (classes, relations)
    }

    #[test]
    fn ports_are_distinct_and_edges_do_not_cross() {
        let (_classes, relations) = layout_and_route(VEHICLE);
        assert!(
            relations.len() >= 8,
            "expected the vehicle relations to be parsed"
        );
        // Every port must sit on its own spot (a relation uses path[0] on its
        // `from` box and path[last] on its `to` box).
        let pts: Vec<(f64, f64)> = relations
            .iter()
            .flat_map(|r| vec![r.path[0], *r.path.last().unwrap()])
            .collect();
        for i in 0..pts.len() {
            for j in i + 1..pts.len() {
                let d = (pts[i].0 - pts[j].0).abs() + (pts[i].1 - pts[j].1).abs();
                assert!(
                    d > 0.5,
                    "coincident ports {} {:?} and {} {:?}",
                    i,
                    pts[i],
                    j,
                    pts[j]
                );
            }
        }
        // No trajectory may cross another mid-way.
        for i in 0..relations.len() {
            for j in i + 1..relations.len() {
                for k in 0..relations[i].path.len().saturating_sub(1) {
                    let (a1, b1) = (relations[i].path[k], relations[i].path[k + 1]);
                    for m in 0..relations[j].path.len().saturating_sub(1) {
                        let (a2, b2) = (relations[j].path[m], relations[j].path[m + 1]);
                        assert!(
                            !segments_intersect(a1, b1, a2, b2),
                            "edges {} and {} cross: {:?}->{:?} vs {:?}->{:?}",
                            i,
                            j,
                            a1,
                            b1,
                            a2,
                            b2
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn renders_inheritance_and_composition() {
        let src = "@startuml\nClass01 <|-- Class02\nClass03 *-- Class04\n@enduml\n";
        let svg = render_ok(src);
        assert!(svg.contains("Class01"));
        assert!(svg.contains("Class02"));
        assert!(svg.contains("Class03"));
        assert!(svg.contains("Class04"));
        // Hollow triangle polygon for inheritance.
        assert!(
            svg.contains("fill=\"#ffffff\" stroke=\"#333333\""),
            "expected hollow triangle: {svg}"
        );
        // Solid diamond for composition.
        assert!(
            svg.contains("<polygon points=") && svg.contains("fill=\"#333333\""),
            "{svg}"
        );
        assert!(svg.contains("role=\"img\""));
        assert!(svg.contains("aria-label=\"plantuml class diagram\""));
    }

    #[test]
    fn supertype_is_drawn_above_subtype() {
        // Real PlantUML places the LEFT operand (the supertype) on top with
        // the triangle at its edge. Verify the supertype box is above the
        // subtype box.
        let src = "@startuml\nAnimal <|-- Dog\n@enduml\n";
        let svg = render_ok(src);
        assert!(box_y_of(&svg, "Animal") < box_y_of(&svg, "Dog"));
    }

    #[test]
    fn hub_classes_are_pulled_toward_the_layer_centre() {
        // The barycenter sweep leaves H on the far left of its row (its three
        // siblings tie at the same barycenter). Hub centering must migrate the
        // most-connected class inward so the row reads as one centred block.
        let src = "@startuml\n\
            class S\n\
            class H\n\
            class A\n\
            class B\n\
            class C\n\
            class H1\n\
            class H2\n\
            S <|-- H\n\
            S <|-- A\n\
            S <|-- B\n\
            S <|-- C\n\
            H <|-- H1\n\
            H <|-- H2\n\
            @enduml\n";
        let svg = render_ok(src);
        let hx = box_x_of(&svg, "H");
        let lo = box_x_of(&svg, "A").min(box_x_of(&svg, "B")).min(box_x_of(&svg, "C"));
        let hi = box_x_of(&svg, "A").max(box_x_of(&svg, "B")).max(box_x_of(&svg, "C"));
        assert!(
            hx > lo && hx < hi,
            "hub should sit between its siblings, got H={hx} in [{lo}, {hi}]: {svg}"
        );
    }

    fn box_x_of(svg: &str, name: &str) -> f64 {
        let marker = format!(">{name}</text>");
        let head = svg.split(&marker).next().unwrap_or("");
        let before = head
            .match_indices("<rect ")
            .map(|(i, _)| &head[i..])
            .last()
            .unwrap_or("");
        before
            .split('"')
            .filter_map(|p| p.trim().parse::<f64>().ok())
            .next()
            .unwrap_or(f64::NAN)
    }

    fn box_y_of(svg: &str, name: &str) -> f64 {
        let marker = format!(">{name}</text>");
        let head = svg.split(&marker).next().unwrap_or("");
        let before = head
            .match_indices("<rect ")
            .map(|(i, _)| &head[i..])
            .last()
            .unwrap_or("");
        before
            .split('"')
            .filter_map(|p| p.trim().parse::<f64>().ok())
            .nth(1)
            .unwrap_or(f64::NAN)
    }

    #[test]
    fn members_and_stereotype_render() {
        let src = "@startuml\nclass Dummy {\n  -field1\n  #field2\n  ~method1()\n  +method2()\n}\n@enduml\n";
        let svg = render_ok(src);
        assert!(svg.contains("field1"));
        assert!(svg.contains("field2"));
        assert!(svg.contains("method1()"));
        assert!(svg.contains("method2()"));
    }

    #[test]
    fn relation_label_and_cardinality() {
        let src = "@startuml\nClass01 \"1\" *-- \"many\" Class02 : contains\n@enduml\n";
        let svg = render_ok(src);
        assert!(svg.contains("contains"));
        assert!(svg.contains(">1</text>"));
        assert!(svg.contains("many"));
    }

    #[test]
    fn dashed_realization() {
        let src = "@startuml\nclass List\nclass ArrayList implements List\n@enduml\n";
        let svg = render_ok(src);
        // implements → dashed realization with hollow triangle
        assert!(svg.contains("8 4"), "{svg}");
        assert!(svg.contains("fill=\"#ffffff\" stroke=\"#333333\""), "{svg}");
    }

    #[test]
    fn extends_keyword() {
        let src = "@startuml\nclass A extends B\nclass B\n@enduml\n";
        let svg = render_ok(src);
        assert!(svg.contains(">A</text>") && svg.contains(">B</text>"));
    }

    #[test]
    fn alias_stereotype() {
        let src = "@startuml\nclass \"My Class\" as cls1 <<Singleton>>\n@enduml\n";
        let svg = render_ok(src);
        assert!(svg.contains("My Class"));
        assert!(svg.contains("Singleton"));
    }

    #[test]
    fn empty_source_returns_none() {
        assert!(render("@startuml\n@enduml\n").is_none());
        assert!(render("").is_none());
    }

    #[test]
    fn cjk_names_do_not_panic() {
        let src = "@startuml\nclass 动物\nclass 狗\n动物 <|-- 狗\n@enduml\n";
        let svg = render_ok(src);
        assert!(svg.contains("动物") && svg.contains("狗"));
    }

    #[test]
    fn full_vehicle_example() {
        let src = VEHICLE;
        let svg = render_ok(src);
        for name in [
            "Vehicle",
            "AbstractCar",
            "Sedan",
            "SUV",
            "Engine",
            "Wheel",
            "Tire",
            "Driver",
            "Manufacturer",
        ] {
            assert!(svg.contains(&format!(">{name}</text>")), "missing {name}");
        }
        // Abstract class declaration (`abstract class`) and `!define` ignored.
        assert!(svg.contains("speed"), "protected member missing");
        assert!(svg.contains("getModel(): String"), "method missing");
        // dashed realization triangle (implements) + hollow inheritance triangle
        assert!(
            svg.contains("8 4"),
            "expected a dashed realization line: {svg}"
        );
        assert!(
            svg.contains("fill=\"#ffffff\" stroke=\"#333333\""),
            "hollow markers"
        );
        // bidirectional association `<-->` → two filled triangles
        assert!(
            svg.match_indices("<polygon points=").count() >= 5,
            "expected arrow/union markers, got: {svg}"
        );
        // notes render as sticky boxes with their text
        assert!(
            svg.contains("制造工厂"),
            "note right of Manufacturer missing"
        );
        assert!(svg.contains("轿车"), "note left of Sedan missing");
        assert!(svg.contains("越野车"), "note right of SUV missing");
        assert!(svg.contains("#fff8d6"), "note box fill missing");
        // relation label
        assert!(svg.contains("depends on"), "relation label missing");
        // Layered layout keeps every supertype / whole above its children and
        // shares one centered column grid instead of one column per relation.
        assert!(box_y_of(&svg, "Vehicle") < box_y_of(&svg, "AbstractCar"));
        assert!(box_y_of(&svg, "AbstractCar") < box_y_of(&svg, "Sedan"));
        assert!(box_y_of(&svg, "AbstractCar") < box_y_of(&svg, "Engine"));
        assert!(box_y_of(&svg, "Wheel") < box_y_of(&svg, "Tire"));
        let (w, h) = svg_size(&svg);
        assert!(w < 1400.0, "diagram too wide: {w}"); // was ~2200 pre-layered-layout
        assert!(h > 300.0, "diagram too shallow: {h}");
    }

    /// `width` / `height` parsed from the `<svg>` root, in that order.
    fn svg_size(svg: &str) -> (f64, f64) {
        let boxed = svg
            .split("viewBox=\"0 0 ")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("viewBox present");
        let mut it = boxed.split_whitespace().map(|t| t.parse::<f64>().unwrap());
        (it.next().unwrap(), it.next().unwrap())
    }

    #[test]
    fn layered_layout_is_compact_for_a_fan_out() {
        let src = "@startuml\n\
            Vehicle <|.. AbstractCar\n\
            AbstractCar <|-- Sedan\n\
            AbstractCar <|-- SUV\n\
            AbstractCar *-- Engine\n\
            AbstractCar o-- Wheel\n\
            Wheel *-- Tire\n\
            @enduml\n";
        let svg = render_ok(src);
        assert!(box_y_of(&svg, "Vehicle") < box_y_of(&svg, "AbstractCar"));
        assert!(box_y_of(&svg, "AbstractCar") < box_y_of(&svg, "Sedan"));
        assert!(box_y_of(&svg, "AbstractCar") < box_y_of(&svg, "Engine"));
        assert!(box_y_of(&svg, "Wheel") < box_y_of(&svg, "Tire"));
        let (w, _) = svg_size(&svg);
        // The three "car" children share the second row (one column each), not
        // three separate columns strung across the canvas.
        assert!(w < 1000.0, "fan-out too wide: {w}");
        assert!(
            box_y_of(&svg, "Sedan") == box_y_of(&svg, "SUV"),
            "children share a row"
        );
        assert!(
            box_y_of(&svg, "Sedan") == box_y_of(&svg, "Engine"),
            "children share a row"
        );
    }
}
