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
//!
//! [`read_typed`] answers what a text typed into a slot is: a number, a date, a
//! duration or text, as the engine reads it, with the dtype Polars gives it.

mod adapt;
mod candidates;
mod context;
mod count;
mod dtype;
mod literal;
mod offers;
mod rules;
mod table;
#[cfg(test)]
mod tests;
mod typed;

pub(crate) use adapt::{
    Loose, Receiver, adapted_dtype, adapts_numbers_only, broadcasts, could_be_taken_by_a_number,
    is_count, is_left_unresolved, is_refused_as_a_count, is_required, keeps_its_dtype,
    operand_adapts, receiver_beside,
};
pub use candidates::{
    ArgumentSlot, Candidate, DefaultArgument, Fit, OptionOffer, OptionWord, Slot, default_argument,
    fit, left_out_at_start, option_offers, slots_at_start,
};
pub use context::{Context, Nested, Outer, Position};
pub use count::Count;
pub use dtype::{Inner, Representative, ResultDtype, dtype_name};
pub use literal::Literal;
pub use offers::{
    After, Allowance, Chosen, ConditionState, Ending, Offer, Place, Refusal, RefusalSource,
    Unknown, Verdict, argument_dtypes, offer, offer_input, offers, options,
    refuse_a_call_the_tables_refuse_after_lists, refuse_a_count_that_may_not_end,
    refuse_a_receiver_not_judged, refuse_an_argument_not_judged, rule_forbids,
    rule_forbids_a_folded_function_in_a_join_key, rule_forbids_a_key_known_not_elementwise,
    rule_forbids_a_parameter_given_more_than_one_value, rule_forbids_a_range_that_cannot_be_made,
    rule_forbids_a_receiver, rule_forbids_a_repeated_pair_of_key_names,
    rule_forbids_a_window_over_a_count, rule_forbids_a_window_over_a_widened_value,
    rule_forbids_arrays_of_other_widths, rule_forbids_replacement_values_of_other_numbers,
    rule_forbids_values_of_other_counts,
};
pub(crate) use offers::{refusal_of_a_connected_list, same_for_any_number};
pub use rules::Rule;
pub use table::{Outcome, Produces, Reason, Shape, Stage};
pub use typed::{LARGEST_WHOLE_NUMBER, Read, Typed, Unread, read_typed};
