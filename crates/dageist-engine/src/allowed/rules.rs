//! The rules Polars does not hold, which the tables name.
//!
//! The tables record what Polars accepts, by running it. Some mistakes are not
//! found by running Polars on a few rows, or not by the table of the place
//! alone, so dageist holds them as named rules beside the catalogue. A line of
//! the context table says which rules govern it. [`offer`](super::offer)
//! applies a rule that forbids an entry before it reads the tables, so that the
//! rule's words explain the refusal, and the rules of how a chain ends to what
//! the tables say the chain gives. An end rule never stops an entry from
//! standing, since more links may follow it; it only says whether the chain
//! may end there.

use std::fmt;

use super::context::{Context, Nested, Outer};
use super::dtype::{Representative, ResultDtype};
use super::table::Shape;

/// A rule that holds wherever it applies, whatever Polars does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    /// A pivot's aggregate gives one value for each group.
    PivotAggregateGivesOneValue,
    /// The predicate of a condition is a `Boolean`.
    ConditionPredicateIsBoolean,
    /// The predicate of a `filter` is a `Boolean`, and so is what a condition
    /// in it gives.
    FilterPredicateIsBoolean,
    /// `polars.element` stands only in a pivot's aggregate.
    ElementOnlyInPivotAggregate,
    /// A join pairs no two output column names twice: the keys of each side
    /// are named by the column they read, and a key that repeats the name of
    /// an earlier one, beside a key on the other side that repeats the name it
    /// was paired with, makes the same pair again.
    JoinKeyNamesAreNotRepeated,
    /// A join's own key is elementwise: it holds no aggregate beside a column,
    /// no column compared with a null, and no replacement of values that are not literals.
    KeyIsElementwise,
    /// A pattern is a regular expression Polars can compile, unless the text is plain text by a
    /// flag such as `literal`.
    PatternIsValid,
    /// A replacement is given as many new values as old ones, or one, a strict one a
    /// default of as many values as what it follows, or one, and the replacement of text a value
    /// of as many as the text it follows, or one.
    ReplacementValuesMatchInNumber,
}

impl Rule {
    /// Every rule, in the order the tables list them.
    pub const ALL: [Self; 8] = [
        Self::ConditionPredicateIsBoolean,
        Self::ElementOnlyInPivotAggregate,
        Self::FilterPredicateIsBoolean,
        Self::JoinKeyNamesAreNotRepeated,
        Self::KeyIsElementwise,
        Self::PatternIsValid,
        Self::PivotAggregateGivesOneValue,
        Self::ReplacementValuesMatchInNumber,
    ];

