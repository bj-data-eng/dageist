//! What the walk says of the methods that take some of the values they follow, and of the others
//! whose count of values the data or the arguments decide: how many values a chain gives, where
//! each place takes such a count, and what it is met with.
//!
//! The table is the walk tests' own, whose columns are `a` (`Int64`), `b` (`String`), `f`
//! (`Boolean`) and `k` (`Int64`). Every expected value is worked out by hand from what Python
//! Polars 2.1.0 does with three rows of them and from the explanation of the catalogue, not read
//! from a run.

use dageist_engine::allowed::{
    After, Context, Count, Ending, Outer, RefusalSource, Representative, Rule, Shape, Stage,
};
use dageist_engine::document::{Document, Inputs, Segment, Walk, WalkedExpression};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, field_then, many, step, whole};

fn source() -> DataFrame {
    df!(
        "a" => [1_i64, 2, 3],
        "b" => ["x", "y", "z"],
        "f" => [true, false, true],
        "k" => [1_i64, 1, 2],
    )
    .expect("the table")
}

/// A table another table joins: `rk` holds whole numbers.
fn inputs() -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "other",
            "Other",
            df!("ra" => ["x", "y", "z"], "rk" => [1_i64, 2, 3])
                .expect("the other table")
                .lazy(),
        )
        .expect("a first input");
    inputs
}

fn walk_of(entry: &str, arguments: Value) -> Walk {
    let document =
        Document::read(&document(vec![step("s", entry, arguments)])).expect("a document");
    document.steps()[0].walk(0, source().schema(), &inputs())
}

/// The walk of a select of `items`.
fn select_of(items: Vec<Value>) -> Walk {
    walk_of("LazyFrame.select", json!({ "exprs": many(items) }))
}

/// The walk of a step that adds `item` to the table.
fn adding(item: Value) -> Walk {
    walk_of(
        "LazyFrame.with_columns",
        json!({ "exprs": many(vec![item]) }),
    )
}

/// The `at`th item of the select, or of the step that adds columns.
fn item(walk: &Walk, at: usize) -> &WalkedExpression {
    let path = [Segment::Parameter("exprs".to_owned()), Segment::Item(at)];
    walk.expressions()
        .iter()
        .find(|expression| expression.place().path() == path)
        .unwrap_or_else(|| panic!("no item {at}"))
}

/// What the expression has at its end.
fn last_after(expression: &WalkedExpression) -> After {
    expression
        .places()
        .last()
        .copied()
        .flatten()
        .expect("a place after the last call")
        .after()
}

fn after(dtype: Representative, shape: Shape) -> After {
    After::Value { dtype, shape }
}

fn head(n: i64) -> Value {
    call("Expr.head", json!({ "n": whole(n) }))
}

fn on(name: &str, calls: Vec<Value>) -> Value {
    field_then(name, calls)
}

fn refusal_source(walk: &Walk) -> RefusalSource {
    walk.refusal().expect("a refusal").refusal().source()
}

