//! Answers as JSON with their keys in the order they were put, so `ok` comes first and an
//! entity reads id, name, parent, components. Document data keeps its stored (sorted) order.
//! Numbers and strings are spelled as in canonical files (`abe_core::json`), so the same answer
//! is always the same bytes.
use abe_core::json::{Value, write_number, write_string};

#[derive(Clone, Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

/// An object under construction, keys in insertion order (a repeated key replaces the first).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Obj(Vec<(String, J)>);

impl Obj {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with(mut self, key: &str, value: impl Into<J>) -> Self {
        self.put(key, value);
        self
    }
    pub fn put(&mut self, key: &str, value: impl Into<J>) {
        let value = value.into();
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key.to_string(), value)),
        }
    }
    pub fn extend(&mut self, other: Obj) {
        for (k, v) in other.0 {
            self.put(&k, v);
        }
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Obj> for J {
    fn from(o: Obj) -> Self {
        J::Obj(o.0)
    }
}
impl From<&Value> for J {
    fn from(v: &Value) -> Self {
        match v {
            Value::Null => J::Null,
            Value::Bool(b) => J::Bool(*b),
            Value::Number(n) => J::Num(*n),
            Value::String(s) => J::Str(s.clone()),
            Value::Array(a) => J::Arr(a.iter().map(J::from).collect()),
            Value::Object(m) => J::Obj(m.iter().map(|(k, v)| (k.clone(), J::from(v))).collect()),
        }
    }
}
impl From<Value> for J {
    fn from(v: Value) -> Self {
        J::from(&v)
    }
}
impl From<bool> for J {
    fn from(b: bool) -> Self {
        J::Bool(b)
    }
}
impl From<&str> for J {
    fn from(s: &str) -> Self {
        J::Str(s.to_string())
    }
}
impl From<String> for J {
    fn from(s: String) -> Self {
        J::Str(s)
    }
}
impl From<&String> for J {
    fn from(s: &String) -> Self {
        J::Str(s.clone())
    }
}
impl From<f64> for J {
    fn from(n: f64) -> Self {
        J::Num(n)
    }
}
impl From<f32> for J {
    fn from(n: f32) -> Self {
        J::Num(f64::from(n))
    }
}
impl From<u32> for J {
    fn from(n: u32) -> Self {
        J::Num(f64::from(n))
    }
}
impl From<u64> for J {
    fn from(n: u64) -> Self {
        J::Num(n as f64)
    }
}
impl From<usize> for J {
    fn from(n: usize) -> Self {
        J::Num(n as f64)
    }
}
impl<const N: usize> From<[f32; N]> for J {
    fn from(v: [f32; N]) -> Self {
        J::Arr(v.iter().map(|x| J::from(*x)).collect())
    }
}
impl<T: Into<J>> From<Option<T>> for J {
    fn from(v: Option<T>) -> Self {
        v.map_or(J::Null, Into::into)
    }
}
impl<T: Into<J>> From<Vec<T>> for J {
    fn from(v: Vec<T>) -> Self {
        J::Arr(v.into_iter().map(Into::into).collect())
    }
}

/// `j` as one line (`pretty`: indented, arrays of numbers on one line as in canonical files).
pub fn write(j: &J, pretty: bool) -> String {
    let mut out = String::new();
    if pretty {
        indented(&mut out, j, 0);
    } else {
        compact(&mut out, j);
    }
    out
}

fn number(out: &mut String, n: f64) {
    // Every number here came from parsed JSON or finite arithmetic; never fail an answer.
    if write_number(out, n).is_err() {
        out.push_str("null");
    }
}

fn compact(out: &mut String, j: &J) {
    match j {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Num(n) => number(out, *n),
        J::Str(s) => write_string(out, s),
        J::Arr(items) => {
            out.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                compact(out, x);
            }
            out.push(']');
        }
        J::Obj(pairs) => {
            out.push('{');
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(out, k);
                out.push(':');
                compact(out, v);
            }
            out.push('}');
        }
    }
}

fn pad(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn indented(out: &mut String, j: &J, depth: usize) {
    let scalar = |x: &J| matches!(x, J::Null | J::Bool(_) | J::Num(_));
    match j {
        J::Arr(items) if !items.is_empty() && !items.iter().all(scalar) => {
            out.push_str("[\n");
            for (i, x) in items.iter().enumerate() {
                pad(out, depth + 1);
                indented(out, x, depth + 1);
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            pad(out, depth);
            out.push(']');
        }
        J::Arr(items) => {
            out.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                compact(out, x);
            }
            out.push(']');
        }
        J::Obj(pairs) if !pairs.is_empty() => {
            out.push_str("{\n");
            for (i, (k, v)) in pairs.iter().enumerate() {
                pad(out, depth + 1);
                write_string(out, k);
                out.push_str(": ");
                indented(out, v, depth + 1);
                out.push_str(if i + 1 < pairs.len() { ",\n" } else { "\n" });
            }
            pad(out, depth);
            out.push('}');
        }
        other => compact(out, other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_keep_their_order_and_numbers_their_spelling() {
        let j: J = Obj::new()
            .with("ok", true)
            .with("b", vec![1.5f64, 0.1])
            .with("a", Obj::new().with("x", "y\n"))
            .with("n", Option::<u32>::None)
            .with("ok", false)
            .into();
        assert_eq!(
            write(&j, false),
            r#"{"ok":false,"b":[1.5,0.1],"a":{"x":"y\n"},"n":null}"#
        );
        assert_eq!(
            write(&j, true),
            "{\n  \"ok\": false,\n  \"b\": [1.5, 0.1],\n  \"a\": {\n    \"x\": \"y\\n\"\n  },\n  \"n\": null\n}"
        );
        assert_eq!(write(&J::Num(f64::NAN), false), "null");
        assert_eq!(write(&J::from(f64::from(0.1f32)), false), "0.1");
    }
}
