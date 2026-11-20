//! What the walk and a run say of the methods of text, on the claim-timing sample, whose
//! `owner` column holds `Company A` and `Company B`, and against what Polars does: the expected
//! cells are worked out by hand from the seven rows.

use dageist_engine::allowed::{After, RefusalSource, Representative, Stage};
use dageist_engine::document::{Document, Outcome, RunCause, Segment, StepId, StepRows};
use serde_json::{Value, json};

use crate::claims::{claims, inputs};
use crate::support::{call, document, field, field_then, named, step, text, whole};

/// The one-step document that selects `expression` as the column `out`.
fn selecting(expression: Value) -> String {
    document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "named_exprs": named(vec![("out", expression)]) }),
    )])
}

fn report_of(text: &str) -> dageist_engine::document::Report {
    Document::read(text)
        .expect("a document")
        .run_on(&inputs(), &claims())
        .expect("the claims have a schema")
}

#[test]
fn a_chain_of_text_methods_runs_and_previews_what_is_left_of_each_owner() {
    // `Company A` and `Company B` in capitals, with every character of `COMPANY ` taken off both
    // ends: `A` goes entirely, and `B` stays, so the lengths are 0 and 1.
    let kept = field_then(
        "owner",
        vec![
            call("Expr.str.to_uppercase", json!({})),
            call(
                "Expr.str.strip_chars",
                json!({ "characters": text("COMPANY ") }),
            ),
            call("Expr.str.len_chars", json!({})),
        ],
    );
    let written = selecting(kept);
    let report = report_of(&written);
    let step = &report.steps()[0];
    assert!(
        matches!(step.outcome(), Outcome::Valid { .. }),
        "{:?}",
        step.outcome()
    );
    // The dtype where each link stands is the one Polars documents: the owner's text before the
    // first two, the text they give before the count, and a count after it.
    let expression = step
        .walk()
        .expressions()
        .iter()
        .find(|expression| {
            expression.place().path()
                == [
                    Segment::Parameter("named_exprs".to_owned()),
                    Segment::Value(0),
                ]
        })
        .expect("the walk holds the chain");
    let after: Vec<Option<Representative>> = expression
        .places()
        .iter()
        .map(|place| match place.as_ref()?.after() {
            After::Value { dtype, .. } => Some(dtype),
            _ => None,
        })
        .collect();
    assert_eq!(
        after,
        [
            Some(Representative::String),
            Some(Representative::String),
            Some(Representative::String),
            Some(Representative::UInt32)
        ]
    );
    let preview = Document::read(&written)
        .expect("a document")
        .preview(&inputs(), &claims(), &StepId::new("s").expect("an id"), 100)
        .expect("the rows");
    assert_eq!(
        preview
            .columns()
            .iter()
            .map(|column| (column.name(), column.dtype()))
            .collect::<Vec<_>>(),
        [("out", "UInt32")]
    );
    let cells: Vec<Option<&str>> = preview.rows().iter().map(|row| row[0].as_deref()).collect();
    assert_eq!(
        cells,
        [
            Some("0"),
            Some("0"),
            Some("1"),
            Some("0"),
            Some("0"),
            Some("1"),
            Some("1")
        ]
    );
}

#[test]
fn a_method_of_text_after_whole_numbers_is_refused_at_its_link_as_polars_refuses_it() {
    let upper = field_then("row_days", vec![call("Expr.str.to_uppercase", json!({}))]);
    let report = report_of(&selecting(upper));
    let step = &report.steps()[0];
    let refusal = step.walk().refusal().expect("the walk refuses it");
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Polars(Stage::Rows)
    );
    assert!(
        refusal.refusal().reason().contains("expected `String`"),
        "{}",
        refusal.refusal().reason()
    );
    // Polars refuses the rows of the same step, as the walk says it does.
    assert!(
        matches!(step.rows(), StepRows::Failed(_)),
        "{:?}",
        step.rows()
    );
}

#[test]
fn a_split_by_a_pattern_is_refused_before_polars_is_asked_and_one_by_text_is_not() {
    let split = |literal: Option<bool>| {
        let mut arguments = json!({ "by": text(" ") });
        if let Some(literal) = literal {
            arguments["literal"] = json!({ "flag": literal });
        }
        field_then("owner", vec![call("Expr.str.split", arguments)])
    };
    let by_text = report_of(&selecting(split(Some(true))));
    assert!(matches!(
        by_text.steps()[0].outcome(),
        Outcome::Valid { .. }
    ));
    let by_default = report_of(&selecting(split(None)));
    assert!(matches!(
        by_default.steps()[0].outcome(),
        Outcome::Valid { .. }
    ));
    let by_pattern = report_of(&selecting(split(Some(false))));
    let Outcome::Refused(refusal) = by_pattern.steps()[0].outcome() else {
        panic!("{:?}", by_pattern.steps()[0].outcome());
    };
    assert!(
        matches!(refusal.cause(), RunCause::Polars(_) | RunCause::Build(_)),
        "{refusal}"
    );
    assert!(refusal.to_string().contains("pattern"), "{refusal}");
}

