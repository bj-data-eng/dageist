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

/// The walk says what Polars gives `item`, or that it does not know.
#[track_caller]
fn agrees_in(item: &Value, walk: Option<Representative>, polars: Option<Representative>) {
    assert!(
        walk.is_none() || walk == polars,
        "{item}: the walk says {walk:?}, Polars gives {polars:?}"
    );
}

/// The select item `item` as the walk reads it, and `expression` as Polars does, each against
/// the table with a column for every dtype class, named by the class.
fn everywhere(item: Value, expression: Expr) -> (Option<Representative>, Option<Representative>) {
    let schema: Schema = Representative::ALL
        .iter()
        .map(|class| Field::new(class.name().into(), class.dtype()))
        .collect();
    let text = document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![item]) }),
    )]);
    let document = Document::read(&text).expect("a document");
    let walk = document.steps()[0].walk(0, &schema, &Inputs::new());
    let walk = walk
        .expressions()
        .last()
        .and_then(|expression| expression.places().last().copied().flatten())
        .and_then(|place| match place.after() {
            After::Value { dtype, .. } => Some(dtype),
            _ => None,
        });
    let polars = DataFrame::empty_with_schema(&schema)
        .lazy()
        .select([expression])
        .collect_schema()
        .ok()
        .and_then(|schema| {
            let (_, dtype) = schema.iter().next()?;
            Representative::of(dtype)
        });
    (walk, polars)
}

fn on_a_column(name: &str, method: Value) -> Value {
    chain(
        call("polars.col", json!({ "name": { "columns": [name] } })),
        vec![method],
    )
}

fn compare(method: &str, other: Value) -> Value {
    call(method, json!({ "other": other }))
}

#[test]
fn a_number_that_fits_the_column_it_is_used_with_gives_what_polars_gives() {
    // The number adapts to the column: `u8 - 1` is a `UInt8` and `f32 - 1` a `Float32`, a number
    // with a fraction beside whole numbers makes a `Float64`, and a number in front of the column
    // adapts as one behind it does.
    let cases = [
        (
            on_a_column("u8", sub(whole(1))),
            col("u8") - lit(1),
            Representative::UInt8,
        ),
        (
            on_a_column("f32", sub(whole(1))),
            col("f32") - lit(1),
            Representative::Float32,
        ),
        (
            on_a_column("i8", sub(whole(1))),
            col("i8") - lit(1),
            Representative::Int8,
        ),
        (
            on_a_column("a", sub(whole(1))),
            col("a") - lit(1),
            Representative::Int64,
        ),
        (
            chain(
                call("polars.lit", json!({ "value": { "integer": 1 } })),
                vec![sub(field("u8"))],
            ),
            lit(1) - col("u8"),
            Representative::UInt8,
        ),
        (
            on_a_column("u8", sub(fraction(1.5))),
            col("u8") - lit(1.5),
            Representative::Float64,
        ),
        (
            on_a_column("f32", sub(fraction(1.5))),
            col("f32") - lit(1.5),
            Representative::Float32,
        ),
        (
            chain(
                call("polars.lit", json!({ "value": { "float": 1.5 } })),
                vec![sub(field("u8"))],
            ),
            lit(1.5) - col("u8"),
            Representative::Float64,
        ),
        (
            on_a_column("u8", call("Expr.shift", json!({ "fill_value": whole(1) }))),
            col("u8").shift_and_fill(lit(1), lit(1)),
            Representative::UInt8,
        ),
    ];
    for (item, expression, expected) in cases {
        assert_eq!(polars(expression), Some(expected), "what Polars gives");
        assert_eq!(walked(item), Some(expected), "what the walk gives");
    }
}

