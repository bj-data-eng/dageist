//! A step that the tables of allowed combinations refuse is not run, whether or not Polars
//! would plan it: it is reported refused, at the place of the walk's refusal and with its words,
//! and it is neither planned nor collected, so what Polars panics on in a step the tables refuse
//! never reaches a caller.
//!
//! The steps are those of a campaign of the fuzz target that Polars panicked on, and steps that
//! Polars plans where a part of them is never evaluated. Each is asked of Polars apart from the
//! document where the test says Polars allows it.

use std::panic::{AssertUnwindSafe, catch_unwind};

use dageist_engine::allowed::RefusalSource;
use dageist_engine::document::{
    Document, Inputs, Outcome, RowsError, RunCause, StepId, StepRows, WalkRefusal,
};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, field_then, many, step};

/// The three rows of a table of the fuzz target: the columns `a`, `b`, `f`, `k`, `g`, `h` and `u`,
/// named by `names`, and `key`, a column of nulls.
fn side(names: [&str; 6], a: Series) -> DataFrame {
    let [a_name, b, f, k, g, h] = names;
    DataFrame::new(
        3,
        vec![
            a.with_name(a_name.into()).into(),
            Series::new(b.into(), [Some(1_i32), None, Some(3)])
                .cast(&DataType::Date)
                .expect("whole numbers cast to dates")
                .into(),
            Series::new(f.into(), [Some(true), Some(false), Some(true)]).into(),
            Series::new(k.into(), [1_i64, 1, 2]).into(),
            Series::new(g.into(), ["x", "y", "x"]).into(),
            Series::new(h.into(), ["x", "x", "y"]).into(),
            Series::new("u".into(), [1_i64, 2, 3]).into(),
            Series::full_null("key".into(), 3, &DataType::Int64).into(),
        ],
    )
    .expect("columns of three rows")
}

fn whole_numbers() -> Series {
    Series::new("a".into(), [1_i64, 2, 3])
}

fn lists_of_booleans() -> Series {
    ListChunked::from_iter([
        Some(Series::new("".into(), [true, false])),
        None,
        Some(Series::new("".into(), [true])),
    ])
    .into_series()
}

fn arrays_of_big_numbers() -> Series {
    ListChunked::from_iter([
        Some(Series::new("".into(), [1_i64, 2])),
        None,
        Some(Series::new("".into(), [3_i64, 4])),
    ])
    .into_series()
    .cast(&DataType::Array(Box::new(DataType::UInt128), 2))
    .expect("lists of two cast to arrays of two")
}

fn inputs() -> Inputs {
    let mut inputs = Inputs::new();
    let other = side(["ra", "rb", "rf", "rk", "rg", "rh"], whole_numbers());
    inputs
        .add_table("other", "Other", other.lazy())
        .expect("a first input");
    let values = DataFrame::new(2, vec![Series::new("h".into(), ["x", "y"]).into()])
        .expect("a column of two rows");
    inputs
        .add_table("on_h", "Columns", values.lazy())
        .expect("a second input");
    inputs
}

fn working(a: Series) -> DataFrame {
    side(["a", "b", "f", "k", "g", "h"], a)
}

/// The table `working` makes, with `key` of nulls of the dtype `key`, which the keys of a join
/// are of.
fn working_keyed(a: Series, key: &DataType) -> DataFrame {
    let mut table = working(a);
    table
        .replace("key", Series::full_null("key".into(), 3, key).into())
        .expect("a column of three rows");
    table
}

fn read(step: Value) -> (Document, StepId) {
    let document = Document::read(&document(vec![step])).expect("a document");
    let id = document.steps()[0].id().clone();
    (document, id)
}

/// The document run on `working` as the host runs it, and its rows asked for: the walk's
/// refusal, and that the step is reported refused at its place and with its words, not counted,
/// and not given rows, and that neither reaches a panic of Polars'.
fn assert_refused_and_not_collected(step: Value, working: &DataFrame) -> WalkRefusal {
    let (document, id) = read(step);
    let collected = catch_unwind(AssertUnwindSafe(|| {
        document.rows(&inputs(), working, &id, 100)
    }));
    let Ok(collected) = collected else {
        panic!("asking for the rows reached a panic of Polars'");
    };
    let Err(RowsError::Refused(refused)) = collected else {
        panic!("the rows of a step the walk refuses are refused: {collected:?}");
    };
    let ran = catch_unwind(AssertUnwindSafe(|| document.run_on(&inputs(), working)));
    let report = ran
        .unwrap_or_else(|_| panic!("running the document reached a panic of Polars'"))
        .expect("the table has a schema");
    let reported = &report.steps()[0];
    let walked = reported
        .walk()
        .refusal()
        .expect("the walk refuses the step")
        .clone();
    let Outcome::Refused(refusal) = reported.outcome() else {
        panic!(
            "the step the walk refuses is {:?}, not refused",
            reported.outcome()
        );
    };
    assert_eq!(refusal.place(), walked.place(), "{refusal}");
    assert_eq!(refused.place(), walked.place(), "{refused}");
    let RunCause::NotAllowed(said) = refusal.cause() else {
        panic!("the step is refused with the words of the walk: {refusal}");
    };
    assert_eq!(said, walked.refusal());
    assert!(
        matches!(reported.rows(), StepRows::NotCounted),
        "a refused step is not collected: {:?}",
        reported.rows()
    );
    assert!(reported.schema().is_none());
    walked
}

