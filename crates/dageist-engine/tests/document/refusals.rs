//! A step Polars refuses is reported with where in the step it is and Polars'
//! own reason, and the steps after it are not run.

use dageist_engine::allowed::{RefusalSource, Rule};
use dageist_engine::catalog::BuildError;
use dageist_engine::document::{
    Document, Outcome, Report, RowsError, RunCause, Segment, StepId, StepRefusal,
};
use polars::prelude::*;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::ask_polars::{self, Connected};
use crate::documents::{documents, inputs};
use crate::support::{call, chain, document, field, field_then, many, named, step, whole};

fn parameter(name: &str) -> Segment {
    Segment::Parameter(name.to_owned())
}

fn run(steps: Vec<Value>) -> Report {
    Document::read(&document(steps))
        .expect("a document")
        .run(&inputs(), documents().lazy())
        .expect("the documents have a schema")
}

fn refusal_of<'a>(report: &'a Report, id: &str) -> &'a StepRefusal {
    let step = report
        .steps()
        .iter()
        .find(|step| step.id().as_str() == id)
        .unwrap_or_else(|| panic!("no step {id}"));
    match step.outcome() {
        Outcome::Refused(refusal) => refusal,
        other => panic!("step {id} is {other:?}"),
    }
}

/// The refusal of the tables that refused the step `id` of `report`, which the step engine reports
/// as it is, at the place the walk found it, and which records what Polars said of the call.
fn refused_by_the_tables_as_polars_did<'a>(
    report: &'a Report,
    id: &str,
    refusal: &StepRefusal,
) -> &'a dageist_engine::allowed::Refusal {
    let step = report
        .steps()
        .iter()
        .find(|step| step.id().as_str() == id)
        .unwrap_or_else(|| panic!("no step {id}"));
    let walked = step.walk().refusal().expect("the walk refuses the step");
    assert_eq!(refusal.place(), walked.place());
    assert!(
        matches!(walked.refusal().source(), RefusalSource::Polars(_)),
        "the tables record Polars refusing it: {:?}",
        walked.refusal().source()
    );
    assert!(
        matches!(refusal.cause(), RunCause::NotAllowed(said) if said == walked.refusal()),
        "not the words of the tables: {refusal}"
    );
    walked.refusal()
}

/// The first line of Polars' words, which the tables record: the plan it adds after them is
/// Polars' own.
fn first_line(words: &str) -> &str {
    words.lines().next().unwrap_or_default()
}

/// Polars' own words for the plan the step builds: what `collect_schema` says
/// of `select` of `expressions` on the documents.
fn polars_says(expressions: Vec<Expr>) -> String {
    documents()
        .lazy()
        .select(expressions)
        .collect_schema()
        .expect_err("Polars refuses it")
        .to_string()
}

#[test]
fn a_sum_of_text_is_refused_at_its_link_with_polars_reason() {
    // `document_id` is text, which Polars cannot add up.
    let report = run(vec![
        step(
            "bad",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![(
                "total",
                field_then("document_id", vec![call("Expr.sum", json!({}))]),
            )]) }),
        ),
        step("after", "LazyFrame.select", json!({})),
    ]);
    let refusal = refusal_of(&report, "bad");
    assert_eq!(refusal.place().step_id().map(StepId::as_str), Some("bad"));
    assert_eq!(refusal.place().step_index(), Some(0));
    assert_eq!(
        refusal.place().path(),
        [
            parameter("named_exprs"),
            Segment::Value(0),
            Segment::Link(0)
        ]
    );
    // The step is not built: it is refused where the walk found the refusal, with the words the
    // tables recorded of Polars, which are Polars' own for the same call.
    let said = refused_by_the_tables_as_polars_did(&report, "bad", refusal);
    assert_eq!(
        said.reason(),
        first_line(&polars_says(vec![col("document_id").sum()]))
    );
    assert!(said.reason().contains("sum"), "{said}");
    assert!(said.reason().contains("str"), "{said}");
}

