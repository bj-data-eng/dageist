//! What the functions that make a date, a time, a moment or a length of time out of parts, or out
//! of a count since 1970, do in a step, on the document-timing sample, whose `row_days` column holds
//! `0, 2, 2, 0, 2, 2, 1`. The expected cells are worked out by hand from those seven numbers and
//! from the calendar, and the dtypes are what Python Polars documents for each function.

use dageist_engine::document::{Document, Outcome, Report, StepId};
use polars::prelude::{DataType, TimeUnit, TimeZone};
use serde_json::{Value, json};

use crate::documents::{documents, inputs};
use crate::support::{call, chain, document, field, field_then, named, step, whole};

/// The one-step document that selects `expression` as the column `out`.
fn selecting(expression: Value) -> String {
    document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "named_exprs": named(vec![("out", expression)]) }),
    )])
}

fn report_of(text: &str) -> Report {
    Document::read(text)
        .expect("a document")
        .run_on(&inputs(), &documents())
        .expect("the documents have a schema")
}

/// The dtype of the column `out` that the step gives, which it must give.
fn dtype_of(written: &str) -> DataType {
    let report = report_of(written);
    match report.steps()[0].outcome() {
        Outcome::Valid { schema, .. } => schema
            .get("out")
            .unwrap_or_else(|| panic!("no column out in {schema:?}"))
            .clone(),
        other => panic!("{other:?}"),
    }
}

fn cells(written: &str) -> Vec<Option<String>> {
    Document::read(written)
        .expect("a document")
        .preview(
            &inputs(),
            &documents(),
            &StepId::new("s").expect("an id"),
            100,
        )
        .expect("the rows")
        .rows()
        .iter()
        .map(|row| row[0].clone())
        .collect()
}

fn texts(cells: &[&str]) -> Vec<Option<String>> {
    cells.iter().map(|cell| Some((*cell).to_owned())).collect()
}

/// The start `function` given `arguments`, as an expression that nothing follows.
fn made(function: &str, arguments: Value) -> Value {
    chain(call(function, arguments), vec![])
}

/// The days of the sample, one more than each number: `1, 3, 3, 1, 3, 3, 2`.
fn days_from_one() -> Value {
    field_then(
        "row_days",
        vec![call("Expr.add", json!({ "other": whole(1) }))],
    )
}

#[test]
fn a_date_is_made_of_a_year_a_month_and_a_day_that_a_column_may_give() {
    let written = selecting(made(
        "polars.date",
        json!({ "year": whole(2026), "month": whole(3), "day": days_from_one() }),
    ));
    assert_eq!(dtype_of(&written), DataType::Date);
    assert_eq!(
        cells(&written),
        texts(&[
            "2026-03-01",
            "2026-03-03",
            "2026-03-03",
            "2026-03-01",
            "2026-03-03",
            "2026-03-03",
            "2026-03-02",
        ])
    );
}

#[test]
fn a_time_of_day_is_made_of_the_parts_given_and_zero_for_those_not() {
    let written = selecting(made(
        "polars.time",
        json!({ "hour": field("row_days"), "minute": whole(30) }),
    ));
    assert_eq!(dtype_of(&written), DataType::Time);
    assert_eq!(
        cells(&written),
        texts(&[
            "00:30:00", "02:30:00", "02:30:00", "00:30:00", "02:30:00", "02:30:00", "01:30:00",
        ])
    );
    // Given no part, it is midnight, one value.
    let midnight = selecting(made("polars.time", json!({})));
    assert_eq!(dtype_of(&midnight), DataType::Time);
}

#[test]
fn a_moment_is_made_in_the_unit_and_the_zone_it_is_given() {
    let parts = |extra: Value| {
        let mut arguments = json!({
            "year": whole(2026),
            "month": whole(3),
            "day": whole(5),
            "hour": field("row_days"),
        });
        arguments
            .as_object_mut()
            .expect("an object")
            .extend(extra.as_object().expect("an object").clone());
        selecting(made("polars.datetime", arguments))
    };
    let naive = parts(json!({}));
    assert_eq!(
        dtype_of(&naive),
        DataType::Datetime(TimeUnit::Microseconds, None)
    );
    assert_eq!(
        cells(&naive),
        texts(&[
            "2026-03-05 00:00:00",
            "2026-03-05 02:00:00",
            "2026-03-05 02:00:00",
            "2026-03-05 00:00:00",
            "2026-03-05 02:00:00",
            "2026-03-05 02:00:00",
            "2026-03-05 01:00:00",
        ])
    );
    let zoned = parts(json!({
        "time_unit": { "choice": "ms" },
        "time_zone": { "time_zone": "Europe/Paris" },
    }));
    let paris = TimeZone::opt_try_new(Some("Europe/Paris"))
        .expect("a time zone")
        .expect("a zone");
    assert_eq!(
        dtype_of(&zoned),
        DataType::Datetime(TimeUnit::Milliseconds, Some(paris))
    );
    // Paris is an hour ahead of UTC on the fifth of March, and the hour is the clock there.
    assert_eq!(cells(&zoned)[1].as_deref(), Some("2026-03-05 02:00:00 CET"));
}

#[test]
fn a_length_of_time_is_in_microseconds_unless_nanoseconds_or_a_unit_say_otherwise() {
    let days = |extra: Value| {
        let mut arguments = json!({ "days": field("row_days"), "hours": whole(12) });
        arguments
            .as_object_mut()
            .expect("an object")
            .extend(extra.as_object().expect("an object").clone());
        selecting(made("polars.duration", arguments))
    };
    assert_eq!(
        dtype_of(&days(json!({}))),
        DataType::Duration(TimeUnit::Microseconds)
    );
    // Python infers nanoseconds where they are given and no unit is.
    assert_eq!(
        dtype_of(&days(json!({ "nanoseconds": whole(5) }))),
        DataType::Duration(TimeUnit::Nanoseconds)
    );
    assert_eq!(
        dtype_of(&days(
            json!({ "nanoseconds": whole(5), "time_unit": { "choice": "ms" } })
        )),
        DataType::Duration(TimeUnit::Milliseconds)
    );
    // Two days and twelve hours.
    let cells = cells(&days(json!({ "time_unit": { "choice": "ms" } })));
    assert_eq!(cells[1].as_deref(), Some("2d 12h"));
}

#[test]
fn a_count_since_1970_is_a_date_for_days_and_a_moment_for_the_other_units() {
    let counted = |unit: Option<&str>| {
        let mut arguments = json!({ "column": field("row_days") });
        if let Some(unit) = unit {
            arguments["time_unit"] = json!({ "choice": unit });
        }
        selecting(made("polars.from_epoch", arguments))
    };
    // The default is seconds, held in microseconds.
    let seconds = counted(None);
    assert_eq!(
        dtype_of(&seconds),
        DataType::Datetime(TimeUnit::Microseconds, None)
    );
    assert_eq!(cells(&seconds)[1].as_deref(), Some("1970-01-01 00:00:02"));
    let days = counted(Some("d"));
    assert_eq!(dtype_of(&days), DataType::Date);
    assert_eq!(cells(&days)[1].as_deref(), Some("1970-01-03"));
    assert_eq!(
        dtype_of(&counted(Some("ns"))),
        DataType::Datetime(TimeUnit::Nanoseconds, None)
    );
    assert_eq!(
        dtype_of(&counted(Some("ms"))),
        DataType::Datetime(TimeUnit::Microseconds, None)
    );
}
