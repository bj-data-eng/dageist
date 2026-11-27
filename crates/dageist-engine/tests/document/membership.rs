//! `implode`, which gathers the values of an expression into one list, and `is_in`, which tests
//! whether each value is among the values of a list.
//!
//! The table has the whole numbers `a` (1, 2, 3) and `b` (2, 3, 4), the text `s` and `t`, and a
//! column `l` of lists of whole numbers (`[1, 2]`, `[3]`, `[4, 5]`), and a list of the whole
//! numbers 2 and 3 is connected as `wanted`. Every expected value is worked out by hand from what
//! Python Polars 2.2.0 gives for the same call.

use dageist_engine::allowed::RefusalSource;
use dageist_engine::document::{Document, Inputs, ListOrigin, Outcome, RunCause, StepId, Walk};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, field_then, many, step, whole};

fn table() -> DataFrame {
    let lists = Series::new(
        "l".into(),
        [vec![1_i64, 2], vec![3], vec![4, 5]].map(|values| Series::new("".into(), values)),
    );
    df!(
        "a" => [1_i64, 2, 3],
        "b" => [2_i64, 3, 4],
        "s" => ["x", "y", "z"],
        "t" => ["y", "z", "w"],
    )
    .expect("the table")
    .hstack(&[lists.into()])
    .expect("a column of three rows")
}

fn inputs() -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_list(
            "wanted",
            ListOrigin::Parameter,
            Series::new("wanted".into(), [2_i64, 3]),
        )
        .expect("a list");
    inputs
}

fn from_input(name: &str, then: Vec<Value>) -> Value {
    chain(json!({ "input": name }), then)
}

/// The document that selects `expression` as the column `out`.
fn selecting(expression: Value) -> String {
    document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![expression]) }),
    )])
}

fn walk_of(expression: Value) -> Walk {
    Document::read(&selecting(expression))
        .expect("a document")
        .steps()[0]
        .walk(0, table().schema(), &inputs())
}

/// The first column of the rows the document gives, as Polars writes them.
fn cells(expression: Value) -> Vec<Option<String>> {
    Document::read(&selecting(expression))
        .expect("a document")
        .preview(&inputs(), &table(), &StepId::new("s").expect("an id"), 100)
        .expect("the rows")
        .rows()
        .iter()
        .map(|row| row[0].clone())
        .collect()
}

fn cell_texts(cells: &[&str]) -> Vec<Option<String>> {
    cells.iter().map(|cell| Some((*cell).to_owned())).collect()
}

/// The dtype of the one column the document gives.
fn dtype_of(expression: Value) -> DataType {
    let report = Document::read(&selecting(expression))
        .expect("a document")
        .run_on(&inputs(), &table())
        .expect("the table has a schema");
    match report.steps()[0].outcome() {
        Outcome::Valid { schema, .. } => schema
            .iter()
            .next()
            .map(|(_, dtype)| dtype.clone())
            .expect("a column"),
        other => panic!("{other:?}"),
    }
}

fn implode() -> Value {
    call("Expr.implode", json!({}))
}

fn is_in(other: Value) -> Value {
    call("Expr.is_in", json!({ "other": other }))
}

#[test]
fn implode_gives_the_values_gathered_into_one_list() {
    let gathered = field_then("b", vec![implode()]);
    assert_eq!(
        dtype_of(gathered.clone()),
        DataType::List(Box::new(DataType::Int64))
    );
    assert_eq!(cells(gathered.clone()), cell_texts(&["[2, 3, 4]"]));
    assert_eq!(walk_of(gathered).refusal(), None);
}

#[test]
fn implode_given_the_flag_that_drops_the_order_gives_a_list_of_the_dtype_of_the_values() {
    let unordered = field_then(
        "t",
        vec![call(
            "Expr.implode",
            json!({ "maintain_order": { "flag": false } }),
        )],
    );
    assert_eq!(
        dtype_of(unordered),
        DataType::List(Box::new(DataType::String))
    );
}

#[test]
fn is_in_given_a_collection_imploded_tells_for_each_value_whether_it_is_among_them() {
    // 1, 2 and 3 among 2, 3 and 4.
    let among = field_then("a", vec![is_in(field_then("b", vec![implode()]))]);
    assert_eq!(walk_of(among.clone()).refusal(), None);
    assert_eq!(dtype_of(among.clone()), DataType::Boolean);
    assert_eq!(cells(among), cell_texts(&["false", "true", "true"]));
}

#[test]
fn is_in_given_a_list_column_tests_each_value_against_the_list_of_its_row() {
    // 1 in [1, 2], 2 in [3], 3 in [4, 5].
    let among = field_then("a", vec![is_in(field("l"))]);
    assert_eq!(walk_of(among.clone()).refusal(), None);
    assert_eq!(cells(among), cell_texts(&["true", "false", "false"]));
}

#[test]
fn is_in_tests_a_value_typed_in_against_a_collection_imploded() {
    let among = chain(
        call("polars.lit", json!({ "value": { "integer": 3 } })),
        vec![is_in(field_then("b", vec![implode()]))],
    );
    assert_eq!(walk_of(among.clone()).refusal(), None);
    assert_eq!(cells(among), cell_texts(&["true"]));
}

/// The rule that refuses `is_in` given what is nested as the value it follows is, which is a form
/// Polars deprecates.
const RULE: &str = "is-in-takes-a-list";

/// The refusal of the walk of `expression`, with the name of the rule that refused and the words
/// it said.
fn refused_by_a_rule(expression: Value) -> (String, String) {
    let walk = walk_of(expression);
    let refusal = walk.refusal().expect("a refusal").refusal();
    match refusal.source() {
        RefusalSource::Rule(rule) => (rule.name().to_owned(), refusal.reason().to_owned()),
        other => panic!("refused by {other:?}: {}", refusal.reason()),
    }
}