#[test]
fn taking_text_from_a_timestamp_is_refused_at_its_link_with_polars_reason() {
    let report = run(vec![step(
        "bad",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![(
            "gap",
            field_then("timestamp", vec![call("Expr.sub", json!({ "other": field("document_id") }))]),
        )]) }),
    )]);
    let refusal = refusal_of(&report, "bad");
    assert_eq!(
        refusal.place().path(),
        [
            parameter("named_exprs"),
            Segment::Value(0),
            Segment::Link(0)
        ]
    );
    let said = refused_by_the_tables_as_polars_did(&report, "bad", refusal);
    assert_eq!(
        said.reason(),
        first_line(&polars_says(vec![binary_expr(
            col("timestamp"),
            Operator::Minus,
            col("document_id")
        )]))
    );
}

#[test]
fn a_step_the_walk_refuses_is_not_planned_so_it_reports_no_dtype_but_says_where_it_is_refused() {
    let report = run(vec![step(
        "bad",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![(
            "total",
            field_then("document_id", vec![call("Expr.first", json!({})), call("Expr.sum", json!({}))]),
        )]) }),
    )]);
    // The dtype of each part of an expression is worked out by planning it, which a step the walk
    // refuses is not.
    assert!(report.steps()[0].expressions().is_empty());
    assert!(report.steps()[0].schema().is_none());
    let refusal = refusal_of(&report, "bad");
    assert_eq!(
        refusal.place().path(),
        [
            parameter("named_exprs"),
            Segment::Value(0),
            Segment::Link(1)
        ]
    );
    // The walk still says what each part is, by its class.
    let walked = report.steps()[0]
        .walk()
        .expressions()
        .iter()
        .find(|expression| {
            expression.place().path() == [parameter("named_exprs"), Segment::Value(0)]
        })
        .expect("the expression");
    assert!(!walked.places().is_empty());
}

#[test]
fn a_step_after_a_refused_step_is_not_run() {
    let report = run(vec![
        step(
            "fine",
            "LazyFrame.select",
            json!({ "exprs": many(vec![field("document_id")]) }),
        ),
        step(
            "bad",
            "LazyFrame.select",
            json!({ "exprs": many(vec![field("nope")]) }),
        ),
        step(
            "later",
            "LazyFrame.select",
            json!({ "exprs": many(vec![field("document_id")]) }),
        ),
        step(
            "last",
            "LazyFrame.select",
            json!({ "exprs": many(vec![field("document_id")]) }),
        ),
    ]);
    assert!(matches!(report.steps()[0].outcome(), Outcome::Valid { .. }));
    assert!(matches!(report.steps()[1].outcome(), Outcome::Refused(_)));
    for step in &report.steps()[2..] {
        assert!(
            matches!(step.outcome(), Outcome::NotRun { blocked_by } if blocked_by.as_str() == "bad"),
            "{}: {:?}",
            step.id(),
            step.outcome()
        );
        assert!(step.expressions().is_empty());
        assert!(step.schema().is_none());
    }
}

#[test]
fn a_missing_column_is_refused_at_its_start_with_polars_reason() {
    let report = run(vec![step(
        "bad",
        "LazyFrame.sort",
        json!({ "by": many(vec![field("document_id"), field("nope")]) }),
    )]);
    let refusal = refusal_of(&report, "bad");
    assert_eq!(
        refusal.place().path(),
        [parameter("by"), Segment::Item(1), Segment::Start]
    );
    let RunCause::Polars(reason) = refusal.cause() else {
        panic!("not Polars' reason: {refusal}");
    };
    assert_eq!(reason.to_string(), polars_says(vec![col("nope")]));
    assert!(reason.to_string().contains("nope"), "{reason}");
    assert!(reason.to_string().contains("valid columns"), "{reason}");
}

#[test]
fn a_column_a_later_step_adds_is_missing_before_that_step() {
    // `later` is made by the second step, so the first cannot use it.
    let report = run(vec![
        step(
            "early",
            "LazyFrame.select",
            json!({ "exprs": many(vec![field("later")]) }),
        ),
        step(
            "second",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![("later", field("document_id"))]) }),
        ),
    ]);
    let refusal = refusal_of(&report, "early");
    assert!(matches!(refusal.cause(), RunCause::Polars(_)), "{refusal}");
    assert!(refusal.to_string().contains("later"), "{refusal}");
}

