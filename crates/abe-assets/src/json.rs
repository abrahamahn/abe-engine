//! Bounded JSON reader for offline content and versioned scene descriptions.
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}
impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        if let Self::Object(m) = self {
            m.get(key)
        } else {
            None
        }
    }
    pub fn array(&self) -> Result<&[Value], String> {
        if let Self::Array(a) = self {
            Ok(a)
        } else {
            Err("expected array".into())
        }
    }
    pub fn string(&self) -> Result<&str, String> {
        if let Self::String(s) = self {
            Ok(s)
        } else {
            Err("expected string".into())
        }
    }
    pub fn integer(&self) -> Result<usize, String> {
        if let Self::Number(n) = self {
            if *n >= 0. && *n <= 9_007_199_254_740_991. && *n < usize::MAX as f64 && n.fract() == 0.
            {
                return Ok(*n as usize);
            }
        }
        Err("expected unsigned integer".into())
    }
    pub fn float(&self) -> Result<f32, String> {
        if let Self::Number(n) = self {
            let n = *n as f32;
            if n.is_finite() {
                return Ok(n);
            }
        }
        Err("expected finite float".into())
    }
}
pub fn parse(input: &[u8]) -> Result<Value, String> {
    if input.len() > 32 * 1024 * 1024 {
        return Err("JSON exceeds 32 MiB".into());
    }
    let mut p = Parser {
        b: input,
        i: 0,
        values: 0,
    };
    let result = p.value(0)?;
    p.space();
    if p.i != input.len() {
        return Err(p.err("trailing data"));
    }
    Ok(result)
}
struct Parser<'a> {
    b: &'a [u8],
    i: usize,
    values: usize,
}
impl Parser<'_> {
    fn err(&self, msg: &str) -> String {
        format!("JSON byte {}: {msg}", self.i)
    }
    fn byte(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn eat(&mut self, b: u8) -> bool {
        if self.byte() == Some(b) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn space(&mut self) {
        while matches!(self.byte(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.i += 1;
        }
    }
    fn value(&mut self, depth: usize) -> Result<Value, String> {
        self.space();
        self.values += 1;
        if depth > 64 || self.values > 1_000_000 {
            return Err(self.err("structure limit"));
        }
        match self.byte() {
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b'[') => {
                self.i += 1;
                let mut out = vec![];
                self.space();
                if self.eat(b']') {
                    return Ok(Value::Array(out));
                }
                loop {
                    out.push(self.value(depth + 1)?);
                    self.space();
                    if self.eat(b']') {
                        return Ok(Value::Array(out));
                    }
                    if !self.eat(b',') {
                        return Err(self.err("expected array comma"));
                    }
                }
            }
            Some(b'{') => {
                self.i += 1;
                let mut out = BTreeMap::new();
                self.space();
                if self.eat(b'}') {
                    return Ok(Value::Object(out));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    self.space();
                    if !self.eat(b':') {
                        return Err(self.err("expected colon"));
                    }
                    if out.insert(key, self.value(depth + 1)?).is_some() {
                        return Err(self.err("duplicate key"));
                    }
                    self.space();
                    if self.eat(b'}') {
                        return Ok(Value::Object(out));
                    }
                    if !self.eat(b',') {
                        return Err(self.err("expected object comma"));
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => {
                for (s, v) in [
                    (b"null".as_slice(), Value::Null),
                    (b"true", Value::Bool(true)),
                    (b"false", Value::Bool(false)),
                ] {
                    if self.b[self.i..].starts_with(s) {
                        self.i += s.len();
                        return Ok(v);
                    }
                }
                Err(self.err("expected value"))
            }
        }
    }
    fn digits(&mut self) -> Result<(), String> {
        let start = self.i;
        while matches!(self.byte(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        if self.i == start {
            Err(self.err("expected digit"))
        } else {
            Ok(())
        }
    }
    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        self.eat(b'-');
        if !self.eat(b'0') {
            self.digits()?
        }
        if self.eat(b'.') {
            self.digits()?
        }
        if self.eat(b'e') || self.eat(b'E') {
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            self.digits()?
        }
        let n = std::str::from_utf8(&self.b[start..self.i])
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .ok_or_else(|| self.err("invalid number"))?;
        Ok(Value::Number(n))
    }
    fn hex(&mut self) -> Result<u32, String> {
        let e = self.i + 4;
        let raw = self
            .b
            .get(self.i..e)
            .ok_or_else(|| self.err("short unicode escape"))?;
        let n = raw
            .iter()
            .try_fold(0u32, |a, &v| (v as char).to_digit(16).map(|d| a * 16 + d))
            .ok_or_else(|| self.err("invalid unicode escape"))?;
        self.i = e;
        Ok(n)
    }
    fn string(&mut self) -> Result<String, String> {
        if !self.eat(b'"') {
            return Err(self.err("expected string"));
        }
        let mut out = String::new();
        loop {
            let start = self.i;
            while matches!(self.byte(),Some(c) if c>=32 && c!=b'"'&& c!=b'\\') {
                self.i += 1
            }
            out.push_str(
                std::str::from_utf8(&self.b[start..self.i])
                    .map_err(|_| self.err("invalid UTF-8"))?,
            );
            if self.eat(b'"') {
                return Ok(out);
            }
            if !self.eat(b'\\') {
                return Err(self.err("unterminated string or control character"));
            }
            let e = self.byte().ok_or_else(|| self.err("short escape"))?;
            self.i += 1;
            out.push(match e {
                b'"' => '"',
                b'\\' => '\\',
                b'/' => '/',
                b'b' => '\u{8}',
                b'f' => '\u{c}',
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'u' => {
                    let mut cp = self.hex()?;
                    if (0xd800..=0xdbff).contains(&cp) {
                        if !self.eat(b'\\') || !self.eat(b'u') {
                            return Err(self.err("missing low surrogate"));
                        }
                        let low = self.hex()?;
                        if !(0xdc00..=0xdfff).contains(&low) {
                            return Err(self.err("invalid low surrogate"));
                        }
                        cp = 0x10000 + ((cp - 0xd800) << 10) + low - 0xdc00;
                    }
                    char::from_u32(cp).ok_or_else(|| self.err("invalid unicode scalar"))?
                }
                _ => return Err(self.err("invalid string escape")),
            });
        }
    }
}
