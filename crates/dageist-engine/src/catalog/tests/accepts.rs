//! What each expression method says it accepts, asked of Polars: the method is
//! built on a column of every dtype it claims, and Polars resolves it against
//! the table's schema and runs it on the table's rows; then on a column of a
//! dtype it does not claim, and Polars refuses it.
//!
//! Polars is the authority here: an entry that claims a dtype Polars refuses,
//! or leaves out one it needs to offer, is wrong.
//!
//! A method that takes an operand, such as `add`, accepts a receiver when Polars resolves it
//! with some column of the table as the operand: a datetime has no sum with a datetime, and
//! one with a duration. It refuses a receiver only when it refuses every column.

use polars::prelude::{
    DataType, Expr, LazyFrame, NonExistent, PolarsError, PolarsResult, TimeZone, col, concat_list,
    lit,
};

use crate::catalog::{
    Applies, Argument, Arguments, Build, Dtypes, Entry, Kind, Role, Shape, ValueType, Variadic,
    entries,
};

use super::build::{expression_arg, many, sample};

/// The group of dtypes the catalogue names `key`, which a list or an array is one of.
fn group(key: &str) -> Dtypes {
    Dtypes::ALL
        .into_iter()
        .find(|group| group.key() == key)
        .unwrap_or_else(|| panic!("the catalogue has no group of dtypes called {key}"))
}

/// The sample table with a column of `Time`, one of a `Datetime` in a time zone, a column of lists
/// of two whole numbers and one of arrays of two, which it lacks.
fn typed_sample() -> LazyFrame {
    sample().with_columns([
        concat_list([col("i"), col("i") * lit(10)])
            .expect("a list of two")
            .alias("l"),
        concat_list([col("i"), col("i") * lit(10)])
            .expect("a list of two")
            .list()
            .to_array(2)
            .alias("ar"),
        col("ts").dt().time().alias("tm"),
        col("ts")
            .dt()
            .replace_time_zone(
                TimeZone::opt_try_new(Some("UTC")).expect("a zone"),
                lit("raise"),
                NonExistent::Raise,
            )
            .alias("tz"),
    ])
}

/// The dtype of the column `name` of the table.
fn dtype_of(name: &str) -> DataType {
    typed_sample()
        .collect_schema()
        .expect("the table has a schema")
        .get(name)
        .expect("the table has the column")
        .clone()
}

/// Each column of the table, and the groups of dtypes its dtype belongs to.
fn columns() -> Vec<(&'static str, Vec<Dtypes>)> {
    vec![
        ("i", vec![Dtypes::Numeric, Dtypes::Integer]),
        ("f", vec![Dtypes::Numeric]),
        ("ts", vec![Dtypes::Temporal, Dtypes::Datetime]),
        ("tz", vec![Dtypes::Temporal, Dtypes::Datetime]),
        ("d", vec![Dtypes::Temporal, Dtypes::Date]),
        ("tm", vec![Dtypes::Temporal, Dtypes::Time]),
        ("dur", vec![Dtypes::Temporal, Dtypes::Duration]),
        ("s", vec![Dtypes::String]),
        ("b", vec![Dtypes::Boolean]),
        ("l", vec![group("List")]),
        ("ar", vec![group("Array")]),
    ]
}

/// The methods that read a time zone a `Datetime` has: Polars refuses the sample's `ts`, which has
/// none, and the sample's `tz` is the column of the group they take.
const ONLY_WITH_A_ZONE: [&str; 2] = ["Expr.dt.base_utc_offset", "Expr.dt.dst_offset"];

/// Whether a column of the groups `groups` is one an entry accepting `accepts` takes.
fn accepted(accepts: &[Dtypes], groups: &[Dtypes]) -> bool {
    accepts.contains(&Dtypes::Any) || groups.iter().any(|group| accepts.contains(group))
}

/// Whether `entry` takes an operand beside what it follows, a column of the table that its
/// receiver has no say in.
fn takes_an_operand(entry: &Entry) -> bool {
    entry.parameters().iter().any(|parameter| {
        matches!(
            parameter.role(),
            Role::Operand
                | Role::Filler
                | Role::Exponent
                | Role::Divisor
                | Role::FloorDivisor
                | Role::Base
                | Role::Bound
        ) && matches!(parameter.kind(), Kind::Expression(_))
    })
}