#[test]
fn a_missing_column_inside_a_nested_expression_is_found_through_its_links() {
    // `document_id.shift(n).over(nope)`: the missing column is the partition.
    let report = run(vec![step(
        "bad",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![(
            "x",
            field_then(
                "document_id",
                vec![call("Expr.over", json!({ "partition_by": many(vec![field("nope")]) }))],
            ),
        )]) }),
    )]);
    let refusal = refusal_of(&report, "bad");
    assert_eq!(
        refusal.place().path(),
        [
            parameter("named_exprs"),
            Segment::Value(0),
            Segment::Link(0),
            parameter("partition_by"),
            Segment::Item(0),
            Segment::Start,
        ]
    );
    assert!(refusal.to_string().contains("nope"), "{refusal}");
}

#[test]
fn a_join_on_keys_of_different_dtypes_is_refused_at_the_step() {
    // Text against whole numbers: each key is fine alone, so the refusal is the
    // step's, with Polars' reason for it.
    let report = run(vec![step(
        "join",
        "LazyFrame.join",
        json!({
            "other": { "table": "step_sla" },
            "left_on": many(vec![field("document_id")]),
            "right_on": many(vec![field("sla_days")]),
        }),
    )]);
    let refusal = refusal_of(&report, "join");
    assert!(refusal.place().path().is_empty(), "{refusal}");
    assert!(matches!(refusal.cause(), RunCause::Polars(_)), "{refusal}");
}

#[test]
fn a_table_that_is_not_connected_is_refused_at_its_argument() {
    let report = run(vec![step(
        "join",
        "LazyFrame.join",
        json!({
            "other": { "table": "nowhere" },
            "on": many(vec![field("document_id")]),
        }),
    )]);
    let refusal = refusal_of(&report, "join");
    assert_eq!(refusal.place().path(), [parameter("other")]);
    assert!(matches!(refusal.cause(), RunCause::UnknownTable(name) if name == "nowhere"));
}

#[test]
fn an_input_that_is_not_connected_is_refused_at_the_start_of_its_expression() {
    // `sla_cutoff` is a value, not a table, and `step_sla` a table, not a value.
    for (name, how) in [("missing", "no input"), ("step_sla", "a table")] {
        let report = run(vec![step(
            "bad",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![(
                "x",
                chain(json!({ "input": name }), vec![]),
            )]) }),
        )]);
        let refusal = refusal_of(&report, "bad");
        assert_eq!(
            refusal.place().path(),
            [parameter("named_exprs"), Segment::Value(0), Segment::Start],
            "{how}"
        );
        assert!(matches!(refusal.cause(), RunCause::UnknownInput(found) if found == name));
    }
}

#[test]
fn a_missing_argument_is_refused_at_its_parameter() {
    let report = run(vec![
        step("no_keys", "LazyFrame.sort", json!({})),
        step("after", "LazyFrame.select", json!({})),
    ]);
    let refusal = refusal_of(&report, "no_keys");
    assert_eq!(refusal.place().path(), [parameter("by")]);
    assert!(matches!(
        refusal.cause(),
        RunCause::Build(BuildError::Missing { parameter: "by" })
    ));
}

#[test]
fn a_number_out_of_the_range_a_parameter_takes_is_refused_at_the_link_and_its_parameter() {
    // Python passes the places of a rounding as an unsigned and the figures as a signed 32-bit
    // number, so a build refuses one outside them, before Polars is asked anything.
    let cases: [(&str, &str, i64); 3] = [
        ("Expr.round", "decimals", -1),
        ("Expr.truncate", "decimals", 4_294_967_296),
        ("Expr.round_sig_figs", "digits", 2_147_483_648),
    ];
    for (entry, name, number) in cases {
        let report = run(vec![step(
            "bad",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![(
                "x",
                field_then("row_days", vec![call(entry, json!({ name: { "integer": number } }))]),
            )]) }),
        )]);
        let refusal = refusal_of(&report, "bad");
        assert_eq!(
            refusal.place().path(),
            [
                parameter("named_exprs"),
                Segment::Value(0),
                Segment::Link(0),
                parameter(name),
            ],
            "{entry}"
        );
        assert!(
            matches!(
                refusal.cause(),
                RunCause::Build(BuildError::OutOfRange { parameter, .. }) if *parameter == name
            ),
            "{entry}"
        );
    }
}

