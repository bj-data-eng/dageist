//! Which entries may stand at a place, answered from the committed tables
//! and the named rules.

use std::fmt;

use polars::prelude::{DataType, TimeUnit, TimeZone};

use crate::catalog::{self, Applies, DurationUse, Entry, Kind};

use super::adapt::{Loose, could_be_taken_by_a_number};
use super::context::{Context, Outer};
use super::dtype::{Representative, ResultDtype, dtype_name, unit_name};
use super::fields::{self, Fields, Gives, Input};
use super::rules::Rule;
use super::table::{
    Looked, OptionValue, Outcome, Produces, Reason, SENSITIVE, Shape, Stage, Tables, has_a_subject,
    is_method, is_of_nested_dtypes, read_as, restored, subject_of,
};
use crate::catalog::DefaultValue;

/// What an expression has so far, where an entry would follow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    /// Nothing: the entry would start an expression.
    Start,
    /// A value of a dtype, and of a shape: an expression method follows it.
    Value {
        /// The dtype of the value. It is the dtype of the column an
        /// expression that starts from `polars.col` reads, and the dtype of
        /// what a pivot's group holds where it starts from `polars.element`.
        dtype: Representative,
        /// Whether the chain so far gives one value, as an aggregate does.
        shape: Shape,
    },
    /// A condition that is not finished: the links that may follow it.
    Condition(ConditionState),
}

/// How far a condition has come: the call that was last made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionState {
    /// After `when`.
    When,
    /// After `when` and `then`.
    Then,
    /// After `then` and a further `when`.
    ChainedWhen,
    /// After a further `when` and its `then`.
    ChainedThen,
}

impl ConditionState {
    /// Every state, in the order a condition comes to them.
    pub const ALL: [Self; 4] = [Self::When, Self::Then, Self::ChainedWhen, Self::ChainedThen];

    const fn name(self) -> &'static str {
        match self {
            Self::When => "when",
            Self::Then => "then",
            Self::ChainedWhen => "chained when",
            Self::ChainedThen => "chained then",
        }
    }

    /// The state an entry that is a link of a condition continues.
    fn needed_by(entry: &str) -> Option<Self> {
        Some(match entry {
            "When.then" => Self::When,
            "Then.otherwise" | "Then.when" => Self::Then,
            "ChainedWhen.then" => Self::ChainedWhen,
            "ChainedThen.otherwise" | "ChainedThen.when" => Self::ChainedThen,
            _ => return None,
        })
    }
}

/// What has been chosen for a call of an entry, so far as it decides what the
/// entry gives: the dtypes of the arguments given to its expression
/// parameters, the options set, and the dtype of the value an entry makes that
/// the tables cannot give.
///
/// Nothing chosen is the [`Default`]: an entry that takes an expression it needs is
/// then judged for any dtype of argument that fits.
#[derive(Debug, Clone, Copy, Default)]
pub struct Chosen<'a> {
    pub(super) arguments: &'a [(&'a str, Representative)],
    pub(super) options: &'a [(&'a str, &'a str)],
    unclassed: &'a [&'a str],
    pub(super) patterns: &'a [(&'a str, &'a str)],
    durations: &'a [(&'a str, &'a str, DurationUse)],
    time_zones: &'a [(&'a str, Option<&'a str>)],
    unit: Option<TimeUnit>,
    pub(super) width: Option<usize>,
    zone: Option<&'a str>,
    zone_clash: Option<(&'a str, &'a str)>,
    gives: Option<Representative>,
    shape: Option<Shape>,
    /// The fields of the struct the call follows, where the chain knows them.
    pub(super) fields: Option<&'a Fields>,
    /// The fields of each argument that is a struct whose fields the chain knows, by the parameter
    /// that holds it.
    pub(super) argument_fields: &'a [(&'a str, &'a Fields)],
    /// The arguments that are a number typed without a dtype, which adapts to what it meets.
    pub(super) open_numbers: &'a [&'a str],
    /// The call follows a number typed without a dtype, which it keeps open in what it makes.
    pub(super) follows_an_open_number: bool,
    /// How many other expressions stand beside this one in the list it is an item of: the other
    /// predicates of a filter.
    pub(super) among_others: usize,
    /// The names the call is given, each by the parameter that holds it: the name of a field to read
    /// or drop, the new names of the fields, the name of a count.
    pub(super) names: &'a [(&'a str, &'a str)],
    /// The whole numbers the call is given, each by the parameter that holds it.
    pub(super) numbers: &'a [(&'a str, i64)],
    /// The name the value the call follows is written under, where the chain knows it.
    pub(super) output_name: Option<&'a str>,
    /// What a call that makes a struct of what it is given is given, one for each.
    pub(super) inputs: &'a [Input<'a>],
}

impl<'a> Chosen<'a> {
    /// The dtypes of the arguments given to expression parameters, each by the
    /// parameter's name.
    #[must_use]
    pub const fn with_arguments(mut self, arguments: &'a [(&'a str, Representative)]) -> Self {
        self.arguments = arguments;
        self
    }

    /// The expression parameters that are given and whose dtype is not known, by name. The tables
    /// are not asked of them, but that they are given may decide what another is: `replace_strict`
    /// converts what replaces leniently where it is given a default.
    #[must_use]
    pub const fn with_unclassed(mut self, unclassed: &'a [&'a str]) -> Self {
        self.unclassed = unclassed;
        self
    }

    /// The options set, each by the parameter's name and the value as the
    /// option table writes it (`true`, `false`, `unset`, or the word of a
    /// choice), as [`options`] takes them. A method is refused for the options
    /// that Polars refuses.
    #[must_use]
    pub const fn with_options(mut self, options: &'a [(&'a str, &'a str)]) -> Self {
        self.options = options;
        self
    }

    /// The patterns given to pattern parameters, each by the parameter's name and the text as
    /// typed, where the text is a regular expression: a pattern that a flag such as `literal`
    /// makes plain text is not one. A text Polars cannot compile is refused, and the groups of one
    /// given to a parameter that names the fields of what the method gives are what it gives.
    #[must_use]
    pub const fn with_patterns(mut self, patterns: &'a [(&'a str, &'a str)]) -> Self {
        self.patterns = patterns;
        self
    }

    /// The durations given to parameters that hold a duration string, each by the parameter's name
    /// and the text as typed. A text Polars does not read as a duration is refused in Polars' words,
    /// whatever follows it.
    #[must_use]
    pub const fn with_durations(
        mut self,
        durations: &'a [(&'a str, &'a str, DurationUse)],
    ) -> Self {
        self.durations = durations;
        self
    }

    /// The time zones given to parameters that take one, each by the parameter's name and the text
    /// as typed, or none for a call given no zone. A text Polars does not read as a time zone is
    /// refused in Polars' words, whatever follows it, and a call gives a moment in the zone, or in
    /// none where it is given none.
    #[must_use]
    pub const fn with_time_zones(mut self, time_zones: &'a [(&'a str, Option<&'a str>)]) -> Self {
        self.time_zones = time_zones;
        self
    }

    /// The two zones of the moments a method is given, where they differ: the one it follows and the
    /// one of an operand it is compared with, subtracted by, filled with or bound by. Polars finds
    /// no dtype that they have in common, which is refused whatever follows.
    #[must_use]
    pub const fn with_zone_clash(mut self, clash: Option<(&'a str, &'a str)>) -> Self {
        self.zone_clash = clash;
        self
    }

    /// The unit the moment or the length of time the method follows is counted in, where the chain
    /// knows it. The tables were measured on microseconds, so a method that keeps the unit gives
    /// the one it follows, and a duration is asked of Polars on a moment counted in it.
    #[must_use]
    pub const fn following(mut self, unit: Option<TimeUnit>) -> Self {
        self.unit = unit;
        self
    }

    /// The width of the array the method follows, where the chain knows it. The tables measured
    /// arrays at one width, so an array the call gives is written at this one.
    #[must_use]
    pub const fn following_width(mut self, width: Option<usize>) -> Self {
        self.width = width;
        self
    }

    /// How many other expressions stand beside this one in the list of items it is one of.
    #[must_use]
    pub const fn among_others(mut self, others: usize) -> Self {
        self.among_others = others;
        self
    }

    /// The arguments that are a number typed without a dtype, by the parameter that holds each.
    #[must_use]
    pub const fn with_open_numbers(mut self, numbers: &'a [&'a str]) -> Self {
        self.open_numbers = numbers;
        self
    }

    /// That the call follows a number typed without a dtype, as the first call of an expression
    /// that starts with one does.
    #[must_use]
    pub(crate) const fn following_an_open_number(mut self, open: bool) -> Self {
        self.follows_an_open_number = open;
        self
    }

    /// The fields of each argument that is a struct whose fields the chain knows, by the parameter
    /// that holds it.
    #[must_use]
    pub const fn with_argument_fields(mut self, fields: &'a [(&'a str, &'a Fields)]) -> Self {
        self.argument_fields = fields;
        self
    }

    /// The fields of the struct the call follows, where the chain knows them.
    #[must_use]
    pub const fn following_fields(mut self, fields: Option<&'a Fields>) -> Self {
        self.fields = fields;
        self
    }

    /// The names the call is given, each as `(parameter, name)`, in order: the fields it reads or
    /// drops, the names it gives the fields, the name it gives a count.
    #[must_use]
    pub const fn with_names(mut self, names: &'a [(&'a str, &'a str)]) -> Self {
        self.names = names;
        self
    }

    /// The whole numbers the call is given, each as `(parameter, number)`: how many pieces a text
    /// is split into.
    #[must_use]
    pub const fn with_numbers(mut self, numbers: &'a [(&'a str, i64)]) -> Self {
        self.numbers = numbers;
        self
    }

    /// The name the value the call follows is written under, where the chain knows it: the column a
    /// count of values names its values by.
    #[must_use]
    pub const fn named(mut self, name: Option<&'a str>) -> Self {
        self.output_name = name;
        self
    }

    /// What a function that makes a struct is given, one for each field it makes: the name that
    /// each is written under and the dtype it has, as far as they are known.
    #[must_use]
    pub const fn with_inputs(mut self, inputs: &'a [Input<'a>]) -> Self {
        self.inputs = inputs;
        self
    }

    /// The time zone of the moment the method follows, where the chain knows it by name: a length
    /// of time given to the method is read in it, and Polars reads days and hours together only in
    /// UTC, where a day is always 24 hours.
    #[must_use]
    pub const fn in_zone(mut self, zone: Option<&'a str>) -> Self {
        self.zone = zone;
        self
    }

    /// The dtype the entry gives, where the call decides it and the tables
    /// cannot: the column `polars.col` reads, the literal `polars.lit` makes
    /// when it is given a dtype, the values of a condition that `otherwise`
    /// ends. The tables give what a method gives, so a method ignores it.
    #[must_use]
    pub const fn giving(mut self, dtype: Representative) -> Self {
        self.gives = Some(dtype);
        self
    }

    /// That the values the entry gives are many, as those of a connected list
    /// are, and not one as a literal's is. An entry that makes a value gives
    /// one unless told so; a method's shape is the table's.
    #[must_use]
    pub const fn giving_many(self) -> Self {
        self.giving_shape(Shape::Many)
    }

    /// That the values the call gives are of `shape`, where the arguments it is given decide it
    /// and the tables, which measured one receiver and arguments of columns, do not: a method that
    /// is given an operand of many values gives many of one, and one that takes the values of its
    /// argument gives as many as it does. It decides whether the chain may end where it does.
    #[must_use]
    pub const fn giving_shape(mut self, shape: Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// That the values the entry gives are one, as those of a function are that is given nothing
    /// but one value each: a function that makes a moment of literals. A start that takes
    /// expressions gives as many values as its widest argument, which the tables, measured with
    /// columns, do not say.
    #[must_use]
    pub const fn giving_one(self) -> Self {
        self.giving_shape(Shape::One)
    }
}

