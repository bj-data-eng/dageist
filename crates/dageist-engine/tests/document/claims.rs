//! The claim-timing steps the step editor's storyboard opens with, written as
//! a document and run on seven claim rows.
//!
//! The sample node is the engine's own, `ClaimTiming`, which the host serves
//! to the view too. Every expected value is worked out by hand from its seven
//! rows and from what Polars documents for each operation, not read from a
//! run.

use dageist_engine::catalog::Laziness;
use dageist_engine::claim_timing::ClaimTiming;
use dageist_engine::document::{
    Document, ExpressionDtypes, Inputs, Outcome, Report, Resolution, Segment, StepId, StepReport,
};
use polars::prelude::*;

/// Microseconds since the Unix epoch at `hour:minute` on `day` of March 2026.
/// 2026-03-01 is day 20,513 of the epoch: 56 years of 365 days and the 14 leap
/// days of 1972 to 2024, then the 59 days of January and February.
fn at(day: i64, hour: i64, minute: i64) -> i64 {
    ((20_512 + day) * 86_400 + hour * 3_600 + minute * 60) * 1_000_000
}

/// The days since the Unix epoch of `day` of March 2026.
fn date(day: i32) -> i32 {
    20_512 + day
}

fn datetime() -> DataType {
    DataType::Datetime(TimeUnit::Microseconds, None)
}

/// The seven claim rows of the storyboard, in no useful order.
pub fn claims() -> DataFrame {
    ClaimTiming::new()
        .expect("the sample node")
        .source()
        .clone()
}

pub fn inputs() -> Inputs {
    ClaimTiming::new()
        .expect("the sample node")
        .inputs()
        .clone()
}

/// The storyboard's starting steps, with what the catalogue covers: `is_in`
/// is not in it, so the holiday test is a comparison with a connected date.
pub fn storyboard_text() -> String {
    ClaimTiming::STEPS_JSON.to_owned()
}

fn storyboard() -> Document {
    Document::read(&storyboard_text()).expect("the storyboard's steps are a document")
}

fn run() -> Report {
    storyboard()
        .run(&inputs(), claims().lazy())
        .expect("the claims have a schema")
}

fn report_of<'a>(report: &'a Report, id: &str) -> &'a StepReport {
    report
        .steps()
        .iter()
        .find(|step| step.id().as_str() == id)
        .unwrap_or_else(|| panic!("no step {id}"))
}

/// The columns of the table `id` gives, with their dtypes.
fn schema_of(report: &Report, id: &str) -> Vec<(String, DataType)> {
    match report_of(report, id).outcome() {
        Outcome::Valid { schema } => schema
            .iter()
            .map(|(name, dtype)| (name.to_string(), dtype.clone()))
            .collect(),
        other => panic!("step {id} is not valid: {other:?}"),
    }
}

fn columns(items: &[(&str, DataType)]) -> Vec<(String, DataType)> {
    items
        .iter()
        .map(|(name, dtype)| ((*name).to_owned(), dtype.clone()))
        .collect()
}

fn parameter(name: &str) -> Segment {
    Segment::Parameter(name.to_owned())
}

/// The expression of step `id` at the way `path`.
fn expression_at<'a>(report: &'a Report, id: &str, path: &[Segment]) -> &'a ExpressionDtypes {
    report_of(report, id)
        .expressions()
        .iter()
        .find(|trace| trace.place().path() == path)
        .unwrap_or_else(|| panic!("no expression of {id} at {path:?}"))
}

fn dtype(resolution: &Resolution) -> &DataType {
    match resolution {
        Resolution::Dtype(dtype) => dtype,
        other => panic!("not one dtype: {other:?}"),
    }
}

/// The dtype at the start of `expression`, then after each of its links.
fn dtypes(expression: &ExpressionDtypes) -> Vec<&DataType> {
    std::iter::once(expression.start())
        .chain(expression.links())
        .map(dtype)
        .collect()
}

fn strings(frame: &DataFrame, name: &str) -> Vec<Option<String>> {
    frame
        .column(name)
        .expect("a column")
        .as_materialized_series()
        .str()
        .expect("text")
        .iter()
        .map(|value| value.map(str::to_owned))
        .collect()
}

