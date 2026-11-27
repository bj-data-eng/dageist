//! The calls after `group_by` in a step that groups, edited as the arguments of any call are.

use dageist_engine::catalog::StepFrame;
use dageist_engine::document::{Segment, Value as Argument};
use serde_json::{Value, json};

use super::{assert_steps, hole, inside, json_of, parameter, read};
use crate::support::{call, field, grouping, many, named};

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
