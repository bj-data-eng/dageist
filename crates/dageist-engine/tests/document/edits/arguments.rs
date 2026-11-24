//! Setting, clearing and adding arguments, and putting items and entries into
//! the lists and pairs they are made of.

use dageist_engine::allowed::Literal;
use dageist_engine::catalog::BuildError;
use dageist_engine::document::{Cause, EditProblem, Expression, Segment, Value};
use serde_json::{Value as Json, json};

use super::{assert_steps, hole, inside, parameter, read, refused, step_at};
use crate::support::{call, chain, field, field_then, many, named, step, whole};

fn flag(value: bool) -> Json {
    json!({ "flag": value })
}

fn columns_of(names: &[&str]) -> Json {
    many(
        names
            .iter()
            .map(|name| json!({ "columns": [name] }))
            .collect(),
    )
}

/// A sort by the fields `a` and `b`, with `extra` arguments.
fn sort(extra: Json) -> Json {
    let mut arguments = json!({ "by": many(vec![field("a"), field("b")]) });
    for (name, value) in extra.as_object().expect("an object") {
        arguments[name] = value.clone();
    }
    step("order", "LazyFrame.sort", arguments)
}

fn join(how: &str) -> Json {
    step(
        "joined",
        "LazyFrame.join",
        json!({
            "other": { "table": "members" },
            "left_on": many(vec![field("a")]),
            "right_on": many(vec![field("a")]),
            "how": { "choice": how },
        }),
    )
}

fn drop_columns() -> Json {
    step(
        "dropped",
        "LazyFrame.drop",
        json!({ "columns": columns_of(&["a", "b"]) }),
    )
}

#[test]
fn a_choice_is_set_by_giving_its_parameter_a_word() {
    let document = read(vec![join("left")]);
    let edited = document
        .set_argument(
            &inside(0, "joined", vec![parameter("how")]),
            Value::Choice("inner".to_owned()),
        )
        .expect("set");
    assert_steps(&edited, vec![join("inner")]);
}

#[test]
fn a_table_is_set_by_its_name() {
    let document = read(vec![join("left")]);
    let edited = document
        .set_argument(
            &inside(0, "joined", vec![parameter("other")]),
            Value::Table("claims".to_owned()),
        )
        .expect("set");
    let mut expected = join("left");
    expected["call"]["arguments"]["other"] = json!({ "table": "claims" });
    assert_steps(&edited, vec![expected]);
}

#[test]
fn an_argument_that_was_not_given_is_added_where_the_entry_puts_its_parameter() {
    let document = read(vec![step(
        "joined",
        "LazyFrame.join",
        json!({ "other": { "table": "members" }, "how": { "choice": "left" } }),
    )]);
    let edited = document
        .set_argument(
            &inside(0, "joined", vec![parameter("left_on")]),
            Value::Many(vec![Value::Expression(Expression::field("a").expect("a"))]),
        )
        .expect("set");
    // `left_on` comes after `how` in Python's signature, so it is written after it.
    let written = super::json_of(&edited);
    let arguments = written["steps"][0]["call"]["arguments"]
        .as_object()
        .expect("an object");
    assert_eq!(
        arguments.keys().map(String::as_str).collect::<Vec<_>>(),
        ["other", "how", "left_on"]
    );
}

#[test]
fn clearing_an_argument_takes_it_away_so_that_its_parameter_holds_its_default() {
    let document = read(vec![join("left")]);
    let edited = document
        .clear_argument(&inside(0, "joined", vec![parameter("how")]))
        .expect("cleared");
    let mut expected = join("left");
    expected["call"]["arguments"]
        .as_object_mut()
        .expect("an object")
        .remove("how");
    assert_steps(&edited, vec![expected]);
}

