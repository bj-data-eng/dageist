//! What the tables of allowed combinations say of a step: the context each
//! expression stands in, what each link follows, and what is refused.
//!
//! Every expected value is worked out by hand from the table below, whose
//! columns are `a` (`Int64`), `b` (`String`), `f` (`Boolean`) and `k`
//! (`Int64`), and from what the explanation says of the contexts and the
//! rules, not read from a run.

use dageist_engine::allowed::{
    After, ConditionState, Context, Ending, Nested, Outer, Place as OfferPlace, RefusalSource,
    Representative, Rule, Shape, Unknown,
};
use dageist_engine::document::{
    Document, Inputs, ListOrigin, Resolution, Segment, Walk, WalkedExpression,
};
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

/// A table another table joins: `ra` is text, `rk` whole numbers.
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
        .add_value("cutoff", Scalar::new(DataType::Int64, AnyValue::Int64(2)))
        .expect("a value");
    inputs
        .add_value(
            "on",
            Scalar::new(DataType::Boolean, AnyValue::Boolean(true)),
        )
        .expect("a flag");
    inputs
        .add_list(
            "holidays",
            ListOrigin::Parameter,
            Series::new("holidays".into(), [1_i64, 2]),
        )
        .expect("a list");
    inputs
}

fn parameter(name: &str) -> Segment {
    Segment::Parameter(name.to_owned())
}

fn document_of(entry: &str, arguments: Value) -> Document {
    Document::read(&document(vec![step("s", entry, arguments)])).expect("a document")
}

fn walk_of(entry: &str, arguments: Value) -> Walk {
    let document = document_of(entry, arguments);
    document.steps()[0].walk(0, source().schema(), &inputs())
}

/// The expression of `walk` at the way `path` from the step.
fn expression_at<'a>(walk: &'a Walk, path: &[Segment]) -> &'a WalkedExpression {
    walk.expressions()
        .iter()
        .find(|expression| expression.place().path() == path)
        .unwrap_or_else(|| panic!("no expression at {path:?}"))
}

fn after(dtype: Representative, shape: Shape) -> After {
    After::Value { dtype, shape }
}

fn at(context: Context, after: After) -> Option<OfferPlace> {
    Some(OfferPlace::new(context, after))
}

fn item(name: &str, at: usize) -> Vec<Segment> {
    vec![parameter(name), Segment::Item(at)]
}

#[test]
fn each_link_of_a_select_item_follows_the_value_before_it() {
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![field_then(
            "a",
            vec![
                call("Expr.cum_sum", json!({})),
                call("Expr.is_null", json!({})),
            ],
        )]) }),
    );
    let item = expression_at(&walk, &item("exprs", 0));
    assert_eq!(item.context(), Some(Context::step(Outer::Select)));
    let select = Context::step(Outer::Select);
    // A running sum of whole numbers is whole numbers, one for each row, and a
    // test for a null is a Boolean, one for each row.
    assert_eq!(
        item.places(),
        vec![
            at(select, after(Representative::Int64, Shape::Many)),
            at(select, after(Representative::Int64, Shape::Many)),
            at(select, after(Representative::Boolean, Shape::Many)),
        ]
    );
    assert_eq!(item.calls()[0].follows(), Some(After::Start));
    assert_eq!(
        item.calls()[1].follows(),
        Some(after(Representative::Int64, Shape::Many))
    );
    assert_eq!(item.ending(), Some(&Ending::Allowed));
    assert_eq!(walk.refusal(), None);
    assert_eq!(walk.not_known(), None);
}

#[test]
fn an_aggregate_gives_one_value_and_what_follows_it_follows_one_value() {
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![field_then(
            "a",
            vec![call("Expr.sum", json!({})), call("Expr.is_null", json!({}))],
        )]) }),
    );
    let item = expression_at(&walk, &item("exprs", 0));
    assert_eq!(
        item.places()[1],
        at(
            Context::step(Outer::Select),
            after(Representative::Int64, Shape::One)
        )
    );
}

#[test]
fn a_window_key_stands_in_its_own_context_inside_the_item_that_holds_it() {
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![field_then(
            "a",
            vec![call(
                "Expr.over",
                json!({ "partition_by": many(vec![field("k")]) }),
            )],
        )]) }),
    );
    let key = expression_at(
        &walk,
        &[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(0),
            parameter("partition_by"),
            Segment::Item(0),
        ],
    );
    let partition = Context::inside(Outer::Select, Nested::PartitionBy);
    assert_eq!(key.context(), Some(partition));
    assert_eq!(
        key.places(),
        vec![at(partition, after(Representative::Int64, Shape::Many))]
    );
    // Walked before the item that holds it.
    assert_eq!(walk.expressions().len(), 2);
    assert_eq!(walk.expressions()[0].place(), key.place());
}

#[test]
fn a_window_that_joins_gives_a_list() {
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![field_then(
            "a",
            vec![call(
                "Expr.over",
                json!({
                    "partition_by": many(vec![field("k")]),
                    "mapping_strategy": { "choice": "join" },
                }),
            )],
        )]) }),
    );
    let item = expression_at(&walk, &item("exprs", 0));
    assert_eq!(
        item.places()[1],
        at(
            Context::step(Outer::Select),
            after(Representative::ListOfInteger, Shape::Many)
        )
    );
}

/// The place the walk finds after the window given `arguments`, which follows
/// `a` in a `select`.
fn after_window(arguments: Value) -> Option<OfferPlace> {
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![field_then("a", vec![call("Expr.over", arguments)])]) }),
    );
    let item = expression_at(&walk, &item("exprs", 0));
    item.places()[1]
}

