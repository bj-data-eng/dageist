//! How the engine writes a typed value and the names in a step's summary: a
//! number as Python writes it, so that a whole float keeps its fraction; text
//! quoted and escaped; a flag as `True`, `False` or `None`; a dtype as the
//! attribute of `pl` that Python Polars names it by.
//!
//! These are the words of [`Step::summary`](super::Step::summary), of
//! [`Value::python`] and, for a number, of
//! [`Literal::shown`](crate::allowed::Literal::shown).

use std::fmt::Write as _;

use polars::prelude::{DataType, TimeUnit};

use super::model::{Expression, Start, Value};

impl Value {
    /// The value as Python writes it, where it is not made of other values
    /// or an expression, or `None` where it is: a number as Python's `repr`
    /// writes it, so that a whole float keeps its fraction; text, a choice, a
    /// duration, a pattern, a path and a time zone as a string in double
    /// quotes; a flag and no time zone as `True`, `False` or `None`; a dtype as
    /// the attribute of `pl` Python Polars names it by, such as `pl.Int64` or
    /// `pl.List(pl.String)`; column names as one string, or a list of them
    /// where there is not one; and a table as its own name.
    #[must_use]
    pub fn python(&self) -> Option<String> {
        scalar(self)
    }
}

/// `value` as Python writes it where it is not made of other values or an
/// expression, or `None` where it is. A table is its own name.
pub(super) fn scalar(value: &Value) -> Option<String> {
    Some(match value {
        Value::Columns(names) => match names.as_slice() {
            [only] => string(only),
            _ => format!(
                "[{}]",
                names
                    .iter()
                    .map(|name| string(name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
        Value::Field(name) => string(name),
        Value::Integer(number) => number.to_string(),
        Value::Float(number) => float(*number),
        Value::String(text)
        | Value::Choice(text)
        | Value::Duration(text)
        | Value::Pattern(text)
        | Value::Path(text) => string(text),
        Value::Flag(Some(true)) => "True".to_owned(),
        Value::Flag(Some(false)) => "False".to_owned(),
        Value::Flag(None) => "None".to_owned(),
        Value::TimeZone(Some(zone)) => string(zone),
        Value::TimeZone(None) => "None".to_owned(),
        // A dtype of a document is one `dtype_python` writes.
        Value::Dtype(dtype) => dtype_python(dtype.as_polars()).unwrap_or_default(),
        Value::Table(name) => name.clone(),
        Value::Expression(_) | Value::Pairs(_) | Value::Many(_) => return None,
    })
}

/// The name of the column `expression` is, when it is a field and nothing more.
pub(super) fn field_alone(expression: &Expression) -> Option<&str> {
    let Start::Call(start) = expression.start() else {
        return None;
    };
    if start.entry().id().as_str() != "polars.col" || !expression.links().is_empty() {
        return None;
    }
    let Some(Value::Columns(names)) = start.argument("name") else {
        return None;
    };
    // `more_names` makes it several columns.
    match (names.as_slice(), start.argument("more_names")) {
        ([only], None) => Some(only),
        ([only], Some(Value::Many(more))) if more.is_empty() => Some(only),
        _ => None,
    }
}

/// `text` as a Python string literal in double quotes.
fn string(text: &str) -> String {
    let mut written = String::with_capacity(text.len() + 2);
    written.push('"');
    for character in text.chars() {
        match character {
            '"' => written.push_str("\\\""),
            '\\' => written.push_str("\\\\"),
            '\n' => written.push_str("\\n"),
            '\r' => written.push_str("\\r"),
            '\t' => written.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(written, "\\x{:02x}", u32::from(control));
            }
            other => written.push(other),
        }
    }
    written.push('"');
    written
}

/// `number` as Python's `repr` writes it: the shortest digits that read back
/// as the same number, with a fraction or an exponent so that it is a float,
/// and an exponent only where the number is below 1e-4 or from 1e16 on.
pub(crate) fn float(number: f64) -> String {
    if number == 0.0 {
        return if number.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    let sign = if number < 0.0 { "-" } else { "" };
    let scientific = format!("{:e}", number.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .unwrap_or((scientific.as_str(), "0"));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let exponent: i32 = exponent.parse().unwrap_or(0);
    // The number is 0.DIGITS times ten to this power.
    let point = exponent + 1;
    let length = i32::try_from(digits.len()).unwrap_or(i32::MAX);
    let body = if (-3..=16).contains(&point) {
        if point <= 0 {
            format!(
                "0.{}{digits}",
                "0".repeat(usize::try_from(-point).unwrap_or(0))
            )
        } else if point >= length {
            format!(
                "{digits}{}.0",
                "0".repeat(usize::try_from(point - length).unwrap_or(0))
            )
        } else {
            let (whole, fraction) = digits.split_at(usize::try_from(point).unwrap_or(0));
            format!("{whole}.{fraction}")
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let fraction = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        let power = exponent;
        format!(
            "{first}{fraction}e{}{:02}",
            if power < 0 { '-' } else { '+' },
            power.abs()
        )
    };
    format!("{sign}{body}")
}

/// `dtype` as Python Polars names it, written from its structure: each dtype is an
/// attribute of `pl`, and a parameter is by name, as in
/// `pl.Datetime(time_unit='us', time_zone=None)` and `pl.List(pl.Int64)`. It is
/// `None` for a dtype Python Polars has no one name for, such as a decimal, a
/// struct or binary.
fn dtype_python(dtype: &DataType) -> Option<String> {
    let simple = match dtype {
        DataType::Boolean => "Boolean",
        DataType::Date => "Date",
        DataType::Time => "Time",
        DataType::Null => "Null",
        DataType::String => "String",
        DataType::Float16 => "Float16",
        DataType::Float32 => "Float32",
        DataType::Float64 => "Float64",
        DataType::Int8 => "Int8",
        DataType::Int16 => "Int16",
        DataType::Int32 => "Int32",
        DataType::Int64 => "Int64",
        DataType::Int128 => "Int128",
        DataType::UInt8 => "UInt8",
        DataType::UInt16 => "UInt16",
        DataType::UInt32 => "UInt32",
        DataType::UInt64 => "UInt64",
        DataType::UInt128 => "UInt128",
        DataType::Datetime(unit, zone) => {
            let zone = zone.as_ref().map_or_else(
                || "None".to_owned(),
                |zone| {
                    format!(
                        "'{}'",
                        zone.to_string().replace('\\', "\\\\").replace('\'', "\\'")
                    )
                },
            );
            return Some(format!(
                "pl.Datetime(time_unit='{}', time_zone={zone})",
                time_unit(*unit)
            ));
        }
        DataType::Duration(unit) => {
            return Some(format!("pl.Duration(time_unit='{}')", time_unit(*unit)));
        }
        DataType::List(inner) => return Some(format!("pl.List({})", dtype_python(inner)?)),
        _ => return None,
    };
    Some(format!("pl.{simple}"))
}

const fn time_unit(unit: TimeUnit) -> &'static str {
    match unit {
        TimeUnit::Nanoseconds => "ns",
        TimeUnit::Microseconds => "us",
        TimeUnit::Milliseconds => "ms",
    }
}
