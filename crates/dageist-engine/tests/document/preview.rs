//! The rows after a step as text, on the claim-timing sample. Every expected
//! cell is worked out by hand from the seven rows and from how Polars writes
//! a value: a date as `2026-03-02`, a timestamp as `2026-03-02 09:15:00`, a
//! duration in days, hours and minutes.

use dageist_engine::document::{Document, Preview, RowsError, StepId};
use serde_json::json;

use crate::claims::{claims, inputs, storyboard_text};
use crate::support::{call, document, field, field_then, named, step, whole};

fn preview_through(text: &str, id: &str, limit: usize) -> Result<Preview, RowsError> {
    Document::read(text).expect("a document").preview(
        &inputs(),
        &claims(),
        &StepId::new(id).expect("an id"),
        limit,
    )
}

fn cells(preview: &Preview) -> Vec<Vec<Option<&str>>> {
    preview
        .rows()
        .iter()
        .map(|row| row.iter().map(Option::as_deref).collect())
        .collect()
}

fn columns(preview: &Preview) -> Vec<(&str, &str)> {
    preview
        .columns()
        .iter()
        .map(|column| (column.name(), column.dtype()))
        .collect()
}

/// The claims sorted by key and time, then the gap to the row before and
/// whether the days are over one.
fn gaps() -> String {
    document(vec![
        step(
            "sort",
            "LazyFrame.sort",
            json!({ "by": crate::support::many(vec![field("dcn"), field("timestamp")]) }),
        ),
        step(
            "gaps",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![
                (
                    "gap",
                    field_then(
                        "timestamp",
                        vec![call(
                            "Expr.sub",
                            json!({ "other": field_then(
                                "timestamp",
                                vec![call("Expr.shift", json!({ "n": whole(1) }))],
                            ) }),
                        )],
                    ),
                ),
                (
                    "slow",
                    field_then("row_days", vec![call("Expr.gt", json!({ "other": whole(1) }))]),
                ),
            ]) }),
        ),
    ])
}

#[test]
fn a_step_gives_its_columns_with_short_dtypes_and_its_rows_as_text() {
    let preview = preview_through(&storyboard_text(), "sort", 100).expect("the rows");
    assert_eq!(
        columns(&preview),
        [
            ("dcn", "String"),
            ("timestamp", "Datetime(us)"),
            ("step_to", "String"),
            ("owner", "String"),
            ("row_days", "Int64"),
        ]
    );
    assert_eq!(preview.rows().len(), 7);
    // Sorted by key, then time: the first two rows of A-1001.
    assert_eq!(
        cells(&preview)[..2],
        [
            vec![
                Some("A-1001"),
                Some("2026-03-02 09:15:00"),
                Some("Intake"),
                Some("Company A"),
                Some("0"),
            ],
            vec![
                Some("A-1001"),
                Some("2026-03-03 16:40:00"),
                Some("Review"),
                Some("Company B"),
                Some("1"),
            ],
        ]
    );
}

#[test]
fn a_null_is_a_missing_cell_and_a_date_is_written_as_a_date() {
    let preview = preview_through(&storyboard_text(), "days_open", 2).expect("the rows");
    let at = |name: &str| {
        preview
            .columns()
            .iter()
            .position(|column| column.name() == name)
            .unwrap_or_else(|| panic!("no column {name}"))
    };
    for (name, dtype) in [
        ("step_index", "UInt32"),
        ("resolved_at", "Datetime(us)"),
        ("resolved_day", "Date"),
        ("days_open", "Int64"),
    ] {
        assert_eq!(preview.columns()[at(name)].dtype(), dtype, "{name}");
    }
    let rows = cells(&preview);
    // The first row of a key has no row before it, so nothing carries over.
    assert_eq!(rows[0][at("step_index")], Some("1"));
    assert_eq!(rows[0][at("resolved_at")], None);
    assert_eq!(rows[0][at("resolved_day")], None);
    assert_eq!(rows[0][at("days_open")], None);
    // The second row's earlier time is the first row's, 2026-03-02 09:15, and
    // 1d 7h 25m of difference is one whole day.
    assert_eq!(rows[1][at("step_index")], Some("2"));
    assert_eq!(rows[1][at("resolved_at")], Some("2026-03-02 09:15:00"));
    assert_eq!(rows[1][at("resolved_day")], Some("2026-03-02"));
    assert_eq!(rows[1][at("days_open")], Some("1"));
}

#[test]
fn a_duration_and_a_boolean_are_written_as_polars_writes_them() {
    let preview = preview_through(&gaps(), "gaps", 3).expect("the rows");
    let gap = preview.columns().len() - 2;
    assert_eq!(
        columns(&preview)[gap..],
        [("gap", "Duration(us)"), ("slow", "Boolean")]
    );
    // 2026-03-02 09:15 to 03-03 16:40 is 1d 7h 25m, and to 03-05 07:50 is
    // 1d 15h 10m. Row days are 0, 1 and 2.
    let tail: Vec<_> = cells(&preview)
        .iter()
        .map(|row| row[gap..].to_vec())
        .collect();
    assert_eq!(
        tail,
        [
            vec![None, Some("false")],
            vec![Some("1d 7h 25m"), Some("false")],
            vec![Some("1d 15h 10m"), Some("true")],
        ]
    );
}

#[test]
fn rows_stop_at_the_limit_as_they_do_unformatted() {
    let text = storyboard_text();
    assert_eq!(
        preview_through(&text, "sla_status", 3)
            .expect("rows")
            .rows()
            .len(),
        3
    );
    assert_eq!(
        preview_through(&text, "sla_status", 0)
            .expect("rows")
            .rows()
            .len(),
        0
    );
}

#[test]
fn a_step_that_is_not_in_the_document_has_no_rows() {
    match preview_through(&storyboard_text(), "nowhere", 3) {
        Err(RowsError::NoSuchStep(id)) => assert_eq!(id.as_str(), "nowhere"),
        other => panic!("not a missing step: {other:?}"),
    }
}
