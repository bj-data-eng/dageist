//! Builders for the JSON of a document, so a test says what a step does and
//! not how it is spelled.

use serde_json::{Value, json};

/// A call of the catalogue entry `entry`.
pub fn call(entry: &str, arguments: Value) -> Value {
    json!({ "entry": entry, "arguments": arguments })
}

/// An expression argument that starts with `start` and continues with `then`.
pub fn chain(start: Value, then: Vec<Value>) -> Value {
    json!({ "expression": { "start": start, "then": then } })
}

/// The start `polars.col(name)`.
pub fn col_call(name: &str) -> Value {
    call("polars.col", json!({ "name": { "columns": [name] } }))
}

/// The expression argument `polars.col(name)`.
pub fn field(name: &str) -> Value {
    chain(col_call(name), vec![])
}

/// The expression argument `polars.col(name)` followed by `then`.
pub fn field_then(name: &str, then: Vec<Value>) -> Value {
    chain(col_call(name), then)
}

/// The start `polars.lit("text")`.
pub fn text_call(text: &str) -> Value {
    call("polars.lit", json!({ "value": { "string": text } }))
}

/// The expression argument `polars.lit("text")`.
pub fn text(text: &str) -> Value {
    chain(text_call(text), vec![])
}

/// The expression argument `polars.lit(number)`.
pub fn whole(number: i64) -> Value {
    chain(
        call("polars.lit", json!({ "value": { "integer": number } })),
        vec![],
    )
}

/// A list argument.
pub fn many(items: Vec<Value>) -> Value {
    json!({ "many": items })
}

/// The `**named_exprs` argument of a step: each name with its expression.
pub fn named(entries: Vec<(&str, Value)>) -> Value {
    json!({ "pairs": entries
        .into_iter()
        .map(|(name, expression)| json!([{ "columns": [name] }, expression]))
        .collect::<Vec<_>>() })
}

/// A step that calls `entry`.
pub fn step(id: &str, entry: &str, arguments: Value) -> Value {
    json!({ "id": id, "call": call(entry, arguments) })
}

/// The text of a document of `steps`.
pub fn document(steps: Vec<Value>) -> String {
    json!({ "version": 1, "steps": steps }).to_string()
}

/// The argument that is the pattern `text`.
pub fn pattern(text: &str) -> Value {
    json!({ "pattern": text })
}
