//! What the walk says of a join's keys: which pairs of output names they give,
//! and the tables `on` is judged against.
//!
//! Every expectation is what Polars does with the same join, which each test asks
//! by running the step. Polars names a key by the column it reads (the receiver of
//! a chain, and the value a condition gives first), or `literal`, and refuses a
//! join that pairs the same two names twice. A name repeated on one side only is
//! not refused.

use dageist_engine::allowed::{Context, Ending, Outer, RefusalSource, Rule};
use dageist_engine::document::{Document, Inputs, Report, Segment, Walk};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::ask_polars::{self, Connected};
use crate::support::{call, chain, document, field, field_then, many, step, whole};

/// The table a join starts from: `k` and `a` are whole numbers, `g` text, `f` Booleans.
fn left() -> DataFrame {
    df!(
        "a" => [1_i64, 2, 3],
        "k" => [1_i64, 1, 2],
        "g" => ["x", "y", "x"],
        "f" => [true, false, true],
    )
    .expect("the left table")
}

/// The table it joins, whose columns are named apart from the left's.
fn right() -> DataFrame {
    df!(
        "ra" => [1_i64, 2, 3],
        "rk" => [1_i64, 2, 3],
        "rg" => ["x", "y", "z"],
        "rf" => [true, false, true],
    )
    .expect("the right table")
}

fn inputs_of(other: DataFrame) -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_table("other", "Other", other.lazy())
        .expect("a first input");
    inputs
}

/// What Polars says of a join: it works out the schema, or refuses.
#[derive(Debug)]
enum Polars {
    Allows,
    Refuses(String),
}

/// What running a join reports, and what Polars said of it when asked apart from the step engine,
/// which does not run a join the walk refuses.
struct Joined {
    report: Report,
    polars: Polars,
}

/// What running a join of `left` and `other` given `arguments` reports: Polars' answer, and what
/// the walk says of it.
fn join_of(left: DataFrame, other: DataFrame, arguments: Value) -> Joined {
    let mut given = arguments;
    given["other"] = json!({ "table": "other" });
    let document =
        Document::read(&document(vec![step("s", "LazyFrame.join", given)])).expect("a document");
    let tables = |name: &str| (name == "other").then(|| other.clone().lazy());
    let step = &document.steps()[0];
    let planned = ask_polars::guarded(step, || {
        ask_polars::plan(step, 0, &left, Connected::tables(&tables))
    });
    let polars = match planned {
        Some(planned) => match planned.schema {
            Ok(_) => Polars::Allows,
            Err(refused) => Polars::Refuses(refused.reason),
        },
        None => Polars::Refuses("the join cannot be asked".to_owned()),
    };
    let report = document
        .run(&inputs_of(other), left.lazy())
        .expect("the left table has a schema");
    Joined { report, polars }
}

fn walk(joined: &Joined) -> &Walk {
    joined.report.steps()[0].walk()
}

fn outcome(joined: &Joined) -> &Polars {
    &joined.polars
}

fn keys(left_on: Vec<Value>, right_on: Vec<Value>) -> Value {
    json!({ "left_on": many(left_on), "right_on": many(right_on) })
}

fn first(name: &str) -> Value {
    field_then(name, vec![call("Expr.first", json!({}))])
}

fn polars_refuses(outcome: &Polars) -> bool {
    matches!(outcome, Polars::Refuses(_))
}