#[test]
fn a_number_raised_to_a_power_or_divided_keeps_the_dtype_polars_gives_it() {
    // A number in front of `**` is no operand that adapts: `2 ** u8` is an `Int32`, as the number
    // is alone, and `2 ** f32` a `Float32`, which the exponent decides. A number with a fraction
    // in front of `/` is a `Float64` beside any float, where a whole number adapts to the float:
    // `1.5 / f32` is a `Float64` and `2 / f32` a `Float32`.
    let number = |value: i64| call("polars.lit", json!({ "value": { "integer": value } }));
    let fractional = |value: f64| call("polars.lit", json!({ "value": { "float": value } }));
    let cases = [
        (
            chain(
                number(2),
                vec![call("Expr.pow", json!({ "exponent": field("u8") }))],
            ),
            lit(2).pow(col("u8")),
            Representative::Int32,
        ),
        (
            chain(
                number(2),
                vec![call("Expr.pow", json!({ "exponent": field("f32") }))],
            ),
            lit(2).pow(col("f32")),
            Representative::Float32,
        ),
        (
            chain(
                fractional(1.5),
                vec![call("Expr.pow", json!({ "exponent": field("u8") }))],
            ),
            lit(1.5).pow(col("u8")),
            Representative::Float64,
        ),
        (
            chain(
                fractional(1.5),
                vec![call("Expr.truediv", json!({ "other": field("f32") }))],
            ),
            lit(1.5).true_div(col("f32")),
            Representative::Float64,
        ),
        (
            chain(
                number(2),
                vec![call("Expr.truediv", json!({ "other": field("f32") }))],
            ),
            lit(2).true_div(col("f32")),
            Representative::Float32,
        ),
        (
            chain(
                number(2),
                vec![call("Expr.truediv", json!({ "other": field("u8") }))],
            ),
            lit(2).true_div(col("u8")),
            Representative::Float64,
        ),
    ];
    for (item, expression, expected) in cases {
        assert_eq!(polars(expression), Some(expected), "what Polars gives");
        assert_eq!(
            walked(item.clone()),
            Some(expected),
            "what the walk gives {item}"
        );
    }
}

/// The dtype class the walk gives after the last link of the item `item` of the parameter
/// `parameter` of the step `entry`, if it knows.
fn walked_in(entry: &str, parameter: &str, item: Value) -> Option<Representative> {
    let text = document(vec![step(
        "s",
        entry,
        json!({ parameter: many(vec![item]) }),
    )]);
    let document = Document::read(&text).expect("a document");
    let walk = document.steps()[0].walk(0, source().schema(), &Inputs::new());
    let expression = walk.expressions().last().expect("the item");
    match expression.places().last().copied().flatten()?.after() {
        After::Value { dtype, .. } => Some(dtype),
        _ => None,
    }
}

#[test]
fn an_integer_raised_to_a_fraction_is_a_float64_where_polars_resolves_it_and_not_known_in_a_key() {
    // Polars gives `i8 ** 1.5` the unresolved dtype of the number, which a select resolves to a
    // `Float64`, and a sort key or a join's key does not: it refuses them for it. So the dtype is
    // known where it is selected and not where it is a key.
    let power = || on_a_column("i8", call("Expr.pow", json!({ "exponent": fraction(1.5) })));
    assert_eq!(
        polars(col("i8").pow(lit(1.5))),
        Some(Representative::Float64)
    );
    assert_eq!(
        walked_in("LazyFrame.select", "exprs", power()),
        Some(Representative::Float64)
    );
    assert_eq!(walked_in("LazyFrame.sort", "by", power()), None);
    // A float it fits is no unresolved number: `f32 ** 1.5` is a `Float32` everywhere.
    let float = on_a_column(
        "f32",
        call("Expr.pow", json!({ "exponent": fraction(1.5) })),
    );
    assert_eq!(
        walked_in("LazyFrame.sort", "by", float),
        Some(Representative::Float32)
    );
}

