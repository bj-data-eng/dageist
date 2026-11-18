//! Which arguments are what their parameter holds when it is not given. Every expectation is
//! worked out from the defaults in Python Polars' signatures: `sort(descending=False,
//! nulls_last=False, maintain_order=False, multithreaded=True)`, `join(how="inner")`,
//! `Expr.shift(n=1)`, `Expr.over(descending=False, mapping_strategy="group_to_rows")`.

use dageist_engine::catalog::{self, Parameter};
use dageist_engine::document::{Document, Inputs, Place, Segment, Value};
use polars::prelude::*;
use serde_json::{Value as Json, json};

use crate::support::{call, chain, document, field, field_then, many, named, step, whole};

fn parameter(entry: &str, name: &str) -> &'static Parameter {
    catalog::entry(entry)
        .expect("an entry")
        .parameters()
        .iter()
        .find(|parameter| parameter.name() == name)
        .expect("a parameter")
}

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

#[test]
fn a_flag_a_choice_and_text_are_at_the_default_when_they_equal_it() {
    let flag_default = parameter("LazyFrame.sort", "multithreaded");
    assert!(Value::Flag(Some(true)).is_default_of(flag_default));
    assert!(!Value::Flag(Some(false)).is_default_of(flag_default));
    // Polars decides where the default is `None`, which a flag left unset is.
    let decided = parameter("LazyFrame.join", "coalesce");
    assert!(Value::Flag(None).is_default_of(decided));
    assert!(!Value::Flag(Some(true)).is_default_of(decided));
    assert!(!Value::Flag(None).is_default_of(flag_default));
    let how = parameter("LazyFrame.join", "how");
    assert!(Value::Choice("inner".to_owned()).is_default_of(how));
    assert!(!Value::Choice("left".to_owned()).is_default_of(how));
    let suffix = parameter("LazyFrame.join", "suffix");
    assert!(Value::String("_right".to_owned()).is_default_of(suffix));
    assert!(!Value::String("_x".to_owned()).is_default_of(suffix));
}

#[test]
fn a_number_is_at_the_default_whether_it_has_a_fraction_or_not() {
    let n = parameter("Expr.shift", "n");
    assert!(Value::Integer(1).is_default_of(n));
    assert!(Value::Float(1.0).is_default_of(n));
    assert!(!Value::Integer(2).is_default_of(n));
    assert!(!Value::Float(1.5).is_default_of(n));
}

#[test]
fn an_expression_is_at_the_default_only_when_it_is_the_number_alone() {
    let n = parameter("Expr.shift", "n");
    let expression = |argument: Json| {
        let document = read(vec![step(
            "s",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![("x", argument)]) }),
        )]);
        let step = &document.steps()[0];
        let Some(Value::Pairs(pairs)) = step.call().argument("named_exprs") else {
            panic!("the pairs of the step");
        };
        pairs[0].1.clone()
    };
    assert!(expression(whole(1)).is_default_of(n));
    assert!(!expression(whole(2)).is_default_of(n));
    assert!(!expression(field("a")).is_default_of(n));
    assert!(!expression(field_then("a", vec![call("Expr.sum", json!({}))])).is_default_of(n));
}

#[test]
fn a_list_and_pairs_are_at_the_default_when_they_are_empty_and_the_default_is_empty() {
    // `select(*exprs)` and `with_columns(**named_exprs)` default to nothing.
    assert!(Value::Many(Vec::new()).is_default_of(parameter("LazyFrame.select", "exprs")));
    assert!(
        !Value::Many(vec![Value::Integer(1)]).is_default_of(parameter("LazyFrame.select", "exprs"))
    );
    assert!(
        Value::Pairs(Vec::new()).is_default_of(parameter("LazyFrame.with_columns", "named_exprs"))
    );
    // `rename(mapping)` has none, so an empty mapping is not what it holds without one, and
    // neither is a list of keys that `join` leaves to `None`.
    assert!(!Value::Pairs(Vec::new()).is_default_of(parameter("LazyFrame.rename", "mapping")));
    assert!(!Value::Many(Vec::new()).is_default_of(parameter("LazyFrame.join", "left_on")));
}

#[test]
fn a_value_for_each_item_is_at_the_default_when_every_item_is() {
    let descending = parameter("LazyFrame.sort", "descending");
    let each = |flags: &[bool]| Value::Many(flags.iter().map(|f| Value::Flag(Some(*f))).collect());
    assert!(each(&[false, false]).is_default_of(descending));
    assert!(!each(&[false, true]).is_default_of(descending));
    // A single flag stands for every item.
    assert!(Value::Flag(Some(false)).is_default_of(descending));
    assert!(!Value::Flag(Some(true)).is_default_of(descending));
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
