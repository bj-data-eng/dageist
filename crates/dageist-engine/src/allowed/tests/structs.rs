//! What the tables and the rules say of a struct whose fields the chain does not know, and of a
//! struct made of the columns of the other table, against Polars' own refusals.

use polars::prelude::{DataType, Field};

use crate::allowed::{
    After, Chosen, Context, Ending, Fields, Input, Outer, Place, Representative, Shape, Verdict,
    offer,
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
