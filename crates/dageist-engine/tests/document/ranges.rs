//! The functions that start an expression with a number of values their arguments decide:
//! `int_range`, `date_range`, `datetime_range`, `time_range`, `linear_space` and `repeat`.
//!
//! Every expectation is what Python Polars 2.1.0 does, which each test asks by running the same
//! document on the rows of a table. The walk must say the same of the same document: how many
//! values each call gives where its arguments are typed in, and which places refuse it. Each
//! sweep collects every disagreement before it asserts, and a case that is not asked, or that
//! the editor refuses more strictly than Polars on purpose, is listed with its reason.
//!
//! The table has seven rows, whose groups by `k` are of four, two and one, and whose first value
//! of `a` is 4, so that no count that a column gives is the rows or one, and no group has the
//! length of another.

use std::collections::BTreeMap;

use dageist_engine::allowed::{Count, RefusalSource};
use dageist_engine::catalog::{self, Applies};
use dageist_engine::document::{
    Document, Inputs, Outcome, Report, Segment, StepRows, WalkedExpression,
};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, field_then, many, step, whole};

/// The table: a column of each dtype a range is given, of seven rows.
fn frame() -> DataFrame {
    let ints = |name: &str| Series::new(name.into(), [4_i64, 5, 6, 7, 1, 2, 3]);
    let cast = |name: &str, dtype: DataType| {
        ints(name)
            .cast(&dtype)
            .unwrap_or_else(|error| panic!("whole numbers cast to {dtype}: {error}"))
    };
    // A list of one whole number in each row, and an array of two.
    let lists = Series::new(
        "l".into(),
        [4_i64, 5, 6, 7, 1, 2, 3].map(|number| Series::new("".into(), [number])),
    );
    let arrays = Series::new(
        "arr".into(),
        [4_i64, 5, 6, 7, 1, 2, 3].map(|number| Series::new("".into(), [number, number + 1])),
    )
    .cast(&DataType::Array(Box::new(DataType::Int64), 2))
    .expect("lists of two cast to arrays of two");
    DataFrame::new(
        7,
        vec![
            lists.into(),
            arrays.into(),
            ints("a").into(),
            // The number of values of a count that the data decide: three, which no row count is.
            Series::new("n".into(), [3_i64; 7]).into(),
            Series::new("x".into(), [0.5_f64, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5]).into(),
            Series::new("k".into(), [1_i64, 1, 1, 1, 2, 2, 3]).into(),
            Series::new("f".into(), [true, false, true, true, false, true, true]).into(),
            Series::new("s".into(), ["x", "y", "z", "x", "y", "z", "x"]).into(),
            cast("dt", DataType::Datetime(TimeUnit::Microseconds, None)).into(),
            cast("d", DataType::Date).into(),
            cast("tm", DataType::Time).into(),
            cast("dur", DataType::Duration(TimeUnit::Microseconds)).into(),
        ],
    )
    .expect("columns of seven rows")
}

/// A table that a join takes: `rk` holds whole numbers.
fn inputs() -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "other",
            "Other",
            df!("ra" => ["x", "y", "z"], "rk" => [1_i64, 2, 3])
                .expect("the other table")
                .lazy(),
        )
        .expect("a first input");
    inputs
}

/// The document of the one step `entry` given `arguments`, run on the table.
fn run(entry: &str, arguments: Value) -> Report {
    let document =
        Document::read(&document(vec![step("s", entry, arguments)])).expect("a document");
    document
        .run_on(&inputs(), &frame())
        .expect("the table has a schema")
}

/// The report of the step with the plan built and no rows run, which a range of more values than
/// Polars holds is asked of.
fn planned(entry: &str, arguments: Value) -> Report {
    let document =
        Document::read(&document(vec![step("s", entry, arguments)])).expect("a document");
    document
        .run(&inputs(), frame().lazy())
        .expect("the table has a schema")
}

fn select_of(items: Vec<Value>) -> Report {
    run("LazyFrame.select", json!({ "exprs": many(items) }))
}

/// The expression of the `at`th item of the select, with what the walk says of it.
fn item(report: &Report, at: usize) -> &WalkedExpression {
    let path = [Segment::Parameter("exprs".to_owned()), Segment::Item(at)];
    report.steps()[0]
        .walk()
        .expressions()
        .iter()
        .find(|expression| expression.place().path() == path)
        .unwrap_or_else(|| panic!("the walk holds no item {at}"))
}

/// What Polars and the walk say of a report.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Said {
    /// Polars runs the step on the rows, or the walk finds nothing refused.
    Accepts,
    /// It is refused, with the words or the rule that refuse it.
    Refuses(String),
}

impl Said {
    const fn accepts(&self) -> bool {
        matches!(self, Self::Accepts)
    }
}

fn polars_says(report: &Report) -> Said {
    let step = &report.steps()[0];
    match (step.outcome(), step.rows()) {
        (Outcome::Valid { .. }, StepRows::Counted(_)) => Said::Accepts,
        (Outcome::Valid { .. }, StepRows::Failed(error)) => Said::Refuses(first_line(error)),
        (Outcome::Refused(refusal), _) => Said::Refuses(first_line(refusal)),
        (other, _) => Said::Refuses(format!("{other:?}")),
    }
}

