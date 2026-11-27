//! The forms of a parameter that the catalogue omits, held to the walk: each listed form is
//! refused where a step holds it, by the rule the list names for it, and the refusal gives the
//! Polars issue that deprecates it, so the reason is recorded where the step is refused.

use dageist_engine::allowed::{RefusalSource, Rule};
use dageist_engine::catalog::{self, OmittedForm, omitted_forms};
use dageist_engine::document::{Document, Inputs, Walk};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, document, field, field_then, many, step};

fn table() -> DataFrame {
    df!("a" => [1_i64, 2, 3], "b" => [2_i64, 3, 4]).expect("the table")
}

/// A step that holds `form`, so that the walk has the form to refuse. A form the test is not
/// taught to build is a failure that names it, for the list is held to the walk.
fn step_holding(form: &OmittedForm) -> Value {
    match (form.entry(), form.parameter()) {
        // A column looked up in a column of its own dtype.
        ("Expr.is_in", "other") => step(
            "s",
            "LazyFrame.select",
            json!({ "exprs": many(vec![field_then(
                "a",
                vec![call("Expr.is_in", json!({ "other": field("b") }))],
            )]) }),
        ),
        (entry, parameter) => panic!(
            "teach this test to build a step that holds the omitted form of {parameter} of {entry}"
        ),
    }
}

fn walk_of(form: &OmittedForm) -> Walk {
    Document::read(&document(vec![step_holding(form)]))
        .expect("a document")
        .steps()[0]
        .walk(0, table().schema(), &Inputs::new())
}

#[test]
fn each_omitted_form_is_of_a_parameter_of_an_entry_the_catalogue_has_and_is_listed_once_in_order() {
    let forms = omitted_forms();
    assert!(!forms.is_empty(), "the list holds the form of is_in");
    for form in forms {
        let entry = catalog::entry(form.entry())
            .unwrap_or_else(|| panic!("{} is no entry of the catalogue", form.entry()));
        assert!(
            entry
                .parameters()
                .iter()
                .any(|parameter| parameter.name() == form.parameter()),
            "{} has no parameter {}",
            form.entry(),
            form.parameter()
        );
        assert!(
            Rule::from_name(form.rule()).is_some(),
            "{} is no rule the engine holds",
            form.rule()
        );
        assert!(!form.form().is_empty() && !form.deprecation().is_empty());
        assert!(form.issue().contains('#'), "{}", form.issue());
    }
    assert!(
        forms
            .windows(2)
            .all(|pair| (pair[0].entry(), pair[0].parameter())
                < (pair[1].entry(), pair[1].parameter())),
        "the forms are ordered by entry and parameter, and none is listed twice"
    );
}

#[test]
fn a_step_that_holds_an_omitted_form_is_refused_by_its_rule_with_the_issue_that_deprecates_it() {
    for form in omitted_forms() {
        let walk = walk_of(form);
        let refusal = walk
            .refusal()
            .unwrap_or_else(|| {
                panic!(
                    "{} {} given {} is not refused",
                    form.entry(),
                    form.parameter(),
                    form.form()
                )
            })
            .refusal();
        match refusal.source() {
            RefusalSource::Rule(rule) => assert_eq!(rule.name(), form.rule(), "{}", form.entry()),
            other => panic!("{} is refused by {other:?}", form.entry()),
        }
        assert!(
            refusal.reason().contains(form.issue()),
            "the refusal of {} does not name {}: {}",
            form.entry(),
            form.issue(),
            refusal.reason()
        );
    }
}

#[test]
fn the_omitted_form_of_is_in_names_the_bug_that_prompted_the_omission() {
    let form = omitted_forms()
        .iter()
        .find(|form| form.entry() == "Expr.is_in" && form.parameter() == "other")
        .expect("the form of is_in is listed");
    assert_eq!(form.issue(), "pola-rs/polars#22149");
    assert_eq!(form.rule(), "is-in-takes-a-list");
    let bug = form.bug().expect("the form has a bug");
    assert!(
        bug.contains("pl.lit(5).is_in(pl.lit(1))") && bug.contains("panics"),
        "{bug}"
    );
}