#[test]
fn the_key_and_the_value_of_a_new_column_are_set_each_on_its_own() {
    let new_column = |name: &str, value: Json| {
        step(
            "added",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![(name, value)]) }),
        )
    };
    let document = read(vec![new_column("total", field("a"))]);
    let renamed = document
        .set_argument(
            &inside(0, "added", vec![parameter("named_exprs"), Segment::Key(0)]),
            Value::Columns(vec!["sum".to_owned()]),
        )
        .expect("renamed");
    assert_steps(&renamed, vec![new_column("sum", field("a"))]);
    let held = renamed
        .set_argument(
            &inside(
                0,
                "added",
                vec![parameter("named_exprs"), Segment::Value(0)],
            ),
            Value::Expression(Expression::field("b").expect("b")),
        )
        .expect("set");
    assert_steps(&held, vec![new_column("sum", field("b"))]);
}

#[test]
fn an_item_of_a_list_of_columns_is_replaced() {
    let edited = read(vec![drop_columns()])
        .set_argument(
            &inside(0, "dropped", vec![parameter("columns"), Segment::Item(1)]),
            Value::Columns(vec!["c".to_owned()]),
        )
        .expect("set");
    assert_steps(
        &edited,
        vec![step(
            "dropped",
            "LazyFrame.drop",
            json!({ "columns": columns_of(&["a", "c"]) }),
        )],
    );
}

#[test]
fn a_column_is_added_to_a_list_of_columns_and_taken_away() {
    let document = read(vec![drop_columns()]);
    let list = inside(0, "dropped", vec![parameter("columns")]);
    let added = document
        .insert_item(&list, 1, Value::Columns(vec!["c".to_owned()]))
        .expect("added");
    let expected = |names: &[&str]| {
        vec![step(
            "dropped",
            "LazyFrame.drop",
            json!({ "columns": columns_of(names) }),
        )]
    };
    assert_steps(&added, expected(&["a", "c", "b"]));
    let at_end = added
        .insert_item(&list, 3, Value::Columns(vec!["d".to_owned()]))
        .expect("added at the end");
    assert_steps(&at_end, expected(&["a", "c", "b", "d"]));
    let removed = at_end
        .remove_item(&inside(
            0,
            "dropped",
            vec![parameter("columns"), Segment::Item(0)],
        ))
        .expect("removed");
    assert_steps(&removed, expected(&["c", "b", "d"]));
}

#[test]
fn a_key_added_to_a_sort_gets_the_default_for_each_per_key_option() {
    let document = read(vec![sort(
        json!({ "descending": many(vec![flag(true), flag(false)]) }),
    )]);
    let by = inside(0, "order", vec![parameter("by")]);
    let edited = document
        .insert_item(
            &by,
            1,
            Value::Expression(Expression::field("c").expect("c")),
        )
        .expect("added");
    let written = super::json_of(&edited);
    let arguments = &written["steps"][0]["call"]["arguments"];
    assert_eq!(
        arguments["by"],
        many(vec![field("a"), field("c"), field("b")])
    );
    // Polars' default of `descending` is false.
    assert_eq!(
        arguments["descending"],
        many(vec![flag(true), flag(false), flag(false)])
    );
}

#[test]
fn a_key_taken_from_a_sort_takes_its_per_key_option_with_it() {
    let document = read(vec![sort(
        json!({ "descending": many(vec![flag(true), flag(false)]) }),
    )]);
    let edited = document
        .remove_item(&inside(0, "order", vec![parameter("by"), Segment::Item(0)]))
        .expect("removed");
    assert_steps(
        &edited,
        vec![step(
            "order",
            "LazyFrame.sort",
            json!({ "by": many(vec![field("b")]), "descending": many(vec![flag(false)]) }),
        )],
    );
}

/// A sort by `a`, then `more_by` the fields `b`, with `extra` arguments.
fn sort_with_more_keys(extra: Json) -> Json {
    let mut arguments = json!({
        "by": many(vec![field("a")]),
        "more_by": many(vec![field("b")]),
    });
    for (name, value) in extra.as_object().expect("an object") {
        arguments[name] = value.clone();
    }
    step("order", "LazyFrame.sort", arguments)
}

