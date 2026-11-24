//! How many values a range or a repeat gives, where its arguments are typed in, and what no
//! arguments make one of.
//!
//! The functions `int_range`, `date_range`, `datetime_range`, `time_range`, `linear_space` and
//! `repeat` give as many values as their bounds, step or interval and count fit. The count is
//! worked out where the arguments that decide it are typed in, a number or a moment written out,
//! and is not known where one is a column, an aggregate or anything the data decide.
//!
//! Whole numbers are counted here, from the numbers. Calendars and clocks are not: how many
//! months fit between two dates, and what an hour is on the night a clock is put back, is
//! Polars' to say, and the engine asks it, with the arguments typed in, on a table of one row, as
//! it asks of a duration. A range too long to ask for is not known.

use std::panic::{AssertUnwindSafe, catch_unwind};

use polars::prelude::{AnyValue, DataType, Duration, Expr, IntoLazy, NamedFrom, Series, TimeUnit};

use crate::allowed::Count;
use crate::catalog::{Argument, Build, Counting, Entry, time_of_day};

use super::model::{Call, Expression, Start, Value};

/// What the arguments of a range or a repeat say of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Made {
    /// The arguments that are typed in fix the number of values.
    Counts(Count),
    /// No arguments make it: Polars refuses it whatever the rows are, for this reason.
    Refused(String),
}

/// The most values of a range of moments the engine asks Polars to make, to count them.
const MOST_ASKED: i128 = 1_000_000;

/// The most values a column of Polars holds, as the build the engine is has an index of 32 bits:
/// Polars panics at a column of more, saying that its maximum length is reached.
const MOST_HELD: u64 = u32::MAX as u64;

/// The count of `count` values, or that Polars holds no column of so many.
fn held(count: u64) -> Made {
    if count > MOST_HELD {
        Made::Refused(format!(
            "{count} values are more than Polars holds in a column, which is at most {MOST_HELD}"
        ))
    } else {
        Made::Counts(Count::Exactly(count))
    }
}

/// How many values the call of `call` gives, or why none can be asked of: it is a function that
/// gives a count of the arguments and is not a call of one.
pub(super) fn made_by(call: &Call) -> Option<Made> {
    if Counting::of(call.entry(), &[]) != Some(Counting::TheArguments) {
        return None;
    }
    Some(match call.entry().id().as_str() {
        "polars.int_range" => whole_numbers(call),
        "polars.linear_space" => times(
            call,
            "num_samples",
            "num_samples must be a non-negative integer",
        ),
        "polars.repeat" => times(call, "n", "n must not be negative"),
        _ => moments(call),
    })
}

/// The whole number an expression is where it is one typed in with no dtype and nothing after it.
fn typed_whole(value: Option<&Value>) -> Option<i64> {
    let Some(Value::Expression(expression)) = value else {
        return None;
    };
    let Start::Call(start) = expression.start() else {
        return None;
    };
    if !expression.links().is_empty()
        || start.entry().id().as_str() != "polars.lit"
        || start.argument("dtype").is_some()
    {
        return None;
    }
    match start.argument("value")? {
        Value::Integer(number) => Some(*number),
        _ => None,
    }
}

/// The number of times of a repeat or of samples of a linear space.
fn times(call: &Call, parameter: &str, below_nothing: &str) -> Made {
    match typed_whole(call.argument(parameter)) {
        Some(number) if number < 0 => Made::Refused(format!("{below_nothing}, got {number}")),
        Some(number) => held(number.unsigned_abs()),
        None => Made::Counts(Count::NotKnown),
    }
}

/// The least and the most whole number a dtype of whole numbers holds.
fn limits_of(dtype: &DataType) -> Option<(i128, i128)> {
    Some(match dtype {
        DataType::Int8 => (i128::from(i8::MIN), i128::from(i8::MAX)),
        DataType::Int16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        DataType::Int32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        DataType::Int64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
        DataType::Int128 => (i128::MIN, i128::MAX),
        DataType::UInt8 => (0, i128::from(u8::MAX)),
        DataType::UInt16 => (0, i128::from(u16::MAX)),
        DataType::UInt32 => (0, i128::from(u32::MAX)),
        DataType::UInt64 => (0, i128::from(u64::MAX)),
        DataType::UInt128 => (0, i128::MAX),
        _ => return None,
    })
}

