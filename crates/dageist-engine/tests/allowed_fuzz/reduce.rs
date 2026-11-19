//! Smaller cases that are valid cases, for the framework's reduction: a
//! simpler dtype for `a` or `b`, a call, a link or an option left out, an
//! argument that is only its start. Each is built through the document's own
//! constructors, so what they refuse is not offered, and nothing is repaired.

use dageist_engine::allowed::Representative;
use dageist_engine::document::{Call, Document, Expression, Step, Value};

use super::case::Case;

/// Whether `smaller` encodes shorter than `larger`, or the same length and
/// lexically below it, as the framework's search measures.
fn smaller(smaller: &Case, larger: &[u8]) -> bool {
    let bytes = smaller.encode();
    (bytes.len(), bytes.as_slice()) < (larger.len(), larger)
}

pub fn candidates(case: &Case) -> Vec<Case> {
    let encoded = case.encode();
    let mut found = Vec::new();
    for receiver in Representative::ALL {
        found.push(Case {
            receiver,
            ..case.clone()
        });
    }
    for argument in Representative::ALL {
        found.push(Case {
            argument,
            ..case.clone()
        });
    }
    let step = &case.document.steps()[0];
    for call in call_alternatives(step.call()) {
        if let Ok(step) = Step::new(step.id().clone(), call)
            && let Ok(document) = Document::new(vec![step])
        {
            found.push(Case {
                document,
                ..case.clone()
            });
        }
    }
    found.retain(|candidate| smaller(candidate, &encoded));
    found
}

/// The calls with one argument left out, or one argument made simpler.
fn call_alternatives(call: &Call) -> Vec<Call> {
    let entry = call.entry();
    let given: Vec<(&str, Value)> = call
        .arguments()
        .iter()
        .map(|given| (given.parameter(), given.value().clone()))
        .collect();
    let mut found = Vec::new();
    for at in 0..given.len() {
        // A parameter that must be given is not left out: a call without it
        // is a different mistake, not a smaller case of the same one.
        let required = entry
            .parameters()
            .iter()
            .any(|parameter| parameter.name() == given[at].0 && parameter.is_needed());
        if !required {
            let mut without = given.clone();
            without.remove(at);
            found.extend(Call::new(entry, without).ok());
        }
        for value in value_alternatives(&given[at].1) {
            let mut changed = given.clone();
            changed[at].1 = value;
            found.extend(Call::new(entry, changed).ok());
        }
    }
    found
}

fn value_alternatives(value: &Value) -> Vec<Value> {
    match value {
        Value::Expression(expression) => expression_alternatives(expression)
            .into_iter()
            .map(Value::Expression)
            .collect(),
        Value::Many(items) => {
            let mut found = Vec::new();
            for at in 0..items.len() {
                if items.len() > 1 {
                    let mut without = items.clone();
                    without.remove(at);
                    found.push(Value::Many(without));
                }
                for item in value_alternatives(&items[at]) {
                    let mut changed = items.clone();
                    changed[at] = item;
                    found.push(Value::Many(changed));
                }
            }
            found
        }
        _ => Vec::new(),
    }
}

fn expression_alternatives(expression: &Expression) -> Vec<Expression> {
    let mut found = Vec::new();
    let start = expression.start();
    let links = expression.links();
    if !links.is_empty() {
        found.extend(Expression::new(start.clone(), Vec::new()).ok());
    }
    for at in 0..links.len() {
        let mut without = links.to_vec();
        without.remove(at);
        found.extend(Expression::new(start.clone(), without).ok());
    }
    if let dageist_engine::document::Start::Call(call) = start {
        for changed in call_alternatives(call) {
            found.extend(
                Expression::new(
                    dageist_engine::document::Start::Call(changed),
                    links.to_vec(),
                )
                .ok(),
            );
        }
    }
    for at in 0..links.len() {
        for changed in call_alternatives(&links[at]) {
            let mut with = links.to_vec();
            with[at] = changed;
            found.extend(Expression::new(start.clone(), with).ok());
        }
    }
    found
}
