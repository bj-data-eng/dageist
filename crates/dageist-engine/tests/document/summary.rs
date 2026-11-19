//! A step said in a few words. Each expected text is the rule of the step
//! editor's summary, worked out by hand: a head is followed by what the step
//! works on.

use dageist_engine::catalog::english;
use dageist_engine::claim_timing::ClaimTiming;
use dageist_engine::document::Document;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, field_then, many, named, step, text, whole};

fn summary(step_json: Value) -> String {
    let document = Document::read(&document(vec![step_json])).expect("a document");
    document.steps()[0].summary(english())
}

fn compare(entry: &str, column: &str, other: Value) -> Value {
    field_then(column, vec![call(entry, json!({ "other": other }))])
}

fn filter(predicates: Vec<Value>) -> String {
    summary(step(
        "s",
        "LazyFrame.filter",
        json!({ "predicates": many(predicates) }),
    ))
}

#[test]
fn the_samples_steps_are_summarised_as_the_editor_summarises_them() {
    let sample = ClaimTiming::new().expect("the sample node");
    let summaries: Vec<String> = sample
        .steps()
        .steps()
        .iter()
        .map(|step| step.summary(english()))
        .collect();
    assert_eq!(
        summaries,
        [
            "by dcn, timestamp",
            "step_index",
            "resolved_at",
            "received_at",
            "resolved_day",
            "days_open",
            "a_days",
            "step_sla on step_to",
            "sla_status",
            "flag",
            "holiday_count",
            "on step_to",
        ]
    );
}

#[test]
fn a_filter_by_one_comparison_is_that_comparison_with_its_sign() {
    assert_eq!(
        filter(vec![compare("Expr.eq", "owner", text("Company A"))]),
        "owner == \"Company A\""
    );
    assert_eq!(filter(vec![compare("Expr.ne", "a", whole(3))]), "a ≠ 3");
    assert_eq!(filter(vec![compare("Expr.gt", "a", field("b"))]), "a > b");
    assert_eq!(filter(vec![compare("Expr.ge", "a", whole(-1))]), "a ≥ -1");
    assert_eq!(
        filter(vec![compare(
            "Expr.lt",
            "a",
            json!({ "expression": { "start": { "input": "cutoff" }, "then": [] } })
        )]),
        "a < cutoff"
    );
    assert_eq!(
        filter(vec![compare(
            "Expr.le",
            "a",
            chain(
                call("polars.lit", json!({ "value": { "float": 2.0 } })),
                vec![]
            )
        )]),
        "a ≤ 2.0"
    );
}

#[test]
fn a_filter_by_more_says_how_many_must_hold_and_by_nothing_says_a_condition_is_to_be_picked() {
    let one = || compare("Expr.eq", "a", whole(1));
    assert_eq!(filter(vec![one(), one(), one()]), "all of 3 conditions");
    assert_eq!(filter(vec![]), "condition");
    // A constraint is a field equal to a value; with a predicate it is a second condition.
    let constrained = step(
        "s",
        "LazyFrame.filter",
        json!({
            "predicates": many(vec![one()]),
            "constraints": named(vec![("b", text("x"))]),
        }),
    );
    assert_eq!(summary(constrained), "all of 2 conditions");
    let only = step(
        "s",
        "LazyFrame.filter",
        json!({ "constraints": named(vec![("b", text("x"))]) }),
    );
    assert_eq!(summary(only), "b == \"x\"");
}

#[test]
fn what_is_worked_out_or_not_yet_picked_is_said_so() {
    // A comparison of a worked-out field, a chain, and a hole.
    let worked = field_then("a", vec![call("Expr.is_null", json!({}))]);
    assert_eq!(filter(vec![worked]), "…");
    let shifted = compare(
        "Expr.gt",
        "a",
        field_then("b", vec![call("Expr.is_null", json!({}))]),
    );
    assert_eq!(filter(vec![shifted]), "a > …");
    let hole = json!({ "expression": { "start": { "hole": null }, "then": [
        call("Expr.eq", json!({ "other": { "expression": { "start": { "hole": null }, "then": [] } } })),
    ] } });
    assert_eq!(filter(vec![hole]), "field == value");
}

#[test]
fn a_list_of_fields_names_up_to_three_and_otherwise_counts_them() {
    let select = |names: &[&str]| {
        summary(step(
            "s",
            "LazyFrame.select",
            json!({ "exprs": many(names.iter().map(|name| field(name)).collect()) }),
        ))
    };
    assert_eq!(select(&[]), "no fields");
    assert_eq!(select(&["a"]), "a");
    assert_eq!(select(&["a", "b", "c"]), "a, b, c");
    assert_eq!(select(&["a", "b", "c", "d"]), "4 fields");
    // What is worked out is `…`.
    let worked = step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![field("a"), field_then("b", vec![call("Expr.sum", json!({}))])]) }),
    );
    assert_eq!(summary(worked), "a, …");
    let sort = |names: &[&str]| {
        summary(step(
            "s",
            "LazyFrame.sort",
            json!({ "by": many(names.iter().map(|name| field(name)).collect()) }),
        ))
    };
    assert_eq!(sort(&["dcn", "timestamp"]), "by dcn, timestamp");
    assert_eq!(sort(&["a", "b", "c", "d", "e"]), "by 5 fields");
}