/// Where an entry would stand: in which context, and after what.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    context: Context,
    after: After,
}

impl Place {
    /// The place after `after`, in `context`.
    #[must_use]
    pub const fn new(context: Context, after: After) -> Self {
        Self { context, after }
    }

    /// The context the expression stands in.
    #[must_use]
    pub const fn context(&self) -> Context {
        self.context
    }

    /// What the expression has so far.
    #[must_use]
    pub const fn after(&self) -> After {
        self.after
    }
}

/// Who refused an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalSource {
    /// Polars, as the tables recorded it, at this stage.
    Polars(Stage),
    /// A rule Polars does not hold.
    Rule(Rule),
    /// The shape of an expression: a step is not part of one, a method needs
    /// a value before it, a link needs its condition.
    Structure,
}

/// How the words of the refusal of a count of values begin, before the context it may not end.
const COUNT_WORDS: &str = "a count of values that the data or the arguments decide, which is \
     neither one value nor the table's rows, may not end an expression in";

/// Why an entry may not stand at a place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    source: RefusalSource,
    reason: String,
}

impl Refusal {
    pub(super) fn structure(reason: impl Into<String>) -> Self {
        Self {
            source: RefusalSource::Structure,
            reason: reason.into(),
        }
    }

    fn polars(reason: &Reason) -> Self {
        // A line of the tables that a rule wrote, because the tables never run what it refuses,
        // is the rule's refusal: its name begins the text, and what follows says what is the matter.
        if let Some((name, said)) = reason.text.split_once(": ")
            && let Some(rule) = Rule::from_name(name)
        {
            return Self::rule_because(rule, &said);
        }
        Self {
            source: RefusalSource::Polars(reason.stage),
            reason: reason.text.clone(),
        }
    }

    fn rule(rule: Rule) -> Self {
        Self {
            source: RefusalSource::Rule(rule),
            reason: rule.statement().to_owned(),
        }
    }

    /// A refusal by `rule` that adds what Polars says of the case, in its words.
    fn rule_because(rule: Rule, said: &impl fmt::Display) -> Self {
        Self {
            source: RefusalSource::Rule(rule),
            reason: format!("{}: {said}", rule.statement()),
        }
    }

    /// The refusal of a chain that gives a count of values other than one or the table's rows
    /// where `context` takes only those, in the words of the table of counts: what Polars says of
    /// such a chain there, after the count it is.
    fn count(context: Context, shape: Shape) -> Option<Self> {
        if shape != Shape::Counted {
            return None;
        }
        let Some(Outcome::Refused(reason)) = Tables::committed().counts.get(&(context, shape))
        else {
            return None;
        };
        Some(Self {
            source: RefusalSource::Polars(reason.stage),
            reason: format!("{COUNT_WORDS} {context}: {}", reason.text),
        })
    }

    /// Who refused it.
    #[must_use]
    pub const fn source(&self) -> RefusalSource {
        self.source
    }

    /// The reason: Polars' message when Polars refused, the rule's statement
    /// when a rule did.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for Refusal {}

/// Why whether an expression may end is not known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unknown {
    /// A start takes the dtype of a column or of a literal, which its arguments
    /// decide, and the table measured it with one argument. The dtype of what
    /// it gives was not told to [`offer`].
    TheStartsDtype,
    /// A dtype that is needed is not known: the argument that decides what a
    /// method gives is not chosen.
    TheResultsDtype,
    /// The fields of a struct that a method reads, drops or renames are not known, as where the
    /// chain lost them, so whether the names are fields of it is not.
    TheFieldsOfAStruct,
    /// An unnest as a join's key writes a column for each field, and how many keys the other
    /// table expands to is not known.
    AnUnnestAsAKey,
    /// An unnest of a struct that has no field expands to no expression, which Polars leaves out of
    /// what holds it or decides by its siblings.
    AnUnnestOfNoField,
    /// A number typed without a dtype stays open in a struct that is a join's key, and Polars does
    /// not match it with the other table's key as its dtype resolves.
    AnOpenNumberInAKey,
    /// A field read by several names gives several columns, which are no value of one dtype outside
    /// a select or a sort.
    SeveralColumnsOfAFieldRead,
    /// An unnest that is one of several predicates of a filter expands to a predicate for each field,
    /// or to none, which Polars takes as they come.
    AnUnnestAmongPredicates,
    /// Arithmetic on a struct is done field by field, and Polars does it irregularly.
    ArithmeticOnAStruct,
    /// Structs whose fields differ meet, which Polars casts field by field and refuses or not by
    /// what the fields are.
    StructsOfOtherFieldsMeet,
}

impl Unknown {
    /// Every reason, in the order they are listed.
    pub const ALL: [Self; 10] = [
        Self::TheStartsDtype,
        Self::TheResultsDtype,
        Self::TheFieldsOfAStruct,
        Self::AnUnnestAsAKey,
        Self::AnUnnestOfNoField,
        Self::AnOpenNumberInAKey,
        Self::SeveralColumnsOfAFieldRead,
        Self::AnUnnestAmongPredicates,
        Self::ArithmeticOnAStruct,
        Self::StructsOfOtherFieldsMeet,
    ];

    /// The name of the reason.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TheStartsDtype => "the-starts-dtype",
            Self::TheResultsDtype => "the-results-dtype",
            Self::TheFieldsOfAStruct => "the-fields-of-a-struct",
            Self::AnUnnestAsAKey => "an-unnest-as-a-key",
            Self::AnUnnestOfNoField => "an-unnest-of-no-field",
            Self::AnOpenNumberInAKey => "an-open-number-in-a-key",
            Self::SeveralColumnsOfAFieldRead => "several-columns-of-a-field-read",
            Self::AnUnnestAmongPredicates => "an-unnest-among-predicates",
            Self::ArithmeticOnAStruct => "arithmetic-on-a-struct",
            Self::StructsOfOtherFieldsMeet => "structs-of-other-fields-meet",
        }
    }
}

impl fmt::Display for Unknown {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TheStartsDtype => "the dtype of the start is not given",
            Self::TheResultsDtype => "the dtype that the arguments decide is not known",
            Self::TheFieldsOfAStruct => "the fields of the struct are not known, so whether the names the call reads, drops or renames are fields of it cannot be said",
            Self::AnUnnestAsAKey => "an unnest as a join's key writes a column for each field, and how many keys the other table expands to is not known",
            Self::AnUnnestOfNoField => "an unnest of a struct that has no field expands to no expression, which Polars leaves out or decides by its siblings",
            Self::AnOpenNumberInAKey => "a number typed without a dtype stays open in a struct that is a join's key, as `dyn float` in its dtype, which Polars does not match with the dtype the other table's key resolves to",
            Self::SeveralColumnsOfAFieldRead => "a field read by several names gives several columns, which are no value of one dtype outside a select or a sort",
            Self::AnUnnestAmongPredicates => "an unnest that is one of several predicates of a filter expands to a predicate for each field, or to none, which Polars takes as they come",
            Self::ArithmeticOnAStruct => "arithmetic on a struct is done field by field, by the dtype of each field and of what meets it, and Polars does it irregularly",
            Self::StructsOfOtherFieldsMeet => "structs whose fields differ meet, which Polars casts field by field and refuses or not by what the fields are",
        })
    }
}

/// Whether an expression may end after an entry, in the context it stands in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    /// It may.
    Allowed,
    /// It may not, for this reason.
    Refused(Refusal),
    /// What the tables and the rules hold does not say.
    NotKnown(Unknown),
}

impl Ending {
    /// Whether it may end there.
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// The refusal, when it may not.
    #[must_use]
    pub const fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Refused(refusal) => Some(refusal),
            _ => None,
        }
    }
}

/// What an entry that may stand at a place gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowance {
    result: Option<ResultDtype>,
    shape: Shape,
    ending: Ending,
    fields: Option<Fields>,
    columns: Option<Fields>,
    width: Option<usize>,
}

impl Allowance {
    /// The same allowance with no dtype for the result, where the argument that
    /// was judged adapts to what it follows and the tables, measured with typed
    /// columns, cannot say what the call gives.
    pub(super) fn without_result(mut self) -> Self {
        self.result = None;
        self
    }

    /// The same allowance with the dtype and the shape a call gives for every number the
    /// argument could be, as [`same_for_any_number`] finds them.
    pub(super) fn with_result(mut self, result: ResultDtype, shape: Shape) -> Self {
        self.result = Some(result);
        self.shape = shape;
        self
    }

    /// The same allowance with `ending` as whether the expression may end after the entry,
    /// where another place than the entry's own is the one the expression ends in.
    pub(super) fn ending_as(mut self, ending: Ending) -> Self {
        self.ending = ending;
        self
    }

    /// The dtype the chain has after the entry. `None` where it is not known
    /// from the entry: a start takes the dtype of a column or of a group, and
    /// a method that takes an argument gives a dtype the argument decides.
    #[must_use]
    pub const fn result(&self) -> Option<&ResultDtype> {
        self.result.as_ref()
    }

    /// The fields of the struct the chain has after the entry, where it is one whose fields are
    /// known: the struct a call makes, the one it keeps, or the one a field read holds.
    #[must_use]
    pub const fn fields(&self) -> Option<&Fields> {
        self.fields.as_ref()
    }

    /// The width of the array the chain has after the entry where the entry decides it: the array a
    /// field read holds.
    #[must_use]
    pub const fn width(&self) -> Option<usize> {
        self.width
    }

    /// The columns the entry writes where it writes several, one for each field of a struct it
    /// unnests, and the fields are known: the chain has no dtype of one value then.
    #[must_use]
    pub const fn columns(&self) -> Option<&Fields> {
        self.columns.as_ref()
    }

    /// Whether the chain gives one value after the entry.
    #[must_use]
    pub const fn shape(&self) -> Shape {
        self.shape
    }

    /// Whether the expression may end after the entry, in the context it
    /// stands in. A chain that is only to be continued is fine, and an entry
    /// that cannot end it may be followed by one that can: an entry that may
    /// not end a chain may still stand.
    ///
    /// A refusal is the rule that the expression, ended here, would break, or
    /// else the reason Polars refused the entry ending a chain in this context,
    /// as the table of contexts recorded it.
    #[must_use]
    pub const fn can_end(&self) -> &Ending {
        &self.ending
    }
}

/// Whether an entry may stand at a place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// It may, and gives this.
    Allowed(Allowance),
    /// It may not, for this reason.
    Refused(Refusal),
}

impl Verdict {
    /// The allowance, when the entry may stand there.
    #[must_use]
    pub const fn allowance(&self) -> Option<&Allowance> {
        match self {
            Self::Allowed(allowance) => Some(allowance),
            Self::Refused(_) => None,
        }
    }

    /// The refusal, when it may not.
    #[must_use]
    pub const fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Allowed(_) => None,
            Self::Refused(refusal) => Some(refusal),
        }
    }

    /// Whether the entry may stand there.
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed(_))
    }
}

/// An entry and whether it may stand at a place.
#[derive(Debug, Clone)]
pub struct Offer {
    entry: &'static Entry,
    verdict: Verdict,
}