/// The whole numbers of `int_range`: from `start` up to but not including `end`, `step` apart.
fn whole_numbers(call: &Call) -> Made {
    let step = match call.argument("step") {
        Some(Value::Integer(step)) => *step,
        _ => 1,
    };
    if step == 0 {
        return Made::Refused("step must not be zero".to_owned());
    }
    let dtype = match call.argument("dtype") {
        Some(Value::Dtype(dtype)) => dtype.as_polars().clone(),
        _ => DataType::Int64,
    };
    let Some((least, most)) = limits_of(&dtype) else {
        return Made::Refused(format!("a range of {dtype} is no range of whole numbers"));
    };
    // Python reads a single bound as the end, counted from 0.
    let given_end = call.argument("end");
    let first = match call.argument("start") {
        None => Some(0),
        Some(_) => typed_whole(call.argument("start")),
    };
    let (start, end) = if given_end.is_none() {
        (Some(0), first)
    } else {
        (first, typed_whole(given_end))
    };
    for (name, bound) in [("start", start), ("end", end)] {
        if let Some(bound) = bound.map(i128::from)
            && !(least..=most).contains(&bound)
        {
            return Made::Refused(format!(
                "the {name} {bound} is no whole number {dtype} holds, which are from {least} to {most}"
            ));
        }
    }
    let (Some(start), Some(end)) = (start.map(i128::from), end.map(i128::from)) else {
        return Made::Counts(Count::NotKnown);
    };
    let step = i128::from(step);
    let (distance, by) = if step > 0 {
        (end - start, step)
    } else {
        (start - end, -step)
    };
    let count = if distance > 0 {
        (distance + by - 1) / by
    } else {
        0
    };
    u64::try_from(count).map_or_else(|_| held(u64::MAX), held)
}

/// The constant that an expression typed in is, as Polars makes it, where it is a number, a text
/// or a flag written out, with a dtype, or a date, a moment or a time of day made of such, and
/// nothing after it.
fn constant(expression: &Expression) -> Option<Expr> {
    if !expression.links().is_empty() {
        return None;
    }
    let Start::Call(start) = expression.start() else {
        return None;
    };
    if !matches!(
        start.entry().id().as_str(),
        "polars.lit" | "polars.date" | "polars.datetime" | "polars.time"
    ) {
        return None;
    }
    let mut given: Vec<(&str, Argument)> = Vec::new();
    for argument in start.arguments() {
        let value = match argument.value() {
            Value::Expression(inner) => Argument::Expression(constant(inner)?),
            other => plain(other)?,
        };
        given.push((argument.parameter(), value));
    }
    built(start.entry(), given)
}

/// What a value that is no expression is as an argument.
fn plain(value: &Value) -> Option<Argument> {
    Some(match value {
        Value::Integer(number) => Argument::Integer(*number),
        Value::Float(number) => Argument::Float(*number),
        Value::String(text) => Argument::String(text.clone()),
        Value::Flag(flag) => Argument::Flag(*flag),
        Value::Choice(word) => Argument::Choice(word.clone()),
        Value::Dtype(dtype) => Argument::Dtype(dtype.as_polars().clone()),
        Value::Duration(text) => Argument::Duration(text.clone()),
        Value::TimeZone(zone) => Argument::TimeZone(zone.clone()),
        _ => return None,
    })
}

fn built(entry: &'static Entry, given: Vec<(&str, Argument)>) -> Option<Expr> {
    let arguments = entry.arguments(given).ok()?;
    match entry.build() {
        Build::Function(build) => build(&arguments).ok(),
        _ => None,
    }
}

/// The nanoseconds from the start of 1970 or of the day that a value of a moment is, where it is
/// one.
fn nanoseconds(value: &AnyValue<'_>) -> Option<i128> {
    const DAY: i128 = 86_400_000_000_000;
    Some(match value {
        AnyValue::Date(days) => i128::from(*days) * DAY,
        AnyValue::Datetime(count, unit, _) | AnyValue::DatetimeOwned(count, unit, _) => {
            i128::from(*count)
                * match unit {
                    TimeUnit::Nanoseconds => 1,
                    TimeUnit::Microseconds => 1_000,
                    TimeUnit::Milliseconds => 1_000_000,
                }
        }
        AnyValue::Time(count) => i128::from(*count),
        AnyValue::Int8(count) => i128::from(*count),
        AnyValue::Int16(count) => i128::from(*count),
        AnyValue::Int32(count) => i128::from(*count),
        AnyValue::Int64(count) => i128::from(*count),
        _ => return None,
    })
}

/// The shortest step an interval makes, in nanoseconds: a month is 28 days, a day 23 hours.
fn shortest_step(interval: &Duration) -> i128 {
    const HOUR: i128 = 3_600_000_000_000;
    i128::from(interval.months()) * 28 * 24 * HOUR
        + i128::from(interval.weeks()) * 7 * 24 * HOUR
        + i128::from(interval.days()) * 23 * HOUR
        + i128::from(interval.nanoseconds())
}

/// What came of asking Polars to make a table of one row of some expressions.
enum Asked {
    /// Polars made the table.
    Made(polars::prelude::DataFrame),
    /// Polars refused, in these words, up to its first blank line.
    Refused(String),
    /// Polars panicked, which is no answer: a build with debug assertions asserts on a range of
    /// days across the clocks changing, which the release builds.
    Panicked,
}