#[test]
fn a_window_ordered_without_a_partition_gives_the_walk_its_dtype() {
    // Polars gives `a` over an order of `k` and no partition the dtype it gives
    // with one: `Int64`, one for each row, and for a window that joins a list
    // of them. The step editor offers the methods that follow it.
    let select = Context::step(Outer::Select);
    let ordered = || many(vec![field("k")]);
    let cases = [
        (json!({ "order_by": ordered() }), Representative::Int64),
        (
            json!({ "order_by": ordered(), "nulls_last": { "flag": true } }),
            Representative::Int64,
        ),
        (
            json!({ "order_by": ordered(), "descending": { "flag": true } }),
            Representative::Int64,
        ),
        (
            json!({
                "order_by": ordered(),
                "descending": { "flag": true },
                "nulls_last": { "flag": true },
            }),
            Representative::Int64,
        ),
        (
            json!({ "order_by": ordered(), "mapping_strategy": { "choice": "explode" } }),
            Representative::Int64,
        ),
        (
            json!({ "order_by": ordered(), "mapping_strategy": { "choice": "join" } }),
            Representative::ListOfInteger,
        ),
        (
            json!({
                "order_by": ordered(),
                "nulls_last": { "flag": true },
                "mapping_strategy": { "choice": "join" },
            }),
            Representative::ListOfInteger,
        ),
    ];
    for (arguments, dtype) in cases {
        assert_eq!(
            after_window(arguments.clone()),
            at(select, after(dtype, Shape::Many)),
            "{arguments}"
        );
    }
}

#[test]
fn a_window_given_neither_partition_by_nor_order_by_gives_the_walk_no_dtype() {
    // Polars refuses a window given neither key, bare or with only flags, a
    // strategy, or `more_exprs`, which extends a `partition_by` it does not have,
    // so there is no dtype to know after it.
    for arguments in [
        json!({}),
        json!({ "nulls_last": { "flag": true } }),
        json!({ "descending": { "flag": true } }),
        json!({ "mapping_strategy": { "choice": "join" } }),
        json!({ "more_exprs": many(vec![field("k")]) }),
    ] {
        assert_eq!(after_window(arguments.clone()), None, "{arguments}");
    }
}

#[test]
fn a_condition_follows_where_it_has_come_to_and_gives_the_dtype_of_its_values() {
    let condition = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("f")]) }),
        ),
        vec![
            call("When.then", json!({ "statement": field("a") })),
            call("Then.otherwise", json!({ "statement": field("k") })),
        ],
    );
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![condition]) }),
    );
    let item = expression_at(&walk, &item("exprs", 0));
    let select = Context::step(Outer::Select);
    assert_eq!(
        item.places(),
        vec![
            at(select, After::Condition(ConditionState::When)),
            at(select, After::Condition(ConditionState::Then)),
            // Whole numbers either way, and a value for each row.
            at(select, after(Representative::Int64, Shape::Many)),
        ]
    );
}

#[test]
fn a_column_of_the_other_table_is_read_against_that_table_in_a_right_key() {
    let walk = walk_of(
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![field("b")]),
            "right_on": many(vec![field("ra")]),
        }),
    );
    let left = expression_at(&walk, &item("left_on", 0));
    let right = expression_at(&walk, &item("right_on", 0));
    assert_eq!(left.context(), Some(Context::step(Outer::JoinLeft)));
    assert_eq!(right.context(), Some(Context::step(Outer::JoinRight)));
    // Both are text, which the left table's `b` and the other's `ra` are.
    assert_eq!(
        left.places(),
        vec![at(
            Context::step(Outer::JoinLeft),
            after(Representative::String, Shape::Many)
        )]
    );
    assert_eq!(
        right.places(),
        vec![at(
            Context::step(Outer::JoinRight),
            after(Representative::String, Shape::Many)
        )]
    );
    assert_eq!(walk.refusal(), None);
}

#[test]
fn a_column_that_the_table_does_not_have_leaves_what_follows_it_not_known() {
    // `a` is the left table's, not the other's.
    let walk = walk_of(
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![field("b")]),
            "right_on": many(vec![field("a")]),
        }),
    );
    let right = expression_at(&walk, &item("right_on", 0));
    assert_eq!(right.places(), vec![None]);
}

#[test]
fn a_column_that_is_not_boolean_is_refused_as_a_filters_predicate_by_a_rule() {
    let walk = walk_of(
        "LazyFrame.filter",
        json!({ "predicates": many(vec![field("a")]) }),
    );
    let refusal = walk.refusal().expect("a whole number is no predicate");
    assert_eq!(refusal.place().path(), item("predicates", 0).as_slice());
    assert!(refusal.at_end());
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Rule(Rule::FilterPredicateIsBoolean)
    );
    let predicate = expression_at(&walk, &item("predicates", 0));
    assert!(matches!(predicate.ending(), Some(Ending::Refused(_))));

    let boolean = walk_of(
        "LazyFrame.filter",
        json!({ "predicates": many(vec![field("f")]) }),
    );
    assert_eq!(boolean.refusal(), None);
    assert_eq!(
        expression_at(&boolean, &item("predicates", 0)).ending(),
        Some(&Ending::Allowed)
    );
}

#[test]
fn a_start_that_names_several_columns_has_no_dtype_and_is_not_known_to_end() {
    // `col("f", "a")` expands to one expression for each column, whatever the
    // first is: Polars refuses it as a predicate though `f` is a Boolean.
    let several = |first: &str, more: Vec<&str>| {
        chain(
            call(
                "polars.col",
                json!({
                    "name": { "columns": [first] },
                    "more_names": { "many": more.iter().map(|name| json!({ "columns": [name] })).collect::<Vec<_>>() },
                }),
            ),
            vec![],
        )
    };
    let walk = walk_of(
        "LazyFrame.filter",
        json!({ "predicates": many(vec![several("f", vec!["a"])]) }),
    );
    assert_eq!(walk.refusal(), None);
    let predicate = expression_at(&walk, &item("predicates", 0));
    assert_eq!(
        predicate.ending(),
        Some(&Ending::NotKnown(Unknown::TheStartsDtype))
    );
    assert_eq!(predicate.places(), vec![None]);
    // Names given as a list of the first argument are the same.
    let listed = chain(
        call("polars.col", json!({ "name": { "columns": ["f", "a"] } })),
        vec![],
    );
    let walk = walk_of(
        "LazyFrame.filter",
        json!({ "predicates": many(vec![listed]) }),
    );
    assert_eq!(
        expression_at(&walk, &item("predicates", 0)).ending(),
        Some(&Ending::NotKnown(Unknown::TheStartsDtype))
    );
    // Where no Boolean is needed it is still not known: the dtypes may differ.
    let walk = walk_of(
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![several("k", vec!["a"])]),
            "right_on": many(vec![field("rk")]),
        }),
    );
    let key = expression_at(&walk, &item("left_on", 0));
    assert_eq!(
        key.ending(),
        Some(&Ending::NotKnown(Unknown::TheStartsDtype))
    );
    assert_eq!(key.places(), vec![None]);
    // One name is as before.
    let one = walk_of(
        "LazyFrame.filter",
        json!({ "predicates": many(vec![several("f", vec![])]) }),
    );
    assert_eq!(
        expression_at(&one, &item("predicates", 0)).ending(),
        Some(&Ending::Allowed)
    );
}