impl Offer {
    /// The entry.
    #[must_use]
    pub const fn entry(&self) -> &'static Entry {
        self.entry
    }

    /// Whether it may stand there, or why not.
    #[must_use]
    pub const fn verdict(&self) -> &Verdict {
        &self.verdict
    }
}

/// The expression parameters of an entry that must be given.
fn required_expressions(entry: &Entry) -> impl Iterator<Item = &'static str> {
    entry
        .parameters()
        .iter()
        .filter(|parameter| {
            parameter.is_needed() && matches!(parameter.kind(), Kind::Expression(_))
        })
        .map(catalog::Parameter::name)
}

/// The class whose lines of the tables are read for `class` in the entry `id`: none for a call
/// that follows no value, and the outer none where the tables do not judge the class.
fn read_as_of(id: &str, class: Option<Representative>) -> Option<Option<Representative>> {
    match class {
        None => Some(None),
        Some(class) => read_as(id, class).map(Some),
    }
}

/// What a method gives on the receiver it follows.
struct Fit {
    result: Option<ResultDtype>,
    shape: Shape,
}

impl Fit {
    fn of(produces: Option<&Produces>) -> Self {
        Self {
            result: produces.map(|produces| produces.result.clone()),
            shape: produces.map_or(Shape::Many, |produces| produces.shape),
        }
    }
}

/// What the dtype table says of a method on a receiver, with the arguments
/// given: what it gives, or the first refusal. What the arguments given decide
/// is what it gives, as a `fill_value` of text makes a shift give text; where
/// two of them give different results, it is not known. A parameter that must
/// be given and is not needs only some dtype of argument that fits.
fn on_receiver(
    entry: &'static Entry,
    receiver: Option<Representative>,
    arguments: &[(&str, Representative)],
    options: &[(&str, &str)],
) -> Result<Fit, Refusal> {
    let tables = Tables::committed();
    let id = entry.id().as_str();
    // `is_in` follows a list or an array only looking up in a collection of lists, which no class of
    // the tables is: no argument of a class fits, and one that has none is not judged.
    if id == "Expr.is_in"
        && receiver.is_some_and(Representative::is_nested)
        && !arguments.iter().any(|(name, _)| *name == "other")
    {
        return Err(Refusal::rule_because(
            Rule::ElementDtypeIsMeasured,
            &"Expr.is_in follows a list or an array only given a collection of lists, which the tables do not measure",
        ));
    }
    // A method that depends on the element is refused after a class it is not measured on, whether
    // it is given an expression or an option in place of one, which no line of the dtype table says.
    if let Some(class) = receiver
        && read_as(id, class).is_none()
    {
        return Err(Refusal::rule_because(
            Rule::ElementDtypeIsMeasured,
            &format!("{id} is not measured on {class}"),
        ));
    }
    let stood_in_for = stood_in_for(entry, options);
    let missing = || match receiver {
        Some(receiver) => {
            Refusal::structure(format!("the tables hold nothing for {id} on {receiver}"))
        }
        None => Refusal::structure(format!("the tables hold nothing for {id}")),
    };
    let line = |parameter: Option<&'static str>, argument: Option<Representative>| {
        // What Polars deprecates is refused before it is asked, in the words of its rule.
        if let (Some(receiver), Some(parameter), Some(argument)) = (receiver, parameter, argument)
            && let Some((rule, said)) =
                Rule::forbids_argument(id, parameter, &receiver.dtype(), &argument.dtype())
        {
            return Err(Refusal::rule_because(rule, &said));
        }
        // The line of the class that stands for one the tables hold none for. A method whose
        // answer depends on the element is refused after a class it is not measured on: Polars is
        // not asked of it, and may panic.
        match tables.dtype(id, parameter, receiver, argument) {
            Looked::NotJudged => {
                let unmeasured = [receiver, argument]
                    .into_iter()
                    .flatten()
                    .find(|class| read_as(id, *class).is_none())
                    .map_or_else(String::new, |class| class.to_string());
                Err(Refusal::rule_because(
                    Rule::ElementDtypeIsMeasured,
                    &format!("{id} is not measured on {unmeasured}"),
                ))
            }
            Looked::Missing => Err(missing()),
            Looked::Line(Outcome::Refused(reason)) => Err(Refusal::polars(&reason)),
            Looked::Line(Outcome::Allowed(produces)) => Ok(Fit::of(produces.as_ref())),
        }
    };
    let mut fit = None;
    // What a chosen option is given in place of is not given, so the lines of the dtype table
    // that give it say nothing of the call: the option table does.
    let required: Vec<&'static str> = required_expressions(entry)
        .filter(|parameter| !stood_in_for.contains(parameter))
        .collect();
    // What the method is refused for with none of its arguments, which it is measured with where
    // it needs none. Where arguments are given, the lines of the calls that have them say what
    // Polars does of the call, and which of two failures it meets first, as it converts a part it
    // is given before it meets the value it follows, so the refusal of an argument is the one told.
    let alone = (required.is_empty() && stood_in_for.is_empty()).then(|| line(None, None));
    // What the arguments given decide: each is measured on its own, so what
    // the method gives is what they agree on, and not known where they do not.
    let mut given_fits: Vec<Fit> = Vec::new();
    for (name, dtype) in arguments {
        let parameter = entry
            .parameters()
            .iter()
            .map(catalog::Parameter::name)
            .find(|parameter| parameter == name)
            .ok_or_else(|| Refusal::structure(format!("{id} has no parameter {name}")))?;
        given_fits.push(line(Some(parameter), Some(*dtype))?);
    }
    if let Some(alone) = alone {
        fit = Some(alone?);
    }
    if let Some(first) = given_fits.first() {
        let same_result = given_fits.iter().all(|found| found.result == first.result);
        let same_shape = given_fits.iter().all(|found| found.shape == first.shape);
        fit = Some(Fit {
            result: if same_result {
                first.result.clone()
            } else {
                None
            },
            shape: if same_shape { first.shape } else { Shape::Many },
        });
    }
    for parameter in required {
        if arguments.iter().any(|(name, _)| *name == parameter) {
            continue;
        }
        let mut refusal: Option<Refusal> = None;
        let mut fitting: Vec<Fit> = Vec::new();
        for argument in Representative::MEASURED {
            match line(Some(parameter), Some(argument)) {
                Ok(found) => fitting.push(found),
                // The first refusal is the one told, but not the rule that asks for a list, which
                // refuses every argument nested as the value is and says nothing of the others.
                Err(error) => {
                    let held_is_the_rule = refusal.as_ref().is_some_and(|held| {
                        held.source() == RefusalSource::Rule(Rule::IsInTakesAList)
                    });
                    if refusal.is_none() || held_is_the_rule {
                        refusal = Some(error);
                    }
                }
            }
        }
        let Some(first) = fitting.first() else {
            return Err(refusal.unwrap_or_else(missing));
        };
        // What the argument decides is known when every dtype that fits
        // gives the same.
        let same_result = fitting.iter().all(|found| found.result == first.result);
        let same_shape = fitting.iter().all(|found| found.shape == first.shape);
        fit.get_or_insert(Fit {
            result: if same_result {
                first.result.clone()
            } else {
                None
            },
            shape: if same_shape { first.shape } else { Shape::Many },
        });
    }
    match fit {
        Some(fit) => Ok(fit),
        // The option table says what the call gives.
        None if !stood_in_for.is_empty() => Ok(Fit::of(None)),
        None => Err(missing()),
    }
}

/// The parameters of `entry` that a chosen option is given in place of: they are not given.
fn stood_in_for(entry: &Entry, options: &[(&str, &str)]) -> Vec<&'static str> {
    entry
        .parameters()
        .iter()
        .filter_map(|parameter| {
            let instead = parameter.in_place_of()?;
            let set = options.iter().any(|(name, word)| {
                *name == parameter.name() && default_word(parameter).as_deref() != Some(*word)
            });
            set.then_some(instead)
        })
        .collect()
}

/// The options chosen that are given in place of another parameter, which a method with them stands
/// in a context in the form of.
fn form_of(entry: &Entry, chosen: &Chosen<'_>) -> Vec<(&'static str, String)> {
    entry
        .parameters()
        .iter()
        .filter(|parameter| parameter.in_place_of().is_some())
        .filter_map(|parameter| {
            let word = chosen
                .options
                .iter()
                .find(|(name, _)| *name == parameter.name())?
                .1;
            (default_word(parameter).as_deref() != Some(word))
                .then(|| (parameter.name(), word.to_owned()))
        })
        .collect()
}

/// What the option table says the options chosen give.
enum OptionsGive {
    /// They change nothing the table records: no option is set to other than its
    /// default.
    Nothing,
    /// The lines that bear on them agree on this.
    Gives(Produces),
    /// The lines that bear on them give different results.
    Unknown,
}

/// The word the option table writes for the default of `parameter`, when it is
/// a flag or a choice that has one.
pub(super) fn default_word(parameter: &catalog::Parameter) -> Option<String> {
    match parameter.default().value() {
        DefaultValue::Flag(flag) => Some(flag.to_string()),
        DefaultValue::Unset => Some("unset".to_owned()),
        DefaultValue::Text(word) => Some(word),
        _ => None,
    }
}

/// What the option table says the options `chosen` of `entry` give. An option
/// set to its default is not a choice. One option chosen is read from the lines
/// that set it with every other option at its default, and more from the line
/// of each pair of them; where those lines give different results, the result
/// is not known.
fn read_options(
    entry: &Entry,
    receiver: Option<Representative>,
    chosen: &[(&str, &str)],
) -> OptionsGive {
    let tables = Tables::committed();
    let id = entry.id().as_str();
    let parameter = |name: &str| entry.parameters().iter().find(|found| found.name() == name);
    let changed: Vec<(&str, &str)> = chosen
        .iter()
        .copied()
        .filter(|(name, word)| {
            parameter(name).is_some_and(|found| default_word(found).as_deref() != Some(word))
        })
        .collect();
    let at_default = |(name, word): &(&str, String)| {
        parameter(name).is_some_and(|found| default_word(found).as_deref() == Some(word.as_str()))
    };
    let is_chosen = |(name, word): &(&str, String)| {
        changed
            .iter()
            .any(|(chosen_name, chosen_word)| chosen_name == name && chosen_word == word)
    };
    let Some(read_receiver) = read_as_of(id, receiver) else {
        return OptionsGive::Nothing;
    };
    // A call given the dtype it gives, as a cast is, gives that dtype whatever it follows, so what
    // it gives is not the class the line was read for that it merely equals.
    let by_a_word = changed
        .iter()
        .any(|(name, _)| parameter(name).is_some_and(|found| found.kind() == Kind::Dtype));
    let mut gives: Vec<Produces> = Vec::new();
    for ((line_id, line_receiver, line), outcome) in &tables.options {
        let bears = match changed.as_slice() {
            [] => false,
            [_] => {
                line.iter().any(is_chosen)
                    && line
                        .iter()
                        .all(|option| is_chosen(option) || at_default(option))
            }
            _ => line.len() > 1 && line.iter().all(is_chosen),
        };
        if *line_id == id
            && *line_receiver == read_receiver
            && bears
            && let Outcome::Allowed(Some(produces)) = if by_a_word {
                outcome.clone()
            } else {
                restored(outcome, &[receiver], &[read_receiver])
            }
        {
            gives.push(produces);
        }
    }
    let Some(first) = gives.first().cloned() else {
        return OptionsGive::Nothing;
    };
    if gives.iter().all(|produces| *produces == first) {
        OptionsGive::Gives(first)
    } else {
        OptionsGive::Unknown
    }
}

