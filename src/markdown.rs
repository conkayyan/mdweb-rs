//! A small, dependency-free CommonMark + GFM markdown renderer written in pure
//! Rust, covering the constructs used by blog articles: headings (ATX and
//! setext), paragraphs, fenced and indented code, blockquotes,
//! ordered/unordered (nested) lists, task lists, GFM-style pipe tables,
//! thematic breaks and raw HTML blocks — plus a faithful inline engine with
//! the full CommonMark emphasis/strong rules, strikethrough, code spans, links
//! and images (inline, reference, collapsed and shortcut), autolinks,
//! entities, backslash escapes and hard/soft breaks.
//!
//! Two pragmatic extras are understood: pandoc-style footnotes (`[^label]`
//! definitions, emitted as a back-linked `<ol>` at the end of the document)
//! and GFM task lists.
//!
//! Fenced and indented code is always HTML-escaped, so a `</pre>` inside a
//! code block can never break out of the page — a security guarantee rather
//! than a style. The renderer makes no network calls and executes nothing, so
//! it is safe to run on untrusted content.
//!
//! The public entry point is [`render`].

use std::collections::HashMap;

/// Render a full markdown document to HTML. Paragraphs are wrapped in
/// `<p>…</p>`, which is what the template layout expects.
pub fn render(source: &str) -> String {
    // Tabs are expanded to 4-column grid positions (CommonMark behaviour).
    // Trailing whitespace is deliberately kept: two spaces at the end of a
    // line mean a hard line break.
    let lines: Vec<String> = source
        .lines()
        .map(|l| expand_tabs(l.trim_end_matches('\r')))
        .collect();

    let mut parser = BlockParser::new(&lines);
    let blocks = parser.parse_blocks();
    let ftns = parser.footnote_numbers();
    let mut html = blocks_to_html(&blocks, &parser.refs, &ftns);
    html.push_str(&parser.footnote_section());
    html
}

/// CommonMark tab expansion: a tab advances to the next multiple of four.
fn expand_tabs(line: &str) -> String {
    if !line.contains('\t') {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len() + 8);
    let mut col = 0;
    for c in line.chars() {
        if c == '\t' {
            let n = 4 - (col % 4);
            for _ in 0..n {
                out.push(' ');
            }
            col += n;
        } else {
            out.push(c);
            col += if c.len_utf8() == 1 { 1 } else { c.len_utf8() };
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Character-level helpers
// ---------------------------------------------------------------------------

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
    escape_html(s).replace('"', "&quot;")
}

/// Leading space columns (tabs are already expanded to spaces).
fn indent_of(line: &str) -> usize {
    line.bytes().take_while(|b| *b == b' ').count()
}

/// The set of characters a backslash may escape (CommonMark "punctuation").
fn is_escapable(c: char) -> bool {
    matches!(
        c,
        '!' | '"'
            | '#'
            | '$'
            | '%'
            | '&'
            | '\''
            | '('
            | ')'
            | '*'
            | '+'
            | ','
            | '-'
            | '.'
            | '/'
            | ':'
            | ';'
            | '<'
            | '='
            | '>'
            | '?'
            | '@'
            | '['
            | '\\'
            | ']'
            | '^'
            | '_'
            | '`'
            | '{'
            | '|'
            | '}'
            | '~'
    )
}

/// Punctuation for the emphasis "flanking" rules: ASCII punctuation plus any
/// non-ASCII char that is neither whitespace, alphanumeric nor a control
/// character (an approximation of the Unicode Pd/Pe/Pf/Pi/Po/Ps/S categories).
fn is_punct(c: char) -> bool {
    is_escapable(c)
        || (!c.is_ascii() && !c.is_whitespace() && !c.is_alphanumeric() && !c.is_control())
}

fn char_at(s: &str, i: usize) -> Option<char> {
    s.get(i..)?.chars().next()
}

fn char_before(s: &str, i: usize) -> Option<char> {
    if i == 0 {
        None
    } else {
        s[..i].chars().next_back()
    }
}

// ---------------------------------------------------------------------------
// Entities
// ---------------------------------------------------------------------------

/// Named character references. Numeric references (`&#…;` / `&#x…;`) are
/// always decoded; anything that fails to decode is left as literal text, and
/// the final `&` is re-escaped so it survives untouched in the HTML.
const ENTITIES: &[(&str, &str)] = &[
    ("amp;", "&"),
    ("AMP;", "&"),
    ("lt;", "<"),
    ("LT;", "<"),
    ("gt;", ">"),
    ("GT;", ">"),
    ("quot;", "\""),
    ("QUOT;", "\""),
    ("apos;", "'"),
    ("nbsp;", "\u{00a0}"),
    ("iexcl;", "\u{00a1}"),
    ("cent;", "\u{00a2}"),
    ("pound;", "\u{00a3}"),
    ("curren;", "\u{00a4}"),
    ("yen;", "\u{00a5}"),
    ("brvbar;", "\u{00a6}"),
    ("sect;", "\u{00a7}"),
    ("uml;", "\u{00a8}"),
    ("copy;", "\u{00a9}"),
    ("ordf;", "\u{00aa}"),
    ("laquo;", "\u{00ab}"),
    ("not;", "\u{00ac}"),
    ("shy;", "\u{00ad}"),
    ("reg;", "\u{00ae}"),
    ("macr;", "\u{00af}"),
    ("deg;", "\u{00b0}"),
    ("plusmn;", "\u{00b1}"),
    ("sup2;", "\u{00b2}"),
    ("sup3;", "\u{00b3}"),
    ("acute;", "\u{00b4}"),
    ("micro;", "\u{00b5}"),
    ("para;", "\u{00b6}"),
    ("middot;", "\u{00b7}"),
    ("cedil;", "\u{00b8}"),
    ("sup1;", "\u{00b9}"),
    ("ordm;", "\u{00ba}"),
    ("raquo;", "\u{00bb}"),
    ("frac14;", "\u{00bc}"),
    ("frac12;", "\u{00bd}"),
    ("frac34;", "\u{00be}"),
    ("iquest;", "\u{00bf}"),
    ("times;", "\u{00d7}"),
    ("divide;", "\u{00f7}"),
    ("Agrave;", "\u{00c0}"),
    ("Aacute;", "\u{00c1}"),
    ("Acirc;", "\u{00c2}"),
    ("Atilde;", "\u{00c3}"),
    ("Auml;", "\u{00c4}"),
    ("Aring;", "\u{00c5}"),
    ("AElig;", "\u{00c6}"),
    ("Ccedil;", "\u{00c7}"),
    ("Egrave;", "\u{00c8}"),
    ("Eacute;", "\u{00c9}"),
    ("Ecirc;", "\u{00ca}"),
    ("Euml;", "\u{00cb}"),
    ("Igrave;", "\u{00cc}"),
    ("Iacute;", "\u{00cd}"),
    ("Icirc;", "\u{00ce}"),
    ("Iuml;", "\u{00cf}"),
    ("ETH;", "\u{00d0}"),
    ("Ntilde;", "\u{00d1}"),
    ("Ograve;", "\u{00d2}"),
    ("Oacute;", "\u{00d3}"),
    ("Ocirc;", "\u{00d4}"),
    ("Otilde;", "\u{00d5}"),
    ("Ouml;", "\u{00d6}"),
    ("Oslash;", "\u{00d8}"),
    ("Ugrave;", "\u{00d9}"),
    ("Uacute;", "\u{00da}"),
    ("Ucirc;", "\u{00db}"),
    ("Uuml;", "\u{00dc}"),
    ("Yacute;", "\u{00dd}"),
    ("THORN;", "\u{00de}"),
    ("szlig;", "\u{00df}"),
    ("agrave;", "\u{00e0}"),
    ("aacute;", "\u{00e1}"),
    ("acirc;", "\u{00e2}"),
    ("atilde;", "\u{00e3}"),
    ("auml;", "\u{00e4}"),
    ("aring;", "\u{00e5}"),
    ("aelig;", "\u{00e6}"),
    ("ccedil;", "\u{00e7}"),
    ("egrave;", "\u{00e8}"),
    ("eacute;", "\u{00e9}"),
    ("ecirc;", "\u{00ea}"),
    ("euml;", "\u{00eb}"),
    ("igrave;", "\u{00ec}"),
    ("iacute;", "\u{00ed}"),
    ("icirc;", "\u{00ee}"),
    ("iuml;", "\u{00ef}"),
    ("eth;", "\u{00f0}"),
    ("ntilde;", "\u{00f1}"),
    ("ograve;", "\u{00f2}"),
    ("oacute;", "\u{00f3}"),
    ("ocirc;", "\u{00f4}"),
    ("otilde;", "\u{00f5}"),
    ("ouml;", "\u{00f6}"),
    ("oslash;", "\u{00f8}"),
    ("ugrave;", "\u{00f9}"),
    ("uacute;", "\u{00fa}"),
    ("ucirc;", "\u{00fb}"),
    ("uuml;", "\u{00fc}"),
    ("yacute;", "\u{00fd}"),
    ("thorn;", "\u{00fe}"),
    ("yuml;", "\u{00ff}"),
    ("fnof;", "\u{0192}"),
    ("hellip;", "\u{2026}"),
    ("dagger;", "\u{2020}"),
    ("Dagger;", "\u{2021}"),
    ("bull;", "\u{2022}"),
    ("lsaquo;", "\u{2039}"),
    ("rsaquo;", "\u{203a}"),
    ("OElig;", "\u{0152}"),
    ("oelig;", "\u{0153}"),
    ("Scaron;", "\u{0160}"),
    ("scaron;", "\u{0161}"),
    ("Yuml;", "\u{0178}"),
    ("circ;", "\u{02c6}"),
    ("tilde;", "\u{02dc}"),
    ("ensp;", "\u{2002}"),
    ("emsp;", "\u{2003}"),
    ("thinsp;", "\u{2009}"),
    ("zwnj;", "\u{200c}"),
    ("zwj;", "\u{200d}"),
    ("lrm;", "\u{200e}"),
    ("rlm;", "\u{200f}"),
    ("ndash;", "\u{2013}"),
    ("mdash;", "\u{2014}"),
    ("lsquo;", "\u{2018}"),
    ("rsquo;", "\u{2019}"),
    ("sbquo;", "\u{201a}"),
    ("ldquo;", "\u{201c}"),
    ("rdquo;", "\u{201d}"),
    ("bdquo;", "\u{201e}"),
    ("permil;", "\u{2030}"),
    ("prime;", "\u{2032}"),
    ("Prime;", "\u{2033}"),
    ("larr;", "\u{2190}"),
    ("uarr;", "\u{2191}"),
    ("rarr;", "\u{2192}"),
    ("darr;", "\u{2193}"),
    ("harr;", "\u{2194}"),
    ("crarr;", "\u{21b5}"),
    ("lceil;", "\u{2308}"),
    ("rceil;", "\u{2309}"),
    ("lfloor;", "\u{230a}"),
    ("rfloor;", "\u{230b}"),
    ("loz;", "\u{25ca}"),
    ("spades;", "\u{2660}"),
    ("clubs;", "\u{2663}"),
    ("hearts;", "\u{2665}"),
    ("diams;", "\u{2666}"),
    ("lang;", "\u{27e8}"),
    ("rang;", "\u{27e9}"),
    ("euro;", "\u{20ac}"),
    ("trade;", "\u{2122}"),
    ("minus;", "\u{2212}"),
    ("forall;", "\u{2200}"),
    ("part;", "\u{2202}"),
    ("exist;", "\u{2203}"),
    ("empty;", "\u{2205}"),
    ("nabla;", "\u{2207}"),
    ("isin;", "\u{2208}"),
    ("notin;", "\u{2209}"),
    ("ni;", "\u{220b}"),
    ("prod;", "\u{220f}"),
    ("sum;", "\u{2211}"),
    ("lowast;", "\u{2217}"),
    ("radic;", "\u{221a}"),
    ("prop;", "\u{221d}"),
    ("infin;", "\u{221e}"),
    ("ang;", "\u{2220}"),
    ("and;", "\u{2227}"),
    ("or;", "\u{2228}"),
    ("cap;", "\u{2229}"),
    ("cup;", "\u{222a}"),
    ("int;", "\u{222b}"),
    ("there4;", "\u{2234}"),
    ("sim;", "\u{223c}"),
    ("cong;", "\u{2245}"),
    ("asymp;", "\u{2248}"),
    ("ne;", "\u{2260}"),
    ("equiv;", "\u{2261}"),
    ("le;", "\u{2264}"),
    ("ge;", "\u{2265}"),
    ("sub;", "\u{2282}"),
    ("sup;", "\u{2283}"),
    ("nsub;", "\u{2284}"),
    ("sube;", "\u{2286}"),
    ("supe;", "\u{2287}"),
    ("oplus;", "\u{2295}"),
    ("otimes;", "\u{2297}"),
    ("perp;", "\u{22a5}"),
    ("sdot;", "\u{22c5}"),
    ("alefsym;", "\u{2135}"),
];

/// Decode a numeric character reference at the start of `s`. Returns the
/// decoded char and the number of bytes consumed (including `&` and `;`).
fn decode_numeric(s: &str) -> Option<(char, usize)> {
    let body = s.as_bytes().get(1..)?;
    let end = body.iter().position(|&b| b == b';')?;
    if end == 0 || end > 8 {
        return None;
    }
    let digits = &s[1..1 + end];
    let (radix, digits) = match digits.as_bytes()[0] {
        b'x' | b'X' => (16, &digits[1..]),
        _ => (10, digits),
    };
    if digits.is_empty() {
        return None;
    }
    let v = u32::from_str_radix(digits, radix).ok()?;
    if v == 0 || v > 0x10ffff || (0xd800..=0xdfff).contains(&v) {
        return None;
    }
    let c = char::from_u32(v)?;
    Some((c, 1 + end + 1))
}

/// Decode a named entity at the start of `s` (looking for `&name;`), returning
/// the decoded char and consumed length.
fn decode_named(s: &str) -> Option<(char, usize)> {
    if !s.starts_with('&') {
        return None;
    }
    let maxlen = (s.len() + 1).min(36);
    for len in 2..maxlen {
        let cand = &s[1..len];
        if let Some((_, val)) = ENTITIES.iter().find(|(n, _)| *n == cand) {
            if let Some(c) = val.chars().next() {
                return Some((c, len + 1));
            }
        }
        if s.as_bytes()[len - 1] == b';' {
            break;
        }
    }
    None
}

fn decode_entity(s: &str) -> Option<(char, usize)> {
    if !s.starts_with('&') {
        return None;
    }
    decode_numeric(s).or_else(|| decode_named(s))
}

// ---------------------------------------------------------------------------
// Inline node tree (a linked list kept in a single arena, mirroring the node
// structure used by the reference CommonMark implementation)
// ---------------------------------------------------------------------------

#[derive(PartialEq, Clone, Copy)]
enum Kind {
    Text,
    Emph,
    Strong,
    Del,
    Code,
    Link,
    Image,
    Linebreak,
    Softbreak,
    Html,
    Footnote,
    Sup,
    Sub,
    Mark,
    Math,
    Root,
}

#[derive(Clone)]
struct Node {
    kind: Kind,
    text: String,
    dest: String,
    title: String,
    parent: Option<usize>,
    first: Option<usize>,
    last: Option<usize>,
    prev: Option<usize>,
    next: Option<usize>,
}

struct NodeList {
    nodes: Vec<Node>,
}

impl NodeList {
    fn new() -> NodeList {
        NodeList {
            nodes: vec![Node {
                kind: Kind::Root,
                text: String::new(),
                dest: String::new(),
                title: String::new(),
                parent: None,
                first: None,
                last: None,
                prev: None,
                next: None,
            }],
        }
    }

    /// Append a fresh node with `text` onto `parent`; returns its index.
    fn add(&mut self, parent: usize, kind: Kind, text: &str) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            text: text.to_string(),
            dest: String::new(),
            title: String::new(),
            parent: None,
            first: None,
            last: None,
            prev: None,
            next: None,
        });
        self.append(parent, id);
        id
    }

    /// Detach `i` from its parent's child chain.
    fn detach(&mut self, i: usize) {
        let (prev, next, parent) = {
            let n = &self.nodes[i];
            (n.prev, n.next, n.parent)
        };
        if let Some(p) = prev {
            self.nodes[p].next = next;
        }
        if let Some(nx) = next {
            self.nodes[nx].prev = prev;
        }
        if let Some(par) = parent {
            if self.nodes[par].first == Some(i) {
                self.nodes[par].first = next;
            }
            if self.nodes[par].last == Some(i) {
                self.nodes[par].last = prev;
            }
        }
        let n = &mut self.nodes[i];
        n.parent = None;
        n.prev = None;
        n.next = None;
    }

    /// Append `child` as the last child of `parent` (detaching it first).
    fn append(&mut self, parent: usize, child: usize) {
        self.detach(child);
        let tail = self.nodes[parent].last;
        self.nodes[child].parent = Some(parent);
        match tail {
            Some(t) => {
                self.nodes[t].next = Some(child);
                self.nodes[child].prev = Some(t);
            }
            None => {
                self.nodes[parent].first = Some(child);
            }
        }
        self.nodes[parent].last = Some(child);
    }

    /// Insert `node` immediately after `anchor`.
    fn insert_after(&mut self, anchor: usize, node: usize) {
        let next = self.nodes[anchor].next;
        self.detach(node);
        self.nodes[node].prev = Some(anchor);
        self.nodes[node].next = next;
        self.nodes[node].parent = self.nodes[anchor].parent;
        self.nodes[anchor].next = Some(node);
        if let Some(nx) = next {
            self.nodes[nx].prev = Some(node);
        } else if let Some(par) = self.nodes[anchor].parent {
            self.nodes[par].last = Some(node);
        }
    }
}

