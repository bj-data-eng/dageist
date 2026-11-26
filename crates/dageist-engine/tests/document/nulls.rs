//! What the walk says of the nulls, checks and conversions, against what Polars does with the
//! same step, which each test asks by running it.
//!
//! The table has `a` (`Int64` with no null), `b` (`String`), `f` (`Boolean`) and `k` (`Int64`),
//! and the table it joins `ra` and `rk` (`Int64`), so that what the walk predicts of a value is
//! worked out by hand from what Polars is documented to do with each.

use dageist_engine::allowed::{
    After, Context, Outer, Place as OfferPlace, RefusalSource, Representative, Rule, Shape,
};
use dageist_engine::document::{Document, Inputs, Outcome, Report, Segment, Walk};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::ask_polars::{self, Connected};
use crate::support::{call, chain, col_call, document, field, field_then, many, step, whole};

fn source() -> DataFrame {
    df!(
        "a" => [1_i64, 2, 3],
        "b" => ["x", "y", "z"],
        "f" => [true, false, true],
        "k" => [1_i64, 1, 2],
    )
    .expect("the table")
}

fn other() -> DataFrame {
    df!("ra" => [1_i64, 2, 3], "rk" => [1_i64, 2, 3]).expect("the other table")
}

fn inputs() -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_table("other", "Other", other().lazy())
        .expect("a first input");
    inputs
}

/// What the step engine reported of a step, and what Polars said of it when asked apart from the
/// engine, which does not run a step the walk refuses.
struct Ran {
    report: Report,
    polars: Option<String>,
}

fn report_of(entry: &str, arguments: Value) -> Ran {
    let mut given = arguments;
    if entry == "LazyFrame.join" {
        given["other"] = json!({ "table": "other" });
    }
    ran_on(
        &Document::read(&document(vec![step("s", entry, given)])).expect("a document"),
        &source(),
    )
}

/// What the step engine reports of `document` run on `working`, and what Polars says of it.
fn ran_on(document: &Document, working: &DataFrame) -> Ran {
    let tables = |name: &str| (name == "other").then(|| other().lazy());
    let step = &document.steps()[0];
    let polars = ask_polars::guarded(step, || {
        ask_polars::refusal_of(step, 0, working, Connected::tables(&tables))
    });
    let report = document
        .run_on(&inputs(), working)
        .expect("the table has a schema");
    Ran { report, polars }
}

fn walk(ran: &Ran) -> &Walk {
    ran.report.steps()[0].walk()
}

fn outcome(ran: &Ran) -> &Outcome {
    ran.report.steps()[0].outcome()
}

/// What Polars said when it refused the step, whether it worked out the schema or ran the three
/// rows of the table.
fn polars_says(ran: &Ran) -> Option<String> {
    ran.polars.clone()
}

fn select(items: Vec<Value>) -> Value {
    json!({ "exprs": many(items) })
}

fn item(at: usize) -> Vec<Segment> {
    vec![Segment::Parameter("exprs".to_owned()), Segment::Item(at)]
}

fn dtype(word: &str) -> Value {
    json!({ "dtype": word })
}

fn choice(word: &str) -> Value {
    json!({ "choice": word })
}

/// The Polars dtype of the one column `report` gives, which the step must have run for.
fn dtype_of_the_column(ran: &Ran) -> DataType {
    let step = &ran.report.steps()[0];
    assert!(
        matches!(step.outcome(), Outcome::Valid { .. }),
        "{:?}",
        step.outcome()
    );
    step.schema()
        .expect("a schema")
        .iter()
        .next()
        .map(|(_, dtype)| dtype.clone())
        .expect("a column")
}

#[test]
fn a_replacement_given_many_new_values_beside_one_old_value_is_refused_by_a_rule_as_polars_does() {
    // The sum of a column is one value, and the column `a` is three.
    let one_old = call(
        "Expr.replace",
        json!({ "old": field_then("k", vec![call("Expr.sum", json!({}))]), "new": field("a") }),
    );
    let report = report_of(
        "LazyFrame.select",
        select(vec![chain(
            json!({ "entry": "polars.col", "arguments": { "name": { "columns": ["b"] } } }),
            vec![one_old],
        )]),
    );
    let refusal = walk(&report).refusal().expect("the walk refuses it");
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Rule(Rule::ReplacementValuesMatchInNumber)
    );
    assert_eq!(
        refusal.place().path(),
        [item(0), vec![Segment::Link(0)]].concat()
    );
    assert!(
        polars_says(&report)
            .is_some_and(|reason| reason.contains("must have the same length as `old`")),
        "{:?}",
        polars_says(&report)
    );
}

#[test]
fn a_replacement_given_one_new_value_beside_many_old_values_or_equally_many_is_not_refused() {
    for (old, new) in [
        (field("a"), whole(7)),
        (field("a"), field("a")),
        (
            field_then("k", vec![call("Expr.sum", json!({}))]),
            field_then("a", vec![call("Expr.sum", json!({}))]),
        ),
    ] {
        let replace = call("Expr.replace", json!({ "old": old, "new": new }));
        let report = report_of(
            "LazyFrame.select",
            select(vec![field_then("a", vec![replace])]),
        );
        assert_eq!(walk(&report).refusal(), None);
        assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    }
}

#[test]
fn a_method_that_follows_a_condition_of_identical_values_beside_many_predicates_is_not_one_value() {
    // `when(f).then(1).otherwise(1).mod(1)`: the values are one expression, so Polars may drop
    // the predicate and the condition is one value or three, and a method that gives a value for
    // each of what it follows, as the remainder does, gives the same number. It is no aggregate
    // that gives one value of any, so what is not counted is still not counted after it: Polars
    // allows the strict replacement with a default of three that it would refuse beside one value.
    let one = || {
        json!({ "expression": { "start": call("polars.lit", json!({
            "value": { "integer": 1 }, "dtype": dtype("Int32") })), "then": [] } })
    };
    let condition = json!({ "expression": {
        "start": call("polars.when", json!({ "predicates": many(vec![field("f")]) })),
        "then": [
            call("When.then", json!({ "statement": one() })),
            call("Then.otherwise", json!({ "statement": one() })),
            call("Expr.mod", json!({ "other": whole(1) })),
            call("Expr.replace_strict", json!({
                "old": field("a"), "new": field("f"), "default": field("f") })),
        ],
    } });
    let report = report_of("LazyFrame.select", select(vec![condition]));
    assert_eq!(walk(&report).refusal(), None);
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert!(matches!(outcome(&report), Outcome::Valid { .. }));
}

#[test]
fn an_aggregate_of_a_condition_of_identical_values_is_one_value_and_is_counted_as_polars_counts_it()
{
    // The sum of the condition is one value whatever Polars does with the condition, so a strict
    // replacement of it given a default of three values is refused by the rule, as Polars refuses
    // it.
    let one = || {
        json!({ "expression": { "start": call("polars.lit", json!({
            "value": { "integer": 1 }, "dtype": dtype("Int32") })), "then": [] } })
    };
    let condition = json!({ "expression": {
        "start": call("polars.when", json!({ "predicates": many(vec![field("f")]) })),
        "then": [
            call("When.then", json!({ "statement": one() })),
            call("Then.otherwise", json!({ "statement": one() })),
            call("Expr.sum", json!({})),
            call("Expr.replace_strict", json!({
                "old": field("a"), "new": field("f"), "default": field("f") })),
        ],
    } });
    let report = report_of("LazyFrame.select", select(vec![condition]));
    let refusal = walk(&report).refusal().expect("the walk refuses it");
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Rule(Rule::ReplacementValuesMatchInNumber)
    );
    // Stricter by design: Python Polars 2.2.0's streaming engine, which the engine of steps is,
    // meets the default with the one value, where its in-memory engine refuses it for the lengths.
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
}