fn whole_numbers(frame: &DataFrame, name: &str) -> Vec<Option<i64>> {
    frame
        .column(name)
        .expect("a column")
        .as_materialized_series()
        .cast(&DataType::Int64)
        .expect("a whole number")
        .i64()
        .expect("whole numbers")
        .iter()
        .collect()
}

fn rows_through(id: &str, limit: usize) -> DataFrame {
    storyboard()
        .rows(
            &inputs(),
            &claims(),
            &StepId::new(id).expect("an id"),
            limit,
        )
        .expect("the rows")
}

fn text_of(frame: &DataFrame, name: &str) -> Vec<String> {
    strings(frame, name)
        .into_iter()
        .map(|value| value.expect("no nulls"))
        .collect()
}

#[test]
fn the_hand_computed_calendar_matches_the_dates_it_stands_for() {
    // The anchors for every number in this file: Polars' own text for them.
    let moment = Series::new("at".into(), [at(2, 13, 30)])
        .cast(&datetime())
        .and_then(|series| series.cast(&DataType::String))
        .expect("text");
    assert_eq!(
        moment.str().expect("text").get(0).map(|text| &text[..19]),
        Some("2026-03-02 13:30:00")
    );
    let day = Series::new("on".into(), [date(5)])
        .cast(&DataType::Date)
        .and_then(|series| series.cast(&DataType::String))
        .expect("text");
    assert_eq!(day.str().expect("text").get(0), Some("2026-03-05"));
}

#[test]
fn the_sample_rows_are_the_instants_the_calendar_works_out() {
    let claims = claims();
    assert_eq!(claims.height(), 7);
    let timestamps = claims
        .column("timestamp")
        .expect("a column")
        .as_materialized_series()
        .cast(&DataType::Int64)
        .expect("microseconds");
    assert_eq!(
        timestamps
            .i64()
            .expect("microseconds")
            .iter()
            .collect::<Vec<_>>(),
        [
            at(2, 13, 30),
            at(5, 7, 50),
            at(6, 10, 0),
            at(2, 9, 15),
            at(4, 17, 20),
            at(9, 11, 5),
            at(3, 16, 40),
        ]
        .map(Some)
    );
    assert_eq!(
        claims.column("timestamp").expect("a column").dtype(),
        &datetime()
    );
}

#[test]
fn every_step_of_the_storyboard_is_valid_and_only_the_pivot_collects() {
    let report = run();
    assert_eq!(report.steps().len(), 12);
    for step in report.steps() {
        assert!(
            matches!(step.outcome(), Outcome::Valid { .. }),
            "{} is {:?}",
            step.id(),
            step.outcome()
        );
        let expected = if step.id().as_str() == "pivot" {
            Laziness::NeedsCollect
        } else {
            Laziness::Lazy
        };
        assert_eq!(step.laziness(), expected, "{}", step.id());
    }
}

#[test]
fn each_step_gives_the_columns_and_dtypes_its_operations_document() {
    let report = run();
    let source = columns(&[
        ("dcn", DataType::String),
        ("timestamp", datetime()),
        ("step_to", DataType::String),
        ("owner", DataType::String),
        ("row_days", DataType::Int64),
    ]);
    let source_schema: Vec<_> = report
        .source()
        .iter()
        .map(|(name, dtype)| (name.to_string(), dtype.clone()))
        .collect();
    assert_eq!(source_schema, source);
    assert_eq!(schema_of(&report, "sort"), source);

    // `cum_count` counts with `u32` (Python's documented example), `shift`,
    // `sub` and the `dt` methods give the dtypes of the temporal operands.
    let mut expected = source;
    for (id, name, dtype) in [
        ("step_index", "step_index", DataType::UInt32),
        ("resolved_at", "resolved_at", datetime()),
        ("received_at", "received_at", datetime()),
        ("resolved_day", "resolved_day", DataType::Date),
        ("days_open", "days_open", DataType::Int64),
        ("a_days", "a_days", DataType::Int64),
    ] {
        expected.push((name.to_owned(), dtype));
        assert_eq!(schema_of(&report, id), expected, "after {id}");
    }

    // A left join adds the other table's columns; the key it matched on is
    // not repeated.
    expected.push(("sla_days".to_owned(), DataType::Int64));
    assert_eq!(schema_of(&report, "join"), expected);
    for (id, name, dtype) in [
        ("sla_status", "sla_status", DataType::String),
        ("flag", "flag", DataType::String),
        ("holiday_count", "holiday_count", DataType::UInt32),
    ] {
        expected.push((name.to_owned(), dtype));
        assert_eq!(schema_of(&report, id), expected, "after {id}");
    }

    // A pivot has the index, then one column for each value it was given.
    assert_eq!(
        schema_of(&report, "pivot"),
        columns(&[
            ("dcn", DataType::String),
            ("Intake", DataType::Int64),
            ("Review", DataType::Int64),
            ("Pending", DataType::Int64),
            ("Complete", DataType::Int64),
        ])
    );
}

