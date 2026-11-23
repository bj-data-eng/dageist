//! What the methods that order values, compare a value with the ones around it, rank it or count
//! what is distinct do in a step, on the claim-timing sample, whose `row_days` column holds
//! `0, 2, 2, 0, 2, 2, 1` and whose `timestamp` column orders its seven rows as the rows `3, 0, 6,
//! 4, 1, 2, 5`. The expected cells are worked out by hand from those, and the dtypes are what
//! Python Polars documents for each method. The walk's part uses the table `a`, `b`, `f` and `k`
//! of the other walk tests.

use dageist_engine::allowed::{
    After, Context, Outer, Place as OfferPlace, RefusalSource, Representative, Rule, Shape,
};
use dageist_engine::document::{Document, Inputs, Outcome, Report, Segment, StepId, Walk};
use polars::prelude::{DataFrame, DataType, IntoLazy, df};
use serde_json::{Value, json};

use crate::claims::{claims, inputs};
use crate::support::{call, chain, document, field, field_then, many, named, step, whole};

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

/// The dtype of the column `out` that the step gives, which it must give.
fn dtype_of(written: &str) -> DataType {
    let report = report_of(written);
    match report.steps()[0].outcome() {
        Outcome::Valid { schema, .. } => schema
            .get("out")
            .unwrap_or_else(|| panic!("no column out in {schema:?}"))
            .clone(),
        other => panic!("{other:?}"),
    }
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

fn texts(cells: &[&str]) -> Vec<Option<String>> {
    cells.iter().map(|cell| Some((*cell).to_owned())).collect()
}

/// The column `name` followed by the one method `entry` given `arguments`.
fn method(name: &str, entry: &str, arguments: Value) -> Value {
    field_then(name, vec![call(entry, arguments)])
}

fn flag(on: bool) -> Value {
    json!({ "flag": on })
}

fn word(choice: &str) -> Value {
    json!({ "choice": choice })
}

/// `row_days`, with a null where it is 2: `0, null, null, 0, null, null, 1`.
fn with_gaps() -> Value {
    let gap = chain(
        call(
            "polars.lit",
            json!({ "value": { "flag": null }, "dtype": { "dtype": "Int64" } }),
        ),
        vec![],
    );
    chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field_then(
                "row_days",
                vec![call("Expr.eq", json!({ "other": whole(2) }))],
            )]) }),
        ),
        vec![
            call("When.then", json!({ "statement": gap })),
            call("Then.otherwise", json!({ "statement": field("row_days") })),
        ],
    )
}

/// Chains `then` after the expression `start` gives.
fn after(start: &Value, then: Vec<Value>) -> Value {
    let mut chained = start.clone();
    chained["expression"]["then"]
        .as_array_mut()
        .expect("links")
        .extend(then);
    chained
}

#[test]
fn a_sort_gives_the_values_in_order_and_descending_turns_it_round() {
    let ascending = selecting(method("row_days", "Expr.sort", json!({})));
    assert_eq!(dtype_of(&ascending), DataType::Int64);
    assert_eq!(
        cells(&ascending),
        texts(&["0", "0", "1", "2", "2", "2", "2"])
    );
    let descending = selecting(method(
        "row_days",
        "Expr.sort",
        json!({ "descending": flag(true) }),
    ));
    assert_eq!(
        cells(&descending),
        texts(&["2", "2", "2", "2", "1", "0", "0"])
    );
}

