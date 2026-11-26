//! The arguments an edit makes from what was typed: a literal given to a parameter of a kind.

use dageist_engine::allowed::Literal;
use dageist_engine::catalog::{BuildError, Kind, ValueType};
use dageist_engine::document::{Cause, EditProblem, Segment, Value};
use serde_json::json;

use super::{inside, parameter, read, refused};
use crate::support::{call, chain, field_then, many, step};

fn text(word: &str) -> Literal {
    Literal::String(word.to_owned())
}

#[test]
fn text_is_the_kind_of_argument_its_parameter_takes() {
    let word = || text("1mo");
    assert_eq!(
        Value::from_literal(&word(), Kind::Duration),
        Value::Duration("1mo".to_owned())
    );
    assert_eq!(
        Value::from_literal(&text("[aeiou]"), Kind::Pattern),
        Value::Pattern("[aeiou]".to_owned())
    );
    assert_eq!(
        Value::from_literal(&text("Europe/Berlin"), Kind::TimeZone),
        Value::TimeZone(Some("Europe/Berlin".to_owned()))
    );
    assert_eq!(
        Value::from_literal(&text("data/documents.parquet"), Kind::Path),
        Value::Path("data/documents.parquet".to_owned())
    );
    assert_eq!(
        Value::from_literal(&word(), Kind::Value(ValueType::String)),
        Value::String("1mo".to_owned())
    );
}

#[test]
fn a_number_is_the_kind_of_number_its_parameter_takes() {
    assert_eq!(
        Value::from_literal(&Literal::Integer(3), Kind::Value(ValueType::Integer)),
        Value::Integer(3)
    );
    // A whole number given to a parameter of numbers is the number it names.
    assert_eq!(
        Value::from_literal(&Literal::Integer(3), Kind::Value(ValueType::Float)),
        Value::Float(3.0)
    );
    assert_eq!(
        Value::from_literal(&Literal::Float(2.5), Kind::Value(ValueType::Float)),
        Value::Float(2.5)
    );
    assert_eq!(
        Value::from_literal(&Literal::Integer(3), Kind::Value(ValueType::Any)),
        Value::Integer(3)
    );
}

#[test]
fn the_largest_whole_number_a_float_holds_exactly_is_still_a_number() {
    // 2^53 is the last whole number below which every whole number is exact.
    let exact = 1_i64 << 53;
    assert_eq!(
        Value::from_literal(&Literal::Integer(exact), Kind::Value(ValueType::Float)),
        Value::Float(9_007_199_254_740_992.0)
    );
}

#[test]
fn a_whole_number_a_float_cannot_hold_exactly_stays_whole_for_the_model_to_refuse() {
    // 2^53 + 1 is the first whole number that has no float of its own.
    let inexact = (1_i64 << 53) + 1;
    assert_eq!(
        Value::from_literal(&Literal::Integer(inexact), Kind::Value(ValueType::Float)),
        Value::Integer(inexact)
    );
    assert_eq!(
        Value::from_literal(&Literal::Integer(i64::MAX), Kind::Value(ValueType::Float)),
        Value::Integer(i64::MAX)
    );
    // Put into a parameter of numbers, it is refused where it was put and nothing is rounded.
    let document = read(vec![step(
        "close",
        "LazyFrame.select",
        json!({ "exprs": many(vec![field_then("a", vec![call("Expr.is_close", json!({ "other": chain(json!({ "hole": null }), vec![]) }))])]) }),
    )]);
    let abs_tol = inside(
        0,
        "close",
        vec![
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(0),
            parameter("abs_tol"),
        ],
    );
    let error = refused(document.set_argument(
        &abs_tol,
        Value::from_literal(&Literal::Integer(inexact), Kind::Value(ValueType::Float)),
    ));
    assert!(matches!(
        error.problem(),
        EditProblem::Refused(Cause::Arguments(BuildError::WrongKind {
            parameter: "abs_tol",
            ..
        }))
    ));
    assert_eq!(error.place(), &abs_tol);
}