#[test]
fn a_chain_has_the_dtype_after_every_link() {
    let report = run();
    let value_of = |index: usize| vec![parameter("named_exprs"), Segment::Value(index)];

    // cum_count counts into u32 and over keeps what it is given.
    assert_eq!(
        dtypes(expression_at(&report, "step_index", &value_of(0))),
        [&DataType::String, &DataType::UInt32, &DataType::UInt32]
    );
    // A timestamp stays a timestamp through shift and over.
    assert_eq!(
        dtypes(expression_at(&report, "resolved_at", &value_of(0))),
        [&datetime(), &datetime(), &datetime()]
    );
    assert_eq!(
        dtypes(expression_at(&report, "resolved_day", &value_of(0))),
        [&datetime(), &DataType::Date]
    );
    // A timestamp minus a timestamp is a duration, whose whole days are Int64.
    assert_eq!(
        dtypes(expression_at(&report, "days_open", &value_of(0))),
        [
            &datetime(),
            &DataType::Duration(TimeUnit::Microseconds),
            &DataType::Int64
        ]
    );
}

#[test]
fn a_condition_has_no_dtype_until_its_then_and_the_dtype_of_its_values_after() {
    let report = run();
    let a_days = expression_at(
        &report,
        "a_days",
        &[parameter("named_exprs"), Segment::Value(0)],
    );
    assert!(matches!(a_days.start(), Resolution::Unfinished));
    // then(row_days), otherwise(0), cum_sum, over: every one is Int64.
    assert_eq!(
        a_days.links().iter().map(dtype).collect::<Vec<_>>(),
        [&DataType::Int64; 4]
    );
}

#[test]
fn the_expressions_inside_a_condition_are_resolved_where_they_stand() {
    let report = run();
    let flag = vec![parameter("named_exprs"), Segment::Value(0)];
    // The first condition: a timestamp, its date, and the comparison with the
    // connected date, a Boolean.
    let holiday_test = [
        flag.clone(),
        vec![Segment::Start, parameter("predicates"), Segment::Item(0)],
    ]
    .concat();
    assert_eq!(
        dtypes(expression_at(&report, "flag", &holiday_test)),
        [&datetime(), &DataType::Date, &DataType::Boolean]
    );
    // The connected date is a literal of its own dtype.
    let as_of = [
        holiday_test.clone(),
        vec![Segment::Link(1), parameter("other")],
    ]
    .concat();
    assert_eq!(
        dtypes(expression_at(&report, "flag", &as_of)),
        [&DataType::Date]
    );
    // The `otherwise` holds another condition, which is text throughout.
    let otherwise = [flag, vec![Segment::Link(1), parameter("statement")]].concat();
    let inner = expression_at(&report, "flag", &otherwise);
    assert!(matches!(inner.start(), Resolution::Unfinished));
    assert_eq!(
        inner.links().iter().map(dtype).collect::<Vec<_>>(),
        [&DataType::String; 2]
    );
}

#[test]
fn a_connected_list_starts_an_expression_as_a_literal_of_its_dtype() {
    let report = run();
    let holidays = expression_at(
        &report,
        "holiday_count",
        &[parameter("named_exprs"), Segment::Value(0)],
    );
    // Two dates, then their count, which `len` gives as u32.
    assert_eq!(dtypes(holidays), [&DataType::Date, &DataType::UInt32]);
}