#[test]
fn a_method_that_takes_some_of_the_values_gives_a_count_the_arguments_state() {
    let predicate = json!({ "predicates": many(vec![field("f")]) });
    let cases: Vec<(&str, Vec<Value>, Count)> = vec![
        ("head", vec![head(2)], Count::AtMost(2)),
        (
            "limit",
            vec![call("Expr.limit", json!({ "n": whole(4) }))],
            Count::AtMost(4),
        ),
        (
            "tail",
            vec![call("Expr.tail", json!({ "n": whole(3) }))],
            Count::AtMost(3),
        ),
        (
            "top_k",
            vec![call("Expr.top_k", json!({ "k": whole(2) }))],
            Count::AtMost(2),
        ),
        (
            "bottom_k",
            vec![call("Expr.bottom_k", json!({ "k": whole(1) }))],
            Count::AtMost(1),
        ),
        (
            "slice with a length",
            vec![call(
                "Expr.slice",
                json!({ "offset": whole(1), "length": whole(2) }),
            )],
            Count::AtMost(2),
        ),
        (
            "slice with none",
            vec![call("Expr.slice", json!({ "offset": whole(1) }))],
            Count::NotKnown,
        ),
        (
            "gather_every of the rows",
            vec![call("Expr.gather_every", json!({ "n": { "integer": 2 } }))],
            Count::NotKnown,
        ),
        (
            "gather_every of the first five",
            vec![
                head(5),
                call("Expr.gather_every", json!({ "n": { "integer": 2 } })),
            ],
            Count::AtMost(3),
        ),
        (
            "unique of the rows",
            vec![call("Expr.unique", json!({}))],
            Count::NotKnown,
        ),
        (
            "unique of the first five",
            vec![head(5), call("Expr.unique", json!({}))],
            Count::AtMost(5),
        ),
        ("mode", vec![call("Expr.mode", json!({}))], Count::NotKnown),
        (
            "arg_unique",
            vec![call("Expr.arg_unique", json!({}))],
            Count::NotKnown,
        ),
        (
            "unique_counts",
            vec![call("Expr.unique_counts", json!({}))],
            Count::NotKnown,
        ),
        (
            "filter",
            vec![call("Expr.filter", predicate.clone())],
            Count::NotKnown,
        ),
        (
            "diff that drops",
            vec![call(
                "Expr.diff",
                json!({ "null_behavior": { "choice": "drop" } }),
            )],
            Count::NotKnown,
        ),
        (
            "head of the first five",
            vec![head(5), head(2)],
            Count::AtMost(2),
        ),
        (
            "head of the first two",
            vec![head(2), head(5)],
            Count::AtMost(2),
        ),
        (
            "a method that gives a value for each of them",
            vec![head(2), call("Expr.abs", json!({}))],
            Count::AtMost(2),
        ),
    ];
    for (name, calls, count) in cases {
        let walk = select_of(vec![on("a", calls)]);
        let expression = item(&walk, 0);
        assert_eq!(
            last_after(expression),
            after(Representative::Int64, Shape::Counted),
            "{name}"
        );
        assert_eq!(expression.count(), Some(count), "{name}");
        assert_eq!(expression.ending(), Some(&Ending::Allowed), "{name}");
        assert_eq!(walk.refusal(), None, "{name}");
    }
}

#[test]
fn a_difference_gives_as_many_values_as_the_rows_unless_it_drops_the_first() {
    let walk = select_of(vec![on("a", vec![call("Expr.diff", json!({}))])]);
    assert_eq!(
        last_after(item(&walk, 0)),
        after(Representative::Int64, Shape::Many)
    );
    assert_eq!(item(&walk, 0).count(), None);
}

#[test]
fn a_chain_that_goes_on_to_an_aggregate_is_one_value_whatever_count_its_first_method_gives() {
    for (name, calls) in [
        ("head", vec![head(2), call("Expr.sum", json!({}))]),
        (
            "filter",
            vec![
                call(
                    "Expr.filter",
                    json!({ "predicates": many(vec![field("f")]) }),
                ),
                call("Expr.max", json!({})),
            ],
        ),
        (
            "unique",
            vec![call("Expr.unique", json!({})), call("Expr.len", json!({}))],
        ),
    ] {
        let walk = select_of(vec![on("a", calls)]);
        assert_eq!(
            last_after(item(&walk, 0)),
            after(
                if name == "unique" {
                    Representative::UInt32
                } else {
                    Representative::Int64
                },
                Shape::One
            ),
            "{name}"
        );
        assert_eq!(item(&walk, 0).count(), None, "{name}");
    }
}

#[test]
fn one_value_that_a_method_takes_some_of_is_one_value() {
    let walk = select_of(vec![
        on("a", vec![call("Expr.sum", json!({})), head(2)]),
        chain(
            call("polars.lit", json!({ "value": { "integer": 1 } })),
            vec![call("Expr.tail", json!({ "n": whole(3) }))],
        ),
    ]);
    // A number typed without a dtype is an `Int32` where nothing beside it says another.
    for (at, dtype) in [(0, Representative::Int64), (1, Representative::Int32)] {
        assert_eq!(
            last_after(item(&walk, at)),
            after(dtype, Shape::One),
            "item {at}"
        );
    }
}

