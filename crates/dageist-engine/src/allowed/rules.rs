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
    /// A moment or a length of time made of numbers typed in is no part of a join's key: Polars
    /// makes a literal of the call and names it by an alias, which a key may not hold anywhere.
    FoldedFunctionIsNoJoinKey,
    /// A join pairs no two output column names twice: the keys of each side
    /// are named by the column they read, and a key that repeats the name of
    /// an earlier one, beside a key on the other side that repeats the name it
    /// was paired with, makes the same pair again.
    JoinKeyNamesAreNotRepeated,
    /// A duration typed in, where a method reads a text as one, is a duration string Polars reads.
    DurationIsValid,
    /// A join's own key is elementwise: it holds no aggregate beside a column,
    /// no column compared with a null, and no replacement of values that are not literals.
    KeyIsElementwise,
    /// A parameter that Polars reads as one value is given one: not the rows, not a count of values
    /// that the data or the arguments decide, and, for the places a shift and a difference look back
    /// by and the value a shift fills with, not one value that a method took of one value.
    ParameterTakesOneValue,
    /// A pattern is a regular expression Polars can compile, unless the text is plain text by a
    /// flag such as `literal`.
    PatternIsValid,
    /// A range or a repeat typed in is one Polars can make and cut: a step other than nothing, an
    /// interval of the length of time its bounds are read in, bounds the dtype holds, a count that is
    /// not below nothing and no length below nothing to cut it by.
    RangeIsValid,
    /// A replacement is given as many new values as old ones, or one, and a strict one a
    /// default of as many values as what it follows, or one.
    ReplacementValuesMatchInNumber,
    /// A time zone is one Polars reads: a name of the time zone database, or a fixed offset.
    TimeZoneIsValid,
    /// The operands of a comparison, a difference, a fill and the bounds of an interval, the values of
    /// a condition, and the new values of a replacement and its default, where they are moments, are in
    /// one time zone: Polars finds no dtype that two zones have in common.
    MomentsShareAZone,
    /// The values a call meets are one value, which meets each of the others, or as many as one
    /// another: a count that the data or the arguments decide is not the table's rows, and not
    /// another such count it is not known to equal.
    ValuesMeetInCount,
    /// A window is given a value as long as its group, which a count that the data or the
    /// arguments decide is not, nor the one value that a method took of one value, unless the window
    /// joins the values of a group or explodes them.
    WindowInputIsNotCounted,
    /// A window is given a value as long as its input, which a value of one row that a part widens is
    /// not.
    WindowInputIsNotWidened,
}

impl Rule {
    /// Every rule, in the order the tables list them.
    pub const ALL: [Self; 17] = [
        Self::ConditionPredicateIsBoolean,
        Self::DurationIsValid,
        Self::ElementOnlyInPivotAggregate,
        Self::FilterPredicateIsBoolean,
        Self::FoldedFunctionIsNoJoinKey,
        Self::JoinKeyNamesAreNotRepeated,
        Self::KeyIsElementwise,
        Self::MomentsShareAZone,
        Self::ParameterTakesOneValue,
        Self::PatternIsValid,
        Self::PivotAggregateGivesOneValue,
        Self::RangeIsValid,
        Self::ReplacementValuesMatchInNumber,
        Self::TimeZoneIsValid,
        Self::ValuesMeetInCount,
        Self::WindowInputIsNotCounted,
        Self::WindowInputIsNotWidened,
    ];

