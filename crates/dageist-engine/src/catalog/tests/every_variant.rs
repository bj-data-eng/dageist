//! The lists of every variant of the catalogue's closed vocabularies the host
//! sends to the view. The view's decoders are checked against them, so a
//! variant missing from a list is one the view would refuse.
//!
//! Each enum has here a `next` that names every variant, so adding one does
//! not compile until `next` gives it a place, and a test walks the chain from
//! the first variant and expects it to be the list. A variant given a place
//! in `next` and left out of the list fails that test.

use crate::catalog::{Applies, ValueType};

/// The variants of a list reached from `first` by `next`, which ends the chain
/// with `None`.
fn chain<T: Copy>(first: T, next: impl Fn(T) -> Option<T>) -> Vec<T> {
    /// More than any list here has, so a chain that loops fails and does not hang.
    const MOST: usize = 64;
    let mut walked = vec![first];
    while let Some(following) = next(walked[walked.len() - 1]) {
        assert!(walked.len() < MOST, "the chain does not end");
        walked.push(following);
    }
    walked
}

const fn next_applies(applies: Applies) -> Option<Applies> {
    match applies {
        Applies::Expression => Some(Applies::Frame),
        Applies::Frame => Some(Applies::Function),
        Applies::Function => Some(Applies::Condition),
        Applies::Condition => None,
    }
}

const fn next_type(value_type: ValueType) -> Option<ValueType> {
    match value_type {
        ValueType::Integer => Some(ValueType::Float),
        ValueType::Float => Some(ValueType::String),
        ValueType::String => Some(ValueType::Date),
        ValueType::Date => Some(ValueType::Any),
        ValueType::Any => None,
    }
}

#[test]
fn the_lists_are_every_variant_in_order() {
    assert_eq!(chain(Applies::Expression, next_applies), Applies::ALL);
    assert_eq!(chain(ValueType::Integer, next_type), ValueType::ALL);
}
