//! The walk of a step over the table `a` (`Int64`), `b` (`String`), `f` (`Boolean`) and `k`
//! (`Int64`), which asks the tables what each of these functions may be given and what each
//! gives. The expected dtypes are what Python Polars documents, and the reasons are Polars'.

use dageist_engine::allowed::{
    After, Context, Ending, Outer, Place as OfferPlace, RefusalSource, Representative, Rule, Shape,
    Stage,
};
use dageist_engine::document::{Document, Inputs, Segment, Walk, WalkedExpression};
use polars::prelude::{DataFrame, IntoLazy, df};
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, field_then, many, step, whole};

fn source() -> DataFrame {
    df!(
        "a" => [1_i64, 2, 3],
        "b" => ["x", "y", "z"],
        "f" => [true, false, true],
        "k" => [1_i64, 1, 2],
    )
    .expect("the table")
}

/// The walk of `LazyFrame.select(function(**arguments))`.
fn walk_of(function: &str, arguments: Value) -> Walk {
    let written = document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![chain(call(function, arguments), vec![])]) }),
    )]);
    Document::read(&written).expect("a document").steps()[0].walk(
        0,
        source().schema(),
        &Inputs::new(),
    )
}

/// The select item the function starts, which the walk reports after the arguments it holds.
fn selected(walk: &Walk) -> &WalkedExpression {
    walk.expressions()
        .iter()
        .find(|expression| {
            expression.place().path() == [Segment::Parameter("exprs".to_owned()), Segment::Item(0)]
        })
        .expect("the select item")
}

/// Where a link added after the function would stand: what the function gives.
fn gives(walk: &Walk) -> Option<OfferPlace> {
    selected(walk).places().last().copied().flatten()
}

fn select_after(dtype: Representative, shape: Shape) -> Option<OfferPlace> {
    Some(OfferPlace::new(
        Context::step(Outer::Select),
        After::Value { dtype, shape },
    ))
}

#[test]
fn what_a_function_is_given_in_text_is_refused_where_and_at_the_stage_polars_refuses_it() {
    // `duration` takes numbers: Polars refuses the text when it works out the schema.
    let text = walk_of("polars.duration", json!({ "days": field("b") }));
    let refusal = text.refusal().expect("text is no number of days");
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Polars(Stage::Schema)
    );
    assert!(
        refusal
            .refusal()
            .reason()
            .contains("expected integer or float dtype"),
        "{}",
        refusal.refusal().reason()
    );
    let number = walk_of("polars.duration", json!({ "days": field("a") }));
    assert_eq!(number.refusal(), None);
    assert_eq!(selected(&number).ending(), Some(&Ending::Allowed));
    // `from_epoch` of text multiplies it for seconds, which Polars refuses when it works out the
    // schema, and casts it to a date for days, which it refuses only when it runs the rows.
    for (unit, stage) in [(None, Stage::Schema), (Some("d"), Stage::Rows)] {
        let mut arguments = json!({ "column": field("b") });
        if let Some(unit) = unit {
            arguments["time_unit"] = json!({ "choice": unit });
        }
        let counted = walk_of("polars.from_epoch", arguments);
        let refusal = counted.refusal().unwrap_or_else(|| panic!("{unit:?}"));
        assert_eq!(
            refusal.refusal().source(),
            RefusalSource::Polars(stage),
            "{unit:?}"
        );
    }
    let epoch = walk_of("polars.from_epoch", json!({ "column": field("a") }));
    assert_eq!(epoch.refusal(), None);
}

