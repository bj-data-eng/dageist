//! The parameters that meet the value their method follows in a way of their own,
//! as Polars' documentation describes the methods: the other side of arithmetic
//! and of a comparison, the value `shift` fills with, and the rows it counts.

use crate::catalog::{self, Role};

#[test]
fn only_the_operands_the_fill_the_row_count_the_power_and_the_divisors_have_a_role() {
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
            ("Expr.add", "other", Role::Operand),
            ("Expr.and_", "others", Role::Operand),
            ("Expr.clip", "lower_bound", Role::Bound),
            ("Expr.clip", "upper_bound", Role::Bound),
            ("Expr.eq", "other", Role::Operand),
            ("Expr.eq_missing", "other", Role::Operand),
            ("Expr.fill_nan", "value", Role::Filler),
            ("Expr.fill_null", "value", Role::Filler),
            ("Expr.floordiv", "other", Role::FloorDivisor),
            ("Expr.ge", "other", Role::Operand),
            ("Expr.gt", "other", Role::Operand),
            ("Expr.is_between", "lower_bound", Role::Operand),
            ("Expr.is_between", "upper_bound", Role::Operand),
            ("Expr.is_close", "other", Role::Operand),
            ("Expr.le", "other", Role::Operand),
            ("Expr.log", "base", Role::Base),
            ("Expr.lt", "other", Role::Operand),
            ("Expr.mod", "other", Role::Operand),
            ("Expr.mul", "other", Role::Operand),
            ("Expr.ne", "other", Role::Operand),
            ("Expr.ne_missing", "other", Role::Operand),
            ("Expr.or_", "others", Role::Operand),
            ("Expr.pow", "exponent", Role::Exponent),
            ("Expr.replace", "new", Role::Bound),
            ("Expr.replace", "old", Role::Bound),
            ("Expr.replace_strict", "old", Role::Bound),
            ("Expr.shift", "fill_value", Role::Filler),
            ("Expr.shift", "n", Role::Count),
            ("Expr.sub", "other", Role::Operand),
            ("Expr.truediv", "other", Role::Divisor),
            ("Expr.xor", "other", Role::Operand),
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

#[test]
fn a_parameter_that_is_needed_without_being_required_is_one_of_which_a_step_gives_at_least_one() {
    // A step always gives at least one of a starred parameter that Python takes none of, as
    // `and_` and `or_`, and of the two bounds of `clip`, which are the expression they follow
    // without one. `fill_null` is refused without a value or a strategy, which is given in place
    // of it, and a replacement without what it replaces by: Python's `new` is `NO_DEFAULT` only
    // for a mapping, which the catalogue does not hold. The mark means nothing on any other
    // parameter.
    let mut marked = Vec::new();
    for entry in catalog::entries() {
        for parameter in entry.parameters() {
            let required = parameter.default() == catalog::ParameterDefault::Required;
            if parameter.is_needed() && !required {
                marked.push((entry.id().as_str(), parameter.name()));
            }
        }
    }
    assert_eq!(
        marked,
        [
            ("Expr.and_", "others"),
            ("Expr.clip", "lower_bound"),
            ("Expr.clip", "upper_bound"),
            ("Expr.fill_null", "value"),
            ("Expr.or_", "others"),
            ("Expr.replace", "new"),
            ("Expr.replace_strict", "new")
        ]
    );
}
