//! What a run on the working window counts: the rows each step kept of those
//! it received, the first values of the columns it added, and which columns it
//! added or replaced. Every expected value is worked out by hand from the
//! seven claim rows, whose `row_days` are 0, 2, 2, 0, 2, 2, 1 in the order
//! they are held.

use dageist_engine::document::{
    Change, ColumnChange, Counted, Document, Outcome, Report, StepRows,
};
use polars::prelude::IntoLazy;
use serde_json::json;

use crate::claims::{claims, inputs, storyboard_text};
use crate::support::{call, chain, document, field, field_then, many, named, step, whole};

fn run_on(text: &str) -> Report {
    Document::read(text)
        .expect("a document")
        .run_on(&inputs(), &claims())
        .expect("the claims have a schema")
}

fn counted<'a>(report: &'a Report, id: &str) -> &'a Counted {
    let reported = report
        .steps()
        .iter()
        .find(|step| step.id().as_str() == id)
        .unwrap_or_else(|| panic!("no step {id}"));
    match reported.rows() {
        StepRows::Counted(count) => count,
        other => panic!("step {id} is not counted: {other:?}"),
    }
}

fn changes<'a>(report: &'a Report, id: &str) -> &'a [ColumnChange] {
    let reported = report
        .steps()
        .iter()
        .find(|step| step.id().as_str() == id)
        .unwrap_or_else(|| panic!("no step {id}"));
    match reported.outcome() {
        Outcome::Valid { changes, .. } => changes,
        other => panic!("step {id} is not valid: {other:?}"),
    }
}

fn named_changes(changes: &[ColumnChange]) -> Vec<(&str, Change)> {
    changes
        .iter()
        .map(|change| (change.name(), change.change()))
        .collect()
}

/// The first values of the column `name` that `count` holds.
fn first_of<'a>(count: &'a Counted, name: &str) -> (Vec<Option<&'a str>>, usize) {
    let found = count
        .first()
        .iter()
        .find(|column| column.name() == name)
        .unwrap_or_else(|| panic!("no first values of {name}"));
    (
        found
            .values()
            .shown()
            .iter()
            .map(Option::as_deref)
            .collect(),
        found.values().more(),
    )
}

fn with_columns(id: &str, entries: Vec<(&str, serde_json::Value)>) -> serde_json::Value {
    step(
        id,
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(entries) }),
    )
}

/// `row_days > 1`, which is true for the rows that waited more than a day.
fn slow() -> serde_json::Value {
    field_then(
        "row_days",
        vec![call("Expr.gt", json!({ "other": whole(1) }))],
    )
}

#[test]
fn a_filter_keeps_the_rows_that_pass_of_the_rows_it_received() {
    let report = run_on(&document(vec![step(
        "slow",
        "LazyFrame.filter",
        json!({ "predicates": many(vec![slow()]) }),
    )]));
    // Four of the seven rows have more than one day.
    let count = counted(&report, "slow");
    assert_eq!((count.received(), count.kept()), (7, 4));
    assert!(count.first().is_empty());
    assert!(changes(&report, "slow").is_empty());
}

#[test]
fn a_step_is_given_the_rows_the_step_before_kept() {
    let report = run_on(&document(vec![
        step(
            "slow",
            "LazyFrame.filter",
            json!({ "predicates": many(vec![slow()]) }),
        ),
        step(
            "late",
            "LazyFrame.filter",
            json!({ "predicates": many(vec![chain(
                call("polars.col", json!({ "name": { "columns": ["dcn"] } })),
                vec![call("Expr.eq", json!({ "other": chain(
                    call("polars.lit", json!({ "value": { "string": "A-1001" } })),
                    vec![],
                ) }))],
            )]) }),
        ),
        step(
            "sorted",
            "LazyFrame.sort",
            json!({ "by": many(vec![field("timestamp")]) }),
        ),
    ]));
    // Of the four rows with more than one day, two are A-1001's: the rows at
    // 03-05 and 03-09.
    let counts: Vec<(usize, usize)> = ["slow", "late", "sorted"]
        .iter()
        .map(|id| {
            let count = counted(&report, id);
            (count.received(), count.kept())
        })
        .collect();
    assert_eq!(counts, [(7, 4), (4, 2), (2, 2)]);
}

