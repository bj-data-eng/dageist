//! What the walk says of a join's keys: which pairs of output names they give,
//! and the tables `on` is judged against.
//!
//! Every expectation is what Polars does with the same join, which each test asks
//! by running the step. Polars names a key by the column it reads (the receiver of
//! a chain, and the value a condition gives first), or `literal`, and refuses a
//! join that pairs the same two names twice. A name repeated on one side only is
//! not refused.

use dageist_engine::allowed::{Context, Outer, RefusalSource};
use dageist_engine::document::{Document, Inputs, Outcome, Report, RunCause, Segment, Walk};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, field_then, many, step, whole};

/// The table a join starts from: `k` and `a` are whole numbers, `g` text.
fn left() -> DataFrame {
    df!(
        "a" => [1_i64, 2, 3],
        "k" => [1_i64, 1, 2],
        "g" => ["x", "y", "x"],
    )
    .expect("the left table")
}

/// The table it joins, whose columns are named apart from the left's.
fn right() -> DataFrame {
    df!(
        "ra" => [1_i64, 2, 3],
        "rk" => [1_i64, 2, 3],
        "rg" => ["x", "y", "z"],
    )
    .expect("the right table")
}

fn inputs_of(other: DataFrame) -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_table("other", other.lazy())
        .expect("a first input");
    inputs
}

/// What running a join of `left` and `other` given `arguments` reports: Polars'
/// outcome, and what the walk says of it.
fn join_of(left: DataFrame, other: DataFrame, arguments: Value) -> Report {
    let mut given = arguments;
    given["other"] = json!({ "table": "other" });
    let document =
        Document::read(&document(vec![step("s", "LazyFrame.join", given)])).expect("a document");
    document
        .run(&inputs_of(other), left.lazy())
        .expect("the left table has a schema")
}

fn walk(report: &Report) -> &Walk {
    report.steps()[0].walk()
}

fn outcome(report: &Report) -> &Outcome {
    report.steps()[0].outcome()
}

fn keys(left_on: Vec<Value>, right_on: Vec<Value>) -> Value {
    json!({ "left_on": many(left_on), "right_on": many(right_on) })
}

fn first(name: &str) -> Value {
    field_then(name, vec![call("Expr.first", json!({}))])
}

fn polars_refuses(outcome: &Outcome) -> bool {
    matches!(
        outcome,
        Outcome::Refused(refusal) if matches!(refusal.cause(), RunCause::Polars(_))
    )
}

fn polars_reason(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Refused(refusal) => refusal.to_string(),
        other => panic!("Polars allows it: {other:?}"),
    }
}

fn item(parameter: &str, at: usize) -> Vec<Segment> {
    vec![Segment::Parameter(parameter.to_owned()), Segment::Item(at)]
}

/// The place and the source of what the walk refuses.
fn refused(walk: &Walk) -> Option<(Vec<Segment>, RefusalSource)> {
    walk.refusal()
        .map(|refusal| (refusal.place().path().to_vec(), refusal.refusal().source()))
}

#[test]
fn a_join_that_pairs_the_same_two_names_twice_is_refused_at_the_key_that_repeats_them() {
    // `k` and `k.first()` are both named `k`, and so are `rk` and `rk.first()`:
    // the pair of `k` and `rk` is made twice.
    let report = join_of(
        left(),
        right(),
        keys(vec![field("k"), first("k")], vec![field("rk"), first("rk")]),
    );
    assert!(polars_refuses(outcome(&report)), "{:?}", outcome(&report));
    assert!(
        polars_reason(outcome(&report)).contains("repeated key names"),
        "{}",
        polars_reason(outcome(&report))
    );
    let (place, source) = refused(walk(&report)).expect("the tables refuse it");
    assert_eq!(place, item("left_on", 1));
    assert!(matches!(source, RefusalSource::Rule(_)), "{source:?}");
}

#[test]
fn a_name_repeated_on_one_side_only_is_not_refused() {
    // Polars refuses a pair made twice, not a name on one side: `k` twice on the
    // left is paired with `rk` and with `ra`, and `rk` twice on the right with `k`
    // and with `a`.
    for (left_on, right_on) in [
        (vec![field("k"), first("k")], vec![field("rk"), field("ra")]),
        (vec![field("k"), field("a")], vec![field("rk"), first("rk")]),
        (vec![field("k"), field("k")], vec![field("rk"), field("ra")]),
    ] {
        let report = join_of(left(), right(), keys(left_on, right_on));
        assert!(
            matches!(outcome(&report), Outcome::Valid { .. }),
            "Polars allows it: {:?}",
            outcome(&report)
        );
        assert_eq!(refused(walk(&report)), None);
    }
}

#[test]
fn a_key_is_named_by_the_column_its_chain_starts_from_whatever_follows_it() {
    // Each chain below is named `k` on the left and `rk` on the right, as Polars
    // names it, so pairing it with the plain column makes the same pair twice.
    let chains = [
        field_then("k", vec![call("Expr.is_null", json!({}))]),
        field_then("k", vec![call("Expr.sum", json!({}))]),
        field_then("k", vec![call("Expr.sub", json!({ "other": field("a") }))]),
    ];
    for (at, chained) in chains.into_iter().enumerate() {
        let right_chained = renamed(&chained);
        let report = join_of(
            left(),
            right(),
            keys(vec![field("k"), chained], vec![field("rk"), right_chained]),
        );
        assert!(
            polars_refuses(outcome(&report)),
            "chain {at}: {:?}",
            outcome(&report)
        );
        let (place, source) =
            refused(walk(&report)).unwrap_or_else(|| panic!("chain {at} is refused"));
        assert_eq!(place, item("left_on", 1), "chain {at}");
        assert!(matches!(source, RefusalSource::Rule(_)), "chain {at}");
    }
}