#[test]
fn a_sort_by_orders_a_column_by_another_and_more_keys_break_its_ties() {
    // The rows by `timestamp` are 3, 0, 6, 4, 1, 2, 5, and `step_to` of them is Intake, Intake,
    // Review, Review, Pending, Complete, Complete.
    let by_time = selecting(method(
        "step_to",
        "Expr.sort_by",
        json!({ "by": many(vec![field("timestamp")]) }),
    ));
    assert_eq!(dtype_of(&by_time), DataType::String);
    assert_eq!(
        cells(&by_time),
        texts(&[
            "Intake", "Intake", "Review", "Review", "Pending", "Complete", "Complete",
        ])
    );
    let latest_first = selecting(method(
        "step_to",
        "Expr.sort_by",
        json!({ "by": many(vec![field("timestamp")]), "descending": flag(true) }),
    ));
    assert_eq!(
        cells(&latest_first),
        texts(&[
            "Complete", "Complete", "Pending", "Review", "Review", "Intake", "Intake",
        ])
    );
    // The days, then the time within a day: rows 0 and 3 have none, so 3 is first, then 0, then
    // the one day of row 6, then rows 4, 1, 2 and 5 by time.
    let by_two = selecting(method(
        "dcn",
        "Expr.sort_by",
        json!({ "by": many(vec![field("row_days")]), "more_by": many(vec![field("timestamp")]) }),
    ));
    assert_eq!(
        cells(&by_two),
        texts(&[
            "A-1001", "A-1002", "A-1001", "A-1002", "A-1001", "A-1002", "A-1001",
        ])
    );
}

#[test]
fn a_reverse_gives_the_values_from_the_last() {
    let reversed = selecting(method("row_days", "Expr.reverse", json!({})));
    assert_eq!(dtype_of(&reversed), DataType::Int64);
    assert_eq!(
        cells(&reversed),
        texts(&["1", "2", "2", "0", "2", "2", "0"])
    );
}

#[test]
fn a_difference_is_a_value_less_the_one_n_rows_before_it_and_null_where_there_is_none() {
    let by_one = selecting(method("row_days", "Expr.diff", json!({})));
    assert_eq!(dtype_of(&by_one), DataType::Int64);
    assert_eq!(
        cells(&by_one)
            .iter()
            .map(|cell| cell.as_deref())
            .collect::<Vec<_>>(),
        [
            None,
            Some("2"),
            Some("0"),
            Some("-2"),
            Some("2"),
            Some("0"),
            Some("-1")
        ]
    );
    let by_two = selecting(method("row_days", "Expr.diff", json!({ "n": whole(2) })));
    assert_eq!(
        cells(&by_two)
            .iter()
            .map(|cell| cell.as_deref())
            .collect::<Vec<_>>(),
        [
            None,
            None,
            Some("2"),
            Some("-2"),
            Some("0"),
            Some("2"),
            Some("-1")
        ]
    );
}

#[test]
fn a_change_is_the_part_of_the_value_n_rows_before_that_a_value_has_grown_by() {
    // The days plus one are `1, 3, 3, 1, 3, 3, 2`: the second is 2 times the first more, the
    // third the same as the second, and the seventh a third less than the sixth.
    let days_plus_one = after(
        &field("row_days"),
        vec![call("Expr.add", json!({ "other": whole(1) }))],
    );
    let changes = selecting(after(
        &days_plus_one,
        vec![call("Expr.pct_change", json!({}))],
    ));
    assert_eq!(dtype_of(&changes), DataType::Float64);
    let shown = cells(&changes);
    assert_eq!(shown[0], None);
    let found: Vec<f64> = shown[1..]
        .iter()
        .map(|cell| cell.as_deref().expect("a number").parse().expect("a float"))
        .collect();
    let wanted = [2.0, 0.0, -2.0 / 3.0, 2.0, 0.0, -1.0 / 3.0];
    for (found, wanted) in found.iter().zip(wanted) {
        assert!((found - wanted).abs() < 1e-5, "{found} is not {wanted}");
    }
}

#[test]
fn a_rank_is_the_place_in_order_and_the_method_says_how_ties_share_it() {
    // The days are 0 twice, 1 once and 2 four times, so the places are 1 and 2, 3, and 4 to 7.
    let ranks = |arguments: Value| selecting(method("row_days", "Expr.rank", arguments));
    let average = ranks(json!({}));
    assert_eq!(dtype_of(&average), DataType::Float64);
    assert_eq!(
        cells(&average),
        texts(&["1.5", "5.5", "5.5", "1.5", "5.5", "5.5", "3.0"])
    );
    let by_method = |method: &str, expected: [&str; 7]| {
        let written = ranks(json!({ "method": word(method) }));
        assert_eq!(dtype_of(&written), DataType::UInt32, "{method}");
        assert_eq!(cells(&written), texts(&expected), "{method}");
    };
    by_method("min", ["1", "4", "4", "1", "4", "4", "3"]);
    by_method("max", ["2", "7", "7", "2", "7", "7", "3"]);
    by_method("dense", ["1", "3", "3", "1", "3", "3", "2"]);
    by_method("ordinal", ["1", "4", "5", "2", "6", "7", "3"]);
    let descending = ranks(json!({ "method": word("dense"), "descending": flag(true) }));
    assert_eq!(
        cells(&descending),
        texts(&["3", "1", "1", "3", "1", "1", "2"])
    );
    // A random rank of a seed is the same each time, and a place of the seven.
    let seeded = ranks(json!({
        "method": word("random"),
        "seed": { "integer": 7 },
    }));
    assert_eq!(dtype_of(&seeded), DataType::UInt32);
    assert_eq!(cells(&seeded), cells(&seeded));
}

