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
        self.json().to_text()
    }

    /// The document as JSON text laid out two spaces deep, as a file holds it: the fields of
    /// [`Document::write`] in the same order, each on a line of its own, and a new line at the
    /// end. A change to one argument changes the lines that hold it, so the file diffs cleanly.
    ///
    /// Reading the text gives a document equal to this one, and writing that gives the same text.
    #[must_use]
    pub fn write_indented(&self) -> String {
        let mut text = self.json().to_indented_text();
        text.push('\n');
        text
    }

    fn json(&self) -> Json {
        Json::Object(vec![
            ("version".to_owned(), Json::integer(VERSION.into())),
            (
                "steps".to_owned(),
                Json::Array(self.steps().iter().map(step).collect()),
            ),
        ])
    }
}

/// How many arrays and objects `step` opens inside each other when written,
/// or `None` when that is more than `limit`. It follows the shape [`step`]
/// writes without building it, and stops as soon as the limit is passed, so
/// it never recurses more than `limit` levels however deep the step is.
pub(super) fn depth_within(step: &Step, limit: usize) -> Option<usize> {
    nested(limit, |left| call_depth(step.call(), left))
}

/// The depth of one array or object whose contents are measured by `inner`
/// with the levels left inside it.
fn nested(limit: usize, inner: impl FnOnce(usize) -> Option<usize>) -> Option<usize> {
    let left = limit.checked_sub(1)?;
    inner(left).map(|depth| depth + 1)
}

/// The deepest of `items`, each measured by `each` within `limit`.
fn deepest<T>(
    items: impl IntoIterator<Item = T>,
    limit: usize,
    each: impl Fn(T, usize) -> Option<usize>,
) -> Option<usize> {
    let mut most = 0;
    for item in items {
        most = most.max(each(item, limit)?);
    }
    Some(most)
}

/// A call's object, holding its entry and the object of its arguments.
fn call_depth(call: &Call, limit: usize) -> Option<usize> {
    nested(limit, |left| {
        nested(left, |left| {
            deepest(call.arguments(), left, |given, left| {
                value_depth(given.value(), left)
            })
        })
    })
}

/// An expression's object, holding its start and the array of its links.
fn expression_depth(expression: &Expression, limit: usize) -> Option<usize> {
    nested(limit, |left| {
        let start = match expression.start() {
            Start::Call(start) => call_depth(start, left)?,
            Start::Input(_) | Start::Hole => nested(left, |_| Some(0))?,
        };
        let links = nested(left, |left| deepest(expression.links(), left, call_depth))?;
        Some(start.max(links))
    })
}

/// A value's object, holding its kind and what it holds.
fn value_depth(value: &Value, limit: usize) -> Option<usize> {
    nested(limit, |left| match value {
        Value::Expression(inner) => expression_depth(inner, left),
        Value::Columns(_) => nested(left, |_| Some(0)),
        Value::Pairs(pairs) => nested(left, |left| {
            deepest(pairs, left, |(key, entry), left| {
                nested(left, |left| {
                    Some(value_depth(key, left)?.max(value_depth(entry, left)?))
                })
            })
        }),
        Value::Many(items) => nested(left, |left| deepest(items, left, value_depth)),
        _ => Some(0),
    })
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
                Start::Hole => Json::Object(vec![entry("hole", Json::Null)]),
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
        Value::Field(name) => ("field", Json::String(name.clone())),
        Value::Integer(number) => ("integer", Json::integer(*number)),
        // A float is finite wherever a `Call` holds it; `null` is not reachable.
        Value::Float(number) => ("float", Json::float(*number).unwrap_or(Json::Null)),
        Value::String(text) => ("string", Json::String(text.clone())),
        Value::Flag(flag) => ("flag", flag.map_or(Json::Null, Json::Bool)),
        Value::Choice(word) => ("choice", Json::String(word.clone())),
        Value::Dtype(dtype) => ("dtype", Json::String(dtype.to_string())),
        Value::Duration(text) => ("duration", Json::String(text.clone())),
        Value::Pattern(text) => ("pattern", Json::String(text.clone())),
        Value::TimeZone(zone) => (
            "time_zone",
            zone.as_ref()
                .map_or(Json::Null, |zone| Json::String(zone.clone())),
        ),
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
