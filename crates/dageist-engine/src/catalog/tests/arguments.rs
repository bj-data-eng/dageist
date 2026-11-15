//! The arguments of an entry, as a saved step gives them, checked against the
//! parameters the entry's Python signature lists.

use polars::prelude::{DataType, col, lit};

use crate::catalog::{Argument, Arguments, Build, BuildError, entries, entry};

use super::build::{choice, columns, expression_arg, flag, many, pairs, sample, table_arg, text};

fn given(id: &str, arguments: Vec<(&str, Argument)>) -> Result<Arguments, BuildError> {
    entry(id).expect("an entry").arguments(arguments)
}

fn on_i() -> Argument {
    many(vec![expression_arg(col("i"))])
}

#[test]
fn arguments_that_fit_every_parameter_are_kept_for_building() {
    let arguments = given(
        "LazyFrame.join",
        vec![
            ("other", table_arg()),
            ("on", on_i()),
            ("how", choice("left")),
            ("suffix", text("_other")),
            ("coalesce", Argument::Flag(None)),
            ("allow_parallel", flag(false)),
        ],
    )
    .expect("arguments that fit");
    assert!(arguments.get("other").is_some());
    assert!(arguments.get("how").is_some());
    // A parameter left out takes its default.
    assert!(arguments.get("validate").is_none());

    let Build::Frame(build) = entry("LazyFrame.join").expect("an entry").build() else {
        panic!("join is a step");
    };
    let joined = build(sample(), &arguments).expect("the step builds");
    assert_eq!(joined.collect().expect("rows").height(), 3);
}

#[test]
fn no_arguments_are_arguments_too() {
    for each in entries() {
        let arguments = each.arguments([]).expect("nothing to refuse");
        for parameter in each.parameters() {
            assert!(arguments.get(parameter.name()).is_none());
        }
    }
}

#[test]
fn a_name_that_is_not_a_parameter_of_the_entry_is_refused() {
    // `Expr.shift(n=1, *, fill_value=None)` has no `periods`, and `eq`'s `other` is not
    // a parameter of `shift`.
    for name in ["periods", "other", "", "N"] {
        let error = given("Expr.shift", vec![(name, Argument::Integer(1))])
            .err()
            .expect("an unknown parameter");
        assert!(
            matches!(&error, BuildError::UnknownParameter { parameter } if parameter == name),
            "{name}: {error}"
        );
    }
}

#[test]
fn a_parameter_given_twice_is_refused() {
    let error = given(
        "Expr.shift",
        vec![("n", expression_arg(lit(1))), ("n", expression_arg(lit(2)))],
    )
    .err()
    .expect("a repeated parameter");
    assert!(
        matches!(error, BuildError::Repeated { parameter: "n" }),
        "{error}"
    );
}

#[test]
fn an_argument_of_another_kind_than_its_parameter_is_refused_with_both_named() {
    // Each case: entry, parameter, argument, what the parameter takes, what was given.
    let cases: Vec<(&str, &str, Argument, &str, &str)> = vec![
        (
            "Expr.eq",
            "other",
            Argument::Integer(1),
            "an expression",
            "a whole number",
        ),
        ("Expr.first", "ignore_nulls", text("yes"), "a flag", "text"),
        (
            "LazyFrame.join",
            "other",
            expression_arg(col("i")),
            "a table",
            "an expression",
        ),
        (
            "LazyFrame.join",
            "suffix",
            Argument::Integer(1),
            "text",
            "a whole number",
        ),
        ("polars.col", "name", text("i"), "column names", "text"),
        ("polars.lit", "dtype", text("Int64"), "a dtype", "text"),
        (
            "polars.lit",
            "value",
            columns(&["i"]),
            "a number, text or a flag",
            "column names",
        ),
        (
            "Expr.dt.total_days",
            "fractional",
            Argument::Choice("true".to_owned()),
            "a flag",
            "a choice",
        ),
        ("LazyFrame.join", "how", text("left"), "a choice", "text"),
    ];
    for (id, parameter, argument, expected, found) in cases {
        let error = given(id, vec![(parameter, argument)])
            .err()
            .expect("an argument of another kind");
        assert!(
            matches!(
                &error,
                BuildError::WrongKind { parameter: named, expected: wanted, found: got }
                    if *named == parameter && *wanted == expected && *got == found
            ),
            "{id} {parameter}: {error}"
        );
    }
}

#[test]
fn a_parameter_of_shape_many_takes_a_list_and_each_item_must_fit() {
    // `LazyFrame.sort(by, *more_by, ...)`: `by` and `more_by` are lists of expressions.
    for parameter in ["by", "more_by"] {
        let error = given(
            "LazyFrame.sort",
            vec![(parameter, expression_arg(col("i")))],
        )
        .err()
        .expect("one expression is not a list");
        assert!(
            matches!(
                &error,
                BuildError::WrongKind { parameter: named, expected: "a list", found: "an expression" }
                    if *named == parameter
            ),
            "{parameter}: {error}"
        );
        let error = given("LazyFrame.sort", vec![(parameter, many(vec![flag(true)]))])
            .err()
            .expect("a flag is not an expression");
        assert!(
            matches!(
                &error,
                BuildError::WrongKind {
                    expected: "an expression",
                    found: "a flag",
                    ..
                }
            ),
            "{parameter}: {error}"
        );
        assert!(given("LazyFrame.sort", vec![(parameter, many(Vec::new()))]).is_ok());
    }
    // A list of lists is no list of expressions.
    let nested = many(vec![many(vec![expression_arg(col("i"))])]);
    assert!(given("LazyFrame.sort", vec![("by", nested)]).is_err());
}