/// The expression parameters of which a call has to be given one for Polars to
/// run it, and for the option table to be read: a window is grouped by
/// `partition_by` or ordered by `order_by`, and Polars refuses one given
/// neither, whatever its options. The table was measured with both, and Polars
/// gives a window the same result with either alone.
fn measured_with(entry: &Entry) -> &'static [&'static str] {
    match entry.id().as_str() {
        "Expr.over" => &["partition_by", "order_by"],
        _ => &[],
    }
}

/// Whether the entry `entry` may stand at `place`, given what has been
/// [chosen](Chosen) for it, the dtypes of the arguments for its expression
/// parameters.
///
/// A rule that forbids the entry where it stands is applied first, so that its
/// words explain the refusal. A method is then judged by the dtype table for
/// the receiver it follows and for each argument given, one parameter at a
/// time, and by the context table for where it stands: the entry may stand
/// where the chain may go on from it, which the table measured with the
/// neutral continuation the context needs. When it takes an expression it
/// needs and none is given, it may stand if some dtype fits, and gives a dtype
/// when every dtype that fits gives the same. Last, whether the chain may end
/// at the entry is worked out: the rules of how a chain ends are applied to
/// what it gives, and what the table says of an entry that ends there. A chain
/// that may not end does not refuse the entry, which may be continued; that is
/// what [`Allowance::can_end`] reports. Where what decides it is not known, it
/// says so: a start whose arguments decide, and was not told the dtype it gives
/// (see [`Chosen::giving`]), or a method whose result an argument decides in a
/// place that needs a `Boolean`.
///
/// The context table was measured at the receiver that best fits each entry,
/// so where the context cares about the receiver, as a predicate cares that it
/// is `Boolean`, the rules of how a chain ends carry what the table cannot. A
/// result whose dtype is not known, which the argument will decide, does not
/// break a rule that is about a dtype: it is not refused for what is not known.
#[must_use]
pub fn offer(entry: &'static Entry, place: &Place, chosen: &Chosen<'_>) -> Verdict {
    match judge(entry, place, chosen) {
        Ok(allowance) => Verdict::Allowed(allowance),
        Err(refusal) => Verdict::Refused(refusal),
    }
}

/// What a method gives, and whether one value, when its argument `parameter` is a number
/// typed without a dtype that the tables cannot give a dtype, and the arguments and options
/// `chosen` are given besides: what it gives for every dtype the number could take that the
/// tables allow, where that is the same for them all. A comparison is a `Boolean` whatever the
/// number is. `None` where the dtypes give different results or none allows the call.
///
/// The number could take any whole number or float dtype, or a `Decimal` that it adapts to, and
/// no other: that a `Time` is subtracted from a `Time` is no word on a number.
pub(crate) fn same_for_any_number(
    entry: &'static Entry,
    place: &Place,
    chosen: &Chosen<'_>,
    parameter: &str,
) -> Option<(ResultDtype, Shape)> {
    let mut found: Option<(ResultDtype, Shape)> = None;
    for class in Representative::ALL
        .into_iter()
        .filter(|class| could_be_taken_by_a_number(*class))
    {
        let mut arguments = chosen.arguments.to_vec();
        arguments.push((parameter, class));
        let Verdict::Allowed(allowance) = offer(entry, place, &chosen.with_arguments(&arguments))
        else {
            continue;
        };
        let (result, shape) = (allowance.result?, allowance.shape);
        match &found {
            None => found = Some((result, shape)),
            Some((first, first_shape)) if *first == result && *first_shape == shape => {}
            Some(_) => return None,
        }
    }
    found
}

/// Why Polars refuses a call whose argument `parameter` is a number typed without a dtype, where no
/// dtype the number could take lets the call stand: the number adapts to a number the call meets
/// and to nothing else, so beside a date, a moment, a length of time, a time of day or a text it
/// has no dtype, and the call is refused whatever the number is. The words are the tables' for the
/// dtype the number has alone, a whole number's `Int64` and a number with a fraction's `Float64`, or
/// for the first dtype that fits where the tables say nothing of them. `None` where some dtype
/// lets the call stand or one is refused only when the rows run, or none fits.
pub(crate) fn refused_for_every_number(
    entry: &'static Entry,
    place: &Place,
    chosen: &Chosen<'_>,
    parameter: &str,
    number: Loose,
) -> Option<Refusal> {
    if !follows_a_moment(place) {
        return None;
    }
    refused_for_every_dtype(number, |class| {
        let mut arguments = chosen.arguments.to_vec();
        arguments.push((parameter, class));
        offer(entry, place, &chosen.with_arguments(&arguments))
    })
}

/// Why Polars refuses a call that follows a number typed without a dtype and is given a date, a
/// moment, a length of time or a time of day as `argument`, where no dtype the number could take
/// lets the call stand: the number is the other side of the arithmetic of a moment, and has no
/// dtype beside it, whichever of the two the other is given to. As [`refused_for_every_number`]
/// says it for the number that is given, this says it for the number that is followed. `None`
/// where some dtype lets the call stand, as a length of time multiplied by a number does.
pub(crate) fn refused_for_every_receiver_number(
    entry: &'static Entry,
    context: Context,
    shape: Shape,
    argument: (&str, Representative),
    number: Loose,
) -> Option<Refusal> {
    if !is_a_moment(argument.1) {
        return None;
    }
    refused_for_every_dtype(number, |class| {
        let place = Place::new(
            context,
            After::Value {
                dtype: class,
                shape,
            },
        );
        offer(
            entry,
            &place,
            &Chosen::default().with_arguments(&[argument]),
        )
    })
}

/// The refusal for the dtype a number typed without a dtype has alone, where the tables refuse the
/// call for every dtype it could take, and only for what Polars refuses when it works out the
/// schema: what it refuses when the rows run is the table of rows' to say, as it is for a number of a
/// dtype. `None` where some dtype lets the call stand.
fn refused_for_every_dtype(
    number: Loose,
    mut offered: impl FnMut(Representative) -> Verdict,
) -> Option<Refusal> {
    let alone = if number.is_whole() {
        Representative::Int64
    } else {
        Representative::Float64
    };
    let mut refusals: Vec<(Representative, Refusal)> = Vec::new();
    for class in Representative::ALL
        .into_iter()
        .filter(|class| could_be_taken_by_a_number(*class) && number.fits(*class))
    {
        match offered(class) {
            Verdict::Allowed(_) => return None,
            Verdict::Refused(refusal) => refusals.push((class, refusal)),
        }
    }
    if refusals
        .iter()
        .any(|(_, refusal)| refusal.source != RefusalSource::Polars(Stage::Schema))
    {
        return None;
    }
    let at = refusals
        .iter()
        .position(|(class, _)| *class == alone)
        .unwrap_or(0);
    refusals.into_iter().nth(at).map(|(_, refusal)| refusal)
}

/// Whether the dtype is a date, a moment, a length of time or a time of day.
const fn is_a_moment(dtype: Representative) -> bool {
    matches!(
        dtype,
        Representative::Date
            | Representative::Time
            | Representative::Datetime
            | Representative::DatetimeZoned
            | Representative::Duration
    )
}

/// Whether the call follows a date, a moment, a length of time or a time of day, which a number
/// typed without a dtype has no dtype beside, where it has one beside every other number.
fn follows_a_moment(place: &Place) -> bool {
    matches!(place.after(), After::Value { dtype, .. } if is_a_moment(dtype))
}

/// Why Polars refuses the value `fill_null` fills with, where it is a number typed without a
/// dtype and the call follows a date, a moment, a length of time or a time of day: the number has
/// the dtype of a number it follows and none beside these, so Polars finds the dtypes ambiguous
/// where a number of a dtype is taken. `None` where the call follows another dtype, and for the
/// other methods that fill, of which `shift` takes such a number as it takes any other.
pub(crate) fn refused_as_ambiguous(
    entry: &'static Entry,
    place: &Place,
    parameter: &str,
) -> Option<Refusal> {
    let After::Value { dtype, .. } = place.after() else {
        return None;
    };
    (entry.id().as_str() == "Expr.fill_null" && follows_a_moment(place)).then(|| Refusal {
        source: RefusalSource::Polars(Stage::Schema),
        reason: format!(
            "{parameter} of {} is a number typed without a dtype, which has the dtype of a number \
             it follows and none beside {}: Polars finds the dtypes ambiguous",
            entry.id().as_str(),
            dtype.name()
        ),
    })
}

