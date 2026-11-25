//! A pivot's aggregate that gives one value for each group and is then sorted, run on rows where
//! a group holds several.
//!
//! Polars 2.1.0 evaluated `sort` of a value that is one for each group as it does the values of a
//! group: it took the places of the group's rows from the one value each group had, and a debug
//! build stopped with a failed bounds check in `Column::take_slice_unchecked` where a release build
//! took them unchecked. Polars 2.2.0 makes the sort of a single value of a group no change
//! (pola-rs/polars#30327). These tests run the aggregates that tripped the check through
//! `Document::run_on` and `Document::rows`, under `catch_unwind` so that a panic is a failure that
//! says which, on a table whose groups hold two and four rows.
//!
//! The expected cells are what Python Polars 2.2.0 gives for `LazyFrame.pivot` with the same
//! aggregate, worked out by hand from the table: the groups of `k` are the rows `0, 1, 3, 4` and
//! `2, 5`, which `h` splits into `x` for the first group and `y` for the second, so the other two
//! cells hold no value.

use std::panic::{AssertUnwindSafe, catch_unwind};

use dageist_engine::document::{Document, Inputs, Outcome, StepId, StepRows};
use polars::prelude::{
    AnyValue, DataFrame, DataType, IntoLazy, NamedFrom, Series, SortMultipleOptions, df,
};
use serde_json::{Value, json};

use crate::support::{call, chain, document, step};

/// A cell of the pivot, as the expectation says it.
#[derive(Debug, Clone, PartialEq)]
enum Cell {
    Whole(i64),
    Fraction(f64),
    Text(String),
    Missing,
}

/// The table: two groups of `k`, split by `h`, with a column for each dtype the aggregates meet.
fn working(values: &str) -> DataFrame {
    let arrays = Series::new(
        "w".into(),
        [
            [1.0_f64, 1.0],
            [2.0, 4.0],
            [9.0, 9.0],
            [4.0, 6.0],
            [10.0, 10.0],
            [5.0, 5.0],
        ]
        .map(|pair| Series::new("".into(), pair)),
    )
    .cast(&DataType::Array(Box::new(DataType::Float64), 2))
    .expect("arrays of two numbers");
    let arrays = if values == "d" {
        arrays
            .cast(&DataType::Array(Box::new(DataType::Decimal(10, 2)), 2))
            .expect("arrays of two decimals")
            .with_name("d".into())
    } else {
        arrays
    };
    let mut frame = df!(
        "k" => [1_i64, 1, 2, 1, 1, 2],
        "h" => ["x", "x", "y", "x", "x", "y"],
        "n" => [1_i64, 2, 3, 4, 5, 6],
        "s" => ["p", "q", "r", "s", "t", "u"],
    )
    .expect("the table");
    frame
        .with_column(polars::prelude::Column::from(arrays))
        .expect("a column of the rows");
    frame
}

fn inputs() -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "on_h",
            "Values of h",
            df!("h" => ["x", "y"]).expect("the values").lazy(),
        )
        .expect("an input");
    inputs
}

/// The pivot of `values` whose aggregate starts from `element` and goes on with `links`.
fn pivot(values: &str, links: Vec<Value>, maintain_order: bool) -> String {
    document(vec![step(
        "s",
        "LazyFrame.pivot",
        json!({
            "on": { "columns": ["h"] },
            "on_columns": { "table": "on_h" },
            "index": { "columns": ["k"] },
            "values": { "columns": [values] },
            "aggregate_function": chain(call("polars.element", json!({})), links),
            "maintain_order": { "flag": maintain_order },
        }),
    )])
}

fn link(entry: &str) -> Value {
    call(entry, json!({}))
}

/// The cells of the rows, by row and then by column, in the order of `k`.
fn cells_of(rows: &DataFrame) -> Vec<Vec<Cell>> {
    let rows = rows
        .sort(["k"], SortMultipleOptions::default())
        .expect("the rows sort by k");
    (0..rows.height())
        .map(|row| {
            rows.columns()
                .iter()
                .map(|column| match column.get(row).expect("a cell") {
                    AnyValue::Int64(whole) => Cell::Whole(whole),
                    AnyValue::UInt32(whole) => Cell::Whole(i64::from(whole)),
                    AnyValue::Float64(fraction) => Cell::Fraction(fraction),
                    AnyValue::String(text) => Cell::Text(text.to_owned()),
                    AnyValue::Null => Cell::Missing,
                    other => panic!("a cell of a kind no expectation names: {other:?}"),
                })
                .collect()
        })
        .collect()
}

fn same(found: &[Vec<Cell>], expected: &[Vec<Cell>]) -> bool {
    found.len() == expected.len()
        && found.iter().zip(expected).all(|(found, expected)| {
            found.len() == expected.len()
                && found.iter().zip(expected).all(|pair| match pair {
                    (Cell::Fraction(found), Cell::Fraction(expected)) => {
                        (found - expected).abs() < 1e-9
                    }
                    (found, expected) => found == expected,
                })
        })
}