/// The same chain read against the other table: `k` is `rk`, `a` is `ra`, `g` is `rg`.
fn renamed(expression: &Value) -> Value {
    let text = expression
        .to_string()
        .replace("\"k\"", "\"rk\"")
        .replace("\"a\"", "\"ra\"")
        .replace("\"g\"", "\"rg\"");
    serde_json::from_str(&text).expect("the same JSON")
}

#[test]
fn a_literal_is_named_literal_and_a_condition_by_its_first_value() {
    // Two literals are both `literal`.
    let report = join_of(
        left(),
        right(),
        keys(vec![whole(1), whole(2)], vec![whole(1), whole(2)]),
    );
    assert!(polars_refuses(outcome(&report)), "{:?}", outcome(&report));
    assert_eq!(
        refused(walk(&report)).map(|found| found.0),
        Some(item("left_on", 1))
    );

    // A condition takes the name of its first value, the column `a`, not of its
    // `otherwise`, `k`: left of `a` and `a`, paired with `rk` and `rk`.
    let when = |then: Value, otherwise: Value| {
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![field("g")]) }),
            ),
            vec![
                call("When.then", json!({ "statement": then })),
                call("Then.otherwise", json!({ "statement": otherwise })),
            ],
        )
    };
    let report = join_of(
        left(),
        right(),
        keys(
            vec![when(field("a"), field("k")), field("a")],
            vec![field("rk"), field("rk")],
        ),
    );
    assert!(polars_refuses(outcome(&report)), "{:?}", outcome(&report));
    assert_eq!(
        refused(walk(&report)).map(|found| found.0),
        Some(item("left_on", 1))
    );
    // Named by `k`, the condition is paired with `rk` and then `a` with `rk`.
    let report = join_of(
        left(),
        right(),
        keys(
            vec![when(field("k"), field("a")), field("a")],
            vec![field("rk"), field("rk")],
        ),
    );
    assert!(
        matches!(outcome(&report), Outcome::Valid { .. }),
        "{:?}",
        outcome(&report)
    );
    assert_eq!(refused(walk(&report)), None);
}

#[test]
fn keys_given_as_on_are_paired_with_themselves() {
    let shared = df!("k" => [1_i64, 1, 2], "a" => [1_i64, 2, 3]).expect("a table");
    let report = join_of(
        shared.clone(),
        shared.clone(),
        json!({ "on": many(vec![field("k"), first("k")]) }),
    );
    assert!(polars_refuses(outcome(&report)), "{:?}", outcome(&report));
    assert_eq!(
        refused(walk(&report)).map(|found| found.0),
        Some(item("on", 1))
    );
    let report = join_of(
        shared.clone(),
        shared,
        json!({ "on": many(vec![field("k"), field("a")]) }),
    );
    assert!(
        matches!(outcome(&report), Outcome::Valid { .. }),
        "{:?}",
        outcome(&report)
    );
    assert_eq!(refused(walk(&report)), None);
}

#[test]
fn on_is_judged_against_the_other_table_too() {
    // A sum of whole numbers is allowed, and a sum of text is refused: `x` is a
    // whole number on the left and text on the right.
    let left = df!("x" => [1_i64, 2]).expect("a table");
    let other = df!("x" => ["a", "b"]).expect("a table");
    let report = join_of(
        left,
        other,
        json!({ "on": many(vec![field_then("x", vec![call("Expr.sum", json!({}))])]) }),
    );
    assert!(polars_refuses(outcome(&report)), "{:?}", outcome(&report));
    let (place, source) =
        refused(walk(&report)).expect("the tables refuse it against the other table");
    assert_eq!(
        place,
        vec![
            Segment::Parameter("on".to_owned()),
            Segment::Item(0),
            Segment::Link(0)
        ]
    );
    assert!(matches!(source, RefusalSource::Polars(_)), "{source:?}");
}

#[test]
fn on_a_column_the_other_table_lacks_is_not_known_to_the_tables() {
    // `k` is the left's only. Polars cannot find it on the right; the tables
    // cannot say what a column that is not there is, so they do not call it allowed.
    let report = join_of(left(), right(), json!({ "on": many(vec![field("k")]) }));
    assert!(polars_refuses(outcome(&report)), "{:?}", outcome(&report));
    assert_eq!(refused(walk(&report)), None);
    assert!(
        walk(&report).not_known().is_some(),
        "{:?}",
        walk(&report).not_known()
    );
}

#[test]
fn on_a_column_both_tables_have_is_known_and_allowed() {
    let shared = df!("k" => [1_i64, 1, 2]).expect("a table");
    let report = join_of(
        shared.clone(),
        shared,
        json!({ "on": many(vec![field("k")]) }),
    );
    assert!(
        matches!(outcome(&report), Outcome::Valid { .. }),
        "{:?}",
        outcome(&report)
    );
    assert_eq!(refused(walk(&report)), None);
    assert_eq!(walk(&report).not_known(), None);
}

#[test]
fn on_is_walked_as_a_left_key_and_as_a_right_key() {
    let shared = df!("k" => [1_i64, 1, 2]).expect("a table");
    let report = join_of(
        shared.clone(),
        shared,
        json!({ "on": many(vec![field("k")]) }),
    );
    let contexts: Vec<Option<Context>> = walk(&report)
        .expressions()
        .iter()
        .filter(|expression| expression.place().path() == item("on", 0).as_slice())
        .map(|expression| expression.context())
        .collect();
    assert_eq!(
        contexts,
        vec![
            Some(Context::step(Outer::JoinLeft)),
            Some(Context::step(Outer::JoinRight))
        ]
    );
}