#[test]
fn a_text_method_of_one_value_given_many_is_many_values_as_polars_gives_them() {
    // Polars meets the one value of a text literal with each of the many values of the operand, so
    // what follows is as many values as the table has rows: a strict replacement of it given a
    // default of three is allowed, as Polars allows it, and not refused by the rule for the number
    // of values of a replacement. A parameter of a method of text is no operand that adapts a
    // number's dtype, and it is met just the same.
    for (method, parameter, operand) in [
        ("Expr.str.starts_with", "prefix", field("k")),
        ("Expr.str.ends_with", "suffix", field("k")),
        ("Expr.str.head", "n", field("a")),
        ("Expr.str.tail", "n", field("a")),
        ("Expr.str.pad_start", "length", field("a")),
        ("Expr.str.strip_chars", "characters", field("b")),
        ("Expr.str.split", "by", field("b")),
    ] {
        let mut chain = vec![call(method, json!({ parameter: operand }))];
        // What replaces the values it gives is a column of three values, whatever they are.
        if method != "Expr.str.split" {
            chain.push(call(
                "Expr.replace_strict",
                json!({ "old": field("a"), "new": field("f"), "default": field("f") }),
            ));
        }
        let expression = json!({ "expression": {
            "start": call("polars.lit", json!({ "value": { "string": "abc" } })),
            "then": chain,
        } });
        let report = report_of("LazyFrame.select", select(vec![expression]));
        assert_eq!(walk(&report).refusal(), None, "{method}");
        assert!(
            polars_says(&report).is_none(),
            "{method}: {:?}",
            outcome(&report)
        );
    }
}

#[test]
fn a_strict_replacement_given_a_default_of_many_values_beside_one_value_is_refused_as_polars_does()
{
    let strict = |default: Value| {
        call(
            "Expr.replace_strict",
            json!({ "old": field("a"), "new": field("a"), "default": default }),
        )
    };
    // What the sum of `a` is replaced in is one value, and the default is three.
    let many_beside_one = report_of(
        "LazyFrame.select",
        select(vec![field_then(
            "a",
            vec![call("Expr.sum", json!({})), strict(field("k"))],
        )]),
    );
    assert_eq!(
        walk(&many_beside_one)
            .refusal()
            .map(|refusal| refusal.refusal().source()),
        Some(RefusalSource::Rule(Rule::ReplacementValuesMatchInNumber))
    );
    // Stricter by design: Python Polars 2.2.0's streaming engine, which the engine of steps is,
    // meets the default with the one value, where its in-memory engine refuses it for the lengths.
    assert!(
        polars_says(&many_beside_one).is_none(),
        "{:?}",
        outcome(&many_beside_one)
    );
    // One value of a default is the same beside any number of values.
    let one_beside_many = report_of(
        "LazyFrame.select",
        select(vec![field_then("a", vec![strict(whole(0))])]),
    );
    assert_eq!(walk(&one_beside_many).refusal(), None);
    assert!(polars_says(&one_beside_many).is_none());
}

#[test]
fn a_new_value_and_a_default_of_other_dtypes_are_not_known_to_have_a_supertype() {
    // `new` is whole numbers and the default is text; each is judged apart, so whether Polars
    // finds a dtype for both is not said, and nothing is refused.
    let replace = call(
        "Expr.replace_strict",
        json!({ "old": field("a"), "new": field("a"), "default": field("b") }),
    );
    let walked = report_of(
        "LazyFrame.select",
        select(vec![field_then("a", vec![replace])]),
    );
    assert_eq!(walk(&walked).refusal(), None);
    assert!(walk(&walked).not_known().is_some());
}

#[test]
fn the_bounds_of_an_interval_of_other_dtypes_are_not_known_to_have_a_supertype() {
    // The receiver and both bounds meet in one dtype, so a bound of another dtype than the other
    // is not judged beside the receiver alone: a whole number and a truth value find none beside a
    // decimal, where each finds one beside it apart. Nothing is refused, and what follows is not
    // known.
    let interval = call(
        "Expr.is_between",
        json!({ "lower_bound": field("a"), "upper_bound": field("f") }),
    );
    let walked = report_of(
        "LazyFrame.select",
        select(vec![field_then("f", vec![interval])]),
    );
    assert_eq!(walk(&walked).refusal(), None);
    assert!(walk(&walked).not_known().is_some());
    // Bounds of one dtype are judged as they are.
    let same = call(
        "Expr.is_between",
        json!({ "lower_bound": field("a"), "upper_bound": field("k") }),
    );
    let known = report_of(
        "LazyFrame.select",
        select(vec![field_then("a", vec![same])]),
    );
    assert_eq!(walk(&known).refusal(), None);
    assert!(walk(&known).not_known().is_none());
}

#[test]
fn bounds_of_other_dtypes_are_not_refused_for_what_one_refuses_beside_a_bound_of_the_receivers_dtype()
 {
    // A duration between a whole number and a date: the date as the only bound is refused for a
    // duration, and Polars, which finds one dtype for all three, runs the interval.
    let interval = call(
        "Expr.is_between",
        json!({
            "lower_bound": field("a"),
            "upper_bound": chain(
                call(
                    "polars.lit",
                    json!({ "value": { "integer": 1 }, "dtype": dtype("Date") }),
                ),
                vec![],
            ),
        }),
    );
    let as_a_duration = call(
        "Expr.cast",
        json!({ "dtype": dtype("Duration(time_unit='us')") }),
    );
    let walked = report_of(
        "LazyFrame.select",
        select(vec![field_then("a", vec![as_a_duration, interval])]),
    );
    assert_eq!(walk(&walked).refusal(), None);
    assert!(polars_says(&walked).is_none());
}

#[test]
fn a_test_of_closeness_given_a_number_without_a_dtype_beside_a_date_does_not_say_what_it_gives() {
    // Polars refuses a date and a number typed without a dtype as ambiguous for this test, where
    // it takes a typed number, so what the test gives of them is not said, as it is beside a
    // number, which the number adapts to.
    let close = call("Expr.is_close", json!({ "other": whole(1) }));
    let as_a_date = call("Expr.cast", json!({ "dtype": dtype("Date") }));
    let after_a_date = report_of(
        "LazyFrame.select",
        select(vec![field_then(
            "a",
            vec![as_a_date, close.clone(), call("Expr.is_null", json!({}))],
        )]),
    );
    let outer = walk(&after_a_date)
        .expressions()
        .last()
        .expect("the expression");
    assert_eq!(outer.places()[2], None);
    let after_a_number = report_of(
        "LazyFrame.select",
        select(vec![field_then(
            "a",
            vec![close, call("Expr.is_null", json!({}))],
        )]),
    );
    assert_eq!(
        walk(&after_a_number)
            .expressions()
            .last()
            .expect("the expression")
            .places()[1],
        Some(OfferPlace::new(
            Context::step(Outer::Select),
            After::Value {
                dtype: Representative::Boolean,
                shape: Shape::Many
            }
        ))
    );
}

#[test]
fn one_value_given_an_operand_of_many_values_is_many_values_for_what_follows_it() {
    // The count of nulls is one value, and a logarithm of it to the base of a column is as many
    // values as the column, so a default of as many values is the number of values it follows.
    let log = call("Expr.log", json!({ "base": field("k") }));
    let replace = call(
        "Expr.replace_strict",
        json!({ "old": field("a"), "new": field("f"), "default": field("f") }),
    );
    let report = report_of(
        "LazyFrame.with_columns",
        json!({ "exprs": many(vec![field_then(
            "a",
            vec![call("Expr.has_nulls", json!({})), log, replace],
        )]) }),
    );
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert_eq!(walk(&report).refusal(), None);
}