/// Runs the document on the rows as the editor does, and says what happened, so that a panic
/// of Polars is a failure that is told.
fn ran(text: &str, frame: &DataFrame) -> Result<Vec<Vec<Cell>>, String> {
    let document = Document::read(text).map_err(|error| format!("unreadable: {error}"))?;
    let id = StepId::new("s").expect("an identifier");
    catch_unwind(AssertUnwindSafe(|| {
        let report = document
            .run_on(&inputs(), frame)
            .map_err(|error| format!("no schema: {error}"))?;
        match (report.steps()[0].outcome(), report.steps()[0].rows()) {
            (Outcome::Valid { .. }, StepRows::Counted(_)) => {}
            (outcome, rows) => return Err(format!("not run: {outcome:?} {rows:?}")),
        }
        document
            .rows(&inputs(), frame, &id, 100)
            .map(|rows| cells_of(&rows))
            .map_err(|error| format!("no rows: {error}"))
    }))
    .unwrap_or_else(|payload| {
        Err(format!(
            "Polars panicked: {}",
            payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("with no text")
        ))
    })
}

fn whole(k: i64, x: i64, y: i64) -> Vec<Cell> {
    vec![Cell::Whole(k), Cell::Whole(x), Cell::Whole(y)]
}

#[test]
fn a_sum_of_the_group_sorted_in_a_pivot_gives_the_sum_of_each_group() {
    for maintain_order in [false, true] {
        let text = pivot(
            "n",
            vec![link("Expr.sum"), link("Expr.sort")],
            maintain_order,
        );
        assert_eq!(
            ran(&text, &working("n")),
            Ok(vec![whole(1, 12, 0), whole(2, 0, 9)]),
            "maintain_order {maintain_order}"
        );
    }
}

#[test]
fn every_aggregate_of_the_group_sorted_in_a_pivot_gives_one_value_for_each_group() {
    // The numbers `n` of the group `x` are 1, 2, 4 and 5, those of `y` are 3 and 6, and a group
    // that holds no row gives no value, or the count of none.
    let aggregates = [
        ("Expr.first", Cell::Whole(1), Cell::Whole(3), Cell::Missing),
        ("Expr.last", Cell::Whole(5), Cell::Whole(6), Cell::Missing),
        ("Expr.max", Cell::Whole(5), Cell::Whole(6), Cell::Missing),
        (
            "Expr.n_unique",
            Cell::Whole(4),
            Cell::Whole(2),
            Cell::Whole(0),
        ),
        ("Expr.len", Cell::Whole(4), Cell::Whole(2), Cell::Whole(0)),
    ];
    for (entry, x, y, none) in aggregates {
        let text = pivot("n", vec![link(entry), link("Expr.sort")], true);
        assert_eq!(
            ran(&text, &working("n")),
            Ok(vec![
                vec![Cell::Whole(1), x, none.clone()],
                vec![Cell::Whole(2), none, y],
            ]),
            "{entry}"
        );
    }
}

#[test]
fn the_largest_text_of_the_group_sorted_in_a_pivot_is_the_text_of_each_group() {
    let text = pivot("s", vec![link("Expr.max"), link("Expr.sort")], true);
    assert_eq!(
        ran(&text, &working("s")),
        Ok(vec![
            vec![Cell::Whole(1), Cell::Text("t".to_owned()), Cell::Missing],
            vec![Cell::Whole(2), Cell::Missing, Cell::Text("u".to_owned())],
        ])
    );
}

#[test]
fn the_skew_of_the_means_of_arrays_sorted_in_a_pivot_is_the_skew_of_each_group() {
    // The means of the arrays of the group `x` are 1, 3, 5 and 10, whose skew is
    // m3 / m2^1.5 with m2 = 11.1875 and m3 = 21.65625, 0.5787402450; the two of the group `y`
    // are 9 and 5, which are as far from their mean on either side.
    let links = || vec![link("Expr.arr.mean"), link("Expr.skew"), link("Expr.sort")];
    for (values, maintain_order) in [("w", false), ("w", true), ("d", true)] {
        let expected = vec![
            vec![
                Cell::Whole(1),
                Cell::Fraction(0.578_740_245_002_337),
                Cell::Missing,
            ],
            vec![Cell::Whole(2), Cell::Missing, Cell::Fraction(0.0)],
        ];
        let found = ran(&pivot(values, links(), maintain_order), &working(values));
        assert!(
            found.as_ref().is_ok_and(|found| same(found, &expected)),
            "{values} maintain_order {maintain_order}: {found:?}"
        );
    }
}

#[test]
fn the_sum_of_the_means_of_arrays_sorted_in_a_pivot_is_the_sum_of_each_group() {
    let links = vec![link("Expr.arr.mean"), link("Expr.sum"), link("Expr.sort")];
    let found = ran(&pivot("w", links, true), &working("w"));
    let expected = vec![
        vec![Cell::Whole(1), Cell::Fraction(19.0), Cell::Fraction(0.0)],
        vec![Cell::Whole(2), Cell::Fraction(0.0), Cell::Fraction(14.0)],
    ];
    assert!(
        found.as_ref().is_ok_and(|found| same(found, &expected)),
        "{found:?}"
    );
}