/// The sort key `when(b).then(g).min()`: the predicate is a date, which the rule of a condition's
/// predicate refuses, and Polars drops a sort key of one value without looking at it.
fn a_date_as_the_predicate_of_a_sort_key() -> Value {
    let key = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("b")]) }),
        ),
        vec![
            call("When.then", json!({ "statement": field("g") })),
            call("Expr.min", json!({})),
        ],
    );
    step("s", "LazyFrame.sort", json!({ "by": many(vec![key]) }))
}

/// The sort key `when(f.last() < a.first()).then(f.sum())`, with `a` of dates: a boolean is
/// compared with a date, which the tables record Polars refusing, and Polars drops a sort key of
/// one value without evaluating it.
fn a_boolean_compared_with_a_date_in_a_sort_key() -> Value {
    let predicate = field_then(
        "f",
        vec![
            call("Expr.last", json!({})),
            call(
                "Expr.lt",
                json!({ "other": field_then("a", vec![call("Expr.first", json!({}))]) }),
            ),
        ],
    );
    let key = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![predicate]) }),
        ),
        vec![call(
            "When.then",
            json!({ "statement": field_then("f", vec![call("Expr.sum", json!({}))]) }),
        )],
    );
    step("s", "LazyFrame.sort", json!({ "by": many(vec![key]) }))
}

fn dates() -> Series {
    Series::new("a".into(), [Some(1_i32), None, Some(3)])
        .cast(&DataType::Date)
        .expect("whole numbers cast to dates")
}

#[test]
fn a_step_refused_by_a_rule_that_polars_plans_is_refused_and_not_collected() {
    let working = working(whole_numbers());
    // Polars plans it: the sort key is one value, which it drops.
    let asked = working
        .clone()
        .lazy()
        .sort_by_exprs(
            [when(col("b")).then(col("g")).otherwise(lit(NULL)).min()],
            SortMultipleOptions::default(),
        )
        .collect_schema();
    assert!(asked.is_ok(), "Polars plans the step: {asked:?}");
    let walked =
        assert_refused_and_not_collected(a_date_as_the_predicate_of_a_sort_key(), &working);
    assert!(
        matches!(walked.refusal().source(), RefusalSource::Rule(_)),
        "a rule refuses it: {:?}",
        walked.refusal().source()
    );
}

#[test]
fn a_step_the_tables_record_polars_refusing_that_polars_plans_is_refused_and_not_collected() {
    let working = working(dates());
    let asked = working
        .clone()
        .lazy()
        .sort_by_exprs(
            [when(col("f").last().lt(col("a").first()))
                .then(col("f").sum())
                .otherwise(lit(NULL))],
            SortMultipleOptions::default(),
        )
        .collect_schema();
    assert!(asked.is_ok(), "Polars plans the step: {asked:?}");
    let walked =
        assert_refused_and_not_collected(a_boolean_compared_with_a_date_in_a_sort_key(), &working);
    assert!(
        matches!(walked.refusal().source(), RefusalSource::Polars(_)),
        "the tables record Polars refusing it: {:?}",
        walked.refusal().source()
    );
}

/// A join of the other table on `right_on` of the join's right keys, with `key` on the left.
fn a_join(left_on: Vec<Value>, right_on: Vec<Value>, options: Value) -> Value {
    let mut arguments = json!({
        "other": { "table": "other" },
        "left_on": many(left_on),
        "right_on": many(right_on),
    });
    for (name, value) in options.as_object().expect("options").clone() {
        arguments[name] = value;
    }
    step("s", "LazyFrame.join", arguments)
}

