//! The lists of every variant of the closed vocabularies the host sends to the
//! view. The view's decoders are checked against them, so a variant missing
//! here is one the view would refuse.

use crate::allowed::{ConditionState, Shape, Stage};

/// Where a variant stands in its list. The match names every variant, so a new
/// one fails to compile here until it is placed.
const fn stage_position(stage: Stage) -> usize {
    match stage {
        Stage::Schema => 0,
        Stage::Rows => 1,
        Stage::Empty => 2,
    }
}

const fn shape_position(shape: Shape) -> usize {
    match shape {
        Shape::One => 0,
        Shape::Many => 1,
    }
}

const fn state_position(state: ConditionState) -> usize {
    match state {
        ConditionState::When => 0,
        ConditionState::Then => 1,
        ConditionState::ChainedWhen => 2,
        ConditionState::ChainedThen => 3,
    }
}

#[test]
fn each_list_has_every_variant_once_in_the_order_it_is_placed() {
    assert_eq!(Stage::ALL.len(), 3);
    for (at, stage) in Stage::ALL.into_iter().enumerate() {
        assert_eq!(stage_position(stage), at);
    }
    assert_eq!(Shape::ALL.len(), 2);
    for (at, shape) in Shape::ALL.into_iter().enumerate() {
        assert_eq!(shape_position(shape), at);
    }
    assert_eq!(ConditionState::ALL.len(), 4);
    for (at, state) in ConditionState::ALL.into_iter().enumerate() {
        assert_eq!(state_position(state), at);
    }
}

#[test]
fn the_stages_are_the_words_the_tables_use() {
    assert_eq!(Stage::ALL.map(Stage::name), ["schema", "rows", "empty"]);
}