#[test]
fn element_has_no_dtype_where_a_pivot_takes_values_from_several_columns() {
    // The group holds `a` and `b`, a whole number and text: `element().sum()` is
    // refused by Polars for the text, whatever the first column is.
    let pivot = |values: Vec<&str>| {
        walk_of(
            "LazyFrame.pivot",
            json!({
                "on": { "columns": ["f"] },
                "on_columns": { "table": "other" },
                "values": { "columns": values },
                "aggregate_function": chain(
                    call("polars.element", json!({})),
                    vec![call("Expr.sum", json!({}))],
                ),
            }),
        )
    };
    let aggregate = |walk: &Walk| expression_at(walk, &[parameter("aggregate_function")]).places();
    let several = pivot(vec!["a", "b"]);
    assert_eq!(aggregate(&several), vec![None, None]);
    // One column is as before: a group of whole numbers, summed to one.
    let one = pivot(vec!["a"]);
    let places = aggregate(&one);
    assert!(places.iter().all(Option::is_some), "{places:?}");
}

#[test]
fn an_untyped_whole_number_is_the_dtype_polars_gives_it_and_is_no_predicate() {
    // `lit(1)` is an `Int32` by itself, which Polars refuses as a filter predicate when it runs.
    let table = source().lazy();
    assert!(table.filter(lit(1)).collect().is_err());
    let walk = walk_of(
        "LazyFrame.filter",
        json!({ "predicates": many(vec![chain(
            call("polars.lit", json!({ "value": { "integer": 1 } })),
            vec![],
        )]) }),
    );
    let refusal = walk.refusal().expect("a whole number is no predicate");
    assert!(refusal.at_end());
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Rule(Rule::FilterPredicateIsBoolean)
    );
}

#[test]
fn an_untyped_number_is_asked_of_the_tables_as_the_dtype_polars_gives_it() {
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![
            chain(
                call("polars.lit", json!({ "value": { "integer": 1 } })),
                vec![call("Expr.is_null", json!({}))],
            ),
            chain(
                call("polars.lit", json!({ "value": { "float": 1.5 } })),
                vec![call("Expr.is_null", json!({}))],
            ),
        ]) }),
    );
    assert_eq!(walk.not_known(), None);
    let select = Context::step(Outer::Select);
    assert_eq!(
        expression_at(&walk, &item("exprs", 0)).places(),
        vec![
            at(select, after(Representative::Int32, Shape::One)),
            at(select, after(Representative::Boolean, Shape::One)),
        ]
    );
    assert_eq!(
        expression_at(&walk, &item("exprs", 1)).places(),
        vec![
            at(select, after(Representative::Float64, Shape::One)),
            at(select, after(Representative::Boolean, Shape::One)),
        ]
    );
}

#[test]
fn an_untyped_number_that_ends_a_condition_takes_the_dtype_of_the_values_beside_it() {
    // `when(f).then(a).otherwise(0)`: the number adapts to the whole numbers `a` holds, so the
    // condition gives `Int64`, as Polars gives it.
    let condition = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("f")]) }),
        ),
        vec![
            call("When.then", json!({ "statement": field("a") })),
            call("Then.otherwise", json!({ "statement": whole(0) })),
            call("Expr.is_null", json!({})),
        ],
    );
    let adapted = source()
        .lazy()
        .select([when(col("f")).then(col("a")).otherwise(lit(0))])
        .collect_schema()
        .expect("Polars resolves it");
    assert_eq!(adapted.get("a"), Some(&DataType::Int64));
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![condition]) }),
    );
    let select = Context::step(Outer::Select);
    assert_eq!(
        expression_at(&walk, &item("exprs", 0)).places()[2..],
        [
            at(select, after(Representative::Int64, Shape::Many)),
            at(select, after(Representative::Boolean, Shape::Many)),
        ]
    );
}

#[test]
fn a_window_key_that_holds_a_single_value_beside_a_column_is_not_refused() {
    // Polars accepts the key of a window that is not elementwise, where the
    // window stands in a select, a with_columns or a filter.
    let mixed = || {
        field_then(
            "k",
            vec![call(
                "Expr.sub",
                json!({ "other": field_then("a", vec![call("Expr.sum", json!({}))]) }),
            )],
        )
    };
    let window = |partition: Value| {
        field_then(
            "a",
            vec![call(
                "Expr.over",
                json!({ "partition_by": many(vec![partition]) }),
            )],
        )
    };
    for (entry, parameter) in [
        ("LazyFrame.select", "exprs"),
        ("LazyFrame.with_columns", "exprs"),
    ] {
        let walk = walk_of(entry, json!({ parameter: many(vec![window(mixed())]) }));
        assert_eq!(walk.refusal(), None, "{entry}");
    }
    let boolean = field_then(
        "f",
        vec![call(
            "Expr.over",
            json!({ "partition_by": many(vec![mixed()]) }),
        )],
    );
    let walk = walk_of(
        "LazyFrame.filter",
        json!({ "predicates": many(vec![boolean]) }),
    );
    assert_eq!(walk.refusal(), None);
}

#[test]
fn a_join_key_that_holds_a_null_beside_a_column_is_not_refused() {
    // Polars allows it: only an aggregate beside a column is not elementwise.
    let key = field_then(
        "k",
        vec![call(
            "Expr.sub",
            json!({ "other": chain(
                call("polars.lit", json!({ "value": { "flag": null } })),
                vec![],
            ) }),
        )],
    );
    let walk = walk_of(
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![key]),
            "right_on": many(vec![field("rk")]),
        }),
    );
    assert_eq!(walk.refusal(), None);
}

