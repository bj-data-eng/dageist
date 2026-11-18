//! The Python Polars source the engine writes for a document, checked two ways.
//!
//! The structural check runs always: every call the source makes names an item
//! Python Polars has, as `api/polars/python.txt` traces it, with parameters it
//! has and arguments of the kind the catalogue gives them. The execution check
//! runs Python Polars on the source and compares its table with the engine's
//! own run of the document; it needs a Python with Polars, so it is ignored
//! unless asked for, with `DAGEIST_PYTHON` naming the Python.

mod cases;
mod execute;
mod fixtures;
mod structure;

use cases::cases;

#[test]
#[ignore = "runs Python Polars: set DAGEIST_PYTHON to a Python that has it"]
fn python_polars_gives_what_the_engine_gives_for_the_source() {
    let python = execute::python();
    let failures = execute::all_in_python(&python, &cases());
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

use std::collections::BTreeSet;

use dageist_engine::catalog::entries;
use dageist_engine::document::{Call, Start, Value};

fn entries_in_call(call: &Call, used: &mut BTreeSet<&'static str>) {
    used.insert(call.entry().id().as_str());
    for given in call.arguments() {
        entries_in_value(given.value(), used);
    }
}

fn entries_in_value(value: &Value, used: &mut BTreeSet<&'static str>) {
    match value {
        Value::Expression(expression) => {
            if let Start::Call(start) = expression.start() {
                entries_in_call(start, used);
            }
            for link in expression.links() {
                entries_in_call(link, used);
            }
        }
        Value::Many(items) => items.iter().for_each(|item| entries_in_value(item, used)),
        Value::Pairs(pairs) => pairs.iter().for_each(|(key, entry)| {
            entries_in_value(key, used);
            entries_in_value(entry, used);
        }),
        _ => {}
    }
}

#[test]
fn the_cases_between_them_use_every_entry_of_the_catalogue() {
    let mut used = BTreeSet::new();
    for case in cases() {
        for step in case.document().steps() {
            entries_in_call(step.call(), &mut used);
        }
    }
    let missing: Vec<&str> = entries()
        .iter()
        .map(|entry| entry.id().as_str())
        .filter(|id| !used.contains(id))
        .collect();
    assert_eq!(missing, Vec::<&str>::new(), "entries no case uses");
}

#[test]
fn every_call_in_the_source_of_every_case_is_python_polars_as_the_inventory_traces_it() {
    for case in cases() {
        let fixture = case.fixture;
        let source = case
            .document()
            .source(fixture.frame_name, &fixture.inputs());
        assert_eq!(source.not_runnable(), [], "{}", case.name);
        structure::check_module(&source).unwrap_or_else(|why| panic!("{}: {why}", case.name));
    }
}

#[test]
fn the_structural_check_refuses_what_python_polars_does_not_have() {
    let refused = |step: &str| structure::check_step(step).expect_err(step);
    // An item the inventory does not trace.
    assert!(refused(".sorted(\"a\")").contains("not an item"));
    // A parameter the item does not have, by name and as one more argument by position.
    assert!(refused(".sort(\"a\", ascending=True)").contains("no parameter ascending"));
    assert!(refused(".rename({\"a\": \"b\"}, False)").contains("by position"));
    // A required parameter left out, and one given twice.
    assert!(refused(".join()").contains("lacks other"));
    assert!(refused(".rename({\"a\": \"b\"}, mapping={})").contains("twice"));
    // An argument of the wrong kind, and a choice the parameter lacks.
    assert!(refused(".sort(\"a\", descending=\"yes\")").contains("takes"));
    assert!(refused(".join(other, how=\"sideways\")").contains("not a choice"));
    assert!(refused(".with_columns(pl.col(\"a\").shift(m=1))").contains("no parameter m"));
    assert!(refused(".with_columns(pl.col(\"a\").sum().shifted())").contains("not an item"));
    // A keyword-only parameter given by position.
    assert!(refused(".with_columns(pl.col(\"a\").cum_sum(True))").contains("by position"));
    // What does hold.
    structure::check_step(".sort(\"a\", \"b\", descending=[True, False])").expect("a sort");
    structure::check_step(".with_columns(pl.col(\"a\").shift(pl.lit(1)).alias(\"b\"))")
        .expect("a shift");
}
