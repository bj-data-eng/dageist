//! What the walk and a run say of the methods that take a pattern, on the claim-timing sample,
//! whose `owner` column holds `Company A` and `Company B`: the expected cells are worked out by
//! hand from the seven rows, and the reasons are Polars' own.

use dageist_engine::allowed::{After, RefusalSource, Representative, Rule};
use dageist_engine::document::{Document, Outcome, Report, RunCause, Segment, StepId, StepRows};
use serde_json::{Value, json};

use crate::claims::{claims, inputs};
use crate::support::{call, document, field_then, named, pattern, step, text};

/// The one-step document that selects `expression` as the column `out`.
fn selecting(expression: Value) -> String {
    document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "named_exprs": named(vec![("out", expression)]) }),
    )])
}

fn report_of(text: &str) -> Report {
    Document::read(text)
        .expect("a document")
        .run_on(&inputs(), &claims())
        .expect("the claims have a schema")
}

/// The owners with one method of a pattern after them.
fn owner_then(id: &str, arguments: Value) -> Value {
    field_then("owner", vec![call(id, arguments)])
}

fn cells(written: &str) -> Vec<Option<String>> {
    Document::read(written)
        .expect("a document")
        .preview(&inputs(), &claims(), &StepId::new("s").expect("an id"), 100)
        .expect("the rows")
        .rows()
        .iter()
        .map(|row| row[0].clone())
        .collect()
}

#[test]
fn a_replacement_by_a_pattern_runs_and_previews_what_is_left_of_each_owner() {
    // Each vowel of `Company A` and `Company B` is an underscore.
    let written = selecting(owner_then(
        "Expr.str.replace_all",
        json!({ "pattern": pattern("[aeiou]"), "value": text("_") }),
    ));
    let report = report_of(&written);
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Valid { .. }),
        "{:?}",
        report.steps()[0].outcome()
    );
    assert!(matches!(report.steps()[0].rows(), StepRows::Counted(_)));
    let expected: Vec<Option<String>> = ["A", "A", "B", "A", "A", "B", "B"]
        .iter()
        .map(|owner| Some(format!("C_mp_ny {owner}")))
        .collect();
    assert_eq!(cells(&written), expected);
}

#[test]
fn a_pattern_that_is_no_regular_expression_is_refused_at_the_pattern_in_polars_words() {
    let written = selecting(owner_then(
        "Expr.str.contains",
        json!({ "pattern": pattern("(Comp") }),
    ));
    // The document reads: a pattern that is half typed is a document still.
    let report = report_of(&written);
    let step = &report.steps()[0];
    let Outcome::Refused(refusal) = step.outcome() else {
        panic!("{:?}", step.outcome());
    };
    // The refusal is at the pattern of the method: the step's expression, its link and its argument.
    assert_eq!(
        refusal.place().path(),
        [
            Segment::Parameter("named_exprs".to_owned()),
            Segment::Value(0),
            Segment::Link(0),
            Segment::Parameter("pattern".to_owned()),
        ]
    );
    assert!(matches!(refusal.cause(), RunCause::Build(_)), "{refusal}");
    let words = refusal.cause().to_string();
    assert!(words.contains("not a valid regular expression"), "{words}");
    assert!(words.contains("unclosed group"), "{words}");
    // The walk says it before the step is built, by the rule, with Polars' words in its own.
    let walked = step.walk().refusal().expect("the walk refuses it");
    assert_eq!(
        walked.refusal().source(),
        RefusalSource::Rule(Rule::PatternIsValid)
    );
    assert!(walked.refusal().reason().contains("unclosed group"));
}

#[test]
fn plain_text_by_the_flag_is_no_pattern_to_be_refused() {
    // `(` is no regular expression, and no owner holds it: nothing matches, and nothing is refused.
    let written = selecting(owner_then(
        "Expr.str.contains",
        json!({ "pattern": pattern("("), "literal": { "flag": true } }),
    ));
    let report = report_of(&written);
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Valid { .. }),
        "{:?}",
        report.steps()[0].outcome()
    );
    assert_eq!(report.steps()[0].walk().refusal(), None);
    assert_eq!(cells(&written), vec![Some("false".to_owned()); 7]);
}

#[test]
fn a_method_started_without_its_pattern_is_refused_at_it_and_not_run() {
    let written = selecting(owner_then("Expr.str.extract", json!({})));
    let report = report_of(&written);
    let Outcome::Refused(refusal) = report.steps()[0].outcome() else {
        panic!("{:?}", report.steps()[0].outcome());
    };
    assert_eq!(
        refusal.place().path().last(),
        Some(&Segment::Parameter("pattern".to_owned()))
    );
}

#[test]
fn the_groups_of_a_pattern_are_the_fields_the_step_reports_and_the_walk_says_no_more_without_it() {
    let written = selecting(owner_then(
        "Expr.str.extract_groups",
        json!({ "pattern": pattern("(?<company>\\w+) (?<letter>\\w)") }),
    ));
    let report = report_of(&written);
    let step = &report.steps()[0];
    assert!(
        matches!(step.outcome(), Outcome::Valid { .. }),
        "{:?}",
        step.outcome()
    );
    // Polars' schema of the expression names the fields of the struct, which the report holds.
    let walked = &step.walk().expressions()[0];
    let standing = walked.calls().last().expect("the call stands");
    let result = standing
        .verdict()
        .and_then(|verdict| verdict.allowance())
        .and_then(|allowance| allowance.result())
        .expect("the walk says what it gives");
    assert_eq!(result.name(), "Struct(company: String, letter: String)");
    assert_eq!(result.class(), Some(Representative::Struct));
    let after: Vec<Option<Representative>> = walked
        .places()
        .iter()
        .map(|place| match place.as_ref()?.after() {
            After::Value { dtype, .. } => Some(dtype),
            _ => None,
        })
        .collect();
    // The call is after the owner's text, and the next link would stand after a struct.
    assert_eq!(
        after,
        [Some(Representative::String), Some(Representative::Struct)]
    );
    // The first owner is `Company A`: the first group is `Company` and the second `A`.
    assert_eq!(
        cells(&written).first().cloned().flatten(),
        Some("{\"Company\",\"A\"}".to_owned())
    );
}

#[test]
fn a_replacement_of_many_values_for_one_text_is_refused_by_the_rule_and_by_polars_for_no_rows() {
    // A text of one value, given a replacement that is a column of many. Polars broadcasts the
    // text over the rows of the table, and refuses it for a table with no rows, where the
    // column has none and the text has one: the tables count the combination refused, as the
    // rule says.
    let written = selecting(json!({ "expression": {
        "start": crate::support::text_call("a_b"),
        "then": [call(
            "Expr.str.replace",
            json!({ "pattern": pattern("_"), "value": crate::support::field("owner") }),
        )],
    } }));
    let report = report_of(&written);
    let walked = report.steps()[0]
        .walk()
        .refusal()
        .expect("the walk refuses it");
    assert_eq!(
        walked.refusal().source(),
        RefusalSource::Rule(Rule::ReplacementValuesMatchInNumber)
    );
    let no_rows = Document::read(&written)
        .expect("a document")
        .run_on(&inputs(), &claims().clear())
        .expect("the claims have a schema");
    let StepRows::Failed(error) = no_rows.steps()[0].rows() else {
        panic!("{:?}", no_rows.steps()[0].rows());
    };
    assert!(
        error
            .to_string()
            .contains("does not match string column length"),
        "{error}"
    );
}