#[test]
fn a_join_key_whose_aggregate_is_compared_with_a_null_is_not_refused() {
    // The comparison is a null, and the aggregate it follows goes with it.
    let key = field_then(
        "k",
        vec![call(
            "Expr.sub",
            json!({ "other": chain(
                call("polars.lit", json!({ "value": { "integer": 1 } })),
                vec![
                    call("Expr.last", json!({})),
                    call(
                        "Expr.eq",
                        json!({ "other": chain(
                            call("polars.lit", json!({ "value": { "flag": null } })),
                            vec![],
                        ) }),
                    ),
                ],
            ) }),
        )],
    );
    let walk = walk_of(
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![key]),
            "right_on": many(vec![field("rk")]),
        }),
    );
    assert_eq!(walk.refusal(), None);
}

/// The literal null, which a comparison with it folds to a null.
fn null() -> Value {
    chain(
        call("polars.lit", json!({ "value": { "flag": null } })),
        vec![],
    )
}

fn compared(id: &str, other: Value) -> Value {
    call(id, json!({ "other": other }))
}

/// What the walk refuses of a join whose left key is `key`.
fn joined_on(key: Value) -> Option<RefusalSource> {
    let walk = walk_of(
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![key]),
            "right_on": many(vec![field("rk")]),
        }),
    );
    walk.refusal().map(|refusal| refusal.refusal().source())
}

#[test]
fn a_join_key_with_a_comparison_of_a_column_with_a_null_at_any_depth_is_refused() {
    // Polars folds the comparison into a null, and what has no column is not
    // elementwise: one level down, in an operand, before a method, after a
    // null, and in the predicate of a condition.
    let rule = Some(RefusalSource::Rule(Rule::KeyIsElementwise));
    let keys = [
        field_then("k", vec![compared("Expr.gt", null())]),
        field_then(
            "k",
            vec![compared(
                "Expr.le",
                field_then("a", vec![compared("Expr.gt", null())]),
            )],
        ),
        field_then(
            "a",
            vec![
                compared("Expr.gt", null()),
                compared("Expr.sub", field("k")),
            ],
        ),
        chain(
            call("polars.lit", json!({ "value": { "string": "1d" } })),
            vec![compared(
                "Expr.le",
                chain(
                    call("polars.lit", json!({ "value": { "flag": null } })),
                    vec![compared("Expr.eq", field("f"))],
                ),
            )],
        ),
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![chain(
                    call("polars.lit", json!({ "value": { "flag": null } })),
                    vec![compared("Expr.lt", field("f"))],
                )]) }),
            ),
            vec![call("When.then", json!({ "statement": field("a") }))],
        ),
    ];
    for (at, key) in keys.into_iter().enumerate() {
        assert_eq!(joined_on(key), rule, "key {at}");
    }
}

#[test]
fn a_comparison_of_a_null_after_a_call_that_is_not_a_step_of_the_plan_is_still_with_a_null() {
    // `and_` and `or_` given no operand are the expression they follow, so Polars meets
    // `null > f` as if they were not there and folds it into a null.
    let rule = Some(RefusalSource::Rule(Rule::KeyIsElementwise));
    let null_start = || call("polars.lit", json!({ "value": { "flag": null } }));
    for identity in ["Expr.and_", "Expr.or_"] {
        let key = chain(
            null_start(),
            vec![call(identity, json!({})), compared("Expr.gt", field("f"))],
        );
        assert_eq!(joined_on(key), rule, "{identity}");
    }
    // The null a column is compared with is the same null followed by such a call.
    let key = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field_then(
                "a",
                vec![compared(
                    "Expr.gt",
                    chain(
                        call("polars.lit", json!({ "value": { "flag": null } })),
                        vec![call("Expr.or_", json!({}))],
                    ),
                )],
            )]) }),
        ),
        vec![call("When.then", json!({ "statement": field("k") }))],
    );
    assert_eq!(joined_on(key), rule);
    // Given an operand they are a call of their own, which the rule does not see through.
    let key = chain(
        null_start(),
        vec![
            call("Expr.and_", json!({ "others": many(vec![field("f")]) })),
            compared("Expr.gt", field("f")),
        ],
    );
    assert_eq!(joined_on(key), None);
}

#[test]
fn a_null_that_two_nots_undo_is_still_the_null_a_comparison_is_given() {
    // Polars simplifies a not of a not away, so a null that an even number of nots follows is
    // the null again, across calls that are the expression they follow, and a comparison with
    // it folds into a null. An odd number leaves a call that Polars does not fold.
    let rule = Some(RefusalSource::Rule(Rule::KeyIsElementwise));
    let null_start = || {
        call(
            "polars.lit",
            json!({ "value": { "flag": null }, "dtype": { "dtype": "UInt32" } }),
        )
    };
    let not = || call("Expr.not_", json!({}));
    let empty_and = || call("Expr.and_", json!({}));
    for links in [
        vec![not(), not(), compared("Expr.eq", field("f"))],
        vec![not(), empty_and(), not(), compared("Expr.eq", field("f"))],
        vec![not(), not(), not(), not(), compared("Expr.eq", field("f"))],
    ] {
        assert_eq!(
            joined_on(chain(null_start(), links.clone())),
            rule,
            "{links:?}"
        );
    }
    for links in [
        vec![not(), compared("Expr.eq", field("f"))],
        vec![not(), not(), not(), compared("Expr.eq", field("f"))],
        vec![
            not(),
            call("Expr.abs", json!({})),
            not(),
            compared("Expr.eq", field("f")),
        ],
    ] {
        assert_eq!(
            joined_on(chain(null_start(), links.clone())),
            None,
            "{links:?}"
        );
    }
    // The null a column is compared with is the same.
    let key = field_then(
        "k",
        vec![compared("Expr.gt", chain(null_start(), vec![not(), not()]))],
    );
    assert_eq!(joined_on(key), rule);
}

