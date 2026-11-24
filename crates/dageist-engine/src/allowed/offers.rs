//! Which entries may stand at a place, answered from the committed tables
//! and the named rules.

use std::fmt;

use polars::prelude::{DataType, TimeUnit, TimeZone};

use crate::catalog::{self, Applies, DurationUse, Entry, Kind};

use super::adapt::could_be_taken_by_a_number;
use super::context::Context;
use super::dtype::{Representative, ResultDtype, dtype_name, unit_name};
use super::rules::Rule;
use super::table::{
    LIST_FORM, Looked, OptionValue, Outcome, Produces, Reason, SENSITIVE, Shape, Stage, Tables,
    has_a_subject, is_a_list, is_method, is_of_nested_dtypes, read_as, restored, subject_of,
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
    arguments: &'a [(&'a str, Representative)],
    options: &'a [(&'a str, &'a str)],
    unclassed: &'a [&'a str],
    patterns: &'a [(&'a str, &'a str)],
    durations: &'a [(&'a str, &'a str, DurationUse)],
    time_zones: &'a [(&'a str, Option<&'a str>)],
    unit: Option<TimeUnit>,
    width: Option<usize>,
    zone: Option<&'a str>,
    zone_clash: Option<(&'a str, &'a str)>,
    gives: Option<Representative>,
    shape: Option<Shape>,
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
            reason: format!(
                "a count of values that the data or the arguments decide, which is neither one \
                 value nor the table's rows, may not end an expression in {context}: {}",
                reason.text
            ),
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
}

impl fmt::Display for Unknown {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TheStartsDtype => "the dtype of the start is not given",
            Self::TheResultsDtype => "the dtype that the arguments decide is not known",
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
        let mut refusal = None;
        let mut fitting: Vec<Fit> = Vec::new();
        for argument in Representative::MEASURED {
            match line(Some(parameter), Some(argument)) {
                Ok(found) => fitting.push(found),
                Err(error) => {
                    refusal.get_or_insert(error);
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

/// The options chosen that are given in place of another parameter, and the list that `is_in` is
/// given, which a method with them stands in a context in the form of.
fn form_of(entry: &Entry, chosen: &Chosen<'_>) -> Vec<(&'static str, String)> {
    let mut form: Vec<(&'static str, String)> = entry
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
        .collect();
    if entry.id().as_str() == "Expr.is_in"
        && chosen
            .arguments
            .iter()
            .any(|(name, class)| *name == LIST_FORM.0 && is_a_list(*class))
    {
        form.push((LIST_FORM.0, LIST_FORM.1.to_owned()));
    }
    form
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
    if is_method(entry) {
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
    ) = (place.after, is_method(entry))
    {
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
        // A call whose pattern names the fields of what it gives, as `extract_groups` does, gives
        // nothing until it is told the pattern, and then the struct Polars works out of it: the
        // dtype table measured it with one pattern, whose groups are not the call's.
        if let Some(parameter) = entry
            .parameters()
            .iter()
            .find(|parameter| parameter.names_the_fields_of_the_result())
        {
            result = chosen
                .patterns
                .iter()
                .find(|(name, _)| *name == parameter.name())
                .and_then(|(_, text)| catalog::groups_dtype(text).ok())
                .map(|dtype| ResultDtype::from_name(&dtype_name(&dtype)));
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
    let cell = tables
        .contexts
        .get(&(id, form, place.context))
        .or_else(|| tables.contexts.get(&(id, Vec::new(), place.context)));
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
    let ending = if let Some(rule) = Rule::forbids_end(place.context, result.as_ref(), shape) {
        Ending::Refused(Refusal::rule(rule))
    } else if let Some(refusal) = Refusal::count(place.context, shape) {
        Ending::Refused(refusal)
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
    // An array is written at the width it has, where the chain knows it.
    if let (Some(width), Some(found)) = (chosen.width, &result)
        && matches!(found.class(), Some(Representative::Array(_)))
    {
        result = Some(found.of_width(width));
    }
    Ok(Allowance {
        result,
        shape,
        ending,
    })
}

/// Whether a connected value, or a connected list of values, of the dtype
/// class `dtype` may start an expression at `place`.
///
/// The run makes a connected input a literal of its dtype, so the tables' lines
/// of `polars.lit` judge it, as they judge a literal given a dtype: where a
/// literal may stand and may end, so may the input. A list gives many values,
/// where a value gives one, which `shape` says. A place that is not a start refuses it.
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
            Shape::Many | Shape::Counted => chosen.giving_many(),
            Shape::One => chosen,
        },
    )
}

/// The refusal of the rule that arrays that meet and give an array are of one width, which is
/// known, where they are not.
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
    if (is_of_nested_dtypes(id) || class.is_nested())
        && let Some(reason) = Tables::committed().recorded_danger(id, class)
    {
        return Err(Refusal::polars(&reason));
    }
    Ok(())
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