    /// The name the tables give the rule.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PivotAggregateGivesOneValue => "pivot-aggregate-gives-one-value",
            Self::ConditionPredicateIsBoolean => "condition-predicate-is-boolean",
            Self::ElementOnlyInPivotAggregate => "element-only-in-pivot-aggregate",
            Self::FilterPredicateIsBoolean => "filter-predicate-is-boolean",
            Self::JoinKeyNamesAreNotRepeated => "join-key-names-are-not-repeated",
            Self::KeyIsElementwise => "key-is-elementwise",
            Self::PatternIsValid => "pattern-is-valid",
            Self::ReplacementValuesMatchInNumber => "replacement-values-match-in-number",
        }
    }

    /// The rule named `name` in a table.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|rule| rule.name() == name)
    }

    /// What the rule says, as the reason a step that breaks it is refused.
    #[must_use]
    pub const fn statement(self) -> &'static str {
        match self {
            Self::PivotAggregateGivesOneValue => {
                "a pivot's aggregate must give one value for each group"
            }
            Self::ConditionPredicateIsBoolean => "a condition's predicate must be Boolean",
            Self::ElementOnlyInPivotAggregate => {
                "`polars.element` stands only in a pivot's aggregate"
            }
            Self::FilterPredicateIsBoolean => "a filter's predicate must be Boolean",
            Self::JoinKeyNamesAreNotRepeated => {
                "a join may not pair the same two output column names twice: where two keys on one side are named alike, the keys they are paired with must be named differently"
            }
            Self::KeyIsElementwise => {
                "a join's key must be elementwise: an aggregate may not stand in it beside a column, nor a column be compared with a null, nor a replacement take a value that reads a column"
            }
            Self::PatternIsValid => "a pattern must be a valid regular expression",
            Self::ReplacementValuesMatchInNumber => {
                "a replacement takes as many new values as old ones, or one, a strict one a default of as many values as what it follows, or one, and the replacement of text a value of as many as the text it follows, or one"
            }
        }
    }

    /// Whether the rule governs the entry `entry` standing in `context`, as a
    /// start or a link: it is the entry the rule is about, or the entry is the
    /// last of a chain in the context the rule is about.
    #[cfg(test)]
    #[must_use]
    pub(super) fn governs(self, entry: &str, context: Context) -> bool {
        match self {
            Self::ElementOnlyInPivotAggregate => entry == "polars.element",
            Self::PivotAggregateGivesOneValue => context == Context::step(Outer::PivotAggregate),
            Self::JoinKeyNamesAreNotRepeated | Self::KeyIsElementwise => context.is_join_key(),
            // A matter of the values a call is given, whatever context it stands in.
            Self::PatternIsValid | Self::ReplacementValuesMatchInNumber => false,
            Self::ConditionPredicateIsBoolean | Self::FilterPredicateIsBoolean => {
                self.needs_boolean_end(context)
            }
        }
    }

    /// The rules that govern `entry` standing in `context`.
    #[cfg(test)]
    #[must_use]
    pub(super) fn governing(entry: &str, context: Context) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|rule| rule.governs(entry, context))
            .collect()
    }

    /// The rule that forbids `entry` to start or continue a chain in
    /// `context`, if one does.
    #[must_use]
    pub(super) fn forbids(entry: &str, context: Context) -> Option<Self> {
        (entry == "polars.element" && context.outer() != Outer::PivotAggregate)
            .then_some(Self::ElementOnlyInPivotAggregate)
    }

    /// The rule that forbids a key that holds an aggregate beside a column, or a
    /// column compared with a null, where `context` is a join's own key, if it
    /// does. Whether the key holds one is a
    /// matter of the whole expression, which the caller found.
    #[must_use]
    pub(super) fn forbids_non_elementwise_key(context: Context) -> Option<Self> {
        context.is_join_key().then_some(Self::KeyIsElementwise)
    }

    /// The rule that forbids a join's keys to repeat a pair of output names,
    /// where `context` is a join's own key, if it does. Whether the keys of a
    /// side repeat a name, and the keys they are paired with, is a matter of the
    /// whole join, which the caller found.
    #[must_use]
    pub(super) fn forbids_repeated_key_names(context: Context) -> Option<Self> {
        context
            .is_join_key()
            .then_some(Self::JoinKeyNamesAreNotRepeated)
    }

    /// The rule that forbids a chain to end in `context` with a value of the
    /// dtype `result` and the shape `shape`, if one does.
    ///
    /// `result` is `None` where the dtype is not known: a start takes the
    /// dtype of a column, and a method that takes an argument gives a dtype
    /// the argument decides. An end rule that is about a dtype does not refuse
    /// what it does not know; the dtype is known to break it, or the rule is
    /// silent.
    #[must_use]
    pub(super) fn forbids_end(
        context: Context,
        result: Option<&ResultDtype>,
        shape: Shape,
    ) -> Option<Self> {
        let known_not_boolean =
            result.is_some_and(|result| result.class() != Some(Representative::Boolean));
        if context == Context::step(Outer::PivotAggregate) && shape != Shape::One {
            return Some(Self::PivotAggregateGivesOneValue);
        }
        if known_not_boolean {
            Self::ALL
                .into_iter()
                .find(|rule| rule.needs_boolean_end(context))
        } else {
            None
        }
    }

    /// Whether some rule says a chain must end `Boolean` in `context`.
    pub(super) fn boolean_needed(context: Context) -> bool {
        Self::ALL
            .into_iter()
            .any(|rule| rule.needs_boolean_end(context))
    }

    /// Whether the rule says a chain must end `Boolean` in `context`. At most
    /// one rule does, for no two of them govern the same context.
    pub(super) fn needs_boolean_end(self, context: Context) -> bool {
        match self {
            Self::ConditionPredicateIsBoolean => {
                context.nested() == Some(Nested::ConditionPredicate)
            }
            Self::FilterPredicateIsBoolean => {
                context.outer() == Outer::Filter
                    && matches!(context.nested(), None | Some(Nested::ConditionValue))
            }
            Self::PivotAggregateGivesOneValue
            | Self::ElementOnlyInPivotAggregate
            | Self::JoinKeyNamesAreNotRepeated
            | Self::KeyIsElementwise
            | Self::PatternIsValid
            | Self::ReplacementValuesMatchInNumber => false,
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}