#[test]
fn a_comparison_with_a_null_drops_what_it_follows_where_that_is_one_value() {
    // `(k - (k != null)).len() <= null` is a null that Polars folds before it meets the
    // comparison of the column with a null inside the subtraction, so the key is allowed; a
    // comparison with a null after what is not one value drops nothing, and its column is met.
    let rule = Some(RefusalSource::Rule(Rule::KeyIsElementwise));
    let in_a_condition = |predicate: Value| {
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![predicate]) }),
            ),
            vec![call("When.then", json!({ "statement": field("a") }))],
        )
    };
    let inside = |method: &str| {
        field_then(
            "k",
            vec![
                call(
                    method,
                    json!({ "other": field_then("k", vec![compared("Expr.ne", null())]) }),
                ),
                call("Expr.len", json!({})),
                compared("Expr.le", null()),
            ],
        )
    };
    for method in ["Expr.sub", "Expr.add", "Expr.mul"] {
        assert_eq!(joined_on(in_a_condition(inside(method))), None, "{method}");
    }
    let evaluated = field_then(
        "k",
        vec![
            call(
                "Expr.sub",
                json!({ "other": field_then("k", vec![compared("Expr.ne", null())]) }),
            ),
            compared("Expr.le", null()),
        ],
    );
    assert_eq!(joined_on(in_a_condition(evaluated)), rule);
}

#[test]
fn a_join_key_that_compares_nothing_of_a_column_with_a_null_is_not_refused() {
    let keys = [
        // A null, a comparison of two literals, a column with a whole number.
        null(),
        chain(
            call("polars.lit", json!({ "value": { "integer": 1 } })),
            vec![compared(
                "Expr.gt",
                chain(
                    call("polars.lit", json!({ "value": { "integer": 2 } })),
                    vec![],
                ),
            )],
        ),
        field_then(
            "a",
            vec![compared(
                "Expr.gt",
                chain(
                    call("polars.lit", json!({ "value": { "integer": 1 } })),
                    vec![],
                ),
            )],
        ),
        // A null compared with a null, then taken from a column.
        chain(
            call("polars.lit", json!({ "value": { "flag": null } })),
            vec![
                compared("Expr.eq", null()),
                compared("Expr.sub", field("k")),
            ],
        ),
        // A test for null of a null.
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![chain(
                    call("polars.lit", json!({ "value": { "flag": null } })),
                    vec![call("Expr.is_not_null", json!({}))],
                )]) }),
            ),
            vec![call("When.then", json!({ "statement": field("b") }))],
        ),
    ];
    for (at, key) in keys.into_iter().enumerate() {
        assert_eq!(joined_on(key), None, "key {at}");
    }
}

#[test]
fn a_join_key_that_is_wholly_one_value_is_not_refused_for_a_null_it_compares() {
    // An aggregate is the whole key, whatever is compared inside it.
    let keys = [
        chain(
            call("polars.lit", json!({ "value": { "flag": null } })),
            vec![compared(
                "Expr.le",
                field_then("b", vec![call("Expr.max", json!({}))]),
            )],
        ),
        field_then(
            "k",
            vec![
                compared(
                    "Expr.gt",
                    field_then("a", vec![compared("Expr.eq", null())]),
                ),
                call("Expr.first", json!({})),
                compared("Expr.gt", null()),
            ],
        ),
    ];
    for (at, key) in keys.into_iter().enumerate() {
        assert_eq!(joined_on(key), None, "key {at}");
    }
}

/// `lit(true) != null`, which Polars folds into a null.
fn folded() -> Value {
    chain(
        call("polars.lit", json!({ "value": { "flag": true } })),
        vec![compared("Expr.ne", null())],
    )
}

#[test]
fn a_comparison_with_what_an_earlier_one_folded_is_left_to_a_limit() {
    // Polars refuses `null == f` and `f == (true != null)`, and allows
    // `ra.is_not_null() == ((1 == null) != 1)`: what it folds depends on dtypes
    // that the document does not decide, so the rule refuses only a column
    // compared with a null literal itself.
    let keys = [
        chain(
            call("polars.lit", json!({ "value": { "flag": true } })),
            vec![compared("Expr.ne", null()), compared("Expr.eq", field("f"))],
        ),
        field_then("f", vec![compared("Expr.eq", folded())]),
        field_then(
            "b",
            vec![
                call("Expr.is_not_null", json!({})),
                compared("Expr.eq", folded()),
            ],
        ),
    ];
    for (at, key) in keys.into_iter().enumerate() {
        assert_eq!(joined_on(key), None, "key {at}");
    }
    assert_eq!(joined_on(folded()), None);
}

#[test]
fn an_aggregate_that_a_comparison_folded_by_a_null_reaches_is_not_a_cause() {
    // A comparison with what an earlier one folded is itself a null, and takes
    // the aggregate it follows or is given with it: Polars allows these keys.
    let keys = [
        // `f.first() <= (null != true) > b`
        field_then(
            "f",
            vec![
                call("Expr.first", json!({})),
                compared(
                    "Expr.le",
                    chain(
                        call(
                            "polars.lit",
                            json!({ "value": { "flag": null }, "dtype": { "dtype": "Float32" } }),
                        ),
                        vec![compared(
                            "Expr.ne",
                            chain(
                                call("polars.lit", json!({ "value": { "flag": true } })),
                                vec![],
                            ),
                        )],
                    ),
                ),
                compared("Expr.gt", field("b")),
            ],
        ),
        // `when(k > k).then((1 == null) >= true.first()).otherwise(f)`
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![field_then("k", vec![compared("Expr.gt", field("k"))])]) }),
            ),
            vec![
                call(
                    "When.then",
                    json!({ "statement": chain(
                        call("polars.lit", json!({ "value": { "integer": 1 } })),
                        vec![
                            compared("Expr.eq", null()),
                            compared(
                                "Expr.ge",
                                chain(
                                    call("polars.lit", json!({ "value": { "flag": true } })),
                                    vec![call("Expr.first", json!({}))],
                                ),
                            ),
                        ],
                    ) }),
                ),
                call("Then.otherwise", json!({ "statement": field("f") })),
            ],
        ),
    ];
    for (at, key) in keys.into_iter().enumerate() {
        assert_eq!(joined_on(key), None, "key {at}");
    }
    // A column compared with a null is refused still, and an aggregate beside a
    // column where no null reaches it.
    let rule = Some(RefusalSource::Rule(Rule::KeyIsElementwise));
    assert_eq!(
        joined_on(field_then("f", vec![compared("Expr.eq", null())])),
        rule
    );
    assert_eq!(
        joined_on(field_then(
            "k",
            vec![compared(
                "Expr.sub",
                field_then("a", vec![call("Expr.sum", json!({}))])
            )]
        )),
        rule
    );
}