#[test]
fn a_missing_argument_of_a_link_is_refused_at_the_link_and_its_parameter() {
    // `sub` needs the value to take away.
    let report = run(vec![step(
        "bad",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![(
            "x",
            field_then("timestamp", vec![call("Expr.is_null", json!({})), call("Expr.sub", json!({}))]),
        )]) }),
    )]);
    let refusal = refusal_of(&report, "bad");
    assert_eq!(
        refusal.place().path(),
        [
            parameter("named_exprs"),
            Segment::Value(0),
            Segment::Link(1),
            parameter("other"),
        ]
    );
    assert!(matches!(
        refusal.cause(),
        RunCause::Build(BuildError::Missing { parameter: "other" })
    ));
}

#[test]
fn rows_of_a_refused_step_or_an_unknown_step_are_errors() {
    let document = Document::read(&document(vec![
        step(
            "fine",
            "LazyFrame.select",
            json!({ "exprs": many(vec![field("document_id")]) }),
        ),
        step(
            "bad",
            "LazyFrame.select",
            json!({ "exprs": many(vec![field("nope")]) }),
        ),
    ]))
    .expect("a document");
    let working = documents();
    let id = |text: &str| StepId::new(text).expect("an id");

    let rows = document
        .rows(&inputs(), &working, &id("fine"), 10)
        .expect("rows");
    assert_eq!((rows.height(), rows.width()), (7, 1));

    let error = document
        .rows(&inputs(), &working, &id("bad"), 10)
        .expect_err("refused");
    let RowsError::Refused(refusal) = error else {
        panic!("not refused: {error:?}");
    };
    assert_eq!(refusal.place().step_id().map(StepId::as_str), Some("bad"));

    let error = document
        .rows(&inputs(), &working, &id("absent"), 10)
        .expect_err("no step");
    assert!(matches!(error, RowsError::NoSuchStep(missing) if missing.as_str() == "absent"));
}

#[test]
fn a_date_part_of_text_passes_schema_resolution_and_fails_when_rows_are_collected() {
    // Characterization of Polars: `dt.date` is known to give a `Date` whatever it
    // is applied to, so `collect_schema` accepts it on text and the dtype
    // mismatch is only found when the rows are collected.
    let steps = vec![step(
        "day",
        "LazyFrame.with_columns",
        json!({ "named_exprs": named(vec![(
            "day",
            field_then("document_id", vec![call("Expr.dt.date", json!({}))]),
        )]) }),
    )];
    let document = Document::read(&document(steps)).expect("a document");
    let nothing = |_: &str| None;
    let connected = Connected::tables(&nothing);
    let step = &document.steps()[0];
    let planned = ask_polars::guarded(step, || ask_polars::plan(step, 0, &documents(), connected))
        .expect("a step that can be asked");
    let schema = planned.schema.expect("Polars plans it");
    assert_eq!(schema.get("day"), Some(&DataType::Date));
    let error = ask_polars::guarded(step, || {
        ask_polars::collect(step, 0, &documents(), connected, 10)
    })
    .expect_err("Polars cannot take the date of text");
    assert!(!error.is_empty());

    // The tables record the failure, so the step engine does not run the step, and neither plans
    // it nor collects its rows.
    let id = StepId::new("day").expect("an id");
    let report = document
        .run(&inputs(), documents().lazy())
        .expect("the documents have a schema");
    assert!(matches!(report.steps()[0].outcome(), Outcome::Refused(_)));
    let error = document
        .rows(&inputs(), &documents(), &id, 10)
        .expect_err("the step is refused");
    assert!(matches!(error, RowsError::Refused(_)), "{error}");
}

#[test]
fn element_outside_a_pivots_aggregate_is_refused_at_its_start_by_the_rule() {
    let report = run(vec![
        step(
            "bad",
            "LazyFrame.with_columns",
            json!({ "named_exprs": named(vec![(
                "x",
                chain(
                    call("polars.element", json!({})),
                    vec![call("Expr.first", json!({}))],
                ),
            )]) }),
        ),
        step("after", "LazyFrame.select", json!({})),
    ]);
    let refusal = refusal_of(&report, "bad");
    assert_eq!(
        refusal.place().path(),
        [parameter("named_exprs"), Segment::Value(0), Segment::Start]
    );
    let RunCause::NotAllowed(not_allowed) = refusal.cause() else {
        panic!("not the rule's reason: {refusal}");
    };
    assert_eq!(
        not_allowed.source(),
        RefusalSource::Rule(Rule::ElementOnlyInPivotAggregate)
    );
    assert!(refusal.to_string().contains("element"), "{refusal}");
    // Polars itself refuses `element` on a table's columns.
    assert!(
        documents()
            .lazy()
            .select([element().first()])
            .collect_schema()
            .is_err()
    );
}