fn judge(entry: &'static Entry, place: &Place, chosen: &Chosen<'_>) -> Result<Allowance, Refusal> {
    let id = entry.id().as_str();
    let tables = Tables::committed();
    // Whether the entry can follow what the expression has.
    match (entry.applies(), place.after) {
        (Applies::Frame, _) => {
            return Err(Refusal::structure(
                "a step stands in the list of steps, not in an expression",
            ));
        }
        (Applies::Expression, After::Value { .. }) => {}
        (Applies::Expression, After::Start) => {
            return Err(Refusal::structure(
                "a method needs the value it applies to before it",
            ));
        }
        (Applies::Expression, After::Condition(state)) => {
            return Err(Refusal::structure(format!(
                "a method cannot follow a condition that is at its {}; continue or end the condition first",
                state.name()
            )));
        }
        (Applies::Function, After::Start) => {}
        (Applies::Function, _) => {
            return Err(Refusal::structure(
                "an expression starts here only once: a start cannot follow a value",
            ));
        }
        (Applies::Condition, after) => match (ConditionState::needed_by(id), after) {
            (None, After::Start) => {}
            (None, _) => {
                return Err(Refusal::structure(
                    "`polars.when` starts an expression and follows nothing",
                ));
            }
            (Some(needed), After::Condition(state)) if needed == state => {}
            (Some(needed), _) => {
                return Err(Refusal::structure(format!(
                    "this continues a condition that is at its {}",
                    needed.name()
                )));
            }
        },
    }
    // A rule that forbids the entry here says so in words of its own, where
    // Polars' reason may be a panic.
    if let Some(rule) = Rule::forbids(id, place.context) {
        return Err(Refusal::rule(rule));
    }
    // What Polars panics on is refused before it is asked, in words that say what the element lacks.
    if let After::Value { dtype, .. } = place.after
        && let Some((rule, said)) = Rule::forbids_receiver(id, &dtype.dtype())
    {
        return Err(Refusal::rule_because(rule, &said));
    }
    // What Polars deprecates in an argument is refused before it is asked, in the words of its rule:
    // a collection nested as deeply as the value `is_in` follows.
    // A number typed in without a dtype is a scalar of the dtype it adapts to, so it is nested as a
    // value that is no list is.
    if let After::Value { dtype, .. } = place.after {
        let numbers = chosen
            .open_numbers
            .iter()
            .map(|name| (*name, Representative::Int64));
        for (name, argument) in chosen.arguments.iter().copied().chain(numbers) {
            if let Some((rule, said)) =
                Rule::forbids_argument(id, name, &dtype.dtype(), &argument.dtype())
            {
                return Err(Refusal::rule_because(rule, &said));
            }
        }
    }
    // What the call says of a struct: a field that is none, names that do not fit, or fields made.
    let receiver_class = match place.after {
        After::Value { dtype, .. } => Some(dtype),
        _ => None,
    };
    let struct_gives = fields::gives(entry, receiver_class, chosen)
        .map_err(|(rule, said)| Refusal::rule_because(rule, &said))?;
    // A filter's predicate is one column. The tables measured an unnest on a struct of two fields, so
    // a struct of one, which is one column, is judged as the field it is, and one of none, which
    // Polars expands to no expression, is refused.
    if struct_gives
        .columns()
        .is_some_and(|columns| columns.len() != 1)
        && place.context.outer() == Outer::Filter
        && place.context.nested().is_none()
        && chosen.among_others == 0
    {
        return Err(Refusal::rule(Rule::OneColumnIsNeeded));
    }
    // Arithmetic on a struct is done field by field, each by its own dtype and that of what meets it,
    // and differs for each call: a struct of numbers is added to a number, a field of text refuses
    // it, a list field is added as a list. The tables measured one struct, of a whole number and
    // text, so they say nothing of the others.
    let struct_arithmetic = is_method(entry)
        && entry.family() == catalog::Family::Arithmetic
        && (receiver_class == Some(Representative::Struct)
            || chosen
                .arguments
                .iter()
                .any(|(_, class)| *class == Representative::Struct));
    // A struct met by a struct whose fields differ is cast field by field, and Polars refuses or
    // not by what the fields are, which the tables, measured on one struct, do not say.
    let meets_other_fields = is_method(entry)
        && receiver_class == Some(Representative::Struct)
        && chosen.fields.is_some_and(|receiver| {
            chosen
                .argument_fields
                .iter()
                .any(|(_, argument)| *argument != receiver)
        });
    // What Polars always refuses of it is refused, in its words.
    if struct_arithmetic
        && let Some(said) = fields::arithmetic_refused(entry, receiver_class, chosen)
    {
        return Err(Refusal::rule_because(
            Rule::StructArithmeticNeedsNumbers,
            &said,
        ));
    }
    // A pattern Polars cannot compile is refused in Polars' words, whatever follows it.
    for (_, text) in chosen.patterns {
        if let Err(said) = catalog::check_pattern(text) {
            return Err(Refusal::rule_because(Rule::PatternIsValid, &said));
        }
    }
    // A duration Polars does not read is refused in Polars' words, whatever follows it.
    let unit = chosen.unit.unwrap_or(TimeUnit::Microseconds);
    let moment = match place.after {
        After::Value {
            dtype: Representative::Date,
            ..
        } => DataType::Date,
        After::Value {
            dtype: Representative::DatetimeZoned,
            ..
        } => DataType::Datetime(
            unit,
            Some(
                chosen
                    .zone
                    .and_then(|name| catalog::check_time_zone(name).ok().flatten())
                    .unwrap_or(TimeZone::UTC),
            ),
        ),
        After::Value { .. } => DataType::Datetime(unit, None),
        _ => catalog::any_moment(),
    };
    for (_, text, using) in chosen.durations {
        if let Err(said) = catalog::check_duration(text, *using, &moment) {
            return Err(Refusal::rule_because(Rule::DurationIsValid, &said));
        }
    }
    // A time zone Polars does not read is refused in Polars' words, whatever follows it.
    for (_, zone) in chosen.time_zones {
        if let Some(text) = zone
            && let Err(said) = catalog::check_time_zone(text)
        {
            return Err(Refusal::rule_because(Rule::TimeZoneIsValid, &said));
        }
    }
    // Moments in two zones are no operands of one another, whatever follows.
    if let Some((first, second)) = chosen.zone_clash {
        return Err(Refusal::rule_because(
            Rule::MomentsShareAZone,
            &format!("{first} and {second}"),
        ));
    }
    // What the options set are refused for, on the dtype of the value the method follows.
    if is_method(entry) && !struct_arithmetic && !meets_other_fields {
        let receiver = match place.after {
            After::Value { dtype, .. } => Some(dtype),
            _ => None,
        };
        // An argument given is refused before an option is: the line of the call that has it says
        // what Polars does of the call, where the option is measured with none.
        if let Some(dtype) = receiver
            && !chosen.arguments.is_empty()
        {
            on_receiver(entry, Some(dtype), chosen.arguments, chosen.options)?;
        }
        check_options(entry, receiver, chosen.options)?;
        check_conversions(entry, chosen)?;
    }
    // What the tables say of a function that starts an expression and takes expressions: it has no
    // receiver, so each argument is judged by its own line, and its options by the lines of the
    // dtype of its first expression. An option decides what the first accepts where one is set, as
    // a count of days is read of a date that a count of seconds refuses, so the line of the dtype
    // table, measured with the options at their defaults, is not asked of that argument then.
    let mut function_gives: Option<(Option<ResultDtype>, Shape)> = None;
    if let Some(subject) = subject_of(entry) {
        let subject_class = chosen
            .arguments
            .iter()
            .find(|(name, _)| *name == subject)
            .map(|(_, class)| *class);
        let lines_of: Vec<(&str, Representative)> = chosen
            .arguments
            .iter()
            .filter(|(name, _)| chosen.options.is_empty() || *name != subject)
            .copied()
            .collect();
        let fit = on_receiver(entry, None, &lines_of, chosen.options)?;
        match subject_class {
            Some(class) => check_options(entry, Some(class), chosen.options)?,
            None => options(entry, chosen.options)?,
        }
        let (mut result, mut shape) = (fit.result.clone(), fit.shape);
        match read_options(entry, subject_class, chosen.options) {
            OptionsGive::Nothing => {}
            OptionsGive::Unknown => result = None,
            OptionsGive::Gives(produces) => {
                result = Some(produces.result.clone());
                shape = produces.shape;
            }
        }
        // A moment is in the zone it is given, and in none where it is given none.
        if entry
            .parameters()
            .iter()
            .any(|parameter| parameter.kind() == Kind::TimeZone)
            && result
                .as_ref()
                .is_some_and(|result| result.name().starts_with("Datetime("))
        {
            let unit = unit_name(chosen.unit.unwrap_or(TimeUnit::Microseconds));
            let zone = chosen
                .time_zones
                .iter()
                .find_map(|(_, zone)| *zone)
                .and_then(|text| catalog::check_time_zone(text).ok().flatten());
            result = Some(ResultDtype::from_name(&match zone {
                Some(zone) => format!("Datetime({unit}, {zone})"),
                None => format!("Datetime({unit})"),
            }));
        }
        function_gives = Some((result, shape));
    }
    // What the dtype table says of a method on the value it follows.
    let (mut result, mut shape) = function_gives.unwrap_or((None, Shape::Many));
    if let (
        After::Value {
            dtype,
            shape: receiver_shape,
        },
        true,
    ) = (
        place.after,
        is_method(entry) && !struct_arithmetic && !meets_other_fields,
    ) {
        let fit = on_receiver(entry, Some(dtype), chosen.arguments, chosen.options)?;
        (result, shape) = (fit.result.clone(), fit.shape);
        // And of the options chosen, which the option table says what they give
        // on the dtype of the value the method follows.
        match read_options(entry, Some(dtype), chosen.options) {
            OptionsGive::Nothing => {}
            OptionsGive::Unknown => result = None,
            OptionsGive::Gives(produces) => {
                result = Some(produces.result.clone());
                // The option table was measured on a column. What follows one
                // value is another matter: a window that joins gives that value
                // again, not a list of it. Where the option changes what the
                // method gives, it is not known after one value.
                if receiver_shape == Shape::One && result != fit.result {
                    result = None;
                }
                shape = produces.shape;
            }
        }
        // The dtype that values are converted to is what the call gives.
        for parameter in entry.parameters() {
            if let Some((_, word)) = chosen
                .options
                .iter()
                .find(|(name, _)| !parameter.converts().is_empty() && *name == parameter.name())
            {
                result = Some(ResultDtype::from_name(word));
            }
        }
        // A call that is given the dtype it gives, as a cast is, gives nothing until it is told
        // one: the dtype table measured it converting to the dtype it follows.
        let needs_a_dtype = entry.parameters().iter().any(|parameter| {
            parameter.kind() == Kind::Dtype
                && parameter.default() == catalog::ParameterDefault::Required
                && !chosen
                    .options
                    .iter()
                    .any(|(name, _)| *name == parameter.name())
        });
        if needs_a_dtype {
            result = None;
        }
        // A call that is given a time zone gives a moment in it, and one given none a moment of no
        // zone, where the tables, measured with the zone UTC, say only that a moment is given: it
        // gives nothing until it is told the zone, where a call may be given none.
        if let Some(parameter) = entry
            .parameters()
            .iter()
            .find(|parameter| parameter.kind() == Kind::TimeZone)
            && result
                .as_ref()
                .is_some_and(|result| result.name().starts_with("Datetime("))
        {
            let unit = unit_name(chosen.unit.unwrap_or(TimeUnit::Microseconds));
            // Polars reads the empty text as no zone: a moment given it has none where the call
            // takes a zone off, and the zone UTC where the call converts, which takes none.
            let zoned = |zone: String| ResultDtype::from_name(&format!("Datetime({unit}, {zone})"));
            let unzoned = || ResultDtype::from_name(&format!("Datetime({unit})"));
            result = match chosen
                .time_zones
                .iter()
                .find(|(name, _)| *name == parameter.name())
            {
                Some((_, Some(text))) => match catalog::check_time_zone(text).ok() {
                    Some(Some(zone)) => Some(zoned(zone.to_string())),
                    Some(None) if parameter.takes_no_time_zone() => Some(unzoned()),
                    Some(None) => Some(zoned(TimeZone::UTC.to_string())),
                    None => Some(unzoned()),
                },
                Some((_, None)) => Some(unzoned()),
                None if parameter.takes_no_time_zone() => None,
                None => result,
            };
        } else if let Some(known) = chosen.unit
            && known != TimeUnit::Microseconds
            && entry.family() == catalog::Family::Temporal
            && !entry
                .parameters()
                .iter()
                .any(|parameter| parameter.name() == "time_unit")
            && let Some(kept) = result
                .as_ref()
                .filter(|result| {
                    matches!(
                        result.class(),
                        Some(Representative::Datetime | Representative::DatetimeZoned)
                    )
                })
                .and_then(|result| result.name().strip_prefix("Datetime(us"))
        {
            // A method of the `dt` namespace keeps the unit of the moment it follows, which the
            // tables, measured on microseconds, name as they were measured.
            result = Some(ResultDtype::from_name(&format!(
                "Datetime({}{kept}",
                unit_name(known)
            )));
        }
        // A call that was not given what Polars needs to run it gives nothing
        // whatever its options: Polars refuses a window given neither key.
        let measured = measured_with(entry);
        if !measured.is_empty()
            && !measured
                .iter()
                .any(|parameter| chosen.arguments.iter().any(|(name, _)| name == parameter))
        {
            result = None;
        }
    }
    // What follows a struct by arithmetic keeps its shape and has no dtype the tables say.
    if (struct_arithmetic || meets_other_fields)
        && let After::Value { shape: kept, .. } = place.after
    {
        shape = kept;
    }
    // What the call says of what it gives, where the tables do not.
    if result.is_none() && !is_method(entry) {
        result = chosen
            .gives
            .map(|dtype| ResultDtype::from_name(dtype.name()));
    }
    // What the context table says of the entry where it stands.
    // The entry stands there when the chain may go on from it: what Polars
    // refuses of an entry that is continued is refused whatever follows.
    let mut ends = Outcome::Allowed(None);
    // An option given in place of a parameter makes the method another form, which the table
    // measured apart where it differs: a fill by a strategy is not a fill by a value.
    let form = form_of(entry, chosen);
    // A struct of one field that is unnested is the field read, where the tables measured an unnest
    // of a struct of two.
    let measured_as = if id == "Expr.struct.unnest" && matches!(struct_gives, Gives::Field(_)) {
        "Expr.struct.field"
    } else {
        id
    };
    // The columns an unnest writes are the keys of a join as many as the other side's keys are, which
    // the tables measured as one, and are named by the fields, which the keys' names are asked of
    // by what the document says: where it is a join's key, they do not say.
    let unnested_in_key = id == "Expr.struct.unnest"
        && matches!(place.context.outer(), Outer::JoinLeft | Outer::JoinRight);
    // Inside a value of the key, as of a condition, the tables are asked what they say of the place,
    // and what they allow is not told, for the key's names are no better known there.
    let unnested_key = unnested_in_key && place.context.nested().is_none();
    // An unnest of a struct that has no field gives no expression, which the tables, measured on a
    // struct of two fields, do not say of any place.
    let unnested_nothing = id == "Expr.struct.unnest"
        && struct_gives
            .columns()
            .is_some_and(|columns| columns.is_empty());
    // Beside another predicate of a filter an unnest is taken, which the tables, measured with it as
    // the only one, refuse.
    let unnested_among_predicates = id == "Expr.struct.unnest"
        && place.context.outer() == Outer::Filter
        && place.context.nested().is_none()
        && chosen.among_others > 0;
    // Each field is a predicate then, which must be a flag.
    if unnested_among_predicates
        && matches!(
            place.after,
            After::Value {
                shape: Shape::Many,
                ..
            }
        )
        && let Some(field) = struct_gives.columns().and_then(|columns| {
            columns.iter().find(|field| {
                field
                    .class()
                    .is_some_and(|class| class != Representative::Boolean)
            })
        })
    {
        return Err(Refusal::rule_because(
            Rule::FilterPredicateIsBoolean,
            &format!(
                "the field {:?} is a predicate of the filter and is {}, not Boolean",
                field.name(),
                field.shown()
            ),
        ));
    }
    let skipped = unnested_key || unnested_nothing || unnested_among_predicates;
    let cell = tables
        .contexts
        .get(&(measured_as, form, place.context))
        .filter(|_| !skipped)
        .or_else(|| {
            tables
                .contexts
                .get(&(measured_as, Vec::new(), place.context))
                .filter(|_| !skipped)
        });
    if let Some(cell) = cell {
        match &cell.continues {
            Outcome::Refused(reason) => return Err(Refusal::polars(reason)),
            Outcome::Allowed(produces) => {
                if !is_method(entry)
                    && let Some(produces) = produces
                {
                    shape = produces.shape;
                }
            }
        }
        ends = cell.ends.clone();
    }
    // A method that follows one value gives one: the aggregate of a value is that value, and a
    // method that takes some of the values of one keeps it. One that follows a count that the
    // data or the arguments decide, and gives a value for each of them, gives that count.
    match place.after {
        After::Value {
            shape: Shape::One, ..
        } => shape = Shape::One,
        After::Value {
            shape: Shape::Counted,
            ..
        } if shape == Shape::Many && is_method(entry) => shape = Shape::Counted,
        _ => {}
    }
    // What the arguments given decide, which the tables, measured with columns, do not say.
    if let Some(given) = chosen.shape {
        shape = given;
    }
    // What a method that takes some of the values says of how it ends is what the table of counts
    // says of the count it gives, which it is not where what it follows is one value: the first of
    // one value is that value, which ends where one value does.
    if shape != Shape::Counted
        && matches!(
            catalog::Counting::of(entry, chosen.options),
            Some(counting) if !matches!(counting, catalog::Counting::TheArgument(_))
        )
    {
        ends = Outcome::Allowed(None);
    }
    // Whether it may also end the chain there: a rule first, so that its words
    // explain the refusal, then what Polars said of an entry that ends.
    let boolean_needed = Rule::boolean_needed(place.context);
    // A field that is read is a value of its own dtype, which the tables measured as a number: it
    // ends where a value of that dtype does, as a start of it does, which the rules judge.
    let read = matches!(struct_gives, Gives::Field(Some(_)))
        .then(|| struct_gives.result())
        .flatten()
        .and_then(|(read, _)| read);
    let judged = read.as_ref().or(result.as_ref());
    let mut ending = if let Some(rule) = Rule::forbids_end(place.context, judged, shape) {
        Ending::Refused(Refusal::rule(rule))
    } else if let Some(refusal) = Refusal::count(place.context, shape) {
        Ending::Refused(refusal)
    } else if let Some(read) = &read {
        if read.class().is_none() && boolean_needed {
            Ending::NotKnown(Unknown::TheResultsDtype)
        } else {
            Ending::Allowed
        }
    } else if entry.applies() == Applies::Function {
        // The table measured a start with one argument, and what its arguments
        // decide is the dtype of the value: a column's own, a literal's. Told
        // it, the rules above have judged it; not told, the table speaks only
        // where the dtype does not matter, and a refusal it recorded may be one
        // that another argument does not meet.
        if chosen.gives.is_some() {
            Ending::Allowed
        } else if boolean_needed || matches!(ends, Outcome::Refused(_)) {
            Ending::NotKnown(Unknown::TheStartsDtype)
        } else {
            Ending::Allowed
        }
    } else if result.is_none() && boolean_needed {
        Ending::NotKnown(Unknown::TheResultsDtype)
    } else {
        match &ends {
            Outcome::Refused(reason) => Ending::Refused(Refusal::polars(reason)),
            Outcome::Allowed(_) => Ending::Allowed,
        }
    };
    // A struct is written with the fields the chain carries, and the call that reads, names or makes
    // fields gives what it says of them: not what the tables measured on a struct of two.
    let mut fields = None;
    match &struct_gives {
        Gives::Nothing => {
            if result.as_ref().and_then(ResultDtype::class) == Some(Representative::Struct) {
                let keeps = receiver_class == Some(Representative::Struct);
                let (written, held) = fields::kept(chosen.fields.filter(|_| keeps));
                result = Some(written);
                fields = held;
            }
        }
        decided => {
            (result, fields) = decided.result().unwrap_or((None, None));
        }
    }
    // Names read, dropped or given to a struct whose fields the chain does not know are refused or
    // not by what the fields are, which Polars says and the walk cannot: where the struct is the
    // values of a pivot's group, or comes of a call that did not say its fields.
    if struct_gives.is_undecided() {
        ending = Ending::NotKnown(Unknown::TheFieldsOfAStruct);
    }
    if unnested_in_key {
        ending = Ending::NotKnown(Unknown::AnUnnestAsAKey);
    }
    if unnested_nothing {
        ending = Ending::NotKnown(Unknown::AnUnnestOfNoField);
    }
    if id == "Expr.struct.unnest"
        && place.context.outer() == Outer::Filter
        && place.context.nested().is_none()
        && chosen.among_others > 0
        && !unnested_nothing
        && struct_gives
            .columns()
            .is_some_and(|columns| columns.len() >= 2)
    {
        ending = Ending::NotKnown(Unknown::AnUnnestAmongPredicates);
    }
    // Polars keeps a number typed without a dtype open in a struct that is a join's key, as `dyn
    // float` in its dtype, so the key does not match the other table's key that the number resolves
    // to, though both are one dtype: a key made of such a struct, or of a field read from one, is
    // not told.
    let in_a_key = matches!(place.context.outer(), Outer::JoinLeft | Outer::JoinRight)
        && place.context.nested().is_none();
    if in_a_key
        && ((id == "polars.struct" && chosen.inputs.iter().any(Input::is_loose))
            || chosen.fields.is_some_and(Fields::has_a_loose_field)
            || struct_gives.reads_a_loose_field())
    {
        ending = Ending::NotKnown(Unknown::AnOpenNumberInAKey);
    }
    // Several columns of a field read with several names are no dtype of one value, and the tables
    // measured one name: where the place is no item of a select or a sort key, they do not say.
    let columns = struct_gives.columns().cloned();
    if id == "Expr.struct.field"
        && columns.as_ref().is_some_and(|columns| columns.len() >= 2)
        && !matches!(place.context.outer(), Outer::Select | Outer::Sort)
    {
        ending = Ending::NotKnown(Unknown::SeveralColumnsOfAFieldRead);
    }
    // An array is written at the width it has, where the chain knows it.
    if let (Some(width), Some(found)) = (chosen.width, &result)
        && matches!(found.class(), Some(Representative::Array(_)))
    {
        result = Some(found.of_width(width));
    }
    let width = struct_gives.width();
    if struct_arithmetic {
        result = None;
        ending = Ending::NotKnown(Unknown::ArithmeticOnAStruct);
    } else if meets_other_fields {
        result = None;
        ending = Ending::NotKnown(Unknown::StructsOfOtherFieldsMeet);
    }
    Ok(Allowance {
        result,
        shape,
        ending,
        fields,
        columns,
        width,
    })
}