/// Asks Polars to make `expressions` as the columns of a table of one row.
fn asked(expressions: Vec<Expr>) -> Asked {
    crate::startup::ensure();
    let made = catch_unwind(AssertUnwindSafe(|| {
        polars::prelude::DataFrame::new(1, vec![Series::new("row".into(), [0_i64]).into()])
            .map_err(|error| error.to_string())?
            .lazy()
            .select(expressions)
            .collect()
            .map_err(|error| first_paragraph(&error.to_string()))
    }));
    match made {
        Ok(Ok(frame)) => Asked::Made(frame),
        Ok(Err(said)) => Asked::Refused(said),
        Err(_) => Asked::Panicked,
    }
}

/// The dates, moments and times of day of `date_range`, `datetime_range` and `time_range`.
fn moments(call: &Call) -> Made {
    let id = call.entry().id().as_str();
    let default = if id == "polars.time_range" {
        "1h"
    } else {
        "1d"
    };
    let text = match call.argument("interval") {
        Some(Value::Duration(text)) => text.as_str(),
        _ => default,
    };
    let interval = match Duration::try_parse(text) {
        Ok(interval) => interval,
        Err(error) => return Made::Refused(first_paragraph(&error.to_string())),
    };
    // An interval that is not above nothing gives no value where the end is not before the start,
    // and is refused there, so what the data decide is refused whatever they are.
    if interval.is_zero() || interval.negative() {
        return Made::Refused(
            "`interval` must be positive: Polars refuses an interval of nothing or below nothing, \
             unless the end is before the start, where it gives no value"
                .to_owned(),
        );
    }
    if id == "polars.date_range" && !interval.is_full_days() {
        return Made::Refused(format!(
            "`interval` input for `date_range` must consist of full days, got: {text}"
        ));
    }
    if id == "polars.time_range"
        && let Some(unit) = ["y", "mo", "w", "d"]
            .into_iter()
            .find(|unit| text.contains(unit))
    {
        return Made::Refused(format!(
            "invalid interval unit for time_range: found '{unit}'"
        ));
    }
    // Counted in a unit the interval is too short for, Polars rounds it down to nothing.
    if let Some(Value::Choice(word)) = call.argument("time_unit")
        && interval.months() == 0
        && interval.weeks() == 0
        && interval.days() == 0
    {
        let unit = match word.as_str() {
            "ns" => 1,
            "us" => 1_000,
            _ => 1_000_000,
        };
        if i128::from(interval.nanoseconds()) < unit {
            return Made::Refused(format!(
                "interval {text} is too small for time unit {word} and was rounded down to zero"
            ));
        }
    }
    let typed = |name: &str| match call.argument(name) {
        Some(Value::Expression(expression)) => constant(expression),
        // `time_range` gives its own bounds where it is given none.
        None if id == "polars.time_range" => Some(if name == "start" {
            time_of_day(0, 0, 0, 0)
        } else {
            time_of_day(23, 59, 59, 999_999)
        }),
        _ => None,
    };
    let (Some(start), Some(end)) = (typed("start"), typed("end")) else {
        // Not both are typed in, but the ones that are can still make what Polars cannot count.
        return Made::Counts(Count::NotKnown);
    };
    let mut given: Vec<(&str, Argument)> = vec![
        ("start", Argument::Expression(start.clone())),
        ("end", Argument::Expression(end.clone())),
    ];
    for argument in call.arguments() {
        if !matches!(argument.parameter(), "start" | "end")
            && let Some(value) = plain(argument.value())
        {
            given.push((argument.parameter(), value));
        }
    }
    let Some(range) = built(call.entry(), given) else {
        return Made::Counts(Count::NotKnown);
    };
    // How many values it can give at the most, from how far apart the ends are.
    let ends = match asked(vec![start.alias("start"), end.alias("end")]) {
        Asked::Made(frame) => (|| {
            let first = frame.column("start").ok()?.get(0).ok()?;
            let last = frame.column("end").ok()?.get(0).ok()?;
            Some((nanoseconds(&first)?, nanoseconds(&last)?))
        })(),
        Asked::Refused(reason) => return Made::Refused(reason),
        Asked::Panicked => return Made::Counts(Count::NotKnown),
    };
    let step = shortest_step(&interval).max(1);
    if let Some((first, last)) = ends
        && (last - first).abs() / step + 2 > MOST_ASKED
    {
        return Made::Counts(Count::NotKnown);
    }
    match asked(vec![range]) {
        Asked::Made(frame) => Made::Counts(Count::Exactly(frame.height() as u64)),
        Asked::Refused(reason) => Made::Refused(reason),
        Asked::Panicked => Made::Counts(Count::NotKnown),
    }
}

fn first_paragraph(text: &str) -> String {
    text.split("\n\n").next().unwrap_or(text).to_owned()
}