#[test]
fn a_cast_gives_the_dtype_it_is_told_and_the_walk_follows_it() {
    let cast = call(
        "Expr.cast",
        json!({ "dtype": dtype("Int32"), "strict": { "flag": true } }),
    );
    let report = report_of(
        "LazyFrame.select",
        select(vec![field_then(
            "a",
            vec![cast, call("Expr.is_null", json!({}))],
        )]),
    );
    let places = walk(&report).expressions()[0].places();
    // After the cast the chain has the dtype it was told, and a test for a null follows it.
    assert_eq!(
        places[1],
        Some(OfferPlace::new(
            Context::step(Outer::Select),
            After::Value {
                dtype: Representative::Int32,
                shape: Shape::Many
            }
        ))
    );
    assert_eq!(walk(&report).refusal(), None);
    let only = report_of(
        "LazyFrame.select",
        select(vec![field_then(
            "a",
            vec![call("Expr.cast", json!({ "dtype": dtype("Int32") }))],
        )]),
    );
    assert_eq!(dtype_of_the_column(&only), DataType::Int32);
}

#[test]
fn a_cast_to_a_dtype_the_tables_do_not_cover_leaves_what_follows_not_known() {
    // A list of lists is no representative dtype, so nothing is said of what a cast to it gives.
    let cast = call(
        "Expr.cast",
        json!({ "dtype": dtype("List(List(Float64))") }),
    );
    let report = report_of(
        "LazyFrame.select",
        select(vec![field_then(
            "a",
            vec![cast, call("Expr.is_null", json!({}))],
        )]),
    );
    assert_eq!(walk(&report).refusal(), None);
    assert!(walk(&report).not_known().is_some());
}

#[test]
fn a_cast_that_polars_refuses_for_the_dtypes_is_refused_at_the_cast() {
    // Text that is a name converts to no number, as the cast is strict.
    let cast = call("Expr.cast", json!({ "dtype": dtype("UInt8") }));
    let report = report_of(
        "LazyFrame.select",
        select(vec![field_then("b", vec![cast])]),
    );
    // The tables were measured on text that is a duration, so they refuse what the rows of this
    // table do too.
    assert!(polars_says(&report).is_some());
    assert!(walk(&report).refusal().is_some());
    let lenient = call(
        "Expr.cast",
        json!({ "dtype": dtype("UInt8"), "strict": { "flag": false } }),
    );
    let report = report_of(
        "LazyFrame.select",
        select(vec![field_then("b", vec![lenient])]),
    );
    assert_eq!(walk(&report).refusal(), None);
    assert!(polars_says(&report).is_none());
}

fn join_on(key: Value) -> Ran {
    report_of(
        "LazyFrame.join",
        json!({ "left_on": many(vec![key]), "right_on": many(vec![field("rk")]) }),
    )
}

#[test]
fn a_fill_by_a_strategy_that_looks_at_other_rows_is_refused_in_a_joins_key_and_one_by_zero_is_not()
{
    let cast = || call("Expr.cast", json!({ "dtype": dtype("Int64") }));
    let fill = |word: &str| {
        field_then(
            "k",
            vec![
                call("Expr.fill_null", json!({ "strategy": choice(word) })),
                cast(),
            ],
        )
    };
    for word in ["forward", "mean"] {
        let report = join_on(fill(word));
        assert!(polars_says(&report).is_some(), "{word}");
        assert!(
            walk(&report)
                .refusal()
                .is_some_and(|refusal| refusal.refusal().reason().contains("elementwise")),
            "{word}"
        );
    }
    let report = join_on(fill("zero"));
    assert!(polars_says(&report).is_none());
    assert_eq!(walk(&report).refusal(), None);
}

#[test]
fn a_count_of_nulls_may_be_a_joins_key_alone_and_not_beside_a_column() {
    let count = || call("Expr.null_count", json!({}));
    let cast = || call("Expr.cast", json!({ "dtype": dtype("Int64") }));
    let alone = join_on(field_then("k", vec![count(), cast()]));
    assert!(polars_says(&alone).is_none(), "{:?}", outcome(&alone));
    assert_eq!(walk(&alone).refusal(), None);
    // Beside a column it is not elementwise.
    let beside = join_on(field_then(
        "k",
        vec![call(
            "Expr.add",
            json!({ "other": field_then("a", vec![count(), cast()]) }),
        )],
    ));
    assert!(polars_says(&beside).is_some());
    assert_eq!(
        walk(&beside)
            .refusal()
            .map(|refusal| refusal.refusal().source()),
        Some(RefusalSource::Rule(Rule::KeyIsElementwise))
    );
}

#[test]
fn an_interval_with_a_null_bound_is_a_comparison_with_a_null_in_a_joins_key_and_a_null_it_follows_is_not()
 {
    let null = || {
        chain(
            json!({ "entry": "polars.lit", "arguments": { "value": { "flag": null } } }),
            Vec::new(),
        )
    };
    let between = |lower: Value, upper: Value| {
        call(
            "Expr.is_between",
            json!({ "lower_bound": lower, "upper_bound": upper }),
        )
    };
    let key_of = |start: Value, lower: Value, upper: Value| {
        chain(
            start,
            vec![
                between(lower, upper),
                call("Expr.cast", json!({ "dtype": dtype("Int64") })),
            ],
        )
    };
    let column = |name: &str| json!({ "entry": "polars.col", "arguments": { "name": { "columns": [name] } } });
    // A column compared with a null bound is not elementwise.
    let bound = join_on(key_of(column("a"), whole(1), null()));
    assert!(polars_says(&bound).is_some());
    assert!(
        walk(&bound)
            .refusal()
            .is_some_and(|refusal| refusal.refusal().reason().contains("elementwise")),
        "{:?}",
        walk(&bound).refusal()
    );
    // A null that has columns for bounds is not a comparison Polars folds.
    let start = join_on(key_of(
        json!({ "entry": "polars.lit", "arguments": { "value": { "flag": null } } }),
        field("a"),
        field("k"),
    ));
    assert!(polars_says(&start).is_none(), "{:?}", outcome(&start));
    assert_eq!(walk(&start).refusal(), None);
    // A null bound of the dtype of what it bounds is not one Polars folds in a key: it keeps the
    // interval a value of each row, so the walk does not refuse it, where a null with no dtype is
    // refused.
    let typed = join_on(key_of(
        column("a"),
        whole(1),
        chain(
            json!({
                "entry": "polars.lit",
                "arguments": { "value": { "flag": null }, "dtype": dtype("Int64") },
            }),
            Vec::new(),
        ),
    ));
    assert!(polars_says(&typed).is_none(), "{:?}", outcome(&typed));
    assert_eq!(walk(&typed).refusal(), None);
    // A null bound of another dtype than what it bounds folds the interval whole, so an aggregate
    // beside a column inside it leaves the key elementwise, which the rule does not refuse.
    let sum = call("Expr.max", json!({}));
    let folded = join_on(chain(
        column("a"),
        vec![
            sum,
            call(
                "Expr.is_between",
                json!({
                    "lower_bound": field("k"),
                    "upper_bound": chain(
                        json!({
                            "entry": "polars.lit",
                            "arguments": { "value": { "flag": null }, "dtype": dtype("Int32") },
                        }),
                        Vec::new(),
                    ),
                }),
            ),
            call("Expr.cast", json!({ "dtype": dtype("Int64") })),
        ],
    ));
    assert!(polars_says(&folded).is_none(), "{:?}", outcome(&folded));
    assert_eq!(walk(&folded).refusal(), None);
    // A null bound with no dtype folds the interval whole: what the other bound holds is not met,
    // so an aggregate beside a column there is no reason for the key not to be elementwise.
    let given = |bound: Value| {
        join_on(chain(
            json!({ "entry": "polars.lit", "arguments": { "value": { "integer": 1 } } }),
            vec![
                call(
                    "Expr.is_between",
                    json!({ "lower_bound": bound, "upper_bound": null_literal() }),
                ),
                call("Expr.cast", json!({ "dtype": dtype("Int64") })),
            ],
        ))
    };
    let aggregate_beside_a_column = chain(
        col_call("f"),
        vec![
            call("Expr.min", json!({})),
            call("Expr.mul", json!({ "other": field("a") })),
        ],
    );
    let dropped = given(aggregate_beside_a_column);
    assert!(polars_says(&dropped).is_none(), "{:?}", outcome(&dropped));
    assert_eq!(walk(&dropped).refusal(), None);
    // The folded interval is one value, so an aggregate that decides a condition beside it, and
    // the column its other bound reads, leave the key elementwise.
    let condition = join_on(chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field_then("a", vec![call("Expr.has_nulls", json!({}))])]) }),
        ),
        vec![
            call(
                "When.then",
                json!({ "statement": field_then(
                    "k",
                    vec![
                        call("Expr.has_nulls", json!({})),
                        call(
                            "Expr.is_between",
                            json!({ "lower_bound": field("k"), "upper_bound": null_literal() }),
                        ),
                    ],
                ) }),
            ),
            call("Expr.cast", json!({ "dtype": dtype("Int64") })),
        ],
    ));
    assert!(
        polars_says(&condition).is_none(),
        "{:?}",
        outcome(&condition)
    );
    assert_eq!(walk(&condition).refusal(), None);
}

