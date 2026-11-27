//! A step that groups a table: `group_by`, the calls on the group by that follow it, and the
//! reasons a chain that does not end as Python's does is refused before Polars is asked.

use dageist_engine::document::{
    Cause, Document, Outcome, Place, ReadError, Report, Resolution, Segment, StepId,
};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, document, field, field_then, grouping, many, named, step};

fn parameter(name: &str) -> Segment {
    Segment::Parameter(name.to_owned())
}

/// The key `k` of a group by.
fn by_k() -> Value {
    json!({ "many": [field("k")] })
}

fn finish(entry: &str, arguments: Value) -> Value {
    call(entry, arguments)
}

fn len() -> Value {
    finish("LazyGroupBy.len", json!({}))
}

/// A `having` that keeps the groups in which every flag `b` is true.
fn having() -> Value {
    finish(
        "LazyGroupBy.having",
        json!({ "predicates": many(vec![field_then("b", vec![call("Expr.all", json!({}))])]) }),
    )
}

/// Four rows of a key, a whole number, a float and a flag.
fn table() -> DataFrame {
    df!(
        "k" => ["a", "b", "a", "b"],
        "x" => [1_i64, 2, 3, 4],
        "y" => [0.5_f64, 1.5, 2.5, 3.5],
        "b" => [true, false, true, true],
    )
    .expect("a table")
}

fn run(steps: Vec<Value>) -> Report {
    Document::read(&document(steps))
        .expect("a document")
        .run_on(&dageist_engine::document::Inputs::new(), &table())
        .expect("the table has a schema")
}

fn refused(steps: Vec<Value>) -> dageist_engine::document::Refusal {
    match Document::read(&document(steps)) {
        Err(ReadError::Refused(refusal)) => refusal,
        other => panic!("not refused: {other:?}"),
    }
}

fn place_of(id: &str, way: Vec<Segment>) -> Place {
    way.into_iter().fold(
        Place::of_step(0, Some(StepId::new(id).expect("an id"))),
        Place::then,
    )
}

#[test]
fn a_step_that_groups_is_written_back_exactly() {
    let text = concat!(
        r#"{"version":1,"steps":[{"id":"g","call":{"entry":"LazyFrame.group_by","arguments":{"by":{"many":["#,
        r#"{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[]}}]},"maintain_order":{"flag":true}}},"#,
        r#""then":[{"entry":"LazyGroupBy.having","arguments":{"predicates":{"many":["#,
        r#"{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[]}}]}}},"#,
        r#"{"entry":"LazyGroupBy.len","arguments":{"name":{"string":"rows"}}}]},"#,
        r#"{"id":"s","call":{"entry":"LazyFrame.head","arguments":{}}}]}"#,
    );
    let document = Document::read(text).expect("a document");
    assert_eq!(document.steps().len(), 2);
    assert_eq!(document.write(), text);
    // A step that does not group holds no `then`.
    let written: Value = serde_json::from_str(&document.write()).expect("JSON");
    assert!(written["steps"][1].get("then").is_none());
}

#[test]
fn a_chain_that_stops_before_a_finishing_call_is_refused_with_the_reason() {
    for then in [vec![], vec![having()]] {
        let refusal = refused(vec![grouping("g", json!({ "by": by_k() }), then)]);
        assert!(
            refusal
                .to_string()
                .contains("needs a call that finishes it"),
            "{refusal}"
        );
    }
    // Where it stops is the step, or the last call it has.
    let refusal = refused(vec![grouping("g", json!({ "by": by_k() }), vec![])]);
    assert_eq!(refusal.place(), &place_of("g", vec![]));
    let refusal = refused(vec![grouping("g", json!({ "by": by_k() }), vec![having()])]);
    assert_eq!(refusal.place(), &place_of("g", vec![Segment::Link(0)]));
}

#[test]
fn a_call_that_cannot_follow_the_one_before_it_is_refused_at_its_place() {
    let cases: [(Vec<Value>, usize, &str); 4] = [
        (vec![len(), len()], 1, "a finished grouping"),
        (vec![len(), having()], 1, "a finished grouping"),
        (
            vec![having(), having(), len()],
            1,
            "a group by with its having",
        ),
        (
            vec![having(), len(), finish("LazyGroupBy.sum", json!({}))],
            2,
            "a finished grouping",
        ),
    ];
    for (then, at, found) in cases {
        let refusal = refused(vec![grouping("g", json!({ "by": by_k() }), then)]);
        assert_eq!(refusal.place(), &place_of("g", vec![Segment::Link(at)]));
        let words = refusal.to_string();
        assert!(words.contains(found), "{words}");
        assert!(words.contains("needs a"), "{words}");
    }
}