#[test]
fn a_parameter_of_shape_one_does_not_take_a_list() {
    let error = given(
        "Expr.eq",
        vec![("other", many(vec![expression_arg(lit(1))]))],
    )
    .err()
    .expect("a list is not one expression");
    assert!(
        matches!(
            error,
            BuildError::WrongKind {
                parameter: "other",
                expected: "an expression",
                found: "a list"
            }
        ),
        "{error}"
    );
}

#[test]
fn a_parameter_per_item_takes_one_argument_or_a_list_of_them() {
    // `LazyFrame.sort(..., descending=False, ...)`: one flag, or one for each key.
    assert!(given("LazyFrame.sort", vec![("descending", flag(true))]).is_ok());
    assert!(
        given(
            "LazyFrame.sort",
            vec![("descending", many(vec![flag(true), flag(false)]))]
        )
        .is_ok()
    );
    let error = given("LazyFrame.sort", vec![("descending", Argument::Integer(1))])
        .err()
        .expect("an integer is not a flag");
    assert!(
        matches!(
            error,
            BuildError::WrongKind {
                parameter: "descending",
                expected: "a flag",
                found: "a whole number"
            }
        ),
        "{error}"
    );
    let error = given(
        "LazyFrame.sort",
        vec![("descending", many(vec![flag(true), text("no")]))],
    )
    .err()
    .expect("an item that is not a flag");
    assert!(
        matches!(error, BuildError::WrongKind { found: "text", .. }),
        "{error}"
    );
}

#[test]
fn a_word_that_is_not_one_of_the_choices_is_refused_before_building() {
    let error = given("LazyFrame.join", vec![("how", choice("outer"))])
        .err()
        .expect("an unknown word");
    assert!(
        matches!(&error, BuildError::UnknownChoice { parameter: "how", word } if word == "outer"),
        "{error}"
    );
    assert!(given("LazyFrame.join", vec![("how", choice("cross"))]).is_ok());
}

#[test]
fn pairs_are_checked_on_each_side() {
    // `LazyFrame.rename(mapping)`: a column name to a new name, which is text.
    let ok = pairs(vec![(columns(&["i"]), text("index"))]);
    assert!(given("LazyFrame.rename", vec![("mapping", ok)]).is_ok());
    let value = pairs(vec![(columns(&["i"]), expression_arg(col("i")))]);
    let error = given("LazyFrame.rename", vec![("mapping", value)])
        .err()
        .expect("an expression is not a column name");
    assert!(
        matches!(
            error,
            BuildError::WrongKind {
                parameter: "mapping",
                expected: "text",
                found: "an expression"
            }
        ),
        "{error}"
    );
    let key = pairs(vec![(text("i"), text("index"))]);
    assert!(given("LazyFrame.rename", vec![("mapping", key)]).is_err());
    // `LazyFrame.with_columns(**named_exprs)`: a column name to an expression.
    let named = pairs(vec![(columns(&["x"]), expression_arg(col("i")))]);
    assert!(given("LazyFrame.with_columns", vec![("named_exprs", named)]).is_ok());
    // A list of names is not pairs.
    let not_pairs = many(vec![columns(&["x"])]);
    assert!(given("LazyFrame.with_columns", vec![("named_exprs", not_pairs)]).is_err());
}

#[test]
fn lit_takes_a_value_of_any_type_a_step_can_type() {
    for value in [
        Argument::Integer(5),
        Argument::Float(2.5),
        text("x"),
        flag(true),
        Argument::Flag(None),
    ] {
        assert!(given("polars.lit", vec![("value", value)]).is_ok());
    }
    let with_dtype = vec![
        ("value", Argument::Integer(5)),
        ("dtype", Argument::Dtype(DataType::Int8)),
    ];
    assert!(given("polars.lit", with_dtype).is_ok());
    assert!(given("polars.lit", vec![("value", many(vec![]))]).is_err());
}

#[test]
fn arguments_that_fit_can_still_be_refused_by_building() {
    // The check is of kind and shape. A step that is missing what it needs, or whose list
    // does not match its keys, is for building to report, with Polars' own reasons.
    let arguments = given("LazyFrame.sort", vec![("descending", flag(true))]).expect("fits");
    let Build::Frame(build) = entry("LazyFrame.sort").expect("an entry").build() else {
        panic!("sort is a step");
    };
    let error = build(sample(), &arguments).err().expect("no keys");
    assert!(
        matches!(error, BuildError::Missing { parameter: "by" }),
        "{error}"
    );
}
