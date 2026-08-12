//! Arithmetic for TikZ coordinates and `plot` expressions.
//!
//! A small precedence-climbing evaluator over the TikZ math subset:
//!
//! ```text
//! expr   := term (('+' | '-') term)*
//! term   := factor (('*' | '/') factor)*
//! factor := ('-' | '+') factor | power
//! power  := atom ('^' factor)?              // right associative
//! atom   := number [unit] | '\'name | name | func '(' args ')' | '(' expr ')'
//!         | atom 'r'                        // radians, per TikZ
//! ```
//!
//! Two TikZ conventions are load-bearing:
//!
//! - **Trigonometry is in degrees.** `sin(30)` is 0.5.
//! - **`r` converts radians to degrees**, so `sin(\x r)` reads `\x` as
//!   radians — the form used by `\draw plot (\x,{sin(\x r)})`.
//!
//! Lengths carry an optional unit and evaluate to TikZ user units (1 = 1 cm),
//! so `(1,2)`, `(10mm,2)` and `(28.45pt,2)` all mean the same point.
//!
//! Unknown names evaluate to 0 rather than failing: a drawing with one typo
//! still renders, matching the module's overall tolerance.

use std::collections::HashMap;

/// Values bound to macro names (`\r`, `\x`, a `\foreach` variable).
#[derive(Clone, Default)]
pub(crate) struct Vars {
    map: HashMap<String, f64>,
}

impl Vars {
    pub(crate) fn new() -> Vars {
        Vars::default()
    }

    pub(crate) fn set(&mut self, name: &str, v: f64) {
        self.map.insert(name.trim_start_matches('\\').to_string(), v);
    }

    pub(crate) fn get(&self, name: &str) -> Option<f64> {
        self.map.get(name.trim_start_matches('\\')).copied()
    }
}

/// Evaluate an expression. `None` when nothing numeric could be parsed.
pub(crate) fn eval(src: &str, vars: &Vars) -> Option<f64> {
    eval_in(src, vars, super::EM_PER_CM)
}

/// Evaluate on a different `em` basis. `cm_per_em` is how many centimetres
/// of user space fit in one `em`, so `1em` and `1ex` round-trip exactly
/// under that basis. The `picture` backend passes its own 10 pt-text basis
/// ([`super::PIC_EM_PER_CM`]) where a TeX point is a tenth of an em.
pub(crate) fn eval_in(src: &str, vars: &Vars, cm_per_em: f64) -> Option<f64> {
    if !(cm_per_em.is_finite() && cm_per_em > 0.0) {
        return None;
    }
    let chars: Vec<char> = src.chars().collect();
    let mut p = Eval {
        c: &chars,
        i: 0,
        vars,
        cm_per_em,
    };
    let v = p.expr()?;
    p.ws();
    // Trailing junk is tolerated: `2cm and more` still yields 2.
    if v.is_finite() {
        Some(v)
    } else {
        None
    }
}

struct Eval<'a> {
    c: &'a [char],
    i: usize,
    vars: &'a Vars,
    cm_per_em: f64,
}

impl Eval<'_> {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: char) -> bool {
        self.ws();
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        loop {
            self.ws();
            match self.peek() {
                Some('+') => {
                    self.i += 1;
                    v += self.term()?;
                }
                // `--` is a path connector, never a subtraction.
                Some('-') if self.c.get(self.i + 1) != Some(&'-') => {
                    self.i += 1;
                    v -= self.term()?;
                }
                _ => return Some(v),
            }
        }
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.factor()?;
        loop {
            self.ws();
            match self.peek() {
                Some('*') => {
                    self.i += 1;
                    v *= self.factor()?;
                }
                Some('/') => {
                    self.i += 1;
                    let d = self.factor()?;
                    if d == 0.0 {
                        return None;
                    }
                    v /= d;
                }
                _ => return Some(v),
            }
        }
    }

    fn factor(&mut self) -> Option<f64> {
        self.ws();
        match self.peek() {
            Some('-') => {
                self.i += 1;
                Some(-self.factor()?)
            }
            Some('+') => {
                self.i += 1;
                self.factor()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.atom()?;
        self.ws();
        if self.peek() == Some('^') {
            self.i += 1;
            let e = self.factor()?;
            return Some(base.powf(e));
        }
        Some(base)
    }

    fn atom(&mut self) -> Option<f64> {
        self.ws();
        let v = match self.peek()? {
            '(' => {
                self.i += 1;
                let v = self.expr()?;
                self.eat(')');
                v
            }
            '{' => {
                self.i += 1;
                let v = self.expr()?;
                self.eat('}');
                v
            }
            '\\' => {
                self.i += 1;
                let name = self.name();
                self.vars.get(&name).unwrap_or(0.0)
            }
            c if c.is_ascii_digit() || c == '.' => self.number()?,
            c if c.is_alphabetic() => {
                let name = self.name();
                self.ws();
                if self.peek() == Some('(') {
                    self.i += 1;
                    let args = self.args();
                    call(&name, &args)?
                } else {
                    match name.as_str() {
                        "pi" => std::f64::consts::PI,
                        "e" => std::f64::consts::E,
                        _ => self.vars.get(&name).unwrap_or(0.0),
                    }
                }
            }
            _ => return None,
        };
        Some(self.suffix(v))
    }

    /// Unit and `r` suffixes. Lengths convert to TikZ user units (1 = 1 cm).
    fn suffix(&mut self, v: f64) -> f64 {
        let save = self.i;
        self.ws();
        if !matches!(self.peek(), Some(c) if c.is_alphabetic()) {
            self.i = save;
            return v;
        }
        let word = self.name();
        match word.as_str() {
            "cm" => v,
            "mm" => v / 10.0,
            "pt" => v / 28.4527,
            "bp" => v / 28.3465,
            "in" => v * 2.54,
            "em" => v / self.cm_per_em,
            "ex" => v * 0.45 / self.cm_per_em,
            // `1 r` is one radian written in degrees, so trig in degrees
            // sees the right angle.
            "r" => v.to_degrees(),
            _ => {
                self.i = save;
                v
            }
        }
    }

    fn name(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '@' {
                s.push(c);
                self.i += 1;
            } else {
                break;
            }
        }
        s
    }

    fn number(&mut self) -> Option<f64> {
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.i += 1;
        }
        if self.peek() == Some('.') {
            self.i += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        let s: String = self.c[start..self.i].iter().collect();
        s.parse().ok()
    }

    fn args(&mut self) -> Vec<f64> {
        let mut out = Vec::new();
        loop {
            self.ws();
            if self.peek() == Some(')') {
                self.i += 1;
                break;
            }
            match self.expr() {
                Some(v) => out.push(v),
                None => {
                    // Skip the unparseable argument and keep going.
                    while !matches!(self.peek(), None | Some(',') | Some(')')) {
                        self.i += 1;
                    }
                }
            }
            if !self.eat(',') && self.eat(')') {
                break;
            }
            if self.peek().is_none() {
                break;
            }
        }
        out
    }
}