fn polars_reason(outcome: &Polars) -> String {
    match outcome {
        Polars::Refuses(reason) => reason.clone(),
        Polars::Allows => panic!("Polars allows it"),
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
    assert!(
        source == RefusalSource::Rule(Rule::JoinKeyNamesAreNotRepeated),
        "{source:?}"
    );
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
            matches!(outcome(&report), Polars::Allows),
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
        assert!(
            source == RefusalSource::Rule(Rule::JoinKeyNamesAreNotRepeated),
            "chain {at}"
        );
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
                json!({ "predicates": many(vec![field("f")]) }),
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
        matches!(outcome(&report), Polars::Allows),
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
        matches!(outcome(&report), Polars::Allows),
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
        matches!(outcome(&report), Polars::Allows),
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

/// A table of whole numbers `a`, `b` and `k`, for a join's left side.
fn wide_left() -> DataFrame {
    df!("a" => [1_i64, 2], "b" => [1_i64, 2], "k" => [1_i64, 2]).expect("a table")
}

/// A table of whole numbers for a join's right side.
fn wide_right() -> DataFrame {
    df!("p" => [1_i64, 2], "z" => [1_i64, 2], "w" => [1_i64, 2], "rk" => [1_i64, 2])
        .expect("a table")
}

/// A `col` that names several columns, which Polars expands to a key for each.
fn columns(names: &[&str]) -> Value {
    chain(
        call("polars.col", json!({ "name": { "columns": names } })),
        vec![],
    )
}

/// An expression that starts from what the editor has not picked yet.
fn hole() -> Value {
    chain(json!({ "hole": null }), vec![])
}

#[test]
fn a_key_that_expands_to_several_shifts_the_pairs_after_it_so_none_is_judged() {
    // Polars expands each side before it pairs the keys: `col("a", "b")` is two keys, so
    // the pairs are (a, p), (b, z), (k, z), (k, w), and none repeats. By index the
    // third key on each side looks like (k, z) twice.
    for expanded in [columns(&["a", "b"]), columns(&["^[ab]$"])] {
        let report = join_of(
            wide_left(),
            wide_right(),
            keys(
                vec![expanded, field("k"), field("k")],
                vec![field("p"), field("z"), field("z"), field("w")],
            ),
        );
        assert!(
            matches!(outcome(&report), Polars::Allows),
            "Polars allows it: {:?}",
            outcome(&report)
        );
        assert_eq!(refused(walk(&report)), None);
    }
}

#[test]
fn the_pairs_before_a_key_that_expands_are_judged() {
    // The keys before the first that may expand pair by index, and Polars refuses the
    // pair (k, rk) made twice whatever follows.
    let report = join_of(
        wide_left(),
        wide_right(),
        keys(
            vec![field("k"), first("k"), columns(&["a", "b"])],
            vec![field("rk"), first("rk"), field("p"), field("z")],
        ),
    );
    assert!(polars_refuses(outcome(&report)), "{:?}", outcome(&report));
    assert_eq!(
        refused(walk(&report)).map(|found| found.0),
        Some(item("left_on", 1))
    );
}

#[test]
fn a_key_that_is_not_picked_yet_ends_the_pairs_that_are_judged() {
    // What the hole stands for may expand, so the pairs after it are not known; the
    // pairs before it are.
    let after = join_of(
        wide_left(),
        wide_right(),
        keys(
            vec![hole(), field("k"), field("k")],
            vec![field("rk"), field("rk"), field("rk")],
        ),
    );
    assert_eq!(refused(walk(&after)), None);
    let before = join_of(
        wide_left(),
        wide_right(),
        keys(
            vec![field("k"), first("k"), hole()],
            vec![field("rk"), first("rk"), field("rk")],
        ),
    );
    assert_eq!(
        refused(walk(&before)).map(|found| found.0),
        Some(item("left_on", 1))
    );
}

#[test]
fn a_key_refused_for_another_reason_keeps_its_refusal_when_it_repeats_a_pair() {
    // `k.sum() > a` holds an aggregate beside a column, which a join's key may not,
    // and is named `k` as the first key is: it makes the same pair again. The key
    // keeps the refusal that it is not elementwise, which came first.
    let mixed = field_then(
        "k",
        vec![
            call("Expr.sum", json!({})),
            call("Expr.gt", json!({ "other": field("a") })),
        ],
    );
    let report = join_of(
        wide_left(),
        wide_right(),
        keys(
            vec![field("k"), mixed.clone()],
            vec![
                field("rk"),
                field_then(
                    "rk",
                    vec![
                        call("Expr.sum", json!({})),
                        call("Expr.gt", json!({ "other": field("p") })),
                    ],
                ),
            ],
        ),
    );
    assert!(polars_refuses(outcome(&report)), "{:?}", outcome(&report));
    let key = walk(&report)
        .expressions()
        .iter()
        .find(|expression| expression.place().path() == item("left_on", 1).as_slice())
        .expect("the second left key is walked");
    match key.ending() {
        Some(Ending::Refused(refusal)) => assert_eq!(
            refusal.source(),
            RefusalSource::Rule(Rule::KeyIsElementwise)
        ),
        other => panic!("the key does not end refused: {other:?}"),
    }
}

/// A table with a column of lists of whole numbers and one of arrays of two, beside whole numbers
/// `a` and the Booleans `f`.
fn with_lists() -> DataFrame {
    let lists = Series::new(
        "l".into(),
        [vec![1_i64, 2], vec![3], vec![4, 5]].map(|values| Series::new("".into(), values)),
    );
    let arrays = Series::new(
        "arr".into(),
        [[1_i64, 2], [3, 4], [5, 6]].map(|pair| Series::new("".into(), pair)),
    )
    .cast(&DataType::Array(Box::new(DataType::Int64), 2))
    .expect("lists of two cast to arrays of two");
    df!("a" => [1_i64, 2, 3], "f" => [true, false, true])
        .expect("the table")
        .hstack(&[lists.into(), arrays.into()])
        .expect("columns of three rows")
}

fn is_in(receiver: &str, other: Value) -> Value {
    field_then(
        receiver,
        vec![call("Expr.is_in", json!({ "other": other }))],
    )
}

#[test]
fn a_key_that_tests_membership_in_a_list_is_elementwise_as_polars_takes_it() {
    // Python Polars 2.2.0: `a.is_in(l)` of a column of lists or of arrays is met row by row, so
    // a join may key on it and on what follows it, where a plain column, which is a set, and one
    // list that an aggregate gives, are no key. Each case is asked of Polars by running the join.
    let keyed = |left_on: Value| keys(vec![left_on], vec![field("rf")]);
    let cases = [
        ("a list column", keyed(is_in("a", field("l"))), true),
        ("an array column", keyed(is_in("a", field("arr"))), true),
        (
            "a list column and a test of the result",
            keyed(field_then(
                "a",
                vec![
                    call("Expr.is_in", json!({ "other": field("l") })),
                    call("Expr.not_", json!({})),
                ],
            )),
            true,
        ),
        ("a plain column", keyed(is_in("a", field("a"))), false),
        // Polars folds the values of a literal gathered into a list into a literal, which no
        // aggregate is; a column gathered is an aggregate, which a key may not hold beside one.
        (
            "a literal imploded",
            keyed(is_in(
                "a",
                chain(
                    call(
                        "polars.lit",
                        json!({ "value": { "integer": 1 }, "dtype": { "dtype": "Int64" } }),
                    ),
                    vec![call("Expr.implode", json!({}))],
                ),
            )),
            true,
        ),
        // What a method made of the literal is not folded: the literal gathered is an aggregate.
        (
            "a literal made by a method, imploded",
            keyed(is_in(
                "a",
                chain(
                    call(
                        "polars.lit",
                        json!({ "value": { "integer": 1 }, "dtype": { "dtype": "Int64" } }),
                    ),
                    vec![call("Expr.abs", json!({})), call("Expr.implode", json!({}))],
                ),
            )),
            false,
        ),
        (
            "a column imploded",
            keyed(is_in(
                "a",
                field_then("a", vec![call("Expr.implode", json!({}))]),
            )),
            false,
        ),
        ("the first list", keyed(is_in("a", first("l"))), false),
        (
            "a list column of an aggregate",
            keyed(field_then(
                "a",
                vec![
                    call("Expr.sum", json!({})),
                    call("Expr.is_in", json!({ "other": field("l") })),
                ],
            )),
            false,
        ),
    ];
    let mut disagreements = Vec::new();
    for (name, arguments, allowed) in cases {
        let report = join_of(with_lists(), right(), arguments);
        let polars_allows = matches!(outcome(&report), Polars::Allows);
        let walk_allows = refused(walk(&report)).is_none();
        if polars_allows != allowed {
            disagreements.push(format!(
                "{name}: Polars does {polars_allows}, the case says {allowed}"
            ));
        }
        if walk_allows != polars_allows {
            disagreements.push(format!(
                "{name}: Polars allows it: {polars_allows}, the walk allows it: {walk_allows}, {:?}",
                refused(walk(&report))
            ));
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} cases disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}

#[test]
fn a_right_key_that_tests_membership_in_a_list_is_elementwise_as_polars_takes_it() {
    let report = join_of(
        right(),
        with_lists(),
        keys(vec![field("rf")], vec![is_in("a", field("l"))]),
    );
    assert!(
        matches!(outcome(&report), Polars::Allows),
        "Polars allows it: {:?}",
        outcome(&report)
    );
    assert_eq!(refused(walk(&report)), None);
}

#[test]
fn a_literal_imploded_twice_beside_a_column_is_no_aggregate_of_a_key_as_polars_takes_it() {
    // Python Polars 2.2.0 folds each gathering of a literal into a literal, so a key that holds one
    // beside a column is judged elementwise, and refused only for what it gives, a list of lists,
    // where it is paired with a column of another dtype.
    let literal = chain(
        call(
            "polars.lit",
            json!({ "value": { "integer": 1 }, "dtype": { "dtype": "Int64" } }),
        ),
        vec![
            call("Expr.implode", json!({})),
            call("Expr.implode", json!({})),
        ],
    );
    let key = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field_then(
                "a",
                vec![call("Expr.gt", json!({ "other": field("a") }))],
            )]) }),
        ),
        vec![call("When.then", json!({ "statement": literal }))],
    );
    let report = join_of(with_lists(), right(), keys(vec![key], vec![field("rf")]));
    let said = polars_reason(outcome(&report));
    assert!(!said.contains("elementwise"), "{said}");
    let source = refused(walk(&report)).map(|found| found.1);
    assert_ne!(
        source,
        Some(RefusalSource::Rule(Rule::KeyIsElementwise)),
        "the walk takes a literal gathered for an aggregate"
    );
}