#[test]
fn a_subtraction_with_a_null_leaves_an_aggregate_in_a_join_key() {
    // Polars refuses `k != f.len() - null`, `f.len() - null != k` and
    // `k - (f.len() - null)` as not elementwise: a comparison with a null drops
    // the aggregate it follows or is given, and a subtraction with one does not.
    let rule = Some(RefusalSource::Rule(Rule::KeyIsElementwise));
    let subtracted = || {
        field_then(
            "f",
            vec![call("Expr.len", json!({})), compared("Expr.sub", null())],
        )
    };
    let keys = [
        field_then("k", vec![compared("Expr.ne", subtracted())]),
        chain(
            call("polars.col", json!({ "name": { "columns": ["f"] } })),
            vec![
                call("Expr.len", json!({})),
                compared("Expr.sub", null()),
                compared("Expr.ne", field("k")),
            ],
        ),
        field_then("k", vec![compared("Expr.sub", subtracted())]),
    ];
    for (at, key) in keys.into_iter().enumerate() {
        assert_eq!(joined_on(key), rule, "key {at}");
    }
    // The comparison with a null still drops it.
    let compared_with_a_null = field_then(
        "k",
        vec![compared(
            "Expr.sub",
            field_then(
                "f",
                vec![call("Expr.len", json!({})), compared("Expr.eq", null())],
            ),
        )],
    );
    assert_eq!(joined_on(compared_with_a_null), None);
}

#[test]
fn a_fold_drops_an_aggregate_only_from_a_side_that_is_one_value_or_literals() {
    // Polars refuses `(k - a.sum()) > (b.first() == null)` as not elementwise: it
    // folds the comparison, but evaluates the side that holds a column beside the
    // aggregate. It drops a side that is one value or literals alone.
    let rule = Some(RefusalSource::Rule(Rule::KeyIsElementwise));
    let aggregate = || field_then("a", vec![call("Expr.sum", json!({}))]);
    let folds = || {
        field_then(
            "b",
            vec![call("Expr.first", json!({})), compared("Expr.eq", null())],
        )
    };
    let mixed_follows = field_then(
        "k",
        vec![
            compared("Expr.sub", aggregate()),
            compared("Expr.gt", folds()),
        ],
    );
    assert_eq!(joined_on(mixed_follows), rule);
    let mixed_given = field_then(
        "a",
        vec![
            call("Expr.first", json!({})),
            compared(
                "Expr.gt",
                field_then(
                    "k",
                    vec![
                        compared("Expr.sub", aggregate()),
                        compared("Expr.eq", null()),
                    ],
                ),
            ),
        ],
    );
    assert_eq!(joined_on(mixed_given), rule);
    // One value on each side is dropped.
    let single = field_then(
        "a",
        vec![call("Expr.first", json!({})), compared("Expr.gt", folds())],
    );
    assert_eq!(joined_on(single), None);
}

#[test]
fn an_aggregate_that_the_first_fold_drops_stays_dropped_for_the_comparisons_after_it() {
    // `a.len() <= null == h <= h`: the first comparison folds into a null and
    // drops the aggregate it follows, which is one value, and the comparisons
    // that follow compare a null, so the key is no more refused for the aggregate
    // than `a.len() <= null` is.
    let key = field_then(
        "a",
        vec![
            call("Expr.len", json!({})),
            compared("Expr.le", null()),
            compared("Expr.eq", field("h")),
            compared("Expr.le", field("h")),
        ],
    );
    assert_eq!(joined_on(key), None);
}

#[test]
fn a_comparison_given_a_null_drops_the_aggregate_it_follows_after_an_earlier_fold() {
    // `null < true`, `.last()`, `!= null`, `.is_not_null()` and `!= f` is a key
    // Polars allows: the first comparison folds into a null, and the one that is
    // given a null drops the aggregate before it, as the first would have, and what
    // follows is a literal that a column is compared with. Without the comparison
    // with a null the aggregate stays, and Polars refuses the key, as it refuses
    // one that ends at the aggregate beside the column.
    let rule = Some(RefusalSource::Rule(Rule::KeyIsElementwise));
    let key = |dropped: bool, test: &str| {
        let mut links = vec![
            compared(
                "Expr.lt",
                chain(
                    call("polars.lit", json!({ "value": { "flag": true } })),
                    vec![],
                ),
            ),
            call("Expr.last", json!({})),
        ];
        if dropped {
            links.push(compared("Expr.ne", null()));
        }
        links.push(call(test, json!({})));
        links.push(compared("Expr.ne", field("f")));
        chain(
            call("polars.lit", json!({ "value": { "flag": null } })),
            links,
        )
    };
    for test in ["Expr.is_not_null", "Expr.is_null"] {
        assert_eq!(joined_on(key(true, test)), None, "{test}");
        assert_eq!(joined_on(key(false, test)), rule, "{test} without the null");
    }
}

#[test]
fn a_column_compared_with_the_null_that_a_dropping_comparison_gives_is_left_to_a_limit() {
    // Polars refuses `null < true`, `.last()`, `!= null`, `!= f` as not
    // elementwise, for a column compared with the null that the fold gave. The
    // walk allows it: the comparison given a null drops the aggregate, and a
    // comparison with what an earlier one folded is left to the limit
    // `a-null-comparison-in-a-key`, which excuses the difference. A later
    // tightening of the rule turns this test, and the kept case of the limit
    // that says the same, over.
    let key = chain(
        call("polars.lit", json!({ "value": { "flag": null } })),
        vec![
            compared(
                "Expr.lt",
                chain(
                    call("polars.lit", json!({ "value": { "flag": true } })),
                    vec![],
                ),
            ),
            call("Expr.last", json!({})),
            compared("Expr.ne", null()),
            compared("Expr.ne", field("f")),
        ],
    );
    assert_eq!(joined_on(key), None);
}