// ---------------------------------------------------------------------------
// The inline parser — a port of the reference CommonMark inline algorithm.
// `pos` is a byte offset into `subject`; all "special" characters are ASCII,
// so byte indexing is safe.
// ---------------------------------------------------------------------------

struct Delim {
    cc: char,
    numdelims: usize,
    origdelims: usize,
    node: usize,
    prev: Option<usize>,
    next: Option<usize>,
    can_open: bool,
    can_close: bool,
}

struct Bracket {
    node: usize,
    prev: Option<usize>,
    previous_delimiter: Option<usize>,
    index: usize,
    image: bool,
    active: bool,
    bracket_after: bool,
}

struct InlineParser<'a> {
    subject: &'a str,
    pos: usize,
    delimiters: Option<usize>,
    delims: Vec<Delim>,
    brackets: Option<usize>,
    brackts: Vec<Bracket>,
    nodes: NodeList,
    refmap: &'a HashMap<String, (String, String)>,
    footnotes: &'a HashMap<String, usize>,
}

impl<'a> InlineParser<'a> {
    fn new(
        subject: &'a str,
        refmap: &'a HashMap<String, (String, String)>,
        footnotes: &'a HashMap<String, usize>,
    ) -> InlineParser<'a> {
        InlineParser {
            subject,
            pos: 0,
            delimiters: None,
            delims: Vec::new(),
            brackets: None,
            brackts: Vec::new(),
            nodes: NodeList::new(),
            refmap,
            footnotes,
        }
    }

    fn peek(&self) -> Option<char> {
        char_at(self.subject, self.pos)
    }

    fn push(&mut self, parent: usize, kind: Kind, text: &str) -> usize {
        self.nodes.add(parent, kind, text)
    }

    /// Normalize a reference label (trim, collapse whitespace, lowercase).
    fn normalize_reference(label: &str) -> String {
        label
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    // -- ordinary text -----------------------------------------------------

    /// Match a run of non-special characters (CommonMark `reMain`).
    fn match_text_run(&mut self) -> Option<&'a str> {
        let start = self.pos;
        let mut i = 0;
        for (j, c) in self.subject[self.pos..].char_indices() {
            if matches!(
                c,
                '\n' | '`'
                    | '['
                    | ']'
                    | '\\'
                    | '!'
                    | '<'
                    | '&'
                    | '*'
                    | '_'
                    | '\''
                    | '"'
                    | '~'
                    | '^'
                    | '='
                    | '$'
                    | ':'
            ) {
                break;
            }
            i = j + c.len_utf8();
        }
        if i == 0 {
            None
        } else {
            let end = start + i;
            self.pos = end;
            Some(&self.subject[start..end])
        }
    }

    fn parse_string(&mut self, block: usize) -> bool {
        if let Some(run) = self.match_text_run() {
            self.push(block, Kind::Text, run);
            true
        } else {
            false
        }
    }

    // -- superscript / subscript ------------------------------------------

    /// Pandoc-style `^sup^` superscript. The closing marker must not be the
    /// last character before whitespace (CommonMark leaves such `^` alone).
    fn parse_supsub(&mut self, marker: char, block: usize) -> bool {
        let open = self.pos;
        let inner_start = open + 1;
        // find the matching closer
        let mut i = inner_start;
        let bytes = self.subject.as_bytes();
        while i < self.subject.len() {
            if bytes[i] as char == marker {
                break;
            }
            i += 1;
        }
        if i >= self.subject.len() {
            return false;
        }
        let inner = &self.subject[inner_start..i];
        if inner.is_empty() {
            return false;
        }
        // CommonMark-style rule would need flanking checks; a pragmatic
        // pandoc-compatible one: content must not be all whitespace.
        if inner.trim().is_empty() {
            return false;
        }
        // avoid clashing with `$` in hi/lo: only when the marker is preceded
        // or followed by no space (genuine word-adjacent scripts).
        self.pos = i + 1;
        let kind = if marker == '^' { Kind::Sup } else { Kind::Sub };
        let node = self.push(block, kind, "");
        self.nodes.nodes[node].text = inner.to_string();
        true
    }

    /// `==highlight==` → `<mark>`.
    fn parse_mark(&mut self, block: usize) -> bool {
        let open = self.pos;
        let inner_start = open + 2;
        let mut i = inner_start;
        let bytes = self.subject.as_bytes();
        while i + 1 < self.subject.len() {
            if bytes[i] as char == '=' && bytes[i + 1] as char == '=' {
                break;
            }
            i += 1;
        }
        if i + 1 >= self.subject.len() {
            return false;
        }
        let inner = &self.subject[inner_start..i];
        if inner.is_empty() {
            return false;
        }
        self.pos = i + 2;
        let node = self.push(block, Kind::Mark, "");
        self.nodes.nodes[node].text = inner.to_string();
        true
    }

    /// `$…$` and `$$…$$` inline math → MathML (no spaces hugging the delimiters).
    /// Block-level math is fenced ` ```math ` or wrapped in `\[…\]`.
    fn parse_math(&mut self, block: usize) -> bool {
        let open = self.pos;
        let bytes = self.subject.as_bytes();
        // Pick the delimiter shape by peeking at the byte right after the
        // opening `$`: a second `$` switches us to the `$$…$$` form.
        let (inner_start, close_len) = if bytes.get(open + 1) == Some(&b'$') {
            (open + 2, 2usize)
        } else {
            (open + 1, 1usize)
        };
        let close = &bytes[open..open + close_len];
        let mut i = inner_start;
        while i + close_len <= self.subject.len() {
            if &bytes[i..i + close_len] == close {
                break;
            }
            i += 1;
        }
        if i + close_len > self.subject.len() {
            return false;
        }
        let inner = &self.subject[inner_start..i];
        if !crate::tex::is_math_span(inner) {
            return false;
        }
        self.pos = i + close_len;
        let node = self.push(block, Kind::Math, "");
        self.nodes.nodes[node].text = inner.to_string();
        true
    }

    /// `\(…\)` inline math — the same span as `$…$` with paren delimiters.
    fn parse_paren_math(&mut self, block: usize) -> bool {
        if !self.subject[self.pos..].starts_with("\\(") {
            return false;
        }
        let inner_start = self.pos + 2;
        let bytes = self.subject.as_bytes();
        let mut i = inner_start;
        while i + 1 < self.subject.len() {
            if bytes[i] as char == '\\' && bytes[i + 1] as char == ')' {
                break;
            }
            i += 1;
        }
        if i + 1 >= self.subject.len() {
            return false;
        }
        let inner = &self.subject[inner_start..i];
        if !crate::tex::is_math_span(inner) {
            return false;
        }
        self.pos = i + 2;
        let node = self.push(block, Kind::Math, "");
        self.nodes.nodes[node].text = inner.to_string();
        true
    }

    /// `:emoji:` shortcode expansion when the code resolves; otherwise leave
    /// the colon as a plain first character so `parse_lone_colon` handles it.
    fn parse_emoji(&mut self, block: usize) -> bool {
        if self.peek() != Some(':') {
            return false;
        }
        let start = self.pos + 1;
        let mut i = start;
        let bytes = self.subject.as_bytes();
        // search for closing ':' with only [a-z0-9_+-] in between
        while i < self.subject.len() {
            let c = bytes[i] as char;
            if c == ':' {
                break;
            }
            if !(c.is_ascii_alphanumeric() || c == '_' || c == '+' || c == '-') {
                return false;
            }
            i += 1;
        }
        if i >= self.subject.len() || i == start {
            return false;
        }
        let code = &self.subject[start..i];
        if code.len() > 40 {
            return false;
        }
        match crate::emoji::lookup(code) {
            Some(glyph) => {
                self.pos = i + 1;
                self.push(block, Kind::Text, glyph);
                true
            }
            None => false,
        }
    }

    /// A lone `:` that is not an emoji — emit it directly.
    fn parse_lone_colon(&mut self, block: usize) -> bool {
        if self.peek() == Some(':') {
            self.push(block, Kind::Text, ":");
            self.pos += 1;
            true
        } else {
            false
        }
    }

    // -- entities ----------------------------------------------------------

    fn parse_entity(&mut self, block: usize) -> bool {
        if self.peek() != Some('&') {
            return false;
        }
        let tail = &self.subject[self.pos..];
        if let Some((c, n)) = decode_entity(tail) {
            let s = c.to_string();
            self.push(block, Kind::Text, &s);
            self.pos += n;
            true
        } else {
            false
        }
    }

    // -- backslash escapes --------------------------------------------------

    fn parse_backslash(&mut self, block: usize) -> bool {
        if self.peek() != Some('\\') {
            return false;
        }
        self.pos += 1;
        match self.peek() {
            Some('\n') => {
                self.pos += 1;
                self.push(block, Kind::Linebreak, "");
            }
            Some(c) if is_escapable(c) => {
                let s = c.to_string();
                self.push(block, Kind::Text, &s);
                self.pos += 1;
            }
            _ => {
                self.push(block, Kind::Text, "\\");
            }
        }
        true
    }

    // -- code spans ---------------------------------------------------------

    fn match_backtick_run(&mut self) -> Option<usize> {
        let start = self.pos;
        if self.peek()? != '`' {
            return None;
        }
        while self.peek() == Some('`') {
            self.pos += 1;
        }
        Some(self.pos - start)
    }

    fn parse_backticks(&mut self, block: usize) -> bool {
        if self.peek() != Some('`') {
            return false;
        }
        let len = self.match_backtick_run().unwrap();
        let after_open = self.pos;
        // scan forward for a closing run of the *same* length
        let mut matched = false;
        let mut i = after_open;
        while i < self.subject.len() {
            let c = char_at(self.subject, i).unwrap();
            if c == '`' {
                let mut j = i;
                while j < self.subject.len() && self.subject.as_bytes()[j] as char == '`' {
                    j += 1;
                }
                if j - i == len {
                    matched = true;
                    self.pos = j;
                    break;
                }
                i = j;
            } else {
                i += c.len_utf8();
            }
        }
        if matched {
            let contents: String = self.subject[after_open..self.pos - len].replace('\n', " ");
            // normalize: strip one leading and trailing space if content has
            // and interior non-space character
            let (a, b) = if contents.len() >= 2
                && contents.starts_with(' ')
                && contents.ends_with(' ')
                && contents[1..contents.len() - 1].chars().any(|c| c != ' ')
            {
                (1, contents.len() - 1)
            } else {
                (0, contents.len())
            };
            self.push(block, Kind::Code, &contents[a..b]);
        } else {
            self.pos = after_open;
            self.push(block, Kind::Text, &"`".repeat(len));
        }
        true
    }

    // -- newlines -----------------------------------------------------------

    fn parse_newline(&mut self, block: usize) -> bool {
        if self.peek() != Some('\n') {
            return false;
        }
        self.pos += 1;
        let (hard, last_text) = match self.nodes.nodes[block].last {
            Some(l) if self.nodes.nodes[l].kind == Kind::Text => {
                let t = self.nodes.nodes[l].text.clone();
                (t.ends_with("  "), Some(l))
            }
            _ => (false, None),
        };
        if let Some(l) = last_text {
            let n = &mut self.nodes.nodes[l];
            n.text = n.text.trim_end_matches(' ').to_string();
        }
        self.push(
            block,
            if hard {
                Kind::Linebreak
            } else {
                Kind::Softbreak
            },
            "",
        );
        // gobble leading spaces on the next line
        while self.peek() == Some(' ') {
            self.pos += 1;
        }
        true
    }

    // -- delimiters (emphasis) ----------------------------------------------

    fn scan_delims(&mut self, cc: char) -> Option<(usize, bool, bool)> {
        let start = self.pos;
        let mut numdelims = 0;
        while self.peek() == Some(cc) {
            numdelims += 1;
            self.pos += 1;
        }
        if numdelims == 0 {
            return None;
        }
        let before = char_before(self.subject, start).unwrap_or('\n');
        let after = self.peek().unwrap_or('\n');
        let after_ws = after.is_whitespace();
        let after_punct = is_punct(after);
        let before_ws = before.is_whitespace();
        let before_punct = is_punct(before);
        let left_flanking = !after_ws && (!after_punct || before_ws || before_punct);
        let right_flanking = !before_ws && (!before_punct || after_ws || after_punct);
        let (can_open, can_close) = if cc == '_' {
            (
                left_flanking && (!right_flanking || before_punct),
                right_flanking && (!left_flanking || after_punct),
            )
        } else {
            (left_flanking, right_flanking)
        };
        Some((numdelims, can_open, can_close))
    }

    fn handle_delim(&mut self, cc: char, block: usize) -> bool {
        let start = self.pos;
        let Some((numdelims, can_open, can_close)) = self.scan_delims(cc) else {
            return false;
        };
        let node = self.push(block, Kind::Text, &self.subject[start..self.pos]);
        if (can_open || can_close) && (cc == '*' || cc == '_' || cc == '~') {
            let id = self.delims.len();
            let prev = self.delimiters;
            self.delims.push(Delim {
                cc,
                numdelims,
                origdelims: numdelims,
                node,
                prev,
                next: None,
                can_open,
                can_close,
            });
            if let Some(p) = prev {
                self.delims[p].next = Some(id);
            }
            self.delimiters = Some(id);
        }
        true
    }

    fn remove_delimiter(&mut self, delim: usize) {
        let (prev, next) = {
            let d = &self.delims[delim];
            (d.prev, d.next)
        };
        if let Some(p) = prev {
            self.delims[p].next = next;
        }
        if next.is_none() {
            self.delimiters = prev;
        } else if let Some(n) = next {
            self.delims[n].prev = prev;
        }
    }

    /// The CommonMark emphasis algorithm (ported from the reference
    /// implementation). `stack_bottom` is the delimiter below which no pairs
    /// may be formed.
    fn process_emphasis(&mut self, stack_bottom: Option<usize>) {
        const COUNT: usize = 14;
        let mut openers_bottom: Vec<Option<usize>> = vec![stack_bottom; COUNT];

        // find first closer above stack_bottom:
        let mut closer = self.delimiters;
        while let Some(c) = closer {
            if self.delims[c].prev == stack_bottom {
                break;
            }
            closer = self.delims[c].prev;
        }

        while let Some(c) = closer {
            if !self.delims[c].can_close {
                closer = self.delims[c].next;
                continue;
            }
            let closercc = self.delims[c].cc;
            let closer_can_open = self.delims[c].can_open;
            let closer_orig = self.delims[c].origdelims;

            // look back for the first matching opener
            let mut opener = self.delims[c].prev;
            let mut opener_found = false;
            let index = delim_index(closercc, closer_can_open, closer_orig);
            while let Some(o) = opener {
                if Some(o) == stack_bottom || openers_bottom[index] == Some(o) {
                    break;
                }
                let odd_match = (closer_can_open || self.delims[o].can_close)
                    && closer_orig % 3 != 0
                    && (self.delims[o].origdelims + closer_orig) % 3 == 0;
                if self.delims[o].cc == closercc && self.delims[o].can_open && !odd_match {
                    opener_found = true;
                    break;
                }
                opener = self.delims[o].prev;
            }
            let old_closer = c;

            if !opener_found {
                closer = self.delims[c].next;
                openers_bottom[index] = self.delims[old_closer].prev;
                if !self.delims[old_closer].can_open {
                    self.remove_delimiter(old_closer);
                }
                continue;
            }

            let opener_id = opener.unwrap();
            let use_delims =
                if self.delims[c].numdelims >= 2 && self.delims[opener_id].numdelims >= 2 {
                    2
                } else {
                    1
                };

            // remove used delimiters from the text nodes
            self.delims[opener_id].numdelims -= use_delims;
            self.delims[c].numdelims -= use_delims;
            let onode = self.delims[opener_id].node;
            let cnode = self.delims[c].node;
            truncate_chars(&mut self.nodes.nodes[onode].text, use_delims);
            truncate_chars(&mut self.nodes.nodes[cnode].text, use_delims);

            // build the emph element holding the content between them
            let emph_kind = if closercc == '~' {
                Kind::Del
            } else if use_delims == 1 {
                Kind::Emph
            } else {
                Kind::Strong
            };
            let emp = self.nodes.nodes.len();
            self.nodes.nodes.push(Node {
                kind: emph_kind,
                text: String::new(),
                dest: String::new(),
                title: String::new(),
                parent: self.nodes.nodes[onode].parent,
                first: None,
                last: None,
                prev: None,
                next: None,
            });

            // move nodes between opener and closer into the emph element
            let mut tmp = self.nodes.nodes[onode].next;
            while tmp != Some(cnode) {
                let t = tmp.unwrap();
                let nx = self.nodes.nodes[t].next;
                self.nodes.detach(t);
                self.nodes.append(emp, t);
                tmp = nx;
            }
            self.nodes.insert_after(onode, emp);

            // drop the delimiter entries between opener and closer
            let bt = self.delims[opener_id].next;
            self.delims[opener_id].next = Some(c);
            self.delims[c].prev = Some(opener_id);
            let _ = bt;

            if self.delims[opener_id].numdelims == 0 {
                let tn = self.delims[opener_id].node;
                self.nodes.detach(tn);
                self.remove_delimiter(opener_id);
            }
            if self.delims[c].numdelims == 0 {
                let tn = self.delims[c].node;
                self.nodes.detach(tn);
                let tempstack = self.delims[c].next;
                self.remove_delimiter(c);
                closer = tempstack;
            }
        }

        // remove all delimiters above stack_bottom
        while let Some(d) = self.delimiters {
            if Some(d) == stack_bottom {
                break;
            }
            self.remove_delimiter(d);
        }
    }

    // -- brackets: links and images -----------------------------------------

    fn add_bracket(&mut self, node: usize, index: usize, image: bool) {
        if let Some(b) = self.brackets {
            self.brackts[b].bracket_after = true;
        }
        let id = self.brackts.len();
        self.brackts.push(Bracket {
            node,
            prev: self.brackets,
            previous_delimiter: self.delimiters,
            index,
            image,
            active: true,
            bracket_after: false,
        });
        self.brackets = Some(id);
    }

    fn remove_bracket(&mut self) {
        let b = self.brackets.expect("bracket stack empty");
        self.brackets = self.brackts[b].prev;
    }

    fn parse_open_bracket(&mut self, block: usize) -> bool {
        if self.peek() != Some('[') {
            return false;
        }
        // footnote reference?
        if self.subject[self.pos..].starts_with("[^") && self.try_footnote_ref(block) {
            return true;
        }
        let start = self.pos;
        self.pos += 1;
        let node = self.push(block, Kind::Text, "[");
        self.add_bracket(node, start, false);
        true
    }

    fn parse_bang(&mut self, block: usize) -> bool {
        if self.peek() != Some('!') {
            return false;
        }
        let start = self.pos;
        self.pos += 1;
        if self.peek() == Some('[') {
            self.pos += 1;
            let node = self.push(block, Kind::Text, "![");
            self.add_bracket(node, start + 1, true);
        } else {
            self.push(block, Kind::Text, "!");
        }
        true
    }

    /// `[^label]` — resolved only for labels that have a definition.
    fn try_footnote_ref(&mut self, block: usize) -> bool {
        if !self.subject[self.pos..].starts_with("[^") {
            return false;
        }
        let tail = &self.subject[self.pos + 2..];
        let mut end = None;
        for (j, c) in tail.char_indices() {
            if c == ']' {
                end = Some(j);
                break;
            }
            if !(c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                return false;
            }
        }
        let end = match end {
            Some(e) if e > 0 => e,
            _ => return false,
        };
        let label = &tail[..end];
        let Some(&num) = self.footnotes.get(label) else {
            return false;
        };
        let node = self.push(block, Kind::Footnote, label);
        self.nodes.nodes[node].dest = num.to_string();
        self.pos += 2 + end + 1;
        true
    }

    /// `]` — try to match against an `[`/`![` opener to form a link or image.
    fn parse_close_bracket(&mut self, block: usize) -> bool {
        if self.peek() != Some(']') {
            return false;
        }
        self.pos += 1;
        let start_pos = self.pos;

        let (opener, is_image) = match self.brackets.and_then(|b| self.brackts.get(b)) {
            Some(b) => (self.brackets.unwrap(), b.image),
            None => {
                self.push(block, Kind::Text, "]");
                return true;
            }
        };

        if !self.brackts[opener].active {
            self.push(block, Kind::Text, "]");
            self.remove_bracket();
            return true;
        }

        let mut matched = false;
        let mut dest = String::new();
        let mut title = String::new();
        let save_pos = self.pos;

        // inline link?
        if self.peek() == Some('(') {
            self.pos += 1;
            self.spnl();
            let dest_opt = self.parse_link_destination();
            self.spnl();
            let ws_before_title = char_before(self.subject, self.pos)
                .map(char::is_whitespace)
                .unwrap_or(false);
            let title_opt = if ws_before_title {
                self.parse_link_title()
            } else {
                None
            };
            self.spnl();
            if dest_opt.is_some() && self.peek() == Some(')') {
                self.pos += 1;
                dest = dest_opt.unwrap_or_default();
                title = title_opt.unwrap_or_default();
                matched = true;
            } else {
                self.pos = save_pos;
            }
        }

        // reference link?
        if !matched {
            let before_label = self.pos;
            let n = self.parse_link_label();
            if n > 2 {
                let label = &self.subject[before_label..before_label + n];
                let label = &label[1..label.len() - 1];
                let key = Self::normalize_reference(label);
                if let Some((d, t)) = self.refmap.get(&key) {
                    dest = d.clone();
                    title = t.clone();
                    matched = true;
                    self.pos = before_label + n;
                }
            } else if !self.brackts[opener].bracket_after {
                // collapsed/shortcut: use the label text between brackets
                let key =
                    Self::normalize_reference(&self.subject[self.brackts[opener].index..start_pos]);
                if let Some((d, t)) = self.refmap.get(&key) {
                    dest = d.clone();
                    title = t.clone();
                    matched = true;
                }
            }
            if n == 0 {
                self.pos = save_pos;
            }
        }

        if matched {
            let node = self.push(block, if is_image { Kind::Image } else { Kind::Link }, "");
            // move the opener's children into the link/image node
            let onode = self.brackts[opener].node;
            let mut cur = self.nodes.nodes[onode].next;
            while let Some(c) = cur {
                if c == node {
                    break; // the link node itself; everything later stays
                }
                let nx = self.nodes.nodes[c].next;
                self.nodes.detach(c);
                self.nodes.append(node, c);
                cur = nx;
            }
            // process emphasis between opener and here first
            let pd = self.brackts[opener].previous_delimiter;
            self.process_emphasis(pd);
            self.remove_bracket();
            self.nodes.detach(onode);

            let n = &mut self.nodes.nodes[node];
            n.dest = dest.clone();
            n.title = title;

            if !is_image {
                // no links in links: deactivate earlier link openers
                let mut b = self.brackets;
                while let Some(bi) = b {
                    if !self.brackts[bi].image {
                        self.brackts[bi].active = false;
                    }
                    b = self.brackts[bi].prev;
                }
            }
            return true;
        }

        // no match: literal `]`
        self.remove_bracket();
        self.pos = start_pos;
        self.push(block, Kind::Text, "]");
        true
    }

    // -- link destinations / titles / labels --------------------------------

    fn spnl(&mut self) -> bool {
        let tail = &self.subject[self.pos..];
        let mut i = 0;
        for c in tail.chars() {
            if c == ' ' || c == '\n' {
                i += c.len_utf8();
            } else {
                break;
            }
        }
        self.pos += i;
        true
    }

    fn parse_link_title(&mut self) -> Option<String> {
        let tail = &self.subject[self.pos..];
        let q = char_at(tail, 0)?;
        if q != '"' && q != '\'' && q != '(' {
            return None;
        }
        let close = if q == '(' { ')' } else { q };
        let body = &tail[1..];
        let mut esc = false;
        for (j, c) in body.char_indices() {
            if esc {
                esc = false;
                continue;
            }
            if c == '\\' {
                esc = true;
                continue;
            }
            if c == '\n' {
                return None;
            }
            if c == close {
                self.pos += 1 + j + c.len_utf8();
                return Some(unescape(&body[..j]));
            }
        }
        None
    }

    fn parse_link_destination(&mut self) -> Option<String> {
        let tail = &self.subject[self.pos..];
        // <...>
        if let Some(rest) = tail.strip_prefix('<') {
            let mut n = 0;
            for (j, c) in rest.char_indices() {
                if c == '>' {
                    n = j + 2;
                    break;
                }
                if c == '\n' || c == '\0' {
                    return None;
                }
            }
            if n == 0 {
                return None;
            }
            let dest = unescape(&tail[1..n - 1]);
            self.pos += n;
            return Some(dest);
        }
        // ) immediately: empty destination
        if tail.starts_with(')') {
            self.pos += 0;
            return Some(String::new());
        }
        // balanced parentheses (no whitespace)
        let mut open = 0usize;
        let mut n = 0usize;
        let mut ok = false;
        let mut escaped = false;
        for c in tail.chars() {
            if escaped {
                escaped = false;
                n += c.len_utf8();
                continue;
            }
            match c {
                '\\' => {
                    escaped = true;
                    n += 1;
                }
                '(' => {
                    open += 1;
                    n += 1;
                }
                ')' => {
                    if open == 0 {
                        break;
                    }
                    open -= 1;
                    n += 1;
                }
                c if c.is_whitespace() => break,
                _ => n += c.len_utf8(),
            }
            ok = true;
        }
        if !ok || open != 0 {
            return None;
        }
        let dest = unescape(&tail[..n]);
        self.pos += n;
        Some(dest)
    }

    /// Length (in bytes) of a `[label]` at the current position, or 0.
    fn parse_link_label(&mut self) -> usize {
        let tail = &self.subject[self.pos..];
        if !tail.starts_with('[') {
            return 0;
        }
        let body = &tail[1..];
        let mut esc = false;
        for (j, c) in body.char_indices() {
            if esc {
                esc = false;
                continue;
            }
            if c == '\\' {
                esc = true;
                continue;
            }
            if c == '[' {
                return 0;
            }
            if c == ']' {
                return j + 2; // includes '['
            }
            if j > 1001 {
                return 0;
            }
        }
        0
    }

    // -- main loop ----------------------------------------------------------

    fn parse_inline(&mut self, block: usize) -> bool {
        match self.peek() {
            None => false,
            Some('\n') => self.parse_newline(block),
            Some('\\') if self.subject[self.pos..].starts_with("\\(") => {
                self.parse_paren_math(block)
            }
            Some('\\') => self.parse_backslash(block),
            Some('`') => self.parse_backticks(block),
            Some('*') | Some('_') => self.handle_delim(self.peek().unwrap(), block),
            Some('[') => self.parse_open_bracket(block),
            Some('!') => self.parse_bang(block),
            Some(']') => self.parse_close_bracket(block),
            Some('<') => self.parse_autolink(block) || self.parse_html_tag(block),
            Some('&') => self.parse_entity(block),
            Some('~') if self.subject[self.pos..].starts_with("~~~") => {
                self.handle_delim('~', block)
            }
            Some('~') if self.subject[self.pos..].starts_with("~~") => {
                self.handle_delim('~', block)
            }
            Some('~') => self.parse_supsub('~', block),
            Some('^') => self.parse_supsub('^', block),
            Some('=') if self.subject[self.pos..].starts_with("==") => self.parse_mark(block),
            Some('$') => self.parse_math(block),
            Some(':') => self.parse_emoji(block) || self.parse_lone_colon(block),
            Some(_) => self.parse_string(block),
        }
    }

    fn parse(&mut self, block: usize) {
        loop {
            match self.peek() {
                None => break,
                Some(_) => {
                    if self.parse_inline(block) {
                        continue;
                    }
                    // fall back: emit the single character literally
                    let c = self.peek().unwrap();
                    let s = c.to_string();
                    self.push(block, Kind::Text, &s);
                    self.pos += c.len_utf8();
                }
            }
        }
        self.process_emphasis(None);
    }

    /// Full entry: parse to HTML.
    fn render(mut self) -> String {
        self.parse(0);
        render_children(&self.nodes, 0, self.refmap, self.footnotes)
    }
}