#[test]
fn a_rename_shows_up_to_two_renames_and_otherwise_the_names_renamed() {
    let rename = |pairs: &[(&str, &str)]| {
        summary(step(
            "s",
            "LazyFrame.rename",
            json!({ "mapping": { "pairs": pairs
                .iter()
                .map(|(from, to)| json!([{ "columns": [from] }, { "string": to }]))
                .collect::<Vec<_>>() } }),
        ))
    };
    assert_eq!(rename(&[("a", "b")]), "a → b");
    assert_eq!(rename(&[("a", "b"), ("c", "d")]), "a → b, c → d");
    assert_eq!(rename(&[("a", "b"), ("c", "d"), ("e", "f")]), "a, c, e");
    assert_eq!(rename(&[]), "no fields");
}

#[test]
fn a_join_and_a_pivot_say_what_they_match_and_what_they_turn_into_columns() {
    let join = step(
        "s",
        "LazyFrame.join",
        json!({
            "other": { "table": "lookup" },
            "left_on": many(vec![field("a"), field("b")]),
            "right_on": many(vec![field("x"), field("y")]),
        }),
    );
    assert_eq!(summary(join), "lookup on a, b");
    let on = step(
        "s",
        "LazyFrame.join",
        json!({ "other": { "table": "lookup" }, "on": many(vec![field("k")]) }),
    );
    assert_eq!(summary(on), "lookup on k");
    let bare = step(
        "s",
        "LazyFrame.join",
        json!({ "other": { "table": "lookup" } }),
    );
    assert_eq!(summary(bare), "lookup");
    let pivot = step(
        "s",
        "LazyFrame.pivot",
        json!({
            "on": { "columns": ["step_to"] },
            "on_columns": { "table": "names" },
            "values": { "columns": ["v"] },
        }),
    );
    assert_eq!(summary(pivot), "on step_to");
}

#[test]
fn a_column_that_is_added_is_named_and_several_are_listed() {
    let added = |names: &[&str]| {
        summary(step(
            "s",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(names.iter().map(|name| (*name, whole(1))).collect()) }),
        ))
    };
    assert_eq!(added(&["new_column"]), "new_column");
    assert_eq!(added(&["a", "b"]), "a, b");
    assert_eq!(added(&["a", "b", "c", "d"]), "4 fields");
}
#[test]
fn a_step_report_carries_the_summary_of_its_step() {
    let sample = ClaimTiming::new().expect("the sample node");
    let report = sample.run(sample.steps()).expect("a run");
    for (reported, step) in report.steps().iter().zip(sample.steps().steps()) {
        assert_eq!(
            reported.summary(),
            step.summary(dageist_engine::catalog::english())
        );
    }
    assert_eq!(report.steps()[0].summary(), "by dcn, timestamp");
}

#[test]
fn a_typed_value_is_written_as_python_writes_it() {
    // Worked out by hand: text is quoted with its quotation marks, backslashes and control
    // characters escaped; a flag is `True`, `False` or `None`; a whole float keeps its `.0`.
    let literal = |value: Value| chain(call("polars.lit", json!({ "value": value })), vec![]);
    let summarised = |value: Value| {
        summary(step(
            "s",
            "LazyFrame.select",
            json!({ "exprs": many(vec![literal(value)]) }),
        ))
    };
    assert_eq!(
        summarised(json!({ "string": "say \"hi\"\n\\ é\t\u{1}" })),
        "\"say \\\"hi\\\"\\n\\\\ é\\t\\x01\""
    );
    assert_eq!(summarised(json!({ "integer": -3 })), "-3");
    assert_eq!(summarised(json!({ "float": 1.0 })), "1.0");
    assert_eq!(summarised(json!({ "float": 1e20 })), "1e+20");
    assert_eq!(summarised(json!({ "flag": true })), "True");
    assert_eq!(summarised(json!({ "flag": false })), "False");
    assert_eq!(summarised(json!({ "flag": null })), "None");
}

#[test]
fn a_name_or_a_value_that_looks_like_a_place_is_written_as_it_is() {
    // The summary of a comparison is filled in one pass: a field called {value} and a text
    // called {field} are not filled again.
    assert_eq!(
        filter(vec![compare("Expr.eq", "{value}", text("{field}"))]),
        "{value} == \"{field}\""
    );
    // The same of a join, whose table and fields are filled together.
    let join = step(
        "s",
        "LazyFrame.join",
        json!({
            "other": { "table": "{fields}" },
            "on": many(vec![field("{table}")]),
        }),
    );
    assert_eq!(summary(join), "{fields} on {table}");
}

#[test]
fn the_names_of_a_list_are_separated_as_the_language_file_says() {
    let file = include_str!("../../src/catalog/en.toml").replacen(
        "separator = \", \"",
        "separator = \" + \"",
        1,
    );
    let words = dageist_engine::catalog::Words::parse(&file).expect("a language file");
    let sort = Document::read(&document(vec![step(
        "s",
        "LazyFrame.sort",
        json!({ "by": many(vec![field("dcn"), field("timestamp")]) }),
    )]))
    .expect("a document");
    assert_eq!(sort.steps()[0].summary(&words), "by dcn + timestamp");
}
