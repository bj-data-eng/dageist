//! What the walk and a run say of the methods of the `dt` namespace that take a duration string,
//! on the claim-timing sample, whose `timestamp` column holds seven moments of March 2026: the
//! expected cells are worked out by hand from them, and the reasons are Polars' own.

use dageist_engine::allowed::{RefusalSource, Representative};
use dageist_engine::document::{Document, Outcome, Report, RunCause, Segment, StepId, StepRows};
use serde_json::{Value, json};

use crate::claims::{claims, inputs};
use crate::support::{call, document, field_then, named, step, text};

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

/// The moments with one method of the `dt` namespace after them.
fn moments_then(id: &str, arguments: Value) -> Value {
    field_then("timestamp", vec![call(id, arguments)])
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
fn moments_are_cut_to_the_start_of_their_day_and_rounded_to_the_nearest_hour() {
    let day = selecting(moments_then(
        "Expr.dt.truncate",
        json!({ "every": text("1d") }),
    ));
    let report = report_of(&day);
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Valid { .. }),
        "{:?}",
        report.steps()[0].outcome()
    );
    assert!(matches!(report.steps()[0].rows(), StepRows::Counted(_)));
    let days = [
        "2026-03-02",
        "2026-03-05",
        "2026-03-06",
        "2026-03-02",
        "2026-03-04",
        "2026-03-09",
        "2026-03-03",
    ];
    let expected: Vec<Option<String>> = days
        .iter()
        .map(|day| Some(format!("{day} 00:00:00")))
        .collect();
    assert_eq!(cells(&day), expected);

    let hour = selecting(moments_then(
        "Expr.dt.round",
        json!({ "every": text("1h") }),
    ));
    // 13:30 rounds up, 07:50 up, 10:00 stays, 09:15 down, 17:20 down, 11:05 down, 16:40 up.
    let hours = [
        "2026-03-02 14:00:00",
        "2026-03-05 08:00:00",
        "2026-03-06 10:00:00",
        "2026-03-02 09:00:00",
        "2026-03-04 17:00:00",
        "2026-03-09 11:00:00",
        "2026-03-03 17:00:00",
    ];
    let expected: Vec<Option<String>> = hours.iter().map(|hour| Some((*hour).to_owned())).collect();
    assert_eq!(cells(&hour), expected);
}

#[test]
fn a_duration_typed_as_no_duration_is_refused_at_the_parameter_in_polars_words() {
    for (id, parameter) in [
        ("Expr.dt.truncate", "every"),
        ("Expr.dt.round", "every"),
        ("Expr.dt.offset_by", "by"),
    ] {
        let written = selecting(moments_then(id, json!({ parameter: text("1x") })));
        let report = report_of(&written);
        let step = &report.steps()[0];
        let Outcome::Refused(refusal) = step.outcome() else {
            panic!("{id}: {:?}", step.outcome());
        };
        // The refusal is at the duration of the method: the step's expression, its link and its
        // argument.
        assert_eq!(
            refusal.place().path(),
            [
                Segment::Parameter("named_exprs".to_owned()),
                Segment::Value(0),
                Segment::Link(0),
                Segment::Parameter(parameter.to_owned()),
            ],
            "{id}"
        );
        assert!(matches!(refusal.cause(), RunCause::Build(_)), "{refusal}");
        let words = refusal.cause().to_string();
        assert!(words.contains("not a duration"), "{id}: {words}");
        assert!(
            words.contains("unit: 'x' not supported; available units are"),
            "{id}: {words}"
        );
        // The walk says it before the step is built, by the rule, with Polars' words in its own.
        let walked = step.walk().refusal().expect("the walk refuses it");
        assert!(
            matches!(walked.refusal().source(), RefusalSource::Rule(rule)
                if rule.name() == "duration-is-valid"),
            "{id}: {:?}",
            walked.refusal().source()
        );
        assert!(
            walked
                .refusal()
                .reason()
                .contains("unit: 'x' not supported"),
            "{id}"
        );
    }
}

#[test]
fn a_duration_that_is_a_column_is_polars_to_refuse_when_it_runs_the_rows() {
    // The owners are text and no duration, which Polars reads when the rows run and refuses.
    let written = selecting(moments_then(
        "Expr.dt.truncate",
        json!({ "every": crate::support::field("owner") }),
    ));
    let report = report_of(&written);
    let step = &report.steps()[0];
    assert_eq!(step.walk().refusal(), None);
    assert!(
        matches!(step.rows(), StepRows::Failed(_)),
        "{:?}",
        step.rows()
    );
}

#[test]
fn a_cut_moment_is_a_moment_and_the_walk_says_so() {
    let written = selecting(moments_then(
        "Expr.dt.truncate",
        json!({ "every": text("1mo") }),
    ));
    let report = report_of(&written);
    let walked = &report.steps()[0].walk().expressions()[0];
    let standing = walked.calls().last().expect("the call stands");
    let result = standing
        .verdict()
        .and_then(|verdict| verdict.allowance())
        .and_then(|allowance| allowance.result())
        .expect("the walk says what it gives");
    assert_eq!(result.class(), Some(Representative::Datetime));
}
