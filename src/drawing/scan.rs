//! Shared character scanner for the three drawing front ends.
//!
//! Just enough TeX lexing to walk `\command[opts](coord){group}` sequences:
//! balanced delimiters, `%` comments, and non-destructive lookahead. Nothing
//! here interprets meaning — that is each front end's job.

pub(crate) struct Scanner {
    c: Vec<char>,
    i: usize,
}

impl Scanner {
    pub(crate) fn new(src: &str) -> Scanner {
        Scanner {
            c: src.chars().collect(),
            i: 0,
        }
    }

    pub(crate) fn eof(&mut self) -> bool {
        self.ws();
        self.i >= self.c.len()
    }

    pub(crate) fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    pub(crate) fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.i += 1;
        }
        c
    }

    /// Current byte index — used by callers that need to skip past a
    /// balanced group the scanner already consumed.
    pub(crate) fn pos(&self) -> usize {
        self.i
    }

    /// Advance `n` characters unconditionally. Convention: `starts_with` has
    /// already confirmed the run can be skipped.
    pub(crate) fn advance(&mut self, n: usize) {
        self.i += n;
    }

    /// Rewind the scanner — needed when a tentative parse fails and the
    /// caller wants to fall back to a different rule.
    pub(crate) fn set_pos(&mut self, i: usize) {
        self.i = i;
    }

    /// Skip whitespace and `%` comments.
    pub(crate) fn ws(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.i += 1;
                }
                Some('%') => {
                    while !matches!(self.peek(), None | Some('\n')) {
                        self.i += 1;
                    }
                }
                _ => return,
            }
        }
    }

    pub(crate) fn starts_with(&self, s: &str) -> bool {
        let mut i = self.i;
        for ch in s.chars() {
            if self.c.get(i) != Some(&ch) {
                return false;
            }
            i += 1;
        }
        true
    }

    pub(crate) fn eat(&mut self, c: char) -> bool {
        self.ws();
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    pub(crate) fn eat_str(&mut self, s: &str) -> bool {
        self.ws();
        if self.starts_with(s) {
            self.i += s.chars().count();
            true
        } else {
            false
        }
    }

    /// Read `\name` (letters), or a single-character control sequence such as
    /// `\\`. The leading backslash must already be the next character.
    pub(crate) fn command(&mut self) -> Option<String> {
        self.ws();
        if self.peek() != Some('\\') {
            return None;
        }
        self.i += 1;
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphabetic() || c == '@' {
                name.push(c);
                self.i += 1;
            } else {
                break;
            }
        }
        if name.is_empty() {
            // `\\`, `\,`, `\;` …
            name.push(self.bump()?);
        }
        Some(name)
    }

    /// A word of letters, e.g. a path operation like `rectangle`.
    pub(crate) fn word(&mut self) -> String {
        self.ws();
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphabetic() {
                s.push(c);
                self.i += 1;
            } else {
                break;
            }
        }
        s
    }

    pub(crate) fn group(&mut self) -> Option<String> {
        self.balanced('{', '}')
    }

    pub(crate) fn bracket(&mut self) -> Option<String> {
        self.balanced('[', ']')
    }

    pub(crate) fn paren(&mut self) -> Option<String> {
        self.balanced('(', ')')
    }

    /// Contents of a balanced `open … close` run, delimiters stripped.
    /// Returns `None` (without consuming) when `open` is not next.
    pub(crate) fn balanced(&mut self, open: char, close: char) -> Option<String> {
        self.ws();
        if self.peek() != Some(open) {
            return None;
        }
        self.i += 1;
        let mut depth = 1usize;
        let mut out = String::new();
        while let Some(c) = self.bump() {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(out);
                }
            }
            out.push(c);
        }
        // Unterminated: hand back what we have rather than losing the source.
        Some(out)
    }

    /// Consume up to and including the next `;` (a TikZ statement end).
    /// Also stops at a fresh `\command` so a TeX preamble like `\small`
    /// without a trailing `;` doesn't swallow the next statement.
    pub(crate) fn skip_statement(&mut self) {
        while let Some(c) = self.bump() {
            if c == ';' {
                return;
            }
            if c == '\\' && matches!(self.peek(), Some(ch) if ch.is_alphabetic()) {
                self.i -= 1;
                return;
            }
        }
    }
}

/// Split on top-level commas, ignoring commas nested in `{}`, `[]` or `()`.
pub(crate) fn split_top(src: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for c in src.chars() {
        match c {
            '{' | '[' | '(' => {
                depth += 1;
                cur.push(c);
            }
            '}' | ']' | ')' => {
                depth -= 1;
                cur.push(c);
            }
            c if c == sep && depth <= 0 => {
                out.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_commands_and_balanced_groups() {
        let mut s = Scanner::new("\\put (1,2){\\line(1,0){50}} % tail\n\\end");
        assert_eq!(s.command().as_deref(), Some("put"));
        assert_eq!(s.paren().as_deref(), Some("1,2"));
        assert_eq!(s.group().as_deref(), Some("\\line(1,0){50}"));
        assert_eq!(s.command().as_deref(), Some("end"), "comment is skipped");
        assert!(s.eof());
    }

    #[test]
    fn double_backslash_is_a_command() {
        let mut s = Scanner::new("\\\\");
        assert_eq!(s.command().as_deref(), Some("\\"));
    }

    #[test]
    fn splits_only_at_top_level() {
        assert_eq!(
            split_top("a, b={1,2}, c[3,4]", ','),
            vec!["a", "b={1,2}", "c[3,4]"]
        );
    }

    #[test]
    fn unterminated_group_returns_the_remainder() {
        let mut s = Scanner::new("{abc");
        assert_eq!(s.group().as_deref(), Some("abc"));
    }
}
