//! What the corpus holds, read back from the documents and the reports alone: each call of each
//! expression, with the place the walk says it stands at and what it says of it. The tests hold
//! these against the engine's own tables, not against what the corpus was made from.

use dageist_engine::document::{Place, Segment, Start, Value};
use serde_json::Value as Json;

use super::Reported;
use super::documents::{Held, held_at};

/// What a call of an expression follows, as the walk reports it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Follows {
    /// Nothing: the call starts the expression.
    Start,
    /// A value of this class of dtype, in this shape.
    Value { dtype: String, shape: String },
    /// A condition that has come to this state.
    Condition(String),
    /// The walk could not say.
    Unknown,
}

/// What the walk says of a call.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Verdict {
    Allowed,
    /// Refused by a rule, the word of its name, or by Polars at a stage, `polars schema`.
    Refused(String),
    /// The walk did not ask.
    NotAsked,
}

/// One call of an expression of a document.
#[derive(Debug, Clone)]
pub(super) struct Fact {
    /// The document it is in.
    pub document: String,
    /// The entry the call is of.
    pub entry: &'static str,
    /// The context of the expression, as the tables write it.
    pub context: Option<String>,
    pub follows: Follows,
    pub verdict: Verdict,
}

fn word(value: &Json) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

fn follows_of(json: &Json) -> Follows {
    match json["kind"].as_str() {
        Some("start") => Follows::Start,
        Some("value") => Follows::Value {
            dtype: word(&json["dtype"]),
            shape: word(&json["shape"]),
        },
        Some("condition") => Follows::Condition(word(&json["state"])),
        _ => Follows::Unknown,
    }
}

fn verdict_of(json: &Json) -> Verdict {
    match json["kind"].as_str() {
        Some("allowed") => Verdict::Allowed,
        Some("refused") => {
            let source = &json["refusal"]["source"];
            Verdict::Refused(match source["by"].as_str() {
                Some("rule") => format!("rule {}", word(&source["rule"])),
                Some("polars") => format!("polars {}", word(&source["stage"])),
                _ => "structure".to_owned(),
            })
        }
        _ => Verdict::NotAsked,
    }
}

/// The place a report names, in the engine's terms.
fn place_of(json: &Json) -> Place {
    let index = usize::try_from(json["stepIndex"].as_u64().expect("a step")).expect("a number");
    let mut place = Place::of_step(index, None);
    for segment in json["path"].as_array().expect("a way") {
        let at =
            || usize::try_from(segment["index"].as_u64().expect("an index")).expect("a number");
        place = place.then(match segment["kind"].as_str().expect("a kind") {
            "parameter" => Segment::Parameter(word(&segment["name"])),
            "item" => Segment::Item(at()),
            "key" => Segment::Key(at()),
            "value" => Segment::Value(at()),
            "start" => Segment::Start,
            "link" => Segment::Link(at()),
            other => panic!("a segment of kind {other}"),
        });
    }
    place
}

/// Every call of every expression the walk of any step of `documents` reports.
pub(super) fn facts(documents: &[&Reported]) -> Vec<Fact> {
    let mut found = Vec::new();
    for held in documents {
        for step in held.run["steps"].as_array().expect("steps") {
            for walked in step["walk"]["expressions"].as_array().expect("expressions") {
                let place = place_of(&walked["place"]);
                let Some(Held::Value(Value::Expression(expression))) = held_at(&held.model, &place)
                else {
                    continue;
                };
                let context = walked["context"]["outer"].as_str().map(|outer| {
                    match walked["context"]["nested"].as_str() {
                        Some(nested) => format!("{outer} > {nested}"),
                        None => outer.to_owned(),
                    }
                });
                let calls = walked["calls"].as_array().expect("calls");
                // The walk has a call for the start, though a start that is no call has no entry.
                let start = match expression.start() {
                    Start::Call(call) => Some(call),
                    Start::Input(_) | Start::Hole => None,
                };
                let entries = std::iter::once(start).chain(expression.links().iter().map(Some));
                for (call, standing) in entries.zip(calls) {
                    let Some(call) = call else { continue };
                    found.push(Fact {
                        document: held.name.clone(),
                        entry: call.entry().id().as_str(),
                        context: context.clone(),
                        follows: follows_of(&standing["follows"]),
                        verdict: verdict_of(&standing["verdict"]),
                    });
                }
            }
        }
    }
    found
}