/// Whether a connected value, or a connected list of values, of the dtype
/// class `dtype` may start an expression at `place`.
///
/// The run makes a connected input a literal of its dtype, so the tables' lines
/// of `polars.lit` judge it, as they judge a literal given a dtype: where a
/// literal may stand and may end, so may the input. A value gives one value, and a list gives a
/// count of values of its own, [`Shape::Counted`]: the values of another table, as many as it
/// has, which are not the rows of the table the expression is read against. `shape` says which;
/// [`Shape::Many`] makes it as many values as a column has. A place that is not a start refuses it.
#[must_use]
pub fn offer_input(place: &Place, dtype: Representative, shape: Shape) -> Verdict {
    let Some(literal) = catalog::entry("polars.lit") else {
        return Verdict::Refused(Refusal::structure("the catalogue has no polars.lit"));
    };
    let chosen = Chosen::default().giving(dtype);
    offer(
        literal,
        place,
        &match shape {
            Shape::Many => chosen.giving_many(),
            Shape::Counted => chosen.giving_shape(Shape::Counted),
            Shape::One => chosen,
        },
    )
}

/// The refusal of the rule that arrays that meet and give an array, or that one is looked up in,
/// are of one width, which is known, where they are not.
///
/// # Errors
/// Always: the caller asks it where it found arrays of other widths or of a width it does not know.
pub fn rule_forbids_arrays_of_other_widths() -> Result<(), Refusal> {
    Err(Refusal::rule(Rule::ArraysMeetAtOneWidth))
}

/// The refusal of a rule that forbids the method `entry` to follow a value of the dtype
/// `receiver`, if one does, without the tables: Polars panics on it where the rows run, so a
/// document that holds it is refused before Polars is asked.
///
/// # Errors
/// The rule that forbids it, with what the element lacks.
pub fn rule_forbids_a_receiver(entry: &Entry, receiver: &DataType) -> Result<(), Refusal> {
    match Rule::forbids_receiver(entry.id().as_str(), receiver) {
        Some((rule, said)) => Err(Refusal::rule_because(rule, &said)),
        None => Ok(()),
    }
}