fn first_line(text: &impl ToString) -> String {
    text.to_string()
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn walk_says(report: &Report) -> Said {
    match report.steps()[0].walk().refusal() {
        None => Said::Accepts,
        Some(refusal) => Said::Refuses(match refusal.refusal().source() {
            RefusalSource::Rule(rule) => rule.name().to_owned(),
            _ => first_line(&refusal.refusal().reason()),
        }),
    }
}

// ---------------------------------------------------------------------------
// Typed arguments

fn lit(arguments: Value) -> Value {
    chain(call("polars.lit", arguments), vec![])
}

/// The parts of a date and a time of day that a text such as `2024-01-31 05:30:15.000250` has.
fn parts_of(text: &str) -> Vec<(&'static str, i64)> {
    let (day, clock) = text.split_once(' ').unwrap_or((text, ""));
    let mut parts = Vec::new();
    let mut numbers = day
        .split('-')
        .map(|number| number.parse::<i64>().expect("a number"));
    if !day.is_empty() && day.contains('-') {
        for name in ["year", "month", "day"] {
            parts.push((name, numbers.next().expect("a part of a date")));
        }
    }
    let clock = if day.contains('-') { clock } else { text };
    if !clock.is_empty() {
        let (whole_seconds, fraction) = clock.split_once('.').unwrap_or((clock, ""));
        let mut numbers = whole_seconds
            .split(':')
            .map(|number| number.parse::<i64>().expect("a number"));
        for name in ["hour", "minute", "second"] {
            parts.push((name, numbers.next().expect("a part of a time")));
        }
        if !fraction.is_empty() {
            let micro = format!("{fraction:0<6}");
            parts.push(("microsecond", micro.parse().expect("microseconds")));
        }
    }
    parts
}

/// A call that makes a moment or a time of the parts of `text`, each part a number typed in.
fn made_of(id: &str, text: &str, only: &[&str]) -> Value {
    let arguments: Vec<(&str, Value)> = parts_of(text)
        .into_iter()
        .filter(|(name, _)| only.contains(name))
        .map(|(name, number)| (name, whole(number)))
        .collect();
    start_of(id, arguments)
}

/// A date typed in: the call that makes one of a year, a month and a day.
fn date(text: &str) -> Value {
    made_of("polars.date", text, &["year", "month", "day"])
}

/// A moment typed in, in microseconds and in no time zone.
fn moment(text: &str) -> Value {
    made_of(
        "polars.datetime",
        text,
        &[
            "year",
            "month",
            "day",
            "hour",
            "minute",
            "second",
            "microsecond",
        ],
    )
}

/// A time of day typed in.
fn time_of_day(text: &str) -> Value {
    made_of(
        "polars.time",
        text,
        &["hour", "minute", "second", "microsecond"],
    )
}

fn choice(word: &str) -> Value {
    json!({ "choice": word })
}

fn interval(text: &str) -> Value {
    json!({ "duration": text })
}

fn start_of(id: &str, arguments: Vec<(&str, Value)>) -> Value {
    let arguments: serde_json::Map<String, Value> = arguments
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    chain(call(id, Value::Object(arguments)), vec![])
}

/// One case of a count typed in: the call, and the number of values Polars gives it, which the
/// run counts, or that the case is too large to run and is worked out by hand.
struct Typed {
    name: &'static str,
    call: Value,
    /// `None` where the range is run in Polars; the number it must have where it is not.
    by_hand: Option<u64>,
}

fn ran(name: &'static str, call: Value) -> Typed {
    Typed {
        name,
        call,
        by_hand: None,
    }
}

fn counted_by_hand(name: &'static str, call: Value, count: u64) -> Typed {
    Typed {
        name,
        call,
        by_hand: Some(count),
    }
}

fn int_range(arguments: Vec<(&str, Value)>) -> Value {
    start_of("polars.int_range", arguments)
}

fn date_range(arguments: Vec<(&str, Value)>) -> Value {
    start_of("polars.date_range", arguments)
}

fn datetime_range(arguments: Vec<(&str, Value)>) -> Value {
    start_of("polars.datetime_range", arguments)
}

fn time_range(arguments: Vec<(&str, Value)>) -> Value {
    start_of("polars.time_range", arguments)
}

fn linear_space(arguments: Vec<(&str, Value)>) -> Value {
    start_of("polars.linear_space", arguments)
}

fn repeat(arguments: Vec<(&str, Value)>) -> Value {
    start_of("polars.repeat", arguments)
}

/// The counts typed in that Polars makes, with the edges of each: an empty range, a negative
/// step, every `closed`, calendar and clock intervals, a time zone with a change of clock, and
/// the units. Every name says what it holds.
fn counts_typed_in() -> Vec<Typed> {
    let both = |first: &str, last: &str| vec![("start", date(first)), ("end", date(last))];
    let with = |mut arguments: Vec<(&'static str, Value)>, more: Vec<(&'static str, Value)>| {
        arguments.extend(more);
        arguments
    };
    let at = |first: &str, last: &str| vec![("start", moment(first)), ("end", moment(last))];
    let clock =
        |first: &str, last: &str| vec![("start", time_of_day(first)), ("end", time_of_day(last))];
    let mut all = vec![
        // int_range
        ran(
            "int_range 0 to 10 by 2",
            int_range(vec![
                ("start", whole(0)),
                ("end", whole(10)),
                ("step", json!({ "integer": 2 })),
            ]),
        ),
        ran(
            "int_range down by 3",
            int_range(vec![
                ("start", whole(10)),
                ("end", whole(0)),
                ("step", json!({ "integer": -3 })),
            ]),
        ),
        ran(
            "int_range up that is empty",
            int_range(vec![("start", whole(10)), ("end", whole(0))]),
        ),
        ran(
            "int_range down that is empty",
            int_range(vec![
                ("start", whole(0)),
                ("end", whole(10)),
                ("step", json!({ "integer": -1 })),
            ]),
        ),
        ran(
            "int_range from a place to itself",
            int_range(vec![("start", whole(5)), ("end", whole(5))]),
        ),
        ran(
            "int_range given only the end",
            int_range(vec![("start", whole(5))]),
        ),
        ran("int_range given nothing", int_range(vec![])),
        ran(
            "int_range of a step past the end",
            int_range(vec![
                ("start", whole(0)),
                ("end", whole(3)),
                ("step", json!({ "integer": 100 })),
            ]),
        ),
        ran(
            "int_range as an unsigned byte",
            int_range(vec![
                ("start", whole(0)),
                ("end", whole(255)),
                ("dtype", json!({ "dtype": "UInt8" })),
            ]),
        ),
        ran(
            "int_range as a signed byte",
            int_range(vec![
                ("start", whole(-128)),
                ("end", whole(127)),
                ("dtype", json!({ "dtype": "Int8" })),
            ]),
        ),
        ran(
            "int_range of 128 bits",
            int_range(vec![
                ("start", whole(0)),
                ("end", whole(3)),
                ("dtype", json!({ "dtype": "Int128" })),
            ]),
        ),
        ran(
            "int_range of a million",
            int_range(vec![("start", whole(0)), ("end", whole(1_000_000))]),
        ),
        ran(
            "int_range at the largest end",
            int_range(vec![
                ("start", whole(i64::MAX - 3)),
                ("end", whole(i64::MAX)),
            ]),
        ),
        ran(
            "int_range from the smallest",
            int_range(vec![
                ("start", whole(i64::MIN)),
                ("end", whole(i64::MIN + 3)),
            ]),
        ),
        counted_by_hand(
            "int_range too long to run",
            int_range(vec![("start", whole(0)), ("end", whole(4_000_000_000))]),
            4_000_000_000,
        ),
        counted_by_hand(
            "int_range of the most values Polars holds",
            int_range(vec![("start", whole(0)), ("end", whole(4_294_967_295))]),
            4_294_967_295,
        ),
        counted_by_hand(
            "repeat of the most values Polars holds",
            repeat(vec![("value", whole(1)), ("n", whole(4_294_967_295))]),
            4_294_967_295,
        ),
        counted_by_hand(
            "int_range over the whole of an i64",
            int_range(vec![
                ("start", whole(i64::MIN)),
                ("end", whole(i64::MAX)),
                ("step", json!({ "integer": i64::MAX })),
            ]),
            3,
        ),
        // date_range
        ran(
            "date_range by the day",
            date_range(both("2024-01-01", "2024-01-10")),
        ),
        ran(
            "date_range by two days",
            date_range(with(
                both("2024-01-01", "2024-01-10"),
                vec![("interval", interval("2d"))],
            )),
        ),
        ran(
            "date_range by the week",
            date_range(with(
                both("2024-01-01", "2024-03-10"),
                vec![("interval", interval("1w"))],
            )),
        ),
        ran(
            "date_range by the month from the end of one",
            date_range(with(
                both("2024-01-31", "2024-06-10"),
                vec![("interval", interval("1mo"))],
            )),
        ),
        ran(
            "date_range by the year from a leap day",
            date_range(with(
                both("2024-02-29", "2030-06-10"),
                vec![("interval", interval("1y"))],
            )),
        ),
        ran(
            "date_range by the quarter",
            date_range(with(
                both("2024-01-01", "2025-06-10"),
                vec![("interval", interval("1q"))],
            )),
        ),
        ran(
            "date_range by a month and a day",
            date_range(with(
                both("2024-01-01", "2024-06-10"),
                vec![("interval", interval("1mo1d"))],
            )),
        ),
        ran(
            "date_range by a day past the end",
            date_range(with(
                both("2024-01-01", "2024-01-10"),
                vec![("interval", interval("30d"))],
            )),
        ),
        ran(
            "date_range that starts after it ends",
            date_range(both("2024-01-10", "2024-01-01")),
        ),
        ran(
            "date_range from a day to itself",
            date_range(both("2024-01-01", "2024-01-01")),
        ),
        ran(
            "date_range from a day to itself closed left",
            date_range(with(
                both("2024-01-01", "2024-01-01"),
                vec![("closed", choice("left"))],
            )),
        ),
        ran(
            "date_range from a day to itself closed none",
            date_range(with(
                both("2024-01-01", "2024-01-01"),
                vec![("closed", choice("none"))],
            )),
        ),
        ran(
            "date_range closed left",
            date_range(with(
                both("2024-01-01", "2024-01-05"),
                vec![("closed", choice("left"))],
            )),
        ),
        ran(
            "date_range closed right",
            date_range(with(
                both("2024-01-01", "2024-01-05"),
                vec![("closed", choice("right"))],
            )),
        ),
        ran(
            "date_range closed none",
            date_range(with(
                both("2024-01-01", "2024-01-05"),
                vec![("closed", choice("none"))],
            )),
        ),
        ran(
            "date_range closed none by two days",
            date_range(with(
                both("2024-01-01", "2024-01-05"),
                vec![("closed", choice("none")), ("interval", interval("2d"))],
            )),
        ),
        ran(
            "date_range of moments",
            date_range(vec![
                ("start", moment("2024-01-01 05:00:00")),
                ("end", moment("2024-01-05 03:00:00")),
            ]),
        ),
        ran(
            "date_range of a date and a moment",
            date_range(vec![
                ("start", date("2024-01-01")),
                ("end", moment("2024-01-05 03:00:00")),
            ]),
        ),
        ran(
            "date_range of a very early date",
            date_range(both("0001-01-01", "0001-01-05")),
        ),
        // datetime_range
        ran(
            "datetime_range by the day",
            datetime_range(at("2024-01-01 00:00:00", "2024-01-05 00:00:00")),
        ),
        ran(
            "datetime_range by six hours",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2024-01-02 00:00:00"),
                vec![("interval", interval("6h"))],
            )),
        ),
        ran(
            "datetime_range by an hour and a half",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2024-01-02 00:00:00"),
                vec![("interval", interval("1h30m"))],
            )),
        ),
        ran(
            "datetime_range by the month from the end of one",
            datetime_range(with(
                at("2024-01-31 00:00:00", "2024-06-01 00:00:00"),
                vec![("interval", interval("1mo"))],
            )),
        ),
        ran(
            "datetime_range by a day and a month",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2024-06-01 00:00:00"),
                vec![("interval", interval("1d1mo"))],
            )),
        ),
        ran(
            "datetime_range by the year",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2030-02-01 00:00:00"),
                vec![("interval", interval("1y"))],
            )),
        ),
        ran(
            "datetime_range by nanoseconds reads nanoseconds",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2024-01-01 00:00:00.000001"),
                vec![("interval", interval("250ns"))],
            )),
        ),
        ran(
            "datetime_range by milliseconds in milliseconds",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2024-01-01 00:00:01"),
                vec![("interval", interval("250ms")), ("time_unit", choice("ms"))],
            )),
        ),
        ran(
            "datetime_range in a zone",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2024-01-05 00:00:00"),
                vec![("time_zone", json!({ "time_zone": "Europe/Berlin" }))],
            )),
        ),
        ran(
            "datetime_range by the day across a change of clock",
            datetime_range(with(
                at("2024-03-29 00:00:00", "2024-04-02 00:00:00"),
                vec![("time_zone", json!({ "time_zone": "Europe/Berlin" }))],
            )),
        ),
        ran(
            "datetime_range by the hour over the hour that is skipped",
            datetime_range(with(
                at("2024-03-31 00:00:00", "2024-03-31 05:00:00"),
                vec![
                    ("interval", interval("1h")),
                    ("time_zone", json!({ "time_zone": "Europe/Berlin" })),
                ],
            )),
        ),
        ran(
            "datetime_range by the hour over the hour that repeats",
            datetime_range(with(
                at("2024-10-27 00:00:00", "2024-10-27 05:00:00"),
                vec![
                    ("interval", interval("1h")),
                    ("time_zone", json!({ "time_zone": "Europe/Berlin" })),
                ],
            )),
        ),
        ran(
            "datetime_range in a fixed offset",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2024-01-02 00:00:00"),
                vec![
                    ("interval", interval("6h")),
                    ("time_zone", json!({ "time_zone": "+05:00" })),
                ],
            )),
        ),
        ran(
            "datetime_range of dates by twelve hours",
            datetime_range(with(
                vec![("start", date("2024-01-01")), ("end", date("2024-01-03"))],
                vec![("interval", interval("12h"))],
            )),
        ),
        ran(
            "datetime_range that starts after it ends",
            datetime_range(with(
                at("2024-01-02 00:00:00", "2024-01-01 00:00:00"),
                vec![("interval", interval("1h"))],
            )),
        ),
        ran(
            "datetime_range from a moment to itself",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2024-01-01 00:00:00"),
                vec![("interval", interval("1h"))],
            )),
        ),
        ran(
            "datetime_range of one moment closed left",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2024-01-01 00:00:00"),
                vec![("interval", interval("1h")), ("closed", choice("left"))],
            )),
        ),
        ran(
            "datetime_range closed none",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2024-01-02 00:00:00"),
                vec![("interval", interval("6h")), ("closed", choice("none"))],
            )),
        ),
        ran(
            "datetime_range closed right",
            datetime_range(with(
                at("2024-01-01 00:00:00", "2024-01-02 00:00:00"),
                vec![("interval", interval("6h")), ("closed", choice("right"))],
            )),
        ),
        // time_range
        ran(
            "time_range by the hour",
            time_range(clock("00:00:00", "05:00:00")),
        ),
        ran(
            "time_range by half hours",
            time_range(with(
                clock("00:00:00", "02:00:00"),
                vec![("interval", interval("30m"))],
            )),
        ),
        ran("time_range of the whole day", time_range(vec![])),
        ran(
            "time_range given only the start",
            time_range(vec![("start", time_of_day("23:00:00"))]),
        ),
        ran(
            "time_range given only the end",
            time_range(vec![("end", time_of_day("02:00:00"))]),
        ),
        ran(
            "time_range by nanoseconds",
            time_range(with(
                clock("00:00:00", "00:00:00.000001"),
                vec![("interval", interval("500ns"))],
            )),
        ),
        ran(
            "time_range that starts after it ends",
            time_range(clock("05:00:00", "00:00:00")),
        ),
        ran(
            "time_range from a time to itself",
            time_range(clock("05:00:00", "05:00:00")),
        ),
        ran(
            "time_range from a time to itself closed none",
            time_range(with(
                clock("05:00:00", "05:00:00"),
                vec![("closed", choice("none"))],
            )),
        ),
        ran(
            "time_range closed left",
            time_range(with(
                clock("00:00:00", "05:00:00"),
                vec![("closed", choice("left"))],
            )),
        ),
        ran(
            "time_range closed right",
            time_range(with(
                clock("00:00:00", "05:00:00"),
                vec![("closed", choice("right"))],
            )),
        ),
        ran(
            "time_range closed none",
            time_range(with(
                clock("00:00:00", "05:00:00"),
                vec![("closed", choice("none"))],
            )),
        ),
        // linear_space
        ran(
            "linear_space of five",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(1)),
                ("num_samples", whole(5)),
            ]),
        ),
        ran(
            "linear_space of none",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(10)),
                ("num_samples", whole(0)),
            ]),
        ),
        ran(
            "linear_space of one",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(10)),
                ("num_samples", whole(1)),
            ]),
        ),
        ran(
            "linear_space of one closed none",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(10)),
                ("num_samples", whole(1)),
                ("closed", choice("none")),
            ]),
        ),
        ran(
            "linear_space of two closed left",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(10)),
                ("num_samples", whole(2)),
                ("closed", choice("left")),
            ]),
        ),
        ran(
            "linear_space of three closed right",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(10)),
                ("num_samples", whole(3)),
                ("closed", choice("right")),
            ]),
        ),
        ran(
            "linear_space down",
            linear_space(vec![
                ("start", whole(10)),
                ("end", whole(0)),
                ("num_samples", whole(3)),
            ]),
        ),
        ran(
            "linear_space of a point",
            linear_space(vec![
                ("start", whole(5)),
                ("end", whole(5)),
                ("num_samples", whole(3)),
            ]),
        ),
        ran(
            "linear_space of dates",
            linear_space(vec![
                ("start", date("2024-01-01")),
                ("end", date("2024-01-10")),
                ("num_samples", whole(4)),
            ]),
        ),
        ran(
            "linear_space of moments",
            linear_space(vec![
                ("start", moment("2024-01-01 00:00:00")),
                ("end", moment("2024-01-10 00:00:00")),
                ("num_samples", whole(4)),
            ]),
        ),
        ran(
            "linear_space of a thousand",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(1)),
                ("num_samples", whole(1000)),
            ]),
        ),
        // repeat
        ran(
            "repeat three times",
            repeat(vec![("value", whole(1)), ("n", whole(3))]),
        ),
        ran(
            "repeat none",
            repeat(vec![("value", whole(1)), ("n", whole(0))]),
        ),
        ran(
            "repeat text",
            repeat(vec![
                (
                    "value",
                    chain(
                        call("polars.lit", json!({ "value": { "string": "a" } })),
                        vec![],
                    ),
                ),
                ("n", whole(4)),
            ]),
        ),
        ran(
            "repeat a null",
            repeat(vec![
                (
                    "value",
                    chain(
                        call("polars.lit", json!({ "value": { "flag": null } })),
                        vec![],
                    ),
                ),
                ("n", whole(2)),
            ]),
        ),
        ran(
            "repeat as another dtype",
            repeat(vec![
                ("value", whole(1)),
                ("n", whole(4)),
                ("dtype", json!({ "dtype": "Float32" })),
            ]),
        ),
        ran(
            "repeat a date",
            repeat(vec![("value", date("2024-01-01")), ("n", whole(2))]),
        ),
        ran(
            "repeat a hundred thousand",
            repeat(vec![("value", whole(1)), ("n", whole(100_000))]),
        ),
    ];
    all.sort_by_key(|case| case.name);
    all
}