#[test]
fn a_comparison_with_a_number_is_a_boolean_whatever_dtype_the_number_takes() {
    // The result of a comparison does not depend on the dtype of its operand, so it is known
    // beside a column the number fits and beside one it does not.
    let cases = [
        (
            on_a_column("a", compare("Expr.gt", whole(1))),
            col("a").gt(lit(1)),
        ),
        (
            on_a_column("u8", compare("Expr.gt", whole(300))),
            col("u8").gt(lit(300)),
        ),
        (
            on_a_column("u8", compare("Expr.lt", fraction(1.5))),
            col("u8").lt(lit(1.5)),
        ),
        (
            on_a_column("f32", compare("Expr.ge", whole(1))),
            col("f32").gt_eq(lit(1)),
        ),
        (
            on_a_column("i8", compare("Expr.eq", whole(1000))),
            col("i8").eq(lit(1000)),
        ),
        (
            on_a_column("f", compare("Expr.ne", whole(1))),
            col("f").neq(lit(1)),
        ),
        (
            chain(
                call("polars.lit", json!({ "value": { "integer": 1 } })),
                vec![compare("Expr.gt", field("u8"))],
            ),
            lit(1).gt(col("u8")),
        ),
    ];
    for (item, expression) in cases {
        assert_eq!(polars(expression), Some(Representative::Boolean));
        assert_eq!(walked(item), Some(Representative::Boolean));
    }
}

#[test]
fn a_whole_number_that_does_not_fit_the_column_is_not_given_the_columns_dtype() {
    // Polars widens the column: `u8 - 300` is a `UInt16`, `u8 - -1` an `Int16` and `i8 - 1000` an
    // `Int16`, which the walk does not say, and it never says the dtype of the column instead.
    let cases = [
        (
            on_a_column("u8", sub(whole(300))),
            col("u8") - lit(300),
            Representative::UInt16,
        ),
        (
            on_a_column("u8", sub(whole(-1))),
            col("u8") - lit(-1),
            Representative::Int16,
        ),
        (
            on_a_column("i8", sub(whole(1000))),
            col("i8") - lit(1000),
            Representative::Int16,
        ),
        (
            chain(
                call("polars.lit", json!({ "value": { "integer": 300 } })),
                vec![sub(field("u8"))],
            ),
            lit(300) - col("u8"),
            Representative::UInt16,
        ),
        (
            on_a_column(
                "u8",
                call("Expr.shift", json!({ "fill_value": whole(300) })),
            ),
            col("u8").shift_and_fill(lit(1), lit(300)),
            Representative::UInt16,
        ),
    ];
    for (item, expression, expected) in cases {
        assert_eq!(polars(expression), Some(expected), "what Polars gives");
        assert_eq!(walked(item), None);
    }
}

#[test]
fn what_follows_a_number_beside_a_column_is_asked_of_the_tables() {
    // After `u8 - 1` the chain has a `UInt8`, and after `a > 1` a `Boolean`, so what follows is
    // asked: `dt.total_days` takes a duration, and neither is one.
    for (first, polars_item) in [
        (sub(whole(1)), (col("u8") - lit(1)).dt().total_days(false)),
        (
            compare("Expr.gt", whole(1)),
            col("u8").gt(lit(1)).dt().total_days(false),
        ),
    ] {
        let item = on_a_column("u8", first);
        let item = {
            let mut item = item;
            item["expression"]["then"]
                .as_array_mut()
                .expect("links")
                .push(call("Expr.dt.total_days", json!({})));
            item
        };
        let text = document(vec![step(
            "s",
            "LazyFrame.select",
            json!({ "exprs": many(vec![item]) }),
        )]);
        let document = Document::read(&text).expect("a document");
        let walk = document.steps()[0].walk(0, source().schema(), &Inputs::new());
        assert!(
            walk.refusal().is_some(),
            "the walk does not ask what follows the number"
        );
        assert!(source().lazy().select([polars_item]).collect().is_err());
    }
}

/// Numbers typed without a dtype, each as the document holds it and as Polars does: ones that
/// fit a narrow column and ones that do not, whole and with a fraction.
fn numbers() -> Vec<(Value, Expr)> {
    vec![
        (whole(1), lit(1_i64)),
        (whole(-1), lit(-1_i64)),
        (whole(127), lit(127_i64)),
        (whole(128), lit(128_i64)),
        (whole(255), lit(255_i64)),
        (whole(256), lit(256_i64)),
        (whole(300), lit(300_i64)),
        (whole(70_000), lit(70_000_i64)),
        (whole(3_000_000_000), lit(3_000_000_000_i64)),
        (whole(5_000_000_000), lit(5_000_000_000_i64)),
        (fraction(1.5), lit(1.5_f64)),
        (fraction(1.0), lit(1.0_f64)),
        (fraction(1e40), lit(1e40_f64)),
    ]
}

