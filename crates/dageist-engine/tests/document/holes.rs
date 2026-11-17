//! A hole is where the editor has not picked what an expression starts from.
//! A step that holds one is walked up to it, reported incomplete at the hole's
//! place, and not built; the steps after it are not run.

use dageist_engine::allowed::{After, Context, Outer, Place as OfferPlace, Representative, Shape};
use dageist_engine::document::{Document, Inputs, Outcome, ReadError, Segment};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, many, step};

fn source() -> DataFrame {
    df!("a" => [1_i64, 2, 3], "b" => ["x", "y", "z"]).expect("the table")
}

/// The expression argument that starts with nothing picked and continues with `then`.
fn hole(then: Vec<Value>) -> Value {
    chain(json!({ "hole": null }), then)
}

fn shift_by_a_hole() -> Value {
    chain(
        call("polars.col", json!({ "name": { "columns": ["a"] } })),
        vec![call("Expr.shift", json!({ "n": hole(vec![]) }))],
    )
}

fn parameter(name: &str) -> Segment {
    Segment::Parameter(name.to_owned())
}

#[test]
fn a_hole_is_read_and_written_back_as_it_was() {
    let text = document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![shift_by_a_hole()]) }),
    )]);
    let read = Document::read(&text).expect("a document with a hole");
    let written: Value = serde_json::from_str(&read.write()).expect("JSON");
    let expected: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(written, expected);
}

#[test]
fn a_hole_that_is_not_null_is_refused_with_its_place() {
    let text = document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![chain(json!({ "hole": 1 }), vec![])]) }),
    )]);
    let ReadError::Refused(refusal) = Document::read(&text).expect_err("refused") else {
        panic!("a syntax error");
    };
    assert_eq!(refusal.place().step_index(), Some(0));
    assert!(refusal.cause().to_string().contains("hole"), "{refusal}");
}

#[test]
fn a_step_with_a_hole_is_incomplete_at_the_hole_and_walked_up_to_it() {
    let text = document(vec![
        step(
            "first",
            "LazyFrame.select",
            json!({ "exprs": many(vec![shift_by_a_hole()]) }),
        ),
        step(
            "second",
            "LazyFrame.select",
            json!({ "exprs": many(vec![field("a")]) }),
        ),
    ]);
    let document = Document::read(&text).expect("a document");
    let report = document
        .run(&Inputs::new(), source().lazy())
        .expect("a report");
    let first = &report.steps()[0];
    let Outcome::Incomplete { place } = first.outcome() else {
        panic!("incomplete, not {:?}", first.outcome());
    };
    // The argument `n` of the shift, which is the first link, at its start.
    assert_eq!(
        place.path(),
        [
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(0),
            parameter("n"),
            Segment::Start
        ]
    );
    assert_eq!(place.step_id().map(|id| id.as_str()), Some("first"));
    // The chain is walked up to the hole: the method stands after a column of whole numbers.
    let walked = first
        .walk()
        .expressions()
        .iter()
        .find(|expression| expression.place().path() == [parameter("exprs"), Segment::Item(0)])
        .expect("the item");
    let select = Context::step(Outer::Select);
    assert_eq!(
        walked.places()[0],
        Some(OfferPlace::new(
            select,
            After::Value {
                dtype: Representative::Int64,
                shape: Shape::Many
            }
        ))
    );
    assert_eq!(first.walk().refusal(), None);
    // Nothing after it is run, and it says which step blocked it.
    assert!(matches!(
        report.steps()[1].outcome(),
        Outcome::NotRun { blocked_by } if blocked_by.as_str() == "first"
    ));
}

#[test]
fn a_step_with_a_hole_gives_no_rows() {
    let text = document(vec![step(
        "first",
        "LazyFrame.select",
        json!({ "exprs": many(vec![hole(vec![])]) }),
    )]);
    let document = Document::read(&text).expect("a document");
    let id = document.steps()[0].id().clone();
    assert!(document.rows(&Inputs::new(), &source(), &id, 10).is_err());
}
