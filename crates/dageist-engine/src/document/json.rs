//! JSON as the document reads and writes it, by RFC 8259.
//!
//! The reader keeps the order of an object's keys, refuses a repeated key and
//! reads a number from its exact text, so a float comes back as the `f64` it
//! was written from. `serde_json` cannot be asked for that here: its exact
//! float reading is a feature that would also change how Polars is built.

use std::collections::HashSet;
use std::fmt;

use super::MAX_NESTING;

/// One JSON number, as its text says it is.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Number {
    /// A whole number written without fraction or exponent, that an `i64` holds.
    Integer(i64),
    /// A number written with a fraction or an exponent. It is finite.
    Float(f64),
    /// A whole number no `i64` holds, as it was written.
    Large(String),
}

/// One JSON value.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Json {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Json>),
    /// Entries in the order written; no key is repeated.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// What the value is, as an error says it: `an object`, `a string`.
    pub(super) const fn kind(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "true or false",
            Self::Number(_) => "a number",
            Self::String(_) => "a string",
            Self::Array(_) => "an array",
            Self::Object(_) => "an object",
        }
    }

    /// A whole number.
    pub(super) const fn integer(value: i64) -> Self {
        Self::Number(Number::Integer(value))
    }

    /// A number written with a fraction or an exponent; `None` unless finite.
    pub(super) fn float(value: f64) -> Option<Self> {
        value
            .is_finite()
            .then_some(Self::Number(Number::Float(value)))
    }

    /// Reads `text` as one JSON value.
    pub(super) fn parse(text: &str) -> Result<Self, SyntaxError> {
        let mut reader = Reader { text, at: 0 };
        reader.skip_space();
        let value = reader.value(0)?;
        reader.skip_space();
        match reader.peek() {
            None => Ok(value),
            Some(found) => Err(reader.fail(SyntaxProblem::TrailingText(found))),
        }
    }

    /// The value as compact JSON text, which [`Json::parse`] reads back as
    /// the same value.
    pub(super) fn to_text(&self) -> String {
        let mut text = String::new();
        self.write_to(&mut text);
        text
    }

    fn write_to(&self, text: &mut String) {
        use std::fmt::Write as _;
        // Writing to a `String` cannot fail, so the `write!` results are dropped.
        match self {
            Self::Null => text.push_str("null"),
            Self::Bool(true) => text.push_str("true"),
            Self::Bool(false) => text.push_str("false"),
            Self::Number(Number::Integer(value)) => {
                let _ = write!(text, "{value}");
            }
            // The shortest digits that read back as the same `f64`; always with a
            // fraction or an exponent, and finite, so never `inf` or `NaN`.
            Self::Number(Number::Float(value)) => {
                let _ = write!(text, "{value:?}");
            }
            Self::Number(Number::Large(written)) => text.push_str(written),
            Self::String(value) => write_string(value, text),
            Self::Array(items) => {
                text.push('[');
                for (at, item) in items.iter().enumerate() {
                    if at > 0 {
                        text.push(',');
                    }
                    item.write_to(text);
                }
                text.push(']');
            }
            Self::Object(entries) => {
                text.push('{');
                for (at, (key, value)) in entries.iter().enumerate() {
                    if at > 0 {
                        text.push(',');
                    }
                    write_string(key, text);
                    text.push(':');
                    value.write_to(text);
                }
                text.push('}');
            }
        }
    }
}

fn write_string(value: &str, text: &mut String) {
    use std::fmt::Write as _;
    text.push('"');
    for character in value.chars() {
        match character {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            '\u{8}' => text.push_str("\\b"),
            '\u{c}' => text.push_str("\\f"),
            control if control < ' ' => {
                let _ = write!(text, "\\u{:04x}", u32::from(control));
            }
            other => text.push(other),
        }
    }
    text.push('"');
}

