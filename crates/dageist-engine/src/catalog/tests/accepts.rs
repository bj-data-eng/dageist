//! What each expression method says it accepts, asked of Polars: the method is
//! built on a column of every dtype it claims, and Polars resolves it against
//! the table's schema and runs it on the table's rows; then on a column of a
//! dtype it does not claim, and Polars refuses it.
//!
//! Polars is the authority here: an entry that claims a dtype Polars refuses,
//! or leaves out one it needs to offer, is wrong.

use polars::prelude::{DataType, Expr, LazyFrame, PolarsResult, col, lit};

use crate::catalog::{Applies, Argument, Arguments, Build, Dtypes, Entry, Kind, entries};

use super::build::{expression_arg, many, sample};

/// The sample table with a column of `Time`, which it lacks.
fn typed_sample() -> LazyFrame {
    sample().with_columns([col("ts").dt().time().alias("tm")])
}

/// Each column of the table, and the groups of dtypes its dtype belongs to.
const COLUMNS: &[(&str, &[Dtypes])] = &[
    ("i", &[Dtypes::Numeric, Dtypes::Integer]),
    ("f", &[Dtypes::Numeric]),
    ("ts", &[Dtypes::Temporal, Dtypes::Datetime]),
    ("d", &[Dtypes::Temporal, Dtypes::Date]),
    ("tm", &[Dtypes::Temporal, Dtypes::Time]),
    ("dur", &[Dtypes::Temporal, Dtypes::Duration]),
    ("s", &[Dtypes::String]),
    ("b", &[Dtypes::Boolean]),
];

/// Whether a column of the groups `groups` is one an entry accepting `accepts` takes.
fn accepted(accepts: &[Dtypes], groups: &[Dtypes]) -> bool {
    accepts.contains(&Dtypes::Any) || groups.iter().any(|group| accepts.contains(group))
}

/// The arguments with which `entry` is used on `column`: its required parameters, each an
/// expression that suits it, and those an expression method cannot do without.
fn arguments_for(entry: &Entry, column: &str) -> Arguments {
    let mut given: Vec<(&str, Argument)> = entry
        .parameters()
        .iter()
        .filter(|parameter| parameter.default() == crate::catalog::ParameterDefault::Required)
        .map(|parameter| match parameter.kind() {
            // `by` of `dt.offset_by` is a duration string; anything else is compared with, or
            // taken from, the column itself, which has the dtype the method takes.
            Kind::Expression(&[Dtypes::String]) => (parameter.name(), expression_arg(lit("1d"))),
            Kind::Expression(_) => (parameter.name(), expression_arg(col(column))),
            other => panic!("{} takes {other:?}", entry.id().as_str()),
        })
        .collect();
    // Python's `over` is refused without something to group or order by.
    if entry.id().as_str() == "Expr.over" {
        given.push(("partition_by", many(vec![expression_arg(col("s"))])));
    }
    entry.arguments(given).expect("arguments that fit")
}

fn built(entry: &Entry, column: &str) -> Expr {
    let Build::Expression(build) = entry.build() else {
        panic!("{} is not an expression method", entry.id().as_str());
    };
    build(col(column), &arguments_for(entry, column)).expect("the method builds")
}

/// What Polars says of `expr` on the sample: its dtype from the schema, which reads no data,
/// then the rows, which run it.
fn polars_resolves(expr: Expr) -> PolarsResult<DataType> {
    let frame = typed_sample().select([expr.alias("out")]);
    let dtype = frame
        .clone()
        .collect_schema()?
        .get("out")
        .expect("one column, out")
        .clone();
    frame.collect()?;
    Ok(dtype)
}

#[test]
fn polars_resolves_each_method_on_every_dtype_it_claims_to_accept() {
    let mut checked = 0;
    for entry in entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
    {
        for (column, groups) in COLUMNS {
            if accepted(entry.accepts(), groups) {
                let result = polars_resolves(built(entry, column));
                assert!(
                    result.is_ok(),
                    "{} claims {:?} but Polars refuses it on {column}: {}",
                    entry.id().as_str(),
                    entry.accepts(),
                    result.unwrap_err()
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "{checked} method and dtype pairs were built");
}

#[test]
fn polars_refuses_each_method_on_a_dtype_it_does_not_accept() {
    let mut refused = 0;
    for entry in entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
    {
        // One dtype the entry does not claim, where there is one in the table.
        let Some((column, _)) = COLUMNS
            .iter()
            .find(|(_, groups)| !accepted(entry.accepts(), groups))
        else {
            continue;
        };
        let result = polars_resolves(built(entry, column));
        assert!(
            result.is_err(),
            "{} does not claim {column}, which Polars resolves to {:?}",
            entry.id().as_str(),
            result.unwrap()
        );
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
        polars_resolves(built(entry, column))
    };
    assert_eq!(resolves("Expr.dt.date", "d").unwrap(), DataType::Date);
    assert_eq!(resolves("Expr.dt.time", "tm").unwrap(), DataType::Time);
    assert_eq!(resolves("Expr.cum_max", "s").unwrap(), DataType::String);
    assert_eq!(resolves("Expr.cum_max", "b").unwrap(), DataType::Boolean);
    assert!(resolves("Expr.dt.date", "tm").is_err());
    assert!(resolves("Expr.dt.time", "d").is_err());
}
