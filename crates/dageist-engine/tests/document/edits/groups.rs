//! The calls after `group_by` in a step that groups, edited as the arguments of any call are.

use dageist_engine::catalog::StepFrame;
use dageist_engine::document::{EditProblem, Segment, Value as Argument};
use serde_json::{Value, json};

use super::{assert_steps, hole, inside, json_of, parameter, read, refused, select, step_at};
use crate::support::{call, chain, field, grouping, many, named};

fn columns() -> Vec<String> {
    vec!["k".to_owned(), "x".to_owned()]
}

#[test]
fn a_new_group_by_starts_with_a_key_still_to_be_picked_and_the_call_that_finishes_it() {
    let columns = columns();
    let document = read(vec![])
        .add_step(
            0,
            "LazyFrame.group_by",
            &StepFrame {
                columns: &columns,
                tables: &[],
            },
        )
        .expect("a step");
    // `agg` of one new column whose aggregate is still to be picked, as a new column of
    // `with_columns` is.
    let finishing = call(
        "LazyGroupBy.agg",
        json!({ "named_aggs": named(vec![("new_column", hole())]) }),
    );
    assert_steps(
        &document,
        vec![grouping(
            "group_by",
            json!({ "by": many(vec![hole()]) }),
            vec![finishing],
        )],
    );
}

#[test]
fn an_option_of_a_call_after_group_by_is_set_at_the_place_of_that_call() {
    let document = read(vec![grouping(
        "g",
        json!({ "by": many(vec![field("k")]) }),
        vec![call("LazyGroupBy.first", json!({}))],
    )]);
    let place = inside(0, "g", vec![Segment::Link(0), parameter("ignore_nulls")]);
    let edited = document
        .set_argument(&place, Argument::Flag(Some(true)))
        .expect("an option of the call");
    assert_steps(
        &edited,
        vec![grouping(
            "g",
            json!({ "by": many(vec![field("k")]) }),
            vec![call(
                "LazyGroupBy.first",
                json!({ "ignore_nulls": { "flag": true } }),
            )],
        )],
    );
    let after: Value = json_of(&edited);
    assert_eq!(after["steps"][0]["then"][0]["entry"], "LazyGroupBy.first");
}

fn having(name: &str) -> Value {
    call(
        "LazyGroupBy.having",
        json!({ "predicates": many(vec![field(name)]) }),
    )
}

fn by_k() -> Value {
    json!({ "by": many(vec![field("k")]) })
}

#[test]
fn a_having_is_added_before_the_call_that_finishes_the_grouping_and_starts_with_a_test() {
    let document = read(vec![grouping(
        "g",
        by_k(),
        vec![call("LazyGroupBy.len", json!({}))],
    )]);
    let edited = document
        .insert_group_call(&step_at(0, "g"), 0, "LazyGroupBy.having")
        .expect("a having");
    // An equality of a field and a value, each still to be picked, as a new filter starts.
    let test = chain(
        json!({ "hole": null }),
        vec![call("Expr.eq", json!({ "other": hole() }))],
    );
    assert_steps(
        &edited,
        vec![grouping(
            "g",
            by_k(),
            vec![
                call(
                    "LazyGroupBy.having",
                    json!({ "predicates": many(vec![test]) }),
                ),
                call("LazyGroupBy.len", json!({})),
            ],
        )],
    );
}

#[test]
fn the_call_that_finishes_the_grouping_is_swapped_for_another_and_keeps_what_fits() {
    let document = read(vec![grouping(
        "g",
        by_k(),
        vec![call(
            "LazyGroupBy.first",
            json!({ "ignore_nulls": { "flag": true } }),
        )],
    )]);
    let call_place = inside(0, "g", vec![Segment::Link(0)]);
    // `last` has the flag too, so it keeps the choice.
    let edited = document
        .swap_group_call(&call_place, "LazyGroupBy.last")
        .expect("a swap");
    assert_steps(
        &edited,
        vec![grouping(
            "g",
            by_k(),
            vec![call(
                "LazyGroupBy.last",
                json!({ "ignore_nulls": { "flag": true } }),
            )],
        )],
    );
    // `sum` has none to keep.
    let edited = document
        .swap_group_call(&call_place, "LazyGroupBy.sum")
        .expect("a swap");
    assert_steps(
        &edited,
        vec![grouping(
            "g",
            by_k(),
            vec![call("LazyGroupBy.sum", json!({}))],
        )],
    );
    // A quantile must be given its share, which it starts with.
    let edited = document
        .swap_group_call(&call_place, "LazyGroupBy.quantile")
        .expect("a swap");
    assert_steps(
        &edited,
        vec![grouping(
            "g",
            by_k(),
            vec![call(
                "LazyGroupBy.quantile",
                json!({ "quantile": { "float": 0.5 } }),
            )],
        )],
    );
}

