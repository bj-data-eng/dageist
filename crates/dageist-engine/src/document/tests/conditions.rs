//! The reader knows the order of a condition's links without building them, by
//! a table of the links' identifiers. The catalogue's build functions are the
//! authority on that order: they refuse a link that cannot follow the state
//! they are given. This holds the table to them.

use polars::prelude::{lit, when};

use crate::catalog::{Argument, Arguments, Build, Conditional, entries};
use crate::document::model::Stage;

const STATES: [Stage; 4] = [
    Stage::When,
    Stage::Then,
    Stage::ChainedWhen,
    Stage::ChainedThen,
];

/// A condition in `stage`, made with Polars.
fn condition_in(stage: Stage) -> Conditional {
    let begun = when(lit(true));
    let then = begun.clone().then(lit(1));
    let chained = then.clone().when(lit(true));
    match stage {
        Stage::When => Conditional::When(begun),
        Stage::Then => Conditional::Then(then),
        Stage::ChainedWhen => Conditional::ChainedWhen(chained),
        Stage::ChainedThen => Conditional::ChainedThen(chained.then(lit(2))),
        Stage::Value => unreachable!("an expression is not a condition"),
    }
}

fn stage_of(condition: &Conditional) -> Stage {
    match condition {
        Conditional::When(_) => Stage::When,
        Conditional::Then(_) => Stage::Then,
        Conditional::ChainedWhen(_) => Stage::ChainedWhen,
        Conditional::ChainedThen(_) => Stage::ChainedThen,
    }
}

/// Complete arguments for a link: a value for a `then` and an `otherwise`, a
/// condition for a `when`.
fn arguments_for(entry: &crate::catalog::Entry) -> Arguments {
    let argument = |name: &str| match name {
        "statement" => Argument::Expression(lit(1)),
        _ => Argument::Many(vec![Argument::Expression(lit(true))]),
    };
    entry
        .arguments(
            entry
                .parameters()
                .iter()
                .filter(|parameter| matches!(parameter.name(), "statement" | "predicates"))
                .map(|parameter| (parameter.name(), argument(parameter.name()))),
        )
        .expect("arguments for the link")
}

#[test]
fn the_table_of_links_matches_what_the_catalogue_builds() {
    let mut links = 0;
    for entry in entries() {
        let (Build::ConditionStep(_) | Build::ConditionEnd(_)) = entry.build() else {
            continue;
        };
        links += 1;
        let id = entry.id().as_str();
        let (needs, leaves) =
            Stage::of_link(id).unwrap_or_else(|| panic!("{id} is not in the table"));
        let arguments = arguments_for(entry);
        for state in STATES {
            let built = match entry.build() {
                Build::ConditionStep(build) => {
                    build(condition_in(state), &arguments).map(|next| stage_of(&next))
                }
                Build::ConditionEnd(build) => {
                    build(condition_in(state), &arguments).map(|_| Stage::Value)
                }
                _ => unreachable!("only links of a condition are looked at"),
            };
            if state == needs {
                let left =
                    built.unwrap_or_else(|error| panic!("{id} after {}: {error}", state.name()));
                assert!(
                    left == leaves,
                    "{id} leaves {}, not {}",
                    left.name(),
                    leaves.name()
                );
            } else {
                assert!(built.is_err(), "{id} follows {} too", state.name());
            }
        }
    }
    assert_eq!(links, 6, "the catalogue's links of a condition");
}
