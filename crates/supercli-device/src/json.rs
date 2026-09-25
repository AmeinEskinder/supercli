//! Minimal JSON value + parser (std only).
//!
//! Supports what `simctl list … --json` and `baguette list --json` emit:
//! objects, arrays, strings (with escapes), numbers, booleans, null,
//! arbitrary nesting. Moved out of `simctl.rs` so `baguette.rs` can share
//! it. `pub(crate)` — this crate has zero mandatory dependencies.
//!
//! Compiled only with the `device` cargo feature.

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub(crate) fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Parser {
            bytes: s.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, b: u8, what: &str) -> Result<(), String> {
        self.ws();
        match self.bump() {
            Some(x) if x == b => Ok(()),
            other => Err(format!(
                "expected {what}, found {:?} at byte {}",
                other.map(char::from),
                self.pos
            )),
        }
    }

    fn parse_value(&mut self) -> Result<Json, String> {
        self.ws();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => Ok(Json::Str(self.parse_string()?)),
            Some(b't') => self.parse_lit("true", Json::Bool(true)),
            Some(b'f') => self.parse_lit("false", Json::Bool(false)),
            Some(b'n') => self.parse_lit("null", Json::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.parse_number(),
            other => Err(format!(
                "unexpected {:?} at byte {}",
                other.map(char::from),
                self.pos
            )),
        }
    }

    fn parse_lit(&mut self, lit: &str, v: Json) -> Result<Json, String> {
        for &b in lit.as_bytes() {
            match self.bump() {
                Some(x) if x == b => {}
                _ => return Err(format!("invalid literal at byte {}", self.pos)),
            }
        }
        Ok(v)
    }

    fn parse_number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let s = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|e| format!("bad number encoding: {e}"))?;
        s.parse::<f64>()
            .map(Json::Num)
            .map_err(|e| format!("bad number {s:?}: {e}"))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"', "'\"'")?;
        let mut out = String::new();
        loop {
            // Scan for the next `"` or `\`. Both are ASCII and can never
            // appear inside a multi-byte UTF-8 sequence, so the chunk before
            // the match is always a valid str boundary.
            let rest = &self.bytes[self.pos..];
            let mut i = 0;
            while i < rest.len() && rest[i] != b'"' && rest[i] != b'\\' {
                i += 1;
            }
            let chunk =
                std::str::from_utf8(&rest[..i]).map_err(|e| format!("bad utf-8 in string: {e}"))?;
            out.push_str(chunk);
            self.pos += i;
            match self.bump() {
                None => return Err("unterminated string".to_string()),
                Some(b'"') => return Ok(out),
                Some(b'\\') => match self.bump() {
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(b'b') => out.push('\u{8}'),
                    Some(b'f') => out.push('\u{C}'),
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'u') => {
                        let mut hex = [0u8; 4];
                        for h in &mut hex {
                            *h = self
                                .bump()
                                .ok_or_else(|| "truncated \\u escape".to_string())?;
                        }
                        let s = std::str::from_utf8(&hex).map_err(|_| "bad \\u hex".to_string())?;
                        let cp =
                            u32::from_str_radix(s, 16).map_err(|_| "bad \\u hex".to_string())?;
                        out.push(
                            char::from_u32(cp)
                                .ok_or_else(|| "invalid unicode scalar".to_string())?,
                        );
                    }
                    Some(other) => {
                        return Err(format!("bad escape \\{}", other as char));
                    }
                    None => return Err("truncated escape".to_string()),
                },
                Some(_) => unreachable!("scan only stops at quote/backslash/end"),
            }
        }
    }

    fn parse_array(&mut self) -> Result<Json, String> {
        self.expect(b'[', "'['")?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.ws();
            match self.bump() {
                Some(b',') => {}
                Some(b']') => return Ok(Json::Arr(items)),
                other => {
                    return Err(format!(
                        "expected ',' or ']', found {:?}",
                        other.map(char::from)
                    ))
                }
            }
        }
    }

    fn parse_object(&mut self) -> Result<Json, String> {
        self.expect(b'{', "'{'")?;
        let mut pairs = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Obj(pairs));
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return Err(format!("expected string key at byte {}", self.pos));
            }
            let key = self.parse_string()?;
            self.ws();
            self.expect(b':', "':'")?;
            let value = self.parse_value()?;
            pairs.push((key, value));
            self.ws();
            match self.bump() {
                Some(b',') => {}
                Some(b'}') => return Ok(Json::Obj(pairs)),
                other => {
                    return Err(format!(
                        "expected ',' or '}}', found {:?}",
                        other.map(char::from)
                    ))
                }
            }
        }
    }
}

/// Parse one JSON document; errors if trailing garbage follows the value.
pub(crate) fn parse_json(s: &str) -> Result<Json, String> {
    let mut p = Parser::new(s);
    let v = p.parse_value()?;
    p.ws();
    if p.peek().is_some() {
        return Err(format!("trailing data at byte {}", p.pos));
    }
    Ok(v)
}

#[cfg(all(test, feature = "device"))]
mod tests {
    use super::*;

    #[test]
    fn json_parser_handles_escapes_and_nesting() {
        let v = parse_json(r#"{"a": [1, -2.5e3, true, false, null, "x\ny\"z\u0041"], "b": {}}"#)
            .expect("parses");
        let arr = v.get("a").and_then(Json::as_arr).expect("a is array");
        assert_eq!(arr.len(), 6);
        assert_eq!(arr[2], Json::Bool(true));
        assert_eq!(arr[4], Json::Null);
        assert_eq!(arr[5], Json::Str("x\ny\"zA".to_string()));
        assert_eq!(v.get("b"), Some(&Json::Obj(vec![])));
    }

    #[test]
    fn json_parser_rejects_trailing_garbage() {
        assert!(parse_json("{} trailing").is_err());
        assert!(parse_json("{").is_err());
        assert!(parse_json("[1,]").is_err());
    }
}