#[test]
fn an_arg_sort_gives_the_row_each_place_in_order_is_from() {
    let by_time = selecting(method("timestamp", "Expr.arg_sort", json!({})));
    assert_eq!(dtype_of(&by_time), DataType::UInt32);
    assert_eq!(cells(&by_time), texts(&["3", "0", "6", "4", "1", "2", "5"]));
    let latest = selecting(method(
        "timestamp",
        "Expr.arg_sort",
        json!({ "descending": flag(true) }),
    ));
    assert_eq!(cells(&latest), texts(&["5", "2", "1", "4", "6", "0", "3"]));
}

#[test]
fn a_run_id_counts_up_each_time_the_value_changes() {
    let runs = selecting(method("row_days", "Expr.rle_id", json!({})));
    assert_eq!(dtype_of(&runs), DataType::UInt32);
    assert_eq!(cells(&runs), texts(&["0", "1", "1", "2", "3", "3", "4"]));
}

#[test]
fn a_count_of_distinct_values_counts_a_null_and_a_count_does_not() {
    let distinct = selecting(method("row_days", "Expr.n_unique", json!({})));
    assert_eq!(dtype_of(&distinct), DataType::UInt32);
    assert_eq!(cells(&distinct), texts(&["3"]));
    let counted = selecting(method("row_days", "Expr.count", json!({})));
    assert_eq!(dtype_of(&counted), DataType::UInt32);
    assert_eq!(cells(&counted), texts(&["7"]));
    // With the gaps there are three distinct values, 0, 1 and the null, and three that are not
    // null.
    let gaps_distinct = selecting(after(&with_gaps(), vec![call("Expr.n_unique", json!({}))]));
    assert_eq!(cells(&gaps_distinct), texts(&["3"]));
    let gaps_counted = selecting(after(&with_gaps(), vec![call("Expr.count", json!({}))]));
    assert_eq!(cells(&gaps_counted), texts(&["3"]));
}

// ---------------------------------------------------------------------------
// The walk

fn source() -> DataFrame {
    df!(
        "a" => [1_i64, 2, 3],
        "b" => ["x", "y", "z"],
        "f" => [true, false, true],
        "k" => [1_i64, 1, 2],
    )
    .expect("the table")
}

fn select_walk(expression: Value) -> Walk {
    let written = document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![expression]) }),
    )]);
    Document::read(&written).expect("a document").steps()[0].walk(
        0,
        source().schema(),
        &Inputs::new(),
    )
}

/// Where a link added after the select item would stand: what the item gives.
fn gives(walk: &Walk) -> Option<OfferPlace> {
    walk.expressions()
        .iter()
        .find(|expression| {
            expression.place().path() == [Segment::Parameter("exprs".to_owned()), Segment::Item(0)]
        })
        .and_then(|expression| expression.places().last().copied().flatten())
}

fn select_after(dtype: Representative, shape: Shape) -> Option<OfferPlace> {
    Some(OfferPlace::new(
        Context::step(Outer::Select),
        After::Value { dtype, shape },
    ))
}

