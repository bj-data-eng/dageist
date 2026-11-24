//! The walk counts a method as one value by a table of identifiers, for a join's key may hold an
//! aggregate and not beside a column. The tables are the authority on whether a method gives one
//! value: this holds the table of identifiers to them.

use crate::allowed::{After, Chosen, Context, Outer, Place, Representative, Shape, offer};
use crate::catalog;
use crate::document::walk::ONE_VALUE;

#[test]
fn every_method_the_walk_counts_as_one_value_gives_one_value_in_the_tables() {
    for id in ONE_VALUE {
        let entry = catalog::entry(id).expect("the walk names an entry of the catalogue");
        // A method of text takes text, and the others take a whole number.
        let gives = [Representative::Int64, Representative::String]
            .into_iter()
            .find_map(|dtype| {
                let place = Place::new(
                    Context::step(Outer::Select),
                    After::Value {
                        dtype,
                        shape: Shape::Many,
                    },
                );
                offer(entry, &place, &Chosen::default())
                    .allowance()
                    .map(|allowance| allowance.shape())
            });
        assert_eq!(gives, Some(Shape::One), "{id}");
    }
}

#[test]
fn a_method_that_gives_one_value_in_the_tables_is_one_the_walk_counts_so() {
    // The reverse: every aggregate of the catalogue that gives one value for any receiver is in the
    // walk's table, or a join's key would hold it beside a column. `first`, `last` and the others
    // that the catalogue has are the walk's; a method added without being counted is found here.
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == catalog::Applies::Expression)
        // The count of what `gather` and `search_sorted` give is their argument's, which is measured
        // with one value; the others that take some of the values give counts, which are not one.
        .filter(|entry| {
            !matches!(
                catalog::Counting::of(entry, &[]),
                Some(catalog::Counting::TheArgument(_))
            )
        })
    {
        let id = entry.id().as_str();
        let one_for_every_receiver = [Representative::Int64, Representative::String]
            .into_iter()
            .all(|dtype| {
                let place = Place::new(
                    Context::step(Outer::Select),
                    After::Value {
                        dtype,
                        shape: Shape::Many,
                    },
                );
                offer(entry, &place, &Chosen::default())
                    .allowance()
                    .is_some_and(|allowance| allowance.shape() == Shape::One)
            });
        if one_for_every_receiver {
            assert!(
                ONE_VALUE.contains(&id),
                "{id} gives one value, and the walk does not count it so"
            );
        }
    }
}
