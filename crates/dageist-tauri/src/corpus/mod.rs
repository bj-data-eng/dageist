//! A corpus of step documents, with the report the host gives for each, written for the view's
//! tests.
//!
//! The view's grid test runs the model for every place the engine allows an entry at, one piece
//! at a time. This holds whole documents, each made by the engine's own edits and run by the
//! engine, so that the view can rebuild every piece of every one from the offers the model makes
//! at its place. See [the explanation](../../../../docs/explanation.md#the-corpus-of-step-documents)
//! for what it holds and how its coverage is made certain.
//!
//! The module is part of the host's tests only: `main.rs` compiles it under `cfg(test)`.

mod arguments;
mod chains;
mod documents;
mod facts;
mod grid;
mod parameters;
mod reach;
mod refused;
mod source;
mod steps;
mod tests;

use dageist_engine::document::Document;

/// One document of the corpus, with the name that says what it is for.
pub(crate) struct Case {
    pub name: String,
    pub document: Document,
}

/// One document of the corpus with what the host reports of it.
pub(crate) struct Reported {
    pub name: String,
    pub model: Document,
    pub document: serde_json::Value,
    pub run: serde_json::Value,
}

/// A file of the corpus: its name, and the documents it holds.
pub(crate) struct Part {
    pub file: &'static str,
    pub documents: Vec<Reported>,
}

/// Sets the number of every temporary column of Polars that a text names to 0. Polars numbers the
/// temporary columns it makes by how many it has made in the process, so a reason that quotes one,
/// such as the plan a failure on the rows prints, would otherwise differ with what else the process
/// has run, and the files would differ from one run to the next.
fn steady(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(text) => {
            let mut steadied = String::with_capacity(text.len());
            let mut rest = text.as_str();
            while let Some(at) = rest.find("_POLARS_") {
                let (before, from) = rest.split_at(at);
                steadied.push_str(before);
                let name_len = from
                    .find(|letter: char| !(letter.is_ascii_alphanumeric() || letter == '_'))
                    .unwrap_or(from.len());
                let (name, after) = from.split_at(name_len);
                let stem = name.trim_end_matches(|letter: char| letter.is_ascii_digit());
                steadied.push_str(stem);
                if stem.len() < name.len() {
                    steadied.push('0');
                }
                rest = after;
            }
            steadied.push_str(rest);
            *text = steadied;
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(steady),
        serde_json::Value::Object(fields) => fields.values_mut().for_each(steady),
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

/// Runs each case on the corpus node and reports it as the host does.
fn reported(cases: Vec<Case>) -> Vec<Reported> {
    cases
        .into_iter()
        .map(|case| Reported {
            model: case.document.clone(),
            run: {
                let mut run =
                    serde_json::to_value(source::run(&case.document)).expect("a report serializes");
                steady(&mut run);
                run
            },
            document: serde_json::to_value(crate::document::document_view(&case.document))
                .expect("a document serializes"),
            name: case.name,
        })
        .collect()
}

/// The corpus, in the files it is written to.
pub(crate) fn corpus() -> Vec<Part> {
    let mut planner = chains::Planner::new(grid::grid());
    let mut coverage = chains::Coverage::default();
    let mut starts = chains::start_cases(&mut planner);
    let (mut methods, functions) = chains::entry_cases(&mut planner, None, &mut coverage);
    methods.extend(chains::context_method_cases(&mut planner));
    starts.extend(functions);
    starts.extend(chains::condition_cases(&planner));
    starts.extend(chains::context_cases(&planner));
    let parameters = chains::parameter_cases(&mut planner, &mut coverage);
    let mut refused = refused::cause_cases();
    refused.extend(refused::call_cases(&mut planner));
    refused.extend(refused::rule_cases(&mut planner));
    let parts = vec![
        Part {
            file: "corpus-starts.json",
            documents: reported(starts),
        },
        Part {
            file: "corpus-methods.json",
            documents: reported(methods),
        },
        Part {
            file: "corpus-parameters.json",
            documents: reported(parameters),
        },
        Part {
            file: "corpus-steps.json",
            documents: reported(steps::step_cases()),
        },
        Part {
            file: "corpus-refused.json",
            documents: reported(refused),
        },
    ];
    let mut names = std::collections::BTreeSet::new();
    for part in &parts {
        for held in &part.documents {
            assert!(
                names.insert(held.name.clone()),
                "{} is made twice",
                held.name
            );
        }
    }
    parts
}