#[test]
fn the_values_where_a_condition_holds_are_a_count_even_of_one_value() {
    // `a.sum().filter(f)` is the sum, once for each row where `f` is true, and `lit(1).filter(f)`
    // too: the condition is met by one value, so its count is the condition's.
    let walk = select_of(vec![on(
        "a",
        vec![
            call("Expr.sum", json!({})),
            call(
                "Expr.filter",
                json!({ "predicates": many(vec![field("f")]) }),
            ),
        ],
    )]);
    assert_eq!(
        last_after(item(&walk, 0)),
        after(Representative::Int64, Shape::Counted)
    );
    assert_eq!(item(&walk, 0).count(), Some(Count::NotKnown));
}

#[test]
fn a_count_may_stand_in_a_select_beside_nothing_that_is_the_rows() {
    let alone = select_of(vec![on("a", vec![head(2)])]);
    assert_eq!(alone.refusal(), None);
    let with_one = select_of(vec![
        on("a", vec![head(2)]),
        on("a", vec![call("Expr.sum", json!({}))]),
    ]);
    assert_eq!(with_one.refusal(), None, "one value is met by each of them");
    let as_many = select_of(vec![on("a", vec![head(2)]), on("k", vec![head(3)])]);
    assert_eq!(
        as_many.refusal(),
        None,
        "two counts that may be the same are not told apart"
    );
}

#[test]
fn a_select_is_refused_items_that_are_not_as_many_as_one_another() {
    let walk = select_of(vec![on("a", vec![head(2)]), field("k")]);
    let refusal = walk.refusal().expect("a refusal");
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Rule(Rule::ValuesMeetInCount)
    );
    assert!(refusal.at_end());
    assert_eq!(refusal.place().path().len(), 2, "{:?}", refusal.place());
}

#[test]
fn a_step_that_adds_columns_takes_no_count_but_one_value() {
    let counted = adding(on("a", vec![head(2)]));
    assert_eq!(
        refusal_source(&counted),
        RefusalSource::Rule(Rule::ValuesMeetInCount)
    );
    assert_eq!(
        adding(on("a", vec![call("Expr.sum", json!({}))])).refusal(),
        None
    );
    assert_eq!(
        adding(on("a", vec![call("Expr.abs", json!({}))])).refusal(),
        None
    );
}

#[test]
fn a_count_may_not_end_where_polars_takes_one_value_or_the_rows() {
    let counted = || on("f", vec![head(2)]);
    let cases = [
        (
            "a filter's predicate",
            walk_of(
                "LazyFrame.filter",
                json!({ "predicates": many(vec![counted()]) }),
            ),
        ),
        (
            "a sort key",
            walk_of(
                "LazyFrame.sort",
                json!({ "by": many(vec![on("a", vec![head(2)])]) }),
            ),
        ),
    ];
    for (place, walk) in cases {
        let refusal = walk
            .refusal()
            .unwrap_or_else(|| panic!("{place} takes a count"));
        assert_eq!(
            refusal.refusal().source(),
            RefusalSource::Polars(Stage::Rows),
            "{place}"
        );
        assert!(refusal.at_end(), "{place}");
        let said = refusal.refusal().reason();
        assert!(
            said.contains("a count of values that the data or the arguments decide")
                && said.contains("zip node received non-equal length inputs"),
            "{place}: {said}"
        );
    }
}

#[test]
fn a_count_beside_an_aggregate_stands_in_a_filter_and_a_sort() {
    let filter = walk_of(
        "LazyFrame.filter",
        json!({ "predicates": many(vec![on(
            "a",
            vec![head(2), call("Expr.sum", json!({})), call("Expr.gt", json!({ "other": whole(1) }))],
        )]) }),
    );
    assert_eq!(filter.refusal(), None);
}

#[test]
fn a_join_key_that_takes_some_of_the_values_is_not_elementwise() {
    let join = |key: Value| {
        walk_of(
            "LazyFrame.join",
            json!({
                "other": { "table": "other" },
                "left_on": many(vec![key]),
                "right_on": many(vec![field("rk")]),
            }),
        )
    };
    // The first of the values is not elementwise, and nor are the values of a literal.
    let counted = join(on("a", vec![head(2)]));
    assert_eq!(
        refusal_source(&counted),
        RefusalSource::Polars(Stage::Schema)
    );
    let literal = join(chain(
        call("polars.lit", json!({ "value": { "integer": 1 } })),
        vec![head(2), call("Expr.abs", json!({}))],
    ));
    assert_eq!(
        refusal_source(&literal),
        RefusalSource::Rule(Rule::KeyIsElementwise)
    );
    // The key that is wholly an aggregate is one value, whatever it takes the aggregate of.
    assert_eq!(
        join(on("a", vec![head(2), call("Expr.sum", json!({}))])).refusal(),
        None
    );
}

