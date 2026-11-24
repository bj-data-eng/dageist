//! Changing what an expression starts from, and putting links into it, taking
//! them away, moving them and swapping them.

use dageist_engine::allowed::{After, Context, Outer, Place as OfferPlace, Representative, Shape};
use dageist_engine::document::{Cause, EditProblem, Expression, Segment, Start};
use serde_json::{Value as Json, json};

use super::{assert_steps, hole, inside, parameter, read, refused};
use crate::support::{call, chain, col_call, field, field_then, many, step, text, whole};

/// A step `calc` that selects the one expression `item`.
fn select(item: Json) -> Json {
    step(
        "calc",
        "LazyFrame.select",
        json!({ "exprs": many(vec![item]) }),
    )
}

/// The place of the one expression of `calc`.
fn item() -> dageist_engine::document::Place {
    inside(0, "calc", vec![parameter("exprs"), Segment::Item(0)])
}

/// The place of the link `at` of the one expression of `calc`.
fn link(at: usize) -> dageist_engine::document::Place {
    item().then(Segment::Link(at))
}

/// Where a link added after a column of whole numbers in a select would stand.
fn after_whole_numbers() -> OfferPlace {
    OfferPlace::new(
        Context::step(Outer::Select),
        After::Value {
            dtype: Representative::Int64,
            shape: Shape::Many,
        },
    )
}

fn started_test() -> Json {
    chain(
        json!({ "hole": null }),
        vec![call("Expr.eq", json!({ "other": hole() }))],
    )
}

/// The start and links a condition begins as, when nothing is picked.
fn started_condition() -> (Json, Vec<Json>) {
    (
        call(
            "polars.when",
            json!({ "predicates": many(vec![started_test()]) }),
        ),
        vec![
            call("When.then", json!({ "statement": hole() })),
            call("Then.otherwise", json!({ "statement": hole() })),
        ],
    )
}

#[test]
fn the_start_of_an_expression_is_changed_and_its_links_stay() {
    let document = read(vec![select(field_then(
        "a",
        vec![call("Expr.abs", json!({}))],
    ))]);
    let Start::Call(call_of_b) = Expression::field("b").expect("b").start().clone() else {
        panic!("a field starts with a call");
    };
    let edited = document
        .set_start(&item(), Start::Call(call_of_b))
        .expect("set");
    assert_steps(
        &edited,
        vec![select(field_then("b", vec![call("Expr.abs", json!({}))]))],
    );
    let input = document
        .set_start(&item(), Start::Input("holidays".to_owned()))
        .expect("set");
    assert_steps(
        &input,
        vec![select(chain(
            json!({ "input": "holidays" }),
            vec![call("Expr.abs", json!({}))],
        ))],
    );
    let picked_again = document.set_start(&item(), Start::Hole).expect("set");
    assert_steps(
        &picked_again,
        vec![select(chain(
            json!({ "hole": null }),
            vec![call("Expr.abs", json!({}))],
        ))],
    );
}

#[test]
fn starting_from_when_makes_the_condition_with_a_test_and_a_then_and_an_otherwise_to_pick() {
    let document = read(vec![select(hole())]);
    let edited = document
        .start_with(&item(), "polars.when")
        .expect("started");
    let (start, links) = started_condition();
    assert_steps(&edited, vec![select(chain(start, links))]);
}

#[test]
fn starting_from_element_makes_a_start_with_no_arguments() {
    let document = read(vec![select(field("a"))]);
    let edited = document
        .start_with(&item(), "polars.element")
        .expect("started");
    assert_steps(
        &edited,
        vec![select(chain(call("polars.element", json!({})), vec![]))],
    );
}

#[test]
fn starting_from_another_function_keeps_the_methods_and_drops_the_links_of_a_condition() {
    let (start, mut links) = started_condition();
    links.push(call("Expr.abs", json!({})));
    let document = read(vec![select(chain(start, links))]);
    let edited = document
        .start_with(&item(), "polars.element")
        .expect("started");
    assert_steps(
        &edited,
        vec![select(chain(
            call("polars.element", json!({})),
            vec![call("Expr.abs", json!({}))],
        ))],
    );
}

#[test]
fn a_method_is_no_start_of_an_expression() {
    let error = refused(read(vec![select(hole())]).start_with(&item(), "Expr.abs"));
    assert!(matches!(
        error.problem(),
        EditProblem::Refused(Cause::WrongPlace {
            entry: "Expr.abs",
            ..
        })
    ));
    assert_eq!(error.place(), &item().then(Segment::Start));
}

