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
use polars::prelude::DataType;

use super::dtype::{Representative, ResultDtype, dtype_name};
use super::table::Shape;

/// A rule that holds wherever it applies, whatever Polars does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    /// A pivot's aggregate gives one value for each group.
    PivotAggregateGivesOneValue,
    /// A `having`'s predicate gives one value for each group, as an aggregate does that is no list:
    /// what gives the values of the group is a list that Polars cannot cast to a `Boolean` when it
    /// runs the rows.
    HavingGivesOneValue,
    /// A `having`'s predicate is a value Polars casts to a `Boolean` to keep a group or not where
    /// it stands alone: a `Boolean`, a number, or null. Text, a date, a list, a struct and the rest
    /// it refuses when it runs the rows, and beside another predicate, which Polars combines it with
    /// as a filter's are, it must be a `Boolean`.
    HavingCastsToBoolean,
    /// The columns a group by gives are named differently: its keys and its aggregates, each named
    /// by the column its chain reads or by the name it is given, and Polars refuses two of one name.
    GroupedNamesAreDistinct,
    /// A filter's only predicate is one column: an expression that gives several, as the unnest of a
    /// struct of several fields does, or none, as the unnest of a struct of no field does, is
    /// refused there, and taken beside another predicate.
    OneColumnIsNeeded,
    /// The columns that the items of a select or of `with_columns` write are named differently,
    /// as the fields that an unnest and a field read write are named by the fields.
    WrittenNamesAreDistinct,
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
    /// A field that a method reads or drops is one of the fields of the struct it follows, which the
    /// walk knows: Polars refuses a name that is none, one that was renamed or dropped before.
    FieldIsOfTheStruct,
    /// The names given to the fields of a struct are as many as its fields where it is renamed, or as
    /// the places of the array it is made of, and no two fields of a struct are named alike.
    FieldNamesFitTheStruct,
    /// No two names given to the fields of a struct that a list or an array is made are alike. This
    /// is stricter than Polars by design, and the rule can go once a release of Polars refuses these
    /// names itself.
    ///
    /// Polars 2.2.0 accepts them, though `polars.struct` and `struct.rename_fields` refuse the same
    /// names, and the struct it builds is inconsistent: its dtype and its series hold two fields of
    /// one name, `struct.field` reads the first and a row's value holds both, `struct.unnest` is
    /// refused after the plan accepted it (the Python binding keeps the first), and `clear`,
    /// `pl.concat` and a streaming query panic. The open pull request pola-rs/polars#30725, "Make
    /// struct dtypes valid by construction", makes the dtype refuse duplicate names. The test
    /// `polars_accepts_a_list_made_a_struct_of_a_name_given_twice_and_builds_one_that_disagrees_with_itself`
    /// shows it on the vendored Polars.
    ToStructNamesAreDistinct,
    /// A join pairs no two output column names twice: the keys of each side
    /// are named by the column they read, and a key that repeats the name of
    /// an earlier one, beside a key on the other side that repeats the name it
    /// was paired with, makes the same pair again.
    JoinKeyNamesAreNotRepeated,
    /// A duration typed in, where a method reads a text as one, is a duration string Polars reads.
    DurationIsValid,
    /// A method of the `list` or the `arr` namespace is run on an element dtype that Polars has
    /// an implementation of it for: for the others it panics where the rows run.
    ElementDtypeIsImplemented,
    /// A method whose answer depends on the element dtype of a list or an array is run on one the
    /// tables are measured for: for another Polars is not asked, and may panic.
    ElementDtypeIsMeasured,
    /// Arrays that meet and give an array are of one width, which is known: Polars refuses to add
    /// arrays of two widths and gives a list where it fills one with the other.
    ArraysMeetAtOneWidth,
    /// Where `maintain_order` keeps the order of the groups, a key of a group by gives one value or
    /// as many as the table has rows: Polars refuses a key that takes some of the values, as the
    /// first of them or those that are not null, for lengths that differ.
    KeptOrderNeedsKeysOfTheRows,
    /// A join's own key is elementwise: it holds no aggregate beside a column,
    /// no column compared with a null, no comparison of missing values of a column of whole numbers
    /// with a literal that is a number with a fraction or a whole number that its dtype cannot hold,
    /// as Polars reads the literal, and no replacement of values that are not literals.
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
    /// not below nothing and, of it, no length below nothing to cut any value by.
    RangeIsValid,
    /// A replacement is given as many new values as old ones, or one, and a strict one a
    /// default of as many values as what it follows, or one.
    ReplacementValuesMatchInNumber,
    /// A struct is filled by a strategy that goes forward or backward: the others, the smallest, the
    /// largest, the mean, zero and one, Polars refuses where the struct holds a null and does
    /// nothing with where it holds none.
    StructIsFilledForwardOrBackward,
    /// Arithmetic with a struct needs its fields to be numbers: Polars refuses `abs`, `neg`, `sign`,
    /// `clip` and `pow` of a struct whatever its fields are, and the operators of one that has a
    /// field of text, a date, a moment or a time.
    StructArithmeticNeedsNumbers,
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
    /// `is_in` looks a value up in a list or an array, one for each value or one for all of them, or
    /// in a collection imploded, and not in a collection nested as deeply as the value it follows,
    /// as a column of its own dtype is.
    ///
    /// Python Polars 2.2.0 deprecates that form as ambiguous (pola-rs/polars#22149), asking for the
    /// collection imploded, and panics on one value looked up in another: `pl.lit(5).is_in(pl.lit(1))`
    /// builds a series of an unknown dtype that it does not support. The catalogue omits the form,
    /// as the list of its [omitted forms](crate::catalog::omitted_forms) records, and the tables
    /// never run it.
    IsInTakesAList,
}