#[test]
fn a_joins_right_keys_are_read_against_the_other_table_not_the_one_the_step_starts_from() {
    let join = |left_on: &str, right_on: &str| {
        run(vec![step(
            "join",
            "LazyFrame.join",
            json!({
                "other": { "table": "step_sla" },
                "left_on": many(vec![field(left_on)]),
                "right_on": many(vec![field(right_on)]),
            }),
        )])
    };
    // `step` is a column of the other table only, and `step_to` of the documents only.
    let report = join("step_to", "step");
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Valid { .. }),
        "{:?}",
        report.steps()[0].outcome()
    );
    let report = join("step_to", "step_to");
    let refusal = refusal_of(&report, "join");
    assert_eq!(
        refusal.place().path(),
        [parameter("right_on"), Segment::Item(0), Segment::Start]
    );
    assert!(matches!(refusal.cause(), RunCause::Polars(_)), "{refusal}");
    assert!(refusal.to_string().contains("step_to"), "{refusal}");
    // And `left_on` is read against the documents: the other table's column is not found there.
    let report = join("step", "step");
    assert_eq!(
        refusal_of(&report, "join").place().path(),
        [parameter("left_on"), Segment::Item(0), Segment::Start]
    );
}

/// The step `id` of `report`, valid or not, as the report holds it.
fn outcome_of<'a>(report: &'a Report, id: &str) -> &'a Outcome {
    report
        .steps()
        .iter()
        .find(|step| step.id().as_str() == id)
        .unwrap_or_else(|| panic!("no step {id}"))
        .outcome()
}

/// `when(predicates[0]).then(then).otherwise(otherwise)` as an argument.
fn when_then_otherwise(predicate: Value, then: Value, otherwise: Value, more: Vec<Value>) -> Value {
    let mut links = vec![
        call("When.then", json!({ "statement": then })),
        call("Then.otherwise", json!({ "statement": otherwise })),
    ];
    links.extend(more);
    chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![predicate]) }),
        ),
        links,
    )
}

#[test]
fn a_pivot_aggregate_that_is_a_condition_over_element_is_valid_as_polars_accepts_it() {
    // `when(element().is_not_null()).then(element()).otherwise(lit(0)).sum()`:
    // the predicate is `element()` tested for a value, a Boolean whatever the
    // group holds, and the sum is one value for each group.
    let aggregate = when_then_otherwise(
        chain(
            call("polars.element", json!({})),
            vec![call("Expr.is_not_null", json!({}))],
        ),
        chain(call("polars.element", json!({})), vec![]),
        chain(
            call("polars.lit", json!({ "value": { "integer": 0 } })),
            vec![],
        ),
        vec![call("Expr.sum", json!({}))],
    );
    // Polars, by hand: the same pivot, built on its own API, collects.
    let step_names =
        df!("step_to" => ["Intake", "Review", "Pending", "Complete"]).expect("the step names");
    let pivoted = documents().lazy().pivot(
        cols(["step_to"]),
        Arc::new(step_names),
        cols(["document_id"]),
        cols(["row_days"]),
        when(element().is_not_null())
            .then(element())
            .otherwise(lit(0))
            .sum(),
        false,
        "_".into(),
        polars::frame::PivotColumnNaming::Auto,
    );
    let collected = pivoted.collect().expect("Polars collects the pivot");
    assert_eq!(collected.height(), 2);

    let report = run(vec![step(
        "pivot",
        "LazyFrame.pivot",
        json!({
            "on": { "columns": ["step_to"] },
            "on_columns": { "table": "step_names" },
            "index": { "columns": ["document_id"] },
            "values": { "columns": ["row_days"] },
            "aggregate_function": aggregate,
        }),
    )]);
    assert!(
        matches!(outcome_of(&report, "pivot"), Outcome::Valid { .. }),
        "{:?}",
        outcome_of(&report, "pivot")
    );
}