#[test]
fn a_link_added_where_the_tables_allow_polars_default_is_given_it() {
    let document = read(vec![select(field("a"))]);
    let edited = document
        .insert_link(&item(), 0, "Expr.shift", Some(&after_whole_numbers()))
        .expect("added");
    // Polars shifts by one row unless it is asked otherwise.
    assert_steps(
        &edited,
        vec![select(field_then(
            "a",
            vec![call("Expr.shift", json!({ "n": whole(1) }))],
        ))],
    );
}

#[test]
fn a_link_added_where_nothing_is_known_of_the_place_has_a_hole_for_its_argument() {
    let edited = read(vec![select(field("a"))])
        .insert_link(&item(), 0, "Expr.shift", None)
        .expect("added");
    assert_steps(
        &edited,
        vec![select(field_then(
            "a",
            vec![call("Expr.shift", json!({ "n": hole() }))],
        ))],
    );
}

#[test]
fn a_method_that_takes_no_argument_is_added_with_none() {
    let edited = read(vec![select(field("a"))])
        .insert_link(&item(), 0, "Expr.abs", Some(&after_whole_numbers()))
        .expect("added");
    assert_steps(
        &edited,
        vec![select(field_then("a", vec![call("Expr.abs", json!({}))]))],
    );
}

#[test]
fn a_method_whose_argument_may_be_left_to_polars_is_added_without_it() {
    let edited = read(vec![select(field("a"))])
        .insert_link(&item(), 0, "Expr.str.strip_chars", None)
        .expect("added");
    // Left out, it trims whitespace.
    assert_steps(
        &edited,
        vec![select(field_then(
            "a",
            vec![call("Expr.str.strip_chars", json!({}))],
        ))],
    );
}

#[test]
fn what_a_method_must_be_given_besides_its_argument_starts_as_a_hole_or_is_left_out() {
    let edited = read(vec![select(field("a"))])
        .insert_link(&item(), 0, "Expr.str.replace", None)
        .expect("added");
    // The pattern is typed in, so there is none yet; what replaces a match is an expression.
    assert_steps(
        &edited,
        vec![select(field_then(
            "a",
            vec![call("Expr.str.replace", json!({ "value": hole() }))],
        ))],
    );
}

#[test]
fn a_link_is_put_in_the_middle_of_a_chain() {
    let document = read(vec![select(field_then(
        "a",
        vec![call("Expr.abs", json!({})), call("Expr.is_null", json!({}))],
    ))]);
    let edited = document
        .insert_link(&item(), 1, "Expr.shift", None)
        .expect("added");
    assert_steps(
        &edited,
        vec![select(field_then(
            "a",
            vec![
                call("Expr.abs", json!({})),
                call("Expr.shift", json!({ "n": hole() })),
                call("Expr.is_null", json!({})),
            ],
        ))],
    );
}

#[test]
fn swapping_a_comparison_keeps_what_it_compares_with() {
    let compare = |entry: &str, other: Json| {
        step(
            "calc",
            "LazyFrame.filter",
            json!({ "predicates": many(vec![field_then("a", vec![call(entry, json!({ "other": other }))])]) }),
        )
    };
    let place = inside(
        0,
        "calc",
        vec![parameter("predicates"), Segment::Item(0), Segment::Link(0)],
    );
    let document = read(vec![compare("Expr.gt", whole(3))]);
    let edited = document
        .swap_link(&place, "Expr.le", None)
        .expect("swapped");
    assert_steps(&edited, vec![compare("Expr.le", whole(3))]);
    // A comparison with nothing picked to compare with starts again.
    let unpicked = read(vec![compare("Expr.gt", hole())])
        .swap_link(&place, "Expr.lt", None)
        .expect("swapped");
    assert_steps(&unpicked, vec![compare("Expr.lt", hole())]);
}

#[test]
fn swapping_a_method_keeps_the_options_the_new_method_has_too() {
    let replacing = |entry: &str, extra: Json| {
        let mut arguments = json!({
            "pattern": { "pattern": "x" },
            "value": text("y"),
            "literal": { "flag": true },
        });
        for (name, value) in extra.as_object().expect("an object") {
            arguments[name] = value.clone();
        }
        select(field_then("a", vec![call(entry, arguments)]))
    };
    let document = read(vec![replacing(
        "Expr.str.replace",
        json!({ "n": { "integer": 2 } }),
    )]);
    let edited = document
        .swap_link(&link(0), "Expr.str.replace_all", None)
        .expect("swapped");
    // `n` is not a parameter of `replace_all`, so it goes; the others stay.
    assert_steps(&edited, vec![replacing("Expr.str.replace_all", json!({}))]);
}

