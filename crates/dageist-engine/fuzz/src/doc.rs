//! The JSON of a step document, so a grid says what a step does and not how it is spelled.

use serde_json::{Value, json};

/// A call of the catalogue entry `entry`.
#[must_use]
pub fn call(entry: &str, arguments: Value) -> Value {
    json!({ "entry": entry, "arguments": arguments })
}

/// An expression argument that starts with `start` and continues with `then`.
#[must_use]
pub fn chain(start: Value, then: Vec<Value>) -> Value {
    json!({ "expression": { "start": start, "then": then } })
}

/// The expression argument `start` continued with `more` links after its own.
///
/// # Panics
///
/// When `expression` is not an expression argument.
#[must_use]
pub fn then(mut expression: Value, more: impl IntoIterator<Item = Value>) -> Value {
    let links = expression["expression"]["then"]
        .as_array_mut()
        .expect("an expression argument");
    links.extend(more);
    expression
}

/// The start `polars.col(name)`.
#[must_use]
pub fn column_call(name: &str) -> Value {
    call("polars.col", json!({ "name": { "columns": [name] } }))
}

/// The expression argument `polars.col(name)`.
#[must_use]
pub fn column(name: &str) -> Value {
    chain(column_call(name), Vec::new())
}

/// The expression argument `polars.col(name)` followed by `then`.
#[must_use]
pub fn column_then(name: &str, then: Vec<Value>) -> Value {
    chain(column_call(name), then)
}

/// The expression argument `polars.lit(number)`, a whole number typed without a dtype.
#[must_use]
pub fn whole(number: i64) -> Value {
    chain(
        call("polars.lit", json!({ "value": { "integer": number } })),
        Vec::new(),
    )
}

/// The expression argument `polars.lit(number)`, a number with a fraction typed without a dtype.
#[must_use]
pub fn fraction(number: f64) -> Value {
    chain(
        call("polars.lit", json!({ "value": { "float": number } })),
        Vec::new(),
    )
}

/// The expression argument `polars.lit("text")`.
#[must_use]
pub fn text(text: &str) -> Value {
    chain(
        call("polars.lit", json!({ "value": { "string": text } })),
        Vec::new(),
    )
}

/// The argument that is the flag `flag`.
#[must_use]
pub fn flag(flag: bool) -> Value {
    json!({ "flag": flag })
}

/// The argument that is the whole number `number`, typed into a parameter that is no expression.
#[must_use]
pub fn integer(number: i64) -> Value {
    json!({ "integer": number })
}

/// The argument that is the number `number` with a fraction, typed into a parameter that is no
/// expression.
#[must_use]
pub fn float(number: f64) -> Value {
    json!({ "float": number })
}

/// The argument that is the text `text`, typed into a parameter that is no expression.
#[must_use]
pub fn string(text: &str) -> Value {
    json!({ "string": text })
}

/// The argument that is one word of a choice.
#[must_use]
pub fn choice(word: &str) -> Value {
    json!({ "choice": word })
}

/// The argument that is the dtype a document writes as `word`.
#[must_use]
pub fn dtype(word: &str) -> Value {
    json!({ "dtype": word })
}

/// The argument that is the pattern `text`.
#[must_use]
pub fn pattern(text: &str) -> Value {
    json!({ "pattern": text })
}

/// The argument that names the columns `names`.
#[must_use]
pub fn columns(names: &[&str]) -> Value {
    json!({ "columns": names })
}

/// A list argument.
#[must_use]
pub fn many(items: Vec<Value>) -> Value {
    json!({ "many": items })
}

/// The `**named_exprs` argument of a step: each name with its expression.
#[must_use]
pub fn named(entries: Vec<(&str, Value)>) -> Value {
    json!({ "pairs": entries
        .into_iter()
        .map(|(name, expression)| json!([{ "columns": [name] }, expression]))
        .collect::<Vec<_>>() })
}

/// A step that calls `entry`.
#[must_use]
pub fn step(id: &str, entry: &str, arguments: Value) -> Value {
    json!({ "id": id, "call": call(entry, arguments) })
}

/// A step that groups by `arguments` and goes on with the calls `then`, the last of which
/// finishes the grouping.
#[must_use]
pub fn grouping(id: &str, arguments: Value, then: Vec<Value>) -> Value {
    json!({ "id": id, "call": call("LazyFrame.group_by", arguments), "then": then })
}

/// The text of a document of `steps`.
#[must_use]
pub fn document(steps: Vec<Value>) -> String {
    json!({ "version": 1, "steps": steps }).to_string()
}