/// One of the 14 "openers_bottom" slots, mirroring the reference algorithm.
fn delim_index(cc: char, can_open: bool, orig: usize) -> usize {
    match cc {
        '_' => 2 + if can_open { 3 } else { 0 } + (orig % 3),
        '*' => 8 + if can_open { 3 } else { 0 } + (orig % 3),
        _ => 0,
    }
}

/// Remove the last `n` characters from a string (used when delimiters are
/// consumed for emphasis; `n` is 1 or 2 chars).
fn truncate_chars(s: &mut String, n: usize) {
    for _ in 0..n {
        s.pop();
    }
}

/// Unescape backslash escapes in a link destination or title.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(n) if is_escapable(n) => out.push(n),
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Autolinks and raw inline HTML
// ---------------------------------------------------------------------------

impl<'a> InlineParser<'a> {
    fn parse_autolink(&mut self, block: usize) -> bool {
        if self.peek() != Some('<') {
            return false;
        }
        let tail = &self.subject[self.pos..];
        if let Some((inner, n)) = email_autolink(tail) {
            let node = self.push(block, Kind::Link, "");
            self.push(node, Kind::Text, &inner);
            self.nodes.nodes[node].dest = format!("mailto:{inner}");
            self.pos += n;
            return true;
        }
        if let Some((inner, n)) = scheme_autolink(tail) {
            let node = self.push(block, Kind::Link, "");
            self.push(node, Kind::Text, &inner);
            self.nodes.nodes[node].dest = inner.clone();
            self.pos += n;
            return true;
        }
        false
    }