#[test]
fn what_a_function_gives_is_one_value_of_one_value_parts_and_a_value_for_each_row_of_a_column() {
    let literal = |function: &str, extra: Value| {
        let mut arguments = json!({ "year": whole(2026), "month": whole(3), "day": whole(5) });
        arguments
            .as_object_mut()
            .expect("an object")
            .extend(extra.as_object().expect("an object").clone());
        walk_of(function, arguments)
    };
    assert_eq!(
        gives(&literal("polars.date", json!({}))),
        select_after(Representative::Date, Shape::One)
    );
    assert_eq!(
        gives(&literal("polars.date", json!({ "day": field("a") }))),
        select_after(Representative::Date, Shape::Many)
    );
    assert_eq!(
        gives(&literal("polars.datetime", json!({}))),
        select_after(Representative::Datetime, Shape::One)
    );
    assert_eq!(
        gives(&walk_of("polars.time", json!({ "hour": field("a") }))),
        select_after(Representative::Time, Shape::Many)
    );
    assert_eq!(
        gives(&walk_of("polars.time", json!({}))),
        select_after(Representative::Time, Shape::One)
    );
    assert_eq!(
        gives(&walk_of("polars.duration", json!({ "days": whole(1) }))),
        select_after(Representative::Duration, Shape::One)
    );
}

#[test]
fn a_moment_made_with_a_zone_is_in_it_and_a_count_of_days_is_a_date() {
    let zoned = walk_of(
        "polars.datetime",
        json!({
            "year": whole(2026),
            "month": whole(3),
            "day": whole(5),
            "time_zone": { "time_zone": "UTC" },
        }),
    );
    assert_eq!(
        gives(&zoned),
        select_after(Representative::DatetimeZoned, Shape::One)
    );
    let days = walk_of(
        "polars.from_epoch",
        json!({ "column": field("a"), "time_unit": { "choice": "d" } }),
    );
    assert_eq!(
        gives(&days),
        select_after(Representative::Date, Shape::Many)
    );
    let seconds = walk_of("polars.from_epoch", json!({ "column": field("a") }));
    assert_eq!(
        gives(&seconds),
        select_after(Representative::Datetime, Shape::Many)
    );
}

/// The start `function` given `arguments`, as an expression.
fn made_by(function: &str, arguments: Value) -> Value {
    chain(call(function, arguments), vec![])
}

/// The walk of `LazyFrame.join(other, left_on=[key], right_on=[rk])`, which a table `other` of
/// whole numbers `rk` is joined to.
fn join_walk(key: Value) -> Walk {
    join_keys_walk(vec![key], vec![field("rk")])
}

/// The walk of `LazyFrame.join(other, left_on=left, right_on=right)`.
fn join_keys_walk(left: Vec<Value>, right: Vec<Value>) -> Walk {
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "other",
            "Other",
            df!("rk" => [1_i64, 2, 3]).expect("the other table").lazy(),
        )
        .expect("a first input");
    let written = document(vec![step(
        "s",
        "LazyFrame.join",
        json!({
            "other": { "table": "other" },
            "left_on": many(left),
            "right_on": many(right),
        }),
    )]);
    Document::read(&written).expect("a document").steps()[0].walk(0, source().schema(), &inputs)
}

#[test]
fn a_key_of_a_function_given_an_aggregate_alone_is_elementwise_and_beside_a_column_is_not() {
    // Polars joins on a length of time of a sum: the sum is one value and no column is beside it.
    let alone = join_walk(made_by(
        "polars.duration",
        json!({ "days": field_then("a", vec![call("Expr.sum", json!({}))]) }),
    ));
    assert_eq!(alone.refusal(), None);
    // And refuses one that has a column beside the sum: the key is not elementwise.
    let beside = join_walk(made_by(
        "polars.duration",
        json!({
            "days": field_then("a", vec![call("Expr.sum", json!({}))]),
            "hours": field("k"),
        }),
    ));
    let refusal = beside.refusal().expect("a sum beside a column");
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Rule(Rule::KeyIsElementwise)
    );
}

#[test]
fn a_boolean_is_a_part_of_a_date_because_true_is_one_and_what_the_rows_hold_is_the_rows() {
    // A Boolean column of true is the month and the day 1, which Polars makes a date of: that a
    // row of false is no day is the rows' to say, so the walk does not refuse the dtype.
    for part in ["month", "day"] {
        let mut arguments = json!({ "year": whole(2026), "month": whole(1), "day": whole(1) });
        arguments[part] = field("f");
        for function in ["polars.date", "polars.datetime"] {
            let walk = walk_of(function, arguments.clone());
            assert_eq!(walk.refusal(), None, "{function} {part}");
        }
    }
}