/// Apply a TikZ math function. Angles are in degrees.
fn call(name: &str, a: &[f64]) -> Option<f64> {
    let x = a.first().copied().unwrap_or(0.0);
    let y = a.get(1).copied().unwrap_or(0.0);
    let v = match name {
        "sin" => x.to_radians().sin(),
        "cos" => x.to_radians().cos(),
        "tan" => x.to_radians().tan(),
        "cot" => 1.0 / x.to_radians().tan(),
        "sec" => 1.0 / x.to_radians().cos(),
        "csc" => 1.0 / x.to_radians().sin(),
        "asin" => x.asin().to_degrees(),
        "acos" => x.acos().to_degrees(),
        "atan" => x.atan().to_degrees(),
        "atan2" => x.atan2(y).to_degrees(),
        "sqrt" => x.sqrt(),
        "exp" => x.exp(),
        "ln" => x.ln(),
        "log10" | "log" => x.log10(),
        "abs" => x.abs(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "round" => x.round(),
        "min" => x.min(y),
        "max" => x.max(y),
        "mod" => x % y,
        "pow" => x.powf(y),
        "veclen" => (x * x + y * y).sqrt(),
        _ => return None,
    };
    if v.is_finite() {
        Some(v)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(src: &str) -> f64 {
        eval(src, &Vars::new()).expect("evaluates")
    }

    #[test]
    fn arithmetic_precedence_and_parens() {
        assert_eq!(e("1+2*3"), 7.0);
        assert_eq!(e("(1+2)*3"), 9.0);
        assert_eq!(e("-2 + 1"), -1.0);
        assert_eq!(e("2^3^2"), 512.0, "power is right associative");
    }

    #[test]
    fn trig_is_degrees_and_r_switches_to_radians() {
        assert!((e("sin(30)") - 0.5).abs() < 1e-9);
        assert!((e("cos(0)") - 1.0).abs() < 1e-9);
        // sin of one radian, the `plot (\x,{sin(\x r)})` form.
        assert!((e("sin(1 r)") - 1.0_f64.sin()).abs() < 1e-9);
    }

    #[test]
    fn lengths_normalise_to_centimetres() {
        assert!((e("10mm") - 1.0).abs() < 1e-9);
        assert!((e("28.4527pt") - 1.0).abs() < 1e-6);
        assert!((e("1in") - 2.54).abs() < 1e-9);
    }

    #[test]
    fn macros_resolve_and_unknown_names_are_zero() {
        let mut v = Vars::new();
        v.set("\\r", 1.8);
        assert!((eval("0.5*\\r", &v).unwrap() - 0.9).abs() < 1e-9);
        assert!((eval("-\\r", &v).unwrap() + 1.8).abs() < 1e-9);
        assert_eq!(eval("\\nope", &v).unwrap(), 0.0);
    }

    #[test]
    fn double_dash_is_not_subtraction() {
        // The path connector must not be eaten as `a - (-b)`.
        let mut p = Eval {
            c: &"1 -- 2".chars().collect::<Vec<_>>(),
            i: 0,
            vars: &Vars::new(),
            cm_per_em: super::super::EM_PER_CM,
        };
        assert_eq!(p.expr(), Some(1.0));
    }

    #[test]
    fn division_by_zero_fails_instead_of_producing_infinity() {
        assert!(eval("1/0", &Vars::new()).is_none());
    }
}
