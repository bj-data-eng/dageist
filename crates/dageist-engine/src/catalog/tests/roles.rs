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