#[test]
fn a_key_added_to_the_more_keys_of_a_sort_gets_the_default_after_the_keys_before_it() {
    // The keys are `a` then `b`, so `descending` holds a value for each.
    let document = read(vec![sort_with_more_keys(
        json!({ "descending": many(vec![flag(true), flag(false)]) }),
    )]);
    let edited = document
        .insert_item(
            &inside(0, "order", vec![parameter("more_by")]),
            1,
            Value::Expression(Expression::field("c").expect("c")),
        )
        .expect("added");
    // `c` is the third key, so the new default goes after the two there are.
    let written = super::json_of(&edited);
    let arguments = &written["steps"][0]["call"]["arguments"];
    assert_eq!(arguments["more_by"], many(vec![field("b"), field("c")]));
    assert_eq!(
        arguments["descending"],
        many(vec![flag(true), flag(false), flag(false)])
    );
}

#[test]
fn a_key_added_to_the_first_keys_of_a_sort_goes_before_the_more_keys_in_the_per_key_option() {
    let document = read(vec![sort_with_more_keys(
        json!({ "descending": many(vec![flag(true), flag(false)]) }),
    )]);
    let edited = document
        .insert_item(
            &inside(0, "order", vec![parameter("by")]),
            0,
            Value::Expression(Expression::field("c").expect("c")),
        )
        .expect("added");
    let written = super::json_of(&edited);
    assert_eq!(
        written["steps"][0]["call"]["arguments"]["descending"],
        many(vec![flag(false), flag(true), flag(false)])
    );
}

#[test]
fn a_key_taken_from_the_more_keys_of_a_sort_takes_its_value_of_the_per_key_option_with_it() {
    let document = read(vec![sort_with_more_keys(
        json!({ "descending": many(vec![flag(true), flag(false)]) }),
    )]);
    let edited = document
        .remove_item(&inside(
            0,
            "order",
            vec![parameter("more_by"), Segment::Item(0)],
        ))
        .expect("removed");
    // `b` was the second key, whose value is the second.
    assert_steps(
        &edited,
        vec![step(
            "order",
            "LazyFrame.sort",
            json!({
                "by": many(vec![field("a")]),
                "more_by": many(vec![]),
                "descending": many(vec![flag(true)]),
            }),
        )],
    );
}

#[test]
fn keys_added_by_giving_more_by_its_first_item_take_a_value_for_each_per_key_option() {
    // One key and one value; the option for more keys starts with one key still to be picked.
    let document = read(vec![step(
        "order",
        "LazyFrame.sort",
        json!({ "by": many(vec![field("a")]), "descending": many(vec![flag(true)]) }),
    )]);
    let edited = document
        .add_option(&step_at(0, "order"), "more_by")
        .expect("added");
    assert_steps(
        &edited,
        vec![step(
            "order",
            "LazyFrame.sort",
            json!({
                "by": many(vec![field("a")]),
                "more_by": many(vec![hole()]),
                "descending": many(vec![flag(true), flag(false)]),
            }),
        )],
    );
}

#[test]
fn keys_taken_away_by_clearing_by_take_their_values_of_each_per_key_option() {
    let document = read(vec![sort(
        json!({ "descending": many(vec![flag(true), flag(false)]) }),
    )]);
    let edited = document
        .clear_argument(&inside(0, "order", vec![parameter("by")]))
        .expect("cleared");
    assert_steps(
        &edited,
        vec![step(
            "order",
            "LazyFrame.sort",
            json!({ "descending": many(vec![]) }),
        )],
    );
}

#[test]
fn keys_set_as_a_whole_list_leave_each_per_key_option_a_value_for_each() {
    let by = inside(0, "order", vec![parameter("by")]);
    let keys = |names: &[&str]| {
        Value::Many(
            names
                .iter()
                .map(|name| Value::Expression(Expression::field(*name).expect("a field")))
                .collect(),
        )
    };
    let document = read(vec![sort(
        json!({ "descending": many(vec![flag(true), flag(false)]) }),
    )]);
    // Three keys: the new one holds Polars' default.
    let longer = document
        .set_argument(&by, keys(&["a", "b", "c"]))
        .expect("set");
    let written = super::json_of(&longer);
    assert_eq!(
        written["steps"][0]["call"]["arguments"]["descending"],
        many(vec![flag(true), flag(false), flag(false)])
    );
    // One key: the values of the others go.
    let shorter = longer.set_argument(&by, keys(&["a"])).expect("set");
    let written = super::json_of(&shorter);
    assert_eq!(
        written["steps"][0]["call"]["arguments"]["descending"],
        many(vec![flag(true)])
    );
}