#[test]
fn a_moment_and_a_length_of_time_are_named_as_their_first_part_for_the_keys_of_a_join() {
    // Polars names a function that is made of parts by the first of them, so two keys made of
    // `rk` are both named `rk`, and a join that pairs `a` with it twice repeats a pair of names.
    let moment = || {
        made_by(
            "polars.datetime",
            json!({ "year": field("rk"), "month": whole(1), "day": whole(1) }),
        )
    };
    let walk = join_keys_walk(vec![field("a"), field("a")], vec![moment(), moment()]);
    let refusal = walk.refusal().expect("the pair of names comes twice");
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Rule(Rule::JoinKeyNamesAreNotRepeated)
    );
    // A length of time is named by its weeks, which are the number 0 and so `literal` where none
    // are given, whatever else is.
    let length = || made_by("polars.duration", json!({ "days": field("rk") }));
    let walk = join_keys_walk(vec![field("a"), field("a")], vec![length(), length()]);
    let refusal = walk.refusal().expect("the pair of names comes twice");
    assert_eq!(
        refusal.refusal().source(),
        RefusalSource::Rule(Rule::JoinKeyNamesAreNotRepeated)
    );
}

#[test]
fn a_moment_or_a_length_of_time_of_numbers_typed_in_is_no_join_key_because_polars_aliases_it() {
    // Polars makes a literal of the call and names it by an alias, which a join's key may not hold.
    let folded = [
        ("polars.duration", json!({})),
        (
            "polars.duration",
            json!({ "days": whole(1), "hours": whole(2) }),
        ),
        (
            "polars.datetime",
            json!({ "year": whole(2026), "month": whole(3), "day": whole(5) }),
        ),
    ];
    for (function, arguments) in folded {
        let walk = join_walk(made_by(function, arguments.clone()));
        let refusal = walk
            .refusal()
            .unwrap_or_else(|| panic!("{function} {arguments}"));
        let RefusalSource::Rule(rule) = refusal.refusal().source() else {
            panic!("{function}: {refusal:?}");
        };
        assert_eq!(
            rule.name(),
            "folded-function-is-no-join-key",
            "{function} {arguments}"
        );
        assert!(
            refusal.refusal().reason().contains("alias"),
            "{}",
            refusal.refusal().reason()
        );
    }
    // A column among the parts, or a zone, leaves the call a call, which has no alias.
    let kept = [
        ("polars.duration", json!({ "days": field("a") })),
        (
            "polars.datetime",
            json!({
                "year": whole(2026),
                "month": whole(3),
                "day": whole(5),
                "time_zone": { "time_zone": "UTC" },
            }),
        ),
        (
            "polars.datetime",
            json!({ "year": field("a"), "month": whole(3), "day": whole(5) }),
        ),
    ];
    for (function, arguments) in kept {
        let walk = join_walk(made_by(function, arguments.clone()));
        assert_eq!(walk.refusal(), None, "{function} {arguments}");
    }
}

#[test]
fn what_a_function_gives_is_not_said_where_the_dtype_of_an_argument_is_not_known() {
    // A column the table does not have has no dtype, so the tables are not asked of the argument,
    // and Polars, which may refuse the argument for its dtype, is not said to give a date.
    let walk = walk_of(
        "polars.date",
        json!({ "year": whole(2026), "month": whole(1), "day": field("missing") }),
    );
    assert_eq!(gives(&walk), None);
    assert!(walk.not_known().is_some());
    // Where every argument is known, it is.
    let known = walk_of(
        "polars.date",
        json!({ "year": whole(2026), "month": whole(1), "day": field("k") }),
    );
    assert_eq!(
        gives(&known),
        select_after(Representative::Date, Shape::Many)
    );
}
