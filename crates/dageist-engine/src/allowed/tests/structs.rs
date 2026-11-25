//! What the tables and the rules say of a struct whose fields the chain does not know, and of a
//! struct made of the columns of the other table, against Polars' own refusals.

use polars::prelude::{DataType, Field};

use crate::allowed::{
    After, Chosen, Context, Ending, Fields, Input, Nested, Outer, Place, Representative, Shape,
    Verdict, offer,
};
use crate::catalog;

fn entry(id: &str) -> &'static catalog::Entry {
    catalog::entry(id).unwrap_or_else(|| panic!("no entry {id}"))
}

fn after_a_struct() -> Place {
    Place::new(
        Context::step(Outer::Select),
        After::Value {
            dtype: Representative::Struct,
            shape: Shape::Many,
        },
    )
}

fn xy() -> Fields {
    Fields::of_dtype(&DataType::Struct(vec![
        Field::new("x".into(), DataType::Int64),
        Field::new("y".into(), DataType::String),
    ]))
    .expect("a struct")
}

fn allowed(verdict: &Verdict) -> &crate::allowed::Allowance {
    verdict
        .allowance()
        .unwrap_or_else(|| panic!("allowed: {verdict:?}"))
}

#[test]
fn a_call_that_reads_drops_or_renames_the_fields_of_a_struct_whose_fields_are_not_known_cannot_say_whether_it_ends_there()
 {
    // What Polars says of a name depends on the fields, which a pivot's values hide from the chain.
    for (id, names) in [
        ("Expr.struct.field", vec![("name", "zz")]),
        ("Expr.struct.drop", vec![("names", "zz")]),
        ("Expr.struct.rename_fields", vec![("names", "p")]),
    ] {
        let verdict = offer(
            entry(id),
            &after_a_struct(),
            &Chosen::default().with_names(&names),
        );
        assert!(
            matches!(allowed(&verdict).can_end(), Ending::NotKnown(_)),
            "{id}: {verdict:?}"
        );
    }
}

#[test]
fn the_same_calls_on_a_struct_whose_fields_are_known_are_decided() {
    let fields = xy();
    let drop = offer(
        entry("Expr.struct.drop"),
        &after_a_struct(),
        &Chosen::default()
            .following_fields(Some(&fields))
            .with_names(&[("names", "x")]),
    );
    assert!(
        matches!(allowed(&drop).can_end(), Ending::Allowed),
        "{drop:?}"
    );
    let read = offer(
        entry("Expr.struct.field"),
        &after_a_struct(),
        &Chosen::default()
            .following_fields(Some(&fields))
            .with_names(&[("name", "y")]),
    );
    assert!(
        matches!(allowed(&read).can_end(), Ending::Allowed),
        "{read:?}"
    );
    let none = offer(
        entry("Expr.struct.field"),
        &after_a_struct(),
        &Chosen::default()
            .following_fields(Some(&fields))
            .with_names(&[("name", "zz")]),
    );
    assert!(none.refusal().is_some(), "{none:?}");
}

#[test]
fn a_struct_that_keeps_its_unknown_fields_is_not_made_undecided_by_a_call_that_names_none() {
    // `is_null` and `value_counts` read no name, so what they say does not wait for the fields.
    for id in ["Expr.is_null", "Expr.value_counts"] {
        let verdict = offer(entry(id), &after_a_struct(), &Chosen::default());
        assert!(
            matches!(allowed(&verdict).can_end(), Ending::Allowed),
            "{id}: {verdict:?}"
        );
    }
}

#[test]
fn a_struct_made_of_the_other_tables_columns_may_stand_as_the_key_of_that_table() {
    // `polars.struct(ra)` is a struct of a field named `ra`, and the key it is matched with is the
    // dtype it has, as the key `a` is where the struct reads the left table's `a`.
    let inputs = [Input::new(Some("ra"), Some(Representative::Int64), None)];
    for outer in [Outer::JoinLeft, Outer::JoinRight] {
        let place = Place::new(Context::step(outer), After::Start);
        let verdict = offer(
            entry("polars.struct"),
            &place,
            &Chosen::default().with_inputs(&inputs).giving_many(),
        );
        let allowance = verdict
            .allowance()
            .unwrap_or_else(|| panic!("{outer:?}: {verdict:?}"));
        assert!(
            matches!(allowance.can_end(), Ending::Allowed),
            "{outer:?}: {:?}",
            allowance.can_end()
        );
    }
}

