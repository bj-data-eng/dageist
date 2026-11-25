//! Whether a method gives one value or many where its receiver is one value and an operand is
//! many, against what Polars gives, for every method of the catalogue that takes an expression.
//!
//! Polars meets a receiver of one value with each value of an operand of many, so the call gives
//! many. The catalogue holds that for each parameter, apart from how a number typed without a
//! dtype adapts, and the walk reads it: a method added without saying so is found here.

use polars::prelude::{DataFrame, DataType, IntoLazy, NamedFrom, Series, lit};

use crate::allowed::dtype::Representative;
use crate::allowed::table::Shape;
use crate::allowed::{After, Chosen, Context, Outer, Place, offer};
use crate::catalog::{self, Applies, Argument, Entry, Kind, Shape as ParameterShape};

use super::build::{
    column, entry, expression, expressions, integer, method, null_of, one_of, patterns, step,
};
use super::probe::measure;
use super::sample::Frames;

/// Whether the method gives a count of values that the data or its arguments decide, which the walk
/// states from the arguments and these measurements of one value and many cannot: the count of
/// `is_in` follows the dtype of the values it is given, a list column's being met row by row and a
/// column of the values' own dtype being their set.
fn gives_a_count(entry: &Entry) -> bool {
    catalog::Counting::of(entry, &[]).is_some() || entry.id().as_str() == "Expr.is_in"
}

/// A value of `dtype` that is one value: the number 1 where it can be, and a null where not.
fn literal_of(dtype: &DataType) -> polars::prelude::Expr {
    one_of(dtype)
        .or_else(|_| null_of(dtype))
        .expect("a literal of the dtype")
}