#[test]
fn a_join_key_that_is_a_condition_of_typed_literals_is_valid_as_polars_accepts_it() {
    // `when(row_days > 1).then(lit(1, Int64)).otherwise(lit(2, Int64))` is an
    // `Int64` key, as `sla_days` is.
    let typed = |number: i64| {
        chain(
            call(
                "polars.lit",
                json!({ "value": { "integer": number }, "dtype": { "dtype": "Int64" } }),
            ),
            vec![],
        )
    };
    let key = when_then_otherwise(
        field_then(
            "row_days",
            vec![call("Expr.gt", json!({ "other": whole(1) }))],
        ),
        typed(1),
        typed(2),
        vec![],
    );
    // Polars, by hand: the same join, on its own API, collects.
    let int = |number: i64| lit(Scalar::new(DataType::Int64, AnyValue::Int64(number)));
    let sla = df!(
        "step" => ["Intake", "Review", "Pending", "Complete"],
        "sla_days" => [1_i64, 1, 2, 3],
    )
    .expect("the SLA table");
    let joined = documents()
        .lazy()
        .join_builder()
        .with(sla.lazy())
        .left_on(vec![
            when(col("row_days").gt(lit(1)))
                .then(int(1))
                .otherwise(int(2)),
        ])
        .right_on(vec![col("sla_days")])
        .finish();
    let collected = joined
        .expect("Polars builds the join")
        .collect()
        .expect("Polars collects the join");
    assert!(collected.height() > 0);

    let report = run(vec![step(
        "join",
        "LazyFrame.join",
        json!({
            "other": { "table": "step_sla" },
            "left_on": many(vec![key]),
            "right_on": many(vec![field("sla_days")]),
        }),
    )]);
    assert!(
        matches!(outcome_of(&report, "join"), Outcome::Valid { .. }),
        "{:?}",
        outcome_of(&report, "join")
    );
}

#[test]
fn a_column_in_a_pivot_aggregate_is_refused_with_polars_reason_for_the_step_as_the_tables_record_it()
 {
    // The aggregate of a pivot is built from `element()`, not from a column.
    // No named rule holds this, so Polars decides as the step is built, and
    // refuses the pivot as a whole: it does not say which expression.
    let report = run(vec![step(
        "pivot",
        "LazyFrame.pivot",
        json!({
            "on": { "columns": ["step_to"] },
            "on_columns": { "table": "step_names" },
            "index": { "columns": ["document_id"] },
            "values": { "columns": ["row_days"] },
            "aggregate_function": field_then("row_days", vec![call("Expr.sum", json!({}))]),
        }),
    )]);
    let refusal = refusal_of(&report, "pivot");
    assert_eq!(refusal.place().step_index(), Some(0));
    // The step is not built: it is refused where the walk found the refusal, in the words the
    // tables recorded of Polars.
    let walked = report.steps()[0]
        .walk()
        .refusal()
        .expect("the walk refuses it");
    assert_eq!(refusal.place(), walked.place());
    assert!(
        matches!(refusal.cause(), RunCause::NotAllowed(said) if said == walked.refusal()),
        "{refusal}"
    );
    assert!(
        refusal.to_string().contains("column references"),
        "{refusal}"
    );
    // Polars, by hand: its own pivot refuses a column in the aggregate.
    let step_names =
        df!("step_to" => ["Intake", "Review", "Pending", "Complete"]).expect("the step names");
    let mut pivoted = documents().lazy().pivot(
        cols(["step_to"]),
        Arc::new(step_names),
        cols(["document_id"]),
        cols(["row_days"]),
        col("row_days").sum(),
        false,
        "_".into(),
        polars::frame::PivotColumnNaming::Auto,
    );
    let error = pivoted.collect_schema().expect_err("Polars refuses it");
    assert!(error.to_string().contains("column references"), "{error}");
}

#[test]
#[should_panic(
    expected = "Polars panicked when it was asked of the step \"bad\" of LazyFrame.select: a thread broke"
)]
fn a_panic_of_polars_when_it_is_asked_of_a_step_fails_the_test_and_names_the_step() {
    let document = Document::read(&document(vec![step(
        "bad",
        "LazyFrame.select",
        json!({ "exprs": many(vec![field("document_id")]) }),
    )]))
    .expect("a document");
    ask_polars::guarded(&document.steps()[0], || panic!("a thread broke"));
}