fn null_literal() -> Value {
    chain(
        json!({ "entry": "polars.lit", "arguments": { "value": { "flag": null } } }),
        Vec::new(),
    )
}

#[test]
fn a_default_of_many_values_after_an_aggregate_of_a_condition_of_identical_values_is_refused() {
    // The values of the condition are one expression, so Polars may drop the predicate and give
    // the values' own number, which an aggregate that follows makes one value whatever it was; a
    // default of many values beside it is then refused as for any other one value.
    let condition = |links: Vec<Value>| {
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![field_then(
                    "a",
                    vec![call("Expr.null_count", json!({})), call("Expr.is_infinite", json!({}))],
                )]) }),
            ),
            [
                vec![
                    call("When.then", json!({ "statement": field("k") })),
                    call("Then.otherwise", json!({ "statement": field("k") })),
                ],
                links,
            ]
            .concat(),
        )
    };
    let replace = call(
        "Expr.replace_strict",
        json!({ "old": field("a"), "new": field("a"), "default": field("a") }),
    );
    let report = report_of(
        "LazyFrame.select",
        select(vec![condition(vec![call("Expr.last", json!({})), replace])]),
    );
    // Stricter by design: Python Polars 2.2.0's streaming engine, which the engine of steps is,
    // meets the default with the one value, where its in-memory engine refuses it for the lengths.
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert!(
        walk(&report)
            .refusal()
            .is_some_and(|refusal| matches!(refusal.refusal().source(), RefusalSource::Rule(_))),
        "{:?}",
        walk(&report).refusal()
    );
}

#[test]
fn a_replacement_of_one_value_by_a_table_of_values_is_one_value_and_a_default_of_many_is_refused() {
    // What a replacement lists is a table that it looks the value up in, so replacing the one
    // value of an aggregate by columns gives one value, and a default of many values beside it
    // is refused for the lengths.
    let replace = |old: &str, new: &str| {
        call(
            "Expr.replace",
            json!({ "old": field(old), "new": field(new) }),
        )
    };
    let strict = call(
        "Expr.replace_strict",
        json!({ "old": field("a"), "new": field("a"), "default": field("a") }),
    );
    let report = report_of(
        "LazyFrame.select",
        select(vec![field_then(
            "k",
            vec![call("Expr.max", json!({})), replace("a", "k"), strict],
        )]),
    );
    // Stricter by design: Python Polars 2.2.0's streaming engine, which the engine of steps is,
    // meets the default with the one value, where its in-memory engine refuses it for the lengths.
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert!(
        walk(&report)
            .refusal()
            .is_some_and(|refusal| matches!(refusal.refusal().source(), RefusalSource::Rule(_))),
        "{:?}",
        walk(&report).refusal()
    );
}

#[test]
fn a_predicate_that_is_no_boolean_in_a_joins_key_is_refused_where_the_other_table_holds_keys() {
    // `when(k).then(1).otherwise(0)` as the key: Python Polars 2.2.0 refuses a predicate of whole
    // numbers when it evaluates the key, which its streaming engine does only where the other
    // table holds a key with a value, as `rk` does here; the rule that a condition's predicate is
    // a `Boolean` says it whatever the other table holds, so the evidence of the rule is this case.
    let key = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("k")]) }),
        ),
        vec![
            call("When.then", json!({ "statement": whole(1) })),
            call("Then.otherwise", json!({ "statement": whole(0) })),
            call("Expr.cast", json!({ "dtype": dtype("Int64") })),
        ],
    );
    let report = join_on(key);
    assert!(
        polars_says(&report).is_some_and(|reason| reason.contains("Boolean")),
        "{:?} {:?}",
        polars_says(&report),
        outcome(&report)
    );
    assert!(
        walk(&report).refusal().is_some_and(|refusal| matches!(
            refusal.refusal().source(),
            RefusalSource::Rule(Rule::ConditionPredicateIsBoolean)
        )),
        "{:?}",
        walk(&report).refusal()
    );
}

#[test]
fn a_missing_comparison_of_whole_numbers_with_a_fraction_is_no_elementwise_key_as_polars_takes_it()
{
    // Python Polars 2.2.0 decides `k.ne_missing(1.5)` of whole numbers while it coerces the types,
    // for none is 1.5, and what it leaves is a constant repeated to the length of `k`, which the
    // join refuses as not elementwise, in both engines. Polars swaps the operands when it coerces
    // them, so `lit(1.5).ne_missing(k)` is decided the same way. A number with no fraction is held
    // by whole numbers, so the comparison is kept and the key is taken, and so is a number given the
    // dtype of whole numbers, which makes `1.5` the whole number 1, and a key that is one value, as
    // the count of a column or an aggregate of the comparison is, for the constant is no longer
    // repeated, and the same comparison in no join.
    let number = |value: f64, whole_dtype: Option<&str>| {
        let mut arguments = json!({ "value": { "float": value } });
        if let Some(name) = whole_dtype {
            arguments["dtype"] = dtype(name);
        }
        chain(
            json!({ "entry": "polars.lit", "arguments": arguments }),
            Vec::new(),
        )
    };
    let cast = || call("Expr.cast", json!({ "dtype": dtype("Int64") }));
    // `k` compared with the number, and the number compared with `k`.
    let beside = |method: &str, value: Value| {
        chain(
            col_call("k"),
            vec![call(method, json!({ "other": value })), cast()],
        )
    };
    let swapped = |method: &str, value: Value| {
        chain(
            value["expression"]["start"].clone(),
            vec![call(method, json!({ "other": field("k") })), cast()],
        )
    };
    let refused = |said: &str, report: &Ran| {
        assert!(
            polars_says(report).is_some_and(|reason| reason.contains("elementwise")),
            "{said}: {:?} {:?}",
            polars_says(report),
            outcome(report)
        );
        assert!(
            walk(report).refusal().is_some_and(|refusal| matches!(
                refusal.refusal().source(),
                RefusalSource::Rule(Rule::KeyIsElementwise)
            )),
            "{said}: {:?}",
            walk(report).refusal()
        );
    };
    let taken = |said: &str, report: &Ran| {
        assert!(
            polars_says(report).is_none(),
            "{said}: {:?}",
            outcome(report)
        );
        assert_eq!(walk(report).refusal(), None, "{said}");
    };
    for method in ["Expr.ne_missing", "Expr.eq_missing"] {
        refused(
            &format!("{method} of k"),
            &join_on(beside(method, number(1.5, None))),
        );
        refused(
            &format!("{method} beside k"),
            &join_on(swapped(method, number(1.5, None))),
        );
        refused(
            &format!("{method} of k, a float given"),
            &join_on(beside(method, number(1.5, Some("Float32")))),
        );
        refused(
            &format!("{method} beside k, a float given"),
            &join_on(swapped(method, number(1.5, Some("Float32")))),
        );
        // Polars rewrites the number only where it is a 32 or 64 bit float, so a 16 bit float is
        // compared as it is and the key is taken.
        taken(
            &format!("{method} of k, a half float given"),
            &join_on(beside(method, number(1.5, Some("Float16")))),
        );
        taken(
            &format!("{method} beside k, a half float given"),
            &join_on(swapped(method, number(1.5, Some("Float16")))),
        );
        taken(
            &format!("{method} of k, no fraction"),
            &join_on(beside(method, number(2.0, None))),
        );
        taken(
            &format!("{method} beside k, no fraction"),
            &join_on(swapped(method, number(2.0, None))),
        );
        // An aggregate that follows leaves a key of one value, which is taken whole.
        taken(
            &format!("{method} of k, summed"),
            &join_on(chain(
                col_call("k"),
                vec![
                    call(method, json!({ "other": number(1.5, None) })),
                    call("Expr.sum", json!({})),
                ],
            )),
        );
        taken(
            &format!("{method} beside k, summed"),
            &join_on(chain(
                number(1.5, None)["expression"]["start"].clone(),
                vec![
                    call(method, json!({ "other": field("k") })),
                    call("Expr.sum", json!({})),
                ],
            )),
        );
        taken(
            &format!("{method} of k, whole numbers given"),
            &join_on(beside(method, number(1.5, Some("Int64")))),
        );
        taken(
            &format!("{method} beside k, whole numbers given"),
            &join_on(swapped(method, number(1.5, Some("Int64")))),
        );
    }
    // A key that is one value is taken whole.
    taken(
        "the count of k",
        &join_on(chain(
            col_call("k"),
            vec![
                call("Expr.len", json!({})),
                call("Expr.ne_missing", json!({ "other": number(1.5, None) })),
                cast(),
            ],
        )),
    );
    taken(
        "beside the count of k",
        &join_on(chain(
            json!({ "entry": "polars.lit", "arguments": { "value": { "float": 1.5 } } }),
            vec![
                call(
                    "Expr.ne_missing",
                    json!({ "other": chain(col_call("k"), vec![call("Expr.len", json!({}))]) }),
                ),
                cast(),
            ],
        )),
    );
    let selected = report_of(
        "LazyFrame.select",
        select(vec![beside("Expr.ne_missing", number(1.5, None))]),
    );
    taken("a select", &selected);
}

