//! Which arguments are what their parameter holds when it is not given. Every expectation is
//! worked out from the defaults in Python Polars' signatures: `sort(descending=False,
//! nulls_last=False, maintain_order=False, multithreaded=True)`, `join(how="inner")`,
//! `Expr.shift(n=1)`, `Expr.over(descending=False, mapping_strategy="group_to_rows")`.

use dageist_engine::document::{Document, Inputs, Place, Segment};
use polars::prelude::*;
use serde_json::{Value as Json, json};

use crate::support::{call, chain, document, field, field_then, many, named, step, whole};

fn read(steps: Vec<Json>) -> Document {
    Document::read(&document(steps)).expect("a document")
}

/// The places of the arguments at their defaults in the first step, each as the words of the
/// way from the step: a parameter by its name, a link, an item and so on.
fn defaulted(steps: Vec<Json>) -> Vec<String> {
    let document = read(steps);
    let places = document.steps()[0].at_default(0);
    places.iter().map(words).collect()
}

fn words(place: &Place) -> String {
    place
        .path()
        .iter()
        .map(|segment| match segment {
            Segment::Parameter(name) => name.clone(),
            Segment::Item(at) => format!("item {at}"),
            Segment::Key(at) => format!("key {at}"),
            Segment::Value(at) => format!("value {at}"),
            Segment::Start => "start".to_owned(),
            Segment::Link(at) => format!("link {at}"),
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn flag(value: bool) -> Json {
    json!({ "flag": value })
}

/// Whether the argument `arguments` gives the parameter `parameter` of the step `entry` is what
/// the parameter holds when it is not given.
fn is_default(entry: &str, arguments: Json, parameter: &str) -> bool {
    read(vec![step("s", entry, arguments)]).steps()[0]
        .call()
        .is_default(parameter)
}

/// A join of `other` on `a` and `b`, with `extra` given besides.
fn join_with(extra: Json) -> Json {
    let mut arguments = json!({
        "other": { "table": "lookup" },
        "left_on": many(vec![field("a")]),
        "right_on": many(vec![field("b")]),
    });
    for (name, value) in extra.as_object().expect("an object") {
        arguments[name] = value.clone();
    }
    arguments
}

#[test]
fn a_flag_a_choice_and_text_are_at_the_default_when_they_equal_it() {
    let join =
        |extra: Json, parameter: &str| is_default("LazyFrame.join", join_with(extra), parameter);
    assert!(join(
        json!({ "allow_parallel": flag(true) }),
        "allow_parallel"
    ));
    assert!(!join(
        json!({ "allow_parallel": flag(false) }),
        "allow_parallel"
    ));
    // Polars decides where the default is `None`, which a flag left unset is.
    assert!(join(json!({ "coalesce": { "flag": null } }), "coalesce"));
    assert!(!join(json!({ "coalesce": flag(true) }), "coalesce"));
    assert!(!join(
        json!({ "nulls_equal": { "flag": null } }),
        "nulls_equal"
    ));
    assert!(join(json!({ "how": { "choice": "inner" } }), "how"));
    assert!(!join(json!({ "how": { "choice": "left" } }), "how"));
    assert!(join(json!({ "suffix": { "string": "_right" } }), "suffix"));
    assert!(!join(json!({ "suffix": { "string": "_x" } }), "suffix"));
    // A parameter that was not given is not given its default.
    assert!(!join(json!({}), "how"));
    assert!(!join(json!({}), "nothing"));
}

#[test]
fn a_list_and_pairs_are_at_the_default_when_they_are_empty_and_the_default_is_empty() {
    // `select(*exprs)` and `with_columns(**named_exprs)` default to nothing.
    assert!(is_default(
        "LazyFrame.select",
        json!({ "exprs": many(Vec::new()) }),
        "exprs"
    ));
    assert!(!is_default(
        "LazyFrame.select",
        json!({ "exprs": many(vec![field("a")]) }),
        "exprs"
    ));
    assert!(is_default(
        "LazyFrame.with_columns",
        json!({ "named_exprs": { "pairs": [] } }),
        "named_exprs"
    ));
    // `rename(mapping)` has none, so an empty mapping is not what it holds without one, and
    // neither is an empty list of keys that `join` leaves to `None`.
    assert!(!is_default(
        "LazyFrame.rename",
        json!({ "mapping": { "pairs": [] } }),
        "mapping"
    ));
    assert!(!is_default(
        "LazyFrame.join",
        join_with(json!({ "left_on": many(Vec::new()) })),
        "left_on"
    ));
}

#[test]
fn a_single_flag_for_every_key_is_at_the_default_when_it_equals_it() {
    let sort = |descending: Json| {
        is_default(
            "LazyFrame.sort",
            json!({ "by": many(vec![field("a")]), "descending": descending }),
            "descending",
        )
    };
    assert!(sort(flag(false)));
    assert!(!sort(flag(true)));
}

#[test]
fn a_step_names_the_arguments_that_are_at_their_default() {
    let sort = step(
        "s",
        "LazyFrame.sort",
        json!({
            "by": many(vec![field("a"), field("b")]),
            "descending": many(vec![flag(false), flag(false)]),
            "nulls_last": flag(true),
            "maintain_order": flag(false),
            "multithreaded": flag(true),
        }),
    );
    // `by` is not at a default, `nulls_last` differs from its.
    assert_eq!(
        defaulted(vec![sort]),
        ["descending", "maintain_order", "multithreaded"]
    );
    let join = step(
        "s",
        "LazyFrame.join",
        json!({
            "other": { "table": "lookup" },
            "how": { "choice": "left" },
            "left_on": many(vec![field("a")]),
            "right_on": many(vec![field("b")]),
            "validate": { "choice": "m:m" },
        }),
    );
    assert_eq!(defaulted(vec![join]), ["validate"]);
}

#[test]
fn the_calls_inside_an_expression_name_theirs_at_the_place_of_their_link() {
    let shifted = step(
        "s",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![(
            "x",
            field_then(
                "a",
                vec![
                    call("Expr.shift", json!({ "n": whole(1) })),
                    call(
                        "Expr.over",
                        json!({
                            "partition_by": many(vec![field("g")]),
                            "descending": flag(false),
                            "mapping_strategy": { "choice": "join" },
                        }),
                    ),
                ],
            ),
        )]) }),
    );
    // The first argument of `shift` is at its default of 1; `over` keeps `descending` at its
    // default and sets `mapping_strategy` to another word than `group_to_rows`.
    assert_eq!(
        defaulted(vec![shifted]),
        [
            "named_exprs/value 0/link 0/n",
            "named_exprs/value 0/link 1/descending"
        ]
    );
}

