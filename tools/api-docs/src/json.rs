//! A small JSON parser for the rustdoc JSON output. It keeps numbers as
//! their source text and objects as sorted maps, which is all the
//! renderer needs.

use std::collections::BTreeMap;
use std::fmt;

/// A parsed JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

/// The shared `null` that lookups of a missing key return.
static NULL: Json = Json::Null;

impl Json {
    /// The value of `key` in an object, or `null`.
    pub fn get(&self, key: &str) -> &Json {
        match self {
            Json::Object(map) => map.get(key).unwrap_or(&NULL),
            _ => &NULL,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> bool {
        matches!(self, Json::Bool(true))
    }

    /// The value as an unsigned integer, when it is one.
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Json::Number(text) => text.parse().ok(),
            _ => None,
        }
    }

    /// The elements of an array; empty for any other value.
    pub fn items(&self) -> &[Json] {
        match self {
            Json::Array(items) => items,
            _ => &[],
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Json::Object(map) => Some(map),
            _ => None,
        }
    }

    /// For an enum encoded as `{"variant": value}` or `"variant"`: the
    /// variant name and its value (`null` for a bare string).
    pub fn variant(&self) -> Option<(&str, &Json)> {
        match self {
            Json::String(name) => Some((name, &NULL)),
            Json::Object(map) if map.len() == 1 => {
                map.iter().next().map(|(key, value)| (key.as_str(), value))
            }
            _ => None,
        }
    }
}

/// A syntax error with the byte offset where it was found.
#[derive(Debug, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub message: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid json at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parses one JSON document.
pub fn parse(text: &str) -> Result<Json, ParseError> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
        depth: 0,
    };
    let value = parser.value()?;
    parser.skip_space();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("trailing characters"));
    }
    Ok(value)
}

/// Rustdoc nests types deeply, but never this deeply.
const MAX_DEPTH: usize = 512;

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
}

