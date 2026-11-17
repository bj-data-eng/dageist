//! A number typed without a dtype is a dyn literal that adapts to the operand it
//! is used with, and the tables are measured with typed columns. What the walk
//! says of a chain that holds one is held to what Polars gives the same
//! expression: it says the dtype Polars gives, or that it does not know, and
//! never another.

use dageist_engine::allowed::{After, Ending, Representative, Unknown};
use dageist_engine::document::{Document, Inputs};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, document, field, many, step, whole};

fn source() -> DataFrame {
    df!(
        "u8" => [1_u8, 2],
        "f32" => [1.0_f32, 2.0],
        "i8" => [1_i8, 2],
        "a" => [1_i64, 2],
        "f" => [true, false],
    )
    .expect("the table")
}

fn fraction(number: f64) -> Value {
    chain(
        call("polars.lit", json!({ "value": { "float": number } })),
        vec![],
    )
}

/// The dtype class the walk gives after the last link of the select item `item`, if it knows.
fn walked(item: Value) -> Option<Representative> {
    let text = document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![item]) }),
    )]);
    let document = Document::read(&text).expect("a document");
    let walk = document.steps()[0].walk(0, source().schema(), &Inputs::new());
    let expression = walk.expressions().last().expect("the item");
    match expression.places().last().copied().flatten()?.after() {
        After::Value { dtype, .. } => Some(dtype),
        _ => None,
    }
}

/// What Polars gives `expression` selected from the table.
fn polars(expression: Expr) -> Option<Representative> {
    let schema = source().lazy().select([expression]).collect_schema().ok()?;
    let (_, dtype) = schema.iter().next()?;
    Representative::of(dtype)
}

/// The walk says what Polars gives, or that it does not know.
#[track_caller]
fn agrees(walk: Option<Representative>, polars: Option<Representative>) {
    assert!(
        walk.is_none() || walk == polars,
        "the walk says {walk:?}, Polars gives {polars:?}"
    );
}

fn sub(other: Value) -> Value {
    call("Expr.sub", json!({ "other": other }))
}

#[test]
fn a_number_beside_a_narrow_column_leaves_the_dtype_of_what_it_gives_not_known() {
    // `u8 - 1` is a `UInt8`, and `f32 - 1` a `Float32`: the number adapts to the column.
    let cases = [
        (
            chain(
                call("polars.col", json!({ "name": { "columns": ["u8"] } })),
                vec![sub(whole(1))],
            ),
            col("u8") - lit(1),
        ),
        (
            chain(
                call("polars.col", json!({ "name": { "columns": ["f32"] } })),
                vec![sub(whole(1))],
            ),
            col("f32") - lit(1),
        ),
        (
            chain(
                call("polars.lit", json!({ "value": { "integer": 1 } })),
                vec![sub(field("u8"))],
            ),
            lit(1) - col("u8"),
        ),
    ];
    for (item, expression) in cases {
        let polars = polars(expression);
        assert!(polars.is_some());
        agrees(walked(item), polars);
    }
}

#[test]
fn a_number_among_the_values_of_a_condition_adapts_only_to_what_it_fits() {
    let condition = |then: &str, otherwise: Value| {
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![field("f")]) }),
            ),
            vec![
                call("When.then", json!({ "statement": field(then) })),
                call("Then.otherwise", json!({ "statement": otherwise })),
            ],
        )
    };
    // A number with a fraction does not adapt down to whole numbers, and 1000 is no `Int8`.
    let cases = [
        (
            condition("a", fraction(1.5)),
            when(col("f")).then(col("a")).otherwise(lit(1.5)),
        ),
        (
            condition("i8", whole(1000)),
            when(col("f")).then(col("i8")).otherwise(lit(1000)),
        ),
        // 300 is no `UInt8`, and a whole number beside a `Float32` is a `Float32`.
        (
            condition("u8", whole(300)),
            when(col("f")).then(col("u8")).otherwise(lit(300)),
        ),
        (
            condition("f32", whole(1)),
            when(col("f")).then(col("f32")).otherwise(lit(1)),
        ),
    ];
    for (item, expression) in cases {
        agrees(walked(item), polars(expression));
    }
}