#[test]
fn a_range_typed_in_gives_the_count_polars_gives_it() {
    let mut disagreements = Vec::new();
    let mut by_hand = 0;
    let mut panicked = Vec::new();
    for case in counts_typed_in() {
        // A build of Polars with debug assertions panics where the release does not.
        let Ok(report) = std::panic::catch_unwind(|| {
            // What is too long to run is planned and not run.
            if case.by_hand.is_some() {
                planned(
                    "LazyFrame.select",
                    json!({ "exprs": many(vec![case.call.clone()]) }),
                )
            } else {
                select_of(vec![case.call.clone()])
            }
        }) else {
            panicked.push(case.name);
            continue;
        };
        let said = walk_says(&report);
        let counted = item(&report, 0).count();
        match case.by_hand {
            // Too many values to run: the number is worked out by hand, which the walk's must be.
            Some(count) => {
                by_hand += 1;
                if counted != Some(Count::Exactly(count)) {
                    disagreements.push(format!(
                        "{}: the walk says {counted:?}, the number worked out is {count}",
                        case.name
                    ));
                }
            }
            None => {
                let polars = polars_says(&report);
                let StepRows::Counted(rows) = report.steps()[0].rows() else {
                    disagreements.push(format!(
                        "{}: Polars did not run it, {polars:?}, and the walk says {said:?}",
                        case.name
                    ));
                    continue;
                };
                if counted != Some(Count::Exactly(rows.kept() as u64)) {
                    disagreements.push(format!(
                        "{}: Polars gives {} values and the walk says {counted:?}",
                        case.name,
                        rows.kept()
                    ));
                }
                if !said.accepts() {
                    disagreements.push(format!("{}: the walk refuses it, {said:?}", case.name));
                }
            }
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} cases disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
    assert_eq!(by_hand, 4, "the cases too large to run are named");
    // Polars asserts, in a build that has debug assertions, that the values of a range of days in a
    // zone are no more than its length over the length of a day, which a day of 23 hours, where the
    // clocks go forward, breaks: Python Polars 2.1.0 gives 5 values of this case.
    assert_eq!(
        panicked,
        ["datetime_range by the day across a change of clock"],
        "the cases Polars panics on"
    );
}

// ---------------------------------------------------------------------------
// A count of one

#[test]
fn a_count_of_exactly_one_beside_the_rows_is_refused_though_the_engine_that_runs_steps_takes_it() {
    // Exception, stricter by design: the streaming engine, which the steps run on, broadcasts a
    // range or a repeat of one value to the rows, where Python Polars 2.1.0's in-memory engine
    // refuses it with "Series literal, length 1 doesn't match the DataFrame height of 7". A count
    // is not one value, and the editor refuses it as the in-memory engine does.
    let ones = [
        (
            "int_range",
            int_range(vec![("start", whole(0)), ("end", whole(1))]),
        ),
        ("repeat", repeat(vec![("value", whole(1)), ("n", whole(1))])),
        (
            "linear_space",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(1)),
                ("num_samples", whole(1)),
            ]),
        ),
    ];
    let mut found = Vec::new();
    for (name, start) in ones {
        let report = select_of(vec![field("k"), start]);
        if !polars_says(&report).accepts() {
            found.push(format!(
                "{name}: the engine of steps refuses it too, which would end this exception"
            ));
        }
        if walk_says(&report) != Said::Refuses("values-meet-in-count".to_owned()) {
            found.push(format!("{name}: the walk says {:?}", walk_says(&report)));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

// ---------------------------------------------------------------------------
// Counts that cannot be worked out

#[test]
fn a_range_whose_arguments_are_not_typed_in_has_a_count_that_is_not_known() {
    // A column's first value is one value, whose number is the data's: `n` holds the number 3.
    let first = |name: &str| field_then(name, vec![call("Expr.first", json!({}))]);
    let cases: Vec<(&str, Value)> = vec![
        (
            "int_range of a column's end",
            int_range(vec![("start", whole(0)), ("end", first("a"))]),
        ),
        (
            "int_range of a column's start",
            int_range(vec![("start", first("a")), ("end", whole(9))]),
        ),
        (
            "date_range of a column's start",
            date_range(vec![
                (
                    "start",
                    field_then("d", vec![call("Expr.first", json!({}))]),
                ),
                ("end", date("2024-01-10")),
            ]),
        ),
        (
            "datetime_range of a column's end",
            datetime_range(vec![
                ("start", moment("2024-01-01 00:00:00")),
                ("end", field_then("dt", vec![call("Expr.max", json!({}))])),
            ]),
        ),
        (
            "time_range of a column's start",
            time_range(vec![(
                "start",
                field_then("tm", vec![call("Expr.min", json!({}))]),
            )]),
        ),
        (
            "linear_space of a column's count",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(1)),
                ("num_samples", first("n")),
            ]),
        ),
        (
            "repeat a column's first value as many times as it says",
            repeat(vec![("value", whole(1)), ("n", first("n"))]),
        ),
        (
            "int_range of a count not an integer",
            int_range(vec![
                ("start", lit(json!({ "value": { "float": 0.5 } }))),
                ("end", whole(3)),
            ]),
        ),
        (
            "linear_space of a count not an integer",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(1)),
                ("num_samples", lit(json!({ "value": { "float": 2.5 } }))),
            ]),
        ),
        (
            "int_range of a number typed as a dtype",
            int_range(vec![
                (
                    "start",
                    lit(json!({ "value": { "integer": 0 }, "dtype": { "dtype": "Int8" } })),
                ),
                ("end", whole(3)),
            ]),
        ),
    ];
    let mut disagreements = Vec::new();
    for (name, start) in cases {
        let report = select_of(vec![start]);
        if let Some(refusal) = report.steps()[0].walk().refusal() {
            disagreements.push(format!("{name}: refused, {:?}", refusal.refusal().reason()));
            continue;
        }
        let counted = item(&report, 0).count();
        if counted != Some(Count::NotKnown) {
            disagreements.push(format!(
                "{name}: the walk says {counted:?}, not that it is not known"
            ));
        }
        if !polars_says(&report).accepts() {
            disagreements.push(format!(
                "{name}: Polars refuses it, {:?}",
                polars_says(&report)
            ));
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} cases disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}

