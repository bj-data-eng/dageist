//! What the walk says of the methods that take some of the values they follow, and of the others
//! whose count of values the data or the arguments decide: how many values a chain gives, where
//! each place takes such a count, and what it is met with.
//!
//! The table is the walk tests' own, whose columns are `a` (`Int64`), `b` (`String`), `f`
//! (`Boolean`) and `k` (`Int64`). Every expected value is worked out by hand from what Python
//! Polars 2.1.0 does with three rows of them and from the explanation of the catalogue, not read
//! from a run.

use dageist_engine::allowed::{
    After, Count, Ending, RefusalSource, Representative, Rule, Shape, Stage,
};
use dageist_engine::document::{Document, Inputs, Segment, Walk, WalkedExpression};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, field_then, many, step, whole};

fn source() -> DataFrame {
    // A column of arrays of two whole numbers each, which a list casts to.
    let lists = Series::new(
        "arr".into(),
        [[1_i64, 2], [3, 4], [5, 6]].map(|pair| Series::new("".into(), pair)),
    );
    let arrays = lists
        .cast(&DataType::Array(Box::new(DataType::Int64), 2))
        .expect("lists of two cast to arrays of two");
    df!(
        "a" => [1_i64, 2, 3],
        "b" => ["x", "y", "z"],
        "f" => [true, false, true],
        "k" => [1_i64, 1, 2],
    )
    .expect("the table")
    .hstack(&[arrays.into()])
    .expect("a column of three rows")
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
        // The places of the values and the number of the distinct ones are whole numbers of 32 bits.
        let dtype = if matches!(name, "arg_unique" | "unique_counts") {
            Representative::UInt32
        } else {
            Representative::Int64
        };
        assert_eq!(
            last_after(expression),
            after(dtype, Shape::Counted),
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
fn what_a_method_takes_of_one_value_is_one_value_only_where_it_keeps_the_value() {
    // Python Polars 2.1.0, of the sum of `a`, one value: the first `n` of it, or from a place, or
    // every nth from a place, or the top `k`, leave the value where the count and the place leave
    // one, and none where they leave none, which a select refuses beside a column. What is
    // different or most common in one value is that value, and a difference that drops the first
    // value of one value drops it.
    let sum = || call("Expr.sum", json!({}));
    let literal = || call("polars.lit", json!({ "value": { "integer": 1 } }));
    let gather_every = |n: i64, offset: i64| {
        call(
            "Expr.gather_every",
            json!({ "n": { "integer": n }, "offset": { "integer": offset } }),
        )
    };
    let cases: Vec<(&str, Value, Representative, Shape, Option<Count>)> = vec![
        (
            "head(2)",
            on("a", vec![sum(), head(2)]),
            Representative::Int64,
            Shape::One,
            None,
        ),
        (
            "head()",
            on("a", vec![sum(), call("Expr.head", json!({}))]),
            Representative::Int64,
            Shape::One,
            None,
        ),
        (
            "head(0)",
            on("a", vec![sum(), head(0)]),
            Representative::Int64,
            Shape::Counted,
            Some(Count::Exactly(0)),
        ),
        (
            "tail(0)",
            on(
                "a",
                vec![sum(), call("Expr.tail", json!({ "n": whole(0) }))],
            ),
            Representative::Int64,
            Shape::Counted,
            Some(Count::Exactly(0)),
        ),
        (
            "tail(5)",
            on(
                "a",
                vec![sum(), call("Expr.tail", json!({ "n": whole(5) }))],
            ),
            Representative::Int64,
            Shape::One,
            None,
        ),
        (
            "slice(1)",
            on(
                "a",
                vec![sum(), call("Expr.slice", json!({ "offset": whole(1) }))],
            ),
            Representative::Int64,
            Shape::Counted,
            Some(Count::Exactly(0)),
        ),
        (
            "slice(0)",
            on(
                "a",
                vec![sum(), call("Expr.slice", json!({ "offset": whole(0) }))],
            ),
            Representative::Int64,
            Shape::One,
            None,
        ),
        (
            "top_k(0)",
            on(
                "a",
                vec![sum(), call("Expr.top_k", json!({ "k": whole(0) }))],
            ),
            Representative::Int64,
            Shape::Counted,
            Some(Count::Exactly(0)),
        ),
        (
            "top_k(1)",
            on(
                "a",
                vec![sum(), call("Expr.top_k", json!({ "k": whole(1) }))],
            ),
            Representative::Int64,
            Shape::One,
            None,
        ),
        (
            "gather_every(1, 1)",
            on("a", vec![sum(), gather_every(1, 1)]),
            Representative::Int64,
            Shape::Counted,
            Some(Count::Exactly(0)),
        ),
        (
            "gather_every(2)",
            on("a", vec![sum(), gather_every(2, 0)]),
            Representative::Int64,
            Shape::One,
            None,
        ),
        (
            "unique",
            on("a", vec![sum(), call("Expr.unique", json!({}))]),
            Representative::Int64,
            Shape::One,
            None,
        ),
        (
            "mode",
            on("a", vec![sum(), call("Expr.mode", json!({}))]),
            Representative::Int64,
            Shape::One,
            None,
        ),
        (
            "arg_unique",
            on("a", vec![sum(), call("Expr.arg_unique", json!({}))]),
            Representative::UInt32,
            Shape::One,
            None,
        ),
        (
            "diff that drops",
            on(
                "a",
                vec![
                    sum(),
                    call(
                        "Expr.diff",
                        json!({ "null_behavior": { "choice": "drop" } }),
                    ),
                ],
            ),
            Representative::Int64,
            Shape::Counted,
            Some(Count::Exactly(0)),
        ),
        (
            "the head(0) of a literal",
            chain(literal(), vec![head(0)]),
            Representative::Int32,
            Shape::Counted,
            Some(Count::Exactly(0)),
        ),
        (
            "the tail(3) of a literal",
            chain(literal(), vec![call("Expr.tail", json!({ "n": whole(3) }))]),
            Representative::Int32,
            Shape::One,
            None,
        ),
    ];
    for (name, expression, dtype, shape, count) in cases {
        let walk = select_of(vec![expression]);
        assert_eq!(last_after(item(&walk, 0)), after(dtype, shape), "{name}");
        assert_eq!(item(&walk, 0).count(), count, "{name}");
    }
}

#[test]
fn a_select_is_refused_one_value_that_is_none_beside_the_rows() {
    // The sum of `a`, the first none of it, beside a column: Polars refuses the lengths, and takes
    // the first one of it, which is one value.
    for (name, taken, refused) in [("head(0)", head(0), true), ("head(2)", head(2), false)] {
        let walk = select_of(vec![
            on("a", vec![call("Expr.sum", json!({})), taken]),
            field("k"),
        ]);
        assert_eq!(walk.refusal().is_some(), refused, "{name}");
    }
}

#[test]
fn a_window_takes_no_one_value_that_a_method_took_of_one_value_but_the_aggregate_of_it() {
    // Polars 2.1.0 refuses these windows with "expressions must have matching group lengths": the
    // value is one of each group, and no value is given to the rows, where `sum().over` and
    // `sum().first().over` are accepted, for an aggregate is one value that every row of its group
    // takes.
    let window = |calls: Vec<Value>| {
        let mut all = vec![call("Expr.sum", json!({}))];
        all.extend(calls);
        all.push(call(
            "Expr.over",
            json!({ "partition_by": many(vec![field("k")]) }),
        ));
        select_of(vec![on("a", all)])
    };
    let search = call(
        "Expr.search_sorted",
        json!({ "element": { "expression": { "start": call("polars.lit", json!({
            "value": { "integer": 2 }, "dtype": { "dtype": "Int64" } })), "then": [] } } }),
    );
    for (name, calls) in [
        ("head(2)", vec![head(2)]),
        ("unique", vec![call("Expr.unique", json!({}))]),
        ("tail(5)", vec![call("Expr.tail", json!({ "n": whole(5) }))]),
        ("mode", vec![call("Expr.mode", json!({}))]),
        (
            "top_k(1)",
            vec![call("Expr.top_k", json!({ "k": whole(1) }))],
        ),
        (
            "slice(0)",
            vec![call("Expr.slice", json!({ "offset": whole(0) }))],
        ),
        ("arg_unique", vec![call("Expr.arg_unique", json!({}))]),
        ("unique_counts", vec![call("Expr.unique_counts", json!({}))]),
        (
            "a method of what it took",
            vec![head(2), call("Expr.abs", json!({}))],
        ),
        ("search_sorted(2)", vec![search]),
    ] {
        assert_eq!(
            refusal_source(&window(calls)),
            RefusalSource::Rule(Rule::WindowInputIsNotCounted),
            "{name}"
        );
    }
    assert_eq!(window(Vec::new()).refusal(), None);
    assert_eq!(
        window(vec![head(2), call("Expr.sum", json!({}))]).refusal(),
        None,
        "an aggregate of what it took is one value of each group"
    );
    // A window that joins the values of a group takes it.
    let joined = select_of(vec![on(
        "a",
        vec![
            call("Expr.sum", json!({})),
            head(2),
            call(
                "Expr.over",
                json!({
                    "partition_by": many(vec![field("k")]),
                    "mapping_strategy": { "choice": "join" },
                }),
            ),
        ],
    )]);
    assert_eq!(joined.refusal(), None);
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
    let flag = |on: bool| {
        chain(
            call("polars.lit", json!({ "value": { "flag": on } })),
            Vec::new(),
        )
    };
    // The values are one value each, of a dtype, so the condition gives as many values as the
    // predicate.
    let walk = select_of(vec![condition(
        counted_predicate.clone(),
        flag(true),
        flag(false),
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

#[test]
fn what_a_method_takes_of_one_value_ends_where_one_value_does() {
    // The first of one value is that value, and the places of the values that differ in one value
    // are the place of that one: a sort key takes one value, which a count it is not.
    let literal = || call("polars.lit", json!({ "value": { "integer": 1 } }));
    for method in [call("Expr.arg_unique", json!({})), head(2)] {
        let walk = walk_of(
            "LazyFrame.sort",
            json!({ "by": many(vec![chain(literal(), vec![method])]) }),
        );
        assert_eq!(walk.refusal(), None);
    }
}

#[test]
fn a_replacement_is_given_what_it_replaces_by_as_many_as_it_replaces_or_one() {
    let replacing = |new: Value| {
        select_of(vec![on(
            "a",
            vec![call(
                "Expr.replace",
                json!({ "old": field("a"), "new": new }),
            )],
        )])
    };
    assert_eq!(
        refusal_source(&replacing(on("a", vec![head(2)]))),
        RefusalSource::Rule(Rule::ValuesMeetInCount)
    );
    assert_eq!(replacing(whole(1)).refusal(), None);
}

#[test]
fn a_search_gives_a_place_for_each_value_it_looks_for() {
    // The sorted values of `a` are searched for one value, for the rows, and for a count.
    let typed = chain(
        call(
            "polars.lit",
            json!({ "value": { "integer": 2 }, "dtype": { "dtype": "Int64" } }),
        ),
        Vec::new(),
    );
    let search = |element: Value| {
        select_of(vec![on(
            "a",
            vec![
                call("Expr.sort", json!({})),
                call("Expr.search_sorted", json!({ "element": element })),
            ],
        )])
    };
    for (name, element, shape, count) in [
        ("one value", typed, Shape::One, None),
        ("the rows", field("k"), Shape::Many, None),
        (
            "a count",
            on("k", vec![head(2)]),
            Shape::Counted,
            Some(Count::AtMost(2)),
        ),
    ] {
        let walk = search(element);
        assert_eq!(walk.refusal(), None, "{name}");
        assert_eq!(
            last_after(item(&walk, 0)),
            after(Representative::UInt32, shape),
            "{name}"
        );
        assert_eq!(item(&walk, 0).count(), count, "{name}");
    }
}

#[test]
fn is_in_gives_the_values_it_follows_unless_it_is_given_a_list_of_each() {
    let is_in = |receiver: Vec<Value>, other: Value| {
        select_of(vec![on(
            "a",
            [
                receiver,
                vec![call("Expr.is_in", json!({ "other": other }))],
            ]
            .concat(),
        )])
    };
    let sum = || vec![call("Expr.sum", json!({}))];
    // A column of the values' own dtype is their set, whole: one value is asked of it, and each of
    // the rows is.
    for (name, receiver, shape) in [
        ("one value", sum(), Shape::One),
        ("the rows", Vec::new(), Shape::Many),
    ] {
        assert_eq!(
            last_after(item(&is_in(receiver, field("k")), 0)),
            after(Representative::Boolean, shape),
            "{name} given a column"
        );
    }
    // A list of each group's values is met row by row, so one value is asked of each.
    let lists = on(
        "a",
        vec![call(
            "Expr.over",
            json!({
                "partition_by": many(vec![field("k")]),
                "mapping_strategy": { "choice": "join" },
            }),
        )],
    );
    assert_eq!(
        last_after(item(&is_in(sum(), lists), 0)),
        after(Representative::Boolean, Shape::Many)
    );
}

#[test]
fn is_in_meets_an_array_column_row_by_row_as_it_does_a_list() {
    // Python Polars 2.1.0: the sum of `a` asked of a column of arrays of two is one answer for each
    // row, three of them, and so many as the rows are, where a count is not.
    let is_in = |receiver: Vec<Value>| {
        on(
            "a",
            [
                receiver,
                vec![call("Expr.is_in", json!({ "other": field("arr") }))],
            ]
            .concat(),
        )
    };
    let sum = || vec![call("Expr.sum", json!({}))];
    let one_asked = select_of(vec![is_in(sum())]);
    assert_eq!(
        last_after(item(&one_asked, 0)),
        after(Representative::Boolean, Shape::Many)
    );
    assert_eq!(select_of(vec![is_in(sum()), field("k")]).refusal(), None);
    let beside_a_count = select_of(vec![is_in(sum()), on("k", vec![head(2)])]);
    assert_eq!(
        refusal_source(&beside_a_count),
        RefusalSource::Rule(Rule::ValuesMeetInCount)
    );
}

#[test]
fn a_window_takes_no_value_that_a_method_took_of_one_value_by_whatever_it_meets_it_in() {
    // Python Polars 2.1.0 refuses each of these windows with "expressions must have matching group
    // lengths": the one value that `head(1)` took of the sum of `k` is met by a method, a condition
    // or a fill, and what they give is no more the value of each row of its group than it was.
    let taken = || on("k", vec![call("Expr.sum", json!({})), head(1)]);
    let over = call(
        "Expr.over",
        json!({ "partition_by": many(vec![field("k")]) }),
    );
    let after_the_sum = |link: Value| {
        select_of(vec![on(
            "a",
            vec![call("Expr.sum", json!({})), link, over.clone()],
        )])
    };
    let condition = |predicate: Value, then: Value, otherwise: Value| {
        select_of(vec![chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![predicate]) }),
            ),
            vec![
                call("When.then", json!({ "statement": then })),
                call("Then.otherwise", json!({ "statement": otherwise })),
                over.clone(),
            ],
        )])
    };
    let a_sum = || on("a", vec![call("Expr.sum", json!({}))]);
    let walks = vec![
        (
            "sum + take",
            after_the_sum(call("Expr.add", json!({ "other": taken() }))),
        ),
        (
            "sum * take",
            after_the_sum(call("Expr.mul", json!({ "other": taken() }))),
        ),
        (
            "sum ** take",
            after_the_sum(call("Expr.pow", json!({ "exponent": taken() }))),
        ),
        (
            "fill_null take",
            after_the_sum(call("Expr.fill_null", json!({ "value": taken() }))),
        ),
        (
            "clip take",
            after_the_sum(call(
                "Expr.clip",
                json!({ "lower_bound": taken(), "upper_bound": taken() }),
            )),
        ),
        (
            "a literal + take",
            select_of(vec![chain(
                call("polars.lit", json!({ "value": { "integer": 1 } })),
                vec![call("Expr.add", json!({ "other": taken() })), over.clone()],
            )]),
        ),
        (
            "a value of a condition",
            condition(on("f", Vec::new()), taken(), a_sum()),
        ),
        (
            "a predicate of a condition",
            condition(
                on(
                    "b",
                    vec![
                        call("Expr.sum", json!({})),
                        head(1),
                        call("Expr.gt", json!({ "other": whole(0) })),
                    ],
                ),
                a_sum(),
                a_sum(),
            ),
        ),
    ];
    for (name, walk) in walks {
        assert_eq!(
            refusal_source(&walk),
            RefusalSource::Rule(Rule::WindowInputIsNotCounted),
            "{name}"
        );
    }
    // What meets only aggregates is the value of each row of its group.
    for (name, other) in [
        ("sum + sum", a_sum()),
        (
            "sum + the sum of what head took",
            on("k", vec![head(2), call("Expr.sum", json!({}))]),
        ),
        (
            "sum + the first of what head took",
            on("k", vec![head(2), call("Expr.first", json!({}))]),
        ),
    ] {
        let walk = after_the_sum(call("Expr.add", json!({ "other": other })));
        assert_eq!(walk.refusal(), None, "{name}");
    }
}

#[test]
fn a_method_that_is_not_told_how_many_takes_what_python_takes_of_the_rows() {
    // Python's `head`, `tail` and `limit` take 10 values, and `top_k` and `bottom_k` take 5.
    for (entry, count) in [
        ("Expr.head", 10),
        ("Expr.tail", 10),
        ("Expr.limit", 10),
        ("Expr.top_k", 5),
        ("Expr.bottom_k", 5),
    ] {
        let walk = select_of(vec![on("a", vec![call(entry, json!({}))])]);
        assert_eq!(
            item(&walk, 0).count(),
            Some(Count::AtMost(count)),
            "{entry}"
        );
    }
}