#[test]
fn a_number_that_stands_alone_or_counts_rows_or_adapts_unambiguously_is_still_known() {
    // `lit(1).is_null()` is a Boolean, `shift(1)` leaves the column's dtype, and `0` beside whole
    // numbers is a whole number: the engine can say so, and the editor offers what follows.
    let null = chain(
        call("polars.lit", json!({ "value": { "integer": 1 } })),
        vec![call("Expr.is_null", json!({}))],
    );
    assert_eq!(walked(null), Some(Representative::Boolean));
    let shifted = chain(
        call("polars.col", json!({ "name": { "columns": ["u8"] } })),
        vec![call("Expr.shift", json!({ "n": whole(1) }))],
    );
    assert_eq!(walked(shifted), Some(Representative::UInt8));
    let adapted = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("f")]) }),
        ),
        vec![
            call("When.then", json!({ "statement": field("a") })),
            call("Then.otherwise", json!({ "statement": whole(0) })),
        ],
    );
    assert_eq!(walked(adapted), Some(Representative::Int64));
    assert_eq!(
        polars(when(col("f")).then(col("a")).otherwise(lit(0))),
        Some(Representative::Int64)
    );
}

#[test]
fn a_number_that_is_a_conditions_predicate_is_refused_as_polars_refuses_it() {
    let item = chain(
        call("polars.when", json!({ "predicates": many(vec![whole(1)]) })),
        vec![
            call("When.then", json!({ "statement": field("a") })),
            call("Then.otherwise", json!({ "statement": field("a") })),
        ],
    );
    let text = document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![item]) }),
    )]);
    let document = Document::read(&text).expect("a document");
    let walk = document.steps()[0].walk(0, source().schema(), &Inputs::new());
    assert!(
        walk.refusal().is_some(),
        "the walk allows a number as a predicate"
    );
    // Polars refuses it when it runs.
    let refused = source()
        .lazy()
        .select([when(lit(1)).then(col("a")).otherwise(col("a"))])
        .collect();
    assert!(refused.is_err());
}

#[test]
fn numbers_typed_without_a_dtype_alone_in_a_condition_have_none_to_give() {
    // `when(f).then(1).otherwise(1)` is numbers Polars cannot always resolve: as a key of a sort
    // it refuses it, as the fuzz target found. The walk does not say what it gives.
    let item = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("f")]) }),
        ),
        vec![
            call("When.then", json!({ "statement": whole(1) })),
            call("Then.otherwise", json!({ "statement": whole(2) })),
        ],
    );
    assert_eq!(walked(item), None);
}

#[test]
fn a_condition_with_no_otherwise_that_holds_a_number_is_not_known_to_end() {
    // Polars refuses `when(f).then(1)` where it runs, as the fuzz target found, so whether the
    // expression may end where it stands is not known, though the tables allow it.
    let item = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("f")]) }),
        ),
        vec![call("When.then", json!({ "statement": whole(1) }))],
    );
    let text = document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![item]) }),
    )]);
    let read = Document::read(&text).expect("a document");
    let walk = read.steps()[0].walk(0, source().schema(), &Inputs::new());
    let expression = walk.expressions().last().expect("the item");
    assert_eq!(
        expression.ending(),
        Some(&Ending::NotKnown(Unknown::TheResultsDtype))
    );
    // With a value that is typed, the tables are not contradicted: it may end.
    let typed = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("f")]) }),
        ),
        vec![call("When.then", json!({ "statement": field("a") }))],
    );
    let text = document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![typed]) }),
    )]);
    let read = Document::read(&text).expect("a document");
    let walk = read.steps()[0].walk(0, source().schema(), &Inputs::new());
    assert_ne!(
        walk.expressions().last().expect("the item").ending(),
        Some(&Ending::NotKnown(Unknown::TheResultsDtype))
    );
}
