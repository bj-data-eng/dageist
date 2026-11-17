//! The lists of every variant of the catalogue's closed vocabularies the host
//! sends to the view. The view's decoders are checked against them, so a
//! variant missing here is one the view would refuse.

use crate::catalog::{Applies, ValueType};

/// Where a variant stands in its list. The match names every variant, so a new
/// one fails to compile here until it is placed.
const fn applies_position(applies: Applies) -> usize {
    match applies {
        Applies::Expression => 0,
        Applies::Frame => 1,
        Applies::Function => 2,
        Applies::Condition => 3,
    }
}

const fn type_position(value_type: ValueType) -> usize {
    match value_type {
        ValueType::Integer => 0,
        ValueType::Float => 1,
        ValueType::String => 2,
        ValueType::Date => 3,
        ValueType::Any => 4,
    }
}

#[test]
fn each_list_has_every_variant_once_in_the_order_it_is_placed() {
    assert_eq!(Applies::ALL.len(), 4);
    for (at, applies) in Applies::ALL.into_iter().enumerate() {
        assert_eq!(applies_position(applies), at);
    }
    assert_eq!(ValueType::ALL.len(), 5);
    for (at, value_type) in ValueType::ALL.into_iter().enumerate() {
        assert_eq!(type_position(value_type), at);
    }
}