// ---------------------------------------------------------------------------
// What no arguments make

/// The calls that Polars refuses whatever the rows are, because what is typed in makes no range:
/// each with what it is refused for, which the editor says where the step is walked. The rule
/// that says it is the one range rule of the engine.
fn never_a_range() -> Vec<(&'static str, Value)> {
    let both = |first: &str, last: &str| vec![("start", date(first)), ("end", date(last))];
    let with = |mut arguments: Vec<(&'static str, Value)>, more: Vec<(&'static str, Value)>| {
        arguments.extend(more);
        arguments
    };
    vec![
        (
            "a step of nothing",
            int_range(vec![
                ("start", whole(0)),
                ("end", whole(3)),
                ("step", json!({ "integer": 0 })),
            ]),
        ),
        (
            "an unsigned byte below nothing",
            int_range(vec![
                ("start", whole(-1)),
                ("end", whole(3)),
                ("dtype", json!({ "dtype": "UInt8" })),
            ]),
        ),
        (
            "an unsigned byte past 255",
            int_range(vec![
                ("start", whole(0)),
                ("end", whole(256)),
                ("dtype", json!({ "dtype": "UInt8" })),
            ]),
        ),
        (
            "a signed byte past 127",
            int_range(vec![
                ("start", whole(0)),
                ("end", whole(128)),
                ("dtype", json!({ "dtype": "Int8" })),
            ]),
        ),
        (
            "a start past the dtype of an empty range",
            int_range(vec![
                ("start", whole(300)),
                ("end", whole(300)),
                ("dtype", json!({ "dtype": "UInt8" })),
            ]),
        ),
        (
            "a dtype that is no whole number",
            int_range(vec![
                ("start", whole(0)),
                ("end", whole(3)),
                ("dtype", json!({ "dtype": "Float64" })),
            ]),
        ),
        (
            "a null start",
            int_range(vec![
                ("start", lit(json!({ "value": { "flag": null } }))),
                ("end", whole(3)),
            ]),
        ),
        (
            "days that are no whole days",
            date_range(with(
                both("2024-01-01", "2024-01-10"),
                vec![("interval", interval("12h"))],
            )),
        ),
        (
            "a day and an hour",
            date_range(with(
                both("2024-01-01", "2024-01-10"),
                vec![("interval", interval("1d1h"))],
            )),
        ),
        (
            "an interval of nothing",
            date_range(with(
                both("2024-01-01", "2024-01-10"),
                vec![("interval", interval("0d"))],
            )),
        ),
        (
            "an interval that is no duration",
            date_range(with(
                both("2024-01-01", "2024-01-10"),
                vec![("interval", interval("xyz"))],
            )),
        ),
        (
            "an empty interval",
            date_range(with(
                both("2024-01-01", "2024-01-10"),
                vec![("interval", interval(""))],
            )),
        ),
        (
            "an interval below nothing that goes forward",
            date_range(with(
                both("2024-01-01", "2024-01-10"),
                vec![("interval", interval("-1d"))],
            )),
        ),
        (
            "a null start of dates",
            date_range(vec![
                (
                    "start",
                    lit(json!({ "value": { "flag": null }, "dtype": { "dtype": "Date" } })),
                ),
                ("end", date("2024-01-10")),
            ]),
        ),
        (
            "a start that is a number",
            date_range(vec![("start", whole(0)), ("end", whole(5))]),
        ),
        (
            "a start that is text",
            date_range(vec![
                ("start", lit(json!({ "value": { "string": "2024-01-01" } }))),
                ("end", date("2024-01-10")),
            ]),
        ),
        (
            "moments in a zone that does not exist",
            datetime_range(with(
                vec![
                    ("start", moment("2024-01-01 00:00:00")),
                    ("end", moment("2024-01-02 00:00:00")),
                ],
                vec![("time_zone", json!({ "time_zone": "Nope/Nope" }))],
            )),
        ),
        (
            "an hour that is none of a zone's",
            datetime_range(with(
                vec![
                    ("start", moment("2024-03-31 02:30:00")),
                    ("end", moment("2024-03-31 05:00:00")),
                ],
                vec![
                    ("interval", interval("1h")),
                    ("time_zone", json!({ "time_zone": "Europe/Berlin" })),
                ],
            )),
        ),
        (
            "an hour that is two of a zone's",
            datetime_range(with(
                vec![
                    ("start", moment("2024-10-27 02:30:00")),
                    ("end", moment("2024-10-27 05:00:00")),
                ],
                vec![
                    ("interval", interval("1h")),
                    ("time_zone", json!({ "time_zone": "Europe/Berlin" })),
                ],
            )),
        ),
        (
            "an interval of nanoseconds in microseconds",
            datetime_range(with(
                vec![
                    ("start", moment("2024-01-01 00:00:00")),
                    ("end", moment("2024-01-01 00:00:01")),
                ],
                vec![("interval", interval("500ns")), ("time_unit", choice("us"))],
            )),
        ),
        (
            "an interval of microseconds in milliseconds",
            datetime_range(with(
                vec![
                    ("start", moment("2024-01-01 00:00:00")),
                    ("end", moment("2024-01-01 00:00:01")),
                ],
                vec![("interval", interval("1us")), ("time_unit", choice("ms"))],
            )),
        ),
        (
            "a start that is a number",
            datetime_range(vec![("start", whole(0)), ("end", whole(5))]),
        ),
        (
            "a day of a time",
            time_range(with(vec![], vec![("interval", interval("1d"))])),
        ),
        (
            "a month of a time",
            time_range(with(vec![], vec![("interval", interval("1mo"))])),
        ),
        (
            "a week of a time",
            time_range(with(vec![], vec![("interval", interval("1w"))])),
        ),
        (
            "a year of a time",
            time_range(with(vec![], vec![("interval", interval("1y"))])),
        ),
        (
            "an hour and a day of a time",
            time_range(with(vec![], vec![("interval", interval("1h1d"))])),
        ),
        (
            "a time interval below nothing that goes forward",
            time_range(with(
                vec![
                    ("start", time_of_day("00:00:00")),
                    ("end", time_of_day("05:00:00")),
                ],
                vec![("interval", interval("-1h"))],
            )),
        ),
        (
            "a date for a time",
            time_range(vec![
                ("start", date("2024-01-01")),
                ("end", date("2024-01-02")),
            ]),
        ),
        (
            "samples below nothing",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(1)),
                ("num_samples", whole(-1)),
            ]),
        ),
        (
            "a null count of samples",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(1)),
                ("num_samples", lit(json!({ "value": { "flag": null } }))),
            ]),
        ),
        (
            "a start that is text",
            linear_space(vec![
                ("start", lit(json!({ "value": { "string": "a" } }))),
                ("end", lit(json!({ "value": { "string": "b" } }))),
                ("num_samples", whole(3)),
            ]),
        ),
        (
            "a date and a moment",
            linear_space(vec![
                ("start", date("2024-01-01")),
                ("end", moment("2024-01-10 00:00:00")),
                ("num_samples", whole(4)),
            ]),
        ),
        (
            "repeats below nothing",
            repeat(vec![("value", whole(1)), ("n", whole(-1))]),
        ),
        (
            "a null count of repeats",
            repeat(vec![
                ("value", whole(1)),
                ("n", lit(json!({ "value": { "flag": null } }))),
            ]),
        ),
        (
            "a count of repeats that is text",
            repeat(vec![
                ("value", whole(1)),
                ("n", lit(json!({ "value": { "string": "2" } }))),
            ]),
        ),
    ]
}