#[test]
fn a_list_for_each_item_is_at_the_default_only_when_it_has_one_value_for_each_item() {
    // `sort(by=[a, b], descending=[False])` and `descending=[]` are refused, by Python Polars as
    // by the engine, for a list of another length than the keys: they are not the default.
    let sort = |by: Vec<Json>, more_by: Option<Vec<Json>>, descending: Json| {
        let mut arguments = json!({ "by": many(by), "descending": descending });
        if let Some(more) = more_by {
            arguments["more_by"] = many(more);
        }
        defaulted(vec![step("s", "LazyFrame.sort", arguments)])
    };
    let keys = || vec![field("a"), field("b")];
    assert_eq!(
        sort(keys(), None, many(vec![flag(false), flag(false)])),
        ["descending"]
    );
    assert!(sort(keys(), None, many(vec![flag(false)])).is_empty());
    assert!(sort(keys(), None, many(Vec::new())).is_empty());
    assert!(sort(keys(), None, many(vec![flag(false); 3])).is_empty());
    // The keys are `by` and the items of `more_by` together.
    assert_eq!(
        sort(
            vec![field("a")],
            Some(vec![field("b")]),
            many(vec![flag(false), flag(false)])
        ),
        ["descending"]
    );
    assert!(
        sort(
            vec![field("a")],
            Some(vec![field("b")]),
            many(vec![flag(false)])
        )
        .is_empty()
    );
}

#[test]
fn a_number_is_the_default_of_an_expression_only_as_the_count_it_is_the_default_of() {
    // `shift(n=1)` counts rows: a whole number is that count. A number with a fraction, a
    // text and a number given to any other parameter are not what it holds by default.
    let shifted = |n: Json| {
        defaulted(vec![step(
            "s",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![(
                "x",
                field_then("a", vec![call("Expr.shift", json!({ "n": n }))]),
            )]) }),
        )])
    };
    let float = chain(
        call("polars.lit", json!({ "value": { "float": 1.0 } })),
        Vec::new(),
    );
    assert_eq!(shifted(whole(1)), ["named_exprs/value 0/link 0/n"]);
    assert!(shifted(float).is_empty());
    assert!(shifted(whole(2)).is_empty());
}

#[test]
fn a_step_report_carries_the_places_whether_or_not_the_step_ran() {
    let sort = |id: &str, by: &str| {
        step(
            id,
            "LazyFrame.sort",
            json!({ "by": many(vec![field(by)]), "descending": flag(false) }),
        )
    };
    let document = read(vec![sort("first", "a"), sort("second", "missing")]);
    let source = df!("a" => [1_i64, 2]).expect("a table").lazy();
    let report = document.run(&Inputs::new(), source).expect("a report");
    let [first, second] = report.steps() else {
        panic!("two steps");
    };
    assert!(first.schema().is_some());
    // Polars refuses the column the second step names, and its places are still given.
    assert!(second.schema().is_none());
    for (index, (reported, id)) in [(first, "first"), (second, "second")]
        .into_iter()
        .enumerate()
    {
        let places = reported.at_default();
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].step_id().map(|step| step.as_str()), Some(id));
        assert_eq!(places[0].step_index(), Some(index));
        assert_eq!(words(&places[0]), "descending");
    }
}
