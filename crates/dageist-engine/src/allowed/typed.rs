//! What a text typed into a slot reads as: a number, a date, a duration or
//! text, as the engine reads it, with the dtype Polars gives the result.
//!
//! A step document holds a typed value as one of its kinds of argument, so what
//! the editor needs from the engine is which of them the text it was given is.
//! A text is read as each place that takes a typed value would read it:
//!
//! - **A number** as the document's own JSON reader reads one, so what it reads
//!   here is what the document holds once it is written: a whole number an
//!   `i64` holds is an [`Integer`](Literal::Integer), a number written with a
//!   fraction or an exponent is a [`Float`](Literal::Float), and `01`, `.5` and
//!   `+1` are no numbers. A whole number beyond [`LARGEST_WHOLE_NUMBER`] is read
//!   as the number with a fraction it is nearest to: a host writes a number as
//!   JSON, which does not carry it exactly past that, and the document is not
//!   to read back another number than the one that was read. A parameter that
//!   takes a number with a fraction reads a whole number as one.
//! - **A date** as Polars reads a text as a date when it is given no format:
//!   `str.to_date` infers it from a fixed list of patterns, and so does this.
//!   The document holds no date, so the date is carried as the ISO text of the
//!   day Polars read.
//! - **A duration** as Polars reads a duration string: the text given to
//!   `dt.offset_by`, whole amounts of units one after another,
//!   such as `1mo` or `2d12h`. A text typed into an expression that takes a
//!   duration is a `String` literal like any other, which Polars reads when it
//!   runs rows; only a slot that takes a duration itself reads the text as one.
//!   An amount whose units add up to more than an `i64` of nanoseconds, or of
//!   months, is refused in every build, for Polars adds the amounts unchecked.
//! - **Text** as it is, with the dtype `String`, or none where the text is a
//!   path, which is not a value of any dtype.
//!
//! Nothing typed reads as nothing.

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};

use polars::chunked_array::temporal::string::StringMethods;
use polars::prelude::{DataType, Duration, IntoSeries, NamedFrom, Series};

use crate::catalog::ValueType;
use crate::document::{NotWritten, WrittenNumber, read_number};

use super::dtype::Representative;
use super::literal::Literal;

/// The largest whole number read as a whole number: the largest that a JSON
/// number carries exactly, because it is also the largest that an `f64` holds
/// with every whole number below it.
pub const LARGEST_WHOLE_NUMBER: i64 = (1 << 53) - 1;

/// What a text is typed into: a place that takes a typed value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typed {
    /// An expression, where a typed value stands as a chain of one literal: a
    /// number or text.
    Expression,
    /// A parameter that takes a literal of a type.
    Value(ValueType),
    /// A parameter that takes a Polars duration string.
    Duration,
    /// A parameter that takes a file, folder or cloud location.
    Path,
}

/// A text as the slot it was typed into reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Read {
    literal: Literal,
    dtype: Option<Representative>,
}

impl Read {
    /// The literal the document holds for the text.
    #[must_use]
    pub const fn literal(&self) -> &Literal {
        &self.literal
    }

    /// The class of the dtype Polars gives the reading. `None` where it has no
    /// dtype of its own: a path, a duration string and what Polars gives a dtype
    /// that has no class.
    #[must_use]
    pub const fn dtype(&self) -> Option<Representative> {
        self.dtype
    }

    /// A reading as the literal it is, with the dtype Polars gives that literal.
    fn of_literal(literal: Literal) -> Self {
        let dtype = literal.class();
        Self { literal, dtype }
    }

    /// A reading that has no dtype of its own.
    const fn untyped(literal: Literal) -> Self {
        Self {
            literal,
            dtype: None,
        }
    }
}

/// Why a text is not read as what it was typed into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unread {
    /// It is not a number.
    NotANumber,
    /// It is a whole number written with a leading zero, such as `01`.
    LeadingZero,
    /// It is a number no `f64` holds.
    OutOfRange,
    /// It is a number with a fraction or an exponent, where a whole number is
    /// wanted.
    NotAWholeNumber,
    /// It is a whole number past [`LARGEST_WHOLE_NUMBER`], where a whole number
    /// is wanted.
    TooLargeForAWholeNumber,
    /// Polars reads it as no date, for this reason.
    NotADate(String),
    /// Polars reads it as no duration, for this reason.
    NotADuration(String),
}