/// A number as the document reads it from its text: a whole number an `i64`
/// holds, a number written with a fraction or an exponent, or a whole number no
/// `i64` holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum WrittenNumber {
    /// A whole number written without fraction or exponent, that an `i64` holds.
    Integer(i64),
    /// A number written with a fraction or an exponent. It is finite.
    Float(f64),
    /// A whole number no `i64` holds; its nearest `f64`.
    Large(f64),
}

/// Why text is not a number as the document writes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotWritten {
    /// A whole number written with a leading zero, such as `01`.
    LeadingZero,
    /// A number too large for an `f64`.
    OutOfRange,
    /// Anything else: text that is not a number, or more than a number.
    NotANumber,
}

/// Reads `text` as the one number it is, by the grammar the document's JSON
/// reader holds a number to, with white space allowed around it as JSON allows.
pub(crate) fn read_number(text: &str) -> Result<WrittenNumber, NotWritten> {
    let mut reader = Reader { text, at: 0 };
    reader.skip_space();
    if !matches!(reader.peek_byte(), Some(b'-' | b'0'..=b'9')) {
        return Err(NotWritten::NotANumber);
    }
    let number = reader.number().map_err(|error| match error.problem {
        SyntaxProblem::LeadingZero => NotWritten::LeadingZero,
        SyntaxProblem::NumberOutOfRange => NotWritten::OutOfRange,
        _ => NotWritten::NotANumber,
    })?;
    reader.skip_space();
    if reader.peek().is_some() {
        return Err(NotWritten::NotANumber);
    }
    match number {
        Number::Integer(value) => Ok(WrittenNumber::Integer(value)),
        Number::Float(value) => Ok(WrittenNumber::Float(value)),
        Number::Large(written) => written
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(WrittenNumber::Large)
            .ok_or(NotWritten::OutOfRange),
    }
}

/// What is wrong with text that is not JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxProblem {
    /// The text ends where more is needed.
    EndOfText,
    /// A character that cannot stand here.
    Unexpected(char),
    /// A value is complete, and more text follows it.
    TrailingText(char),
    /// A control character, `U+0000` to `U+001F`, inside a string; JSON
    /// writes it as an escape.
    ControlCharacter(char),
    /// A backslash followed by a character that makes no escape.
    InvalidEscape(char),
    /// `\u` not followed by four hexadecimal digits.
    InvalidUnicodeEscape,
    /// A `\u` escape of half a surrogate pair, with no other half beside it.
    LoneSurrogate,
    /// A whole number written with a leading zero, such as `01`.
    LeadingZero,
    /// A number too large for an `f64`, which has no infinity in JSON.
    NumberOutOfRange,
    /// An object that has this key twice.
    DuplicateKey(String),
    /// Arrays and objects open inside each other more than
    /// [`MAX_NESTING`](crate::document::MAX_NESTING) deep.
    TooDeep,
}

impl fmt::Display for SyntaxProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndOfText => formatter.write_str("the text ends too soon"),
            Self::Unexpected(found) => write!(formatter, "unexpected {found:?}"),
            Self::TrailingText(found) => {
                write!(formatter, "{found:?} follows the end of the JSON value")
            }
            Self::ControlCharacter(found) => write!(
                formatter,
                "the control character {found:?} must be written as an escape in a string"
            ),
            Self::InvalidEscape(found) => write!(formatter, "\\{found} is not an escape"),
            Self::InvalidUnicodeEscape => {
                formatter.write_str("\\u must be followed by four hexadecimal digits")
            }
            Self::LoneSurrogate => {
                formatter.write_str("a \\u escape of half a surrogate pair has no other half")
            }
            Self::LeadingZero => formatter.write_str("a number has no leading zero"),
            Self::NumberOutOfRange => formatter.write_str("the number is too large"),
            Self::DuplicateKey(key) => write!(formatter, "the key {key:?} is used twice"),
            Self::TooDeep => write!(
                formatter,
                "arrays and objects are nested deeper than {MAX_NESTING} levels"
            ),
        }
    }
}

/// Text that is not JSON, and where it goes wrong.
///
/// The place is the first character that is wrong, or the end of the text;
/// for a repeated key, the key's opening quote; for a number out of range or
/// with a leading zero, the number's first character. The line and the column
/// count from 1: a line ends at `\n`, and the column counts characters, not
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    line: usize,
    column: usize,
    problem: SyntaxProblem,
}

