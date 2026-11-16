//! Writing a document as JSON: the text [`Document::read`] reads back as the
//! same document.

use super::VERSION;
use super::json::Json;
use super::model::{Call, Document, Expression, Start, Step, Value};

impl Document {
    /// The document as compact JSON text, with each object's fields in the
    /// order [`Document::read`] describes them and each call's arguments in
    /// the order they were given.
    ///
    /// Reading the text gives a document equal to this one, and writing that
    /// gives the same text.
    #[must_use]
    pub fn write(&self) -> String {
        Json::Object(vec![
            ("version".to_owned(), Json::Number(VERSION.into())),
            (
                "steps".to_owned(),
                Json::Array(self.steps().iter().map(step).collect()),
            ),
        ])
        .to_text()
    }
}

fn entry(name: &str, value: Json) -> (String, Json) {
    (name.to_owned(), value)
}

fn step(step: &Step) -> Json {
    Json::Object(vec![
        entry("id", Json::String(step.id().as_str().to_owned())),
        entry("call", call(step.call())),
    ])
}

fn call(call: &Call) -> Json {
    Json::Object(vec![
        entry("entry", Json::String(call.entry().id().as_str().to_owned())),
        entry(
            "arguments",
            Json::Object(
                call.arguments()
                    .iter()
                    .map(|given| entry(given.parameter(), value(given.value())))
                    .collect(),
            ),
        ),
    ])
}

fn expression(expression: &Expression) -> Json {
    Json::Object(vec![
        entry(
            "start",
            match expression.start() {
                Start::Call(start) => call(start),
                Start::Input(name) => {
                    Json::Object(vec![entry("input", Json::String(name.clone()))])
                }
            },
        ),
        entry(
            "then",
            Json::Array(expression.links().iter().map(call).collect()),
        ),
    ])
}

fn strings(texts: &[String]) -> Json {
    Json::Array(texts.iter().cloned().map(Json::String).collect())
}

fn value(value: &Value) -> Json {
    let (kind, inner) = match value {
        Value::Expression(inner) => ("expression", expression(inner)),
        Value::Columns(names) => ("columns", strings(names)),
        Value::Integer(number) => ("integer", Json::Number((*number).into())),
        // A float is finite wherever a `Call` holds it; `null` is not reachable.
        Value::Float(number) => ("float", Json::float(*number).unwrap_or(Json::Null)),
        Value::String(text) => ("string", Json::String(text.clone())),
        Value::Flag(flag) => ("flag", flag.map_or(Json::Null, Json::Bool)),
        Value::Choice(word) => ("choice", Json::String(word.clone())),
        Value::Dtype(dtype) => ("dtype", Json::String(dtype.to_string())),
        Value::Duration(text) => ("duration", Json::String(text.clone())),
        Value::Table(name) => ("table", Json::String(name.clone())),
        Value::Path(text) => ("path", Json::String(text.clone())),
        Value::Pairs(pairs) => (
            "pairs",
            Json::Array(
                pairs
                    .iter()
                    .map(|(key, entry)| Json::Array(vec![self::value(key), self::value(entry)]))
                    .collect(),
            ),
        ),
        Value::Many(items) => ("many", Json::Array(items.iter().map(self::value).collect())),
    };
    Json::Object(vec![entry(kind, inner)])
}
