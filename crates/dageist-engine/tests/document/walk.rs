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
use dageist_engine::document::{Document, Inputs, Resolution, Segment, Walk, WalkedExpression};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, field_then, many, step};

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
            df!("ra" => ["x", "y", "z"], "rk" => [1_i64, 2, 3])
                .expect("the other table")
                .lazy(),
        )
        .expect("a first input");
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
fn whether_a_literal_of_no_dtype_may_end_a_predicate_is_not_known() {
    // `lit(1)` is whatever the other operand makes it: it may be a Boolean.
    let walk = walk_of(
        "LazyFrame.filter",
        json!({ "predicates": many(vec![chain(
            call("polars.lit", json!({ "value": { "integer": 1 } })),
            vec![],
        )]) }),
    );
    assert_eq!(walk.refusal(), None);
    let predicate = expression_at(&walk, &item("predicates", 0));
    assert_eq!(
        predicate.ending(),
        Some(&Ending::NotKnown(Unknown::TheStartsDtype))
    );
    assert!(walk.not_known().is_some());
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
    let select = Context::step(Outer::Select);
    assert_eq!(found.context(), Some(select));
    assert_eq!(
        found.places(),
        [
            at(select, after(Representative::Int64, Shape::Many)),
            at(select, after(Representative::Int64, Shape::Many)),
        ]
    );
    // Polars' own answer is beside it.
    assert!(matches!(found.start(), Resolution::Dtype(DataType::Int64)));
    // The walk is the one the step gives.
    assert_eq!(
        step.walk(),
        &document.steps()[0].walk(0, source().schema(), &inputs())
    );
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