impl SyntaxError {
    /// The line of the first character that is wrong, from 1.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// The column of that character on its line, from 1.
    #[must_use]
    pub const fn column(&self) -> usize {
        self.column
    }

    /// What is wrong.
    #[must_use]
    pub const fn problem(&self) -> &SyntaxProblem {
        &self.problem
    }

    fn at(text: &str, offset: usize, problem: SyntaxProblem) -> Self {
        let before = &text[..offset];
        let line = 1 + before.matches('\n').count();
        let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
        Self {
            line,
            column: 1 + before[line_start..].chars().count(),
            problem,
        }
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "line {}, column {}: {}",
            self.line, self.column, self.problem
        )
    }
}

impl std::error::Error for SyntaxError {}

/// A strict RFC 8259 reader over text, by recursive descent no deeper than
/// [`MAX_NESTING`].
struct Reader<'a> {
    text: &'a str,
    /// Byte offset of the next character; always on a character boundary.
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<char> {
        self.text[self.at..].chars().next()
    }

    fn peek_byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    /// Fails at the next character, or at the end of the text.
    fn fail(&self, problem: SyntaxProblem) -> SyntaxError {
        SyntaxError::at(self.text, self.at, problem)
    }

    fn fail_at(&self, offset: usize, problem: SyntaxProblem) -> SyntaxError {
        SyntaxError::at(self.text, offset, problem)
    }

    /// Fails because the next character is not what the text needs here.
    fn unexpected(&self) -> SyntaxError {
        self.fail(
            self.peek()
                .map_or(SyntaxProblem::EndOfText, SyntaxProblem::Unexpected),
        )
    }

    fn skip_space(&mut self) {
        while matches!(self.peek_byte(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    /// Takes the ASCII byte `byte`, or fails.
    fn expect(&mut self, byte: u8) -> Result<(), SyntaxError> {
        if self.peek_byte() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }

    /// `depth` is how many arrays and objects are open around the value.
    fn value(&mut self, depth: usize) -> Result<Json, SyntaxError> {
        match self.peek_byte() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::String),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number().map(Json::Number),
            _ => Err(self.unexpected()),
        }
    }

    fn word(&mut self, word: &str, value: Json) -> Result<Json, SyntaxError> {
        for byte in word.bytes() {
            self.expect(byte)?;
        }
        Ok(value)
    }

    fn open(&self, depth: usize) -> Result<usize, SyntaxError> {
        if depth >= MAX_NESTING {
            Err(self.fail(SyntaxProblem::TooDeep))
        } else {
            Ok(depth + 1)
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, SyntaxError> {
        let depth = self.open(depth)?;
        self.at += 1;
        let mut items = Vec::new();
        self.skip_space();
        if self.peek_byte() == Some(b']') {
            self.at += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_space();
            items.push(self.value(depth)?);
            self.skip_space();
            match self.peek_byte() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.unexpected()),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, SyntaxError> {
        let depth = self.open(depth)?;
        self.at += 1;
        let mut entries: Vec<(String, Json)> = Vec::new();
        let mut keys: HashSet<String> = HashSet::new();
        self.skip_space();
        if self.peek_byte() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(entries));
        }
        loop {
            self.skip_space();
            let key_at = self.at;
            if self.peek_byte() != Some(b'"') {
                return Err(self.unexpected());
            }
            let key = self.string()?;
            if !keys.insert(key.clone()) {
                return Err(self.fail_at(key_at, SyntaxProblem::DuplicateKey(key)));
            }
            self.skip_space();
            self.expect(b':')?;
            self.skip_space();
            entries.push((key, self.value(depth)?));
            self.skip_space();
            match self.peek_byte() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Object(entries));
                }
                _ => return Err(self.unexpected()),
            }
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while self.peek_byte().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
        self.at - start
    }

    /// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`
    fn number(&mut self) -> Result<Number, SyntaxError> {
        let start = self.at;
        if self.peek_byte() == Some(b'-') {
            self.at += 1;
        }
        match self.peek_byte() {
            Some(b'0') => {
                self.at += 1;
                if self.peek_byte().is_some_and(|byte| byte.is_ascii_digit()) {
                    return Err(self.fail_at(start, SyntaxProblem::LeadingZero));
                }
            }
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.unexpected()),
        }
        let mut whole = true;
        if self.peek_byte() == Some(b'.') {
            whole = false;
            self.at += 1;
            if self.digits() == 0 {
                return Err(self.unexpected());
            }
        }
        if matches!(self.peek_byte(), Some(b'e' | b'E')) {
            whole = false;
            self.at += 1;
            if matches!(self.peek_byte(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                return Err(self.unexpected());
            }
        }
        let written = &self.text[start..self.at];
        if whole {
            return Ok(written
                .parse::<i64>()
                .map_or_else(|_| Number::Large(written.to_owned()), Number::Integer));
        }
        // Rust rounds a decimal to the nearest `f64`, as IEEE 754 asks.
        match written.parse::<f64>() {
            Ok(value) if value.is_finite() => Ok(Number::Float(value)),
            _ => Err(self.fail_at(start, SyntaxProblem::NumberOutOfRange)),
        }
    }

    fn string(&mut self) -> Result<String, SyntaxError> {
        self.at += 1;
        let mut value = String::new();
        loop {
            // A run of characters that need no care, up to a quote, a
            // backslash or a control character; all of them ASCII.
            let run = self.at;
            while self
                .peek_byte()
                .is_some_and(|byte| byte != b'"' && byte != b'\\' && byte >= b' ')
            {
                self.at += 1;
            }
            value.push_str(&self.text[run..self.at]);
            match self.peek_byte() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(value);
                }
                Some(b'\\') => self.escape(&mut value)?,
                Some(_) => {
                    let control = self.peek().unwrap_or_default();
                    return Err(self.fail(SyntaxProblem::ControlCharacter(control)));
                }
                None => return Err(self.fail(SyntaxProblem::EndOfText)),
            }
        }
    }

    fn escape(&mut self, value: &mut String) -> Result<(), SyntaxError> {
        let backslash = self.at;
        self.at += 1;
        let simple = match self.peek_byte() {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                self.at += 1;
                return self.unicode(backslash, value);
            }
            _ => {
                return Err(self.fail(
                    self.peek()
                        .map_or(SyntaxProblem::EndOfText, SyntaxProblem::InvalidEscape),
                ));
            }
        };
        self.at += 1;
        value.push(simple);
        Ok(())
    }

    /// The four hexadecimal digits of a `\u` escape.
    fn hex4(&mut self) -> Result<u32, SyntaxError> {
        let mut code = 0;
        for _ in 0..4 {
            let digit = self
                .peek()
                .and_then(|character| character.to_digit(16))
                .ok_or_else(|| self.fail(SyntaxProblem::InvalidUnicodeEscape))?;
            code = code * 16 + digit;
            self.at += 1;
        }
        Ok(code)
    }

    /// After `\u`: one character, or a surrogate pair written as two escapes.
    fn unicode(&mut self, backslash: usize, value: &mut String) -> Result<(), SyntaxError> {
        let first = self.hex4()?;
        let code = match first {
            0xD800..=0xDBFF => {
                if !self.text[self.at..].starts_with("\\u") {
                    return Err(self.fail_at(backslash, SyntaxProblem::LoneSurrogate));
                }
                self.at += 2;
                let second = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&second) {
                    return Err(self.fail_at(backslash, SyntaxProblem::LoneSurrogate));
                }
                0x1_0000 + ((first - 0xD800) << 10) + (second - 0xDC00)
            }
            0xDC00..=0xDFFF => return Err(self.fail_at(backslash, SyntaxProblem::LoneSurrogate)),
            other => other,
        };
        // Every code left is a Unicode scalar value.
        value.push(char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER));
        Ok(())
    }
}
