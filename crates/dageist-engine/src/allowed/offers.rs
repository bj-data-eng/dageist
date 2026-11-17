//! Which entries may stand at a place, answered from the committed tables
//! and the named rules.

use std::fmt;

use crate::catalog::{self, Applies, Entry, Kind, ParameterDefault};

use super::context::Context;
use super::dtype::{Representative, ResultDtype};
use super::rules::Rule;
use super::table::{Outcome, Produces, Reason, Shape, Stage, Tables, is_method};
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
    gives: Option<Representative>,
    many: bool,
}

impl<'a> Chosen<'a> {
    /// The dtypes of the arguments given to expression parameters, each by the
    /// parameter's name.
    #[must_use]
    pub const fn with_arguments(mut self, arguments: &'a [(&'a str, Representative)]) -> Self {
        self.arguments = arguments;
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
    pub const fn giving_many(mut self) -> Self {
        self.many = true;
        self
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
            parameter.default() == ParameterDefault::Required
                && matches!(parameter.kind(), Kind::Expression(_))
        })
        .map(catalog::Parameter::name)
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
    receiver: Representative,
    arguments: &[(&str, Representative)],
) -> Result<Fit, Refusal> {
    let tables = Tables::committed();
    let id = entry.id().as_str();
    let missing = || Refusal::structure(format!("the tables hold nothing for {id} on {receiver}"));
    let line = |parameter: Option<&'static str>, argument: Option<Representative>| match tables
        .dtypes
        .get(&(id, parameter, receiver, argument))
    {
        None => Err(missing()),
        Some(Outcome::Refused(reason)) => Err(Refusal::polars(reason)),
        Some(Outcome::Allowed(produces)) => Ok(Fit::of(produces.as_ref())),
    };
    let mut fit = None;
    let required: Vec<&'static str> = required_expressions(entry).collect();
    if required.is_empty() {
        fit = Some(line(None, None)?);
    }
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
        for argument in Representative::ALL {
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
    fit.ok_or_else(missing)
}

/// What the option table says the options chosen give.
enum OptionsGive {
    /// They change nothing the table records: no option is set to other than its
    /// default.
    Nothing,
    /// The lines that bear on them agree on this, measured on a receiver of this
    /// dtype.
    Gives {
        produces: Produces,
        measured_on: Option<Representative>,
    },
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
fn read_options(entry: &Entry, chosen: &[(&str, &str)]) -> OptionsGive {
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
    let mut gives: Vec<(&Produces, Option<Representative>)> = Vec::new();
    for ((line_id, line), cell) in &tables.options {
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
            && bears
            && let Outcome::Allowed(Some(produces)) = &cell.outcome
        {
            gives.push((produces, cell.receiver));
        }
    }
    let Some((first, measured_on)) = gives.first().copied() else {
        return OptionsGive::Nothing;
    };
    if gives.iter().all(|(produces, _)| *produces == first) {
        OptionsGive::Gives {
            produces: first.clone(),
            measured_on,
        }
    } else {
        OptionsGive::Unknown
    }
}

/// The expression parameters the option table was measured with, for an entry
/// whose options mean nothing without them: a window is grouped by
/// `partition_by`, and without it a mapping strategy changes nothing.
fn measured_with(entry: &Entry) -> &'static [&'static str] {
    match entry.id().as_str() {
        "Expr.over" => &["partition_by"],
        _ => &[],
    }
}

/// The result an option gives where the method follows `receiver`, from the
/// result `option` the table measured on `measured_on`, and the results the
/// method gives at its default options on each. The option table was measured
/// on one dtype: where the receiver is another, the option is read as it
/// changes what the method gives there, which is to leave it, or to make a
/// list of it, and not known where it does otherwise.
fn changed_result(
    receiver: Representative,
    measured_on: Option<Representative>,
    option: &ResultDtype,
    default_there: Option<&ResultDtype>,
    default_here: Option<&ResultDtype>,
) -> Option<ResultDtype> {
    if measured_on == Some(receiver) {
        return Some(option.clone());
    }
    let there = default_there?;
    if option == there {
        return default_here.cloned();
    }
    (option.name() == format!("List({})", there.name()))
        .then(|| default_here.map(|here| ResultDtype::from_name(&format!("List({})", here.name()))))
        .flatten()
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
    // What the options set are refused for.
    if is_method(entry) {
        options(entry, chosen.options)?;
    }
    // What the dtype table says of a method on the value it follows.
    let (mut result, mut shape) = (None, Shape::Many);
    if let (
        After::Value {
            dtype,
            shape: receiver_shape,
        },
        true,
    ) = (place.after, is_method(entry))
    {
        let fit = on_receiver(entry, dtype, chosen.arguments)?;
        (result, shape) = (fit.result.clone(), fit.shape);
        // And of the options chosen, which the option table says what they give
        // on the dtype it was measured on.
        match read_options(entry, chosen.options) {
            OptionsGive::Nothing => {}
            OptionsGive::Unknown => result = None,
            OptionsGive::Gives {
                produces,
                measured_on,
            } => {
                let there = measured_on.and_then(|measured| {
                    on_receiver(entry, measured, chosen.arguments)
                        .ok()
                        .and_then(|fit| fit.result)
                });
                result = changed_result(
                    dtype,
                    measured_on,
                    &produces.result,
                    there.as_ref(),
                    fit.result.as_ref(),
                );
                // The option table was measured on a column. What follows one
                // value is another matter: a window that joins gives that value
                // again, not a list of it. Where the option changes what the
                // method gives, it is not known after one value.
                if receiver_shape == Shape::One && result != fit.result {
                    result = None;
                }
                // And the call has to be given what the table was measured
                // with.
                if measured_with(entry)
                    .iter()
                    .any(|parameter| !chosen.arguments.iter().any(|(name, _)| name == parameter))
                {
                    result = None;
                }
                shape = produces.shape;
            }
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
    if let Some(cell) = tables.contexts.get(&(id, place.context)) {
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
    if chosen.many && !is_method(entry) {
        shape = Shape::Many;
    }
    if let After::Value {
        shape: Shape::One, ..
    } = place.after
    {
        shape = Shape::One;
    }
    // Whether it may also end the chain there: a rule first, so that its words
    // explain the refusal, then what Polars said of an entry that ends.
    let boolean_needed = Rule::boolean_needed(place.context);
    let ending = if let Some(rule) = Rule::forbids_end(place.context, result.as_ref(), shape) {
        Ending::Refused(Refusal::rule(rule))
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
            Shape::Many => chosen.giving_many(),
            Shape::One => chosen,
        },
    )
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
            tables
                .dtypes
                .get(&(id, Some(parameter), receiver, Some(argument)))
                .map(|outcome| (argument, outcome.clone()))
        })
        .collect()
}

/// Whether the options `chosen` of the entry `entry`, each by the parameter's
/// name and the value as the tables write it (`true`, `false`, `unset`, or the
/// word of a choice), may be set together: the table is read for every pair
/// of them, or for the one when only one is chosen.
///
/// # Errors
///
/// The first pair, or single option, Polars refused, with its reason.
pub fn options(entry: &Entry, chosen: &[(&str, &str)]) -> Result<(), Refusal> {
    let tables = Tables::committed();
    let id = entry.id().as_str();
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
    let check = |key: Vec<(&'static str, String)>| match tables.options.get(&(id, key)) {
        Some(cell) => match &cell.outcome {
            Outcome::Refused(reason) => Err(Refusal::polars(reason)),
            Outcome::Allowed(_) => Ok(()),
        },
        None => Ok(()),
    };
    match named.as_slice() {
        [] => Ok(()),
        [one] => check(vec![one.clone()]),
        many => {
            for (at, first) in many.iter().enumerate() {
                for second in &many[at + 1..] {
                    check(vec![first.clone(), second.clone()])?;
                }
            }
            Ok(())
        }
    }
}
