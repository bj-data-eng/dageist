//! Adding, swapping, removing and moving steps.

use dageist_engine::catalog::StepFrame;
use dageist_engine::document::{
    Document, EditProblem, Expression, Inputs, Outcome, Value as Argument,
};
use polars::prelude::*;
use serde_json::{Value, json};

use super::{assert_steps, hole, inside, parameter, read, refused, select, step_at};
use crate::support::{call, chain, field, many, named, step};

fn columns() -> Vec<String> {
    vec!["a".to_owned(), "b".to_owned()]
}

fn frame<'a>(columns: &'a [String], tables: &'a [String]) -> StepFrame<'a> {
    StepFrame { columns, tables }
}

fn empty() -> Document {
    read(vec![])
}

/// The step a new `entry` is, started on columns `a` and `b`, written out.
fn added(entry: &str) -> Value {
    let columns = columns();
    let document = empty()
        .add_step(0, entry, &frame(&columns, &[]))
        .expect("a step");
    super::json_of(&document)["steps"][0].clone()
}

#[test]
fn a_sort_starts_with_one_key_still_to_be_picked() {
    assert_eq!(
        added("LazyFrame.sort"),
        step(
            "sort",
            "LazyFrame.sort",
            json!({ "by": many(vec![hole()]) })
        )
    );
}

#[test]
fn a_filter_starts_with_an_equality_of_a_field_and_a_value_still_to_be_picked() {
    let equality = chain(
        json!({ "hole": null }),
        vec![call("Expr.eq", json!({ "other": hole() }))],
    );
    assert_eq!(
        added("LazyFrame.filter"),
        step(
            "filter",
            "LazyFrame.filter",
            json!({ "predicates": many(vec![equality]) })
        )
    );
}

#[test]
fn a_select_starts_by_keeping_every_column() {
    assert_eq!(
        added("LazyFrame.select"),
        step(
            "select",
            "LazyFrame.select",
            json!({ "exprs": many(vec![field("a"), field("b")]) })
        )
    );
}

#[test]
fn a_new_column_starts_as_new_column_holding_a_value_still_to_be_picked() {
    assert_eq!(
        added("LazyFrame.with_columns"),
        step(
            "with_columns",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![("new_column", hole())]) })
        )
    );
}

#[test]
fn a_join_starts_with_the_first_table_and_keeps_the_rows_that_have_no_match() {
    let columns = columns();
    let tables = vec!["claims".to_owned(), "members".to_owned()];
    let document = empty()
        .add_step(0, "LazyFrame.join", &frame(&columns, &tables))
        .expect("a join");
    assert_steps(
        &document,
        vec![step(
            "join",
            "LazyFrame.join",
            json!({
                "other": { "table": "claims" },
                "left_on": many(vec![hole()]),
                "right_on": many(vec![hole()]),
                "how": { "choice": "left" },
            }),
        )],
    );
}

#[test]
fn a_join_with_no_table_to_bring_in_starts_without_one_and_is_refused_when_run() {
    let columns = columns();
    let document = empty()
        .add_step(0, "LazyFrame.join", &frame(&columns, &[]))
        .expect("a join");
    assert!(document.steps()[0].call().argument("other").is_none());
    // Its keys are still to be picked, which is what the report says first; with the keys
    // picked, it is the table that is missing.
    let key = |name: &str| {
        Argument::Many(vec![Argument::Expression(
            Expression::field(name).expect("a"),
        )])
    };
    let keyed = document
        .set_argument(&inside(0, "join", vec![parameter("left_on")]), key("a"))
        .and_then(|document| {
            document.set_argument(&inside(0, "join", vec![parameter("right_on")]), key("a"))
        })
        .expect("keys");
    let source = df!("a" => [1_i64, 2], "b" => ["x", "y"]).expect("a table");
    let report = keyed.run(&Inputs::new(), source.lazy()).expect("a report");
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Refused(_)),
        "{:?}",
        report.steps()[0].outcome()
    );
}

#[test]
fn a_step_that_takes_nothing_to_start_has_no_arguments() {
    assert_eq!(
        added("LazyFrame.head"),
        step("head", "LazyFrame.head", json!({}))
    );
}

#[test]
fn a_new_step_goes_where_it_is_asked_and_the_others_keep_their_places() {
    let columns = columns();
    let document = read(vec![select("first", &["a"]), select("last", &["b"])]);
    let edited = document
        .add_step(1, "LazyFrame.sort", &frame(&columns, &[]))
        .expect("a step");
    let ids: Vec<&str> = edited
        .steps()
        .iter()
        .map(|step| step.id().as_str())
        .collect();
    assert_eq!(ids, ["first", "sort", "last"]);
    // The others are as they were.
    assert_eq!(edited.steps()[0], document.steps()[0]);
    assert_eq!(edited.steps()[2], document.steps()[1]);
}