/// A join of the table `x` whose key is `key`, where `x` is one column of `dtype` holding 1, 1 and
/// 2, with the key of the other table `rk`.
fn join_on_a_column_of(dtype: &DataType, key: Value) -> Ran {
    let working = df!("x" => [1_i64, 1, 2])
        .expect("the table")
        .lazy()
        .select([col("x").cast(dtype.clone())])
        .collect()
        .expect("the column cast to its dtype");
    let mut given = json!({ "left_on": many(vec![key]), "right_on": many(vec![field("rk")]) });
    given["other"] = json!({ "table": "other" });
    ran_on(
        &Document::read(&document(vec![step("s", "LazyFrame.join", given)])).expect("a document"),
        &working,
    )
}

/// What each report says of a key: that Polars and the walk both refuse it as not elementwise, or
/// that neither refuses it.
fn assert_refused_by_both(said: &str, report: &Ran) {
    assert!(
        polars_says(report).is_some_and(|reason| reason.contains("elementwise")),
        "{said}: Polars says {:?}, the engine {:?}",
        polars_says(report),
        outcome(report)
    );
    assert!(
        walk(report).refusal().is_some_and(|refusal| matches!(
            refusal.refusal().source(),
            RefusalSource::Rule(Rule::KeyIsElementwise)
        )),
        "{said}: {:?}",
        walk(report).refusal()
    );
}

fn assert_taken_by_both(said: &str, report: &Ran) {
    assert!(
        polars_says(report).is_none(),
        "{said}: Polars says {:?}",
        polars_says(report)
    );
    assert_eq!(walk(report).refusal(), None, "{said}");
}

#[test]
fn a_missing_comparison_of_a_column_with_a_number_its_dtype_cannot_hold_is_no_elementwise_key_as_polars_takes_it()
 {
    // Python Polars 2.2.0 decides `x.eq_missing(300)` of an `Int8` column at once, for none of its
    // values is 300, and the key it leaves is a constant repeated to the length of the column,
    // which the join refuses as not elementwise, in both engines. It does the same for the number
    // before the column, for `ne_missing`, and for a number below the lowest value or above the
    // highest of any of the whole-number dtypes, and takes the number each of them holds, the
    // lowest and the highest among them. The numbers are the ones a document can write, which
    // are 64 bit, so every one of them is held by `Int64` and `Int128`; Polars decides none
    // against a column of `UInt128`, not even a number below zero.
    let bounds: [(DataType, i128, i128); 10] = [
        (DataType::Int8, i128::from(i8::MIN), i128::from(i8::MAX)),
        (DataType::Int16, i128::from(i16::MIN), i128::from(i16::MAX)),
        (DataType::Int32, i128::from(i32::MIN), i128::from(i32::MAX)),
        (DataType::Int64, i128::from(i64::MIN), i128::from(i64::MAX)),
        (DataType::Int128, i128::MIN, i128::MAX),
        (DataType::UInt8, 0, i128::from(u8::MAX)),
        (DataType::UInt16, 0, i128::from(u16::MAX)),
        (DataType::UInt32, 0, i128::from(u32::MAX)),
        (DataType::UInt64, 0, i128::from(u64::MAX)),
        (DataType::UInt128, i128::MIN, i128::MAX),
    ];
    let cast = || call("Expr.cast", json!({ "dtype": dtype("Int64") }));
    let beside = |method: &str, number: i64| {
        chain(
            col_call("x"),
            vec![call(method, json!({ "other": whole(number) })), cast()],
        )
    };
    let swapped = |method: &str, number: i64| {
        chain(
            call("polars.lit", json!({ "value": { "integer": number } })),
            vec![call(method, json!({ "other": field("x") })), cast()],
        )
    };
    for (held, lowest, highest) in bounds {
        let mut numbers: Vec<i64> = vec![-1, 0, 1, 127, 128, 300, i64::MIN, i64::MAX];
        for edge in [
            lowest.checked_sub(1),
            Some(lowest),
            Some(highest),
            highest.checked_add(1),
        ]
        .into_iter()
        .flatten()
        {
            numbers.extend(i64::try_from(edge));
        }
        numbers.sort_unstable();
        numbers.dedup();
        for number in numbers {
            let holds = (lowest..=highest).contains(&i128::from(number));
            for method in ["Expr.eq_missing", "Expr.ne_missing"] {
                for (form, key) in [
                    ("beside", beside(method, number)),
                    ("swapped", swapped(method, number)),
                ] {
                    let said = format!("{method} {form}, {number} and a column of {held}");
                    let report = join_on_a_column_of(&held, key);
                    if holds {
                        assert_taken_by_both(&said, &report);
                    } else {
                        assert_refused_by_both(&said, &report);
                    }
                }
            }
        }
    }
}