#[test]
fn text_joined_into_one_value_is_an_aggregate_beside_a_column_in_a_select() {
    // The two owners joined by a dash, beside the owner of each row: one value and seven.
    let joined = field_then(
        "owner",
        vec![call(
            "Expr.str.join",
            json!({ "delimiter": { "string": "-" } }),
        )],
    );
    let kept = field("owner");
    let written = document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "named_exprs": named(vec![("joined", joined), ("owner", kept)]) }),
    )]);
    let report = report_of(&written);
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Valid { .. }),
        "{:?}",
        report.steps()[0].outcome()
    );
    // Polars broadcasts the one value, as the preview of the rows shows.
    let preview = Document::read(&written)
        .expect("a document")
        .preview(&inputs(), &claims(), &StepId::new("s").expect("an id"), 100)
        .expect("the rows");
    assert_eq!(preview.rows().len(), 7);
    let first = preview.rows()[0][0].as_deref().expect("a value");
    assert_eq!(
        first,
        "Company A-Company A-Company B-Company A-Company A-Company B-Company B"
    );
    assert!(
        preview
            .rows()
            .iter()
            .all(|row| row[0].as_deref() == Some(first))
    );
}

#[test]
fn a_test_for_a_prefix_is_a_condition_a_filter_may_hold() {
    let starts = field_then(
        "owner",
        vec![call(
            "Expr.str.starts_with",
            json!({ "prefix": text("Company A") }),
        )],
    );
    let written = document(vec![step(
        "keep",
        "LazyFrame.filter",
        json!({ "predicates": crate::support::many(vec![starts]) }),
    )]);
    let preview = Document::read(&written)
        .expect("a document")
        .preview(
            &inputs(),
            &claims(),
            &StepId::new("keep").expect("an id"),
            100,
        )
        .expect("the rows");
    // Four of the seven claims are with Company A.
    assert_eq!(preview.rows().len(), 4);
}

#[test]
fn a_negative_length_typed_as_a_whole_number_is_refused_by_polars_and_gives_the_walk_no_dtype() {
    // Polars converts a length to an unsigned whole number when it works out the schema, so a
    // negative literal is refused there, and the walk, which cannot say what it gives, claims no
    // dtype after it. A negative head is allowed, and gives text.
    let padded = field_then(
        "owner",
        vec![call("Expr.str.pad_start", json!({ "length": whole(-1) }))],
    );
    let report = report_of(&selecting(padded));
    let step = &report.steps()[0];
    assert!(
        matches!(step.outcome(), Outcome::Refused(refusal) if matches!(refusal.cause(), RunCause::Polars(_))),
        "{:?}",
        step.outcome()
    );
    let expression = step
        .walk()
        .expressions()
        .iter()
        .find(|expression| {
            expression.place().path()
                == [
                    Segment::Parameter("named_exprs".to_owned()),
                    Segment::Value(0),
                ]
        })
        .expect("the walk holds the chain");
    let last = expression.places().last().cloned().flatten();
    assert_eq!(last, None, "no dtype is claimed after the pad");
    let headed = field_then(
        "owner",
        vec![call("Expr.str.head", json!({ "n": whole(-1) }))],
    );
    let report = report_of(&selecting(headed));
    assert!(matches!(report.steps()[0].outcome(), Outcome::Valid { .. }));
}

#[test]
fn a_trim_that_is_given_no_characters_is_complete_and_trims_whitespace() {
    // The editor starts `strip_chars` with nothing for `characters`, which Python leaves out: the
    // step is complete, runs, and takes the whitespace off both ends.
    let trimmed = field_then(
        "owner",
        vec![
            call(
                "Expr.str.pad_start",
                json!({ "length": whole(13), "fill_char": { "string": " " } }),
            ),
            call("Expr.str.pad_end", json!({ "length": whole(15) })),
            call("Expr.str.strip_chars", json!({})),
        ],
    );
    let written = selecting(trimmed);
    let report = report_of(&written);
    let step = &report.steps()[0];
    assert!(
        matches!(step.outcome(), Outcome::Valid { .. }),
        "{:?}",
        step.outcome()
    );
    assert_eq!(step.walk().refusal(), None);
    let preview = Document::read(&written)
        .expect("a document")
        .preview(&inputs(), &claims(), &StepId::new("s").expect("an id"), 100)
        .expect("the rows");
    let cells: Vec<Option<&str>> = preview.rows().iter().map(|row| row[0].as_deref()).collect();
    assert_eq!(cells[0], Some("Company A"));
    assert_eq!(cells[2], Some("Company B"));
}