#[test]
fn an_edit_that_leaves_the_grouping_unfinished_or_finished_twice_is_refused_with_the_reason() {
    let document = read(vec![grouping(
        "g",
        by_k(),
        vec![having("b"), call("LazyGroupBy.len", json!({}))],
    )]);
    let finishing = inside(0, "g", vec![Segment::Link(1)]);
    let error = refused(document.remove_group_call(&finishing));
    assert!(
        error.to_string().contains("needs a call that finishes it"),
        "{error}"
    );
    // The grouping is as it was.
    assert!(document.remove_group_call(&finishing).is_err());
    // The call that finishes it is swapped for a `having`, which does not.
    let error = refused(document.swap_group_call(&finishing, "LazyGroupBy.having"));
    assert!(
        matches!(error.problem(), EditProblem::Refused(_)),
        "{error}"
    );
    // A second `having`, and a second call that finishes the grouping.
    let error = refused(document.insert_group_call(&step_at(0, "g"), 0, "LazyGroupBy.having"));
    assert!(
        error.to_string().contains("a group by with its having"),
        "{error}"
    );
    let error = refused(document.insert_group_call(&step_at(0, "g"), 2, "LazyGroupBy.sum"));
    assert!(error.to_string().contains("a finished grouping"), "{error}");
    // Past the end, and a call that is no call of a group by.
    let error = refused(document.insert_group_call(&step_at(0, "g"), 3, "LazyGroupBy.sum"));
    assert!(matches!(
        error.problem(),
        EditProblem::OutOfRange { index: 3, len: 2 }
    ));
    let error = refused(document.insert_group_call(&step_at(0, "g"), 0, "Expr.sum"));
    assert!(
        matches!(error.problem(), EditProblem::NoStart { .. }),
        "{error}"
    );
    // What is not a call of a step is no place of one.
    let error = refused(document.remove_group_call(&inside(0, "g", vec![parameter("by")])));
    assert!(
        matches!(error.problem(), EditProblem::WrongPlace { .. }),
        "{error}"
    );
    let error = refused(document.remove_group_call(&inside(0, "g", vec![Segment::Link(2)])));
    assert!(matches!(error.problem(), EditProblem::Nothing), "{error}");
}

#[test]
fn a_step_that_does_not_group_takes_no_call_of_a_group_by() {
    let document = read(vec![select("s", &["k"])]);
    let error = refused(document.insert_group_call(&step_at(0, "s"), 0, "LazyGroupBy.len"));
    assert!(
        error.to_string().contains("a step that does not group"),
        "{error}"
    );
}

#[test]
fn every_call_of_a_group_by_is_put_in_and_reads_back_as_the_document_it_made() {
    use dageist_engine::catalog::{self, Applies, Build};
    use dageist_engine::document::Document;
    let base = read(vec![grouping(
        "g",
        by_k(),
        vec![call("LazyGroupBy.len", json!({}))],
    )]);
    let finishing = inside(0, "g", vec![Segment::Link(0)]);
    let mut made = 0;
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Grouped)
    {
        let id = entry.id().as_str();
        let edited = match entry.build() {
            Build::GroupedStep(_) => base.insert_group_call(&step_at(0, "g"), 0, id),
            _ => base.swap_group_call(&finishing, id),
        }
        .unwrap_or_else(|error| panic!("{id}: {error}"));
        let again = Document::read(&edited.write())
            .unwrap_or_else(|error| panic!("{id}: {error}\n{}", edited.write()));
        assert_eq!(again, edited, "{id}");
        made += 1;
    }
    assert_eq!(made, 15, "the calls of a group by");
}