/// The expression parameters of the method, with `name` the column `b` and every other that must
/// be given a literal of `operand`.
fn arguments(
    entry: &Entry,
    name: &str,
    operand: Representative,
    column_named: &str,
) -> Vec<(&'static str, Argument)> {
    let mut given: Vec<(&'static str, Argument)> = patterns(entry);
    for parameter in entry.parameters() {
        if !matches!(parameter.kind(), Kind::Expression(_)) {
            continue;
        }
        let value = if parameter.name() == name {
            if column_named.is_empty() {
                literal_of(&operand.dtype())
            } else if SHARES.contains(&(entry.id().as_str(), name)) {
                // The sample's column holds 1, a null and 3, which are no share, and Polars refuses a
                // null: a quarter of each is one, and a half stands for the null.
                (column(column_named) / lit(4.0)).fill_null(lit(0.5))
            } else {
                column(column_named)
            }
        } else if parameter.is_needed() {
            literal_of(&operand.dtype())
        } else {
            continue;
        };
        given.push((
            parameter.name(),
            match parameter.shape() {
                ParameterShape::Many => expressions(vec![value]),
                _ => expression(value),
            },
        ));
    }
    given
}

/// What Polars gives for the method with the receiver `receiver_expr` and the parameter `name` the
/// expression `operand_column` stands for: one value or many.
fn gives(
    entry: &'static Entry,
    name: &str,
    receivers: (Representative, bool),
    operand: Representative,
) -> Option<Shape> {
    let (receiver, one_valued) = receivers;
    let aggregated = AGGREGATE_RECEIVER.contains(&(entry.id().as_str(), name));
    let frames = Frames::new(receiver, operand, &DataType::Int64);
    let plan = |frames: &Frames, empty: bool| {
        // A receiver of one value is a literal and the operand a column; a receiver of many is a
        // column and the operand a literal.
        let call = method(
            entry,
            if one_valued && aggregated {
                column("a").first()
            } else if one_valued {
                literal_of(&receiver.dtype())
            } else {
                column("a")
            },
            arguments(
                entry,
                name,
                operand,
                match (one_valued, aggregated) {
                    // A column of text with no null: Polars refuses a replacement of every match
                    // given a null that it meets in a piece of one row, at more threads than the
                    // tables were measured at, so the one that holds a null measures nothing.
                    (true, true) => "g",
                    (true, false) => "b",
                    (false, _) => "",
                },
            ),
        )?;
        step(
            "LazyFrame.select",
            frames.left(empty),
            vec![("exprs", expressions(vec![call]))],
        )
    };
    measure(&plan, &frames)
        .ok()
        .and_then(|measured| measured.produces())
        .map(|produces| produces.shape)
}

/// The first receiver and operand dtypes with which Polars allows the method with the parameter
/// `name` a column beside a literal receiver, and what it gives.
fn found(entry: &'static Entry, name: &str) -> Option<(Representative, Representative, Shape)> {
    Representative::MEASURED.into_iter().find_map(|receiver| {
        Representative::MEASURED.into_iter().find_map(|operand| {
            gives(entry, name, (receiver, true), operand).map(|shape| (receiver, operand, shape))
        })
    })
}

/// The parameters that Polars takes a column of as shares from 0 to 1, and refuses a column of the
/// sample's numbers or a null for: they are measured with a column of shares made of its numbers.
const SHARES: [(&str, &str); 1] = [("Expr.approx_quantile", "quantile")];

/// The parameters measured with an aggregate of the column `a` as the receiver of one value, and
/// not a literal: Polars meets a text of one value that is an aggregate with each value of what
/// replaces it, and refuses a literal text beside them for a table with no rows, which the tables
/// count as refused, so a literal receiver measures nothing of them.
const AGGREGATE_RECEIVER: [(&str, &str); 2] = [
    ("Expr.str.replace", "value"),
    ("Expr.str.replace_all", "value"),
];

/// The parameters that Polars allows no operand of many for, with why: nothing is measured of them.
/// `over`'s keys are met by the window, which has a rule of its own.
const NOT_MEASURED: [(&str, &str, &str); 9] = [
    ("Expr.over", "more_exprs", "a key of the window"),
    ("Expr.replace", "new", "the values a replacement lists"),
    (
        "Expr.replace_strict",
        "new",
        "the values a replacement lists",
    ),
    (
        "Expr.replace_strict",
        "default",
        "the values a replacement lists",
    ),
    ("Expr.diff", "n", "Polars takes one value only"),
    ("Expr.pct_change", "n", "Polars takes one value only"),
    ("Expr.shift", "n", "Polars takes one value only"),
    ("Expr.shift", "fill_value", "Polars takes one value only"),
    ("Expr.quantile", "quantile", "Polars takes one value only"),
];

#[test]
fn the_catalogue_says_for_each_expression_parameter_whether_polars_meets_a_receiver_of_one_value_with_each_value()
 {
    let mut measured = 0;
    // The threads Polars runs on, which its pool says: a machine of one core has one as well.
    let one_thread = polars_core::runtime::THREAD_POOL.current_num_threads() == 1;
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
        // What a method that gives a count of values that the data or the arguments decide gives
        // is that count, which the walk states from the arguments and these measurements of one
        // value and many cannot.
        .filter(|entry| !gives_a_count(entry))
    {
        for parameter in entry
            .parameters()
            .iter()
            .filter(|parameter| matches!(parameter.kind(), Kind::Expression(_)))
        {
            let id = entry.id().as_str();
            let name = parameter.name();
            match found(entry, name) {
                Some((receiver, operand, shape)) => {
                    let message = format!(
                        "{id} {name}: a literal {receiver} given a column {operand} gives {shape:?}"
                    );
                    if one_thread {
                        // On one thread Polars does not meet a literal with each value of a
                        // column in some calls, where it does on more: it never gives many
                        // where the catalogue says there is one.
                        assert!(shape != Shape::Many || parameter.broadcasts(), "{message}");
                    } else {
                        assert_eq!(parameter.broadcasts(), shape == Shape::Many, "{message}");
                    }
                    measured += 1;
                }
                None => {
                    assert!(
                        NOT_MEASURED
                            .iter()
                            .any(|(owner, parameter, _)| *owner == id && *parameter == name),
                        "Polars allows {id} {name} given a column beside a literal of no dtype: \
                         the test cannot say whether it broadcasts"
                    );
                    // What takes one value only, or lists its values, does not broadcast.
                    if id != "Expr.over" {
                        assert!(!parameter.broadcasts(), "{id} {name}");
                    }
                }
            }
        }
    }
    assert!(measured > 30, "only {measured} parameters were measured");
}

#[test]
fn a_method_given_a_literal_beside_a_column_receiver_gives_what_the_tables_say() {
    // The other way round, a receiver of many and an operand of one value: what the method gives
    // is what the table of its dtypes says, so the widening of a receiver of one value is the only
    // thing the walk adds.
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
        .filter(|entry| !gives_a_count(entry))
    {
        for parameter in entry
            .parameters()
            .iter()
            .filter(|parameter| matches!(parameter.kind(), Kind::Expression(_)))
        {
            let Some((receiver, operand, _)) = found(entry, parameter.name()) else {
                continue;
            };
            let Some(polars) = gives(entry, parameter.name(), (receiver, false), operand) else {
                continue;
            };
            let place = Place::new(
                Context::step(Outer::Select),
                After::Value {
                    dtype: receiver,
                    shape: Shape::Many,
                },
            );
            let verdict = offer(
                entry,
                &place,
                &Chosen::default().with_arguments(&[(parameter.name(), operand)]),
            );
            let Some(allowance) = verdict.allowance() else {
                continue;
            };
            assert_eq!(
                allowance.shape(),
                polars,
                "{} {}: a column {receiver} given a literal {operand}",
                entry.id().as_str(),
                parameter.name()
            );
        }
    }
}

#[test]
fn the_parameters_of_a_method_that_gives_a_count_say_what_polars_does_with_one_value_and_many() {
    // `gives_a_count` leaves these methods out of the measurements above, for what they give is
    // the count their arguments decide and not the one value or the rows those compare. What the
    // catalogue says of each of their parameters is held to Polars here, by how many values it
    // gives of a literal receiver, one value, and a column of three rows for the parameter.
    crate::startup::ensure();
    let lists = Series::new(
        "l".into(),
        [[1_i64, 2], [3, 4], [5, 6]].map(|pair| Series::new("".into(), pair)),
    );
    let frame = DataFrame::new(
        3,
        vec![
            Series::new("t".into(), [true, true, true]).into(),
            Series::new("i".into(), [0_i64, 0, 0]).into(),
            lists.into(),
        ],
    )
    .expect("columns of three rows");
    let rows = |id: &str, parameter: &str, given: &str| -> usize {
        let method_entry = entry(id);
        let shape = method_entry
            .parameters()
            .iter()
            .find(|candidate| candidate.name() == parameter)
            .expect("the entry has the parameter")
            .shape();
        let value = column(given);
        let argument = match shape {
            ParameterShape::Many => expressions(vec![value]),
            _ => expression(value),
        };
        let call = method(
            method_entry,
            integer(5),
            vec![(parameter_name(id, parameter), argument)],
        )
        .expect("the call builds");
        step(
            "LazyFrame.select",
            frame.clone().lazy(),
            vec![("exprs", expressions(vec![call]))],
        )
        .expect("a select")
        .collect()
        .expect("Polars runs it")
        .height()
    };
    // One value met by the three values of the parameter is three values: it broadcasts.
    for (id, parameter, given) in [
        ("Expr.filter", "predicates", "t"),
        ("Expr.gather", "indices", "i"),
        ("Expr.search_sorted", "element", "i"),
        // A list column is met row by row; a column of the value's own dtype is their set, whole,
        // which is one answer.
        ("Expr.is_in", "other", "l"),
    ] {
        assert_eq!(rows(id, parameter, given), 3, "{id} {parameter}");
        assert!(
            entry(id)
                .parameters()
                .iter()
                .find(|candidate| candidate.name() == parameter)
                .is_some_and(catalog::Parameter::broadcasts),
            "{id} {parameter} broadcasts"
        );
    }
    assert_eq!(rows("Expr.is_in", "other", "i"), 1);
    // The counts and the places a method takes are one value, which Polars does not meet a receiver
    // of one value with each of many for.
    for method_entry in catalog::entries()
        .iter()
        .filter(|candidate| catalog::Counting::of(candidate, &[]).is_some())
    {
        for parameter in method_entry
            .parameters()
            .iter()
            .filter(|parameter| parameter.role().is_a_count())
        {
            assert!(
                !parameter.broadcasts(),
                "{} {} takes one value",
                method_entry.id().as_str(),
                parameter.name()
            );
        }
    }
}

/// The name of the parameter `parameter` of the entry `id`, as the entry holds it.
fn parameter_name(id: &str, parameter: &str) -> &'static str {
    entry(id)
        .parameters()
        .iter()
        .find(|candidate| candidate.name() == parameter)
        .map(catalog::Parameter::name)
        .expect("the entry has the parameter")
}