#[test]
fn a_missing_comparison_with_a_number_that_the_dtype_of_what_is_compared_holds_is_taken_in_a_joins_key()
 {
    // The column is of the dtype it is cast to, so a cast that widens it holds the number that
    // the column did not, and one that narrows it does not hold the number the column did.
    let cast_to = |word: &str| call("Expr.cast", json!({ "dtype": dtype(word) }));
    let compared = |casts: Vec<Value>, method: &str, other: Value| {
        let mut then = casts;
        then.push(call(method, json!({ "other": other })));
        then.push(cast_to("Int64"));
        chain(col_call("x"), then)
    };
    for method in ["Expr.eq_missing", "Expr.ne_missing"] {
        taken_cases(method, &compared);
        // `x` is an `Int8` and a cast to `Int16` holds 300.
        assert_taken_by_both(
            &format!("{method}, widened"),
            &join_on_a_column_of(
                &DataType::Int8,
                compared(vec![cast_to("Int16")], method, whole(300)),
            ),
        );
        assert_taken_by_both(
            &format!("{method}, widened to whole numbers with a sign"),
            &join_on_a_column_of(
                &DataType::UInt8,
                compared(vec![cast_to("Int64")], method, whole(-1)),
            ),
        );
        assert_refused_by_both(
            &format!("{method}, narrowed"),
            &join_on_a_column_of(
                &DataType::Int64,
                compared(vec![cast_to("Int8")], method, whole(300)),
            ),
        );
        assert_refused_by_both(
            &format!("{method}, narrowed to an unsigned dtype"),
            &join_on_a_column_of(
                &DataType::Int64,
                compared(vec![cast_to("UInt16")], method, whole(-1)),
            ),
        );
    }
}

/// The comparisons with a number that Polars takes as they are: a comparison that does not take a
/// missing value as a value, whatever the number is, a number with no fraction, and what is one
/// value, as the sum of the column is.
fn taken_cases(method: &str, compared: &dyn Fn(Vec<Value>, &str, Value) -> Value) {
    let none = Vec::new;
    let float = |number: f64| {
        chain(
            call("polars.lit", json!({ "value": { "float": number } })),
            Vec::new(),
        )
    };
    assert_taken_by_both(
        &format!("{method} of an Int8 and a number with no fraction"),
        &join_on_a_column_of(&DataType::Int8, compared(none(), method, float(2.0))),
    );
    for other in [
        "Expr.eq", "Expr.ne", "Expr.gt", "Expr.ge", "Expr.lt", "Expr.le",
    ] {
        assert_taken_by_both(
            &format!("{other} of an Int8 and 300"),
            &join_on_a_column_of(&DataType::Int8, compared(none(), other, whole(300))),
        );
    }
    let summed = chain(
        col_call("x"),
        vec![
            call(method, json!({ "other": whole(300) })),
            call("Expr.sum", json!({})),
        ],
    );
    assert_taken_by_both(
        &format!("{method} of an Int8 and 300, summed"),
        &join_on_a_column_of(&DataType::Int8, summed),
    );
}

#[test]
fn a_missing_comparison_of_a_field_read_from_a_struct_or_with_a_number_given_its_dtype_is_judged_alike()
 {
    // What a struct holds in a field is a column of the dtype of the field, and a number given a
    // dtype is the number it is, whichever dtype it is given: Polars decides the comparison for
    // each as it does for a column, and so does the walk.
    let cast = |word: &str| call("Expr.cast", json!({ "dtype": dtype(word) }));
    let eq_missing = |other: Value| call("Expr.eq_missing", json!({ "other": other }));
    let given = |number: i64, word: &str| {
        chain(
            call(
                "polars.lit",
                json!({ "value": { "integer": number }, "dtype": dtype(word) }),
            ),
            Vec::new(),
        )
    };
    let in_a_struct = |number: i64| {
        chain(
            call(
                "polars.struct",
                json!({ "exprs": many(vec![field_then("x", vec![cast("Int8")])]) }),
            ),
            vec![
                call("Expr.struct.field", json!({ "name": { "field": "x" } })),
                eq_missing(whole(number)),
                cast("Int64"),
            ],
        )
    };
    assert_refused_by_both(
        "a field of Int8 and 300",
        &join_on_a_column_of(&DataType::Int64, in_a_struct(300)),
    );
    assert_taken_by_both(
        "a field of Int8 and 1",
        &join_on_a_column_of(&DataType::Int64, in_a_struct(1)),
    );
    let beside = |number: Value| chain(col_call("x"), vec![eq_missing(number), cast("Int64")]);
    let swapped = |start: Value| {
        chain(
            start,
            vec![
                call("Expr.eq_missing", json!({ "other": field("x") })),
                cast("Int64"),
            ],
        )
    };
    for (held, number, word, refused) in [
        (DataType::Int8, 300, "Int16", true),
        (DataType::Int8, 300, "Float64", true),
        (DataType::UInt8, -1, "Int8", true),
        (DataType::Int16, 300, "Int16", false),
        (DataType::Int8, 100, "Int16", false),
    ] {
        let said = format!("{number} given {word} and a column of {held}");
        let report = join_on_a_column_of(&held, beside(given(number, word)));
        let start = given(number, word)["expression"]["start"].clone();
        let reversed = join_on_a_column_of(&held, swapped(start));
        if refused {
            assert_refused_by_both(&said, &report);
            assert_refused_by_both(&format!("{said}, swapped"), &reversed);
        } else {
            assert_taken_by_both(&said, &report);
            assert_taken_by_both(&format!("{said}, swapped"), &reversed);
        }
    }
}

#[test]
fn a_missing_comparison_of_a_column_whose_dtype_is_not_known_is_not_judged_in_a_joins_key() {
    // A list of lists is no representative dtype, so the dtype of what a cast to it gives is not
    // known and whether it holds the number cannot be said: the walk neither refuses the key nor
    // takes it, but says that it cannot tell.
    let cast = || {
        call(
            "Expr.cast",
            json!({ "dtype": dtype("List(List(Float64))") }),
        )
    };
    for method in ["Expr.eq_missing", "Expr.ne_missing"] {
        let beside = chain(
            col_call("x"),
            vec![cast(), call(method, json!({ "other": whole(300) }))],
        );
        let swapped = chain(
            call("polars.lit", json!({ "value": { "integer": 300 } })),
            vec![call(
                method,
                json!({ "other": field_then("x", vec![cast()]) }),
            )],
        );
        for (form, key) in [("beside", beside), ("swapped", swapped)] {
            let report = join_on_a_column_of(&DataType::Int64, key);
            assert_eq!(walk(&report).refusal(), None, "{method} {form}");
            assert!(walk(&report).not_known().is_some(), "{method} {form}");
        }
    }
}

#[test]
fn a_missing_comparison_that_polars_keeps_leaves_the_rest_of_a_joins_key_to_the_rules() {
    // A column that holds the number is compared as it is, so an aggregate of the comparison beside
    // a column makes the key not elementwise, as an aggregate of any other value does, and so does one
    // of a comparison that Polars decides, for the aggregate is still beside a column.
    let cast = || call("Expr.cast", json!({ "dtype": dtype("Int64") }));
    let beside_the_column = |method: &str, number: Value| {
        chain(
            col_call("x"),
            vec![
                cast(),
                call(
                    "Expr.add",
                    json!({ "other": chain(
                        col_call("x"),
                        vec![
                            call(method, json!({ "other": number })),
                            call("Expr.sum", json!({})),
                            cast(),
                        ],
                    ) }),
                ),
            ],
        )
    };
    for method in ["Expr.eq_missing", "Expr.ne_missing"] {
        for (held, number) in [
            (DataType::Int16, 300),
            (DataType::Int64, 70_000),
            (DataType::UInt8, 200),
            (DataType::Int8, 300),
            (DataType::Int8, 100),
        ] {
            assert_refused_by_both(
                &format!("{method} {number} of {held}, summed beside the column"),
                &join_on_a_column_of(&held, beside_the_column(method, whole(number))),
            );
        }
    }
}