#[test]
fn a_call_refused_without_the_dtype_of_an_argument_says_so() {
    // `b.sub(1)` takes text and a number that is given no dtype, which the walk
    // cannot say the dtype of: the subtraction is refused for any dtype the
    // number might take, and the reason is Polars' for one of them. `b.sub(a)`
    // is refused with every dtype known.
    let select = |argument: Value| {
        walk_of(
            "LazyFrame.select",
            json!({ "exprs": many(vec![field_then("b", vec![compared("Expr.sub", argument)])]) }),
        )
    };
    let number = chain(
        call("polars.lit", json!({ "value": { "integer": 1 } })),
        vec![],
    );
    let blind = select(number);
    let refusal = blind.refusal().expect("text minus a number is refused");
    assert!(refusal.argument_unknown());
    let known = select(field("a"));
    let refusal = known.refusal().expect("text minus a column is refused");
    assert!(!refusal.argument_unknown());
}

#[test]
fn a_comparison_of_an_aggregate_with_a_null_is_no_comparison_of_a_column() {
    // `f.first() == null` is one value, and what is compared with it is a column
    // compared with a value that is not null.
    let key = field_then(
        "b",
        vec![compared(
            "Expr.le",
            field_then(
                "a",
                vec![
                    call("Expr.first", json!({})),
                    compared("Expr.eq", null()),
                    compared("Expr.gt", field("f")),
                ],
            ),
        )],
    );
    assert_eq!(joined_on(key), None);
}

#[test]
fn a_condition_whose_values_are_one_expression_has_the_shape_of_that_expression() {
    // Polars simplifies `when(p).then(1).otherwise(1)` to `1`, and drops the
    // predicate, whatever it gives: the condition is one value, so a window that
    // joins after it is not known to give a list.
    let literal = || {
        chain(
            call(
                "polars.lit",
                json!({ "value": { "integer": 1 }, "dtype": { "dtype": "UInt64" } }),
            ),
            vec![],
        )
    };
    let condition = |otherwise: Value| {
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![field("f")]) }),
            ),
            vec![
                call("When.then", json!({ "statement": literal() })),
                call("Then.otherwise", json!({ "statement": otherwise })),
                call(
                    "Expr.over",
                    json!({
                        "partition_by": many(vec![field("k")]),
                        "mapping_strategy": { "choice": "join" },
                    }),
                ),
            ],
        )
    };
    let places = |otherwise: Value| {
        let walk = walk_of(
            "LazyFrame.select",
            json!({ "exprs": many(vec![condition(otherwise)]) }),
        );
        expression_at(&walk, &item("exprs", 0)).places()
    };
    // One expression twice: one value.
    assert_eq!(places(literal()).last(), Some(&None));
    // Two values: a value for each row, as the predicate is, and a list.
    let other = chain(
        call(
            "polars.lit",
            json!({ "value": { "integer": 2 }, "dtype": { "dtype": "UInt64" } }),
        ),
        vec![],
    );
    let many_values = places(other);
    assert!(matches!(many_values.last(), Some(Some(_))));
}

#[test]
fn a_condition_with_a_refused_predicate_gives_a_place_that_is_not_known() {
    // The shape of a condition is that of its parts, and a predicate that is
    // refused has none, so a window that joins after it is not known to give a
    // list.
    let condition = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![chain(
                call("polars.lit", json!({ "value": { "string": "1d" } })),
                vec![call("Expr.dt.month_start", json!({}))],
            )]) }),
        ),
        vec![
            call(
                "When.then",
                json!({ "statement": field_then("f", vec![call("Expr.len", json!({}))]) }),
            ),
            call(
                "Then.otherwise",
                json!({ "statement": field_then("b", vec![call("Expr.len", json!({}))]) }),
            ),
            call(
                "Expr.over",
                json!({
                    "partition_by": many(vec![field("k")]),
                    "mapping_strategy": { "choice": "join" },
                }),
            ),
        ],
    );
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![condition]) }),
    );
    let item = expression_at(&walk, &item("exprs", 0));
    assert_eq!(item.places().last(), Some(&None));
}

#[test]
fn the_values_of_a_condition_need_be_boolean_in_a_filter_only_where_it_ends_the_predicate() {
    let condition = |links: Vec<Value>| {
        let mut all = vec![
            call("When.then", json!({ "statement": field("b") })),
            call("Then.otherwise", json!({ "statement": field("b") })),
        ];
        all.extend(links);
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![field("f")]) }),
            ),
            all,
        )
    };
    // Text is no predicate where the condition is the predicate.
    let ends = walk_of(
        "LazyFrame.filter",
        json!({ "predicates": many(vec![condition(vec![])]) }),
    );
    let refusal = ends.refusal().expect("text is no predicate");
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Rule(Rule::FilterPredicateIsBoolean)
    );
    // A test for a null goes on from it, and gives the Boolean.
    let goes_on = walk_of(
        "LazyFrame.filter",
        json!({ "predicates": many(vec![condition(vec![call("Expr.is_null", json!({}))])]) }),
    );
    assert_eq!(goes_on.refusal(), None);
}

#[test]
fn a_join_key_that_holds_a_single_value_beside_a_column_is_refused_by_a_rule() {
    let key = field_then(
        "k",
        vec![call(
            "Expr.sub",
            json!({ "other": field_then("a", vec![call("Expr.sum", json!({}))]) }),
        )],
    );
    let walk = walk_of(
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![key]),
            "right_on": many(vec![field("rk")]),
        }),
    );
    let refusal = walk.refusal().expect("a key that is not elementwise");
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Rule(Rule::KeyIsElementwise)
    );
    assert_eq!(refusal.place().path(), item("left_on", 0).as_slice());

    // A key that is wholly one value is the whole key.
    let whole = field_then("k", vec![call("Expr.sum", json!({}))]);
    let accepted = walk_of(
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![whole]),
            "right_on": many(vec![field("rk")]),
        }),
    );
    assert_eq!(accepted.refusal(), None);
}