#[test]
fn the_walk_gives_a_value_for_each_row_after_a_method_that_keeps_the_rows_and_one_after_a_count() {
    let each = |entry: &str, arguments: Value, dtype: Representative| {
        assert_eq!(
            gives(&select_walk(method("a", entry, arguments))),
            select_after(dtype, Shape::Many),
            "{entry}"
        );
    };
    each("Expr.sort", json!({}), Representative::Int64);
    each("Expr.reverse", json!({}), Representative::Int64);
    each("Expr.diff", json!({}), Representative::Int64);
    each("Expr.pct_change", json!({}), Representative::Float64);
    each("Expr.rank", json!({}), Representative::Float64);
    each(
        "Expr.rank",
        json!({ "method": word("dense") }),
        Representative::UInt32,
    );
    each("Expr.arg_sort", json!({}), Representative::UInt32);
    each("Expr.rle_id", json!({}), Representative::UInt32);
    each(
        "Expr.sort_by",
        json!({ "by": many(vec![field("k")]) }),
        Representative::Int64,
    );
    for entry in ["Expr.n_unique", "Expr.count"] {
        assert_eq!(
            gives(&select_walk(method("a", entry, json!({})))),
            select_after(Representative::UInt32, Shape::One),
            "{entry}"
        );
    }
}

#[test]
fn a_sort_by_of_one_value_by_a_column_is_a_value_for_each_row_as_polars_gives() {
    // The sum of `a` sorted by `k` is the sum met by each of the rows of `k`, and a column sorted
    // by the sum is the column.
    let summed = after(
        &field("a"),
        vec![
            call("Expr.sum", json!({})),
            call("Expr.sort_by", json!({ "by": many(vec![field("k")]) })),
        ],
    );
    assert_eq!(
        gives(&select_walk(summed)),
        select_after(Representative::Int64, Shape::Many)
    );
    let by_the_sum = method(
        "a",
        "Expr.sort_by",
        json!({ "by": many(vec![field_then("k", vec![call("Expr.sum", json!({}))])]) }),
    );
    assert_eq!(
        gives(&select_walk(by_the_sum)),
        select_after(Representative::Int64, Shape::Many)
    );
    let both_one = after(
        &field("a"),
        vec![
            call("Expr.sum", json!({})),
            call(
                "Expr.sort_by",
                json!({ "by": many(vec![field_then("k", vec![call("Expr.sum", json!({}))])]) }),
            ),
        ],
    );
    assert_eq!(
        gives(&select_walk(both_one)),
        select_after(Representative::Int64, Shape::One)
    );
}

/// The walk of a join of the table on `ra` with the key `key` on the left.
fn join_walk(key: Value) -> Walk {
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "other",
            "Other",
            df!("ra" => [1_i64, 2, 3]).expect("the other table").lazy(),
        )
        .expect("an input");
    let written = document(vec![step(
        "s",
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(vec![key]),
            "right_on": many(vec![field("ra")]),
        }),
    )]);
    Document::read(&written).expect("a document").steps()[0].walk(0, source().schema(), &inputs)
}

#[test]
fn a_method_that_is_not_elementwise_is_refused_by_polars_as_a_join_key() {
    for (entry, arguments) in [
        ("Expr.sort", json!({})),
        ("Expr.reverse", json!({})),
        ("Expr.rank", json!({ "method": word("dense") })),
        ("Expr.arg_sort", json!({})),
        ("Expr.diff", json!({})),
        ("Expr.rle_id", json!({})),
    ] {
        let walk = join_walk(method("a", entry, arguments));
        let refusal = walk
            .refusal()
            .unwrap_or_else(|| panic!("{entry} is no join key"));
        assert!(
            matches!(refusal.refusal().source(), RefusalSource::Polars(_)),
            "{entry}: {:?}",
            refusal.refusal().source()
        );
    }
}

#[test]
fn a_count_beside_a_column_is_refused_in_a_join_key_as_any_aggregate_is() {
    // The count of the distinct values of `a` is one value, which a join's key may not hold beside
    // the column `k`.
    for entry in ["Expr.n_unique", "Expr.count"] {
        let key = field_then(
            "a",
            vec![
                call(entry, json!({})),
                call("Expr.gt", json!({ "other": field("k") })),
            ],
        );
        let walk = join_walk(key);
        let refusal = walk
            .refusal()
            .unwrap_or_else(|| panic!("{entry} beside a column is no join key"));
        assert_eq!(
            refusal.refusal().source(),
            RefusalSource::Rule(Rule::KeyIsElementwise),
            "{entry}"
        );
    }
}