#[test]
fn a_missing_comparison_that_polars_decides_in_what_a_joins_key_holds_is_no_elementwise_key() {
    // The constant repeated to the length of the column is no elementwise key wherever it stands
    // in the key: as an operand of arithmetic, or as the predicate of a condition.
    let cast = || call("Expr.cast", json!({ "dtype": dtype("Int64") }));
    let compared = |method: &str, number: Value| {
        chain(
            col_call("x"),
            vec![call(method, json!({ "other": number })), cast()],
        )
    };
    let fraction = || {
        chain(
            call("polars.lit", json!({ "value": { "float": 1.5 } })),
            Vec::new(),
        )
    };
    for method in ["Expr.eq_missing", "Expr.ne_missing"] {
        for (said, held, number) in [
            ("a number the dtype cannot hold", DataType::Int8, whole(300)),
            ("a number with a fraction", DataType::Int64, fraction()),
        ] {
            let operand = join_on_a_column_of(
                &held,
                chain(
                    col_call("x"),
                    vec![
                        cast(),
                        call(
                            "Expr.add",
                            json!({ "other": compared(method, number.clone()) }),
                        ),
                    ],
                ),
            );
            assert_refused_by_both(&format!("{method} with {said}, an operand"), &operand);
            let predicate = join_on_a_column_of(
                &held,
                chain(
                    call(
                        "polars.when",
                        json!({ "predicates": many(vec![chain(
                            col_call("x"),
                            vec![call(method, json!({ "other": number }))],
                        )]) }),
                    ),
                    vec![
                        call("When.then", json!({ "statement": whole(1) })),
                        call("Then.otherwise", json!({ "statement": whole(0) })),
                        cast(),
                    ],
                ),
            );
            assert_refused_by_both(&format!("{method} with {said}, a predicate"), &predicate);
        }
    }
}

#[test]
fn an_interval_with_a_negative_bound_of_a_count_is_decided_and_is_not_refused_in_a_joins_key() {
    // A count is unsigned, so no count is at most -1: Polars decides the interval at once, and
    // what its other bound reads is not met, where the interval of a count and a column bounded
    // by a positive number is refused for the aggregate beside the column. The document does not
    // decide the dtype, so the walk refuses neither.
    let interval = |upper: i64| {
        join_on(chain(
            json!({ "entry": "polars.lit", "arguments": { "value": { "integer": 1 } } }),
            vec![
                call("Expr.len", json!({})),
                call(
                    "Expr.is_between",
                    json!({ "lower_bound": field("k"), "upper_bound": whole(upper) }),
                ),
                call("Expr.cast", json!({ "dtype": dtype("Int64") })),
            ],
        ))
    };
    let decided = interval(-1);
    assert!(polars_says(&decided).is_none(), "{:?}", outcome(&decided));
    assert_eq!(walk(&decided).refusal(), None);
    let kept = interval(5);
    assert!(polars_says(&kept).is_some(), "{:?}", outcome(&kept));
    assert!(
        walk(&kept)
            .refusal()
            .is_some_and(|refusal| refusal.refusal().reason().contains("elementwise")),
        "{:?}",
        walk(&kept).refusal()
    );
}

#[test]
fn a_column_of_the_dtype_null_is_no_column_that_a_comparison_with_a_null_replaces_in_a_joins_key() {
    // Every value of a column of `Null` is a null already, so comparing it with a null leaves
    // nothing in the key that was a value, as a null literal is compared. Polars takes the key.
    let table = df!(
        "n" => [None::<i64>, None, None],
        "a" => [1_i64, 2, 3],
    )
    .expect("a table")
    .lazy()
    .with_column(col("n").cast(DataType::Null))
    .collect()
    .expect("a table of a column of Null");
    let key = chain(
        col_call("n"),
        vec![
            call(
                "Expr.is_between",
                json!({ "lower_bound": null_literal(), "upper_bound": field("n") }),
            ),
            call("Expr.cast", json!({ "dtype": dtype("Int64") })),
        ],
    );
    let mut given = json!({ "left_on": many(vec![key]), "right_on": many(vec![field("rk")]) });
    given["other"] = json!({ "table": "other" });
    let report = ran_on(
        &Document::read(&document(vec![step("s", "LazyFrame.join", given)])).expect("a document"),
        &table,
    );
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert_eq!(walk(&report).refusal(), None);
    // Nor is it replaced by a null where it is compared with a column.
    let compared = chain(
        col_call("n"),
        vec![
            call("Expr.lt", json!({ "other": field("a") })),
            call("Expr.cast", json!({ "dtype": dtype("Int64") })),
        ],
    );
    let mut given = json!({ "left_on": many(vec![compared]), "right_on": many(vec![field("rk")]) });
    given["other"] = json!({ "table": "other" });
    let report = ran_on(
        &Document::read(&document(vec![step("s", "LazyFrame.join", given)])).expect("a document"),
        &table,
    );
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert_eq!(walk(&report).refusal(), None);
}

#[test]
fn a_condition_of_single_values_that_are_one_expression_gives_one_value_for_a_replacement() {
    // The predicate and the values are each one value, so the condition gives one value whether
    // or not Polars drops its predicate, and a default of many values beside it is refused.
    let one = |name: &str| field_then(name, vec![call("Expr.first", json!({}))]);
    let condition = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field_then(
                "a",
                vec![call("Expr.min", json!({})), call("Expr.is_duplicated", json!({}))],
            )]) }),
        ),
        vec![
            call("When.then", json!({ "statement": one("b") })),
            call("Then.otherwise", json!({ "statement": one("b") })),
            call(
                "Expr.replace_strict",
                json!({ "old": field("a"), "new": field("k"), "default": field("k") }),
            ),
        ],
    );
    let report = report_of("LazyFrame.select", select(vec![condition]));
    // Stricter by design: Python Polars 2.2.0's streaming engine, which the engine of steps is,
    // meets the default with the one value, where its in-memory engine refuses it for the lengths.
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert!(
        walk(&report)
            .refusal()
            .is_some_and(|refusal| matches!(refusal.refusal().source(), RefusalSource::Rule(_))),
        "{:?}",
        walk(&report).refusal()
    );
}

#[test]
fn one_value_over_a_window_is_as_many_values_as_the_rows_and_a_default_of_many_is_not_refused() {
    // The minimum is one value, and a window gives each row of its group that value, so a strict
    // replacement of it, given a default of many values, is given as many as it follows.
    let replace = call(
        "Expr.replace_strict",
        json!({ "old": field("a"), "new": field("k"), "default": field("k") }),
    );
    let windowed = call(
        "Expr.over",
        json!({ "partition_by": many(vec![field("k")]) }),
    );
    let report = report_of(
        "LazyFrame.select",
        select(vec![field_then(
            "k",
            vec![call("Expr.min", json!({})), windowed, replace],
        )]),
    );
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert_eq!(walk(&report).refusal(), None);
}

#[test]
fn a_null_literal_of_another_dtype_given_to_a_fill_leaves_what_it_gives_not_known() {
    // Polars gives the supertype of the two in its plan, and the dtype of what it fills in the
    // rows where the null is folded away, for a list, so the walk does not say which.
    let filler = chain(
        json!({
            "entry": "polars.lit",
            "arguments": { "value": { "flag": null }, "dtype": dtype("Float64") },
        }),
        Vec::new(),
    );
    let fill = call("Expr.fill_null", json!({ "value": filler }));
    let report = report_of(
        "LazyFrame.select",
        select(vec![field_then(
            "a",
            vec![fill, call("Expr.is_null", json!({}))],
        )]),
    );
    let outer = walk(&report).expressions().last().expect("the expression");
    assert_eq!(outer.places()[1], None);
    assert_eq!(walk(&report).refusal(), None);
}

#[test]
fn one_value_over_a_window_that_explodes_is_one_value_for_each_group_and_not_for_each_row() {
    // With no partition the table is one group, so the window that explodes gives one value, and
    // a default of many values beside it is refused for the lengths.
    let replace = call(
        "Expr.replace_strict",
        json!({ "old": field("a"), "new": field("k"), "default": field("k") }),
    );
    let windowed = call(
        "Expr.over",
        json!({ "order_by": many(vec![field("k")]), "mapping_strategy": choice("explode") }),
    );
    let report = report_of(
        "LazyFrame.select",
        select(vec![field_then(
            "k",
            vec![call("Expr.min", json!({})), windowed, replace],
        )]),
    );
    // Stricter by design: Python Polars 2.2.0's streaming engine, which the engine of steps is,
    // meets the default with the one value, where its in-memory engine refuses it for the lengths.
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert!(
        walk(&report)
            .refusal()
            .is_some_and(|refusal| matches!(refusal.refusal().source(), RefusalSource::Rule(_))),
        "{:?}",
        walk(&report).refusal()
    );
}