#[test]
fn is_in_given_what_is_nested_as_the_value_it_follows_is_refused_by_a_rule_that_says_to_give_a_list()
 {
    // Python Polars 2.2.0 warns that the form is ambiguous and deprecated, and asks for the
    // collection imploded, which gives the same answer.
    for (name, receiver, other) in [
        ("whole numbers", "a", "b"),
        ("text", "s", "t"),
        ("whole numbers and text", "a", "s"),
        ("lists", "l", "l"),
    ] {
        let given = field_then(receiver, vec![is_in(field(other))]);
        let (rule, said) = refused_by_a_rule(given.clone());
        assert_eq!(rule, RULE, "{name}");
        assert!(
            said.contains("implode") && said.contains("list"),
            "{name}: {said}"
        );
        let report = Document::read(&selecting(given))
            .expect("a document")
            .run_on(&inputs(), &table())
            .expect("the table has a schema");
        assert!(
            !matches!(report.steps()[0].outcome(), Outcome::Valid { .. }),
            "{name}: the step is run"
        );
    }
}

#[test]
fn is_in_given_a_number_typed_in_is_refused_by_the_same_rule() {
    // A number typed in is a scalar of the dtype it adapts to, which is nested as a column that is
    // no list is, whatever the dtype is.
    let given = field_then("a", vec![is_in(whole(2))]);
    let (rule, said) = refused_by_a_rule(given);
    assert_eq!(rule, RULE);
    assert!(said.contains("implode") && said.contains("list"), "{said}");
}

#[test]
fn a_value_typed_in_looked_up_in_a_value_typed_in_is_refused_and_not_run_into_a_panic() {
    // Python Polars 2.2.0 panics on `pl.lit(5).is_in(pl.lit(1))`. A number typed in without a
    // dtype adapts to what it meets, so the walk cannot tell its dtype, and the step is refused as
    // it is built, by the same rule, before Polars is asked of the call.
    let given = chain(
        call("polars.lit", json!({ "value": { "integer": 5 } })),
        vec![is_in(whole(1))],
    );
    let report = Document::read(&selecting(given))
        .expect("a document")
        .run_on(&inputs(), &table())
        .expect("the table has a schema");
    let Outcome::Refused(refusal) = report.steps()[0].outcome() else {
        panic!("the step is not refused: {:?}", report.steps()[0].outcome());
    };
    match refusal.cause() {
        RunCause::NotAllowed(refusal) => match refusal.source() {
            RefusalSource::Rule(rule) => assert_eq!(rule.name(), RULE),
            other => panic!("refused by {other:?}: {}", refusal.reason()),
        },
        other => panic!("refused for another cause: {other}"),
    }
}

#[test]
fn a_connected_list_given_to_is_in_is_its_values_looked_up_and_is_allowed() {
    let among = field_then("a", vec![is_in(from_input("wanted", vec![]))]);
    assert_eq!(walk_of(among.clone()).refusal(), None);
    assert_eq!(dtype_of(among.clone()), DataType::Boolean);
    assert_eq!(cells(among), cell_texts(&["false", "true", "true"]));
}

#[test]
fn a_chain_of_a_connected_list_given_to_is_in_is_refused_until_it_is_imploded() {
    let chained = |links: Vec<Value>| field_then("a", vec![is_in(from_input("wanted", links))]);
    let unique = || call("Expr.unique", json!({}));
    assert_eq!(refused_by_a_rule(chained(vec![unique()])).0, RULE);
    let imploded = chained(vec![unique(), implode()]);
    assert_eq!(walk_of(imploded.clone()).refusal(), None);
    assert_eq!(cells(imploded), cell_texts(&["false", "true", "true"]));
}

/// The name of the test below that runs in a process of its own.
const WARNING_TEST: &str = "membership::a_connected_list_given_to_is_in_makes_no_warning_of_polars";

/// Set in the process that the test below starts, to tell it to run the document.
const CHILD: &str = "DAGEIST_TEST_MEMBERSHIP_CHILD";

#[test]
fn a_connected_list_given_to_is_in_makes_no_warning_of_polars() {
    // Polars prints a warning on the standard error stream where `is_in` is given a collection
    // nested as the value it follows is, which a list of the values' own dtype is, so the list is
    // built imploded. The document is run in a process of its own to read what it prints.
    if std::env::var(CHILD).is_ok() {
        let among = field_then("a", vec![is_in(from_input("wanted", vec![]))]);
        assert_eq!(cells(among), cell_texts(&["false", "true", "true"]));
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("the test program"))
        .args([
            "--exact",
            WARNING_TEST,
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .env(CHILD, "1")
        .output()
        .expect("the test program starts again");
    let printed = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("running 1 test"),
        "the process did not run exactly the test\n{printed}"
    );
    assert!(
        output.status.success(),
        "the document is not run\n{printed}"
    );
    assert!(!printed.contains("deprecated"), "Polars warned\n{printed}");
}

#[test]
fn a_list_column_looked_up_in_a_collection_of_lists_is_not_judged_by_the_tables() {
    // Python Polars 2.2.0 takes `l.is_in(l.implode().implode())`: a list is looked up in a list of
    // lists. No class of the tables is a list of lists, so the engine does not offer it, and says
    // that the tables do not measure it, and not that Polars refuses a collection that is no list.
    let looked_up = field_then(
        "l",
        vec![is_in(field_then("l", vec![implode(), implode()]))],
    );
    let (rule, said) = refused_by_a_rule(looked_up);
    assert_eq!(rule, "element-dtype-is-measured");
    assert!(said.contains("Expr.is_in"), "{said}");
}
