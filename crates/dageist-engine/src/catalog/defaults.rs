//! What a parameter holds when it is not given: Polars' default as Python
//! writes it, and that text read as a value.
//!
//! The text is kept because it is the authority: the inventory in
//! `api/polars/python.txt` writes each default as Python's signature does.
//! Reading it here, once, means no host or view parses Python.

/// What a parameter holds when it is not given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterDefault {
    /// It must be given.
    Required,
    /// Polars' default, written as Python writes it, such as `1`, `None` or
    /// `'group_to_rows'`.
    Python(&'static str),
}

/// A default read as a value.
#[derive(Debug, Clone, PartialEq)]
pub enum DefaultValue {
    /// The parameter has no default: it must be given.
    Required,
    /// Python's `None`: unset, which leaves the choice to Polars. Also Polars' `NO_DEFAULT`, the
    /// name Python's signature gives a parameter that is not given where `None` is a value that
    /// can be: `replace`'s `new` and `replace_strict`'s `default`.
    Unset,
    /// `True` or `False`.
    Flag(bool),
    /// A whole number.
    Integer(i64),
    /// A floating-point number.
    Float(f64),
    /// Text, with the escapes Python wrote in it resolved.
    Text(String),
    /// An empty `*` or `**` parameter: `()` or `{}`.
    Empty,
    /// Anything else Python can write: a name such as `DEFAULT_QUERY_OPT_FLAGS`,
    /// a tuple, a number in hexadecimal. The engine does not read it as a value.
    Other,
}

impl ParameterDefault {
    /// The default read as a value; see [`DefaultValue::from_python`].
    #[must_use]
    pub fn value(self) -> DefaultValue {
        match self {
            Self::Required => DefaultValue::Required,
            Self::Python(text) => DefaultValue::from_python(text),
        }
    }
}

impl DefaultValue {
    /// Reads a default as Python writes it in a signature.
    ///
    /// Python's literals are read as Python writes them: `None`, `True` and
    /// `False`; decimal integers and floats, signed, with the underscores
    /// Python allows between digits; single- or double-quoted text, with the
    /// escapes `\\`, `\'`, `\"`, `\n`, `\t` and `\r`; the empty `()` and
    /// `{}`; `NO_DEFAULT` as unset; and the one name of the inventory's defaults that is a
    /// number, `math.e`, as Euler's number. Text with any other escape, or with an
    /// unescaped quote inside, is [`DefaultValue::Other`], as is a number
    /// outside `i64` or `f64`.
    #[must_use]
    pub fn from_python(text: &str) -> Self {
        match text {
            "None" | "NO_DEFAULT" => Self::Unset,
            "True" => Self::Flag(true),
            "False" => Self::Flag(false),
            "()" | "{}" => Self::Empty,
            "math.e" => Self::Float(std::f64::consts::E),
            _ => number(text)
                .or_else(|| string(text).map(Self::Text))
                .unwrap_or(Self::Other),
        }
    }
}

/// `digit (["_"] digit)*` at the start of `text`, and what follows it.
fn digits(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    if !bytes.first()?.is_ascii_digit() {
        return None;
    }
    let mut end = 1;
    loop {
        match bytes.get(end) {
            Some(byte) if byte.is_ascii_digit() => end += 1,
            Some(b'_') if bytes.get(end + 1).is_some_and(u8::is_ascii_digit) => end += 2,
            _ => return Some(&text[end..]),
        }
    }
}

/// What follows an optional exponent, `e` or `E`, an optional sign and
/// digits. `None` when an exponent is begun and not finished.
fn after_exponent(text: &str) -> Option<&str> {
    match text.strip_prefix(['e', 'E']) {
        None => Some(text),
        Some(rest) => digits(rest.strip_prefix(['+', '-']).unwrap_or(rest)),
    }
}

/// Which kind of decimal number `text`, without a sign, is written as.
enum Written {
    Integer,
    Float,
}

fn written(text: &str) -> Option<Written> {
    let (kind, rest) = match digits(text) {
        Some("") => return Some(Written::Integer),
        Some(rest) => match rest.strip_prefix('.') {
            Some(fraction) => (Written::Float, digits(fraction).unwrap_or(fraction)),
            None => (Written::Float, rest),
        },
        None => (Written::Float, digits(text.strip_prefix('.')?)?),
    };
    after_exponent(rest).filter(|left| left.is_empty())?;
    Some(kind)
}

fn number(text: &str) -> Option<DefaultValue> {
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let kind = written(unsigned)?;
    let clean: String = unsigned.chars().filter(|&c| c != '_').collect();
    let sign = if negative { "-" } else { "" };
    match kind {
        Written::Integer => format!("{sign}{clean}")
            .parse()
            .ok()
            .map(DefaultValue::Integer),
        Written::Float => format!("{sign}{clean}")
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(DefaultValue::Float),
    }
}

fn string(text: &str) -> Option<String> {
    let quote = text.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    let inner = text.strip_prefix(quote)?.strip_suffix(quote)?;
    let mut value = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        match character {
            // A quote of the kind that delimits the text must be escaped.
            c if c == quote => return None,
            '\\' => value.push(match characters.next()? {
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                _ => return None,
            }),
            c => value.push(c),
        }
    }
    Some(value)
}