#[test]
fn what_polars_never_makes_a_range_of_is_refused_where_the_step_is_walked() {
    let mut disagreements = Vec::new();
    for (name, start) in never_a_range() {
        let report = select_of(vec![start]);
        let (polars, walk) = (polars_says(&report), walk_says(&report));
        if polars.accepts() {
            disagreements.push(format!("{name}: Polars runs it, and the case is wrong"));
        }
        if walk.accepts() {
            disagreements.push(format!(
                "{name}: Polars refuses it ({polars:?}), and the walk does not"
            ));
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} cases disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}

#[test]
fn a_range_of_more_values_than_polars_holds_is_refused_where_the_step_is_walked() {
    // Polars is built with an index of 32 bits, so a column of more than 4294967295 values panics
    // with "Polars' maximum length reached": the plan is built, which no rows are run on.
    let too_many = [
        (
            "int_range of one past the most",
            int_range(vec![("start", whole(0)), ("end", whole(4_294_967_296))]),
        ),
        (
            "int_range down to the most",
            int_range(vec![
                ("start", whole(4_294_967_296)),
                ("end", whole(0)),
                ("step", json!({ "integer": -1 })),
            ]),
        ),
        (
            "int_range from the smallest to the largest",
            int_range(vec![("start", whole(i64::MIN)), ("end", whole(i64::MAX))]),
        ),
        (
            "repeat one past the most",
            repeat(vec![("value", whole(1)), ("n", whole(4_294_967_296))]),
        ),
        (
            "linear_space of one past the most",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(1)),
                ("num_samples", whole(4_294_967_296)),
            ]),
        ),
    ];
    let mut found = Vec::new();
    for (name, start) in too_many {
        let report = planned("LazyFrame.select", json!({ "exprs": many(vec![start]) }));
        if walk_says(&report) != Said::Refuses("range-is-valid".to_owned()) {
            found.push(format!("{name}: the walk says {:?}", walk_says(&report)));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn a_join_pairs_the_output_names_of_two_ranges_as_polars_names_them() {
    // Python Polars 2.1.0 names what a range gives by the first expression it is given, `literal`
    // where that is a number typed in or none is given, and what `repeat` gives by its value. A join
    // refuses a pair of names made twice: "joining with repeated key names". A key of a count is
    // not elementwise, so each key goes on to an aggregate of the values, which is one value.
    let minimum = |start: &Value| on_start(start, vec![call("Expr.arg_min", json!({}))]);
    let first = |name: &str| field_then(name, vec![call("Expr.first", json!({}))]);
    let cases = [
        ("int_range of nothing", int_range(vec![])),
        ("int_range of an end", int_range(vec![("start", whole(2))])),
        (
            "int_range of a column",
            int_range(vec![("start", first("a")), ("end", whole(9))]),
        ),
        (
            "linear_space of numbers",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(1)),
                ("num_samples", whole(3)),
            ]),
        ),
        (
            "repeat of a column",
            repeat(vec![("value", first("a")), ("n", whole(3))]),
        ),
    ];
    let mut found = Vec::new();
    for (name, range) in cases {
        let key = minimum(&range);
        let report = run(
            "LazyFrame.join",
            json!({
                "other": { "table": "other" },
                "left_on": many(vec![key.clone(), key]),
                "right_on": many(vec![field("rk"), field("rk")]),
            }),
        );
        let (polars, walk) = (polars_says(&report), walk_says(&report));
        if polars.accepts() || walk != Said::Refuses("join-key-names-are-not-repeated".to_owned()) {
            found.push(format!(
                "{name}: Polars says {polars:?}, the walk says {walk:?}"
            ));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn a_range_is_always_lazy_and_never_offers_eager() {
    // Polars' `eager` makes a series where the engine's steps make plans: the flag is in the
    // signature, which the catalogue holds as Python writes it, and a step that sets it is
    // refused where it is built.
    let ids = [
        "polars.int_range",
        "polars.date_range",
        "polars.datetime_range",
        "polars.time_range",
        "polars.linear_space",
        "polars.repeat",
    ];
    for id in ids {
        let entry = catalog::entry(id).unwrap_or_else(|| panic!("the catalogue has no {id}"));
        let eager = entry
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == "eager")
            .unwrap_or_else(|| panic!("{id} has no eager"));
        assert!(!eager.offered(), "{id} offers eager");
        assert_eq!(entry.applies(), Applies::Function, "{id}");
    }
    let report = select_of(vec![repeat(vec![
        ("value", whole(1)),
        ("n", whole(3)),
        ("eager", json!({ "flag": true })),
    ])]);
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Refused(_)),
        "a step of `eager` is refused: {:?}",
        report.steps()[0].outcome()
    );
}