/// The arguments with which `entry` is used on `column`: its required parameters, each an
/// expression that suits it, and those an expression method cannot do without. An operand is
/// `operand`, a column of the table; the other expressions are `column` itself.
fn arguments_for(entry: &Entry, column: &str, operand: &str) -> Arguments {
    let mut given: Vec<(&str, Argument)> = entry
        .parameters()
        .iter()
        // `and_(*others)` and `clip` are their receiver as it was without an operand or a bound,
        // and so say nothing of the receiver's dtype: they are given one, as a step is.
        .filter(|parameter| parameter.is_needed())
        .map(|parameter| {
            let value = match parameter.kind() {
                // `by` of `dt.offset_by` is a duration string; anything else is compared with,
                // or taken from, a column of the table.
                Kind::Expression(&[Dtypes::String]) => expression_arg(lit("1d")),
                // The time of day `combine` puts on a date or a moment.
                Kind::Expression(&[Dtypes::Time]) => expression_arg(col("tm")),
                // What the method keeps is chosen by a test that holds for some of the values.
                Kind::Expression(&[Dtypes::Boolean]) => expression_arg(col("b")),
                // The first place of what a method gathers, which Polars takes of any column.
                Kind::Expression(&[Dtypes::Integer]) if parameter.name() == "indices" => {
                    expression_arg(lit(0))
                }
                // The values `top_k` and `bottom_k` take are one count, and Polars refuses a column.
                Kind::Expression(_) if matches!(parameter.name(), "k") => expression_arg(lit(1)),
                // The length or the count of characters of a method of text is a whole number.
                Kind::Expression(_) if parameter.role().is_a_count() => expression_arg(col("i")),
                // A quantile is a share from 0 to 1, and a single value, which Polars refuses a
                // column of for the exact quantile and a column of whole numbers for any.
                Kind::Expression(_) if parameter.name() == "quantile" => expression_arg(lit(0.5)),
                Kind::Expression(_) if parameter.role() != Role::Plain => {
                    expression_arg(col(operand))
                }
                Kind::Expression(_) => expression_arg(col(column)),
                // `round_sig_figs`'s `digits`: any whole number it takes will do.
                Kind::Value(ValueType::Integer) => Argument::Integer(2),
                // `cast`'s `dtype`: the dtype the column has, which converts to itself.
                Kind::Dtype => Argument::Dtype(dtype_of(column)),
                // The format that every dtype of a date or a time takes.
                Kind::Value(ValueType::String) => Argument::String("iso".to_owned()),
                // A time zone Polars reads.
                Kind::TimeZone => Argument::TimeZone(Some("UTC".to_owned())),
                // A time unit that Polars counts a moment in: the first of the words.
                Kind::Choice(words) => Argument::Choice(words[0].to_owned()),
                // A pattern that every method of text that takes one accepts: a group of `a`.
                Kind::Pattern => Argument::Pattern("(a)".to_owned()),
                other => panic!("{} takes {other:?}", entry.id().as_str()),
            };
            let value = if parameter.variadic() == Some(Variadic::Args)
                || parameter.shape() == Shape::Many
            {
                many(vec![value])
            } else {
                value
            };
            (parameter.name(), value)
        })
        .collect();
    // A replacement is refused where what it replaces holds a value twice, as the sample's text
    // and booleans do, and where it replaces too few values and gives no default, so each is a
    // single value, one of the column and the one it is replaced by.
    if matches!(entry.id().as_str(), "Expr.replace" | "Expr.replace_strict") {
        given.retain(|(name, _)| !matches!(*name, "old" | "new"));
        given.push(("old", expression_arg(col(column).first())));
        given.push(("new", expression_arg(col(operand).first())));
        if entry.id().as_str() == "Expr.replace_strict" {
            given.push(("default", expression_arg(col(operand).first())));
        }
    }
    // Python's `over` is refused without something to group or order by.
    if entry.id().as_str() == "Expr.over" {
        given.push(("partition_by", many(vec![expression_arg(col("s"))])));
    }
    entry.arguments(given).expect("arguments that fit")
}

fn built(entry: &Entry, column: &str, operand: &str) -> Expr {
    let Build::Expression(build) = entry.build() else {
        panic!("{} is not an expression method", entry.id().as_str());
    };
    build(col(column), &arguments_for(entry, column, operand)).expect("the method builds")
}