#[test]
fn a_new_step_is_named_for_its_entry_and_a_number_when_the_name_is_taken() {
    let columns = columns();
    let frame = frame(&columns, &[]);
    let once = empty().add_step(0, "LazyFrame.sort", &frame).expect("one");
    let twice = once.add_step(1, "LazyFrame.sort", &frame).expect("two");
    let thrice = twice.add_step(0, "LazyFrame.sort", &frame).expect("three");
    let ids: Vec<&str> = thrice
        .steps()
        .iter()
        .map(|step| step.id().as_str())
        .collect();
    assert_eq!(ids, ["sort_3", "sort", "sort_2"]);
}

#[test]
fn a_name_that_a_removed_step_had_is_free_again() {
    let columns = columns();
    let frame = frame(&columns, &[]);
    let document = read(vec![select("select", &["a"]), select("select_2", &["b"])]);
    let without = document
        .remove_step(&step_at(0, "select"))
        .expect("removed");
    let again = without
        .add_step(0, "LazyFrame.select", &frame)
        .expect("added");
    assert_eq!(again.steps()[0].id().as_str(), "select");
}

#[test]
fn swapping_a_step_keeps_its_name_and_place_and_starts_the_new_entry_afresh() {
    let columns = columns();
    let document = read(vec![select("keep", &["a"]), select("swapped", &["b"])]);
    let edited = document
        .swap_step(
            &step_at(1, "swapped"),
            "LazyFrame.sort",
            &frame(&columns, &[]),
        )
        .expect("swapped");
    assert_steps(
        &edited,
        vec![
            select("keep", &["a"]),
            step(
                "swapped",
                "LazyFrame.sort",
                json!({ "by": many(vec![hole()]) }),
            ),
        ],
    );
}

#[test]
fn removing_a_step_takes_it_away_and_leaves_the_others() {
    let document = read(vec![
        select("one", &["a"]),
        select("two", &["b"]),
        select("three", &["a", "b"]),
    ]);
    let edited = document.remove_step(&step_at(1, "two")).expect("removed");
    assert_steps(
        &edited,
        vec![select("one", &["a"]), select("three", &["a", "b"])],
    );
}

#[test]
fn a_step_moved_forward_and_back_gives_the_same_order_it_had() {
    let document = read(vec![
        select("one", &["a"]),
        select("two", &["b"]),
        select("three", &["a", "b"]),
    ]);
    let forward = document.move_step(&step_at(0, "one"), 2).expect("moved");
    let ids: Vec<&str> = forward
        .steps()
        .iter()
        .map(|step| step.id().as_str())
        .collect();
    assert_eq!(ids, ["two", "three", "one"]);
    let back = forward
        .move_step(&step_at(2, "one"), 0)
        .expect("moved back");
    assert_eq!(back, document);
}

#[test]
fn a_step_that_is_added_is_incomplete_where_something_is_still_to_be_picked() {
    let columns = columns();
    let document = empty()
        .add_step(0, "LazyFrame.filter", &frame(&columns, &[]))
        .expect("a filter");
    let source = df!("a" => [1_i64, 2], "b" => ["x", "y"]).expect("a table");
    let report = document
        .run(&Inputs::new(), source.lazy())
        .expect("a report");
    let Outcome::Incomplete { place } = report.steps()[0].outcome() else {
        panic!("incomplete, not {:?}", report.steps()[0].outcome());
    };
    // The field of the equality is the first thing still to be picked.
    assert_eq!(
        place,
        &inside(
            0,
            "filter",
            vec![
                parameter("predicates"),
                dageist_engine::document::Segment::Item(0),
                dageist_engine::document::Segment::Start,
            ]
        )
    );
}

#[test]
fn an_entry_that_is_no_step_is_refused_and_so_is_one_the_catalogue_lacks() {
    let columns = columns();
    let frame = frame(&columns, &[]);
    let not_a_step = refused(empty().add_step(0, "Expr.abs", &frame));
    assert!(matches!(
        not_a_step.problem(),
        EditProblem::NoStart { entry: "Expr.abs" }
    ));
    let unknown = refused(empty().add_step(0, "LazyFrame.fly", &frame));
    assert!(matches!(unknown.problem(), EditProblem::UnknownEntry(id) if id == "LazyFrame.fly"));
}