impl Rule {
    /// Every rule, in the order the tables list them.
    pub const ALL: [Self; 32] = [
        Self::ArraysMeetAtOneWidth,
        Self::ConditionPredicateIsBoolean,
        Self::DurationIsValid,
        Self::ElementDtypeIsImplemented,
        Self::ElementDtypeIsMeasured,
        Self::ElementOnlyInPivotAggregate,
        Self::FieldIsOfTheStruct,
        Self::FieldNamesFitTheStruct,
        Self::FilterPredicateIsBoolean,
        Self::FoldedFunctionIsNoJoinKey,
        Self::GroupedNamesAreDistinct,
        Self::HavingCastsToBoolean,
        Self::HavingGivesOneValue,
        Self::IsInTakesAList,
        Self::JoinKeyNamesAreNotRepeated,
        Self::KeptOrderNeedsKeysOfTheRows,
        Self::KeyIsElementwise,
        Self::MomentsShareAZone,
        Self::OneColumnIsNeeded,
        Self::ParameterTakesOneValue,
        Self::PatternIsValid,
        Self::PivotAggregateGivesOneValue,
        Self::RangeIsValid,
        Self::ReplacementValuesMatchInNumber,
        Self::StructArithmeticNeedsNumbers,
        Self::StructIsFilledForwardOrBackward,
        Self::TimeZoneIsValid,
        Self::ToStructNamesAreDistinct,
        Self::ValuesMeetInCount,
        Self::WindowInputIsNotCounted,
        Self::WindowInputIsNotWidened,
        Self::WrittenNamesAreDistinct,
    ];