    /// The name the tables give the rule.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PivotAggregateGivesOneValue => "pivot-aggregate-gives-one-value",
            Self::ConditionPredicateIsBoolean => "condition-predicate-is-boolean",
            Self::DurationIsValid => "duration-is-valid",
            Self::ElementOnlyInPivotAggregate => "element-only-in-pivot-aggregate",
            Self::FilterPredicateIsBoolean => "filter-predicate-is-boolean",
            Self::FoldedFunctionIsNoJoinKey => "folded-function-is-no-join-key",
            Self::JoinKeyNamesAreNotRepeated => "join-key-names-are-not-repeated",
            Self::KeyIsElementwise => "key-is-elementwise",
            Self::MomentsShareAZone => "moments-share-a-zone",
            Self::ParameterTakesOneValue => "parameter-takes-one-value",
            Self::PatternIsValid => "pattern-is-valid",
            Self::RangeIsValid => "range-is-valid",
            Self::ReplacementValuesMatchInNumber => "replacement-values-match-in-number",
            Self::TimeZoneIsValid => "time-zone-is-valid",
            Self::ValuesMeetInCount => "values-meet-in-count",
            Self::WindowInputIsNotCounted => "window-input-is-not-counted",
            Self::WindowInputIsNotWidened => "window-input-is-not-widened",
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
            Self::FoldedFunctionIsNoJoinKey => {
                "a moment or a length of time made of numbers typed in is folded by Polars into a literal named by an alias, which a join's key may not hold: give it a column among its parts, or make it in a step before the join"
            }
            Self::JoinKeyNamesAreNotRepeated => {
                "a join may not pair the same two output column names twice: where two keys on one side are named alike, the keys they are paired with must be named differently"
            }
            Self::KeyIsElementwise => {
                "a join's key must be elementwise: an aggregate may not stand in it beside a column, nor a method that takes some of the values, as head and filter do, unless it goes on to an aggregate, nor a column be compared with a null, nor a replacement take a value that reads a column"
            }
            Self::DurationIsValid => "a duration typed in must be a duration string Polars reads",
            Self::TimeZoneIsValid => "a time zone must be one Polars reads",
            Self::ValuesMeetInCount => {
                "the values a call meets must be one value, which meets each of the others, or as many as one another: a count of values that the data or the arguments decide, as the first values of a column or the values that differ are, is not the table's rows and not another count it is not known to equal"
            }
            Self::WindowInputIsNotCounted => {
                "a window takes a value as long as its group, and a count of values that the data or the arguments decide is not, nor is the one value that a method took of one value, unless the window joins the values of a group into a list or explodes them"
            }
            Self::WindowInputIsNotWidened => {
                "a window takes a value as long as its input, and a value of one row that a part widens, as dt.replace's parts do, is not: Polars refuses the length of its output"
            }
            Self::MomentsShareAZone => {
                "moments that are compared, subtracted, filled, bound by one another, or given as the values of a condition or as a replacement's new values and default, must be in one time zone: Polars finds no dtype that two zones have in common, and fails where a shift is filled with a moment of another zone"
            }
            Self::ParameterTakesOneValue => {
                "a parameter that Polars reads as one value must be given one: not the rows, nor a count of values that the data or the arguments decide, and, for the count of rows that `shift` and `diff` look back by and the value `shift` fills with, not the one value that a method took of one value either, which Polars cannot tell is one before the rows run"
            }
            Self::PatternIsValid => "a pattern must be a valid regular expression",
            Self::RangeIsValid => {
                "a range or a repeat typed in must be one Polars can make and cut: a step other than nothing, an interval above nothing that its bounds are read in, bounds the dtype holds, a number of values that is not below nothing, and no length below nothing to cut it by"
            }
            Self::ReplacementValuesMatchInNumber => {
                "a replacement takes as many new values as old ones, or one, and a strict one a default of as many values as what it follows, or one"
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
            Self::FoldedFunctionIsNoJoinKey => {
                matches!(context.outer(), Outer::JoinLeft | Outer::JoinRight)
                    && matches!(entry, "polars.datetime" | "polars.duration")
            }
            // A matter of the values a call is given, whatever context it stands in.
            Self::DurationIsValid
            | Self::MomentsShareAZone
            | Self::ParameterTakesOneValue
            | Self::PatternIsValid
            | Self::RangeIsValid
            | Self::ReplacementValuesMatchInNumber
            | Self::TimeZoneIsValid
            | Self::ValuesMeetInCount
            | Self::WindowInputIsNotCounted
            | Self::WindowInputIsNotWidened => false,
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

    /// The rule that forbids a moment or a length of time made of numbers typed in, where
    /// `context` is a join's key or inside one, if it does. Whether the call is made of nothing else is a
    /// matter of its arguments, which the caller found.
    #[must_use]
    pub(super) fn forbids_a_folded_function_in_a_key(context: Context) -> Option<Self> {
        matches!(context.outer(), Outer::JoinLeft | Outer::JoinRight)
            .then_some(Self::FoldedFunctionIsNoJoinKey)
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
            | Self::FoldedFunctionIsNoJoinKey
            | Self::JoinKeyNamesAreNotRepeated
            | Self::KeyIsElementwise
            | Self::DurationIsValid
            | Self::PatternIsValid
            | Self::RangeIsValid
            | Self::ReplacementValuesMatchInNumber
            | Self::MomentsShareAZone
            | Self::ParameterTakesOneValue
            | Self::TimeZoneIsValid
            | Self::ValuesMeetInCount
            | Self::WindowInputIsNotCounted
            | Self::WindowInputIsNotWidened => false,
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}
