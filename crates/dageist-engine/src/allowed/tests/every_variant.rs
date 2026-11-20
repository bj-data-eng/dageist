//! The lists of every variant of the closed vocabularies the host sends to the
//! view. The view's decoders are checked against them, so a variant missing
//! from a list is one the view would refuse.
//!
//! A list cannot be checked against the enum it lists, which Rust does not let
//! a program enumerate. Each enum has here a `next` that names every variant,
//! so adding one does not compile until `next` gives it a place, and a test
//! walks the chain from the first variant and expects it to be the list. A
//! variant given a place in `next` and left out of the list fails that test.
//! The walk reaches only the variants `next` links from the first one, so a
//! new variant is not caught when it is made the end of its own chain, or put
//! before the first variant the walk starts from; the match makes either a
//! deliberate act. Add a new variant inside or at the end of the chain.

use crate::allowed::{ConditionState, Context, Nested, Outer, Rule, Shape, Stage};

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

const fn next_outer(outer: Outer) -> Option<Outer> {
    match outer {
        Outer::Select => Some(Outer::Filter),
        Outer::Filter => Some(Outer::Sort),
        Outer::Sort => Some(Outer::JoinLeft),
        Outer::JoinLeft => Some(Outer::JoinRight),
        Outer::JoinRight => Some(Outer::PivotAggregate),
        Outer::PivotAggregate => None,
    }
}

const fn next_nested(nested: Nested) -> Option<Nested> {
    match nested {
        Nested::PartitionBy => Some(Nested::OrderBy),
        Nested::OrderBy => Some(Nested::ConditionPredicate),
        Nested::ConditionPredicate => Some(Nested::ConditionValue),
        Nested::ConditionValue => None,
    }
}

const fn next_rule(rule: Rule) -> Option<Rule> {
    match rule {
        Rule::ConditionPredicateIsBoolean => Some(Rule::ElementOnlyInPivotAggregate),
        Rule::ElementOnlyInPivotAggregate => Some(Rule::FilterPredicateIsBoolean),
        Rule::FilterPredicateIsBoolean => Some(Rule::JoinKeyNamesAreNotRepeated),
        Rule::JoinKeyNamesAreNotRepeated => Some(Rule::KeyIsElementwise),
        Rule::KeyIsElementwise => Some(Rule::PivotAggregateGivesOneValue),
        Rule::PivotAggregateGivesOneValue => Some(Rule::ReplacementValuesMatchInNumber),
        Rule::ReplacementValuesMatchInNumber => None,
    }
}

const fn next_stage(stage: Stage) -> Option<Stage> {
    match stage {
        Stage::Schema => Some(Stage::Rows),
        Stage::Rows => Some(Stage::Empty),
        Stage::Empty => None,
    }
}

const fn next_shape(shape: Shape) -> Option<Shape> {
    match shape {
        Shape::One => Some(Shape::Many),
        Shape::Many => None,
    }
}

const fn next_state(state: ConditionState) -> Option<ConditionState> {
    match state {
        ConditionState::When => Some(ConditionState::Then),
        ConditionState::Then => Some(ConditionState::ChainedWhen),
        ConditionState::ChainedWhen => Some(ConditionState::ChainedThen),
        ConditionState::ChainedThen => None,
    }
}

#[test]
fn the_position_lists_are_every_variant_in_order() {
    assert_eq!(chain(Outer::Select, next_outer), Outer::ALL);
    assert_eq!(chain(Nested::PartitionBy, next_nested), Nested::ALL);
    // The whole list of contexts is built from the two, so it holds each pair.
    assert_eq!(
        Context::all().len(),
        Outer::ALL.len() * (1 + Nested::ALL.len())
    );
}

#[test]
fn the_rule_list_is_every_variant_in_order() {
    assert_eq!(
        chain(Rule::ConditionPredicateIsBoolean, next_rule),
        Rule::ALL
    );
}

#[test]
fn the_stage_shape_and_state_lists_are_every_variant_in_order() {
    assert_eq!(chain(Stage::Schema, next_stage), Stage::ALL);
    assert_eq!(chain(Shape::One, next_shape), Shape::ALL);
    assert_eq!(chain(ConditionState::When, next_state), ConditionState::ALL);
}

#[test]
fn the_stages_are_the_words_the_tables_use() {
    assert_eq!(Stage::ALL.map(Stage::name), ["schema", "rows", "empty"]);
}