#[test]
fn a_link_is_taken_away_and_moved() {
    let document = read(vec![select(field_then(
        "a",
        vec![
            call("Expr.abs", json!({})),
            call("Expr.is_null", json!({})),
            call("Expr.shift", json!({ "n": whole(2) })),
        ],
    ))]);
    let removed = document.remove_link(&link(1)).expect("removed");
    assert_steps(
        &removed,
        vec![select(field_then(
            "a",
            vec![
                call("Expr.abs", json!({})),
                call("Expr.shift", json!({ "n": whole(2) })),
            ],
        ))],
    );
    let moved = document.move_link(&link(2), 0).expect("moved");
    assert_steps(
        &moved,
        vec![select(field_then(
            "a",
            vec![
                call("Expr.shift", json!({ "n": whole(2) })),
                call("Expr.abs", json!({})),
                call("Expr.is_null", json!({})),
            ],
        ))],
    );
}

#[test]
fn a_link_of_a_condition_that_would_stand_out_of_order_is_refused_where_it_would_stand() {
    let (start, links) = started_condition();
    let document = read(vec![select(chain(start, links))]);
    let error = refused(document.remove_link(&link(0)));
    assert!(matches!(
        error.problem(),
        EditProblem::Refused(Cause::OutOfOrder {
            entry: "Then.otherwise",
            ..
        })
    ));
    assert_eq!(error.place(), &link(0));
}

#[test]
fn a_method_cannot_come_before_the_then_of_a_condition_it_follows() {
    let (start, links) = started_condition();
    let document = read(vec![select(chain(start, links))]);
    let error = refused(document.insert_link(&item(), 0, "Expr.abs", None));
    assert!(matches!(
        error.problem(),
        EditProblem::Refused(Cause::OutOfOrder {
            entry: "Expr.abs",
            ..
        })
    ));
}

#[test]
fn a_hole_start_that_is_set_to_a_function_call_that_is_no_start_is_refused() {
    let document = read(vec![select(hole())]);
    let call_of_abs = dageist_engine::document::Call::new(
        dageist_engine::catalog::entry("Expr.abs").expect("abs"),
        [],
    )
    .expect("a call");
    let error = refused(document.set_start(&item(), Start::Call(call_of_abs)));
    assert!(matches!(
        error.problem(),
        EditProblem::Refused(Cause::WrongPlace {
            entry: "Expr.abs",
            ..
        })
    ));
    assert_eq!(error.place(), &item().then(Segment::Start));
}

#[test]
fn the_field_a_test_reads_is_the_start_of_its_expression() {
    let document = read(vec![step(
        "calc",
        "LazyFrame.filter",
        json!({ "predicates": many(vec![started_test()]) }),
    )]);
    let Start::Call(a) = Expression::field("a").expect("a").start().clone() else {
        panic!("a call");
    };
    let edited = document
        .set_start(
            &inside(0, "calc", vec![parameter("predicates"), Segment::Item(0)]),
            Start::Call(a),
        )
        .expect("set");
    assert_steps(
        &edited,
        vec![step(
            "calc",
            "LazyFrame.filter",
            json!({ "predicates": many(vec![chain(
                col_call("a"),
                vec![call("Expr.eq", json!({ "other": hole() }))],
            )]) }),
        )],
    );
}

#[test]
fn swapping_a_method_carries_the_argument_into_the_shape_the_new_method_holds() {
    let shifted = read(vec![select(field_then(
        "a",
        vec![call("Expr.shift", json!({ "n": whole(2) }))],
    ))]);
    // `over` takes a list of expressions, so the one expression becomes a list of one.
    let over = shifted
        .swap_link(&link(0), "Expr.over", None)
        .expect("swapped");
    assert_steps(
        &over,
        vec![select(field_then(
            "a",
            vec![call(
                "Expr.over",
                json!({ "partition_by": many(vec![whole(2)]) }),
            )],
        ))],
    );
    // And the first of a list becomes the one expression of a method that takes one.
    let back = over
        .swap_link(&link(0), "Expr.shift", None)
        .expect("swapped");
    assert_steps(
        &back,
        vec![select(field_then(
            "a",
            vec![call("Expr.shift", json!({ "n": whole(2) }))],
        ))],
    );
}