#[test]
fn keys_set_as_a_whole_list_leave_one_value_for_every_key_as_it_is() {
    let document = read(vec![sort(json!({ "descending": flag(true) }))]);
    let edited = document
        .set_argument(
            &inside(0, "order", vec![parameter("by")]),
            Value::Many(vec![Value::Expression(Expression::field("a").expect("a"))]),
        )
        .expect("set");
    let written = super::json_of(&edited);
    assert_eq!(
        written["steps"][0]["call"]["arguments"]["descending"],
        flag(true)
    );
}

#[test]
fn a_per_key_option_is_set_for_one_key_and_the_other_keys_keep_the_default() {
    let edited = read(vec![sort(json!({}))])
        .set_for_item(
            &step_at(0, "order"),
            "descending",
            1,
            Value::Flag(Some(true)),
        )
        .expect("set");
    assert_steps(
        &edited,
        vec![sort(
            json!({ "descending": many(vec![flag(false), flag(true)]) }),
        )],
    );
}

#[test]
fn a_per_key_option_that_was_one_value_for_every_key_keeps_it_for_the_others() {
    let edited = read(vec![sort(json!({ "descending": flag(true) }))])
        .set_for_item(
            &step_at(0, "order"),
            "descending",
            0,
            Value::Flag(Some(false)),
        )
        .expect("set");
    assert_steps(
        &edited,
        vec![sort(
            json!({ "descending": many(vec![flag(false), flag(true)]) }),
        )],
    );
}

#[test]
fn a_per_key_option_that_held_too_few_values_gives_the_default_to_the_keys_it_had_none_for() {
    // Two keys and one value: the second key holds Polars' default of `descending`, which is false.
    let edited = read(vec![sort(json!({ "descending": many(vec![flag(true)]) }))])
        .set_for_item(
            &step_at(0, "order"),
            "descending",
            0,
            Value::Flag(Some(true)),
        )
        .expect("set");
    assert_steps(
        &edited,
        vec![sort(
            json!({ "descending": many(vec![flag(true), flag(false)]) }),
        )],
    );
}

#[test]
fn a_flag_option_starts_with_the_word_that_is_not_its_default() {
    let edited = read(vec![sort(json!({}))])
        .add_option(&step_at(0, "order"), "descending")
        .expect("added");
    // Polars' default of `descending` is false, so the option starts as true.
    assert_steps(&edited, vec![sort(json!({ "descending": flag(true) }))]);
}

#[test]
fn a_choice_option_starts_with_the_first_word_that_is_not_its_default() {
    let edited = read(vec![step(
        "joined",
        "LazyFrame.join",
        json!({ "other": { "table": "members" } }),
    )])
    .add_option(&step_at(0, "joined"), "validate")
    .expect("added");
    let written = super::json_of(&edited);
    // The default of `validate` is `m:m`, the first word of its choice.
    assert_eq!(
        written["steps"][0]["call"]["arguments"]["validate"],
        json!({ "choice": "m:1" })
    );
}

#[test]
fn an_option_that_polars_refuses_every_other_word_of_cannot_be_added_and_says_why() {
    // Beside a cross join Polars refuses every way to validate that is not its default.
    let cross = step(
        "joined",
        "LazyFrame.join",
        json!({ "other": { "table": "members" }, "how": { "choice": "cross" } }),
    );
    let error = refused(read(vec![cross]).add_option(&step_at(0, "joined"), "validate"));
    let EditProblem::CannotAdd(refusal) = error.problem() else {
        panic!("{error:?}");
    };
    assert!(
        matches!(
            refusal.source(),
            dageist_engine::allowed::RefusalSource::Polars(_)
        ),
        "{refusal:?}"
    );
    assert!(!refusal.reason().is_empty());
    assert_eq!(
        error.place(),
        &inside(0, "joined", vec![parameter("validate")])
    );
}

