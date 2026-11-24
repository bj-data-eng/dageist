//! The parameters that meet the value their method follows in a way of their own,
//! as Polars' documentation describes the methods: the other side of arithmetic
//! and of a comparison, the value `shift` fills with, the rows it counts and the characters a
//! method of text counts, and the share a quantile reads.

use crate::catalog::{self, Role};

#[test]
fn only_the_operands_the_fill_the_row_count_the_power_the_divisors_and_the_share_have_a_role() {
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
            ("Expr.approx_quantile", "quantile", Role::Share),
            ("Expr.bottom_k", "k", Role::Count),
            ("Expr.clip", "lower_bound", Role::Bound),
            ("Expr.clip", "upper_bound", Role::Bound),
            ("Expr.diff", "n", Role::Count),
            ("Expr.eq", "other", Role::Operand),
            ("Expr.eq_missing", "other", Role::Operand),
            ("Expr.fill_nan", "value", Role::Filler),
            ("Expr.fill_null", "value", Role::Filler),
            ("Expr.floordiv", "other", Role::FloorDivisor),
            ("Expr.ge", "other", Role::Operand),
            ("Expr.gt", "other", Role::Operand),
            ("Expr.head", "n", Role::Count),
            ("Expr.is_between", "lower_bound", Role::Operand),
            ("Expr.is_between", "upper_bound", Role::Operand),
            ("Expr.is_close", "other", Role::Operand),
            ("Expr.le", "other", Role::Operand),
            ("Expr.limit", "n", Role::Count),
            ("Expr.log", "base", Role::Base),
            ("Expr.lt", "other", Role::Operand),
            ("Expr.mod", "other", Role::Operand),
            ("Expr.mul", "other", Role::Operand),
            ("Expr.ne", "other", Role::Operand),
            ("Expr.ne_missing", "other", Role::Operand),
            ("Expr.or_", "others", Role::Operand),
            ("Expr.pct_change", "n", Role::Count),
            ("Expr.pow", "exponent", Role::Exponent),
            ("Expr.quantile", "quantile", Role::Share),
            ("Expr.replace", "new", Role::Bound),
            ("Expr.replace", "old", Role::Bound),
            ("Expr.replace_strict", "old", Role::Bound),
            ("Expr.shift", "fill_value", Role::Filler),
            ("Expr.shift", "n", Role::Count),
            ("Expr.slice", "length", Role::Count),
            ("Expr.slice", "offset", Role::Count),
            ("Expr.str.head", "n", Role::Count),
            ("Expr.str.pad_end", "length", Role::Length),
            ("Expr.str.pad_start", "length", Role::Length),
            ("Expr.str.slice", "length", Role::Length),
            ("Expr.str.slice", "offset", Role::Count),
            ("Expr.str.tail", "n", Role::Count),
            ("Expr.str.zfill", "length", Role::Length),
            ("Expr.sub", "other", Role::Operand),
            ("Expr.tail", "n", Role::Count),
            ("Expr.top_k", "k", Role::Count),
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
            ("Expr.sort_by", "descending"),
            ("Expr.sort_by", "nulls_last"),
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
    // of it, a replacement without what it replaces by: Python's `new` is `NO_DEFAULT` only
    // for a mapping, which the catalogue does not hold, and the `filter` of an expression, which
    // Python refuses without a predicate or a constraint. The mark means nothing on any other
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
            ("Expr.filter", "predicates"),
            ("Expr.or_", "others"),
            ("Expr.replace", "new"),
            ("Expr.replace_strict", "new")
        ]
    );
}

#[test]
fn the_share_of_a_quantile_takes_numbers_only() {
    // Polars reads a text or a moment as the share it writes in a select and refuses it where a
    // window or a pivot's aggregate evaluates the values of a group, so the editor offers numbers.
    for id in ["Expr.quantile", "Expr.approx_quantile"] {
        let entry = catalog::entry(id).expect("an entry");
        let share = entry
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == "quantile")
            .expect("the share");
        assert_eq!(
            share.kind(),
            catalog::Kind::Expression(&[catalog::Dtypes::Numeric]),
            "{id}"
        );
    }
}

#[test]
fn only_the_parameters_polars_reads_as_one_value_say_how_they_must_be_given_it() {
    // Python Polars 2.1.0, in a select: `shift`'s count and fill and `diff`'s count refuse the rows, a
    // count of values and the one value that `head` took of one value, with "must be a scalar value";
    // a percentage change, a quantile and `top_k` and `bottom_k` refuse the rows and a count and take
    // that value. A count of values that `head` or `slice` take, and an approximate quantile's
    // share, take any of them.
    let mut found: Vec<(&str, &str, catalog::OneValue)> = catalog::entries()
        .iter()
        .flat_map(|entry| {
            entry.parameters().iter().filter_map(move |parameter| {
                parameter
                    .one_value()
                    .map(|how| (entry.id().as_str(), parameter.name(), how))
            })
        })
        .collect();
    found.sort_by_key(|(entry, parameter, _)| (*entry, *parameter));
    assert_eq!(
        found,
        [
            ("Expr.bottom_k", "k", catalog::OneValue::SingleRow),
            ("Expr.diff", "n", catalog::OneValue::Scalar),
            ("Expr.pct_change", "n", catalog::OneValue::SingleRow),
            ("Expr.quantile", "quantile", catalog::OneValue::SingleRow),
            ("Expr.shift", "fill_value", catalog::OneValue::Scalar),
            ("Expr.shift", "n", catalog::OneValue::Scalar),
            ("Expr.top_k", "k", catalog::OneValue::SingleRow),
        ]
    );
}
