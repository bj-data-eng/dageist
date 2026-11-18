//! The parameters whose expressions write columns of the table a step gives,
//! which decide that a column a step computes is replaced even when it keeps
//! its dtype.

use crate::catalog::{self, Kind, Writes};

#[test]
fn only_the_expressions_of_select_and_with_columns_write_columns() {
    let mut found: Vec<(&str, &str)> = catalog::entries()
        .iter()
        .flat_map(|entry| {
            entry
                .parameters()
                .iter()
                .filter(|parameter| parameter.writes() == Writes::Columns)
                .map(move |parameter| (entry.id().as_str(), parameter.name()))
        })
        .collect();
    found.sort_unstable();
    assert_eq!(
        found,
        [
            ("LazyFrame.select", "exprs"),
            ("LazyFrame.select", "named_exprs"),
            ("LazyFrame.with_columns", "exprs"),
            ("LazyFrame.with_columns", "named_exprs"),
        ]
    );
}

#[test]
fn a_parameter_that_writes_columns_holds_expressions_or_names_with_expressions() {
    for entry in catalog::entries() {
        for parameter in entry.parameters() {
            if parameter.writes() == Writes::Columns {
                assert!(
                    matches!(
                        parameter.kind(),
                        Kind::Expression(_) | Kind::Pairs(Kind::Column, Kind::Expression(_))
                    ),
                    "{} {}",
                    entry.id().as_str(),
                    parameter.name()
                );
            }
        }
    }
}