    fn parse_html_tag(&mut self, block: usize) -> bool {
        let tail = &self.subject[self.pos..];
        if let Some((html, n)) = match_html_inline(tail) {
            // HTML comments are invisible: drop them entirely.
            if html.starts_with("<!--") {
                self.pos += n;
            } else {
                self.push(block, Kind::Html, &html);
                self.pos += n;
            }
            true
        } else {
            false
        }
    }
}

/// `^<local@host>` email autolink; returns the address and total length.
fn email_autolink(s: &str) -> Option<(String, usize)> {
    let s = s.strip_prefix('<')?;
    let mut at = None;
    for (j, c) in s.char_indices() {
        if c == '@' {
            at = Some(j);
            break;
        }
        if !(c.is_ascii_alphanumeric() || ".!#$%&'*+/=?^_`{|}~-".contains(c)) {
            return None;
        }
    }
    let at = at?;
    if at == 0 {
        return None;
    }
    let rest = &s[at + 1..];
    let gt = rest.find('>')?;
    let host = &rest[..gt];
    if host.is_empty() {
        return None;
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    for p in &parts {
        let b = p.as_bytes();
        if b.is_empty()
            || !b[0].is_ascii_alphanumeric()
            || !b[b.len() - 1].is_ascii_alphanumeric()
            || b.len() > 63
            || !b.iter().all(|x| x.is_ascii_alphanumeric() || *x == b'-')
        {
            return None;
        }
    }
    let inner_len = at + 1 + gt;
    Some((s[..inner_len].to_string(), inner_len + 2))
}

/// `^<scheme:…>` autolink; returns the destination and total length.
fn scheme_autolink(s: &str) -> Option<(String, usize)> {
    let s = s.strip_prefix('<')?;
    let bytes = s.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    let mut j = 1;
    loop {
        match bytes.get(j) {
            Some(&b) if b.is_ascii_alphanumeric() || b == b'.' || b == b'+' || b == b'-' => {
                j += 1;
                if j > 32 {
                    return None;
                }
            }
            Some(&b':') => break,
            _ => return None,
        }
    }
    if j < 2 {
        return None; // scheme must be at least 2 chars
    }
    let mut k = j + 1;
    while let Some(&b) = bytes.get(k) {
        if b == b'>' {
            break;
        }
        if b < 0x21 || b == b'<' {
            return None;
        }
        k += 1;
    }
    if bytes.get(k) != Some(&b'>') {
        return None;
    }
    Some((s[..k].to_string(), k + 2))
}

/// The CommonMark `html_inline` grammar: comment, CDATA, PI, declaration,
/// open tag, closing tag. Returns the matched tag and its length.
fn match_html_inline(s: &str) -> Option<(String, usize)> {
    if let Some(rest) = s.strip_prefix("<!--") {
        return rest.find("-->").map(|e| (s[..e + 8].to_string(), e + 8));
    }
    if let Some(rest) = s.strip_prefix("<![CDATA[") {
        return rest.find("]]>").map(|e| (s[..e + 11].to_string(), e + 11));
    }
    if let Some(rest) = s.strip_prefix("<?") {
        return rest.find("?>").map(|e| (s[..e + 5].to_string(), e + 5));
    }
    if let Some(rest) = s.strip_prefix("<!") {
        // declaration: `<!` + one or more ASCII letters, then `>`
        let letters = rest.bytes().take_while(|b| b.is_ascii_alphabetic()).count();
        if letters == 0 {
            return None;
        }
        let after = &rest[letters..];
        if let Some(gt) = after.find('>') {
            if !after[..gt].contains('\n') {
                return Some((s[..2 + letters + gt + 1].to_string(), 2 + letters + gt + 1));
            }
        }
        return None;
    }
    // closing tag
    if s.starts_with("</") {
        if let Some(len) = match_close_tag(s) {
            return Some((s[..len].to_string(), len));
        }
        return None;
    }
    // open tag (with optional attributes)
    if let Some(len) = match_open_tag(s) {
        return Some((s[..len].to_string(), len));
    }
    None
}

fn match_close_tag(s: &str) -> Option<usize> {
    let rest = s.strip_prefix("</")?;
    let mut i = 0;
    for (j, c) in rest.char_indices() {
        if j == 0 {
            if !c.is_ascii_alphabetic() {
                return None;
            }
            i = 1;
        } else if c.is_ascii_alphanumeric() || c == '-' {
            i = j + 1;
        } else {
            break;
        }
    }
    let after = rest[i..].trim_start();
    let after = after.strip_prefix('>')?;
    Some(s.len() - after.len())
}

/// Parse a complete open tag `<name attr=…>` (optionally self-closing), using
/// the CommonMark `html_open` grammar.
fn match_open_tag(s: &str) -> Option<usize> {
    let rest = &s[1..]; // after '<'
                        // tag name
    let mut i = 0;
    for (j, c) in rest.char_indices() {
        if j == 0 {
            if !c.is_ascii_alphabetic() {
                return None;
            }
            i = 1;
        } else if c.is_ascii_alphanumeric() || c == '-' {
            i = j + 1;
        } else {
            break;
        }
    }
    let mut pos = i;
    loop {
        // `>` may follow a tag directly (no trailing whitespace needed)
        let tail = &rest[pos..];
        if tail.starts_with('/') && tail.as_bytes().get(1) == Some(&b'>') {
            return Some(1 + pos + 2);
        }
        if tail.starts_with('>') {
            return Some(1 + pos + 1);
        }
        // between attributes (or before `>` after attrs) there must be whitespace
        let (ws, nl) = skip_html_ws(&rest[pos..]);
        if nl {
            return None;
        }
        if ws == 0 {
            return None;
        }
        pos += ws;
        let tail = &rest[pos..];
        if tail.starts_with('/') && tail.as_bytes().get(1) == Some(&b'>') {
            return Some(1 + pos + 2);
        }
        if tail.starts_with('>') {
            return Some(1 + pos + 1);
        }
        // attribute name: [a-zA-Z_:][a-zA-Z0-9_.:-]*
        let mut an = 0;
        let mut ok = false;
        for (j, c) in tail.char_indices() {
            if j == 0 {
                if !(c.is_ascii_alphabetic() || c == '_' || c == ':') {
                    return None;
                }
                ok = true;
                an = j + 1;
            } else if c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '-') {
                an = j + 1;
            } else {
                break;
            }
        }
        if !ok {
            return None;
        }
        pos += an;
        // optional value: \s*=\s*value
        let (ws2, _nl) = skip_html_ws(&rest[pos..]);
        pos += ws2;
        if rest[pos..].starts_with('=') {
            pos += 1;
            let (ws3, _nl) = skip_html_ws(&rest[pos..]);
            pos += ws3;
            match parse_attr_value(&rest[pos..]) {
                Some(len) => pos += len,
                None => return None,
            }
        }
    }
}

/// The RFC-ish attr value: `'…'`, `"…"`, or an unquoted run.
fn parse_attr_value(tail: &str) -> Option<usize> {
    let first = tail.as_bytes().first().copied()?;
    match first {
        b'\'' | b'"' => {
            let mut end = None;
            for (j, c) in tail[1..].char_indices() {
                if c as u8 == first {
                    end = Some(j + 1);
                    break;
                }
                if c == '\n' || c == '\0' {
                    return None;
                }
            }
            // one for the quote + content + one for the closing quote
            Some(end.map(|e| e + 1)?)
        }
        b if b.is_ascii_whitespace() || matches!(b, b'=' | b'<' | b'>' | b'`') => None,
        _ => {
            let mut n = 0;
            for (j, c) in tail.char_indices() {
                if c.is_ascii_whitespace() || matches!(c, '\0' | '"' | '\'' | '=' | '<' | '>' | '`')
                {
                    break;
                }
                n = j + c.len_utf8();
            }
            if n == 0 {
                None
            } else {
                Some(n)
            }
        }
    }
}

/// Count of leading spaces/tabs (and whether a newline was hit).
fn skip_html_ws(s: &str) -> (usize, bool) {
    let mut n = 0;
    for b in s.bytes() {
        match b {
            b' ' | b'\t' => n += 1,
            b'\n' => return (n + 1, true),
            _ => break,
        }
    }
    (n, false)
}

// ---------------------------------------------------------------------------
// Inline → HTML rendering
// ---------------------------------------------------------------------------

/// The plain text content of a node's children (used for image `alt`).
fn plain_text(nodes: &NodeList, parent: usize) -> String {
    let mut out = String::new();
    let mut c = nodes.nodes[parent].first;
    while let Some(i) = c {
        match nodes.nodes[i].kind {
            Kind::Text | Kind::Code | Kind::Html => out.push_str(&nodes.nodes[i].text),
            Kind::Linebreak | Kind::Softbreak => out.push(' '),
            _ => out.push_str(&plain_text(nodes, i)),
        }
        c = nodes.nodes[i].next;
    }
    out
}

