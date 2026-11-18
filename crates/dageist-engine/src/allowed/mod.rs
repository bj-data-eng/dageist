//! Which combinations the catalogue's entries may stand in, found by running
//! Polars and kept as tables.
//!
//! Polars' schema resolution does not refuse every invalid combination:
//! `dt.date` of a `String` column has a schema, and fails only when rows are
//! read. So the tables are built by running each combination on a few rows of
//! the dtype, including a null, and on an empty table, and a combination is
//! allowed only when the schema and both runs succeed. There are three, each a
//! text artifact under `api/catalogue/`, one combination to a line, sorted:
//!
//! - the **dtype table**: each expression method on a receiver of each
//!   [representative dtype](Representative), and, for each expression
//!   parameter, with an argument of each of them;
//! - the **context table**: each entry in each [context](Context), a place
//!   that holds an expression, up to two levels deep, twice: with the chain
//!   going on from the entry, which says whether it may stand there, and with
//!   the chain ending at it, which says whether it may end an expression
//!   there;
//! - the **option table**: each entry with each pair of its options, the flags
//!   and the words of a choice.
//!
//! Some rules are not Polars' to hold, and are named in [`Rule`] and applied
//! beside the tables. [`offers`] and [`offer`] answer, from the committed
//! tables embedded in the engine and those rules, whether an entry may stand
//! at a [`Place`], and give a reason for each refusal, without running Polars.
//! [`rule_forbids`] answers from the rules alone, [`rule_forbids_a_key_known_not_elementwise`]
//! for a join's key whose whole expression holds an aggregate beside a column,
//! or a column compared with a null, and [`rule_forbids_a_repeated_pair_of_key_names`]
//! for a join's key that makes a pair of output names again.

mod adapt;
mod candidates;
mod context;
mod dtype;
mod offers;
mod rules;
mod table;
#[cfg(test)]
mod tests;

pub(crate) use adapt::{Loose, is_count, is_required, operand_adapts, receiver_adapts};
pub use candidates::{
    ArgumentSlot, Candidate, DefaultArgument, Fit, Literal, OptionOffer, OptionWord, Slot,
    default_argument, fit, option_offers,
};
pub use context::{Context, Nested, Outer, Position};
pub use dtype::{Representative, ResultDtype, dtype_name};
pub(crate) use offers::same_for_any_number;
pub use offers::{
    After, Allowance, Chosen, ConditionState, Ending, Offer, Place, Refusal, RefusalSource,
    Unknown, Verdict, argument_dtypes, offer, offer_input, offers, options, rule_forbids,
    rule_forbids_a_key_known_not_elementwise, rule_forbids_a_repeated_pair_of_key_names,
};
pub use rules::Rule;
pub use table::{Outcome, Produces, Reason, Shape, Stage};
