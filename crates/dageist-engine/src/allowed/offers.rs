//! Which entries may stand at a place, answered from the committed tables
//! and the named rules.

use std::fmt;

use crate::catalog::{self, Applies, Entry, Kind, ParameterDefault};

use super::context::Context;
use super::dtype::{Representative, ResultDtype};
use super::rules::Rule;
use super::table::{Outcome, Produces, Reason, Shape, Stage, Tables, is_method};

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
    fn structure(reason: impl Into<String>) -> Self {
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

/// What an entry that may stand at a place gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowance {
    result: Option<ResultDtype>,
    shape: Shape,
    cannot_end: Option<Refusal>,
}

impl Allowance {
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
    /// # Errors
    ///
    /// The rule that the expression, ended here, would break, or else the
    /// reason Polars refused the entry ending a chain in this context, as the
    /// table of contexts recorded it.
    pub fn can_end(&self) -> Result<(), &Refusal> {
        self.cannot_end.as_ref().map_or(Ok(()), Err)
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
/// given: what it gives, or the first refusal. A parameter that must be given
/// and is not needs only some dtype of argument that fits.
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
    for (name, dtype) in arguments {
        let parameter = entry
            .parameters()
            .iter()
            .map(catalog::Parameter::name)
            .find(|parameter| parameter == name)
            .ok_or_else(|| Refusal::structure(format!("{id} has no parameter {name}")))?;
        let given = line(Some(parameter), Some(*dtype))?;
        fit.get_or_insert(given);
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

/// Whether the entry `entry` may stand at `place`, given the dtypes of the
/// arguments that have been chosen for its expression parameters.
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
/// what [`Allowance::can_end`] reports.
///
/// The context table was measured at the receiver that best fits each entry,
/// so where the context cares about the receiver, as a predicate cares that it
/// is `Boolean`, the rules of how a chain ends carry what the table cannot. A
/// result whose dtype is not known, which the argument will decide, does not
/// break a rule that is about a dtype: it is not refused for what is not known.
#[must_use]
pub fn offer(
    entry: &'static Entry,
    place: &Place,
    arguments: &[(&str, Representative)],
) -> Verdict {
    match judge(entry, place, arguments) {
        Ok(allowance) => Verdict::Allowed(allowance),
        Err(refusal) => Verdict::Refused(refusal),
    }
}

fn judge(
    entry: &'static Entry,
    place: &Place,
    arguments: &[(&str, Representative)],
) -> Result<Allowance, Refusal> {
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
    // What the dtype table says of a method on the value it follows.
    let (mut result, mut shape) = (None, Shape::Many);
    if let (After::Value { dtype, .. }, true) = (place.after, is_method(entry)) {
        let fit = on_receiver(entry, dtype, arguments)?;
        (result, shape) = (fit.result, fit.shape);
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
    if let After::Value {
        shape: Shape::One, ..
    } = place.after
    {
        shape = Shape::One;
    }
    // Whether it may also end the chain there: a rule first, so that its words
    // explain the refusal, then what Polars said of an entry that ends.
    let cannot_end = match Rule::forbids_end(place.context, result.as_ref(), shape) {
        Some(rule) => Some(Refusal::rule(rule)),
        None => match &ends {
            Outcome::Refused(reason) => Some(Refusal::polars(reason)),
            Outcome::Allowed(_) => None,
        },
    };
    Ok(Allowance {
        result,
        shape,
        cannot_end,
    })
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
            verdict: offer(entry, place, &[]),
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
        Some(Outcome::Refused(reason)) => Err(Refusal::polars(reason)),
        _ => Ok(()),
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