    /// The name the tables give the rule.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PivotAggregateGivesOneValue => "pivot-aggregate-gives-one-value",
            Self::ConditionPredicateIsBoolean => "condition-predicate-is-boolean",
            Self::DurationIsValid => "duration-is-valid",
            Self::ElementDtypeIsImplemented => "element-dtype-is-implemented",
            Self::ElementDtypeIsMeasured => "element-dtype-is-measured",
            Self::ArraysMeetAtOneWidth => "arrays-meet-at-one-width",
            Self::ElementOnlyInPivotAggregate => "element-only-in-pivot-aggregate",
            Self::FieldIsOfTheStruct => "field-is-of-the-struct",
            Self::FieldNamesFitTheStruct => "field-names-fit-the-struct",
            Self::FilterPredicateIsBoolean => "filter-predicate-is-boolean",
            Self::FoldedFunctionIsNoJoinKey => "folded-function-is-no-join-key",
            Self::GroupedNamesAreDistinct => "grouped-names-are-distinct",
            Self::HavingCastsToBoolean => "having-casts-to-boolean",
            Self::HavingGivesOneValue => "having-gives-one-value",
            Self::IsInTakesAList => "is-in-takes-a-list",
            Self::JoinKeyNamesAreNotRepeated => "join-key-names-are-not-repeated",
            Self::KeptOrderNeedsKeysOfTheRows => "kept-order-needs-keys-of-the-rows",
            Self::KeyIsElementwise => "key-is-elementwise",
            Self::MomentsShareAZone => "moments-share-a-zone",
            Self::OneColumnIsNeeded => "one-column-is-needed",
            Self::ParameterTakesOneValue => "parameter-takes-one-value",
            Self::PatternIsValid => "pattern-is-valid",
            Self::RangeIsValid => "range-is-valid",
            Self::ReplacementValuesMatchInNumber => "replacement-values-match-in-number",
            Self::StructArithmeticNeedsNumbers => "struct-arithmetic-needs-numbers",
            Self::StructIsFilledForwardOrBackward => "struct-is-filled-forward-or-backward",
            Self::TimeZoneIsValid => "time-zone-is-valid",
            Self::ToStructNamesAreDistinct => "to-struct-names-are-distinct",
            Self::ValuesMeetInCount => "values-meet-in-count",
            Self::WindowInputIsNotCounted => "window-input-is-not-counted",
            Self::WindowInputIsNotWidened => "window-input-is-not-widened",
            Self::WrittenNamesAreDistinct => "written-names-are-distinct",
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
            Self::ArraysMeetAtOneWidth => {
                "arrays that meet, as the operands of a method or the values of a condition, and give an array, or that one is looked up in, must be of one width, which must be known: Polars refuses to add arrays of two widths or to look one up in the other, and gives a list where it fills one with the other or takes one for a value of a condition and the other for another"
            }
            Self::ElementDtypeIsMeasured => {
                "a method whose answer depends on the element dtype of a list or an array must be given one the tables measure it on, the whole numbers, the floats, text, booleans, dates and datetimes: Polars is not asked of another, which it may panic on"
            }
            Self::ElementOnlyInPivotAggregate => {
                "`polars.element` stands only in a pivot's aggregate"
            }
            Self::FieldIsOfTheStruct => {
                "a field that a method reads or drops must be one of the fields of the struct it follows: Polars refuses a name that is none, as one that was renamed or dropped before is"
            }
            Self::FieldNamesFitTheStruct => {
                "the names given to the fields of a struct must be as many as its fields where it is renamed, or as the places of the array it is made of, and no two fields of a struct may be named alike: Polars refuses names of another number and two fields of one name"
            }
            Self::OneColumnIsNeeded => {
                "a filter's only predicate must be one column: an expression that gives several, as the unnest of a struct of several fields does, is ambiguous, and one that gives none, as the unnest of a struct of no field does, is empty, and Polars refuses both; beside another predicate each expression it expands to is a predicate of its own, which it takes"
            }
            Self::WrittenNamesAreDistinct => {
                "the columns that the items of a select or of with_columns write must be named differently: an unnest and a field read write the names of the fields, and Polars refuses two items that write one name"
            }
            Self::FilterPredicateIsBoolean => "a filter's predicate must be Boolean",
            Self::FoldedFunctionIsNoJoinKey => {
                "a moment or a length of time made of numbers typed in is folded by Polars into a literal named by an alias, which a join's key may not hold: give it a column among its parts, or make it in a step before the join"
            }
            Self::GroupedNamesAreDistinct => {
                "the columns a group by gives must be named differently: a key is named by the column it reads or the name it is given, an aggregate by the column its chain reads or the name it is given, and Polars refuses two that are named alike, as a key and an aggregate of the same column are"
            }
            Self::HavingGivesOneValue => {
                "a having's predicate must give one value for each group: the values of the group, as a column or a method that keeps some of them gives, are a list that Polars cannot cast to a Boolean when it runs the rows, so give an aggregate such as the sum, the first value, whether all are true, or a count compared with a number"
            }
            Self::HavingCastsToBoolean => {
                "a having's predicate must be a value Polars casts to a Boolean to keep a group or not: a Boolean, a number, or null where it stands alone, and not text, a date, a list or a struct, which it refuses when it runs the rows; beside another predicate, which Polars combines it with as the predicates of a filter are, it must be a Boolean"
            }
            Self::IsInTakesAList => {
                "is_in must be given a list or an array to look the value up in, or a collection imploded, and not a collection nested as deeply as the value it follows, as a column of its own dtype is: Polars deprecates that form as ambiguous (pola-rs/polars#22149) and panics on one value looked up in another, so give a list, or the collection imploded"
            }
            Self::JoinKeyNamesAreNotRepeated => {
                "a join may not pair the same two output column names twice: where two keys on one side are named alike, the keys they are paired with must be named differently"
            }
            Self::KeptOrderNeedsKeysOfTheRows => {
                "where `maintain_order` keeps the order of the groups, a key must give one value or as many as the table has rows: Polars refuses a key that takes some of the values, as head, filter and drop_nulls do, for lengths that differ where the arguments or the data make it fewer, so give the key as a column, or leave the order unset"
            }
            Self::KeyIsElementwise => {
                "a join's key must be elementwise: an aggregate may not stand in it beside a column, nor a method that takes some of the values, as head and filter do, unless it goes on to an aggregate, nor a column be compared with a null, nor a column of whole numbers be compared for missing values with a literal that is a number with a fraction or a whole number its dtype cannot hold, as Polars reads the literal (a literal of 16 bit floats or of Boolean, and a float that does not cast exactly, are kept), nor a replacement take a value that reads a column"
            }
            Self::DurationIsValid => "a duration typed in must be a duration string Polars reads",
            Self::ElementDtypeIsImplemented => {
                "a method of lists and arrays must be given an element dtype that Polars implements it for: it panics for the others where the rows run, on a thread of its own that its streaming executor may not recover from"
            }
            Self::TimeZoneIsValid => "a time zone must be one Polars reads",
            Self::ToStructNamesAreDistinct => {
                "no two names given to the fields of a struct that a list or an array is made may be alike: Polars 2.2.0 accepts them and builds a struct that disagrees with itself, as its dtype and its series hold two fields of one name, and refuses the same names for a struct made of columns or renamed"
            }
            Self::StructArithmeticNeedsNumbers => {
                "arithmetic with a struct needs its fields to be numbers: Polars refuses the absolute value, the negation, the sign, a clip and a power of a struct whatever its fields are, and the sum, the difference, the product and the quotients of one that has a field of text, a date, a moment or a time"
            }
            Self::StructIsFilledForwardOrBackward => {
                "a struct is filled by a strategy only forward or backward: Polars refuses the smallest, the largest, the mean, zero and one where the struct holds a null, which a pivot's missing cells make, and does nothing where it holds none"
            }
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
                "a range or a repeat typed in must be one Polars can make and cut: a step other than nothing, an interval above nothing that its bounds are read in, bounds the dtype holds, a number of values that is not below nothing, and, of it, no length below nothing to cut any value by"
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
            Self::HavingGivesOneValue => context == Context::step(Outer::Having),
            Self::JoinKeyNamesAreNotRepeated | Self::KeyIsElementwise => context.is_join_key(),
            Self::FoldedFunctionIsNoJoinKey => {
                matches!(context.outer(), Outer::JoinLeft | Outer::JoinRight)
                    && matches!(entry, "polars.datetime" | "polars.duration")
            }
            // A matter of the values a call is given, whatever context it stands in.
            Self::DurationIsValid
            | Self::ElementDtypeIsImplemented
            | Self::ElementDtypeIsMeasured
            | Self::ArraysMeetAtOneWidth
            | Self::MomentsShareAZone
            | Self::FieldIsOfTheStruct
            | Self::FieldNamesFitTheStruct
            | Self::GroupedNamesAreDistinct
            | Self::IsInTakesAList
            | Self::KeptOrderNeedsKeysOfTheRows
            | Self::OneColumnIsNeeded
            | Self::ParameterTakesOneValue
            | Self::PatternIsValid
            | Self::RangeIsValid
            | Self::ReplacementValuesMatchInNumber
            | Self::StructArithmeticNeedsNumbers
            | Self::StructIsFilledForwardOrBackward
            | Self::TimeZoneIsValid
            | Self::ToStructNamesAreDistinct
            | Self::ValuesMeetInCount
            | Self::WindowInputIsNotCounted
            | Self::WindowInputIsNotWidened
            | Self::WrittenNamesAreDistinct => false,
            Self::ConditionPredicateIsBoolean
            | Self::FilterPredicateIsBoolean
            | Self::HavingCastsToBoolean => self.needs_boolean_end(context),
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