#[test]
fn an_option_that_is_already_given_is_left_as_it_is() {
    let document = read(vec![sort(json!({ "descending": flag(false) }))]);
    let edited = document
        .add_option(&step_at(0, "order"), "descending")
        .expect("added");
    assert_eq!(edited, document);
}

#[test]
fn an_option_that_takes_an_expression_starts_as_one_still_to_be_picked() {
    let edited = read(vec![step(
        "shifting",
        "LazyFrame.select",
        json!({ "exprs": many(vec![field_then("a", vec![call("Expr.shift", json!({}))])]) }),
    )])
    .add_option(
        &inside(
            0,
            "shifting",
            vec![parameter("exprs"), Segment::Item(0), Segment::Link(0)],
        ),
        "fill_value",
    )
    .expect("added");
    let written = super::json_of(&edited);
    assert_eq!(
        written["steps"][0]["call"]["arguments"]["exprs"]["many"][0]["expression"]["then"][0]["arguments"]
            ["fill_value"],
        hole()
    );
}

#[test]
fn an_option_that_is_typed_in_cannot_start_until_it_has_a_value() {
    let document = read(vec![step("indexed", "LazyFrame.with_row_index", json!({}))]);
    let error = refused(document.add_option(&step_at(0, "indexed"), "name"));
    assert!(matches!(
        error.problem(),
        EditProblem::NeedsValue { parameter: "name" }
    ));
    assert_eq!(
        error.place(),
        &inside(0, "indexed", vec![parameter("name")])
    );
    // It is given its value in the same way as any other argument.
    let edited = document
        .set_argument(
            &inside(0, "indexed", vec![parameter("name")]),
            Value::String("row".to_owned()),
        )
        .expect("set");
    assert_steps(
        &edited,
        vec![step(
            "indexed",
            "LazyFrame.with_row_index",
            json!({ "name": { "string": "row" } }),
        )],
    );
}

#[test]
fn an_option_the_entry_does_not_have_is_refused_by_name() {
    let error = refused(read(vec![sort(json!({}))]).add_option(&step_at(0, "order"), "colour"));
    let EditProblem::Refused(Cause::Arguments(BuildError::UnknownParameter { parameter })) =
        error.problem()
    else {
        panic!("{error:?}");
    };
    assert_eq!(parameter, "colour");
    assert_eq!(
        error.place(),
        &inside(0, "order", vec![parameter_named("colour")])
    );
}

fn parameter_named(name: &str) -> Segment {
    parameter(name)
}

#[test]
fn a_test_added_to_a_filter_is_an_equality_with_nothing_picked() {
    let filter = |tests: Vec<Json>| {
        step(
            "kept",
            "LazyFrame.filter",
            json!({ "predicates": many(tests) }),
        )
    };
    let existing = chain(
        crate::support::col_call("a"),
        vec![call("Expr.gt", json!({ "other": whole(3) }))],
    );
    let document = read(vec![filter(vec![existing.clone()])]);
    let edited = document
        .insert_test(&inside(0, "kept", vec![parameter("predicates")]), 0)
        .expect("added");
    let started = chain(
        json!({ "hole": null }),
        vec![call("Expr.eq", json!({ "other": hole() }))],
    );
    assert_steps(&edited, vec![filter(vec![started, existing])]);
}

