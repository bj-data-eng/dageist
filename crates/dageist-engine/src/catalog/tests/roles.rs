//! The parameters that meet the value their method follows in a way of their own,
//! as Polars' documentation describes the methods: the other side of arithmetic
//! and of a comparison, the value `shift` fills with, and the rows it counts.

use crate::catalog::{self, Role};

#[test]
fn only_the_operands_the_fill_and_the_row_count_have_a_role() {
    let mut found: Vec<(&str, &str, Role)> = catalog::entries()
        .iter()
        .flat_map(|entry| {
            entry
                .parameters()
                .iter()
                .map(move |parameter| (entry.id().as_str(), parameter.name(), parameter.role()))
        })
        .filter(|(_, _, role)| *role != Role::Plain)
        .collect();
    found.sort_by_key(|(entry, parameter, _)| (*entry, *parameter));
    assert_eq!(
        found,
        [
            ("Expr.eq", "other", Role::Operand),
            ("Expr.ge", "other", Role::Operand),
            ("Expr.gt", "other", Role::Operand),
            ("Expr.le", "other", Role::Operand),
            ("Expr.lt", "other", Role::Operand),
            ("Expr.ne", "other", Role::Operand),
            ("Expr.shift", "fill_value", Role::Filler),
            ("Expr.shift", "n", Role::Count),
            ("Expr.sub", "other", Role::Operand),
        ]
    );
}

#[test]
fn a_parameter_that_holds_one_value_for_each_item_of_another_is_a_flag_or_a_choice() {
    // The step editor draws such a parameter as a control at the end of the row of each item,
    // which has room for a flag or a choice and for no list of its own, and says that it does
    // not draw another kind. This keeps that unreachable: a parameter of another kind with this
    // shape is to be drawn before it is added.
    let mut per_item = Vec::new();
    for entry in catalog::entries() {
        for parameter in entry.parameters() {
            if matches!(parameter.shape(), catalog::Shape::PerItem(_)) {
                assert!(
                    matches!(
                        parameter.kind(),
                        catalog::Kind::Flag | catalog::Kind::Choice(_)
                    ),
                    "{} {}: {:?}",
                    entry.id().as_str(),
                    parameter.name(),
                    parameter.kind()
                );
                per_item.push((entry.id().as_str(), parameter.name()));
            }
        }
    }
    per_item.sort_unstable();
    assert_eq!(
        per_item,
        [
            ("LazyFrame.sort", "descending"),
            ("LazyFrame.sort", "nulls_last")
        ]
    );
}