#[test]
fn a_bitwise_and_of_whole_numbers_and_text_in_a_join_key_is_not_run_into_a_panic() {
    // Polars plans it and panics where it joins the rows, in the thread of its own that builds
    // the table of keys. The tables record the panic.
    let key = field_then(
        "rk",
        vec![call(
            "Expr.and_",
            json!({ "others": many(vec![field("rh")]) }),
        )],
    );
    let walked = assert_refused_and_not_collected(
        a_join(
            vec![field("key")],
            vec![key],
            json!({ "how": { "choice": "right" }, "force_parallel": { "flag": true } }),
        ),
        // The keys are what Polars gives the other side's key, text.
        &working_keyed(whole_numbers(), &DataType::String),
    );
    assert!(matches!(
        walked.refusal().source(),
        RefusalSource::Polars(_)
    ));
}

#[test]
fn a_bitwise_and_of_a_count_and_text_in_a_sort_key_is_not_run_into_a_panic() {
    // Polars plans it and panics in its top-k node, in a thread of its own.
    let key = field_then(
        "f",
        vec![
            call("Expr.arg_max", json!({})),
            call("Expr.and_", json!({ "others": many(vec![field("g")]) })),
        ],
    );
    assert_refused_and_not_collected(
        step("s", "LazyFrame.sort", json!({ "by": many(vec![key]) })),
        &working(whole_numbers()),
    );
}

#[test]
fn a_moment_made_of_a_quotient_in_a_join_key_is_not_run_into_a_panic() {
    // Polars plans it and panics in its time code where the rows are joined.
    let moment = chain(
        call(
            "polars.datetime",
            json!({
                "year": field("u"),
                "month": field("u"),
                "day": field_then(
                    "u",
                    vec![call("Expr.truediv", json!({ "other": field("u") }))],
                ),
                "time_unit": { "choice": "ns" },
                "ambiguous": { "choice": "raise" },
                "time_zone": { "time_zone": "Asia/Kolkata" },
            }),
        ),
        vec![],
    );
    assert_refused_and_not_collected(
        step(
            "s",
            "LazyFrame.join",
            json!({
                "other": { "table": "other" },
                "on": many(vec![moment]),
                "nulls_equal": { "flag": false },
                "force_parallel": { "flag": true },
            }),
        ),
        &working(whole_numbers()),
    );
}

#[test]
fn a_comparison_with_the_sum_of_a_null_and_a_list_is_not_run_into_a_panic() {
    // `a.list.std(0) != null + a` of lists of booleans: Polars plans it and panics in its
    // arithmetic of lists where the rows are filtered.
    let predicate = field_then(
        "a",
        vec![
            call("Expr.list.std", json!({ "ddof": { "integer": 0 } })),
            call(
                "Expr.ne",
                json!({ "other": chain(
                    call("polars.lit", json!({ "value": { "flag": null } })),
                    vec![call("Expr.add", json!({ "other": field("a") }))],
                ) }),
            ),
        ],
    );
    assert_refused_and_not_collected(
        step(
            "s",
            "LazyFrame.filter",
            json!({ "predicates": many(vec![predicate]) }),
        ),
        &working(lists_of_booleans()),
    );
}

#[test]
fn a_running_sum_of_the_first_of_an_array_in_a_pivot_is_not_run_into_a_panic() {
    // The schema of the aggregate says whole numbers and the rows give wider ones, so that
    // Polars panics where it builds the lists of a group: the tables record the difference.
    let aggregate = chain(
        call("polars.element", json!({})),
        vec![
            call("Expr.arr.first", json!({})),
            call("Expr.cum_sum", json!({ "reverse": { "flag": true } })),
        ],
    );
    assert_refused_and_not_collected(
        step(
            "s",
            "LazyFrame.pivot",
            json!({
                "on": { "columns": ["h"] },
                "on_columns": { "table": "on_h" },
                "index": { "columns": ["k"] },
                "values": { "columns": ["a"] },
                "aggregate_function": aggregate,
            }),
        ),
        &working(arrays_of_big_numbers()),
    );
}

#[test]
fn a_step_after_one_the_walk_refuses_is_not_run() {
    let refused = a_date_as_the_predicate_of_a_sort_key();
    let after = step("after", "LazyFrame.select", json!({}));
    let read = Document::read(&document(vec![refused, after])).expect("a document");
    let working = working(whole_numbers());
    let report = read
        .run_on(&inputs(), &working)
        .expect("the table has a schema");
    assert!(matches!(report.steps()[0].outcome(), Outcome::Refused(_)));
    let Outcome::NotRun { blocked_by } = report.steps()[1].outcome() else {
        panic!(
            "the step after is not run: {:?}",
            report.steps()[1].outcome()
        );
    };
    assert_eq!(blocked_by.as_str(), "s");
}
