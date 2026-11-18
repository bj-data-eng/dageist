//! Candidates for a slot against Polars' documented behavior: the dtype a typed
//! literal has is the one Polars gives it, and a candidate fits where the
//! tables allow its dtype.

use polars::prelude::{DataType, IntoLazy, lit};

use crate::allowed::offers::RefusalSource;
use crate::allowed::{
    After, ArgumentSlot, Candidate, Context, Literal, Outer, Place, Representative, Rule, Shape,
    Slot, default_argument, fit, option_offers,
};
use crate::catalog;

fn entry(id: &str) -> &'static catalog::Entry {
    catalog::entry(id).unwrap_or_else(|| panic!("no entry {id}"))
}

fn after(dtype: Representative) -> After {
    After::Value {
        dtype,
        shape: Shape::Many,
    }
}

fn select() -> Context {
    Context::step(Outer::Select)
}

/// The slot of the argument `parameter` of `id` after a value of `receiver`, in a select.
fn argument(id: &str, parameter: &'static str, receiver: Representative) -> Slot<'static> {
    let place = Place::new(select(), after(receiver));
    Slot::new(None, Some(ArgumentSlot::new(entry(id), parameter, place))).expect("a slot")
}

/// The dtype Polars gives `expression` selected from a table with no rows.
fn polars_dtype(expression: polars::prelude::Expr) -> DataType {
    let schema = polars::prelude::DataFrame::empty()
        .lazy()
        .select([expression])
        .collect_schema()
        .expect("Polars resolves a literal");
    schema.iter().next().expect("one column").1.clone()
}

#[test]
fn a_typed_literal_has_the_dtype_polars_gives_it() {
    // A whole number is the narrowest integer it fits, a number with a
    // fraction a float, and text a String.
    let cases = [
        (Literal::Integer(1), lit(1_i64)),
        (Literal::Integer(3_000_000_000), lit(3_000_000_000_i64)),
        (Literal::Float(1.5), lit(1.5_f64)),
        (Literal::String("x".to_owned()), lit("x")),
    ];
    for (literal, expression) in cases {
        let expected = Representative::of(&polars_dtype(expression)).expect("a class");
        assert_eq!(literal.class(), Some(expected), "{literal:?}");
    }
    // By hand, from what Polars documents of a number typed without a dtype.
    assert_eq!(Literal::Integer(1).class(), Some(Representative::Int32));
    assert_eq!(
        Literal::Integer(3_000_000_000).class(),
        Some(Representative::Int64)
    );
    assert_eq!(Literal::Float(1.5).class(), Some(Representative::Float64));
}

#[test]
fn a_typed_literal_fits_where_the_dtype_polars_gives_it_does() {
    // `dt.offset_by` takes a duration written as text: a number is no duration.
    let by = argument("Expr.dt.offset_by", "by", Representative::Datetime);
    let text = fit(&Candidate::Literal(Literal::String("1d".to_owned())), &by).expect("a class");
    assert_eq!(text.dtype(), Representative::String);
    assert!(text.verdict().is_allowed(), "{text:?}");
    for number in [Literal::Integer(1), Literal::Float(1.5)] {
        let found = fit(&Candidate::Literal(number.clone()), &by).expect("a class");
        let refusal = found.verdict().refusal().expect("a number is no duration");
        assert!(
            matches!(refusal.source(), RefusalSource::Polars(_)),
            "{number:?}: {refusal:?}"
        );
    }
    // `sub` takes a number after a float: 1.5 is a Float64, and text is no number.
    let other = argument("Expr.sub", "other", Representative::Float64);
    let fraction = fit(&Candidate::Literal(Literal::Float(1.5)), &other).expect("a class");
    assert_eq!(fraction.dtype(), Representative::Float64);
    assert!(fraction.verdict().is_allowed(), "{fraction:?}");
    assert!(
        !fit(&Candidate::Literal(Literal::String("x".to_owned())), &other)
            .expect("a class")
            .verdict()
            .is_allowed()
    );
}

#[test]
fn a_field_and_an_input_fit_by_the_class_of_their_dtype() {
    let by = argument("Expr.dt.offset_by", "by", Representative::Datetime);
    let input = |dtype| Candidate::Input {
        dtype,
        shape: Shape::One,
    };
    for (candidate, fits) in [
        (Candidate::Field(Representative::String), true),
        (Candidate::Field(Representative::Int64), false),
        (input(Representative::String), true),
        (input(Representative::Int8), false),
    ] {
        let found = fit(&candidate, &by).expect("a class");
        assert_eq!(found.verdict().is_allowed(), fits, "{candidate:?}");
    }
}

#[test]
fn a_candidate_as_a_start_is_judged_by_the_context_it_stands_in() {
    let filter = Slot::new(Some(Context::step(Outer::Filter)), None).expect("a slot");
    // A field is offered as the start of a predicate, and may end it only if it is a Boolean.
    let number = fit(&Candidate::Field(Representative::Int64), &filter).expect("a class");
    let allowance = number.verdict().allowance().expect("it may stand");
    let refusal = allowance
        .can_end()
        .refusal()
        .expect("it may not end a predicate");
    assert_eq!(
        refusal.source(),
        RefusalSource::Rule(Rule::FilterPredicateIsBoolean)
    );
    let flag = fit(&Candidate::Field(Representative::Boolean), &filter).expect("a class");
    assert!(
        flag.verdict()
            .allowance()
            .expect("it may stand")
            .can_end()
            .is_allowed()
    );
    // A connected value is a literal of its dtype, and a list gives many values.
    let value = Candidate::Input {
        dtype: Representative::Boolean,
        shape: Shape::One,
    };
    let list = Candidate::Input {
        dtype: Representative::Boolean,
        shape: Shape::Many,
    };
    let shapes = [value, list].map(|candidate| {
        fit(&candidate, &filter)
            .expect("a class")
            .verdict()
            .allowance()
            .expect("it may stand")
            .shape()
    });
    assert_eq!(shapes, [Shape::One, Shape::Many]);
}

#[test]
fn a_candidate_that_stands_and_is_an_argument_must_fit_both() {
    // `over`'s `partition_by` is an argument of `over` and an expression in a
    // context of its own. A key may be a field of any dtype there.
    let place = Place::new(select(), after(Representative::Int64));
    let key = Slot::new(
        Some(Context::inside(
            Outer::Select,
            crate::allowed::Nested::PartitionBy,
        )),
        Some(ArgumentSlot::new(entry("Expr.over"), "partition_by", place)),
    )
    .expect("a slot");
    let found = fit(&Candidate::Field(Representative::String), &key).expect("a class");
    assert!(found.verdict().is_allowed(), "{found:?}");
}

#[test]
fn a_candidate_with_no_class_cannot_be_judged_and_no_slot_has_no_place() {
    let rows = argument("Expr.shift", "n", Representative::Int64);
    assert_eq!(
        fit(&Candidate::Literal(Literal::Float(f64::NAN)), &rows).map(|found| found.dtype()),
        Some(Representative::Float64)
    );
    assert!(Slot::new(None, None).is_none());
    assert_eq!(
        Representative::of_name("Datetime(ns)"),
        Some(Representative::Datetime)
    );
    assert_eq!(Representative::of_name("Categorical"), None);
}

#[test]
fn a_method_starts_with_polars_default_when_the_tables_allow_it() {
    let place = Place::new(select(), after(Representative::Int64));
    // `shift(n=1)`: one row, a whole number.
    let shift = default_argument(entry("Expr.shift"), &place).expect("a default");
    assert_eq!(shift.parameter(), "n");
    assert_eq!(shift.literal(), &Literal::Integer(1));
    // `is_null` takes no argument, and `dt.offset_by` has no default.
    assert_eq!(default_argument(entry("Expr.is_null"), &place), None);
    assert_eq!(default_argument(entry("Expr.dt.offset_by"), &place), None);
}

#[test]
fn the_options_of_an_entry_come_with_their_default_and_the_words_it_may_be_set_to() {
    let over = option_offers(entry("Expr.over"), &[]);
    let names: Vec<&str> = over.iter().map(|option| option.parameter()).collect();
    assert_eq!(names, ["descending", "nulls_last", "mapping_strategy"]);
    let flag = &over[0];
    assert_eq!(flag.default_word(), Some("false"));
    let words = |option: &crate::allowed::OptionOffer| -> Vec<String> {
        option
            .words()
            .iter()
            .map(|word| word.word().to_owned())
            .collect()
    };
    assert_eq!(words(flag), ["true", "false"]);
    let strategy = &over[2];
    assert_eq!(strategy.default_word(), Some("group_to_rows"));
    assert_eq!(words(strategy), ["group_to_rows", "join", "explode"]);
    // Each is allowed set alone.
    assert!(
        over.iter()
            .flat_map(|option| option.words())
            .all(|word| word.refusal().is_none())
    );
    // An entry with no flag or choice has none.
    assert!(option_offers(entry("Expr.is_null"), &[]).is_empty());
}

#[test]
fn a_word_is_refused_for_the_reason_polars_gives_beside_the_options_already_chosen() {
    // A join that takes no keys, a cross join, is refused with any validation; an anti join
    // is refused only the validations that count matches on both sides.
    let validate = |how: &str| {
        let offers = option_offers(entry("LazyFrame.join"), &[("how", how)]);
        let option = offers
            .into_iter()
            .find(|option| option.parameter() == "validate")
            .expect("validate");
        option
            .words()
            .iter()
            .map(|word| (word.word().to_owned(), word.refusal().map(|r| r.source())))
            .collect::<Vec<_>>()
    };
    let refused: Vec<String> = validate("anti")
        .into_iter()
        .filter(|(_, source)| source.is_some())
        .map(|(word, _)| word)
        .collect();
    let mut refused = refused;
    refused.sort();
    assert_eq!(refused, ["1:1", "1:m", "m:1"]);
    let cross = validate("cross");
    assert!(
        cross
            .iter()
            .all(|(_, source)| matches!(source, Some(RefusalSource::Polars(_))))
    );
    assert!(validate("inner").iter().all(|(_, source)| source.is_none()));
    // The choice being judged is not held to the word chosen for it.
    let how = option_offers(
        entry("LazyFrame.join"),
        &[("how", "cross"), ("validate", "m:m")],
    )
    .into_iter()
    .find(|option| option.parameter() == "how")
    .expect("how");
    assert!(
        how.words()
            .iter()
            .any(|word| word.word() == "inner" && word.refusal().is_none())
    );
}

#[test]
fn a_number_that_fits_the_receiver_gives_the_dtype_polars_gives_the_call() {
    // `u8 - 1` is a `UInt8` and `f32 - 1` a `Float32`: the number adapts to the column, where the
    // tables, measured with a column of its own dtype, would say `Int32` and `Float64`. A number
    // with a fraction beside whole numbers makes a `Float64`.
    let cases = [
        (
            Representative::UInt8,
            Literal::Integer(1),
            lit(1_i64),
            DataType::UInt8,
        ),
        (
            Representative::Float32,
            Literal::Integer(1),
            lit(1_i64),
            DataType::Float32,
        ),
        (
            Representative::Float32,
            Literal::Float(1.5),
            lit(1.5_f64),
            DataType::Float32,
        ),
        (
            Representative::Int8,
            Literal::Float(1.5),
            lit(1.5_f64),
            DataType::Float64,
        ),
    ];
    for (receiver, literal, number, polars) in cases {
        let slot = argument("Expr.sub", "other", receiver);
        let found = fit(&Candidate::Literal(literal.clone()), &slot).expect("a class");
        assert!(
            found.verdict().is_allowed(),
            "{literal:?} after {receiver}: {found:?}"
        );
        let said = found
            .verdict()
            .allowance()
            .and_then(|allowance| allowance.result())
            .and_then(|result| result.class());
        let column = polars_dtype_after(receiver, |column| column - number.clone());
        assert_eq!(column, polars, "what Polars gives");
        assert_eq!(
            said,
            Representative::of(&polars),
            "{literal:?} after {receiver}"
        );
    }
}

#[test]
fn a_comparison_with_a_number_gives_a_boolean_whether_or_not_the_number_fits() {
    // The result of a comparison does not depend on the dtype of its operand.
    for (receiver, literal) in [
        (Representative::UInt8, Literal::Integer(1)),
        (Representative::UInt8, Literal::Integer(300)),
        (Representative::UInt8, Literal::Float(1.5)),
        (Representative::Int64, Literal::Integer(1)),
    ] {
        let slot = argument("Expr.gt", "other", receiver);
        let found = fit(&Candidate::Literal(literal.clone()), &slot).expect("a class");
        let said = found
            .verdict()
            .allowance()
            .and_then(|allowance| allowance.result())
            .and_then(|result| result.class());
        assert_eq!(
            said,
            Some(Representative::Boolean),
            "{literal:?} after {receiver}"
        );
    }
}

#[test]
fn a_number_that_does_not_fit_the_receiver_leaves_what_arithmetic_gives_not_known() {
    // Polars widens the column: `u8 - 300` is a `UInt16`, which is not what the tables say of
    // either dtype the number could take.
    let slot = argument("Expr.sub", "other", Representative::UInt8);
    let found = fit(&Candidate::Literal(Literal::Integer(300)), &slot).expect("a class");
    assert!(found.verdict().is_allowed(), "{found:?}");
    let said = found
        .verdict()
        .allowance()
        .and_then(|allowance| allowance.result());
    assert!(said.is_none(), "{said:?}");
    assert_eq!(
        polars_dtype_after(Representative::UInt8, |column| column - lit(300_i64)),
        DataType::UInt16
    );
}

/// The dtype Polars gives `build` of a column of `receiver`.
fn polars_dtype_after(
    receiver: Representative,
    build: impl FnOnce(polars::prelude::Expr) -> polars::prelude::Expr,
) -> DataType {
    use polars::prelude::{DataFrame, Field, Schema, col};
    let schema = Schema::from_iter([Field::new("a".into(), receiver.dtype())]);
    let schema = DataFrame::empty_with_schema(&schema)
        .lazy()
        .select([build(col("a"))])
        .collect_schema()
        .expect("Polars resolves it");
    schema.iter().next().expect("one column").1.clone()
}

#[test]
fn a_candidate_that_stands_and_is_an_argument_has_the_verdict_of_the_argument_where_both_allow() {
    // `shift`'s `n` after a column of whole numbers is an argument, and in a select it is also an
    // expression that starts. A `1.0` typed there is allowed by both, and the call it is given to
    // gives one value for each row, where an expression that is a literal alone is one value.
    let place = Place::new(select(), after(Representative::Int64));
    let as_argument = ArgumentSlot::new(entry("Expr.shift"), "n", place);
    let candidate = Candidate::Literal(Literal::Float(1.0));
    let verdict_in = |slot: Slot<'_>| fit(&candidate, &slot).expect("a class").verdict().clone();
    let argument_only = verdict_in(Slot::new(None, Some(as_argument)).expect("a slot"));
    let start_only = verdict_in(Slot::new(Some(select()), None).expect("a slot"));
    let both = verdict_in(Slot::new(Some(select()), Some(as_argument)).expect("a slot"));
    assert_eq!(
        start_only.allowance().expect("it may stand").shape(),
        Shape::One
    );
    assert_eq!(
        argument_only.allowance().expect("it may stand").shape(),
        Shape::Many
    );
    assert_eq!(both, argument_only);
    assert_ne!(both, start_only);
}

#[test]
fn a_candidate_that_stands_and_is_an_argument_is_refused_where_either_refuses() {
    // Text is no number for `sub`, which refuses it as an argument; a start refuses what the
    // rule of a filter's predicate refuses to end, which is not a refusal to stand.
    let place = Place::new(select(), after(Representative::Float64));
    let as_argument = ArgumentSlot::new(entry("Expr.sub"), "other", place);
    let text = Candidate::Literal(Literal::String("x".to_owned()));
    let by_argument = Slot::new(Some(select()), Some(as_argument)).expect("a slot");
    let found = fit(&text, &by_argument).expect("a class");
    assert!(found.verdict().refusal().is_some(), "{found:?}");
    // A start that refuses: `element` stands only in a pivot's aggregate, and Polars refuses a
    // column in it; an input there is refused by the start, whatever the argument says.
    let pivot = Context::step(Outer::PivotAggregate);
    let by_start = Slot::new(Some(pivot), None).expect("a slot");
    let column = fit(&Candidate::Field(Representative::Int64), &by_start).expect("a class");
    assert!(column.verdict().refusal().is_some(), "{column:?}");
    let allowed_argument = ArgumentSlot::new(entry("Expr.shift"), "n", place);
    let both = Slot::new(Some(pivot), Some(allowed_argument)).expect("a slot");
    let refused = fit(&Candidate::Field(Representative::Int64), &both).expect("a class");
    assert!(refused.verdict().refusal().is_some(), "{refused:?}");
}

#[test]
fn a_candidate_that_stands_and_is_an_argument_may_end_as_the_expression_it_starts_may() {
    // `shift`'s `n` stands in a select, where the call it is given to may end a chain, and the
    // `1.0` given to it is also the start of the expression in a filter's predicate, which may
    // not end as a number. Whether the candidate's own chain may end is the start's to say, and
    // what the call gives is the argument's.
    let place = Place::new(select(), after(Representative::Int64));
    let as_argument = ArgumentSlot::new(entry("Expr.shift"), "n", place);
    let candidate = Candidate::Literal(Literal::Float(1.0));
    let allowance_in = |slot: Slot<'_>| {
        fit(&candidate, &slot)
            .expect("a class")
            .verdict()
            .allowance()
            .expect("it may stand")
            .clone()
    };
    let filter = Some(Context::step(Outer::Filter));
    let argument_only = allowance_in(Slot::new(None, Some(as_argument)).expect("a slot"));
    let start_only = allowance_in(Slot::new(filter, None).expect("a slot"));
    let both = allowance_in(Slot::new(filter, Some(as_argument)).expect("a slot"));
    // The call may end a chain, and the number may not end a predicate.
    assert!(argument_only.can_end().is_allowed());
    let refusal = start_only
        .can_end()
        .refusal()
        .expect("a number ends no predicate");
    assert_eq!(
        refusal.source(),
        RefusalSource::Rule(Rule::FilterPredicateIsBoolean)
    );
    // Both: what the call gives is the argument's, and whether the chain may end is the start's.
    assert_eq!(both.shape(), argument_only.shape());
    assert_eq!(both.result(), argument_only.result());
    assert_eq!(both.can_end(), start_only.can_end());
}