    /// The rule that forbids `entry` to follow a value of the dtype `receiver`, with what the
    /// element lacks, if one does: Polars panics where the rows run on the smallest and the largest
    /// of an array of anything but whole numbers and floats, on its values reversed, and on the
    /// mean of a list of `Float16`.
    ///
    /// The tables never run what a rule refuses, and a document is refused before Polars is asked.
    #[must_use]
    pub(super) fn forbids_receiver(entry: &str, receiver: &DataType) -> Option<(Self, String)> {
        let said = match (entry, receiver) {
            ("Expr.arr.min" | "Expr.arr.max" | "Expr.reverse", DataType::Array(element, _))
                if !(element.is_integer() || element.is_float()) =>
            {
                let what = match entry {
                    "Expr.arr.min" => "has no smallest value for",
                    "Expr.arr.max" => "has no largest value for",
                    _ => "cannot reverse",
                };
                format!(
                    "Polars {what} an array of {}, which it implements for whole numbers and floats only, and panics",
                    dtype_name(element)
                )
            }
            ("Expr.list.mean", DataType::List(element)) if **element == DataType::Float16 => {
                format!(
                    "Polars has no mean for a list of {}, and panics",
                    dtype_name(element)
                )
            }
            _ => return None,
        };
        Some((Self::ElementDtypeIsImplemented, said))
    }