#[test]
fn one_value_over_a_partitioned_window_that_explodes_is_one_value_for_each_group_which_the_rows_decide()
 {
    // Partitioned by a column of three different values the table has three groups, as many as
    // it has rows, so a default of many values is as many as the window gives and Polars takes
    // it. How many groups there are depends on the rows, so the walk does not refuse it.
    let replace = call(
        "Expr.replace_strict",
        json!({ "old": field("a"), "new": field("a"), "default": field("k") }),
    );
    let windowed = call(
        "Expr.over",
        json!({ "partition_by": many(vec![field("a")]), "mapping_strategy": choice("explode") }),
    );
    let report = report_of(
        "LazyFrame.select",
        select(vec![field_then(
            "k",
            vec![call("Expr.sum", json!({})), windowed, replace],
        )]),
    );
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert_eq!(walk(&report).refusal(), None);
}

#[test]
fn a_condition_of_identical_many_valued_branches_gives_many_values_for_what_replaces() {
    // Whether or not Polars drops the predicate, the values are the column, so what replaces one
    // value by them is given many, which Polars refuses and the rule says so.
    let condition = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("f")]) }),
        ),
        vec![
            call("When.then", json!({ "statement": field("k") })),
            call("Then.otherwise", json!({ "statement": field("k") })),
        ],
    );
    let replace = call("Expr.replace", json!({ "old": whole(1), "new": condition }));
    let report = report_of(
        "LazyFrame.select",
        select(vec![field_then("a", vec![replace])]),
    );
    assert!(polars_says(&report).is_some(), "{:?}", outcome(&report));
    assert!(
        walk(&report)
            .refusal()
            .is_some_and(|refusal| matches!(refusal.refusal().source(), RefusalSource::Rule(_))),
        "{:?}",
        walk(&report).refusal()
    );
}

/// `when(first).then(k).when(a.is_null()).then(1)`, the key a join is given.
fn two_branches_of_which_the_first_is(first: Value) -> Value {
    chain(
        call("polars.when", json!({ "predicates": many(vec![first]) })),
        vec![
            call("When.then", json!({ "statement": field("k") })),
            call(
                "Then.when",
                json!({ "predicates": many(vec![field_then("a", vec![call("Expr.is_null", json!({}))])]) }),
            ),
            call("ChainedWhen.then", json!({ "statement": whole(1) })),
        ],
    )
}

fn a_replacement_of_columns_summed() -> Vec<Value> {
    vec![
        call(
            "Expr.replace_strict",
            json!({ "old": field("a"), "new": whole(1) }),
        ),
        call("Expr.sum", json!({})),
    ]
}

#[test]
fn a_replacement_of_columns_that_a_comparison_with_a_null_leaves_unevaluated_is_no_refusal_in_a_joins_key()
 {
    // Polars folds the comparison of the sum with a null into a null before it asks whether the
    // key is elementwise, so the replacement that the sum follows is never met.
    let mut links = a_replacement_of_columns_summed();
    links.push(call("Expr.lt", json!({ "other": null_literal() })));
    let report = join_on(two_branches_of_which_the_first_is(field_then("k", links)));
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert_eq!(walk(&report).refusal(), None);
}

#[test]
fn a_replacement_of_columns_that_is_evaluated_is_refused_in_a_joins_key_beside_one_a_null_drops() {
    // Compared with a number it is evaluated, and Polars refuses the key as it does a
    // replacement of columns anywhere else in one.
    let mut links = a_replacement_of_columns_summed();
    links.push(call("Expr.lt", json!({ "other": whole(5) })));
    let compared = join_on(two_branches_of_which_the_first_is(field_then("k", links)));
    // The sum is an aggregate beside a column, which is a refusal of its own.
    assert!(polars_says(&compared).is_some(), "{:?}", outcome(&compared));
    assert!(walk(&compared).refusal().is_some());
    // A replacement of columns that is a value of the key, beside a comparison with a null that
    // drops another, is evaluated too.
    let mut links = a_replacement_of_columns_summed();
    links.push(call("Expr.lt", json!({ "other": null_literal() })));
    let key = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field_then("k", links)]) }),
        ),
        vec![
            call(
                "When.then",
                json!({ "statement": field_then(
                    "k",
                    vec![call(
                        "Expr.replace",
                        json!({ "old": field("k"), "new": whole(7) }),
                    )],
                ) }),
            ),
            call("Then.otherwise", json!({ "statement": whole(1) })),
        ],
    );
    let report = join_on(key);
    assert!(polars_says(&report).is_some(), "{:?}", outcome(&report));
    assert_eq!(
        walk(&report)
            .refusal()
            .map(|refusal| refusal.refusal().source()),
        Some(RefusalSource::Rule(Rule::KeyIsElementwise))
    );
}

fn a_count_compared_with(entry: &str, number: i64) -> Value {
    chain(
        call("polars.lit", json!({ "value": { "integer": 1 } })),
        vec![
            call("Expr.len", json!({})),
            call(entry, json!({ "other": whole(number) })),
            call("Expr.sub", json!({ "other": field("a") })),
        ],
    )
}

#[test]
fn a_count_compared_as_a_value_with_a_number_no_dtype_holds_is_one_polars_may_fold_in_a_joins_key()
{
    // Polars decides a comparison that takes a missing value as a value, of an unsigned count
    // with a number below zero, while it coerces the types. What is compared is one value, so it
    // leaves a plain constant, with no length to repeat it to and no aggregate, and the key is
    // taken. It does not decide a comparison that is not of missing values so.
    for entry in ["Expr.ne_missing", "Expr.eq_missing"] {
        let report = join_on(a_count_compared_with(entry, -1));
        assert!(
            polars_says(&report).is_none(),
            "{entry}: {:?}",
            outcome(&report)
        );
        assert_eq!(walk(&report).refusal(), None, "{entry}");
    }
}

#[test]
fn a_count_compared_with_a_number_that_is_kept_is_an_aggregate_beside_a_column_in_a_joins_key() {
    // A number that every dtype holds is never decided at once, and a comparison that is not of
    // missing values is kept whatever the number is.
    for (entry, number) in [
        ("Expr.ne_missing", 1),
        ("Expr.eq_missing", 1),
        ("Expr.ne", -1),
        ("Expr.ge", -1),
        ("Expr.gt", 1),
    ] {
        let report = join_on(a_count_compared_with(entry, number));
        assert!(
            polars_says(&report).is_some(),
            "{entry}: {:?}",
            outcome(&report)
        );
        assert_eq!(
            walk(&report)
                .refusal()
                .map(|refusal| refusal.refusal().source()),
            Some(RefusalSource::Rule(Rule::KeyIsElementwise)),
            "{entry}"
        );
    }
}

#[test]
fn a_number_no_dtype_holds_that_a_count_is_compared_by_is_a_value_polars_may_fold_too() {
    // The number that starts the chain is the one compared with the count it is given.
    let key = chain(
        call("polars.lit", json!({ "value": { "integer": -1 } })),
        vec![
            call(
                "Expr.ne_missing",
                json!({ "other": chain(
                    call("polars.lit", json!({ "value": { "integer": 1 } })),
                    vec![call("Expr.len", json!({}))],
                ) }),
            ),
            call("Expr.sub", json!({ "other": field("a") })),
        ],
    );
    let report = join_on(key);
    assert!(polars_says(&report).is_none(), "{:?}", outcome(&report));
    assert_eq!(walk(&report).refusal(), None);
}