#[test]
fn the_first_values_of_an_added_column_are_written_as_text_with_how_many_follow() {
    let report = run_on(&document(vec![with_columns("add", vec![("slow", slow())])]));
    assert_eq!(
        named_changes(changes(&report, "add")),
        [("slow", Change::Added)]
    );
    // The first four of seven, in the order the rows are held.
    assert_eq!(
        first_of(counted(&report, "add"), "slow"),
        (
            vec![Some("false"), Some("true"), Some("true"), Some("false")],
            3
        )
    );
}

#[test]
fn a_null_among_the_first_values_has_no_text() {
    let report = run_on(&document(vec![with_columns(
        "earlier",
        vec![(
            "earlier_days",
            field_then(
                "row_days",
                vec![call("Expr.shift", json!({ "n": whole(1) }))],
            ),
        )],
    )]));
    assert_eq!(
        first_of(counted(&report, "earlier"), "earlier_days"),
        (vec![None, Some("0"), Some("2"), Some("2")], 3)
    );
}

#[test]
fn a_column_with_fewer_values_than_are_shown_has_none_following() {
    let report = run_on(&document(vec![
        step(
            "slow",
            "LazyFrame.filter",
            json!({ "predicates": many(vec![slow()]) }),
        ),
        with_columns("add", vec![("slow", slow())]),
    ]));
    assert_eq!(
        first_of(counted(&report, "add"), "slow"),
        (vec![Some("true"); 4], 0)
    );
}

#[test]
fn a_column_a_step_writes_is_replaced_even_when_its_dtype_is_the_same() {
    let report = run_on(&document(vec![with_columns(
        "again",
        vec![("owner", field("owner")), ("slow", slow())],
    )]));
    // `owner` stays the text it was, and is the step's own; `slow` is new.
    assert_eq!(
        named_changes(changes(&report, "again")),
        [("owner", Change::Replaced), ("slow", Change::Added)]
    );
}

#[test]
fn a_column_that_takes_another_dtype_is_replaced() {
    let report = run_on(&document(vec![with_columns(
        "flag",
        vec![("row_days", slow())],
    )]));
    assert_eq!(
        named_changes(changes(&report, "flag")),
        [("row_days", Change::Replaced)]
    );
}

#[test]
fn a_select_of_columns_it_was_given_changes_none() {
    let report = run_on(&document(vec![step(
        "pick",
        "LazyFrame.select",
        json!({ "exprs": many(vec![field("owner"), field("dcn")]) }),
    )]));
    assert!(changes(&report, "pick").is_empty());
    let count = counted(&report, "pick");
    assert_eq!((count.received(), count.kept()), (7, 7));
}

#[test]
fn a_rename_adds_the_new_name() {
    let report = run_on(&document(vec![step(
        "rename",
        "LazyFrame.rename",
        json!({ "mapping": { "pairs": [[{ "columns": ["dcn"] }, { "string": "claim" }]] } }),
    )]));
    assert_eq!(
        named_changes(changes(&report, "rename")),
        [("claim", Change::Added)]
    );
    assert_eq!(
        first_of(counted(&report, "rename"), "claim").0[0],
        Some("A-1002")
    );
}

#[test]
fn a_join_adds_the_columns_of_the_other_table() {
    let report = run_on(&document(vec![step(
        "sla",
        "LazyFrame.join",
        json!({
            "other": { "table": "step_sla" },
            "left_on": many(vec![field("step_to")]),
            "right_on": many(vec![field("step")]),
            "how": { "choice": "left" },
            // Polars may reorder the rows of a join unless it is told not to.
            "maintain_order": { "choice": "left" },
        }),
    )]));
    // The key `step` is joined to `step_to`, so it is not another column.
    assert_eq!(
        named_changes(changes(&report, "sla")),
        [("sla_days", Change::Added)]
    );
    let count = counted(&report, "sla");
    assert_eq!((count.received(), count.kept()), (7, 7));
    // The days each row's step may take: Intake 1, Pending 2, Complete 3, Intake 1, then
    // Review 1, Complete 3 and Review 1.
    assert_eq!(
        first_of(count, "sla_days"),
        (vec![Some("1"), Some("2"), Some("3"), Some("1")], 3)
    );
}