/// How Polars combines two expressions.
type Combine = fn(Expr, Expr) -> Expr;

/// A method that takes another expression, and how Polars combines the two: the entry, the
/// parameter that holds the other one, whether the parameter holds a list, and the combination.
struct Operation {
    entry: &'static str,
    parameter: &'static str,
    list: bool,
    combine: Combine,
}

impl Operation {
    const fn new(entry: &'static str, combine: Combine) -> Self {
        Self {
            entry,
            parameter: "other",
            list: false,
            combine,
        }
    }

    /// The call of the method given `other`.
    fn given(&self, other: Value) -> Value {
        let other = if self.list { many(vec![other]) } else { other };
        call(self.entry, json!({ self.parameter: other }))
    }
}

/// Every class as a column of the table `everywhere` reads, beside each number, in each of
/// `operations`, with the number behind the column and in front of it. How many of the
/// combinations the walk knows the dtype of.
fn numbers_beside_every_dtype(operations: &[Operation]) -> usize {
    let numbers = numbers();
    let mut known = 0;
    for class in Representative::ALL {
        let name = class.name();
        for (number, literal) in &numbers {
            let start = match number["expression"]["start"].clone() {
                start if start.is_object() => start,
                _ => unreachable!("a number is an expression"),
            };
            for operation in operations {
                let build = operation.combine;
                let behind = (
                    on_a_column(name, operation.given(number.clone())),
                    build(col(name), literal.clone()),
                );
                let in_front = (
                    chain(start.clone(), vec![operation.given(field(name))]),
                    build(literal.clone(), col(name)),
                );
                for (item, expression) in [behind, in_front] {
                    let (walk, polars) = everywhere(item.clone(), expression);
                    known += usize::from(walk.is_some());
                    agrees_in(&item, walk, polars);
                }
            }
        }
    }
    known
}

#[test]
fn a_number_beside_any_dtype_is_known_only_as_polars_gives_it() {
    let known = numbers_beside_every_dtype(&[
        Operation::new("Expr.sub", |left, right| left - right),
        Operation::new("Expr.gt", |left, right| left.gt(right)),
        Operation::new("Expr.eq", |left, right| left.eq(right)),
    ]);
    // The walk knows what a number that fits gives and what a comparison gives, and not what
    // every number beside every dtype gives.
    assert!(known > 100, "the walk knew {known} of them");
}

#[test]
fn a_number_beside_any_dtype_in_arithmetic_and_logic_is_known_only_as_polars_gives_it() {
    let known = numbers_beside_every_dtype(&[
        Operation::new("Expr.add", |left, right| left + right),
        Operation::new("Expr.mul", |left, right| left * right),
        Operation::new("Expr.truediv", |left, right| left.true_div(right)),
        Operation::new("Expr.floordiv", |left, right| left.floor_div(right)),
        Operation::new("Expr.mod", |left, right| left % right),
        Operation {
            parameter: "exponent",
            ..Operation::new("Expr.pow", |left, right| left.pow(right))
        },
        Operation::new("Expr.xor", |left, right| left.xor(right)),
        Operation {
            parameter: "others",
            list: true,
            ..Operation::new("Expr.and_", |left, right| left.and(right))
        },
        Operation {
            parameter: "others",
            list: true,
            ..Operation::new("Expr.or_", |left, right| left.or(right))
        },
    ]);
    assert!(known > 500, "the walk knew {known} of them");
}

#[test]
fn a_number_as_a_fill_value_is_known_only_as_polars_gives_it() {
    let mut known = 0;
    for class in Representative::ALL {
        let name = class.name();
        for (number, literal) in numbers() {
            let shifted = on_a_column(
                name,
                call("Expr.shift", json!({ "fill_value": number.clone() })),
            );
            let (walk, polars) = everywhere(
                shifted.clone(),
                col(name).shift_and_fill(lit(1), literal.clone()),
            );
            known += usize::from(walk.is_some());
            agrees_in(&shifted, walk, polars);
        }
    }
    assert!(known > 100, "the walk knew {known} of them");
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
