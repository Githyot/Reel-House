//! Small JSON reader and writer. It has no dependencies so the core logic builds and tests anywhere.
use std::fmt::Write as _;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Json {
    #[default]
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

static NULL: Json = Json::Null;

impl Json {
    pub fn parse(src: &str) -> Result<Json, String> {
        let mut p = Parser { s: src.as_bytes(), i: 0 };
        p.ws();
        let v = p.value(0)?;
        p.ws();
        if p.i != p.s.len() {
            return Err(format!("unexpected data after the JSON value at byte {}", p.i));
        }
        Ok(v)
    }

    pub fn obj(pairs: Vec<(&str, Json)>) -> Json {
        Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    pub fn get(&self, key: &str) -> &Json {
        if let Json::Obj(v) = self {
            for (k, val) in v {
                if k == key {
                    return val;
                }
            }
        }
        &NULL
    }

    pub fn at(&self, i: usize) -> &Json {
        match self {
            Json::Arr(v) => v.get(i).unwrap_or(&NULL),
            _ => &NULL,
        }
    }

    /// Dotted path such as "details.files.0.name".
    pub fn path(&self, p: &str) -> &Json {
        let mut cur = self;
        for part in p.split('.') {
            cur = match (cur, part.parse::<usize>()) {
                (Json::Arr(_), Ok(i)) => cur.at(i),
                _ => cur.get(part),
            };
        }
        cur
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    pub fn as_str(&self) -> Option<&str> {
        if let Json::Str(s) = self { Some(s) } else { None }
    }

    /// Text of a string or number, otherwise an empty string.
    pub fn str(&self) -> String {
        match self {
            Json::Str(s) => s.clone(),
            Json::Num(n) => fmt_num(*n),
            Json::Bool(b) => b.to_string(),
            _ => String::new(),
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            Json::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        self.as_f64().map(|n| n as i64)
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.as_f64().filter(|n| *n >= 0.0).map(|n| n as u64)
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let Json::Bool(b) = self { Some(*b) } else { None }
    }

    pub fn items(&self) -> &[Json] {
        match self {
            Json::Arr(v) => v,
            _ => &[],
        }
    }

    pub fn entries(&self) -> &[(String, Json)] {
        match self {
            Json::Obj(v) => v,
            _ => &[],
        }
    }

    pub fn strings(&self) -> Vec<String> {
        self.items().iter().filter_map(|j| j.as_str().map(|s| s.to_string())).collect()
    }

    pub fn set(&mut self, key: &str, val: Json) {
        if let Json::Obj(v) = self {
            for (k, old) in v.iter_mut() {
                if k == key {
                    *old = val;
                    return;
                }
            }
            v.push((key.to_string(), val));
        }
    }

    pub fn dump(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, None, 0);
        out
    }

    pub fn pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, Some(2), 0);
        out
    }

    fn write(&self, out: &mut String, indent: Option<usize>, depth: usize) {
        let nl = |out: &mut String, d: usize| {
            if let Some(n) = indent {
                out.push('\n');
                for _ in 0..(n * d) {
                    out.push(' ');
                }
            }
        };
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Num(n) => out.push_str(&fmt_num(*n)),
            Json::Str(s) => write_str(out, s),
            Json::Arr(v) => {
                if v.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    nl(out, depth + 1);
                    x.write(out, indent, depth + 1);
                }
                nl(out, depth);
                out.push(']');
            }
            Json::Obj(v) => {
                if v.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                for (i, (k, x)) in v.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    nl(out, depth + 1);
                    write_str(out, k);
                    out.push(':');
                    if indent.is_some() {
                        out.push(' ');
                    }
                    x.write(out, indent, depth + 1);
                }
                nl(out, depth);
                out.push('}');
            }
        }
    }
}

pub fn fmt_num(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 9.0e15 {
        format!("{}", n as i64)
    } else if n.is_finite() {
        format!("{}", n)
    } else {
        "null".to_string()
    }
}

fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl From<&str> for Json {
    fn from(s: &str) -> Json {
        Json::Str(s.to_string())
    }
}
impl From<String> for Json {
    fn from(s: String) -> Json {
        Json::Str(s)
    }
}
impl From<bool> for Json {
    fn from(b: bool) -> Json {
        Json::Bool(b)
    }
}
impl From<f64> for Json {
    fn from(n: f64) -> Json {
        Json::Num(n)
    }
}
impl From<i64> for Json {
    fn from(n: i64) -> Json {
        Json::Num(n as f64)
    }
}
impl From<u32> for Json {
    fn from(n: u32) -> Json {
        Json::Num(n as f64)
    }
}
impl From<Vec<Json>> for Json {
    fn from(v: Vec<Json>) -> Json {
        Json::Arr(v)
    }
}
impl From<Vec<String>> for Json {
    fn from(v: Vec<String>) -> Json {
        Json::Arr(v.into_iter().map(Json::Str).collect())
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\n' | b'\r' | b'\t') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn lit(&mut self, word: &str, v: Json) -> Result<Json, String> {
        if self.s[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(format!("unexpected character at byte {}", self.i))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > 128 {
            return Err("JSON is nested too deeply".to_string());
        }
        match self.peek() {
            None => Err("unexpected end of JSON".to_string()),
            Some(b'{') => {
                self.i += 1;
                let mut out = Vec::new();
                self.ws();
                if self.peek() == Some(b'}') {
                    self.i += 1;
                    return Ok(Json::Obj(out));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.ws();
                    if self.peek() != Some(b':') {
                        return Err(format!("expected ':' at byte {}", self.i));
                    }
                    self.i += 1;
                    self.ws();
                    let v = self.value(depth + 1)?;
                    out.push((k, v));
                    self.ws();
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            break;
                        }
                        _ => return Err(format!("expected ',' or '}}' at byte {}", self.i)),
                    }
                }
                Ok(Json::Obj(out))
            }
            Some(b'[') => {
                self.i += 1;
                let mut out = Vec::new();
                self.ws();
                if self.peek() == Some(b']') {
                    self.i += 1;
                    return Ok(Json::Arr(out));
                }
                loop {
                    self.ws();
                    out.push(self.value(depth + 1)?);
                    self.ws();
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            break;
                        }
                        _ => return Err(format!("expected ',' or ']' at byte {}", self.i)),
                    }
                }
                Ok(Json::Arr(out))
            }
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.lit("true", Json::Bool(true)),
            Some(b'f') => self.lit("false", Json::Bool(false)),
            Some(b'n') => self.lit("null", Json::Null),
            Some(_) => self.number(),
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let st = self.i;
        while let Some(c) = self.peek() {
            if matches!(c, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') {
                self.i += 1;
            } else {
                break;
            }
        }
        if st == self.i {
            return Err(format!("unexpected character at byte {}", st));
        }
        let t = std::str::from_utf8(&self.s[st..self.i]).unwrap_or("");
        t.parse::<f64>().map(Json::Num).map_err(|_| format!("bad number at byte {}", st))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        if self.i + 4 > self.s.len() {
            return Err("bad \\u escape".to_string());
        }
        let t = std::str::from_utf8(&self.s[self.i..self.i + 4]).map_err(|_| "bad \\u escape".to_string())?;
        let v = u32::from_str_radix(t, 16).map_err(|_| "bad \\u escape".to_string())?;
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, String> {
        if self.peek() != Some(b'"') {
            return Err(format!("expected a string at byte {}", self.i));
        }
        self.i += 1;
        let mut buf: Vec<u8> = Vec::new();
        loop {
            let c = *self.s.get(self.i).ok_or_else(|| "unterminated string".to_string())?;
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = *self.s.get(self.i).ok_or_else(|| "bad escape".to_string())?;
                    self.i += 1;
                    match e {
                        b'n' => buf.push(b'\n'),
                        b't' => buf.push(b'\t'),
                        b'r' => buf.push(b'\r'),
                        b'b' => buf.push(8),
                        b'f' => buf.push(12),
                        b'/' => buf.push(b'/'),
                        b'\\' => buf.push(b'\\'),
                        b'"' => buf.push(b'"'),
                        b'u' => {
                            let mut cp = self.hex4()?;
                            if (0xD800..0xDC00).contains(&cp) && self.s[self.i..].starts_with(b"\\u") {
                                let save = self.i;
                                self.i += 2;
                                let lo = self.hex4()?;
                                if (0xDC00..0xE000).contains(&lo) {
                                    cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                } else {
                                    self.i = save;
                                }
                            }
                            let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
                            let mut tmp = [0u8; 4];
                            buf.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
                        }
                        _ => return Err("bad escape".to_string()),
                    }
                }
                _ => buf.push(c),
            }
        }
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_reads_nested_values() {
        let j = Json::parse(r#"{"a":[1,2,{"b":"x\né😀"}],"c":true,"d":null,"n":-1.5e1}"#).unwrap();
        assert_eq!(j.path("a.2.b").str(), "x\né😀");
        assert_eq!(j.get("c").as_bool(), Some(true));
        assert!(j.get("d").is_null());
        assert!(j.get("missing").is_null());
        assert_eq!(j.get("n").as_f64(), Some(-15.0));
        assert_eq!(j.get("a").items().len(), 3);
    }

    #[test]
    fn round_trips() {
        let src = r#"{"k":"q\"uote\\","list":[1,2.5,"z"],"o":{}}"#;
        let j = Json::parse(src).unwrap();
        assert_eq!(Json::parse(&j.dump()).unwrap(), j);
        assert_eq!(Json::parse(&j.pretty()).unwrap(), j);
        assert_eq!(Json::Num(3.0).dump(), "3");
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Json::parse("{").is_err());
        assert!(Json::parse("[1,]").is_err());
        assert!(Json::parse("{} x").is_err());
        assert!(Json::parse("").is_err());
    }

    #[test]
    fn set_replaces_or_adds() {
        let mut j = Json::obj(vec![("a", 1i64.into())]);
        j.set("a", 2i64.into());
        j.set("b", "x".into());
        assert_eq!(j.dump(), r#"{"a":2,"b":"x"}"#);
    }
}