fn a_struct_of(fields: &[(&str, DataType)]) -> Fields {
    Fields::of_dtype(&DataType::Struct(
        fields
            .iter()
            .map(|(name, dtype)| Field::new((*name).into(), dtype.clone()))
            .collect(),
    ))
    .expect("a struct")
}

#[test]
fn a_field_read_of_a_boolean_may_end_a_filters_predicate_and_one_of_a_number_may_not() {
    // The tables measured a read of a number, which a predicate refuses; what is read is the field.
    let fields = a_struct_of(&[("s", DataType::Boolean), ("n", DataType::Int64)]);
    let place = Place::new(
        Context::step(Outer::Filter),
        After::Value {
            dtype: Representative::Struct,
            shape: Shape::Many,
        },
    );
    let read = |name: &str| {
        let names = [("name", name)];
        offer(
            entry("Expr.struct.field"),
            &place,
            &Chosen::default()
                .following_fields(Some(&fields))
                .with_names(&names),
        )
    };
    let boolean = read("s");
    assert!(
        matches!(allowed(&boolean).can_end(), Ending::Allowed),
        "{boolean:?}"
    );
    let number = read("n");
    assert!(
        matches!(allowed(&number).can_end(), Ending::Refused(_)),
        "{number:?}"
    );
}

#[test]
fn a_field_read_of_a_boolean_may_end_a_conditions_predicate() {
    let fields = a_struct_of(&[("s", DataType::Boolean)]);
    let place = Place::new(
        Context::inside(Outer::Select, Nested::ConditionPredicate),
        After::Value {
            dtype: Representative::Struct,
            shape: Shape::Many,
        },
    );
    let verdict = offer(
        entry("Expr.struct.field"),
        &place,
        &Chosen::default()
            .following_fields(Some(&fields))
            .with_names(&[("name", "s")]),
    );
    assert!(
        matches!(allowed(&verdict).can_end(), Ending::Allowed),
        "{verdict:?}"
    );
}

#[test]
fn a_count_given_a_name_where_the_name_of_the_values_is_not_known_cannot_say_whether_it_clashes() {
    // In a pivot's group the values are named by the column the pivot takes them from, which the
    // chain does not know, and a count named as they are is refused by Polars.
    let named = offer(
        entry("Expr.value_counts"),
        &after_a_struct(),
        &Chosen::default().with_names(&[("name", "a")]),
    );
    assert!(
        matches!(allowed(&named).can_end(), Ending::NotKnown(_)),
        "{named:?}"
    );
    // Told the name of the values, it is decided: a clash is refused and another name is not.
    let clash = offer(
        entry("Expr.value_counts"),
        &after_a_struct(),
        &Chosen::default()
            .named(Some("a"))
            .with_names(&[("name", "a")]),
    );
    assert!(clash.refusal().is_some(), "{clash:?}");
    let other = offer(
        entry("Expr.value_counts"),
        &after_a_struct(),
        &Chosen::default()
            .named(Some("a"))
            .with_names(&[("name", "n")]),
    );
    assert!(
        matches!(allowed(&other).can_end(), Ending::Allowed),
        "{other:?}"
    );
}

#[test]
fn names_given_to_an_array_of_no_known_width_are_not_judged_and_fields_are_not_made_of_them() {
    // Polars counts the names against the width of the array, which the chain may not know.
    let place = Place::new(
        Context::step(Outer::Select),
        After::Value {
            dtype: Representative::Array(crate::allowed::Inner::Int64),
            shape: Shape::Many,
        },
    );
    let verdict = offer(
        entry("Expr.arr.to_struct"),
        &place,
        &Chosen::default().with_names(&[("fields", "a")]),
    );
    let allowance = allowed(&verdict);
    assert!(
        matches!(allowance.can_end(), Ending::NotKnown(_)),
        "{verdict:?}"
    );
    assert!(allowance.fields().is_none(), "{verdict:?}");
    // Where the width is known and is the number of the names, the fields are the names.
    let known = offer(
        entry("Expr.arr.to_struct"),
        &place,
        &Chosen::default()
            .with_names(&[("fields", "a")])
            .following_width(Some(1)),
    );
    assert_eq!(
        allowed(&known).fields().map(|fields| fields.len()),
        Some(1),
        "{known:?}"
    );
}