#[test]
fn an_entry_is_added_to_pairs_and_taken_away_by_its_key_or_its_value() {
    let rename = |entries: Vec<(&str, &str)>| {
        step(
            "renamed",
            "LazyFrame.rename",
            json!({ "mapping": { "pairs": entries
                .into_iter()
                .map(|(from, to)| json!([{ "columns": [from] }, { "string": to }]))
                .collect::<Vec<_>>() } }),
        )
    };
    let mapping = inside(0, "renamed", vec![parameter("mapping")]);
    let document = read(vec![rename(vec![("a", "x")])]);
    let added = document
        .insert_entry(
            &mapping,
            1,
            Value::Columns(vec!["b".to_owned()]),
            Value::String("y".to_owned()),
        )
        .expect("added");
    assert_steps(&added, vec![rename(vec![("a", "x"), ("b", "y")])]);
    let removed = added
        .remove_entry(&inside(
            0,
            "renamed",
            vec![parameter("mapping"), Segment::Value(0)],
        ))
        .expect("removed");
    assert_steps(&removed, vec![rename(vec![("b", "y")])]);
    let by_key = added
        .remove_entry(&inside(
            0,
            "renamed",
            vec![parameter("mapping"), Segment::Key(1)],
        ))
        .expect("removed");
    assert_steps(&by_key, vec![rename(vec![("a", "x")])]);
}

fn shifted(n: Json) -> Json {
    step(
        "shifting",
        "LazyFrame.select",
        json!({ "exprs": many(vec![field_then("a", vec![call("Expr.shift", json!({ "n": n }))])]) }),
    )
}

#[test]
fn the_argument_of_a_link_inside_an_expression_is_set_by_its_place() {
    let document = read(vec![shifted(whole(1))]);
    let n = inside(
        0,
        "shifting",
        vec![
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(0),
            parameter("n"),
        ],
    );
    let edited = document
        .set_argument(
            &n,
            Value::Expression(Expression::literal(&Literal::Integer(2)).expect("a literal")),
        )
        .expect("set");
    assert_steps(&edited, vec![shifted(whole(2))]);
    let input = document
        .set_argument(&n, Value::Expression(Expression::input("holidays")))
        .expect("set");
    assert_steps(
        &input,
        vec![shifted(chain(json!({ "input": "holidays" }), vec![]))],
    );
    let cleared = edited.clear_argument(&n).expect("cleared");
    assert_steps(
        &cleared,
        vec![step(
            "shifting",
            "LazyFrame.select",
            json!({ "exprs": many(vec![field_then("a", vec![call("Expr.shift", json!({}))])]) }),
        )],
    );
}

#[test]
fn a_float_literal_is_an_expression_of_a_float() {
    let edited = read(vec![shifted(whole(1))])
        .set_argument(
            &inside(
                0,
                "shifting",
                vec![
                    parameter("exprs"),
                    Segment::Item(0),
                    Segment::Link(0),
                    parameter("n"),
                ],
            ),
            Value::Expression(Expression::literal(&Literal::Float(1.5)).expect("a literal")),
        )
        .expect("set");
    assert_steps(
        &edited,
        vec![shifted(chain(
            call("polars.lit", json!({ "value": { "float": 1.5 } })),
            vec![],
        ))],
    );
}

#[test]
fn an_argument_of_the_wrong_kind_is_refused_at_the_item_it_was_put_in() {
    let error = refused(read(vec![drop_columns()]).set_argument(
        &inside(0, "dropped", vec![parameter("columns"), Segment::Item(0)]),
        Value::Integer(5),
    ));
    assert!(matches!(
        error.problem(),
        EditProblem::Refused(Cause::Arguments(BuildError::WrongKind {
            parameter: "columns",
            ..
        }))
    ));
    assert_eq!(
        error.place(),
        &inside(0, "dropped", vec![parameter("columns"), Segment::Item(0)])
    );
}

#[test]
fn a_number_that_is_not_finite_is_refused_where_it_was_put() {
    let document = read(vec![step(
        "close",
        "LazyFrame.select",
        json!({ "exprs": many(vec![field_then("a", vec![call("Expr.is_close", json!({ "other": field("b") }))])]) }),
    )]);
    let error = refused(document.set_argument(
        &inside(
            0,
            "close",
            vec![
                parameter("exprs"),
                Segment::Item(0),
                Segment::Link(0),
                parameter("abs_tol"),
            ],
        ),
        Value::Float(f64::NAN),
    ));
    assert!(matches!(
        error.problem(),
        EditProblem::Refused(Cause::NotFinite)
    ));
}