#[test]
fn a_window_takes_no_count_unless_it_joins_the_values_of_a_group_or_explodes_them() {
    let window =
        |arguments: Value| select_of(vec![on("a", vec![head(2), call("Expr.over", arguments)])]);
    let by_group = json!({ "partition_by": many(vec![field("k")]) });
    assert_eq!(
        refusal_source(&window(by_group)),
        RefusalSource::Rule(Rule::WindowInputIsNotCounted)
    );
    let joined = window(json!({
        "partition_by": many(vec![field("k")]),
        "mapping_strategy": { "choice": "join" },
    }));
    assert_eq!(joined.refusal(), None);
    assert_eq!(
        last_after(item(&joined, 0)),
        after(Representative::ListOfInteger, Shape::Many)
    );
    let exploded = window(json!({
        "partition_by": many(vec![field("k")]),
        "mapping_strategy": { "choice": "explode" },
    }));
    assert_eq!(exploded.refusal(), None);
    assert_eq!(item(&exploded, 0).count(), Some(Count::NotKnown));
}

#[test]
fn a_method_meets_values_as_many_as_one_another_or_one() {
    let sum_with = |other: Value| {
        select_of(vec![on(
            "a",
            vec![call("Expr.add", json!({ "other": other }))],
        )])
    };
    // The rows are no count, and two counts that cannot be the same are not.
    assert_eq!(
        refusal_source(&sum_with(on("a", vec![head(2)]))),
        RefusalSource::Rule(Rule::ValuesMeetInCount)
    );
    let counted_with_the_rows = select_of(vec![on(
        "a",
        vec![head(2), call("Expr.add", json!({ "other": field("k") }))],
    )]);
    assert_eq!(
        refusal_source(&counted_with_the_rows),
        RefusalSource::Rule(Rule::ValuesMeetInCount)
    );
    // One value meets the count, and so is the count.
    let one_value = sum_with(on("a", vec![call("Expr.sum", json!({}))]));
    assert_eq!(one_value.refusal(), None);
    let one_by_counted = select_of(vec![on(
        "a",
        vec![
            call("Expr.sum", json!({})),
            call("Expr.add", json!({ "other": on("a", vec![head(2)]) })),
        ],
    )]);
    assert_eq!(one_by_counted.refusal(), None);
    assert_eq!(
        last_after(item(&one_by_counted, 0)),
        after(Representative::Int64, Shape::Counted)
    );
    assert_eq!(item(&one_by_counted, 0).count(), Some(Count::AtMost(2)));
    // Counts that may be the same give the one both fix.
    let both = select_of(vec![on(
        "a",
        vec![
            head(3),
            call("Expr.add", json!({ "other": on("a", vec![head(2)]) })),
        ],
    )]);
    assert_eq!(both.refusal(), None);
    assert_eq!(item(&both, 0).count(), Some(Count::AtMost(2)));
}

#[test]
fn the_values_of_a_condition_are_as_many_as_one_another() {
    let condition = |predicate: Value, then: Value, otherwise: Value| {
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![predicate]) }),
            ),
            vec![
                call("When.then", json!({ "statement": then })),
                call("Then.otherwise", json!({ "statement": otherwise })),
            ],
        )
    };
    let counted_predicate = on("f", vec![head(2)]);
    // The values are one value each, so the condition gives as many values as the predicate.
    let walk = select_of(vec![condition(
        counted_predicate.clone(),
        whole(1),
        whole(0),
    )]);
    assert_eq!(walk.refusal(), None);
    assert_eq!(item(&walk, 0).count(), Some(Count::AtMost(2)));
    // The values are the rows, which the predicate's count is not.
    let walk = select_of(vec![condition(counted_predicate, field("a"), field("k"))]);
    assert_eq!(
        refusal_source(&walk),
        RefusalSource::Rule(Rule::ValuesMeetInCount)
    );
}