fn render_children(
    nodes: &NodeList,
    parent: usize,
    refmap: &HashMap<String, (String, String)>,
    footnotes: &HashMap<String, usize>,
) -> String {
    let mut out = String::new();
    let mut c = nodes.nodes[parent].first;
    while let Some(i) = c {
        let n = &nodes.nodes[i];
        match n.kind {
            Kind::Text => out.push_str(&escape_html(&n.text)),
            Kind::Code => {
                out.push_str("<code>");
                out.push_str(&escape_html(&n.text));
                out.push_str("</code>");
            }
            Kind::Html => out.push_str(&n.text),
            Kind::Linebreak => out.push_str("<br />\n"),
            Kind::Softbreak => out.push('\n'),
            Kind::Emph => {
                out.push_str("<em>");
                out.push_str(&render_children(nodes, i, refmap, footnotes));
                out.push_str("</em>");
            }
            Kind::Strong => {
                out.push_str("<strong>");
                out.push_str(&render_children(nodes, i, refmap, footnotes));
                out.push_str("</strong>");
            }
            Kind::Del => {
                out.push_str("<del>");
                out.push_str(&render_children(nodes, i, refmap, footnotes));
                out.push_str("</del>");
            }
            Kind::Link => {
                out.push_str("<a href=\"");
                out.push_str(&escape_attr(&n.dest));
                out.push('"');
                if !n.title.is_empty() {
                    out.push_str(" title=\"");
                    out.push_str(&escape_attr(&n.title));
                    out.push('"');
                }
                out.push('>');
                out.push_str(&render_children(nodes, i, refmap, footnotes));
                out.push_str("</a>");
            }
            Kind::Image => {
                out.push_str("<img src=\"");
                out.push_str(&escape_attr(&n.dest));
                out.push_str("\" alt=\"");
                out.push_str(&escape_attr(&plain_text(nodes, i)));
                out.push('"');
                if !n.title.is_empty() {
                    out.push_str(" title=\"");
                    out.push_str(&escape_attr(&n.title));
                    out.push('"');
                }
                out.push_str(" />");
            }
            Kind::Footnote => {
                // text holds the label, dest the number
                out.push_str("<sup class=\"footnote-ref\" id=\"fnref-");
                out.push_str(&escape_attr(&n.text));
                out.push_str("\"><a href=\"#fn-");
                out.push_str(&escape_attr(&n.text));
                out.push_str("\">");
                out.push_str(&n.dest);
                out.push_str("</a></sup>");
            }
            Kind::Sup => {
                out.push_str("<sup>");
                out.push_str(&render_inline(&n.text, refmap, footnotes));
                out.push_str("</sup>");
            }
            Kind::Sub => {
                out.push_str("<sub>");
                out.push_str(&render_inline(&n.text, refmap, footnotes));
                out.push_str("</sub>");
            }
            Kind::Mark => {
                out.push_str("<mark>");
                out.push_str(&render_inline(&n.text, refmap, footnotes));
                out.push_str("</mark>");
            }
            Kind::Math => {
                out.push_str(&crate::tex::render(&n.text));
            }
            Kind::Root => {}
        }
        c = n.next;
    }
    out
}

/// Parse a single inline string and render it to HTML.
fn render_inline(
    src: &str,
    refmap: &HashMap<String, (String, String)>,
    footnotes: &HashMap<String, usize>,
) -> String {
    if src.is_empty() {
        return String::new();
    }
    InlineParser::new(src, refmap, footnotes).render()
}

// ---------------------------------------------------------------------------
// Block-level parsing and rendering
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Block {
    Paragraph(String),
    Heading {
        level: usize,
        text: String,
        id: String,
    },
    Fence {
        info: String,
        code: String,
    },
    IndentedCode(String),
    Html(String),
    Hr,
    List {
        ordered: bool,
        start: usize,
        tight: bool,
        items: Vec<ListItem>,
    },
    Quote(Vec<Block>),
    Table {
        align: Vec<Option<u8>>,
        head: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Math(String),
    DefList(Vec<(String, Vec<String>)>),
    Alert {
        kind: String,
        inner: Vec<Block>,
    },
}

#[derive(Debug, PartialEq)]
struct ListItem {
    task: Option<bool>,
    blocks: Vec<Block>,
}

#[derive(Debug)]
struct Footnote {
    label: String,
    lines: Vec<String>,
}

struct BlockParser<'a> {
    lines: &'a [String],
    pos: usize,
    refs: HashMap<String, (String, String)>,
    footnotes: Vec<Footnote>,
}

