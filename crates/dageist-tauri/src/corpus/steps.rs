//! The documents of steps: each step the catalogue has, as the editor adds it and with what it
//! needs, and each parameter of it given.

use dageist_engine::allowed::Outer;
use dageist_engine::catalog::{Applies, Build, StepFrame};
use dageist_engine::document::{Document, Place, Segment};

use super::Case;
use super::arguments::{give, sample_for, with_required};
use super::documents::{call_at, filled};
use super::source::{ON_COLUMNS, OTHER, column_of};

/// The columns a new step is given to start from, and the tables it can bring in.
fn frame_columns() -> Vec<String> {
    use dageist_engine::allowed::Representative::{Boolean, Int64, String};
    [Int64, String, Boolean].map(column_of).to_vec()
}

/// The document with the step `id` added as the editor adds it: what the engine starts it with,
/// and nothing given that it leaves to be picked.
pub(super) fn added(id: &str) -> Document {
    let columns = frame_columns();
    let tables = vec![OTHER.to_owned(), ON_COLUMNS.to_owned()];
    Document::new(Vec::new())
        .expect("no steps")
        .add_step(
            0,
            id,
            &StepFrame {
                columns: &columns,
                tables: &tables,
            },
        )
        .unwrap_or_else(|error| panic!("{id} is not added: {error}"))
}

/// The place of the only step of `document`.
pub(super) fn step_place(document: &Document) -> Place {
    Place::of_step(0, Some(document.steps()[0].id().clone()))
}

/// The documents of every step of the catalogue: each as it is added, with what it needs, and with
/// each parameter the engine offers given, on its own.
pub(super) fn step_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for entry in dageist_engine::catalog::entries() {
        if entry.applies() != Applies::Frame {
            continue;
        }
        let id = entry.id().as_str();
        let base = added(id);
        let place = step_place(&base);
        cases.push(Case {
            name: format!("{id} as it is added"),
            document: base.clone(),
        });
        let mut needed = with_required(&base, &place, Outer::Select);
        // A pivot is of the values of a column, grouped by another: it needs one of the two.
        if id == "LazyFrame.pivot" {
            let index = sample_for(
                entry
                    .parameters()
                    .iter()
                    .find(|held| held.name() == "index")
                    .expect("pivot has index"),
                Outer::Select,
            );
            needed = give(&needed, &place, "index", index);
        }
        let needed = filled(&needed);
        cases.push(Case {
            name: format!("{id} with what it needs"),
            document: needed.clone(),
        });
        for parameter in entry.parameters().iter().filter(|held| held.offered()) {
            let step = &needed.steps()[0];
            if step.call().argument(parameter.name()).is_some() {
                continue;
            }
            let value = sample_for(parameter, Outer::Select);
            let document = filled(&give(&needed, &place, parameter.name(), value));
            cases.push(Case {
                name: format!("{id} given {}", parameter.name()),
                document,
            });
        }
    }
    cases.extend(group_call_cases());
    cases
}

/// The documents of every call of a group by: each as it is put into a step that groups, with what
/// it needs, and with each parameter the engine offers given, on its own.
fn group_call_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for entry in dageist_engine::catalog::entries() {
        if entry.applies() != Applies::Grouped {
            continue;
        }
        let id = entry.id().as_str();
        let base = added("LazyFrame.group_by");
        let step = step_place(&base);
        // A `having` goes before the call that finishes the grouping, and any other call takes its
        // place.
        let (document, at) = if matches!(entry.build(), Build::GroupedStep(_)) {
            let made = base
                .insert_group_call(&step, 0, id)
                .unwrap_or_else(|error| panic!("{id} is not added: {error}"));
            (made, 0)
        } else {
            let made = base
                .swap_group_call(&step.clone().then(Segment::Link(0)), id)
                .unwrap_or_else(|error| panic!("{id} is not swapped in: {error}"));
            (made, 0)
        };
        let place = step.clone().then(Segment::Link(at));
        cases.push(Case {
            name: format!("{id} as it is put into a group by"),
            document: filled(&document),
        });
        let needed = filled(&with_required(&document, &place, Outer::Aggregate));
        cases.push(Case {
            name: format!("{id} with what it needs"),
            document: needed.clone(),
        });
        for parameter in entry.parameters().iter().filter(|held| held.offered()) {
            if call_at(&needed, &place)
                .argument(parameter.name())
                .is_some()
            {
                continue;
            }
            let outer = if id == "LazyGroupBy.having" {
                Outer::Having
            } else {
                Outer::Aggregate
            };
            let value = sample_for(parameter, outer);
            cases.push(Case {
                name: format!("{id} given {}", parameter.name()),
                document: filled(&give(&needed, &place, parameter.name(), value)),
            });
        }
    }
    cases
}
