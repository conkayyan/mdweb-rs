use std::collections::BTreeMap;

/// A tiny dynamic value used for both parsed config/frontmatter and template
/// contexts. Deliberately std-only.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
    Null,
    Arr(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

impl Default for Value {
    fn default() -> Self {
        Value::Null
    }
}

impl Value {
    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(s.into())
    }

    pub fn int(i: i64) -> Value {
        Value::Int(i)
    }

    pub fn arr(v: Vec<Value>) -> Value {
        Value::Arr(v)
    }

    pub fn map() -> Value {
        Value::Map(BTreeMap::new())
    }

    pub fn empty_map(m: BTreeMap<String, Value>) -> Value {
        Value::Map(m)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Truthiness used by `{% if %}`.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Str(s) => !s.is_empty(),
            Value::Arr(a) => !a.is_empty(),
            Value::Map(m) => !m.is_empty(),
        }
    }

    /// Resolve a dotted path such as "a.b.c" or "a". Keys may be quoted with
    /// double quotes inside the path, e.g. page."og:title" — not needed here.
    pub fn path(&self, expr: &str) -> Option<&Value> {
        let parts: Vec<&str> = expr.split('.').collect();
        let mut cur = self;
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() && i + 1 < parts.len() {
                continue;
            }
            match cur {
                Value::Map(m) => match m.get(*part) {
                    Some(v) => cur = v,
                    None => return None,
                },
                _ => return None,
            }
        }
        Some(cur)
    }

    /// Insert into a nested map given a dotted path.
    pub fn insert_path(&mut self, expr: &str, value: Value) {
        let parts: Vec<&str> = expr.split('.').collect();
        if parts.is_empty() {
            return;
        }
        let mut cur = match self {
            Value::Map(m) => m,
            _ => {
                *self = Value::map();
                self.as_map_mut()
            }
        };
        for (i, part) in parts.iter().enumerate() {
            if i + 1 == parts.len() {
                cur.insert((*part).to_string(), value);
                return;
            }
            let entry = cur.entry((*part).to_string()).or_insert_with(Value::map);
            match entry {
                Value::Map(m) => cur = m,
                _ => {
                    *entry = Value::map();
                    cur = entry.as_map_mut();
                }
            }
        }
    }

    fn as_map_mut(&mut self) -> &mut BTreeMap<String, Value> {
        match self {
            Value::Map(m) => m,
            _ => unreachable!(),
        }
    }

    /// Render a scalar to a string for output.
    pub fn render(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            Value::Arr(a) => {
                let items: Vec<String> = a.iter().map(|v| v.render()).collect();
                items.join(", ")
            }
            Value::Map(_) => "[object]".to_string(),
        }
    }
}