#[test]
fn only_a_step_that_groups_holds_calls_after_its_own() {
    // The calls of a group by follow `group_by` and nothing else.
    let sorted = json!({
        "id": "s",
        "call": call("LazyFrame.sort", json!({ "by": json!({ "many": [field("k")] }) })),
        "then": [len()],
    });
    let refusal = refused(vec![sorted]);
    assert_eq!(refusal.place(), &place_of("s", vec![Segment::Link(0)]));
    assert!(
        refusal.to_string().contains("a step that does not group"),
        "{refusal}"
    );
    // A method of an expression is no call of a group by.
    let refusal = refused(vec![grouping(
        "g",
        json!({ "by": by_k() }),
        vec![finish("Expr.sum", json!({}))],
    )]);
    assert_eq!(refusal.place(), &place_of("g", vec![Segment::Link(0)]));
    assert!(
        matches!(refusal.cause(), Cause::WrongPlace { .. }),
        "{refusal}"
    );
    // A `then` that is not an array of calls is a document that is not read.
    let text = document(vec![
        json!({ "id": "g", "call": call("LazyFrame.group_by", json!({ "by": by_k() })), "then": {} }),
    ]);
    assert!(Document::read(&text).is_err());
}

#[test]
fn a_chain_that_ends_in_one_finishing_call_is_a_step_that_gives_a_table() {
    let report = run(vec![grouping(
        "g",
        json!({ "by": by_k(), "maintain_order": { "flag": true } }),
        vec![having(), len()],
    )]);
    let [group] = report.steps() else {
        panic!("one step")
    };
    match group.outcome() {
        Outcome::Valid { schema, .. } => {
            let columns: Vec<(String, DataType)> = schema
                .iter()
                .map(|(name, dtype)| (name.to_string(), dtype.clone()))
                .collect();
            assert_eq!(
                columns,
                [
                    ("k".to_owned(), DataType::String),
                    ("len".to_owned(), DataType::UInt32)
                ]
            );
        }
        other => panic!("not valid: {other:?}"),
    }
}

#[test]
fn an_aggregate_that_does_not_aggregate_is_reported_as_a_list_for_each_group() {
    let sum = crate::support::field_then("x", vec![call("Expr.sum", json!({}))]);
    let plus_one = crate::support::field_then(
        "x",
        vec![call(
            "Expr.add",
            json!({ "other": crate::support::whole(1) }),
        )],
    );
    let report = run(vec![grouping(
        "g",
        json!({ "by": by_k() }),
        vec![finish(
            "LazyGroupBy.agg",
            json!({
                "named_aggs": named(vec![("total", sum), ("shifted", plus_one)])
            }),
        )],
    )]);
    let [group] = report.steps() else {
        panic!("one step")
    };
    let Outcome::Valid { schema, .. } = group.outcome() else {
        panic!("not valid: {:?}", group.outcome())
    };
    assert_eq!(schema.get("total"), Some(&DataType::Int64));
    assert_eq!(
        schema.get("shifted"),
        Some(&DataType::List(Box::new(DataType::Int64)))
    );
    // The dtypes the report gives of the expressions are what they give in the group: the column
    // alone is the list of its values, and its sum is one value.
    let dtype_after = |parameter_of: usize, links: usize| {
        let place = place_of(
            "g",
            vec![
                Segment::Link(0),
                parameter("named_aggs"),
                Segment::Value(parameter_of),
            ],
        );
        let expression = group
            .expressions()
            .iter()
            .find(|expression| expression.place() == &place)
            .unwrap_or_else(|| panic!("no expression at {place}"));
        let resolved = if links == 0 {
            expression.start()
        } else {
            &expression.links()[links - 1]
        };
        match resolved {
            Resolution::Dtype(dtype) => dtype.clone(),
            other => panic!("not one dtype: {other:?}"),
        }
    };
    assert_eq!(dtype_after(0, 0), DataType::List(Box::new(DataType::Int64)));
    assert_eq!(dtype_after(0, 1), DataType::Int64);
    assert_eq!(dtype_after(1, 1), DataType::List(Box::new(DataType::Int64)));
}

#[test]
fn what_is_still_to_be_picked_in_a_call_after_group_by_makes_the_step_incomplete() {
    let hole = crate::edits::hole();
    let report = run(vec![grouping(
        "g",
        json!({ "by": by_k() }),
        vec![finish(
            "LazyGroupBy.agg",
            json!({ "named_aggs": named(vec![("total", hole)]) }),
        )],
    )]);
    let [group] = report.steps() else {
        panic!("one step")
    };
    let Outcome::Incomplete { place } = group.outcome() else {
        panic!("not incomplete: {:?}", group.outcome())
    };
    assert_eq!(
        place,
        &place_of(
            "g",
            vec![
                Segment::Link(0),
                parameter("named_aggs"),
                Segment::Value(0),
                Segment::Start
            ]
        )
    );
}

#[test]
fn a_step_that_groups_runs_before_the_steps_after_it_and_gives_them_its_table() {
    let report = run(vec![
        grouping("g", json!({ "by": by_k() }), vec![len()]),
        step(
            "sorted",
            "LazyFrame.sort",
            json!({ "by": json!({ "many": [field("len")] }) }),
        ),
    ]);
    assert_eq!(report.steps().len(), 2);
    assert!(matches!(report.steps()[0].outcome(), Outcome::Valid { .. }));
    assert!(matches!(report.steps()[1].outcome(), Outcome::Valid { .. }));
}
