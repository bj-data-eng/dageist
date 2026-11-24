//! Editing a step document through its public interface. Each test starts from
//! a document written by hand, makes an edit, and compares the document it gives
//! with the JSON the document format says that edit makes.

mod arguments;
mod chains;
mod refusals;
mod round_trip;
mod steps;

use dageist_engine::document::{Document, EditError, Place, Segment};
use serde_json::{Value, json};

use crate::support::{chain, document, field, step};

/// The document the text of `steps` reads as.
pub fn read(steps: Vec<Value>) -> Document {
    Document::read(&document(steps)).expect("a document")
}

/// The JSON a document is written as.
pub fn json_of(document: &Document) -> Value {
    serde_json::from_str(&document.write()).expect("JSON")
}

/// Asserts that `edited` is the document of `steps`, and that it reads back as itself.
pub fn assert_steps(edited: &Document, steps: Vec<Value>) {
    let expected: Value = serde_json::from_str(&document(steps)).expect("JSON");
    assert_eq!(json_of(edited), expected);
    assert_eq!(
        &Document::read(&edited.write()).expect("the edited document reads"),
        edited
    );
}

/// The place of the step `id`, which is the `index`th.
pub fn step_at(index: usize, id: &str) -> Place {
    Place::of_step(
        index,
        Some(dageist_engine::document::StepId::new(id).expect("an id")),
    )
}

pub fn parameter(name: &str) -> Segment {
    Segment::Parameter(name.to_owned())
}

/// The place of a way inside the step `id`, which is the `index`th.
pub fn inside(index: usize, id: &str, way: Vec<Segment>) -> Place {
    way.into_iter()
        .fold(step_at(index, id), |place, segment| place.then(segment))
}

/// The expression argument that has nothing picked yet.
pub fn hole() -> Value {
    chain(json!({ "hole": null }), vec![])
}

/// A step that selects the fields `names`.
pub fn select(id: &str, names: &[&str]) -> Value {
    step(
        id,
        "LazyFrame.select",
        json!({ "exprs": { "many": names.iter().map(|name| field(name)).collect::<Vec<_>>() } }),
    )
}

/// The refusal an edit gave, which it must have.
pub fn refused(result: Result<Document, EditError>) -> EditError {
    result.expect_err("the edit is refused")
}