/// The refusal of a call that follows a value of the dtype `receiver`, where the tables are not
/// asked of the call another way: the dtype of the value was not known before Polars built it, as
/// where it follows a value of lists of lists, and the call is one that Polars panics on or that
/// the tables do not measure. A method that depends on the element dtype is refused after a
/// list, an array or a value of a dtype with no class that it is not measured on, and a method of
/// lists or arrays, or one that follows a list or an array, is refused after one where the tables
/// record that its schema and its rows differ or that Polars panicked, whatever the call is given.
///
/// # Errors
/// The rule or the recorded refusal, said as the tables would say it of the call.
pub fn refuse_a_receiver_not_judged(
    entry: &'static Entry,
    receiver: &DataType,
) -> Result<(), Refusal> {
    let id = entry.id().as_str();
    let depends_on_the_element = SENSITIVE.contains(&id) || is_of_nested_dtypes(id);
    let not_measured = || {
        Refusal::rule_because(
            Rule::ElementDtypeIsMeasured,
            &format!("{id} is not measured on {}", dtype_name(receiver)),
        )
    };
    let Some(class) = Representative::of(receiver) else {
        // A list or an array of what has no class: of lists, of arrays, of structs.
        return if depends_on_the_element
            && matches!(receiver, DataType::List(_) | DataType::Array(..))
        {
            Err(not_measured())
        } else {
            Ok(())
        };
    };
    if read_as(id, class).is_none() {
        return Err(not_measured());
    }
    Ok(())
}

/// The refusal of a call that a rule or the tables refuse of the dtype `receiver` given `arguments`,
/// the dtypes of the expressions it is given by the names of its parameters, where the walk lost the
/// dtypes. A rule refuses an argument for the dtypes it is given, as `is_in` is refused a collection
/// nested as the value it follows is, whether or not the dtypes have a class. The tables refuse a
/// call after a list or an array, or a method of lists or arrays: Polars plans such a call
/// nevertheless where it does not work it out, as where both branches of a condition share it, and
/// panics on the plan. The walk refuses the same call where it knows the dtypes, whatever it is
/// given; this is asked where it lost them. An argument of no class is left to
/// [`refuse_an_argument_not_judged`], and the options the call is given are not asked.
///
/// # Errors
/// The refusal of the rule or of the tables, as the walk would have given it.
pub fn refuse_a_call_the_tables_refuse_after_lists(
    entry: &'static Entry,
    receiver: &DataType,
    arguments: &[(&str, DataType)],
) -> Result<(), Refusal> {
    // What Polars deprecates, whatever the dtypes are: the nesting of a dtype with no class is known.
    for (name, argument) in arguments {
        if let Some((rule, said)) =
            Rule::forbids_argument(entry.id().as_str(), name, receiver, argument)
        {
            return Err(Refusal::rule_because(rule, &said));
        }
    }
    let Some(class) = Representative::of(receiver) else {
        return Ok(());
    };
    if !(is_of_nested_dtypes(entry.id().as_str()) || class.is_nested()) {
        return Ok(());
    }
    let classes: Vec<(&str, Representative)> = arguments
        .iter()
        .filter_map(|(name, dtype)| Some((*name, Representative::of(dtype)?)))
        .collect();
    match on_receiver(entry, Some(class), &classes, &[]) {
        Err(refusal)
            if matches!(
                refusal.source(),
                RefusalSource::Polars(_) | RefusalSource::Rule(_)
            ) =>
        {
            Err(refusal)
        }
        _ => Ok(()),
    }
}

/// The refusal of a method that depends on the element dtype of a list or an array, given an
/// expression of the dtype `argument` that the tables do not judge: a list or an array of a list,
/// an array or a struct has no class, and a class the method is not measured on is not judged.
/// Polars panics on some of what it is given, as it does of a receiver of such a dtype. It does not
/// matter whether the tables could follow the chain before the call: they say nothing of an
/// argument of no class either way.
///
/// # Errors
/// The rule `element-dtype-is-measured`, with the dtype the method is not measured on.
pub fn refuse_an_argument_not_judged(
    entry: &'static Entry,
    argument: &DataType,
) -> Result<(), Refusal> {
    let id = entry.id().as_str();
    if !SENSITIVE.contains(&id) {
        return Ok(());
    }
    let unjudged = match Representative::of(argument) {
        None => matches!(argument, DataType::List(_) | DataType::Array(..)),
        Some(class) => read_as(id, class).is_none(),
    };
    if unjudged {
        Err(Refusal::rule_because(
            Rule::ElementDtypeIsMeasured,
            &format!("{id} is not measured on {}", dtype_name(argument)),
        ))
    } else {
        Ok(())
    }
}

/// The refusal of a rule that forbids `entry` to start or continue a chain in
/// `context`, if one does, without the tables: the rules alone. Polars is left
/// to decide the rest, as it builds.
///
/// # Errors
///
/// The rule, as a refusal that says it in its words.
pub fn rule_forbids(entry: &Entry, context: Context) -> Result<(), Refusal> {
    Rule::forbids(entry.id().as_str(), context).map_or(Ok(()), |rule| Err(Refusal::rule(rule)))
}

/// The refusal of a key that the caller has already found is not elementwise, as
/// it holds an aggregate beside a column, or a column compared with a null,
/// where `context` holds one of a join's own keys, if the rule refuses it there.
/// Polars accepts such a key in a window, so the rule is silent there. What a
/// key holds is a matter of the whole expression, not of one entry, so the
/// caller finds it, and this does not.
///
/// # Errors
///
/// The rule, as a refusal that says it in its words.
pub fn rule_forbids_a_key_known_not_elementwise(context: Context) -> Result<(), Refusal> {
    Rule::forbids_non_elementwise_key(context).map_or(Ok(()), |rule| Err(Refusal::rule(rule)))
}

/// The refusal of a replacement that the caller has found is given values in other numbers than
/// the rule allows: `new` of many values beside an `old` of one, or a strict one's `default` of
/// many beside a value of one. How many values an expression gives is the walk's to say, so the
/// caller finds it, and this does not.
///
/// # Errors
///
/// The rule, as a refusal that says it in its words.
pub fn rule_forbids_replacement_values_of_other_numbers() -> Result<(), Refusal> {
    Err(Refusal::rule(Rule::ReplacementValuesMatchInNumber))
}

/// The refusal of a moment or a length of time made of numbers typed in, which the caller has found,
/// where `context` is a join's key or inside one, if the rule refuses it there. Polars makes a
/// literal of such a call and names it by an alias, which a key may not hold anywhere.
///
/// # Errors
///
/// The rule, as a refusal that says it in its words.
pub fn rule_forbids_a_folded_function_in_a_join_key(context: Context) -> Result<(), Refusal> {
    Rule::forbids_a_folded_function_in_a_key(context)
        .map_or(Ok(()), |rule| Err(Refusal::rule(rule)))
}

/// The refusal of a window given a value of one row that a part widens, which the caller has found.
///
/// # Errors
///
/// The rule, as a refusal that says it in its words.
pub fn rule_forbids_a_window_over_a_widened_value() -> Result<(), Refusal> {
    Err(Refusal::rule(Rule::WindowInputIsNotWidened))
}

/// The refusal of a parameter that Polars reads as one value and is given another, which the caller
/// has found: `given` says what it was given, as `the rows`, `a count of values` or `one value that
/// a method took of one value`.
///
/// # Errors
///
/// The rule, as a refusal that says it in its words and what the parameter was given.
pub fn rule_forbids_a_parameter_given_more_than_one_value(
    parameter: &str,
    given: &str,
) -> Result<(), Refusal> {
    Err(Refusal::rule_because(
        Rule::ParameterTakesOneValue,
        &format!("{parameter} was given {given}"),
    ))
}

/// The refusal of a range or a repeat that no arguments typed in make, which the caller has found:
/// `said` is why, in Polars' words where Polars says it.
///
/// # Errors
///
/// The rule, as a refusal that says it in its words and what the range is refused for.
pub fn rule_forbids_a_range_that_cannot_be_made(said: &str) -> Result<(), Refusal> {
    Err(Refusal::rule_because(Rule::RangeIsValid, &said))
}

/// The refusal of a window given a count of values that the data or the arguments decide, which the
/// caller has found: the window gives each row of its group the value it gives the group, so it
/// takes a value as long as the group, unless it joins the values of a group or explodes them.
///
/// # Errors
///
/// The rule, as a refusal that says it in its words.
pub fn rule_forbids_a_window_over_a_count() -> Result<(), Refusal> {
    Err(Refusal::rule(Rule::WindowInputIsNotCounted))
}

/// The refusal of a call that meets values of other counts than it takes, which the caller has
/// found: one value meets each of the others, and others are as many as one another.
///
/// # Errors
///
/// The rule, as a refusal that says it in its words.
pub fn rule_forbids_values_of_other_counts() -> Result<(), Refusal> {
    Err(Refusal::rule(Rule::ValuesMeetInCount))
}

impl Refusal {
    /// Whether the refusal is about how many values a chain gives: the table of counts refused it,
    /// or a rule of counts did.
    fn is_about_counts(&self) -> bool {
        match self.source {
            RefusalSource::Rule(rule) => matches!(
                rule,
                Rule::ValuesMeetInCount
                    | Rule::PivotAggregateGivesOneValue
                    | Rule::WindowInputIsNotCounted
            ),
            RefusalSource::Polars(_) => self.reason.starts_with(COUNT_WORDS),
            RefusalSource::Structure => false,
        }
    }
}

/// `refusal` of a chain that is or meets a connected list, as it is said of such a chain: where it
/// is about how many values the chain gives, the list is the values of another table, a count of
/// its own and not the rows of this one. A refusal of anything else, a dtype for one, is as it was.
#[must_use]
pub(crate) fn refusal_of_a_connected_list(refusal: &Refusal) -> Refusal {
    if !refusal.is_about_counts() {
        return refusal.clone();
    }
    Refusal {
        source: refusal.source,
        reason: format!(
            "a connected list is the values of another table, a count of its own and not this table's rows: {}",
            refusal.reason
        ),
    }
}

/// The refusal of a chain that gives a count of values that the data or the arguments decide, which
/// the caller has found, where `context` takes only one value or the table's rows: a chain of
/// another count may not end an expression there. Polars' own words follow the count's.
///
/// # Errors
///
/// The refusal, in the words of the table of counts.
pub fn refuse_a_count_that_may_not_end(context: Context) -> Result<(), Refusal> {
    Refusal::count(context, Shape::Counted).map_or(Ok(()), Err)
}

/// The refusal of a key that the caller has already found repeats a pair of
/// output names, where `context` holds one of a join's own keys, if the rule
/// refuses it there. Polars refuses a join that pairs the same two names twice:
/// a name repeated on one side only is allowed. What the keys of the two sides
/// are named, and whether a pair comes twice, is a matter of the whole join, not
/// of one entry, so the caller finds it, and this does not.
///
/// # Errors
///
/// The rule, as a refusal that says it in its words.
pub fn rule_forbids_a_repeated_pair_of_key_names(context: Context) -> Result<(), Refusal> {
    Rule::forbids_repeated_key_names(context).map_or(Ok(()), |rule| Err(Refusal::rule(rule)))
}

/// The refusal of the rule that the columns that the items of a select, or of the columns a step
/// adds, write are named differently, where two of them write the name `name`. What each item
/// writes, an unnest and a field read writing the names of fields, is a matter of the whole step,
/// which the caller finds.
///
/// # Errors
///
/// Always: the caller asks it where it found a name written twice.
pub fn rule_forbids_a_name_written_twice(name: &str) -> Result<(), Refusal> {
    Err(Refusal::rule_because(
        Rule::WrittenNamesAreDistinct,
        &format!("two items write the name {name:?}"),
    ))
}