#[test]
fn a_pivots_aggregate_from_element_has_no_dtype_without_its_context() {
    let report = run();
    let aggregate = expression_at(&report, "pivot", &[parameter("aggregate_function")]);
    assert!(matches!(aggregate.start(), Resolution::NoContext));
    assert!(matches!(aggregate.links(), [Resolution::NoContext]));
}

#[test]
fn rows_after_sorting_are_in_key_then_time_order() {
    let rows = rows_through("sort", 100);
    assert_eq!(rows.height(), 7);
    assert_eq!(
        text_of(&rows, "dcn"),
        [
            "A-1001", "A-1001", "A-1001", "A-1001", "A-1002", "A-1002", "A-1002"
        ]
    );
    assert_eq!(
        text_of(&rows, "step_to"),
        [
            "Intake", "Review", "Pending", "Complete", "Intake", "Review", "Complete"
        ]
    );
}

#[test]
fn rows_number_the_steps_of_each_key_from_one_and_carry_the_one_before() {
    let rows = rows_through("days_open", 100);
    assert_eq!(
        whole_numbers(&rows, "step_index"),
        [1, 2, 3, 4, 1, 2, 3].map(Some)
    );
    // The first row of each key has nothing before it.
    let day = |d: i32| Some(i64::from(date(d)));
    assert_eq!(
        whole_numbers(&rows, "resolved_day"),
        [None, day(2), day(3), day(5), None, day(2), day(4)]
    );
    // Whole days between the row and the one before it, dropping the rest of a
    // day: 1d 7h25m, 1d 15h10m, 4d 3h15m, then 2d 3h50m and 1d 16h40m.
    assert_eq!(
        whole_numbers(&rows, "days_open"),
        [None, Some(1), Some(1), Some(4), None, Some(2), Some(1)]
    );
}

#[test]
fn rows_of_a_running_total_count_only_the_rows_of_one_owner() {
    let rows = rows_through("a_days", 100);
    // Company A's days run up, other owners' rows add nothing, and each key
    // starts again.
    assert_eq!(
        whole_numbers(&rows, "a_days"),
        [0, 0, 2, 2, 0, 2, 2].map(Some)
    );
}

#[test]
fn rows_after_a_left_join_carry_the_other_tables_values_in_order() {
    let rows = rows_through("sla_status", 100);
    assert_eq!(
        whole_numbers(&rows, "sla_days"),
        [1, 1, 2, 3, 1, 1, 3].map(Some)
    );
    // row_days <= sla_days is true for every row but the second key's Review.
    assert_eq!(
        text_of(&rows, "sla_status"),
        [
            "within", "within", "within", "within", "within", "late", "within"
        ]
    );
}

#[test]
fn rows_of_the_nested_condition_follow_its_first_matching_case() {
    let rows = rows_through("flag", 100);
    assert_eq!(
        text_of(&rows, "flag"),
        ["holiday", "ok", "over", "over", "holiday", "over", "over"]
    );
}

#[test]
fn rows_stop_at_the_limit() {
    let rows = rows_through("sla_status", 3);
    assert_eq!(rows.height(), 3);
    assert_eq!(text_of(&rows, "step_to"), ["Intake", "Review", "Pending"]);
}

#[test]
fn rows_of_the_pivot_collect_there_and_give_one_row_for_each_key() {
    let rows = rows_through("pivot", 100)
        .sort(["dcn"], SortMultipleOptions::default())
        .expect("sorted");
    assert_eq!(text_of(&rows, "dcn"), ["A-1001", "A-1002"]);
    assert_eq!(whole_numbers(&rows, "Intake"), [Some(0), Some(0)]);
    assert_eq!(whole_numbers(&rows, "Review"), [Some(1), Some(2)]);
    // A-1002 never reached Pending.
    assert_eq!(whole_numbers(&rows, "Pending"), [Some(2), None]);
    assert_eq!(whole_numbers(&rows, "Complete"), [Some(2), Some(2)]);
}