#[test]
fn a_call_that_may_not_stand_where_it_does_is_the_first_refusal_and_stops_its_chain() {
    // A text has no running sum.
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![field_then(
            "b",
            vec![
                call("Expr.cum_sum", json!({})),
                call("Expr.is_null", json!({})),
            ],
        )]) }),
    );
    let refusal = walk.refusal().expect("refused");
    assert!(!refusal.at_end());
    assert_eq!(
        refusal.place().path(),
        [parameter("exprs"), Segment::Item(0), Segment::Link(0)]
    );
    let item = expression_at(&walk, &item("exprs", 0));
    assert!(item.calls()[1].verdict().is_some_and(|v| !v.is_allowed()));
    // Nothing is asked after a refused call.
    assert!(item.calls()[2].verdict().is_none());
    assert_eq!(item.places()[1..], [None, None]);
}

#[test]
fn the_report_gives_each_expression_its_context_and_what_each_link_follows() {
    let document = document_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![field_then(
            "a",
            vec![call("Expr.cum_sum", json!({}))],
        )]) }),
    );
    let report = document.run(&inputs(), source().lazy()).expect("a report");
    let step = &report.steps()[0];
    let found = step
        .expressions()
        .iter()
        .find(|expression| expression.place().path() == item("exprs", 0).as_slice())
        .expect("the item");
    // The walk owns the context and the places, and is the one the step gives.
    let walk = step.walk();
    assert_eq!(
        walk,
        &document.steps()[0].walk(0, source().schema(), &inputs())
    );
    let select = Context::step(Outer::Select);
    let walked = expression_at(walk, &item("exprs", 0));
    assert_eq!(walked.context(), Some(select));
    assert_eq!(
        walked.places(),
        [
            at(select, after(Representative::Int64, Shape::Many)),
            at(select, after(Representative::Int64, Shape::Many)),
        ]
    );
    // Polars' own answer is beside it.
    assert!(matches!(found.start(), Resolution::Dtype(DataType::Int64)));
}

#[test]
fn an_expression_after_a_refused_one_is_walked_all_the_same() {
    // `dt.date` of whole numbers is refused by the tables, so the step stops
    // there when it is built; the second expression is walked all the same.
    let document = document_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![
            field_then("a", vec![call("Expr.dt.date", json!({}))]),
            field_then("b", vec![call("Expr.is_null", json!({}))]),
        ]) }),
    );
    let report = document.run(&inputs(), source().lazy()).expect("a report");
    let step = &report.steps()[0];
    assert!(step.walk().refusal().is_some());
    let second = expression_at(step.walk(), &item("exprs", 1));
    assert_eq!(second.calls().len(), 2);
    assert!(second.calls()[1].verdict().is_some_and(|v| v.is_allowed()));
}

/// The expression argument that starts from the connected input `name` and
/// continues with `then`.
fn from_input(name: &str, then: Vec<Value>) -> Value {
    chain(json!({ "input": name }), then)
}

#[test]
fn a_chain_that_starts_from_a_connected_value_is_walked_as_a_literal_of_its_dtype() {
    // A connected value is run as a literal of its dtype, so what is true of
    // `lit` of a whole number is true of it: one value, and a test for a null
    // of one value is a Boolean, one value.
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![from_input(
            "cutoff",
            vec![call("Expr.is_null", json!({}))],
        )]) }),
    );
    assert_eq!(walk.not_known(), None);
    assert_eq!(walk.refusal(), None);
    let walked = expression_at(&walk, &item("exprs", 0));
    let select = Context::step(Outer::Select);
    assert_eq!(walked.calls()[0].follows(), Some(After::Start));
    assert!(walked.calls()[0].verdict().is_some_and(|v| v.is_allowed()));
    assert_eq!(
        walked.places(),
        vec![
            at(select, after(Representative::Int64, Shape::One)),
            at(select, after(Representative::Boolean, Shape::One)),
        ]
    );
    assert_eq!(walked.ending(), Some(&Ending::Allowed));
}

#[test]
fn a_connected_list_gives_many_values_of_its_dtype() {
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![from_input("holidays", vec![])]) }),
    );
    assert_eq!(walk.not_known(), None);
    let walked = expression_at(&walk, &item("exprs", 0));
    assert_eq!(
        walked.places(),
        vec![at(
            Context::step(Outer::Select),
            after(Representative::Int64, Shape::Many)
        )]
    );
}

#[test]
fn a_connected_input_is_held_to_what_a_place_needs_of_the_dtype_it_gives() {
    // A filter's predicate is a Boolean, which a rule decides from the dtype.
    let predicate = |name: &str| {
        walk_of(
            "LazyFrame.filter",
            json!({ "predicates": many(vec![from_input(name, vec![])]) }),
        )
    };
    let flag = predicate("on");
    assert_eq!(flag.refusal(), None);
    assert_eq!(
        expression_at(&flag, &item("predicates", 0)).ending(),
        Some(&Ending::Allowed)
    );
    let number = predicate("cutoff");
    let refusal = number.refusal().expect("a whole number is no predicate");
    assert!(refusal.at_end());
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Rule(Rule::FilterPredicateIsBoolean)
    );
}

#[test]
fn an_input_that_is_not_connected_leaves_what_follows_it_not_known() {
    let walk = walk_of(
        "LazyFrame.select",
        json!({ "exprs": many(vec![from_input(
            "missing",
            vec![call("Expr.is_null", json!({}))],
        )]) }),
    );
    assert_eq!(
        walk.not_known(),
        Some("step \"s\", exprs item 1: there is no connected value or list called \"missing\"")
    );
    let walked = expression_at(&walk, &item("exprs", 0));
    assert_eq!(walked.places(), vec![None, None]);
}

#[test]
fn and_and_or_given_no_operand_are_refused_after_a_float_as_they_are_with_one() {
    // Polars lets them through with none, but a step always gives one, so they are judged with
    // one: after a float no operand suits, and after a whole number or a Boolean one does.
    for id in ["Expr.and_", "Expr.or_"] {
        let refused = |column: &str| {
            let walk = walk_of(
                "LazyFrame.select",
                json!({ "exprs": many(vec![field_then(column, vec![call(id, json!({}))])]) }),
            );
            walk.refusal().is_some()
        };
        assert!(refused("f32"), "{id} after a float");
        assert!(!refused("i8"), "{id} after a whole number");
        assert!(!refused("f"), "{id} after a Boolean");
    }
}