impl Parser<'_> {
    fn error(&self, message: &'static str) -> ParseError {
        ParseError {
            offset: self.pos,
            message,
        }
    }

    fn skip_space(&mut self) {
        while let Some(b' ' | b'\n' | b'\r' | b'\t') = self.bytes.get(self.pos) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8, message: &'static str) -> Result<(), ParseError> {
        if self.bytes.get(self.pos) == Some(&byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn literal(&mut self, word: &[u8], value: Json) -> Result<Json, ParseError> {
        if self.bytes[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("unknown literal"))
        }
    }

    fn value(&mut self) -> Result<Json, ParseError> {
        self.skip_space();
        match self.bytes.get(self.pos) {
            None => Err(self.error("unexpected end")),
            Some(b'n') => self.literal(b"null", Json::Null),
            Some(b't') => self.literal(b"true", Json::Bool(true)),
            Some(b'f') => self.literal(b"false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::String),
            Some(b'[') => self.nested(Self::array),
            Some(b'{') => self.nested(Self::object),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("unexpected character")),
        }
    }

    fn nested(
        &mut self,
        parse: fn(&mut Self) -> Result<Json, ParseError>,
    ) -> Result<Json, ParseError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("nesting too deep"));
        }
        let value = parse(self);
        self.depth -= 1;
        value
    }

    fn array(&mut self) -> Result<Json, ParseError> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_space();
        if self.bytes.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_space();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn object(&mut self) -> Result<Json, ParseError> {
        self.pos += 1;
        let mut map = BTreeMap::new();
        self.skip_space();
        if self.bytes.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(Json::Object(map));
        }
        loop {
            self.skip_space();
            if self.bytes.get(self.pos) != Some(&b'"') {
                return Err(self.error("expected a key"));
            }
            let key = self.string()?;
            self.skip_space();
            self.expect(b':', "expected ':'")?;
            let value = self.value()?;
            if map.insert(key, value).is_some() {
                return Err(self.error("duplicate key"));
            }
            self.skip_space();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(map));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn number(&mut self) -> Result<Json, ParseError> {
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        let digits = self.digits();
        if digits == 0 {
            return Err(self.error("expected a digit"));
        }
        if self.bytes.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            if self.digits() == 0 {
                return Err(self.error("expected a digit"));
            }
        }
        if let Some(b'e' | b'E') = self.bytes.get(self.pos) {
            self.pos += 1;
            if let Some(b'+' | b'-') = self.bytes.get(self.pos) {
                self.pos += 1;
            }
            if self.digits() == 0 {
                return Err(self.error("expected a digit"));
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.error("invalid number"))?;
        Ok(Json::Number(text.to_owned()))
    }

    fn digits(&mut self) -> usize {
        let start = self.pos;
        while let Some(b'0'..=b'9') = self.bytes.get(self.pos) {
            self.pos += 1;
        }
        self.pos - start
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            let Some(&byte) = self.bytes.get(self.pos) else {
                return Err(self.error("unterminated string"));
            };
            self.pos += 1;
            match byte {
                b'"' => break,
                b'\\' => self.escape(&mut out)?,
                0..=0x1f => return Err(self.error("control character in string")),
                _ => out.push(byte),
            }
        }
        String::from_utf8(out).map_err(|_| self.error("invalid utf-8 in string"))
    }

    fn escape(&mut self, out: &mut Vec<u8>) -> Result<(), ParseError> {
        let Some(&byte) = self.bytes.get(self.pos) else {
            return Err(self.error("unterminated escape"));
        };
        self.pos += 1;
        let simple = match byte {
            b'"' => b'"',
            b'\\' => b'\\',
            b'/' => b'/',
            b'b' => 0x08,
            b'f' => 0x0c,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'u' => {
                let ch = self.unicode_escape()?;
                let mut buf = [0; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                return Ok(());
            }
            _ => return Err(self.error("unknown escape")),
        };
        out.push(simple);
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32, ParseError> {
        let digits = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| self.error("short escape"))?;
        let text = std::str::from_utf8(digits).map_err(|_| self.error("invalid escape"))?;
        let value = u32::from_str_radix(text, 16).map_err(|_| self.error("invalid escape"))?;
        self.pos += 4;
        Ok(value)
    }

    fn unicode_escape(&mut self) -> Result<char, ParseError> {
        let first = self.hex4()?;
        let code = if (0xd800..0xdc00).contains(&first) {
            if !self.bytes[self.pos..].starts_with(b"\\u") {
                return Err(self.error("lone surrogate"));
            }
            self.pos += 2;
            let second = self.hex4()?;
            if !(0xdc00..0xe000).contains(&second) {
                return Err(self.error("lone surrogate"));
            }
            0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
        } else {
            first
        };
        char::from_u32(code).ok_or_else(|| self.error("invalid code point"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_values() {
        let value =
            parse(r#" {"a": [1, -2.5e3, true, null], "b": {"c": "d\n\u00e9\ud83d\ude00"}} "#)
                .expect("valid json");
        assert_eq!(value.get("a").items().len(), 4);
        assert_eq!(value.get("a").items()[0].as_u32(), Some(1));
        assert_eq!(value.get("a").items()[1], Json::Number("-2.5e3".into()));
        assert_eq!(value.get("b").get("c").as_str(), Some("d\n\u{e9}\u{1f600}"));
        assert!(value.get("missing").is_null());
    }

    #[test]
    fn variant_reads_both_enum_encodings() {
        let value = parse(r#"["infer", {"generic": "T"}]"#).expect("valid json");
        assert_eq!(value.items()[0].variant().map(|v| v.0), Some("infer"));
        let (name, inner) = value.items()[1].variant().expect("one key");
        assert_eq!((name, inner.as_str()), ("generic", Some("T")));
    }

    #[test]
    fn rejects_malformed_input() {
        for text in [
            "",
            "{",
            "[1,]",
            "{\"a\" 1}",
            "\"\\x\"",
            "01x",
            "[1] 2",
            "{\"a\":1,\"a\":2}",
        ] {
            assert!(parse(text).is_err(), "{text:?} must fail");
        }
    }
}