/// Every entry that stands in an expression, offered or refused at `place`.
/// A step is not offered: steps stand in a list.
///
/// Where an entry takes an expression it needs, it is offered when some dtype
/// of argument fits; [`offer`] answers for the arguments chosen.
#[must_use]
pub fn offers(place: &Place) -> Vec<Offer> {
    catalog::entries()
        .iter()
        .filter(|entry| entry.applies() != Applies::Frame)
        .map(|entry| Offer {
            entry,
            verdict: offer(entry, place, &Chosen::default()),
        })
        .collect()
}

/// The dtypes an argument for `parameter` of the method `entry` may have, when
/// the method follows a value of dtype `receiver`, and what the table says of
/// each.
#[must_use]
pub fn argument_dtypes(
    entry: &Entry,
    parameter: &str,
    receiver: Representative,
) -> Vec<(Representative, Outcome)> {
    let tables = Tables::committed();
    let id = entry.id().as_str();
    let Some(parameter) = entry
        .parameters()
        .iter()
        .map(catalog::Parameter::name)
        .find(|name| *name == parameter)
    else {
        return Vec::new();
    };
    Representative::ALL
        .into_iter()
        .filter_map(|argument| {
            match tables.dtype(id, Some(parameter), Some(receiver), Some(argument)) {
                Looked::Line(outcome) => Some((argument, outcome)),
                Looked::NotJudged | Looked::Missing => None,
            }
        })
        .collect()
}

/// Whether the options `chosen` of the entry `entry`, each by the parameter's
/// name and the value as the tables write it (`true`, `false`, `unset`, or the
/// word of a choice), may be set together: the table is read for every pair
/// of them, or for the one when only one is chosen. A method is measured on
/// each dtype of the value it follows, which this does not know: the options
/// are refused when Polars refuses them on every one of them, and allowed
/// when it allows them on any. A call at a place is asked on its own dtype.
///
/// # Errors
///
/// The first pair, or single option, Polars refused, with its reason.
pub fn options(entry: &Entry, chosen: &[(&str, &str)]) -> Result<(), Refusal> {
    if !has_a_subject(entry) {
        return check_options(entry, None, chosen);
    }
    let mut refusal = None;
    for receiver in Representative::ALL {
        match check_options(entry, Some(receiver), chosen) {
            Ok(()) => return Ok(()),
            Err(error) => {
                refusal.get_or_insert(error);
            }
        }
    }
    refusal.map_or(Ok(()), Err)
}

/// Whether the values that a method converts to the dtype it was given may be, as a cast of each
/// of them: the dtype table says what a cast does to each dtype, and the arguments chosen say what
/// dtype each value has.
fn check_conversions(entry: &Entry, chosen: &Chosen<'_>) -> Result<(), Refusal> {
    let tables = Tables::committed();
    for parameter in entry.parameters() {
        let Some((_, word)) = chosen
            .options
            .iter()
            .find(|(name, _)| !parameter.converts().is_empty() && *name == parameter.name())
        else {
            continue;
        };
        // Values of two dtypes meet in a supertype before they are converted, which the tables
        // do not give, so what is converted is judged only where they have one dtype.
        let classes: Vec<Representative> = parameter
            .converts()
            .iter()
            .filter_map(|converted| {
                chosen
                    .arguments
                    .iter()
                    .find(|(name, _)| name == converted)
                    .map(|(_, class)| *class)
            })
            .collect();
        if classes.windows(2).any(|pair| pair[0] != pair[1]) {
            continue;
        }
        // Beside the parameter that makes the conversion lenient, a value that does not convert
        // is a null, as a cast that is not strict gives one.
        let lenient = parameter.lenient_beside().is_some_and(|other| {
            chosen.arguments.iter().any(|(name, _)| *name == other)
                || chosen.unclassed.contains(&other)
        });
        for class in &classes {
            let mut options = vec![("dtype", (*word).to_owned())];
            if lenient {
                options.push(("strict", "false".to_owned()));
            }
            let Some(read_class) = read_as("Expr.cast", *class) else {
                continue;
            };
            let key = ("Expr.cast", Some(read_class), options);
            if let Some(Outcome::Refused(reason)) = tables.options.get(&key) {
                return Err(Refusal::polars(reason));
            }
        }
    }
    Ok(())
}

/// Whether the option `name` of `entry` may be set to `word`, where the other options are at their
/// defaults, as the lines of the option table that pair it with an option at its default say.
/// `Ok` where no line does.
fn alone_in_a_pair(
    entry: &Entry,
    receiver: Option<Representative>,
    name: &str,
    word: &str,
) -> Result<(), Refusal> {
    let tables = Tables::committed();
    let id = entry.id().as_str();
    let Some(receiver) = read_as_of(id, receiver) else {
        return Ok(());
    };
    let at_default = |(other, set): &(&str, String)| {
        entry
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == *other)
            .is_some_and(|parameter| default_word(parameter).as_deref() == Some(set.as_str()))
    };
    let mut lines: Vec<(&Vec<OptionValue>, &Outcome)> = Vec::new();
    for ((line_id, line_receiver, line), outcome) in &tables.options {
        if *line_id != id
            || *line_receiver != receiver
            || line.len() != 2
            || !line
                .iter()
                .any(|(set, value)| *set == name && value == word)
        {
            continue;
        }
        let others: Vec<&(&str, String)> = line
            .iter()
            .filter(|(set, value)| !(*set == name && value == word))
            .collect();
        if others.iter().all(|other| at_default(other)) {
            lines.push((line, outcome));
        }
    }
    first_refusal(lines)
}

/// The refusal of the first of `lines`, in the order of what they set, if any is refused: the
/// lines come from a map that has no order, so the reason is chosen by the options, and a refusal
/// is preferred where the lines disagree, for Polars refusing the option beside one of its pairs
/// is refusing it.
fn first_refusal(mut lines: Vec<(&Vec<OptionValue>, &Outcome)>) -> Result<(), Refusal> {
    lines.sort_by_key(|(first, _)| *first);
    lines
        .into_iter()
        .find_map(|(_, outcome)| match outcome {
            Outcome::Refused(reason) => Some(Refusal::polars(reason)),
            Outcome::Allowed(_) => None,
        })
        .map_or(Ok(()), Err)
}

/// Whether the options `chosen` of `entry` may be set together where a method follows a value
/// of the dtype `receiver`, or for a step, which follows none.
fn check_options(
    entry: &Entry,
    receiver: Option<Representative>,
    chosen: &[(&str, &str)],
) -> Result<(), Refusal> {
    let tables = Tables::committed();
    let id = entry.id().as_str();
    let Some(receiver) = read_as_of(id, receiver) else {
        return Ok(());
    };
    let named: Vec<(&'static str, String)> = {
        let mut named = Vec::with_capacity(chosen.len());
        for (name, value) in chosen {
            let name = entry
                .parameters()
                .iter()
                .map(catalog::Parameter::name)
                .find(|parameter| parameter == name)
                .ok_or_else(|| Refusal::structure(format!("{id} has no option {name}")))?;
            named.push((name, (*value).to_owned()));
        }
        // The tables list the options of an entry in the order of its parameters.
        let position = |name: &str| {
            entry
                .parameters()
                .iter()
                .position(|parameter| parameter.name() == name)
        };
        named.sort_by_key(|(name, _)| position(name));
        named
    };
    let check =
        |key: Vec<(&'static str, String)>| match tables.options.get(&(id, receiver, key.clone())) {
            Some(Outcome::Refused(reason)) => Err(Refusal::polars(reason)),
            Some(Outcome::Allowed(_)) => Ok(()),
            // A method with several options has no line for one of them alone. It is set to its
            // word beside every other option at its default, so the lines that say that are
            // asked: Polars refusing the pair is refusing the option. A step is measured with the
            // arguments the harness gives it, which a pair of its options is refused for as well,
            // as a cross join is given keys, so its lines say nothing of one option alone.
            None => match key.as_slice() {
                [(name, word)] if has_a_subject(entry) => {
                    alone_in_a_pair(entry, receiver, name, word)
                }
                _ => Ok(()),
            },
        };
    match named.as_slice() {
        [] => Ok(()),
        [one] => check(vec![one.clone()]),
        many => {
            // An option that a call needs, as a cast needs its dtype, has a line of its own, which
            // options at their defaults beside it do not leave unasked. Another option that is not
            // at its default changes what it means, and its pair is asked.
            let changed: Vec<&(&'static str, String)> = many
                .iter()
                .filter(|(name, word)| {
                    entry
                        .parameters()
                        .iter()
                        .find(|parameter| parameter.name() == *name)
                        .is_some_and(|parameter| {
                            default_word(parameter).as_deref() != Some(word.as_str())
                        })
                })
                .collect();
            if let [one] = changed.as_slice() {
                check(vec![(*one).clone()])?;
            }
            for (at, first) in many.iter().enumerate() {
                for second in &many[at + 1..] {
                    check(vec![first.clone(), second.clone()])?;
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(text: &str) -> Outcome {
        Outcome::Refused(Reason {
            stage: Stage::Schema,
            text: text.to_owned(),
        })
    }

    #[test]
    fn a_refusal_is_preferred_where_the_lines_that_pair_an_option_disagree() {
        let (first, second) = (
            vec![("a", "false".to_owned()), ("z", "true".to_owned())],
            vec![("b", "false".to_owned()), ("z", "true".to_owned())],
        );
        let allowed = Outcome::Allowed(None);
        let no = refused("no");
        // However the lines come, the refusal is found, and it is the same.
        for lines in [
            vec![(&first, &allowed), (&second, &no)],
            vec![(&second, &no), (&first, &allowed)],
        ] {
            let found = first_refusal(lines).expect_err("a refusal");
            assert_eq!(found.reason(), "no");
        }
        assert!(first_refusal(vec![(&first, &allowed), (&second, &allowed)]).is_ok());
        assert!(first_refusal(Vec::new()).is_ok());
    }

    #[test]
    fn the_refusal_of_two_lines_is_the_one_whose_options_come_first_whatever_the_order_given() {
        let (first, second) = (
            vec![("a", "false".to_owned()), ("z", "true".to_owned())],
            vec![("b", "false".to_owned()), ("z", "true".to_owned())],
        );
        let (one, other) = (refused("one"), refused("other"));
        for lines in [
            vec![(&first, &one), (&second, &other)],
            vec![(&second, &other), (&first, &one)],
        ] {
            assert_eq!(first_refusal(lines).expect_err("a refusal").reason(), "one");
        }
    }

    fn entry(id: &str) -> &'static Entry {
        catalog::entry(id).unwrap_or_else(|| panic!("no entry {id}"))
    }

    #[test]
    fn a_method_with_one_option_other_than_a_split_is_judged_by_its_own_lines() {
        // `dt.total_days(fractional)` has the one option, so the table has a line for each word.
        for word in ["true", "false"] {
            assert!(options(entry("Expr.dt.total_days"), &[("fractional", word)]).is_ok());
        }
    }

    #[test]
    fn a_method_with_several_options_is_judged_the_same_every_time_by_the_pairs_of_each() {
        // `over` has several options and no line for one alone, and Polars refuses none of its
        // words: each is allowed, as often as it is asked. What the pairs of a word say is
        // chosen by the options, not by the order of a map: see the tests of `first_refusal`.
        for word in ["group_to_rows", "join", "explode"] {
            for _ in 0..3 {
                assert!(options(entry("Expr.over"), &[("mapping_strategy", word)]).is_ok());
            }
        }
        // A split has several options too, and Polars refuses neither word of `literal`.
        for word in ["true", "false"] {
            for _ in 0..3 {
                assert!(options(entry("Expr.str.split"), &[("literal", word)]).is_ok());
            }
        }
    }
}