// ---------------------------------------------------------------------------
// Every expression parameter, in every place

/// A function that starts a range, with what its expression parameters are given where they are
/// not the one that is tested: each is a value typed in.
struct Range {
    id: &'static str,
    /// The parameters of expressions, each with the column of the table that gives a value of the
    /// dtype it takes, and the number of values the call gives where each is typed in.
    parameters: Vec<(&'static str, &'static str)>,
    typed: Vec<(&'static str, Value)>,
}

fn ranges() -> Vec<Range> {
    vec![
        Range {
            id: "polars.int_range",
            parameters: vec![("start", "a"), ("end", "a")],
            typed: vec![("start", whole(0)), ("end", whole(4))],
        },
        Range {
            id: "polars.date_range",
            parameters: vec![("start", "d"), ("end", "d")],
            typed: vec![("start", date("2024-01-01")), ("end", date("2024-01-04"))],
        },
        Range {
            id: "polars.datetime_range",
            parameters: vec![("start", "dt"), ("end", "dt")],
            typed: vec![
                ("start", moment("2024-01-01 00:00:00")),
                ("end", moment("2024-01-01 03:00:00")),
                ("interval", interval("1h")),
            ],
        },
        Range {
            id: "polars.time_range",
            parameters: vec![("start", "tm"), ("end", "tm")],
            typed: vec![
                ("start", time_of_day("01:00:00")),
                ("end", time_of_day("04:00:00")),
            ],
        },
        Range {
            id: "polars.linear_space",
            parameters: vec![("start", "x"), ("end", "x"), ("num_samples", "n")],
            typed: vec![
                ("start", whole(0)),
                ("end", whole(10)),
                ("num_samples", whole(4)),
            ],
        },
        Range {
            id: "polars.repeat",
            parameters: vec![("value", "a"), ("n", "n")],
            typed: vec![("value", whole(1)), ("n", whole(4))],
        },
    ]
}

fn on_column(column: &str, links: Vec<Value>) -> Value {
    field_then(column, links)
}

fn head(count: i64) -> Value {
    call("Expr.head", json!({ "n": whole(count) }))
}

/// The forms an argument takes: one value, the one value that `head` took of one value, a count
/// of two values, and the rows.
fn forms(column: &str) -> Vec<(&'static str, Value)> {
    vec![
        (
            "one value",
            on_column(column, vec![call("Expr.first", json!({}))]),
        ),
        (
            "a value taken of one value",
            on_column(column, vec![call("Expr.first", json!({})), head(1)]),
        ),
        ("a count of values", on_column(column, vec![head(2)])),
        ("the rows", field(column)),
    ]
}

/// The places that hold a start, each a step of a document that is built from the start, with
/// what it holds named: where the values it gives are one count, beside the rows, a fill, a key.
fn places(start: &Value) -> Vec<(&'static str, Report)> {
    vec![
        ("a select", select_of(vec![start.clone()])),
        (
            "a select beside the rows",
            select_of(vec![field("k"), start.clone()]),
        ),
        (
            "a step that adds a column",
            run(
                "LazyFrame.with_columns",
                json!({ "exprs": many(vec![start.clone()]) }),
            ),
        ),
        (
            "the fill of a shift",
            select_of(vec![chain(
                call("polars.col", json!({ "name": { "columns": ["a"] } })),
                vec![
                    call("Expr.first", json!({})),
                    call(
                        "Expr.shift",
                        json!({ "n": whole(1), "fill_value": start.clone() }),
                    ),
                ],
            )]),
        ),
        (
            "the rows of a shift",
            select_of(vec![on_column(
                "a",
                vec![call("Expr.shift", json!({ "n": start.clone() }))],
            )]),
        ),
        (
            "a sort key",
            run("LazyFrame.sort", json!({ "by": many(vec![start.clone()]) })),
        ),
        (
            "a join key",
            run(
                "LazyFrame.join",
                json!({
                    "other": { "table": "other" },
                    "left_on": many(vec![start.clone()]),
                    "right_on": many(vec![field("rk")]),
                }),
            ),
        ),
        (
            "the key of a window",
            select_of(vec![chain(
                call("polars.col", json!({ "name": { "columns": ["a"] } })),
                vec![
                    call("Expr.sum", json!({})),
                    call(
                        "Expr.over",
                        json!({ "partition_by": many(vec![start.clone()]) }),
                    ),
                ],
            )]),
        ),
        (
            "a window over the values",
            select_of(vec![chain(
                start_call(start),
                vec![call(
                    "Expr.over",
                    json!({ "partition_by": many(vec![field("k")]) }),
                )],
            )]),
        ),
        (
            "a filter",
            run(
                "LazyFrame.filter",
                json!({ "predicates": many(vec![start.clone()]) }),
            ),
        ),
        (
            "an aggregate of the values",
            select_of(vec![on_start(start, vec![call("Expr.sum", json!({}))])]),
        ),
        (
            "an aggregate of the values beside a column",
            select_of(vec![
                field("k"),
                on_start(start, vec![call("Expr.sum", json!({}))]),
            ]),
        ),
        (
            "the first value beside the rows",
            select_of(vec![
                field("k"),
                on_start(start, vec![call("Expr.first", json!({}))]),
            ]),
        ),
        (
            "a sum of the rows beside the values",
            select_of(vec![
                field_then("k", vec![call("Expr.sum", json!({}))]),
                start.clone(),
            ]),
        ),
    ]
}

/// The start of the chain `expression`, as a call.
fn start_call(expression: &Value) -> Value {
    expression["expression"]["start"].clone()
}

/// `expression` followed by `links`.
fn on_start(expression: &Value, links: Vec<Value>) -> Value {
    chain(start_call(expression), links)
}

/// The expression with `parameter` of the start given `form`, and the others typed in.
fn with_form(range: &Range, parameter: &str, form: &Value) -> Value {
    let arguments: Vec<(&str, Value)> = range
        .typed
        .iter()
        .map(|(name, value)| {
            if *name == parameter {
                (*name, form.clone())
            } else {
                (*name, value.clone())
            }
        })
        .collect();
    start_of(range.id, arguments)
}

/// What the editor refuses where Polars does not, on purpose, and why. The key is the id of the
/// function, the parameter, the form and the place. A count equal to the rows by chance is the
/// editor's to refuse, because a count that the data decide is not the table's rows.
const STRICTER_BY_DESIGN: [(&str, &str); 0] = [];

#[test]
fn every_parameter_of_a_range_is_given_one_value_as_polars_asks_in_every_place() {
    let mut disagreements = Vec::new();
    let mut agreed = 0;
    let mut stricter: BTreeMap<String, String> = BTreeMap::new();
    for range in ranges() {
        // The call as it is typed, in every place, which no parameter is under test of.
        let mut cases: Vec<(String, Value)> = vec![(
            format!("{} typed in", range.id),
            start_of(range.id, range.typed.clone()),
        )];
        for (parameter, column) in &range.parameters {
            for (form, given) in forms(column) {
                cases.push((
                    format!("{} {parameter} given {form}", range.id),
                    with_form(&range, parameter, &given),
                ));
            }
        }
        for (name, start) in cases {
            for (place, report) in places(&start) {
                let (polars, walk) = (polars_says(&report), walk_says(&report));
                let key = format!("{name} in {place}");
                match (polars.accepts(), walk.accepts()) {
                    (true, true) | (false, false) => agreed += 1,
                    (true, false) => {
                        if STRICTER_BY_DESIGN.iter().any(|(listed, _)| *listed == key) {
                            stricter.insert(key, format!("{walk:?}"));
                        } else {
                            disagreements.push(format!(
                                "{key}: Polars runs it and the walk refuses it, {walk:?}"
                            ));
                        }
                    }
                    (false, true) => disagreements.push(format!(
                        "{key}: Polars refuses it, {polars:?}, and the walk does not"
                    )),
                }
            }
        }
    }
    eprintln!("agreed {agreed}, stricter by design {}", stricter.len());
    assert!(
        disagreements.is_empty(),
        "{} cases disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
    assert!(agreed > 400, "agreed {agreed}");
}

#[test]
fn every_expression_parameter_of_a_range_is_in_the_sweep() {
    // The sweep is of the parameters the catalogue holds: a parameter that it holds and the
    // sweep does not give a column is not asked of Polars.
    let swept: BTreeMap<&str, Vec<&str>> = ranges()
        .into_iter()
        .map(|range| {
            (
                range.id,
                range.parameters.iter().map(|(name, _)| *name).collect(),
            )
        })
        .collect();
    for (id, names) in &swept {
        let entry = catalog::entry(id).unwrap_or_else(|| panic!("the catalogue has no {id}"));
        let held: Vec<&str> = entry
            .parameters()
            .iter()
            .filter(|parameter| matches!(parameter.kind(), catalog::Kind::Expression(_)))
            .map(|parameter| parameter.name())
            .collect();
        assert_eq!(&held, names, "the expression parameters of {id}");
    }
}

// ---------------------------------------------------------------------------
// What the editor refuses that Polars takes, pinned

/// The refusal of the walk by the name of its rule.
fn refused_by(report: &Report, rule: &str) -> bool {
    walk_says(report) == Said::Refuses(rule.to_owned())
}

#[test]
fn an_interval_of_nothing_is_refused_where_polars_gives_no_values_for_the_end_before_the_start() {
    // Stricter by design: Polars gives no values where the end is before the start, whatever the
    // interval is, and refuses an interval of nothing or below nothing only where it is not. The
    // editor is strict where the data decide, so it refuses the interval whatever the bounds are.
    let backwards = |first: &str, last: &str| vec![("start", date(first)), ("end", date(last))];
    let with = |mut arguments: Vec<(&'static str, Value)>, interval_text: &str| {
        arguments.push(("interval", interval(interval_text)));
        arguments
    };
    let cases = [
        (
            "date_range of nothing",
            date_range(with(backwards("2024-02-01", "2024-01-01"), "0d")),
        ),
        (
            "date_range below nothing",
            date_range(with(backwards("2024-02-01", "2024-01-01"), "-1d")),
        ),
        (
            "datetime_range of nothing",
            datetime_range(with(
                vec![
                    ("start", moment("2024-01-02 00:00:00")),
                    ("end", moment("2024-01-01 00:00:00")),
                ],
                "0h",
            )),
        ),
        (
            "time_range below nothing",
            time_range(with(
                vec![
                    ("start", time_of_day("05:00:00")),
                    ("end", time_of_day("00:00:00")),
                ],
                "-1h",
            )),
        ),
    ];
    let mut found = Vec::new();
    for (name, start) in cases {
        let report = select_of(vec![start]);
        let StepRows::Counted(rows) = report.steps()[0].rows() else {
            found.push(format!(
                "{name}: Polars does not give the table: {:?}",
                polars_says(&report)
            ));
            continue;
        };
        if rows.kept() != 0 {
            found.push(format!("{name}: Polars gives {} values", rows.kept()));
        }
        if !refused_by(&report, "range-is-valid") {
            found.push(format!("{name}: the walk says {:?}", walk_says(&report)));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn a_count_of_exactly_one_is_refused_beside_every_kind_of_place_that_polars_broadcasts_it_to() {
    // Stricter by design, as for a select beside the rows: the streaming engine meets a count of one
    // with each of the values of an operand, a fill, an exponent and a share, where Python's
    // in-memory engine refuses it. A count is not one value, and the editor refuses all of them.
    let one = || int_range(vec![("start", whole(0)), ("end", whole(1))]);
    let four = || int_range(vec![("start", whole(0)), ("end", whole(4))]);
    let share = linear_space(vec![
        ("start", whole(0)),
        ("end", whole(1)),
        ("num_samples", whole(1)),
    ]);
    let cases = [
        (
            "an operand",
            on_start(&four(), vec![call("Expr.add", json!({ "other": one() }))]),
        ),
        (
            "a fill",
            on_column("a", vec![call("Expr.fill_null", json!({ "value": one() }))]),
        ),
        (
            "an exponent",
            on_column("a", vec![call("Expr.pow", json!({ "exponent": one() }))]),
        ),
        (
            "a share",
            on_column(
                "a",
                vec![call("Expr.quantile", json!({ "quantile": share }))],
            ),
        ),
    ];
    let mut found = Vec::new();
    for (name, expression) in cases {
        let report = select_of(vec![expression]);
        if !polars_says(&report).accepts() {
            found.push(format!(
                "{name}: the engine of steps refuses it too, which ends this exception: {:?}",
                polars_says(&report)
            ));
        }
        if walk_says(&report).accepts() {
            found.push(format!("{name}: the walk takes it"));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

// ---------------------------------------------------------------------------
// A condition of identical branches

#[test]
fn a_condition_of_identical_branches_drops_its_predicate_only_where_they_are_one_value() {
    // Python Polars 2.1.0, in both engines: where both values of a condition are the same one
    // value, the predicate is met by it, and the condition is as long as the predicate; where they
    // are the same many values or the same count, a predicate that is not as many refuses it with
    // "shapes of `self`, `mask` and `other` are not suitable for `zip_with`" or "zip node received
    // non-equal length inputs".
    let when = |predicate: Value, branch: Value| {
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![predicate]) }),
            ),
            vec![
                call("When.then", json!({ "statement": branch.clone() })),
                call("Then.otherwise", json!({ "statement": branch })),
            ],
        )
    };
    let four = || int_range(vec![("start", whole(0)), ("end", whole(4))]);
    let sum = || on_column("a", vec![call("Expr.sum", json!({}))]);
    let cases = [
        ("a count in both", when(field("f"), four()), false),
        (
            "a count as the predicate",
            when(
                on_start(&four(), vec![call("Expr.gt", json!({ "other": whole(1) }))]),
                field("a"),
            ),
            false,
        ),
        (
            "the first four values in both",
            when(field("f"), on_column("a", vec![head(4)])),
            false,
        ),
        (
            "the first two flags as the predicate",
            when(on_column("f", vec![head(2)]), field("a")),
            false,
        ),
        ("the rows in both", when(field("f"), field("a")), true),
        ("a number in both", when(field("f"), whole(1)), true),
        ("a sum in both", when(field("f"), sum()), true),
    ];
    let mut found = Vec::new();
    for (name, condition, allowed) in cases {
        let report = select_of(vec![condition]);
        let (polars, walk) = (polars_says(&report), walk_says(&report));
        if polars.accepts() != allowed || walk.accepts() != allowed {
            found.push(format!(
                "{name}: Polars says {polars:?}, the walk says {walk:?}, and it is {}",
                if allowed { "allowed" } else { "refused" }
            ));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

// ---------------------------------------------------------------------------
// A range that is not asked for what no bound is known of

#[test]
fn a_range_of_times_is_not_asked_of_polars_where_its_bounds_are_not_read() {
    // The walk runs on every edit, so it never makes a range it has not found the length of: bounds
    // of dtypes that are not read, whole numbers that are unsigned, of 128 bits, and floats, make a
    // range of 25 million nanoseconds a count that is not known, as one of `Int64` is.
    let bound = |dtype: Option<&str>, number: Value| match dtype {
        Some(dtype) => lit(json!({ "value": number, "dtype": { "dtype": dtype } })),
        None => lit(json!({ "value": number })),
    };
    let cases = [
        ("UInt64", Some("UInt64")),
        ("UInt32", Some("UInt32")),
        ("UInt16", Some("UInt16")),
        ("Int128", Some("Int128")),
        ("Float32", Some("Float32")),
        ("Float64", Some("Float64")),
        ("a float typed without a dtype", None),
        ("Int64", Some("Int64")),
    ];
    let mut found = Vec::new();
    for (name, dtype) in cases {
        let number = |at: i64| match dtype {
            Some("Float32" | "Float64") | None => json!({ "float": at as f64 }),
            Some(_) => json!({ "integer": at }),
        };
        let end = if dtype == Some("UInt16") {
            60_000
        } else {
            25_000_000
        };
        let start = time_range(vec![
            ("start", bound(dtype, number(0))),
            ("end", bound(dtype, number(end))),
            ("interval", interval("1ns")),
        ]);
        let report = planned("LazyFrame.select", json!({ "exprs": many(vec![start]) }));
        let counted = item(&report, 0).count();
        // 60,000 values are made and counted; more than a million are not asked.
        let expected = if dtype == Some("UInt16") {
            Some(Count::Exactly(60_001))
        } else {
            Some(Count::NotKnown)
        };
        if counted != expected {
            found.push(format!(
                "{name}: the walk says {counted:?}, not {expected:?}"
            ));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

// ---------------------------------------------------------------------------
// A length below nothing after a range

#[test]
fn a_range_is_not_cut_by_a_length_below_nothing_as_polars_refuses_it() {
    // Python Polars 2.1.0, in both engines: "unable to extract length from Scalar(... -2)" of the
    // first, last and `limit` values and of a slice with a length, where the receiver is a range
    // or a repeat. The same calls of a column run, and so do the places counted from the end.
    let ranges = [
        (
            "int_range",
            int_range(vec![("start", whole(0)), ("end", whole(10))]),
        ),
        (
            "repeat",
            repeat(vec![("value", whole(7)), ("n", whole(10))]),
        ),
        (
            "linear_space",
            linear_space(vec![
                ("start", whole(0)),
                ("end", whole(1)),
                ("num_samples", whole(10)),
            ]),
        ),
    ];
    let cuts = [
        ("head", call("Expr.head", json!({ "n": whole(-2) })), false),
        ("tail", call("Expr.tail", json!({ "n": whole(-2) })), false),
        (
            "limit",
            call("Expr.limit", json!({ "n": whole(-2) })),
            false,
        ),
        (
            "slice with a length",
            call(
                "Expr.slice",
                json!({ "offset": whole(2), "length": whole(-1) }),
            ),
            false,
        ),
        (
            "head of some",
            call("Expr.head", json!({ "n": whole(2) })),
            true,
        ),
        (
            "slice from the end",
            call("Expr.slice", json!({ "offset": whole(-2) })),
            true,
        ),
        (
            "slice from the end and a length",
            call(
                "Expr.slice",
                json!({ "offset": whole(-2), "length": whole(3) }),
            ),
            true,
        ),
    ];
    let mut found = Vec::new();
    for (range, start) in &ranges {
        for (cut, link, allowed) in &cuts {
            let report = select_of(vec![on_start(start, vec![link.clone()])]);
            let (polars, walk) = (polars_says(&report), walk_says(&report));
            if polars.accepts() != *allowed || walk.accepts() != *allowed {
                found.push(format!(
                    "{cut} of {range}: Polars says {polars:?}, the walk says {walk:?}"
                ));
            }
        }
    }
    for (cut, link, _) in &cuts[..4] {
        // The same calls of a column of the table run, as the engine of steps runs them.
        let report = select_of(vec![on_column("a", vec![link.clone()])]);
        if !polars_says(&report).accepts() || !walk_says(&report).accepts() {
            found.push(format!(
                "{cut} of a column: Polars says {:?}, the walk says {:?}",
                polars_says(&report),
                walk_says(&report)
            ));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

// ---------------------------------------------------------------------------
// A repeat of a value of a dtype that a number does not cast to

#[test]
fn a_repeat_of_a_list_or_an_array_is_taken_as_polars_takes_it() {
    // Python Polars 2.1.0: `repeat(l.first(), 3)` of a column of lists gives three lists, and so do
    // arrays. The tables made one value of a dtype by casting a number to it, which no list, array
    // or struct takes, and refused them with a reason that is not true of the value.
    let mut found = Vec::new();
    for column in ["l", "arr"] {
        let value = on_column(column, vec![call("Expr.first", json!({}))]);
        let report = select_of(vec![repeat(vec![("value", value), ("n", whole(3))])]);
        let (polars, walk) = (polars_says(&report), walk_says(&report));
        if !polars.accepts() || !walk.accepts() {
            found.push(format!(
                "{column}: Polars says {polars:?}, the walk says {walk:?}"
            ));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}
