//! Every place an expression stands, as the tables of contexts name them, and the case of a step
//! that holds an expression there.
//!
//! An expression stands at a position of a step, and may stand inside another expression there:
//! what a window groups or orders by, a condition's predicate or value. A nested place is built
//! around the expression by a host of the world's columns that fits the position: a flag where the
//! position takes a predicate (a filter, a group's `having`), a whole number elsewhere.

use dageist_engine::allowed::{Context, Nested, Outer};
use dageist_fuzz::grid::Choices;
use polars::prelude::DataFrame;
use serde_json::{Value, json};

use crate::doc::{call, chain, column, column_then, many, named};
use crate::judge::{Case, Connections};
use crate::world::World;

/// Every place the tables measure, as an axis named `name`. A right key of a join is left out, for
/// the case of a left key holds the same key on the right too, and a pivot's aggregate is left out,
/// for it starts from `element()`, which an expression of the world's columns does not.
#[must_use]
pub fn places(name: &'static str) -> Choices<Context> {
    let mut axis = Choices::new(name);
    for context in Context::all() {
        let reason = match context.outer() {
            Outer::JoinRight => Some(
                "a join's key stands on both sides in the case of a left key, which judges it as a \
                 right key too",
            ),
            Outer::PivotAggregate => Some(
                "a pivot's aggregate starts from element(), which an expression of the world's \
                 columns does not",
            ),
            _ => None,
        };
        axis = match reason {
            Some(reason) => axis.leave_out(context.name(), reason),
            None => axis.with(context.name(), context),
        };
    }
    axis
}

/// The places of a step only, with no expression around the one judged, as an axis named `name`.
#[must_use]
pub fn step_places(name: &'static str) -> Choices<Context> {
    places(name).keep_unless(|_, context| {
        context
            .nested()
            .map(|_| "a place inside another expression".to_owned())
    })
}

/// The column of the world a host built around a nested place reads: a flag where the position
/// of the step takes a predicate, and a whole number elsewhere.
const fn host_column(outer: Outer) -> &'static str {
    match outer {
        Outer::Filter | Outer::Having => "f",
        _ => "n",
    }
}

/// The expression that holds `expression` at the nested place `nested`.
fn host(nested: Nested, outer: Outer, expression: Value) -> Value {
    let hosted = host_column(outer);
    match nested {
        Nested::PartitionBy => column_then(
            hosted,
            vec![call(
                "Expr.over",
                json!({ "partition_by": many(vec![expression]) }),
            )],
        ),
        Nested::OrderBy => column_then(
            hosted,
            vec![call(
                "Expr.over",
                json!({ "order_by": many(vec![expression]) }),
            )],
        ),
        Nested::ConditionPredicate => chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![expression]) }),
            ),
            vec![
                call("When.then", json!({ "statement": column(hosted) })),
                call("Then.otherwise", json!({ "statement": column(hosted) })),
            ],
        ),
        Nested::ConditionValue => chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![column("f")]) }),
            ),
            vec![
                call("When.then", json!({ "statement": expression.clone() })),
                call("Then.otherwise", json!({ "statement": expression })),
            ],
        ),
    }
}

/// The same expression read against the table a join takes, whose columns are the world's under
/// the names [`Column::other_name`](crate::world::Column::other_name) gives them.
#[must_use]
pub fn on_the_other_table(expression: &Value) -> Value {
    match expression {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| {
                    let value = match (key.as_str(), value) {
                        ("columns", Value::Array(names)) => Value::Array(
                            names
                                .iter()
                                .map(|name| match name {
                                    Value::String(name) => Value::String(format!("r{name}")),
                                    other => other.clone(),
                                })
                                .collect(),
                        ),
                        _ => on_the_other_table(value),
                    };
                    (key.clone(), value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(on_the_other_table).collect()),
        other => other.clone(),
    }
}

/// The case of a step that holds `expression` at `context`, run on `working`, with the world's
/// other table connected for a join.
///
/// # Errors
///
/// When the context is one [`places`] leaves out, or the document cannot be read.
pub fn at(context: Context, expression: Value, working: DataFrame) -> Result<Case, String> {
    let outer = context.outer();
    let expression = match context.nested() {
        Some(nested) => host(nested, outer, expression),
        None => expression,
    };
    let (entry, arguments) = match outer {
        Outer::Select => (
            "LazyFrame.select",
            json!({ "exprs": many(vec![expression]) }),
        ),
        Outer::Filter => (
            "LazyFrame.filter",
            json!({ "predicates": many(vec![expression]) }),
        ),
        Outer::Sort => ("LazyFrame.sort", json!({ "by": many(vec![expression]) })),
        Outer::JoinLeft => {
            let right = on_the_other_table(&expression);
            return Case::of_step(
                "LazyFrame.join",
                json!({
                    "other": { "table": "other" },
                    "left_on": many(vec![expression]),
                    "right_on": many(vec![right]),
                }),
                working,
                Connections::new().table("other", World::get().other().clone()),
            );
        }
        Outer::GroupKey => {
            return Case::of_grouping(
                json!({ "by": many(vec![expression]) }),
                vec![call("LazyGroupBy.len", json!({}))],
                working,
                Connections::new(),
            );
        }
        Outer::Aggregate => {
            return Case::of_grouping(
                json!({ "by": many(vec![column("k")]) }),
                vec![call(
                    "LazyGroupBy.agg",
                    json!({ "named_aggs": named(vec![("out", expression)]) }),
                )],
                working,
                Connections::new(),
            );
        }
        Outer::Having => {
            return Case::of_grouping(
                json!({ "by": many(vec![column("k")]) }),
                vec![
                    call(
                        "LazyGroupBy.having",
                        json!({ "predicates": many(vec![expression]) }),
                    ),
                    call("LazyGroupBy.len", json!({})),
                ],
                working,
                Connections::new(),
            );
        }
        Outer::JoinRight | Outer::PivotAggregate => {
            return Err(format!("the place {context} is left out of the grids"));
        }
    };
    Case::of_step(entry, arguments, working, Connections::new())
}