impl fmt::Display for Unread {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotANumber => formatter.write_str("not a number"),
            Self::LeadingZero => formatter.write_str("a number has no leading zero"),
            Self::OutOfRange => formatter.write_str("the number is too large"),
            Self::NotAWholeNumber => formatter.write_str("not a whole number"),
            Self::TooLargeForAWholeNumber => formatter.write_str(
                "the whole number is too large to be written exactly; write it with a fraction",
            ),
            Self::NotADate(reason) | Self::NotADuration(reason) => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for Unread {}

/// What `text` reads as where it is typed into `typed`: each way the slot reads
/// it, in the order it offers them, a [`Read`] or why it is not that. The text
/// is read as it is typed, white space included, except that a number may have
/// white space around it. Nothing is read of an empty text.
///
/// A slot that takes a number or text reads a text that is a number as one
/// first and as text after; a slot that takes one kind of value reads it as that.
#[must_use]
pub fn read_typed(text: &str, typed: Typed) -> Vec<Result<Read, Unread>> {
    if text.is_empty() {
        return Vec::new();
    }
    match typed {
        Typed::Expression | Typed::Value(ValueType::Any) => {
            let mut readings = Vec::new();
            if let Ok(number) = number(text) {
                readings.push(Ok(number));
            }
            readings.push(Ok(Read::of_literal(Literal::String(text.to_owned()))));
            readings
        }
        Typed::Value(ValueType::Integer) => vec![whole_number(text)],
        Typed::Value(ValueType::Float) => vec![float_number(text)],
        Typed::Value(ValueType::String) => {
            vec![Ok(Read::of_literal(Literal::String(text.to_owned())))]
        }
        Typed::Value(ValueType::Date) => vec![date(text)],
        Typed::Duration => vec![duration(text)],
        Typed::Path => vec![Ok(Read::untyped(Literal::String(text.to_owned())))],
    }
}

/// Whether the whole number `value`, either side of zero, is no larger than [`LARGEST_WHOLE_NUMBER`].
/// The distance from zero is taken unsigned, so that the smallest `i64` is not negated.
const fn is_exact(value: i64) -> bool {
    value.unsigned_abs() <= LARGEST_WHOLE_NUMBER.unsigned_abs()
}

/// The number `text` is, as the document reads one, before it is given a literal.
fn written(text: &str) -> Result<WrittenNumber, Unread> {
    read_number(text).map_err(|problem| match problem {
        NotWritten::NotANumber => Unread::NotANumber,
        NotWritten::LeadingZero => Unread::LeadingZero,
        NotWritten::OutOfRange => Unread::OutOfRange,
    })
}

/// The number `text` is: a whole number where it is written as one that can be
/// written exactly, and otherwise a number with a fraction.
fn number(text: &str) -> Result<Read, Unread> {
    let literal = match written(text)? {
        WrittenNumber::Integer(value) if is_exact(value) => Literal::Integer(value),
        WrittenNumber::Integer(value) => Literal::Float(value as f64),
        WrittenNumber::Float(value) | WrittenNumber::Large(value) => Literal::Float(value),
    };
    Ok(Read::of_literal(literal))
}

/// The whole number `text` is, which a number with a fraction or too large is not.
fn whole_number(text: &str) -> Result<Read, Unread> {
    match written(text)? {
        WrittenNumber::Integer(value) if is_exact(value) => {
            Ok(Read::of_literal(Literal::Integer(value)))
        }
        WrittenNumber::Integer(_) | WrittenNumber::Large(_) => Err(Unread::TooLargeForAWholeNumber),
        WrittenNumber::Float(_) => Err(Unread::NotAWholeNumber),
    }
}

/// The number with a fraction `text` is, which a whole number is as well.
fn float_number(text: &str) -> Result<Read, Unread> {
    let value = match written(text)? {
        WrittenNumber::Integer(value) => value as f64,
        WrittenNumber::Float(value) | WrittenNumber::Large(value) => value,
    };
    Ok(Read::of_literal(Literal::Float(value)))
}

/// The date Polars reads `text` as, with no format given.
fn date(text: &str) -> Result<Read, Unread> {
    crate::startup::ensure();
    let series = Series::new("".into(), [text]);
    let days = series
        .str()
        .and_then(|strings| strings.as_date(None, false))
        .and_then(|dates| dates.into_series().cast(&DataType::String))
        .map_err(|error| Unread::NotADate(error.to_string()))?;
    let written = days
        .str()
        .ok()
        .and_then(|strings| strings.get(0))
        .ok_or_else(|| Unread::NotADate(format!("{text:?} is not a date")))?;
    Ok(Read {
        literal: Literal::String(written.to_owned()),
        dtype: Some(Representative::Date),
    })
}

/// What each unit of a duration string counts in: nanoseconds, for a unit of fixed length, or
/// months. A week is 7 days and a day 24 hours, as Polars counts them in nanoseconds.
enum Count {
    Nanoseconds(i64),
    Months(i64),
}

/// How a unit of a duration string counts. `None` for a word that is no unit, which Polars refuses
/// with its own reason.
fn count_of(unit: &[u8]) -> Option<Count> {
    const MICROSECOND: i64 = 1_000;
    const MILLISECOND: i64 = 1_000 * MICROSECOND;
    const SECOND: i64 = 1_000 * MILLISECOND;
    const MINUTE: i64 = 60 * SECOND;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const WEEK: i64 = 7 * DAY;
    Some(match unit {
        b"ns" | b"i" => Count::Nanoseconds(1),
        b"us" => Count::Nanoseconds(MICROSECOND),
        b"ms" => Count::Nanoseconds(MILLISECOND),
        b"s" => Count::Nanoseconds(SECOND),
        b"m" => Count::Nanoseconds(MINUTE),
        b"h" => Count::Nanoseconds(HOUR),
        b"d" => Count::Nanoseconds(DAY),
        b"w" => Count::Nanoseconds(WEEK),
        b"mo" => Count::Months(1),
        b"q" => Count::Months(3),
        b"y" => Count::Months(12),
        _ => return None,
    })
}

/// Whether the amounts of the duration string `text` fit what Polars holds: the whole of them in
/// the nanoseconds of the units of fixed length, and in the months of the others, each in an
/// `i64`. Polars adds them up with arithmetic that is not checked, so that a build with overflow
/// checks panics and one without wraps around, and it does not multiply days and weeks into
/// nanoseconds until the duration is used. This is checked here, the same in every build, and
/// only to find an amount that is too large: what is no duration for another reason is Polars'
/// to refuse.
fn fits_polars(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'-' | b'+')));
    let (mut nanoseconds, mut months) = (0_i64, 0_i64);
    while at < bytes.len() && bytes[at].is_ascii_digit() {
        let mut amount = 0_i64;
        while let Some(digit) = bytes.get(at).filter(|byte| byte.is_ascii_digit()) {
            let Some(next) = amount
                .checked_mul(10)
                .and_then(|tens| tens.checked_add(i64::from(digit - b'0')))
            else {
                return false;
            };
            amount = next;
            at += 1;
        }
        let unit_start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_alphabetic) {
            at += 1;
        }
        let Some(count) = count_of(&bytes[unit_start..at]) else {
            return true;
        };
        let counted = match count {
            Count::Nanoseconds(per) => amount
                .checked_mul(per)
                .and_then(|counted| nanoseconds.checked_add(counted))
                .map(|total| nanoseconds = total),
            Count::Months(per) => amount
                .checked_mul(per)
                .and_then(|counted| months.checked_add(counted))
                .map(|total| months = total),
        };
        if counted.is_none() {
            return false;
        }
    }
    true
}

/// The duration Polars reads `text` as.
fn duration(text: &str) -> Result<Read, Unread> {
    const TOO_LARGE: &str = "the duration is too large for Polars to hold";
    if !fits_polars(text) {
        return Err(Unread::NotADuration(TOO_LARGE.to_owned()));
    }
    // What `fits_polars` lets through does not overflow Polars' parser; the catch is for a text
    // that its checks do not foresee.
    match catch_unwind(AssertUnwindSafe(|| Duration::try_parse(text))) {
        Ok(Ok(_)) => Ok(Read::untyped(Literal::String(text.to_owned()))),
        Ok(Err(error)) => Err(Unread::NotADuration(error.to_string())),
        Err(_) => Err(Unread::NotADuration(TOO_LARGE.to_owned())),
    }
}
