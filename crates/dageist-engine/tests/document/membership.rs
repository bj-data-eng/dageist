//! `implode`, which gathers the values of an expression into one list, and `is_in`, which tests
//! whether each value is among the values of a list.
//!
//! The table has the whole numbers `a` (1, 2, 3) and `b` (2, 3, 4), the text `s` and `t`, and a
//! column `l` of lists of whole numbers (`[1, 2]`, `[3]`, `[4, 5]`), and a list of the whole
//! numbers 2 and 3 is connected as `wanted`. Every expected value is worked out by hand from what
//! Python Polars 2.2.0 gives for the same call.

use dageist_engine::document::{Document, Inputs, ListOrigin, Outcome, StepId, Walk};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, field_then, many, step};

fn table() -> DataFrame {
    let lists = Series::new(
        "l".into(),
        [vec![1_i64, 2], vec![3], vec![4, 5]].map(|values| Series::new("".into(), values)),
    );
    df!(
        "a" => [1_i64, 2, 3],
        "b" => [2_i64, 3, 4],
        "s" => ["x", "y", "z"],
        "t" => ["y", "z", "w"],
    )
    .expect("the table")
    .hstack(&[lists.into()])
    .expect("a column of three rows")
}

fn inputs() -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_list(
            "wanted",
            ListOrigin::Parameter,
            Series::new("wanted".into(), [2_i64, 3]),
        )
        .expect("a list");
    inputs
}

fn from_input(name: &str, then: Vec<Value>) -> Value {
    chain(json!({ "input": name }), then)
}

/// The document that selects `expression` as the column `out`.
fn selecting(expression: Value) -> String {
    document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![expression]) }),
    )])
}

fn walk_of(expression: Value) -> Walk {
    Document::read(&selecting(expression))
        .expect("a document")
        .steps()[0]
        .walk(0, table().schema(), &inputs())
}

/// The first column of the rows the document gives, as Polars writes them.
fn cells(expression: Value) -> Vec<Option<String>> {
    Document::read(&selecting(expression))
        .expect("a document")
        .preview(&inputs(), &table(), &StepId::new("s").expect("an id"), 100)
        .expect("the rows")
        .rows()
        .iter()
        .map(|row| row[0].clone())
        .collect()
}

fn cell_texts(cells: &[&str]) -> Vec<Option<String>> {
    cells.iter().map(|cell| Some((*cell).to_owned())).collect()
}

/// The dtype of the one column the document gives.
fn dtype_of(expression: Value) -> DataType {
    let report = Document::read(&selecting(expression))
        .expect("a document")
        .run_on(&inputs(), &table())
        .expect("the table has a schema");
    match report.steps()[0].outcome() {
        Outcome::Valid { schema, .. } => schema
            .iter()
            .next()
            .map(|(_, dtype)| dtype.clone())
            .expect("a column"),
        other => panic!("{other:?}"),
    }
}

fn implode() -> Value {
    call("Expr.implode", json!({}))
}

fn is_in(other: Value) -> Value {
    call("Expr.is_in", json!({ "other": other }))
}

#[test]
fn implode_gives_the_values_gathered_into_one_list() {
    let gathered = field_then("b", vec![implode()]);
    assert_eq!(
        dtype_of(gathered.clone()),
        DataType::List(Box::new(DataType::Int64))
    );
    assert_eq!(cells(gathered.clone()), cell_texts(&["[2, 3, 4]"]));
    assert_eq!(walk_of(gathered).refusal(), None);
}

#[test]
fn implode_given_the_flag_that_drops_the_order_gives_a_list_of_the_dtype_of_the_values() {
    let unordered = field_then(
        "t",
        vec![call(
            "Expr.implode",
            json!({ "maintain_order": { "flag": false } }),
        )],
    );
    assert_eq!(
        dtype_of(unordered),
        DataType::List(Box::new(DataType::String))
    );
}

#[test]
fn is_in_given_a_collection_imploded_tells_for_each_value_whether_it_is_among_them() {
    // 1, 2 and 3 among 2, 3 and 4.
    let among = field_then("a", vec![is_in(field_then("b", vec![implode()]))]);
    assert_eq!(walk_of(among.clone()).refusal(), None);
    assert_eq!(dtype_of(among.clone()), DataType::Boolean);
    assert_eq!(cells(among), cell_texts(&["false", "true", "true"]));
}

#[test]
fn is_in_given_a_list_column_tests_each_value_against_the_list_of_its_row() {
    // 1 in [1, 2], 2 in [3], 3 in [4, 5].
    let among = field_then("a", vec![is_in(field("l"))]);
    assert_eq!(walk_of(among.clone()).refusal(), None);
    assert_eq!(cells(among), cell_texts(&["true", "false", "false"]));
}

#[test]
fn is_in_tests_a_value_typed_in_against_a_collection_imploded() {
    let among = chain(
        call("polars.lit", json!({ "value": { "integer": 3 } })),
        vec![is_in(field_then("b", vec![implode()]))],
    );
    assert_eq!(walk_of(among.clone()).refusal(), None);
    assert_eq!(cells(among), cell_texts(&["true"]));
}