/// The columns that are tried as the operand of `entry` beside a receiver `column`: all of them
/// where it takes one, and otherwise `column`, which is no operand.
fn operands(entry: &Entry, column: &str) -> Vec<&'static str> {
    if takes_an_operand(entry) {
        columns()
            .into_iter()
            .map(|(name, _)| name)
            // Polars drops the test for NaN that `fill_nan` is made of where the value is the
            // expression it follows, as it drops the predicate of a condition whose two values
            // are one expression, so a column beside itself says nothing of what it accepts.
            .filter(|name| entry.id().as_str() != "Expr.fill_nan" || *name != column)
            .collect()
    } else {
        columns()
            .into_iter()
            .map(|(name, _)| name)
            .filter(|name| *name == column)
            .collect()
    }
}

/// What Polars says of `expr` on the sample: its dtype from the schema, which reads no data,
/// then the rows, which run it. A method whose schema says one dtype and whose rows give another
/// is not resolved: `sqrt` of a date says it is a `Date` and gives a `Float64`, which the tables of
/// allowed combinations refuse as they do any such call.
fn polars_resolves(expr: Expr) -> PolarsResult<DataType> {
    let frame = typed_sample().select([expr.alias("out")]);
    let dtype = frame
        .clone()
        .collect_schema()?
        .get("out")
        .expect("one column, out")
        .clone();
    let rows = frame.collect()?;
    let given = rows.column("out")?.dtype().clone();
    if given != dtype {
        return Err(PolarsError::SchemaMismatch(
            format!("the schema says {dtype} and the rows give {given}").into(),
        ));
    }
    Ok(dtype)
}

#[test]
fn polars_resolves_each_method_on_every_dtype_it_claims_to_accept() {
    let mut checked = 0;
    let mut refused = Vec::new();
    for entry in entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
    {
        for (column, groups) in &columns() {
            if *column == "ts" && ONLY_WITH_A_ZONE.contains(&entry.id().as_str()) {
                continue;
            }
            if accepted(entry.accepts(), groups) {
                let results: Vec<_> = operands(entry, column)
                    .into_iter()
                    .map(|operand| polars_resolves(built(entry, column, operand)))
                    .collect();
                if !results.iter().any(Result::is_ok) {
                    refused.push(format!(
                        "{} claims {:?} but Polars refuses it on {column}: {}",
                        entry.id().as_str(),
                        entry.accepts(),
                        first_line(results[0].as_ref().unwrap_err())
                    ));
                }
                checked += 1;
            }
        }
    }
    assert!(
        refused.is_empty(),
        "{} methods claim a dtype Polars refuses:\n{}",
        refused.len(),
        refused.join("\n")
    );
    assert!(checked > 100, "{checked} method and dtype pairs were built");
}

/// The first line of what Polars says.
fn first_line(error: &PolarsError) -> String {
    error
        .to_string()
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn polars_refuses_each_method_on_a_dtype_it_does_not_accept() {
    let mut refused = 0;
    for entry in entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
    {
        // One dtype the entry does not claim, where there is one in the table.
        let Some((column, _)) = columns()
            .into_iter()
            .find(|(_, groups)| !accepted(entry.accepts(), groups))
        else {
            continue;
        };
        for operand in operands(entry, column) {
            let result = polars_resolves(built(entry, column, operand));
            assert!(
                result.is_err(),
                "{} does not claim {column}, which Polars resolves to {:?} beside {operand}",
                entry.id().as_str(),
                result.unwrap()
            );
        }
        refused += 1;
    }
    assert!(refused >= 5, "{refused} methods were refused a dtype");
}

#[test]
fn the_dtypes_a_method_claims_are_those_the_documentation_names() {
    // The three examples of the review of the first part, by name: `dt.date` takes a Date and
    // `dt.time` a Time, which pass through unchanged, and `cum_max` takes text and booleans.
    let resolves = |id: &str, column: &str| {
        let entry = crate::catalog::entry(id).expect("an entry");
        polars_resolves(built(entry, column, column))
    };
    assert_eq!(resolves("Expr.dt.date", "d").unwrap(), DataType::Date);
    assert_eq!(resolves("Expr.dt.time", "tm").unwrap(), DataType::Time);
    assert_eq!(resolves("Expr.cum_max", "s").unwrap(), DataType::String);
    assert_eq!(resolves("Expr.cum_max", "b").unwrap(), DataType::Boolean);
    assert!(resolves("Expr.dt.date", "tm").is_err());
    assert!(resolves("Expr.dt.time", "d").is_err());
}