    /// The rule that forbids `entry` to be given an argument of the dtype `argument` for `parameter`
    /// after a value of the dtype `receiver`, with what is the matter, if one does: `is_in` looks the
    /// value up in a list or an array, and Polars deprecates a collection nested as deeply as the
    /// value, which a column of its own dtype is, and panics on one value looked up in another.
    ///
    /// The tables never run what a rule refuses, and a document is refused before Polars is asked.
    #[must_use]
    pub(super) fn forbids_argument(
        entry: &str,
        parameter: &str,
        receiver: &DataType,
        argument: &DataType,
    ) -> Option<(Self, String)> {
        (entry == "Expr.is_in"
            && parameter == "other"
            && receiver.nesting_level() == argument.nesting_level())
        .then(|| {
            (
                Self::IsInTakesAList,
                format!(
                    "{} is looked up in {}, which is nested as the value is",
                    dtype_name(receiver),
                    dtype_name(argument)
                ),
            )
        })
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
    /// `among_others` is how many other predicates stand beside the one the chain is, where it is an
    /// item of a list of them.
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
        among_others: usize,
    ) -> Option<Self> {
        let known_not_boolean =
            result.is_some_and(|result| result.class() != Some(Representative::Boolean));
        if context == Context::step(Outer::PivotAggregate) && shape != Shape::One {
            return Some(Self::PivotAggregateGivesOneValue);
        }
        // A predicate of a `having` is one value for each group, which Polars casts to a `Boolean`
        // as it can where it stands alone, so a number is kept and a dtype it cannot cast is not.
        // Beside another predicate it is combined with it as a filter's are, which must be a
        // `Boolean`.
        // The value a condition gives is the predicate where the condition stands as it, so the
        // value of each branch is judged as the predicate is.
        if Self::is_a_having_predicate(context) {
            if context.nested().is_none() && shape != Shape::One {
                return Some(Self::HavingGivesOneValue);
            }
            let kept = |result: &ResultDtype| {
                if among_others == 0 {
                    casts_to_boolean(result)
                } else {
                    result.class() == Some(Representative::Boolean)
                }
            };
            return result
                .is_some_and(|result| !kept(result))
                .then_some(Self::HavingCastsToBoolean);
        }
        if known_not_boolean {
            Self::ALL
                .into_iter()
                .find(|rule| rule.needs_boolean_end(context))
        } else {
            None
        }
    }