impl<'a> BlockParser<'a> {
    fn new(lines: &'a [String]) -> BlockParser<'a> {
        BlockParser {
            lines,
            pos: 0,
            refs: HashMap::new(),
            footnotes: Vec::new(),
        }
    }

    fn line(&self) -> Option<&str> {
        self.lines.get(self.pos).map(String::as_str)
    }

    fn peek_line(&self, ahead: usize) -> Option<&str> {
        self.lines.get(self.pos + ahead).map(String::as_str)
    }

    fn is_end(&self) -> bool {
        self.pos >= self.lines.len()
    }

    fn footnote_numbers(&self) -> HashMap<String, usize> {
        self.footnotes
            .iter()
            .enumerate()
            .map(|(i, f)| (f.label.clone(), i + 1))
            .collect()
    }

    fn footnote_section(&self) -> String {
        if self.footnotes.is_empty() {
            return String::new();
        }
        let nums = self.footnote_numbers();
        let mut out = String::new();
        out.push_str("\n<div class=\"footnotes\">\n<hr />\n<ol>\n");
        for f in &self.footnotes {
            out.push_str("<li id=\"fn-");
            out.push_str(&escape_attr(&f.label));
            out.push_str("\">\n<p>");
            out.push_str(&render_inline(&f.lines.join("\n"), &self.refs, &nums));
            out.push_str("</p>\n<a href=\"#fnref-");
            out.push_str(&escape_attr(&f.label));
            out.push_str("\" class=\"footnote-back\">&crarr;</a>\n</li>\n");
        }
        out.push_str("</ol>\n</div>\n");
        out
    }

    fn skip_blanks(&mut self) {
        while let Some(l) = self.line() {
            if l.trim().is_empty() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_blocks(&mut self) -> Vec<Block> {
        let mut out = Vec::new();
        loop {
            self.skip_blanks();
            if self.is_end() {
                break;
            }
            let line = self.line().unwrap().to_string();

            if indent_of(&line) >= 4 {
                out.push(Block::IndentedCode(self.parse_indented_code()));
                continue;
            }
            if let Some((level, text)) = parse_atx(&line) {
                self.pos += 1;
                out.push(Block::Heading {
                    level,
                    text,
                    id: String::new(),
                });
                continue;
            }
            if let Some(marker) = fence_start(&line) {
                let (info, fc, flen) = marker;
                let (code, _) = self.scan_fence(fc, flen, indent_of(&line));
                if info.trim().eq_ignore_ascii_case("math") {
                    out.push(Block::Math(code));
                } else {
                    out.push(Block::Fence { info, code });
                }
                continue;
            }
            if is_thematic(&line) {
                self.pos += 1;
                out.push(Block::Hr);
                continue;
            }
            if line.trim_start().starts_with('>') {
                out.push(self.parse_quote());
                continue;
            }
            if parse_list_marker(&line).is_some() {
                if let Some(list) = self.parse_list() {
                    out.push(list);
                    continue;
                }
            }
            if html_block_start(&line).is_some() {
                out.push(self.parse_html_block());
                continue;
            }
            if let Some(table) = self.try_table() {
                out.push(table);
                continue;
            }
            if self.try_reference() {
                continue;
            }
            if self.try_footnote() {
                continue;
            }
            let is_math_block = match self.line() {
                Some(l) => {
                    let t = l.trim();
                    // `$$` on its own line opens a multi-line display-math
                    // block; single-line `$$…$$` is inline math and is
                    // handled by `parse_math`. `\[…\]` is always block.
                    t == "$$" || t.starts_with("\\[")
                }
                None => false,
            };
            if is_math_block {
                if let Some(b) = self.try_math_block() {
                    out.push(b);
                    continue;
                }
            }
            let para = self.parse_paragraph();
            if let Some(dl) = self.try_deflist(&para) {
                out.push(dl);
            } else {
                out.push(para);
            }
        }
        out
    }

    // ------------------------------------------------------ paragraph/quote

    fn parse_paragraph(&mut self) -> Block {
        let mut lines: Vec<String> = Vec::new();
        let mut setext: Option<usize> = None;
        loop {
            let Some(l) = self.line() else { break };
            let t = l.trim_start();
            if t.is_empty() {
                break;
            }
            // a setext underline turns the collected lines into a heading
            if !lines.is_empty() {
                if let Some(level) = setext_level(l) {
                    setext = Some(level);
                    self.pos += 1;
                    break;
                }
                if is_thematic(l) {
                    break;
                }
                if starts_block(l) || parse_list_marker(l).is_some() {
                    break;
                }
                if def_marker(l).is_some() && indent_of(l) < 4 {
                    break;
                }
            } else if is_thematic(l) {
                break;
            }
            lines.push(l.to_string());
            self.pos += 1;
        }
        match setext {
            Some(level) => Block::Heading {
                level,
                text: lines.join("\n"),
                id: String::new(),
            },
            None => Block::Paragraph(lines.join("\n")),
        }
    }

    fn parse_quote(&mut self) -> Block {
        let mut inner: Vec<String> = Vec::new();
        let mut last_blank = false;
        loop {
            let Some(l) = self.line() else { break };
            let t = l.trim_start();
            if t.starts_with('>') {
                if last_blank {
                    break;
                }
                let ind = indent_of(l);
                let rest = l[ind..].strip_prefix('>').unwrap_or(&l[ind..]);
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                inner.push(rest.to_string());
                last_blank = false;
                self.pos += 1;
            } else if t.is_empty() {
                inner.push(String::new());
                last_blank = true;
                self.pos += 1;
            } else if last_blank {
                break;
            } else {
                inner.push(l.to_string());
                self.pos += 1;
            }
        }
        if let Some(kind) = alert_kind(&inner) {
            if let Some(idx) = inner.iter().position(|l| !l.trim().is_empty()) {
                inner[idx].clear();
            }
            let sub = BlockParser::new(&inner).parse_blocks();
            return Block::Alert { kind, inner: sub };
        }
        Block::Quote(BlockParser::new(&inner).parse_blocks())
    }

    /// Display math: a line that is exactly `$$` (or starts with `\[`) opens an
    /// equation block. `\[…\]` accepts both single-line and multi-line shapes.
    /// `$$…$$` only opens a block when the opening `$$` sits on its own line
    /// (closing `$$` likewise); single-line `$$…$$` is inline math handled by
    /// [`Self::parse_math`]. Anything malformed falls back to a paragraph.
    fn try_math_block(&mut self) -> Option<Block> {
        let line = self.line()?.to_string();
        let t = line.trim();
        if t == "$$" {
            // opening delimiter alone: body until a closing-delimiter line
            self.pos += 1;
            let mut body: Vec<String> = Vec::new();
            while let Some(l) = self.line() {
                if l.trim_start().starts_with("$$") {
                    self.pos += 1;
                    break;
                }
                body.push(l.to_string());
                self.pos += 1;
            }
            return Some(Block::Math(body.join("\n")));
        }
        if !t.starts_with("\\[") {
            return None;
        }
        let open = "\\[";
        let close = "\\]";
        let rest = &t[open.len()..];
        if rest.trim().is_empty() {
            // opening delimiter alone: body until a closing-delimiter line
            self.pos += 1;
            let mut body: Vec<String> = Vec::new();
            while let Some(l) = self.line() {
                if l.trim_start().starts_with(close) {
                    self.pos += 1;
                    break;
                }
                body.push(l.to_string());
                self.pos += 1;
            }
            return Some(Block::Math(body.join("\n")));
        }
        if let Some(inner) = rest.strip_suffix(close) {
            let inner = inner.trim();
            if inner.is_empty() {
                return None;
            }
            self.pos += 1;
            return Some(Block::Math(inner.to_string()));
        }
        None
    }

    /// Pandoc-style definition list: a paragraph whose following line starts
    /// with `: ` (or `:`) becomes the term; each `:` line yields a <dd> entry
    /// with any indented/blank-separated continuation appended to it.
    fn try_deflist(&mut self, para: &Block) -> Option<Block> {
        let Block::Paragraph(term) = para else {
            return None;
        };
        while !self.is_end() && self.line()?.trim().is_empty() {
            self.pos += 1;
        }
        if self.is_end() {
            return None;
        }
        let first = self.line().unwrap();
        if indent_of(first) >= 4 || def_marker(first).is_none() {
            return None;
        }
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut cur: Vec<String> = Vec::new();
        while !self.is_end() {
            let l = self.line().unwrap();
            let t = l.trim_start();
            if t.is_empty() {
                let mut j = self.pos;
                while j < self.lines.len() && self.lines[j].trim().is_empty() {
                    j += 1;
                }
                if j == self.lines.len() {
                    break;
                }
                if indent_of(&self.lines[j]) >= 4 {
                    cur.push(String::new());
                    self.pos = j;
                    continue;
                }
                if def_marker(&self.lines[j]).is_some() {
                    if !cur.is_empty() {
                        groups.push(std::mem::take(&mut cur));
                    }
                    self.pos = j;
                    continue;
                }
                break;
            }
            if indent_of(l) >= 4 {
                let s = if l.len() >= 4 { &l[4..] } else { "" };
                cur.push(s.to_string());
                self.pos += 1;
                continue;
            }
            if let Some(rest) = def_marker(l) {
                if !cur.is_empty() {
                    groups.push(std::mem::take(&mut cur));
                }
                cur.push(rest.to_string());
                self.pos += 1;
                continue;
            }
            break;
        }
        if !cur.is_empty() {
            groups.push(std::mem::take(&mut cur));
        }
        if groups.is_empty() {
            return None;
        }
        let defs: Vec<String> = groups.into_iter().map(|g| g.join("\n")).collect();
        Some(Block::DefList(vec![(term.clone(), defs)]))
    }

    // ------------------------------------------------------- code blocks

    fn parse_indented_code(&mut self) -> String {
        let mut code: Vec<String> = Vec::new();
        loop {
            let Some(l) = self.line() else { break };
            if l.trim().is_empty() {
                // blank line: consume, but only keep the trailing blank runs
                // that are followed by more indented content
                let mut j = self.pos;
                while j < self.lines.len() && self.lines[j].trim().is_empty() {
                    j += 1;
                }
                if j >= self.lines.len() || indent_of(&self.lines[j]) < 4 {
                    break;
                }
                for _ in self.pos..j {
                    code.push(String::new());
                }
                self.pos = j;
                continue;
            }
            if indent_of(l) >= 4 {
                code.push(l[4..].to_string());
                self.pos += 1;
            } else {
                break;
            }
        }
        code.join("\n")
    }

    /// Consume a fenced code block opened by `fc` (` of length `flen`.
    /// Returns `(code, next_pos)`.
    fn scan_fence(&mut self, fc: char, flen: usize, indent: usize) -> (String, usize) {
        self.pos += 1;
        let mut code: Vec<String> = Vec::new();
        while !self.is_end() {
            let l = self.line().unwrap();
            if let Some((_, c, len)) = fence_start(l) {
                if c == fc && len >= flen {
                    let next = self.pos + 1;
                    self.pos = next;
                    return (code.join("\n"), next);
                }
            }
            code.push(strip_to_col(l, indent));
            self.pos += 1;
        }
        (code.join("\n"), self.pos)
    }

    // ------------------------------------------------------- HTML blocks

    fn parse_html_block(&mut self) -> Block {
        let first = self.line().unwrap().to_string();
        let kind = html_block_start(&first).map(|k| k.0).unwrap_or(6);
        let mut acc: Vec<String> = vec![first.clone()];
        self.pos += 1;
        let closed_first = match kind {
            2 => first.contains("-->"),
            3 => first.contains("?>"),
            4 => first.contains('>'),
            5 => first.contains("]]>"),
            _ => false,
        };
        if closed_first {
            return Block::Html(first);
        }
        loop {
            if self.is_end() {
                break;
            }
            let l = self.line().unwrap().to_string();
            acc.push(l.clone());
            self.pos += 1;
            let done = match kind {
                1 => html_close_found(&acc),
                2 => l.contains("-->"),
                3 => l.contains("?>"),
                4 => l.contains('>'),
                5 => l.contains("]]>"),
                _ => l.trim().is_empty(),
            };
            if done {
                break;
            }
        }
        Block::Html(acc.join("\n"))
    }

    // -------------------------------------------------------------- tables

    fn try_table(&mut self) -> Option<Block> {
        let head = self.line()?.to_string();
        let hc = split_table_row(&head)?;
        if hc.len() <= 1 && !head.contains('|') {
            return None;
        }
        let delim_line = self.peek_line(1)?.to_string();
        let cells = split_table_row(&delim_line)?;
        let align: Vec<Option<u8>> = cells.iter().map(|c| cell_align(c)).collect();
        if align.len() != hc.len() {
            return None;
        }
        self.pos += 2;
        let mut rows: Vec<Vec<String>> = Vec::new();
        loop {
            match self.line() {
                Some(l) if !l.trim().is_empty() && !starts_block(l) && l.contains('|') => {
                    match split_table_row(l) {
                        Some(r) => rows.push(r),
                        None => break,
                    }
                    self.pos += 1;
                }
                _ => break,
            }
        }
        Some(Block::Table {
            align,
            head: hc,
            rows,
        })
    }

    // ------------------------------------------------ references, footnotes

    fn try_reference(&mut self) -> bool {
        let Some(line) = self.line() else {
            return false;
        };
        if indent_of(line) >= 4 {
            return false;
        }
        let t = line.trim_start();
        if !t.starts_with('[') {
            return false;
        }
        let Some(end) = t.find(']') else { return false };
        if end == 0 {
            return false;
        }
        let label = &t[1..end];
        if label.is_empty() || label.starts_with('^') {
            return false; // footnote definitions are handled elsewhere
        }
        let Some(colon) = t[end + 1..].strip_prefix(':') else {
            return false;
        };
        let dest_rest = colon.trim_start();
        let Some((dest, used)) = ref_dest(dest_rest) else {
            return false;
        };
        if dest.is_empty() {
            return false;
        }
        let after_dest = &dest_rest[used..];
        let title = ref_title(after_dest).map(|(s, _)| s).unwrap_or_default();
        let key = InlineParser::<'_>::normalize_reference(label);
        self.refs.entry(key).or_insert((dest, title));
        self.pos += 1;
        true
    }

    fn try_footnote(&mut self) -> bool {
        let Some(line) = self.line() else {
            return false;
        };
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("[^") else {
            return false;
        };
        let mut end = 0;
        let mut ok = true;
        for (j, c) in rest.char_indices() {
            if c == ']' {
                end = j;
                break;
            }
            if !(c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                ok = false;
                break;
            }
        }
        if !ok || end == 0 {
            return false;
        }
        let label = rest[..end].to_string();
        let Some(after) = rest[end + 1..].strip_prefix(':') else {
            return false;
        };
        let first_content = after.trim_start().to_string();

        self.pos += 1;
        let mut lines: Vec<String> = vec![first_content];
        loop {
            let Some(l) = self.line() else { break };
            if l.trim().is_empty() {
                // a blank continues the definition only if a more-indented
                // (>=4 columns) line follows
                let mut j = self.pos + 1;
                while j < self.lines.len() && self.lines[j].trim().is_empty() {
                    j += 1;
                }
                if j >= self.lines.len() || indent_of(&self.lines[j]) < 4 {
                    break;
                }
                self.pos = j;
                continue;
            }
            if indent_of(l) >= 4 {
                lines.push(l[4..].to_string());
                self.pos += 1;
            } else {
                break;
            }
        }
        if lines.iter().all(|l| l.trim().is_empty()) {
            return false; // nothing but a marker: not a definition
        }
        self.footnotes.push(Footnote { label, lines });
        true
    }

    // --------------------------------------------------------------- lists

    fn parse_list(&mut self) -> Option<Block> {
        let marker0 = parse_list_marker(self.line()?)?;
        let ordered = marker0.ordered;
        let start = marker0.start;
        let mut items: Vec<ListItem> = Vec::new();
        let mut loose = false;

        while let Some(line0) = self.line() {
            let Some(m) = parse_list_marker(line0) else {
                break;
            };
            if m.ordered != ordered {
                break;
            }
            let marker_start = indent_of(line0);
            let content_col = m.content_col;
            let first_line = &line0[content_col..];
            let mut raw: Vec<String> = Vec::new();
            if !first_line.trim().is_empty() {
                raw.push(first_line.to_string());
            }
            self.pos += 1;

            loop {
                if self.is_end() {
                    break;
                }
                let l = self.line().unwrap().to_string();
                if l.trim().is_empty() {
                    let mut j = self.pos;
                    while j < self.lines.len() && self.lines[j].trim().is_empty() {
                        j += 1;
                    }
                    let Some(next) = self.lines.get(j) else { break };
                    let next_ind = indent_of(next);
                    let same_item_level = {
                        match parse_list_marker(next) {
                            Some(m2) => next_ind == marker_start && m2.ordered == ordered,
                            None => false,
                        }
                    };
                    if same_item_level {
                        loose = true;
                        break;
                    }
                    // a blank belongs to the item only when content follows
                    if next_ind >= content_col {
                        loose = true;
                        let n = j - self.pos;
                        for _ in 0..n {
                            raw.push(String::new());
                        }
                        self.pos = j;
                        continue;
                    }
                    break;
                }
                let ind = indent_of(&l);
                let same_item_level = match parse_list_marker(&l) {
                    Some(m2) => ind == marker_start && m2.ordered == ordered,
                    None => false,
                };
                if same_item_level {
                    break;
                }
                if ind >= content_col {
                    raw.push(l[content_col..].to_string());
                    self.pos += 1;
                    continue;
                }
                if parse_list_marker(&l).is_some() {
                    break; // a different list at this level ends this list
                }
                if starts_block(&l) || is_thematic(&l) {
                    break;
                }
                // lazy paragraph continuation
                raw.push(l.to_string());
                self.pos += 1;
            }

            let task = task_status(raw.iter());
            if task.is_some() {
                strip_task_marker(&mut raw);
            }
            let mut sub = BlockParser::new(&raw);
            let blocks = sub.parse_blocks();
            items.push(ListItem { task, blocks });
        }

        Some(Block::List {
            ordered,
            start,
            tight: !loose,
            items,
        })
    }
}

// ---------------------------------------------------------------------------
// Block helpers
// ---------------------------------------------------------------------------

struct ListMarker {
    ordered: bool,
    start: usize,
    content_col: usize,
}

/// Parse a list marker: `-`/`*`/`+`, or `1.`/`1)` (up to two leading spaces,
/// up to three spaces indentation per CommonMark).
fn parse_list_marker(line: &str) -> Option<ListMarker> {
    let ind = indent_of(line);
    if ind >= 4 {
        return None;
    }
    let b = line.as_bytes();
    let (ordered, start, width) = match b.get(ind) {
        Some(b'-') | Some(b'*') | Some(b'+') => (false, 1usize, 1usize),
        Some(c) if c.is_ascii_digit() => {
            let mut j = ind;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j - ind > 9 {
                return None;
            }
            let start: usize = std::str::from_utf8(&b[ind..j]).ok()?.parse().ok()?;
            match b.get(j) {
                Some(b'.') | Some(b')') => (true, start, j - ind + 1),
                _ => return None,
            }
        }
        _ => return None,
    };
    let after = ind + width;
    let content_col = if after < b.len() && b[after] == b' ' {
        after + 1
    } else if after >= b.len() {
        after
    } else {
        return None;
    };
    Some(ListMarker {
        ordered,
        start,
        content_col,
    })
}

/// Look for a `[ ]`/`[x]`/`[X]` checkbox at the start of the first content line
/// of a list item. Returns `Some(true/false)` when a task marker is present.
fn task_status<'a>(lines: impl Iterator<Item = &'a String>) -> Option<bool> {
    for l in lines {
        let t = l.trim_start();
        if t.is_empty() {
            continue;
        }
        // `[ ]` / `[x]` / `[X]` at the very start of the item text
        let b = t.as_bytes();
        if b.len() >= 3 && b[0] == b'[' && b[2] == b']' {
            return match b[1] {
                b' ' => Some(false),
                b'x' | b'X' => Some(true),
                _ => Some(false),
            };
        }
        return None;
    }
    None
}

/// Remove a leading `[ ]`/`[x]` task marker (and following space) from the
/// item's first content line.
fn strip_task_marker(raw: &mut [String]) {
    let Some(l) = raw.first_mut() else { return };
    let t = l.trim_start();
    let lead = l.len() - t.len();
    let Some(rest) = t.strip_prefix('[') else {
        return;
    };
    let Some(mark_end) = rest.find(']') else {
        return;
    };
    let mark = &rest[..mark_end];
    if mark.len() != 1 || !matches!(mark.as_bytes()[0], b' ' | b'x' | b'X') {
        return;
    }
    let after = rest[mark_end + 1..].trim_start();
    *l = format!("{}{}", " ".repeat(lead), after);
}

// ----------------------------------------------------------- ATX / setext

/// `` : text `` (or a lone `:`) starts a definition-list entry.
fn def_marker(l: &str) -> Option<&str> {
    let t = l.trim_start();
    let rest = t.strip_prefix(':')?;
    if rest.is_empty() {
        return Some("");
    }
    if let Some(body) = rest.strip_prefix(' ') {
        return Some(body);
    }
    None
}

/// GitHub-style admonition label at the top of a blockquote body.
fn alert_kind(lines: &[String]) -> Option<String> {
    for l in lines {
        let t = l.trim();
        if t.is_empty() {
            continue;
        }
        let rest = t.strip_prefix("[!")?;
        let end = rest.find(']')?;
        let kind = rest[..end].to_ascii_uppercase();
        if matches!(
            kind.as_str(),
            "NOTE" | "TIP" | "WARNING" | "IMPORTANT" | "CAUTION" | "INFO" | "SUCCESS" | "DANGER"
        ) {
            return Some(kind);
        }
        return None;
    }
    None
}

fn parse_atx(line: &str) -> Option<(usize, String)> {
    let t = line.trim_start();
    let n = t.chars().take_while(|&c| c == '#').count();
    if n == 0 || n > 6 {
        return None;
    }
    let rest = &t[n..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let body = rest.trim_start();
    let text = strip_closing_hashes(body.trim_end());
    Some((n, text))
}

/// Remove a closing run of `#`s that is preceded by whitespace.
fn strip_closing_hashes(s: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    let t = s.trim_end();
    if t.ends_with('#') {
        let idx = t.trim_end_matches('#').len();
        let before = &t[..idx];
        if before.ends_with(' ') {
            return before.trim_end().to_string();
        }
    }
    t.to_string()
}

fn setext_level(line: &str) -> Option<usize> {
    let t = line.trim_start();
    if t.is_empty() {
        return None;
    }
    if t.starts_with('=') && t.chars().all(|c| c == '=' || c == ' ') {
        return Some(1);
    }
    if t.starts_with('-') && t.len() >= 3 && t.chars().all(|c| c == '-' || c == ' ') {
        return Some(2);
    }
    None
}

fn is_thematic(line: &str) -> bool {
    let t = line.trim_start();
    if t.len() < 3 {
        return false;
    }
    let first = t.chars().next().unwrap();
    if first != '-' && first != '*' && first != '_' {
        return false;
    }
    let mut seen = 0;
    for c in t.chars() {
        if c == ' ' || c == '\t' {
            continue;
        }
        if c != first {
            return false;
        }
        seen += 1;
    }
    seen >= 3
}

fn fence_start(line: &str) -> Option<(String, char, usize)> {
    let t = line.trim_start();
    let c = t.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let len = t.chars().take_while(|&x| x == c).count();
    if len < 3 {
        return None;
    }
    let info = t[len..].trim().to_string();
    Some((info, c, len))
}

fn strip_to_col(line: &str, col: usize) -> String {
    let ind = indent_of(line);
    if ind >= col {
        line[col..].to_string()
    } else {
        line.to_string()
    }
}

/// CommonMark HTML block types 1-7. Returns `(kind, matched_tag)`.
fn html_block_start(line: &str) -> Option<(u8, String)> {
    let t = line.trim_start().to_ascii_lowercase();
    if !t.starts_with('<') {
        return None;
    }
    if t.starts_with("<!--") {
        return Some((2, String::new()));
    }
    if t.starts_with("<?") {
        return Some((3, String::new()));
    }
    if t.starts_with("<![cdata[") {
        return Some((5, String::new()));
    }
    if let Some(rest) = t.strip_prefix("<!") {
        if let Some(c) = rest.chars().next() {
            if c.is_ascii_alphabetic() {
                return Some((4, String::new()));
            }
        }
    }
    const TYPE6: [&str; 62] = [
        "address",
        "article",
        "aside",
        "base",
        "basefont",
        "blockquote",
        "body",
        "caption",
        "center",
        "col",
        "colgroup",
        "dd",
        "details",
        "dialog",
        "dir",
        "div",
        "dl",
        "dt",
        "fieldset",
        "figcaption",
        "figure",
        "footer",
        "form",
        "frame",
        "frameset",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "head",
        "header",
        "hr",
        "html",
        "iframe",
        "legend",
        "li",
        "link",
        "main",
        "menu",
        "menuitem",
        "nav",
        "noframes",
        "ol",
        "optgroup",
        "option",
        "p",
        "param",
        "section",
        "source",
        "summary",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "title",
        "tr",
        "track",
        "ul",
    ];
    for tag in TYPE6.iter() {
        let rest = &t[1..];
        if let Some(after) = rest.strip_prefix(tag) {
            if after.is_empty()
                || after.starts_with('>')
                || after.starts_with("/>")
                || after.starts_with(char::is_whitespace)
            {
                return Some((6, tag.to_string()));
            }
        }
    }
    // type 7: any complete open/close tag, followed only by whitespace, to EOL
    let t_orig = line.trim_start();
    if let Some((_, len)) = match_html_inline(t_orig) {
        if t_orig[len..].trim().is_empty() {
            return Some((7, String::new()));
        }
    }
    None
}

fn html_close_found(acc: &[String]) -> bool {
    let joined = acc.join("\n").to_ascii_lowercase();
    for tag in [
        "script",
        "pre",
        "style",
        "textarea",
        "title",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "noscript",
        "plaintext",
    ] {
        if joined.contains(&format!("</{tag}")) {
            return true;
        }
    }
    false
}

// --------------------------------------------------------------- tables

fn split_table_row(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    if !t.starts_with('|') && !t.contains('|') {
        return None;
    }
    let body = match (t.starts_with('|'), t.ends_with('|')) {
        (true, true) => &t[1..t.len() - 1],
        (true, false) => &t[1..],
        (false, true) => &t[..t.len() - 1],
        (false, false) => t,
    };
    let mut cells: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut esc = false;
    for c in body.chars() {
        if esc {
            cur.push(c);
            esc = false;
            continue;
        }
        if c == '\\' {
            esc = true;
            cur.push(c);
            continue;
        }
        if c == '|' {
            cells.push(cur.trim().to_string());
            cur = String::new();
        } else {
            cur.push(c);
        }
    }
    cells.push(cur.trim().to_string());
    Some(cells)
}

/// `:---`, `:---:`, `---:` or `---`; returns 1 (left), 2 (center), 3 (right),
/// 0 (default).
fn cell_align(cell: &str) -> Option<u8> {
    let c = cell.trim();
    if c.is_empty() {
        return None;
    }
    let body = c.trim_matches(':');
    if body.is_empty() || !body.chars().all(|ch| ch == '-') {
        return None;
    }
    Some(match (c.starts_with(':'), c.ends_with(':')) {
        (true, true) => 2,
        (true, false) => 1,
        (false, true) => 3,
        (false, false) => 0,
    })
}

/// A line starts a block that cannot be lazy paragraph continuation.
fn starts_block(l: &str) -> bool {
    if indent_of(l) >= 4 {
        return true;
    }
    if parse_atx(l).is_some() || fence_start(l).is_some() {
        return true;
    }
    if is_thematic(l) || l.trim_start().starts_with('>') || html_block_start(l).is_some() {
        return true;
    }
    false
}

// ------------------------------------------------- references / footnotes

/// Parse a reference definition destination. Returns `(destination, bytes)`.
fn ref_dest(rest: &str) -> Option<(String, usize)> {
    if let Some(e) = rest.strip_prefix('<') {
        let end = e.find('>')?;
        return Some((unescape(&e[..end]), end + 2));
    }
    let mut len = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '\\' => {
                // keep slash + next char in the destination
                let _ = c;
            }
            c if c.is_whitespace() => break,
            _ => {}
        }
        len = i + c.len_utf8();
    }
    if len == 0 {
        return None;
    }
    Some((unescape(&rest[..len]), len))
}

/// Parse an optional title after a reference destination: `"..."`, `'...'` or
/// `(...)`. Returns `(title, bytes consumed within rest)`.
fn ref_title(rest: &str) -> Option<(String, usize)> {
    let t = rest.trim_start();
    let lead = rest.len() - t.len();
    let q = t.chars().next()?;
    let close = match q {
        '"' | '\'' => q,
        '(' => ')',
        _ => return None,
    };
    let body = &t[1..];
    let mut esc = false;
    for (i, c) in body.char_indices() {
        if esc {
            esc = false;
            continue;
        }
        if c == '\\' {
            esc = true;
            continue;
        }
        if c == close {
            return Some((unescape(&body[..i]), lead + 1 + i + 1));
        }
        if c == '\n' {
            return None;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// HTML rendering of blocks
// ---------------------------------------------------------------------------

fn blocks_to_html(
    blocks: &[Block],
    refs: &HashMap<String, (String, String)>,
    footnotes: &HashMap<String, usize>,
) -> String {
    let mut out = String::new();
    let mut ctx = RenderCtx::default();
    for b in blocks {
        block_to_html(b, refs, footnotes, &mut ctx, &mut out);
    }
    out
}

/// Raw HTML blocks are passed through verbatim, except that `$$…$$` (rendered
/// inline, since double dollars are inline math now) and `\[…\]` spans are
/// replaced by their rendered form — so formulas written inside a `<p>` or
/// `<div>` wrapper still get typeset. Drawing environments (`picture`,
/// xy-pic, TikZ) only honour the display `\[…\]` form here.
fn math_in_html(code: &str) -> String {
    let mut out = String::new();
    let mut rest = code;
    loop {
        let open = match (rest.find("$$"), rest.find("\\[")) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let Some(i) = open else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..i]);
        // `$$…$$` is inline; only `\[…\]` keeps the display wrappers and the
        // drawing-environment short-circuit.
        let (open_s, close_s, block) = if rest[i..].starts_with("$$") {
            ("$$", "$$", false)
        } else {
            ("\\[", "\\]", true)
        };
        let tail = &rest[i + open_s.len()..];
        if let Some(j) = tail.find(close_s) {
            let src = tail[..j].trim();
            if !src.is_empty() {
                if block && crate::drawing::is_drawing(src) {
                    if let Some(svg) = crate::drawing::render(src) {
                        out.push_str("<div class=\"drawing\">");
                        out.push_str(&svg);
                        out.push_str("</div>");
                        rest = &tail[j + close_s.len()..];
                        continue;
                    }
                }
                let m = if block {
                    crate::tex::render_block(src)
                } else {
                    crate::tex::render(src)
                };
                if !m.is_empty() {
                    if block {
                        out.push_str("<div class=\"math\">");
                        out.push_str(&m);
                        out.push_str("</div>");
                    } else {
                        out.push_str(&m);
                    }
                    rest = &tail[j + close_s.len()..];
                    continue;
                }
            }
            // empty body: keep the literal delimiters and the body text
            out.push_str(open_s);
            out.push_str(&tail[..j]);
            out.push_str(close_s);
            rest = &tail[j + close_s.len()..];
            continue;
        }
        // unterminated: emit the opening delimiter literally and move on
        out.push_str(open_s);
        rest = tail;
    }
    out
}

fn align_attr(a: &Option<u8>) -> &'static str {
    match a {
        Some(1) => " style=\"text-align: left\"",
        Some(2) => " style=\"text-align: center\"",
        Some(3) => " style=\"text-align: right\"",
        _ => "",
    }
}

/// Per-document render state: heading slug dedup + collected TOC entries.
#[derive(Default)]
struct RenderCtx {
    used_slugs: HashMap<String, usize>,
    toc: Vec<(usize, String, String)>,
    toc_rendered: bool,
}

/// Turn a heading's plain text into an anchor id: lowercase, spaces/dots →
/// hyphens, strip other punctuation, JS-free.
fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in text.chars() {
        let k = c.to_ascii_lowercase();
        if k.is_ascii_alphanumeric() {
            out.push(k);
            last_dash = false;
        } else if !out.is_empty() && !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Reserve a unique slug for a heading: dedupe with a numeric suffix.
fn reserve_slug(ctx: &mut RenderCtx, text: &str) -> String {
    let base = slugify(text);
    if base.is_empty() {
        return String::new();
    }
    let n = ctx.used_slugs.entry(base.clone()).or_insert(0);
    *n += 1;
    if *n == 1 {
        base
    } else {
        format!("{base}-{n}")
    }
}

fn toc_html(toc: &[(usize, String, String)]) -> String {
    if toc.is_empty() {
        return String::new();
    }
    // Simplify: flat nested <ul> by keeping a stack of open levels.
    let mut out = String::new();
    out.push_str("<nav class=\"toc\"><ul>\n");
    let mut stack: Vec<usize> = Vec::new();
    for (level, id, text) in toc {
        while stack.last().copied().unwrap_or(0) >= *level {
            out.push_str("</ul></li>\n");
            stack.pop();
        }
        out.push_str(&format!("<li><a href=\"#{id}\">{}</a>", escape_html(text)));
        if *level > stack.len() {
            out.push_str("<ul>\n");
            stack.push(*level);
        } else {
            out.push_str("</li>\n");
        }
    }
    while !stack.is_empty() {
        out.push_str("</ul></li>\n");
        stack.pop();
    }
    out.push_str("</ul></nav>\n");
    out
}

fn block_to_html(
    b: &Block,
    refs: &HashMap<String, (String, String)>,
    footnotes: &HashMap<String, usize>,
    ctx: &mut RenderCtx,
    out: &mut String,
) {
    match b {
        Block::Paragraph(text) => {
            let t = text.trim();
            // a `[[TOC]]` / `[TOC]` marker renders the collected heading list
            if (t == "[[TOC]]" || t == "[TOC]") && !ctx.toc_rendered {
                out.push_str(&toc_html(&ctx.toc));
                ctx.toc_rendered = true;
                return;
            }
            out.push_str("<p>");
            out.push_str(&render_inline(text, refs, footnotes));
            out.push_str("</p>\n");
        }
        Block::Heading { level, text, id } => {
            let heading_id = if !id.is_empty() {
                id.clone()
            } else {
                let plain = plain_inline_text(text);
                let slug = reserve_slug(ctx, &plain);
                ctx.toc.push((*level, slug.clone(), plain));
                slug
            };
            out.push_str(&format!("<h{level} id=\"{heading_id}\">"));
            out.push_str(&render_inline(text, refs, footnotes));
            out.push_str(&format!("</h{level}>\n"));
        }
        Block::Fence { info, code } => {
            let lang = info.split_whitespace().next().unwrap_or("");
            if lang == "mermaid" || lang == "flowchart" {
                if let Some(svg) = crate::diagram::render(code) {
                    out.push_str("<div class=\"mermaid\">");
                    out.push_str(&svg);
                    out.push_str("</div>\n");
                    return;
                }
            }
            if matches!(lang, "latex" | "tex" | "tikz" | "xy" | "picture") {
                if let Some(svg) = crate::drawing::render(code) {
                    out.push_str("<div class=\"drawing\">");
                    out.push_str(&svg);
                    out.push_str("</div>\n");
                    return;
                }
            }
            out.push_str("<pre><code");
            if !lang.is_empty() {
                out.push_str(" class=\"language-");
                out.push_str(&escape_attr(lang));
                out.push('"');
            }
            out.push('>');
            out.push_str(&escape_html(code));
            out.push_str("</code></pre>\n");
        }
        Block::IndentedCode(code) => {
            out.push_str("<pre><code>");
            out.push_str(&escape_html(code));
            out.push_str("</code></pre>\n");
        }
        Block::Html(code) => {
            // HTML comments are invisible: drop them entirely.
            if code.trim_start().starts_with("<!--") {
                return;
            }
            out.push_str(&math_in_html(code));
            out.push('\n');
        }
        Block::Hr => out.push_str("<hr />\n"),
        Block::Math(src) => {
            if crate::drawing::is_drawing(src) {
                if let Some(svg) = crate::drawing::render(src) {
                    out.push_str("<div class=\"drawing\">");
                    out.push_str(&svg);
                    out.push_str("</div>\n");
                    return;
                }
            }
            let m = crate::tex::render_block(src);
            if !m.is_empty() {
                out.push_str("<div class=\"math\">");
                out.push_str(&m);
                out.push_str("</div>\n");
            }
        }
        Block::DefList(entries) => {
            out.push_str("<dl>\n");
            for (term, defs) in entries {
                out.push_str("<dt>");
                out.push_str(&render_inline(term, refs, footnotes));
                out.push_str("</dt>\n");
                for d in defs {
                    out.push_str("<dd>");
                    out.push_str(&render_inline(d, refs, footnotes));
                    out.push_str("</dd>\n");
                }
            }
            out.push_str("</dl>\n");
        }
        Block::Alert { kind, inner } => {
            let cls = kind.to_ascii_lowercase();
            out.push_str(&format!("<div class=\"admonition {cls}\">\n"));
            out.push_str(&format!(
                "<p class=\"admonition-title\"><strong>{}</strong></p>\n",
                escape_html(kind)
            ));
            out.push_str(&blocks_to_html(inner, refs, footnotes));
            out.push_str("</div>\n");
        }
        Block::Quote(inner) => {
            out.push_str("<blockquote>\n");
            out.push_str(&blocks_to_html(inner, refs, footnotes));
            out.push_str("</blockquote>\n");
        }
        Block::List {
            ordered,
            start,
            tight,
            items,
        } => {
            if *ordered {
                if *start > 1 {
                    out.push_str(&format!("<ol start=\"{start}\">\n"));
                } else {
                    out.push_str("<ol>\n");
                }
            } else {
                out.push_str("<ul>\n");
            }
            for item in items {
                out.push_str("<li");
                if item.task.is_some() {
                    out.push_str(" class=\"task-list-item\"");
                }
                out.push('>');
                if let Some(task) = item.task {
                    let checked = if task { " checked" } else { "" };
                    out.push_str(&format!("<input type=\"checkbox\" disabled{checked} /> "));
                }
                let inner = blocks_to_html(&item.blocks, refs, footnotes);
                // tight items: collapse a single wrapping paragraph
                if *tight && inner.starts_with("<p>") && inner.ends_with("</p>\n") {
                    out.push_str(&inner[3..inner.len() - 5]);
                } else {
                    out.push_str(&inner);
                }
                out.push_str("</li>\n");
            }
            if *ordered {
                out.push_str("</ol>\n");
            } else {
                out.push_str("</ul>\n");
            }
        }
        Block::Table { align, head, rows } => {
            out.push_str("<table>\n<thead>\n<tr>");
            for (i, h) in head.iter().enumerate() {
                out.push_str("<th");
                out.push_str(align_attr(align.get(i).unwrap_or(&None)));
                out.push('>');
                out.push_str(&render_inline(h, refs, footnotes));
                out.push_str("</th>");
            }
            out.push_str("</tr>\n</thead>\n<tbody>\n");
            for row in rows {
                out.push_str("<tr>");
                for (i, c) in row.iter().enumerate() {
                    out.push_str("<td");
                    out.push_str(align_attr(align.get(i).unwrap_or(&None)));
                    out.push('>');
                    out.push_str(&render_inline(c, refs, footnotes));
                    out.push_str("</td>");
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</tbody>\n</table>\n");
        }
    }
}

/// HTML-escape text but keep it inline-safe (used for heading anchor ids).
/// Rough plain-text extraction of a heading (strip inline markdown markers).
fn plain_inline_text(src: &str) -> String {
    let mut out = String::new();
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '_' | '~' | '`' | '=' | '^' | '!' | '\\' => {}
            '[' | ']' if chars.peek() == Some(&']') || c == ']' => {}
            '$' => {}
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_code_span_is_not_reparsed() {
        let h = render(
            "a `:smile:` and `$x^2$` and `H~2~O` and `x==y==` and `<!-- c -->` and `**em**`",
        );
        assert!(h.contains("<code>:smile:</code>"));
        assert!(h.contains("<code>$x^2$</code>"));
        assert!(h.contains("<code>H~2~O</code>"));
        assert!(h.contains("<code>x==y==</code>"));
        assert!(h.contains("<code>&lt;!-- c --&gt;</code>"));
        assert!(h.contains("<code>**em**</code>"));
        assert!(!h.contains("<math"));
        assert!(!h.contains("<sub>"));
        assert!(!h.contains("<mark>"));
    }

    #[test]
    fn inline_code_span_multiple_and_empty() {
        let h = render("`` `foo` `` and `` `` and a `b` c `d`");
        assert!(h.contains("<code>`foo`</code>"));
        assert!(h.contains("<code> </code>"));
        assert!(h.contains("<code>b</code>"));
        assert!(h.contains("<code>d</code>"));
    }

    #[test]
    fn unmatched_backtick_stays_literal() {
        let h = render("a ` b c");
        assert!(h.contains("<p>a ` b c</p>"));
    }

    #[test]
    fn renders_heading_and_emphasis() {
        let h = render("# Title\n\nSome **bold** and *italic* text.");
        assert!(h.contains("<h1 id=\"title\">Title</h1>"));
        assert!(h.contains("<strong>bold</strong>"));
        assert!(h.contains("<em>italic</em>"));
    }

    #[test]
    fn renders_code_block() {
        let h = render("```rust\nfn main() {}\n```");
        assert!(h.contains("language-rust"));
        assert!(h.contains("fn main() {}"));
    }

    #[test]
    fn renders_link() {
        let h = render("[google](https://google.com)");
        assert!(h.contains("<a href=\"https://google.com\">google</a>"));
    }

    #[test]
    fn renders_list() {
        let h = render("- a\n- b");
        assert!(h.contains("<li>a</li>"));
        assert!(h.contains("<li>b</li>"));
    }

    #[test]
    fn renders_nested_list() {
        let h = render("- a\n  - b\n- c");
        assert!(h.contains("<ul>"));
        assert_eq!(h.matches("<li>").count(), 3);
    }

    #[test]
    fn renders_ordered_list_with_start() {
        let h = render("3. three\n4. four");
        assert!(h.contains("<ol start=\"3\">"));
        assert!(h.contains("<li>three</li>"));
    }

    #[test]
    fn renders_task_list() {
        let h = render("- [x] done\n- [ ] todo");
        assert!(h.contains("class=\"task-list-item\""));
        assert!(h.contains("checked"));
        assert!(h.contains("done</li>"));
        assert!(h.contains("todo</li>"));
    }

    #[test]
    fn renders_setext_heading() {
        let h = render("Title\n=====");
        assert!(h.contains("<h1 id=\"title\">Title</h1>"));
    }

    #[test]
    fn renders_indented_code() {
        let h = render("    let x = 1;");
        assert!(h.contains("<pre><code>let x = 1;"));
    }

    #[test]
    fn renders_table() {
        let src = "| a | b |\n| :- | :-: |\n| 1 | 2 |";
        let h = render(src);
        assert!(h.contains("<table>"));
        assert!(h.contains("<th style=\"text-align: left\">a</th>"));
        assert!(h.contains("<td style=\"text-align: center\">2</td>"));
        assert!(h.contains("text-align: center"));
    }

    #[test]
    fn renders_blockquote() {
        let h = render("> quoted\n> text");
        assert!(h.contains("<blockquote>"));
        assert!(h.contains("quoted"));
    }

    #[test]
    fn renders_footnote() {
        let h = render("text[^1]\n\n[^1]: a note");
        assert!(h.contains("id=\"fnref-1\""));
        assert!(h.contains("a note"));
        assert!(h.contains("class=\"footnotes\""));
    }

    #[test]
    fn renders_horizontal_rule() {
        let h = render("---");
        assert!(h.contains("<hr />"));
    }

    #[test]
    fn escapes_code_injection_from_fence() {
        let h = render("```\n</pre><script>alert(1)</script>\n```");
        assert!(!h.contains("</pre></code></pre><script>"));
        assert!(h.contains("&lt;/pre&gt;"));
    }

    #[test]
    fn reference_link_resolution() {
        let src = "[foo][id]\n\n[id]: https://example.com\n";
        let h = render(src);
        assert!(h.contains("<a href=\"https://example.com\">foo</a>"));
    }

    #[test]
    fn autolink() {
        let h = render("see <https://example.com>");
        assert!(h.contains("<a href=\"https://example.com\">https://example.com</a>"));
    }

    #[test]
    fn inline_math_renders_mathml() {
        let h = render("value $a^2$ here");
        assert!(h.contains("<math"));
        assert!(h.contains("</math>"));
        assert!(h.contains("a"));
        assert!(!h.contains("$a^2$"));
    }

    #[test]
    fn dollar_inline_is_math_not_block() {
        let h = render("$$\\frac{1}{2}$$");
        // `$$…$$` is inline math, so it renders as a bare MathML <math>
        // element without the display-math <div class="math"> wrapper.
        assert!(h.contains("<math"));
        assert!(h.contains("<mfrac>"));
        assert!(!h.contains("<div class=\"math\">"));
    }

    #[test]
    fn fenced_math_block_still_wraps_div() {
        let h = render("```math\n\\sum_{i=1}^n i\n```");
        assert!(h.contains("<div class=\"math\"><svg"));
        assert!(h.contains("<mo>∑</mo>") || h.contains("&#x2211;") || h.contains("∑"));
    }

    #[test]
    fn multi_line_dollar_block_still_wraps_div() {
        let h = render("$$\n\\sum_{i=1}^n i\n$$");
        assert!(h.contains("<div class=\"math\"><svg"));
        assert!(h.contains("<mo>∑</mo>") || h.contains("&#x2211;") || h.contains("∑"));
        assert!(!h.contains("$$\n"));
    }

    #[test]
    fn inline_dollar_inside_paragraph_stays_inline() {
        // `$$x^2$$` on its own line inside a paragraph should render
        // inline (the block form needs `$$` on its own line).
        let h = render("first\n\n$$x^2$$\n\nlast");
        assert!(h.contains("<math"));
        assert!(!h.contains("<div class=\"math\">"));
        assert!(h.contains("first"));
        assert!(h.contains("last"));
    }

    #[test]
    fn picture_fence_routes_to_drawing() {
        let h = render(
            "```picture\n\\begin{picture}(4,4)\n\\put(2,2){\\circle*{2}}\n\\end{picture}\n```",
        );
        assert!(h.contains("<div class=\"drawing\"><svg"), "{h}");
        assert!(h.contains("aria-label=\"LaTeX picture\""), "{h}");
    }

    #[test]
    fn latex_fence_routes_to_drawing() {
        let h = render(
            "```latex\n\\xymatrix{A \\ar[r] & B}\n```",
        );
        assert!(h.contains("<div class=\"drawing\"><svg"), "{h}");
        assert!(h.contains("aria-label=\"commutative diagram\""), "{h}");
    }

    #[test]
    fn tikz_fence_routes_to_drawing() {
        let h = render(
            "```tikz\n\\begin{tikzpicture}\\draw (0,0) -- (1,1);\\end{tikzpicture}\n```",
        );
        assert!(h.contains("<div class=\"drawing\"><svg"), "{h}");
        assert!(h.contains("aria-label=\"TikZ drawing\""), "{h}");
    }

    #[test]
    fn latex_fence_falls_back_when_unsupported() {
        let h = render("```latex\nnot a real drawing\n```");
        assert!(h.contains("<pre><code"), "fallback to code: {h}");
    }

    #[test]
    fn drawing_inside_raw_html_block() {
        let h = render(
            "<p>$$</p>\n\n```xy\n\\xymatrix{A \\ar[r] & B}\n```\n\n<p>trailing</p>",
        );
        assert!(h.contains("<div class=\"drawing\"><svg"), "{h}");
    }

    #[test]
    fn inline_math_requires_real_latex() {
        let h = render("price $5 and throw $$ at it");
        assert!(!h.contains("<math"));
    }

    #[test]
    fn paren_and_bracket_math() {
        let h = render(
            "line \\(e^{i\\pi}+1=0\\) ok\n\n\\[ \\int_0^1 x \\, dx \\]\n\n\\[\n\\sum_{i=1}^{n} \\frac{1}{i}\n\\]",
        );
        assert!(h.contains("<math"));
        assert!(h.contains("<msup>"));
        assert!(h.contains("<mfrac>"));
        assert!(h.contains("<mo>∫</mo>") || h.contains("&#x222B;") || h.contains("∫"));
        assert!(!h.contains("\\("));
        assert!(!h.contains("\\(e"));
    }

    #[test]
    fn unmatched_math_delims_stay_literal() {
        let h = render("a \\( b c");
        assert!(!h.contains("<math"));
        assert!(h.contains("a \\("));
    }

    #[test]
    fn supsub_and_mark() {
        let h = render("x^2^ and H~2~O and a ==mark== b");
        assert!(h.contains("<sup>2</sup>"));
        assert!(h.contains("<sub>2</sub>"));
        assert!(h.contains("<mark>mark</mark>"));
    }

    #[test]
    fn emoji_shortcode_expands() {
        let h = render("a :smile: and :unknown: b");
        assert!(h.contains("😄"));
        assert!(h.contains(":unknown:"));
    }

    #[test]
    fn definition_list() {
        let src = "Apple\n: a fruit\n: the company\n\nOrange\n: a citrus";
        let h = render(src);
        assert!(h.contains("<dl>"));
        assert!(h.contains("<dt>Apple</dt>"));
        assert!(h.contains("<dd>a fruit</dd>"));
        assert!(h.contains("<dd>the company</dd>"));
        assert!(h.contains("<dt>Orange</dt>"));
        assert!(h.contains("<dd>a citrus</dd>"));
    }

    #[test]
    fn non_deflist_with_colon_stays_paragraph() {
        let h = render("Time: 10:30 AM");
        assert!(h.contains("<p>Time: 10:30 AM</p>"));
        assert!(!h.contains("<dl>"));
    }

    #[test]
    fn alert_admonitions() {
        let h = render("> [!NOTE]\n> keep me posted\n\n> [!WARNING]\n> careful");
        assert!(h.contains("class=\"admonition note\""));
        assert!(h.contains("class=\"admonition warning\""));
        assert!(!h.contains("> [!NOTE]"));
        assert!(h.contains("careful"));
    }

    #[test]
    fn toc_marker_renders_navigation() {
        let src = "# One\n\n## Two\n\n[[TOC]]";
        let h = render(src);
        assert!(h.contains("<nav class=\"toc\">"));
        assert!(h.contains("href=\"#one\""));
        assert!(h.contains("href=\"#two\""));
        assert!(h.contains("<h2 id=\"two\">Two</h2>"));
    }

    #[test]
    fn duplicate_heading_slugs_dedupe() {
        let h = render("# Edit\n# Edit");
        assert!(h.contains("id=\"edit\""));
        assert!(h.contains("id=\"edit-2\""));
    }

    #[test]
    fn html_comments_dropped() {
        let h = render("before <!-- hidden --> after");
        assert!(!h.contains("<!-- hidden -->"));
        assert!(h.contains("before"));
        assert!(h.contains("after"));
    }

    #[test]
    fn math_renders_inside_raw_html_blocks() {
        // `$$…$$` is inline math now, so it sits flush inside the `<p>`;
        // `\[…\]` is the display form and gets wrapped in a `<div>`.
        let src = "<p>moment $$\\vec{A} = \\vec{R}_0$$ point</p>\n\n<p>\\[\n\\vec{A} = \\vec{R}_0\n\\]</p>\n\n<p>$$\\vec{A} = {\\vec{\\mathfrak{m}}\n\\times \\vec{R}_0 \\over R_0^3}.$$(1)</p>";
        let h = render(src);
        assert!(h.contains("<p>moment "));
        // inline `$$…$$` produces an inline `<svg>` with MathML inside
        // its `<desc>`, so it lands between the surrounding text.
        assert!(h.contains("<p>moment <svg"));
        assert!(h.contains("</svg> point</p>"));
        // display `\[…\]` gets the `<div class="math">` wrapper.
        assert!(h.contains("<p><div class=\"math\"><svg"));
        assert!(h.contains("</svg></div></p>"));
        // multi-line `$$…$$` inside `<p>` still renders as inline math
        // (the body collapses newlines into spaces in the SVG).
        assert!(h.contains("<p><svg"));
        assert!(h.contains("</svg>(1)</p>"));
        assert!(!h.contains("$$\\vec"));
    }

    #[test]
    fn math_delimiters_left_literal_when_malformed() {
        let h = render("<p>price $$5</p>");
        assert!(h.contains("price $$5"));
        assert!(!h.contains("<svg"));
        let h2 = render("<p>\\[unclosed</p>");
        assert!(h2.contains("\\[unclosed"));
    }
}