#[test]
fn a_pivot_adds_a_column_for_each_value_and_keeps_a_row_for_each_key() {
    let report = run_on(&storyboard_text());
    assert_eq!(
        named_changes(changes(&report, "pivot")),
        [
            ("Intake", Change::Added),
            ("Review", Change::Added),
            ("Pending", Change::Added),
            ("Complete", Change::Added),
        ]
    );
    let count = counted(&report, "pivot");
    // Two claims, from seven rows.
    assert_eq!((count.received(), count.kept()), (7, 2));
    // The sort puts A-1001 first, and A-1002 never reached Pending, so it has
    // no days there.
    assert_eq!(first_of(count, "Intake"), (vec![Some("0"), Some("0")], 0));
    assert_eq!(first_of(count, "Review"), (vec![Some("1"), Some("2")], 0));
    assert_eq!(first_of(count, "Pending"), (vec![Some("2"), None], 0));
}

#[test]
fn every_step_of_a_run_is_counted_as_the_rows_after_it_are() {
    let text = storyboard_text();
    let report = run_on(&text);
    let steps = Document::read(&text).expect("a document");
    let mut before = claims().height();
    for reported in report.steps() {
        let count = counted(&report, reported.id().as_str());
        let rows = steps
            .rows(&inputs(), &claims(), reported.id(), 1_000)
            .expect("the rows after the step");
        assert_eq!(count.received(), before, "{}", reported.id());
        assert_eq!(count.kept(), rows.height(), "{}", reported.id());
        before = count.kept();
    }
}

#[test]
fn a_run_that_reads_no_rows_counts_none() {
    let report = Document::read(&storyboard_text())
        .expect("a document")
        .run(&inputs(), claims().lazy())
        .expect("the claims have a schema");
    assert!(
        report
            .steps()
            .iter()
            .all(|step| matches!(step.rows(), StepRows::NotCounted))
    );
}

#[test]
fn a_step_polars_refuses_is_not_counted_and_nor_are_the_steps_after_it() {
    let report = run_on(&document(vec![
        step(
            "sort",
            "LazyFrame.sort",
            json!({ "by": many(vec![field("dcn")]) }),
        ),
        with_columns(
            "total",
            vec![(
                "total",
                field_then("dcn", vec![call("Expr.sum", json!({}))]),
            )],
        ),
        with_columns("after", vec![("days", field("row_days"))]),
    ]));
    assert_eq!(counted(&report, "sort").kept(), 7);
    for id in ["total", "after"] {
        let reported = report
            .steps()
            .iter()
            .find(|step| step.id().as_str() == id)
            .expect("a step");
        assert!(matches!(reported.rows(), StepRows::NotCounted), "{id}");
    }
}

#[test]
fn a_step_polars_accepts_but_cannot_run_on_the_rows_fails_and_the_steps_after_it_are_not_counted() {
    // The date of text is a date by its schema and fails when rows are read.
    let report = run_on(&document(vec![
        with_columns(
            "date",
            vec![(
                "day",
                field_then("dcn", vec![call("Expr.dt.date", json!({}))]),
            )],
        ),
        with_columns("after", vec![("again", field("row_days"))]),
    ]));
    let rows: Vec<&StepRows> = report.steps().iter().map(|step| step.rows()).collect();
    assert!(matches!(rows[0], StepRows::Failed(_)), "{:?}", rows[0]);
    assert!(matches!(rows[1], StepRows::NotCounted), "{:?}", rows[1]);
    // The schema still holds: the report says it is valid.
    assert!(matches!(report.steps()[0].outcome(), Outcome::Valid { .. }));
}