    /// Whether `context` is the predicate of a `having` or the value a condition gives as it.
    pub(super) fn is_a_having_predicate(context: Context) -> bool {
        context.outer() == Outer::Having
            && matches!(context.nested(), None | Some(Nested::ConditionValue))
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
            Self::HavingCastsToBoolean => Self::is_a_having_predicate(context),
            Self::PivotAggregateGivesOneValue
            | Self::HavingGivesOneValue
            | Self::ElementOnlyInPivotAggregate
            | Self::FoldedFunctionIsNoJoinKey
            | Self::JoinKeyNamesAreNotRepeated
            | Self::KeyIsElementwise
            | Self::KeptOrderNeedsKeysOfTheRows
            | Self::DurationIsValid
            | Self::ElementDtypeIsImplemented
            | Self::ElementDtypeIsMeasured
            | Self::ArraysMeetAtOneWidth
            | Self::PatternIsValid
            | Self::RangeIsValid
            | Self::ReplacementValuesMatchInNumber
            | Self::MomentsShareAZone
            | Self::FieldIsOfTheStruct
            | Self::FieldNamesFitTheStruct
            | Self::GroupedNamesAreDistinct
            | Self::IsInTakesAList
            | Self::OneColumnIsNeeded
            | Self::ParameterTakesOneValue
            | Self::StructArithmeticNeedsNumbers
            | Self::StructIsFilledForwardOrBackward
            | Self::TimeZoneIsValid
            | Self::ToStructNamesAreDistinct
            | Self::ValuesMeetInCount
            | Self::WindowInputIsNotCounted
            | Self::WindowInputIsNotWidened
            | Self::WrittenNamesAreDistinct => false,
        }
    }
}

/// Whether Polars casts a value of the dtype `result` to a `Boolean`, as a `having` does to keep a
/// group or not: a `Boolean`, a number, or null. A dtype with no class is not one of them.
fn casts_to_boolean(result: &ResultDtype) -> bool {
    use Representative as R;
    matches!(
        result.class(),
        Some(
            R::Boolean
                | R::Null
                | R::Int8
                | R::Int16
                | R::Int32
                | R::Int64
                | R::Int128
                | R::UInt8
                | R::UInt16
                | R::UInt32
                | R::UInt64
                | R::UInt128
                | R::Float16
                | R::Float32
                | R::Float64
                | R::Decimal
        )
    )
}

impl fmt::Display for Rule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}
