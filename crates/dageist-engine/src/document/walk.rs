//! What the tables of allowed combinations say of a step, without running
//! Polars: where each expression in it stands, what each call of it follows,
//! whether each call may stand there and whether the expression may end.
//!
//! The walk follows the document as running it does: the arguments of a call
//! first, each expression start by link, the way it is placed in the named
//! positions that hold it. Every call of an expression is asked of
//! [`allowed::offer`], at the place the chain has come to, with the dtypes of
//! the arguments chosen for its expression parameters, the options it is given,
//! and, for a start, the dtype of the column or literal it reads. Every
//! expression that a position holds is asked whether it may end there, and a
//! join's key whether it holds an aggregate beside a column, or a column compared
//! with a null. A join's keys are also asked whether they pair the same two output
//! names twice, which Polars refuses, and `on` is walked against both tables, as
//! Polars reads it. The options a step is given are asked of [`allowed::options`].
//!
//! The tables are measured, so what they cannot say is not guessed: a chain
//! whose dtype is not known where a method follows it is not asked from there
//! on, and the walk says it cannot tell.

use polars::prelude::{DataType, Schema, SchemaRef, TimeUnit};

use crate::allowed::{
    self, After, Chosen, ConditionState, Context, Ending, Literal, Nested, Outer,
    Place as OfferPlace, Position as Slot, Refusal, Representative, Shape, Unknown, Verdict,
};
use crate::allowed::{
    Loose, Receiver, adapted_dtype, adapts_numbers_only, broadcasts, could_be_taken_by_a_number,
    is_count, is_left_unresolved, is_refused_as_a_count, is_required, operand_adapts,
    receiver_beside,
};
use crate::catalog::{self, Applies, AsManyAs, Comparison, Entry, Kind, Role};

use super::error::{Place, Segment};
use super::inputs::Inputs;
use super::model::{Call, Expression, Start, Step, Value};

/// A call of an expression: what it follows, and whether it may stand there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    follows: Option<After>,
    verdict: Option<Verdict>,
}

impl Standing {
    const fn not_asked() -> Self {
        Self {
            follows: None,
            verdict: None,
        }
    }

    /// What the call follows: nothing for a start, otherwise the value or the
    /// condition before it. `None` where that is not known, because the dtype
    /// of what comes before it is not.
    #[must_use]
    pub const fn follows(&self) -> Option<After> {
        self.follows
    }

    /// Whether the call may stand there and what it gives, as [`allowed::offer`]
    /// answered. `None` where it was not asked. Where the dtype of an argument
    /// was not known it was asked with the arguments that were, so what it says
    /// the call gives is not what the chain has; the places say that.
    #[must_use]
    pub const fn verdict(&self) -> Option<&Verdict> {
        self.verdict.as_ref()
    }
}

/// An expression of a step, as the walk found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedExpression {
    place: Place,
    context: Option<Context>,
    calls: Vec<Standing>,
    next: Option<After>,
    ending: Option<Ending>,
}

impl WalkedExpression {
    /// Where the expression is: the step, then the way to the argument that
    /// holds it.
    #[must_use]
    pub const fn place(&self) -> &Place {
        &self.place
    }

    /// The context the expression stands in. `None` when no named position
    /// holds it.
    #[must_use]
    pub const fn context(&self) -> Option<Context> {
        self.context
    }

    /// The start of the expression, then each of its links.
    #[must_use]
    pub fn calls(&self) -> &[Standing] {
        &self.calls
    }

    /// The places the links stand in, in order, and after them the place a link
    /// added at the end would stand in. `None` where the context or what comes
    /// before is not known. They are what [`allowed::offers`] is asked at.
    #[must_use]
    pub fn places(&self) -> Vec<Option<OfferPlace>> {
        let follows = self
            .calls
            .iter()
            .skip(1)
            .map(Standing::follows)
            .chain(std::iter::once(self.next));
        follows
            .map(|after| Some(OfferPlace::new(self.context?, after?)))
            .collect()
    }

    /// Whether the expression may end where it does, when a position holds it
    /// and the walk could ask.
    #[must_use]
    pub const fn ending(&self) -> Option<&Ending> {
        self.ending.as_ref()
    }
}

/// A refusal the walk found, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkRefusal {
    place: Place,
    refusal: Refusal,
    at_end: bool,
    argument_unknown: bool,
}

impl WalkRefusal {
    /// Where, in the document.
    #[must_use]
    pub const fn place(&self) -> &Place {
        &self.place
    }

    /// Who refused it, and why.
    #[must_use]
    pub const fn refusal(&self) -> &Refusal {
        &self.refusal
    }

    /// Whether the expression there may not end where it does, and not that a
    /// call may not stand.
    #[must_use]
    pub const fn at_end(&self) -> bool {
        self.at_end
    }

    /// Whether the call was refused without the dtype of an argument being known:
    /// no dtype that argument might have lets the call stand, and the reason is
    /// the one Polars gave for one of them, not for the dtype the argument has.
    #[must_use]
    pub const fn argument_unknown(&self) -> bool {
        self.argument_unknown
    }
}

/// What the tables say of a step: its expressions, and what is refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Walk {
    expressions: Vec<WalkedExpression>,
    refusal: Option<WalkRefusal>,
    not_known: Option<String>,
}

impl Walk {
    /// The expressions of the step, those inside another expression's
    /// arguments first.
    #[must_use]
    pub fn expressions(&self) -> &[WalkedExpression] {
        &self.expressions
    }

    /// The first thing refused: a call that may not stand, an expression that
    /// may not end, or an option Polars refuses.
    #[must_use]
    pub const fn refusal(&self) -> Option<&WalkRefusal> {
        self.refusal.as_ref()
    }

    /// Why the tables could not be asked past a point, where they could not.
    #[must_use]
    pub fn not_known(&self) -> Option<&str> {
        self.not_known.as_deref()
    }
}

impl Step {
    /// What the tables of allowed combinations say of the step as the
    /// `index`th of a document, when it starts from a table with the schema
    /// `source`. `inputs` are what its `table` arguments name.
    #[must_use]
    pub fn walk(&self, index: usize, source: &Schema, inputs: &Inputs) -> Walk {
        crate::startup::ensure();
        let other = self
            .call()
            .argument("other")
            .and_then(|value| match value {
                Value::Table(name) => inputs.table(name),
                _ => None,
            })
            .and_then(|mut table| table.collect_schema().ok());
        let mut walker = Walker {
            zones: Vec::new(),
            step: self.call(),
            source,
            inputs,
            other,
            base: Place::of_step(index, Some(self.id().clone())),
            path: Vec::new(),
            slots: Vec::new(),
            walked: Vec::new(),
            refusal: None,
            not_known: None,
        };
        walker.arguments(self.call(), false);
        walker.step_options(self.call());
        walker.nan_test_columns();
        walker.key_names();
        Walk {
            expressions: walker.walked,
            refusal: walker.refusal,
            not_known: walker.not_known,
        }
    }
}

/// What an expression held by an argument has: the class it gives, if known,
/// and whether it gives one value.
#[derive(Clone, Copy)]
struct Operand {
    dtype: Option<Representative>,
    /// Whether it is a number typed without a dtype and alone, which adapts to
    /// the operand it is used with.
    loose: Option<Loose>,
    shape: Shape,
    /// Whether the expression was walked to its end: a refused or unwalked one
    /// has no shape either.
    known: bool,
    /// Whether how many values it gives can be counted on: not where a condition has one
    /// expression for all its values, whose predicate Polars drops where it optimizes the plan
    /// and not where a call that is no elementwise one follows it.
    counted: bool,
    /// The time zone it is a moment in, where the chain knows it.
    zone: Option<Zone>,
}

/// The time zone of a moment: none, or a name that the walk holds once, by its place in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Zone {
    /// A moment that has none.
    Naive,
    /// A moment in the zone the walker named at that place.
    Named(usize),
}

/// The class of one value of a condition, and whether it is a number typed
/// without a dtype, which adapts to the numbers beside it.
type Valued = (Option<Representative>, Option<Loose>);

/// What surrounds a link of an expression besides the chain before it.
#[derive(Clone, Copy)]
struct Around {
    /// Whether the values of the condition the link is part of are one expression.
    branches_are_one: bool,
    /// What the chain starts from, when that is a number typed without a dtype and the
    /// link follows it.
    receiver_loose: Option<Loose>,
    /// The unit the moment or the length of time the link follows is counted in, where it is known.
    unit: Option<TimeUnit>,
    /// Whether the value the link follows is one that a part widened: a value of one row that a method
    /// met with each of the many values of a part, which a window refuses.
    widened: bool,
    /// The zone of the moment the link follows, where it is known.
    zone: Option<Zone>,
}

/// What a chain has so far.
#[derive(Clone)]
enum Cur {
    Value {
        dtype: Option<Representative>,
        shape: Shape,
    },
    Condition {
        state: ConditionState,
        values: Vec<Valued>,
        /// The zone of each value, where it is a moment whose zone is known, in the order of `values`.
        zones: Vec<Option<Zone>>,
        /// Whether every predicate and value so far gives one value, which a
        /// condition of them gives too.
        single: bool,
        /// Whether every value so far gives one value: what the condition is
        /// where its values are one expression, and the predicates are dropped.
        values_single: bool,
        /// Whether every predicate and value so far was walked to its end: a
        /// refused one has no shape, so the condition has none.
        known: bool,
    },
    /// Refused: nothing more is asked of it.
    Dead,
}

struct Walker<'a> {
    step: &'a Call,
    source: &'a Schema,
    inputs: &'a Inputs,
    other: Option<SchemaRef>,
    base: Place,
    path: Vec<Segment>,
    slots: Vec<Slot>,
    walked: Vec<WalkedExpression>,
    refusal: Option<WalkRefusal>,
    not_known: Option<String>,
    /// The names of the zones the walk has met, which a [`Zone::Named`] points into.
    zones: Vec<String>,
}

/// The output names of a join's keys before the first whose name is not known, in
/// order. Polars expands each side's keys before it pairs them, so a key that may
/// expand to several columns, or that is not picked yet, shifts every pair after
/// it: only the keys before it pair by position. Such a key is one whose name is
/// not known, as is an item that is no expression.
fn known_key_names(value: &Value) -> Vec<String> {
    let names = |items: &[&Value]| {
        items
            .iter()
            .map_while(|item| match item {
                Value::Expression(expression) => key_name(expression),
                _ => None,
            })
            .collect()
    };
    match value {
        Value::Many(items) => names(&items.iter().collect::<Vec<_>>()),
        other => names(&[other]),
    }
}

/// Whether the parameter `parameter` of `step` holds a list of keys, whose items
/// are each a place.
fn left_is_many(step: &Call, parameter: &str) -> bool {
    matches!(step.argument(parameter), Some(Value::Many(_)))
}

/// The output name Polars gives a join's key: the column the expression reads
/// first, whatever methods follow it, `literal` for a literal, and for a
/// condition the name of the first value it gives. `None` where it is not known:
/// a start that expands to several columns or names them by a pattern, an input,
/// a hole. A key is never given a name by an alias, which Polars refuses in a
/// join's key and the catalogue does not hold.
fn key_name(key: &Expression) -> Option<String> {
    let Start::Call(start) = key.start() else {
        return None;
    };
    match start.entry().id().as_str() {
        "polars.col" => {
            if Walker::names_several(start) {
                return None;
            }
            let Some(Value::Columns(names)) = start.argument("name") else {
                return None;
            };
            let name = names.first()?;
            (!(name.starts_with('^') && name.ends_with('$'))).then(|| name.clone())
        }
        "polars.lit" => Some("literal".to_owned()),
        // Python names what these make by an alias.
        "polars.date" => Some("date".to_owned()),
        "polars.time" => Some("time".to_owned()),
        // A count since 1970 is named as the column it counts is.
        "polars.from_epoch" => match start.argument("column") {
            Some(Value::Expression(column)) => key_name(column),
            _ => None,
        },
        // A moment and a length of time are named as the first of the parts they are made of,
        // which is `literal` for one that is a number typed in, unless every part is one: Polars
        // then folds the call into a literal where it can, and names it `datetime` or `duration`,
        // or leaves it as a call where it cannot, which depends on the values.
        "polars.datetime" => {
            let zoned = matches!(start.argument("time_zone"), Some(Value::TimeZone(Some(_))));
            first_part_name(start, "year", &PARTS_OF_A_MOMENT, !zoned)
        }
        "polars.duration" => first_part_name(start, "weeks", &PARTS_OF_A_LENGTH, true),
        "polars.when" => key
            .links()
            .iter()
            .find(|link| link.entry().id().as_str() == "When.then")
            .and_then(|link| match link.argument("statement") {
                Some(Value::Expression(value)) => key_name(value),
                _ => None,
            }),
        _ => None,
    }
}

/// The unit a call is given for the parameter `parameter`, one of the three a time unit has, if it
/// is given one.
fn time_unit_word(call: &Call, parameter: &str) -> Option<String> {
    match call.argument(parameter) {
        Some(Value::Choice(word)) => Some(word.clone()),
        _ => None,
    }
}

/// The dtype a function that makes a date, a time, a moment or a length of time gives, where its
/// arguments decide it: the unit and the zone it is given, which Python reads as it builds it,
/// and the unit a count since 1970 is read in.
fn made_dtype(start: &Call) -> Option<DataType> {
    let unit = |word: &str| match word {
        "ns" => TimeUnit::Nanoseconds,
        "ms" => TimeUnit::Milliseconds,
        _ => TimeUnit::Microseconds,
    };
    match start.entry().id().as_str() {
        "polars.date" => Some(DataType::Date),
        "polars.time" => Some(DataType::Time),
        "polars.datetime" => {
            let unit = unit(&time_unit_word(start, "time_unit").unwrap_or_else(|| "us".to_owned()));
            let zone = match start.argument("time_zone") {
                Some(Value::TimeZone(Some(text))) => crate::catalog::check_time_zone(text).ok()?,
                _ => None,
            };
            Some(DataType::Datetime(unit, zone))
        }
        "polars.duration" => Some(DataType::Duration(
            match time_unit_word(start, "time_unit") {
                Some(word) => unit(&word),
                None if start.argument("nanoseconds").is_some() => TimeUnit::Nanoseconds,
                None => TimeUnit::Microseconds,
            },
        )),
        "polars.from_epoch" => Some(match time_unit_word(start, "time_unit").as_deref() {
            Some("d") => DataType::Date,
            Some("ns") => DataType::Datetime(TimeUnit::Nanoseconds, None),
            _ => DataType::Datetime(TimeUnit::Microseconds, None),
        }),
        _ => None,
    }
}

/// The parts a moment is made of, which `polars.datetime` folds into a literal where all are.
const PARTS_OF_A_MOMENT: [&str; 7] = [
    "year",
    "month",
    "day",
    "hour",
    "minute",
    "second",
    "microsecond",
];

/// The parts a length of time is made of, which `polars.duration` folds into a literal where all
/// are.
const PARTS_OF_A_LENGTH: [&str; 8] = [
    "weeks",
    "days",
    "hours",
    "minutes",
    "seconds",
    "milliseconds",
    "microseconds",
    "nanoseconds",
];

/// The output name of a function that makes a moment or a length of time of `parts`, which is the
/// name of the first part it is given, `literal` for one that is a number typed in or is not given.
/// Where `may_fold` and every part is a number typed in and nothing more, Polars makes a literal
/// of the call where the values fit, which it names otherwise, so the name is not known.
fn first_part_name(start: &Call, first: &str, parts: &[&str], may_fold: bool) -> Option<String> {
    // A part that Polars may fold: a number, or a flag that it reads as one, typed in. A null and
    // text are no number, so a call that holds one stays a call. A flag is taken for foldable here
    // and not by `is_folded_into_a_literal`, on purpose: the name is then not known, which only
    // leaves the rule for repeated names silent, where taking a flag for folded there would refuse
    // a key that Polars may allow. The two disagree only on the side of saying less.
    let bare_literal = |value: Option<&Value>| match value {
        None => true,
        Some(Value::Expression(part)) => {
            part.links().is_empty()
                && matches!(part.start(), Start::Call(call)
                if call.entry().id().as_str() == "polars.lit"
                    && matches!(
                        call.argument("value"),
                        Some(Value::Integer(_) | Value::Float(_) | Value::Flag(Some(_)))
                    ))
        }
        Some(_) => false,
    };
    if may_fold && parts.iter().all(|part| bare_literal(start.argument(part))) {
        return None;
    }
    match start.argument(first) {
        Some(Value::Expression(part)) => key_name(part),
        _ => Some("literal".to_owned()),
    }
}

/// Whether Polars folds the call into a literal, which it names by an alias: a moment, with no
/// zone, or a length of time whose every part is a number typed in, given or left to the number 0.
/// The values may stop it, which the document does not decide: a month of 13 is no moment, and
/// Polars then keeps the call and refuses it when the rows run.
fn is_folded_into_a_literal(start: &Call) -> bool {
    let (parts, zoned): (&[&str], bool) = match start.entry().id().as_str() {
        "polars.datetime" => (
            &PARTS_OF_A_MOMENT,
            matches!(start.argument("time_zone"), Some(Value::TimeZone(Some(_)))),
        ),
        "polars.duration" => (&PARTS_OF_A_LENGTH, false),
        _ => return false,
    };
    let a_number = |value: Option<&Value>| match value {
        None => true,
        Some(Value::Expression(part)) => {
            part.links().is_empty()
                && matches!(part.start(), Start::Call(call)
                    if call.entry().id().as_str() == "polars.lit"
                        && matches!(call.argument("value"), Some(Value::Integer(_) | Value::Float(_))))
        }
        Some(_) => false,
    };
    !zoned && parts.iter().all(|part| a_number(start.argument(part)))
}

/// The word of a flag or a choice, as the option table writes it.
fn option_word(value: &Value) -> Option<String> {
    match value {
        Value::Flag(Some(true)) => Some("true".to_owned()),
        Value::Flag(Some(false)) => Some("false".to_owned()),
        Value::Flag(None) => Some("unset".to_owned()),
        Value::Choice(word) => Some(word.clone()),
        _ => None,
    }
}

/// The options `call` was given, each by the parameter and its word as the option table writes
/// it: a flag, a choice, and a dtype, which the table writes as the name of its class, such as
/// `Int64`. A dtype that is none of the classes has no word, and is reported apart: what the
/// call gives depends on it, and the tables say nothing of it.
fn option_words(call: &Call) -> (Vec<(String, String)>, bool) {
    let entry = call.entry();
    let mut unknown = false;
    let mut words = Vec::new();
    for given in call.arguments() {
        let Some(parameter) = entry
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == given.parameter())
        else {
            continue;
        };
        let word = match (parameter.kind(), given.value()) {
            (Kind::Flag | Kind::Choice(_), value) => option_word(value),
            (Kind::Dtype, Value::Dtype(dtype)) => {
                let class = Representative::of(dtype.as_polars());
                unknown |= class.is_none();
                class.map(|class| class.name().to_owned())
            }
            _ => None,
        };
        words.extend(word.map(|word| (given.parameter().to_owned(), word)));
    }
    (words, unknown)
}

/// Whether the chain has a value that a part widened after `link`: the link met a receiver of one
/// value with each of the many values a part gives, where the part is not met so in a window, or the
/// value was widened before and is still many.
fn widens(
    link: &Call,
    arguments: &[(String, Operand)],
    receiver: Option<Shape>,
    next: &Cur,
    before: bool,
) -> bool {
    let widens_here = receiver == Some(Shape::One)
        && arguments.iter().any(|(name, operand)| {
            operand.shape == Shape::Many
                && link.entry().parameters().iter().any(|parameter| {
                    parameter.name() == name && !parameter.broadcasts_in_a_window()
                })
        });
    let many = matches!(
        next,
        Cur::Value {
            shape: Shape::Many,
            ..
        }
    );
    widens_here || (before && many)
}

/// The unit a moment or a length of time of the dtype is counted in.
const fn unit_of(dtype: &DataType) -> Option<TimeUnit> {
    match dtype {
        DataType::Datetime(unit, _) | DataType::Duration(unit) => Some(*unit),
        _ => None,
    }
}

/// The unit the word the tables write for a dtype, such as `Datetime(ns, UTC)`, counts in.
fn unit_in_name(name: &str) -> Option<TimeUnit> {
    let inside = name
        .strip_prefix("Datetime(")
        .or_else(|| name.strip_prefix("Duration("))?;
    match inside.split([',', ')']).next()? {
        "ns" => Some(TimeUnit::Nanoseconds),
        "us" => Some(TimeUnit::Microseconds),
        "ms" => Some(TimeUnit::Milliseconds),
        _ => None,
    }
}

/// The unit the chain is counted in after `link`, which follows a value counted in `before`: what a
/// method that is given a unit says it gives, as the table writes it, and the unit it follows for
/// the others, where they give a moment or a length of time.
fn unit_after(link: &Call, before: Option<TimeUnit>, standing: &Standing) -> Option<TimeUnit> {
    let result = standing.verdict()?.allowance()?.result()?;
    if !matches!(
        result.class(),
        Some(Representative::Datetime | Representative::DatetimeZoned | Representative::Duration)
    ) {
        return None;
    }
    if link
        .entry()
        .parameters()
        .iter()
        .any(|parameter| parameter.name() == "time_unit")
    {
        return unit_in_name(result.name());
    }
    before
}

/// The class a literal has, when its dtype is known without another operand:
/// one it was given, or text, a flag, or a null, which have one.
fn literal_class(call: &Call) -> Option<Representative> {
    if let Some(Value::Dtype(dtype)) = call.argument("dtype") {
        return Representative::of(dtype.as_polars());
    }
    match call.argument("value")? {
        Value::String(_) => Some(Representative::String),
        Value::Flag(Some(_)) => Some(Representative::Boolean),
        Value::Flag(None) => Some(Representative::Null),
        // A number typed without a dtype has the one Polars gives it by itself.
        Value::Integer(number) => Literal::Integer(*number).class(),
        Value::Float(number) => Literal::Float(*number).class(),
        _ => None,
    }
}

/// What `call` is when it is a number typed without a dtype.
fn loose_number(call: &Call) -> Option<Loose> {
    if call.entry().id().as_str() != "polars.lit" || call.argument("dtype").is_some() {
        return None;
    }
    match call.argument("value") {
        Some(Value::Integer(number)) => Loose::of(&Literal::Integer(*number)),
        Some(Value::Float(number)) => Loose::of(&Literal::Float(*number)),
        _ => None,
    }
}

/// Whether a method takes an operand besides what it follows, which a number
/// typed without a dtype adapts to or keeps its dtype beside: arithmetic,
/// comparisons, a fill value, an exponent.
fn takes_an_operand(entry: &Entry) -> bool {
    entry.applies() == Applies::Expression
        && entry.parameters().iter().any(|parameter| {
            matches!(parameter.kind(), Kind::Expression(_)) && !parameter.role().is_a_count()
        })
}

/// The class every value of a condition has, when they have the same one. A
/// number typed without a dtype adapts to the values beside it that are typed,
/// as Polars gives it: a whole number to whole numbers and numbers with a
/// fraction it fits, and a number with a fraction only to numbers with a
/// fraction. Among such numbers alone it has no class. A condition of values
/// of different dtypes has the supertype, which the tables do not give.
fn unify(values: &[Valued]) -> Option<Representative> {
    let classes: Option<Vec<(Representative, Option<Loose>)>> = values
        .iter()
        .map(|(class, loose)| class.map(|class| (class, *loose)))
        .collect();
    let classes = classes?;
    let first_strict = classes
        .iter()
        .find(|(_, loose)| loose.is_none())
        .map(|(class, _)| *class);
    // Numbers typed without a dtype alone have none to give: Polars cannot resolve them where
    // it runs, in a sort key, for one.
    let first = first_strict?;
    let same = |(class, loose): &(Representative, Option<Loose>)| match loose {
        None => *class == first,
        Some(number) => number.fits(first),
    };
    classes.iter().all(same).then_some(first)
}

impl Walker<'_> {
    fn here(&self) -> Place {
        self.path
            .iter()
            .cloned()
            .fold(self.base.clone(), Place::then)
    }

    fn refuse(&mut self, refusal: &Refusal, at_end: bool) {
        self.refuse_given(refusal, at_end, false);
    }

    /// Refuses a call that was asked without the dtype of one of its arguments.
    fn refuse_blind(&mut self, refusal: &Refusal) {
        self.refuse_given(refusal, false, true);
    }

    fn refuse_given(&mut self, refusal: &Refusal, at_end: bool, argument_unknown: bool) {
        if self.refusal.is_none() {
            self.refusal = Some(WalkRefusal {
                place: self.here(),
                refusal: refusal.clone(),
                at_end,
                argument_unknown,
            });
        }
    }

    fn blind(&mut self, reason: impl Into<String>) {
        if self.not_known.is_none() {
            self.not_known = Some(format!("{}: {}", self.here(), reason.into()));
        }
    }

    /// The options a step was given, asked of the option table. The options of
    /// a method are asked of [`allowed::offer`], with the call.
    fn step_options(&mut self, step: &Call) {
        let (chosen, _) = option_words(step);
        let chosen: Vec<(&str, &str)> = chosen
            .iter()
            .map(|(name, word)| (name.as_str(), word.as_str()))
            .collect();
        if let Err(refusal) = allowed::options(step.entry(), &chosen) {
            self.refuse(&refusal, false);
        }
    }

    /// Asks whether the columns that `drop_nans` is told to look at may each be tested for a NaN,
    /// which is what Polars makes of them: it refuses a column that `is_nan` refuses, such as text,
    /// when the rows run, with that method's words. The tables say which dtypes those are. The
    /// columns it looks at when it is told none are the floating-point ones, which it accepts.
    fn nan_test_columns(&mut self) {
        if self.step.entry().id().as_str() != "LazyFrame.drop_nans" {
            return;
        }
        let Some(Value::Columns(names)) = self.step.argument("subset") else {
            return;
        };
        let Some(test) = catalog::entry("Expr.is_nan") else {
            return;
        };
        for name in names {
            let Some(class) = self.source.get(name).and_then(Representative::of) else {
                continue;
            };
            let place = OfferPlace::new(
                Context::step(Outer::Select),
                After::Value {
                    dtype: class,
                    shape: Shape::Many,
                },
            );
            if let Verdict::Refused(refusal) = allowed::offer(test, &place, &Chosen::default()) {
                self.path = vec![Segment::Parameter("subset".to_owned())];
                self.refuse(&refusal, false);
                self.path.clear();
                return;
            }
        }
    }

    /// Asks whether a join's keys pair the same two output names twice, which
    /// Polars refuses, at the later key that makes the pair again. Only the pairs
    /// before the first key on either side whose name is not known are judged:
    /// Polars pairs the keys after expanding each side, and a key that may expand
    /// or is not picked yet shifts every pair after it.
    fn key_names(&mut self) {
        if self.step.entry().id().as_str() != "LazyFrame.join" {
            return;
        }
        let on = self.step.argument("on");
        let left = self.step.argument("left_on").or(on);
        let right = self.step.argument("right_on").or(on);
        let (Some(left), Some(right)) = (left, right) else {
            return;
        };
        let left_parameter = if self.step.argument("left_on").is_some() {
            "left_on"
        } else {
            "on"
        };
        let (left, right) = (known_key_names(left), known_key_names(right));
        let mut paired: Vec<(&String, &String)> = Vec::new();
        for (at, pair) in left.iter().zip(&right).enumerate() {
            if !paired.contains(&pair) {
                paired.push(pair);
                continue;
            }
            let context = Context::step(Outer::JoinLeft);
            let Err(refusal) = allowed::rule_forbids_a_repeated_pair_of_key_names(context) else {
                return;
            };
            let mut path = vec![Segment::Parameter(left_parameter.to_owned())];
            if left_is_many(self.step, left_parameter) {
                path.push(Segment::Item(at));
            }
            let place = path.into_iter().fold(self.base.clone(), Place::then);
            // A key already refused keeps its refusal: this one is the later reason.
            if let Some(walked) = self
                .walked
                .iter_mut()
                .find(|walked| walked.place == place && walked.context == Some(context))
                && !matches!(walked.ending, Some(Ending::Refused(_)))
            {
                walked.ending = Some(Ending::Refused(refusal.clone()));
            }
            if self.refusal.is_none() {
                self.refusal = Some(WalkRefusal {
                    place,
                    refusal,
                    at_end: true,
                    argument_unknown: false,
                });
            }
            return;
        }
    }

    /// Walks the arguments of `call`, and gives what each expression it holds
    /// has, by the parameter that holds it. `values_end` says whether the values
    /// of a condition that `call` is a link of end the predicate: the condition
    /// is all the expression is.
    fn arguments(&mut self, call: &Call, values_end: bool) -> Vec<(String, Operand)> {
        let entry = call.entry().id().as_str();
        let mut found = Vec::new();
        for given in call.arguments() {
            self.path
                .push(Segment::Parameter(given.parameter().to_owned()));
            let slot = Slot::of_parameter(entry, given.parameter());
            self.slots.extend(slot);
            // What an expression held here ends: a filter's predicate does, and
            // a value of a condition does when the condition does.
            let ends = match slot {
                Some(Slot::Outer(Outer::Filter)) => true,
                Some(Slot::Nested(Nested::ConditionValue)) => values_end,
                _ => false,
            };
            self.value(
                given.value(),
                given.parameter(),
                slot.is_some().then_some(ends),
                &mut found,
            );
            // `on` is the keys of both tables: Polars evaluates it against the other
            // one as well, so what the tables say of it there is asked too. Its
            // expressions are the left's first, at the same places.
            if slot == Some(Slot::Outer(Outer::JoinLeft)) && given.parameter() == "on" {
                self.slots.pop();
                self.slots.push(Slot::Outer(Outer::JoinRight));
                self.value(
                    given.value(),
                    given.parameter(),
                    Some(false),
                    &mut Vec::new(),
                );
            }
            if slot.is_some() {
                self.slots.pop();
            }
            self.path.pop();
        }
        found
    }

    fn value(
        &mut self,
        value: &Value,
        parameter: &str,
        held: Option<bool>,
        found: &mut Vec<(String, Operand)>,
    ) {
        match value {
            Value::Expression(expression) => {
                let operand = self.expression(expression, held);
                found.push((parameter.to_owned(), operand));
            }
            Value::Many(items) => {
                for (at, item) in items.iter().enumerate() {
                    self.path.push(Segment::Item(at));
                    self.value(item, parameter, held, found);
                    self.path.pop();
                }
            }
            Value::Pairs(pairs) => {
                for (at, (key, entry)) in pairs.iter().enumerate() {
                    self.path.push(Segment::Key(at));
                    self.value(key, parameter, held, found);
                    self.path.pop();
                    self.path.push(Segment::Value(at));
                    self.value(entry, parameter, held, found);
                    self.path.pop();
                }
            }
            _ => {}
        }
    }

    /// Whether a `col` names more than one column, which expands it to an
    /// expression for each, whatever the first one is.
    fn names_several(call: &Call) -> bool {
        let several = |value: Option<&Value>| match value {
            Some(Value::Columns(names)) => names.len() > 1,
            _ => false,
        };
        let more = match call.argument("more_names") {
            Some(Value::Many(items)) => !items.is_empty(),
            Some(_) => true,
            None => false,
        };
        call.entry().id().as_str() == "polars.col" && (several(call.argument("name")) || more)
    }

    /// The class of the column an argument names, in the table the expression
    /// is evaluated against, where it names one.
    fn column_of(&self, call: &Call) -> Option<Representative> {
        let Some(Value::Columns(names)) = call.argument("name") else {
            return None;
        };
        if Self::names_several(call) {
            return None;
        }
        Representative::of(self.read_against()?.get(names.first()?)?)
    }

    /// The schema of the table the expression being walked reads its columns
    /// from: the other table's for a join's `right_on`, and for `on`, which is
    /// read against it as well, and the table the step starts from otherwise.
    /// `None` where it is the other table and that is not known.
    fn read_against(&self) -> Option<&Schema> {
        if self.slots.first() == Some(&Slot::Outer(Outer::JoinRight)) {
            self.other.as_deref()
        } else {
            Some(self.source)
        }
    }

    /// Says the tables cannot tell what follows a column of a join's `on` that
    /// the table it is read against does not have, which Polars refuses as not
    /// found. `on` is read against both tables, so a column in only one of them is
    /// a mistake the walk can see. The editor picks the columns of other positions
    /// from the table they are read against, which the walk does not ask.
    fn blind_to_a_missing_column(&mut self, call: &Call) {
        let in_on = matches!(
            self.path.first(),
            Some(Segment::Parameter(parameter)) if parameter == "on"
        ) && self.step.entry().id().as_str() == "LazyFrame.join";
        if !in_on || call.entry().id().as_str() != "polars.col" || Self::names_several(call) {
            return;
        }
        let Some(Value::Columns(names)) = call.argument("name") else {
            return;
        };
        let Some(name) = names.first() else {
            return;
        };
        match self.read_against() {
            Some(schema) if schema.get(name).is_some() => {}
            Some(_) => self.blind(format!("the table has no column called {name:?}")),
            None => self.blind("the other table's columns are not known"),
        }
    }

    /// The class of what a pivot's group holds, which `element` stands for: the
    /// column the pivot takes values from, where it takes them from one.
    fn group_of(&self) -> Option<Representative> {
        if self.step.entry().id().as_str() != "LazyFrame.pivot" {
            return None;
        }
        let Some(Value::Columns(names)) = self.step.argument("values") else {
            return None;
        };
        // A group of several columns has the dtype of none of them alone.
        let [name] = names.as_slice() else {
            return None;
        };
        Representative::of(self.source.get(name)?)
    }

    /// The dtype a start gives, where its arguments decide it: that of the column it reads, or of
    /// the dtype a literal is given.
    fn start_dtype(&self, start: &Call) -> Option<DataType> {
        match start.entry().id().as_str() {
            "polars.col" => {
                let Some(Value::Columns(names)) = start.argument("name") else {
                    return None;
                };
                if Self::names_several(start) {
                    return None;
                }
                self.read_against()?.get(names.first()?).cloned()
            }
            "polars.lit" => match start.argument("dtype") {
                Some(Value::Dtype(dtype)) => Some(dtype.as_polars().clone()),
                _ => None,
            },
            "polars.element" => {
                let Some(Value::Columns(names)) = self.step.argument("values") else {
                    return None;
                };
                let [name] = names.as_slice() else {
                    return None;
                };
                self.source.get(name).cloned()
            }
            _ => made_dtype(start),
        }
    }

    /// The zone of the moment a start gives, where its arguments decide it.
    fn start_zone(&mut self, start: &Call) -> Option<Zone> {
        match self.start_dtype(start)? {
            DataType::Datetime(_, None) => Some(Zone::Naive),
            DataType::Datetime(_, Some(zone)) => Some(self.zone_named(zone.as_str())),
            _ => None,
        }
    }

    /// The names of the two zones of a moment a method follows and an operand it is given as one the
    /// moment is compared with, subtracted by, filled with or bound by, where both are known and
    /// they differ.
    fn zone_clash(
        &self,
        entry: &Entry,
        arguments: &[(String, Operand)],
        receiver: Option<Zone>,
    ) -> Option<(String, String)> {
        let zone_of = |name: &str| {
            arguments
                .iter()
                .find(|(given, _)| given == name)
                .and_then(|(_, operand)| operand.zone)
        };
        let differ = |first: Option<Zone>, second: Option<Zone>| match (first, second) {
            (Some(Zone::Named(first)), Some(Zone::Named(second))) if first != second => {
                Some((self.zones[first].clone(), self.zones[second].clone()))
            }
            _ => None,
        };
        entry.parameters().iter().find_map(|parameter| {
            let own = zone_of(parameter.name())?;
            // An operand is met with the value it follows, as a comparison, a difference, a fill and
            // the bounds of an interval are; a parameter that shares a dtype with another is met with
            // that one, as the default of a replacement is with its new values, and the value it
            // follows is no matter.
            if matches!(parameter.role(), Role::Operand | Role::Filler)
                && let Some(clash) = differ(receiver, Some(own))
            {
                return Some(clash);
            }
            let partner = parameter.shares_a_dtype_with()?;
            differ(zone_of(partner), Some(own))
        })
    }

    /// The names of two zones of the values a condition has when this link ends it, where both are
    /// known and they differ: Polars finds no dtype that the moments of two zones have in common.
    fn condition_zone_clash(
        &self,
        entry: &Entry,
        cur: &Cur,
        arguments: &[(String, Operand)],
    ) -> Option<(String, String)> {
        if !matches!(
            entry.id().as_str(),
            "Then.otherwise" | "ChainedThen.otherwise"
        ) {
            return None;
        }
        let Cur::Condition { zones, .. } = cur else {
            return None;
        };
        let last = arguments
            .iter()
            .find(|(name, _)| name == "statement")
            .and_then(|(_, operand)| operand.zone);
        let known: Vec<usize> = zones
            .iter()
            .copied()
            .chain(std::iter::once(last))
            .filter_map(|zone| match zone {
                Some(Zone::Named(at)) => Some(at),
                _ => None,
            })
            .collect();
        let first = *known.first()?;
        let other = known.iter().find(|at| **at != first)?;
        Some((self.zones[first].clone(), self.zones[*other].clone()))
    }

    /// The zone the walk holds under `name`.
    fn zone_named(&mut self, name: &str) -> Zone {
        let at = self
            .zones
            .iter()
            .position(|known| known == name)
            .unwrap_or_else(|| {
                self.zones.push(name.to_owned());
                self.zones.len() - 1
            });
        Zone::Named(at)
    }

    /// The zone of the chain after `link`, which follows a moment in `before`: the zone a method that
    /// is given one gives, as the call says it, and the zone it follows for the others, where they
    /// give a moment.
    fn zone_after(
        &mut self,
        link: &Call,
        before: Option<Zone>,
        standing: &Standing,
    ) -> Option<Zone> {
        let result = standing.verdict()?.allowance()?.result()?;
        match result.class() {
            Some(Representative::Datetime) => Some(Zone::Naive),
            Some(Representative::DatetimeZoned) => {
                if link
                    .entry()
                    .parameters()
                    .iter()
                    .any(|parameter| parameter.kind() == Kind::TimeZone)
                {
                    let (_, zone) = result.name().split_once(", ")?;
                    Some(self.zone_named(zone.strip_suffix(')')?))
                } else {
                    before.filter(|zone| matches!(zone, Zone::Named(_)))
                }
            }
            _ => None,
        }
    }

    /// The unit the moment or the length of time a start gives is counted in, where its arguments
    /// decide it.
    fn start_unit(&self, start: &Call) -> Option<TimeUnit> {
        unit_of(&self.start_dtype(start)?)
    }

    /// The class a start gives, where its arguments decide it.
    fn start_class(&self, start: &Call) -> Option<Representative> {
        match start.entry().id().as_str() {
            "polars.col" => self.column_of(start),
            "polars.lit" => literal_class(start),
            "polars.element" => self.group_of(),
            _ => Representative::of(&made_dtype(start)?),
        }
    }

    /// Walks `expression`, which a position holds when `held` says whether it ends
    /// the predicate there, and which is an argument of a call otherwise.
    fn expression(&mut self, expression: &Expression, held: Option<bool>) -> Operand {
        // The values of a condition end the predicate where the condition is
        // all the expression is: a method that goes on from it is what ends it.
        let values_end = held == Some(true)
            && matches!(expression.start(), Start::Call(start) if start.entry().applies() == Applies::Condition)
            && expression
                .links()
                .iter()
                .all(|link| link.entry().applies() == Applies::Condition);
        let place = self.here();
        let context = Context::from_positions(&self.slots);
        let unsaid = Operand {
            dtype: None,
            loose: None,
            shape: Shape::Many,
            known: false,
            counted: false,
            zone: None,
        };
        let mut calls: Vec<Standing> = Vec::new();
        let mut last_end: Option<Ending> = None;
        let mut cur = Cur::Dead;
        let mut start_arguments: Vec<(String, Operand)> = Vec::new();
        match expression.start() {
            Start::Call(start) => {
                self.path.push(Segment::Start);
                start_arguments = self.arguments(start, values_end);
                self.path.pop();
            }
            Start::Input(_) | Start::Hole => {}
        }
        let Some(context) = context else {
            self.blind("no named position holds the expression");
            self.walked.push(WalkedExpression {
                place,
                context: None,
                calls,
                next: None,
                ending: None,
            });
            return unsaid;
        };
        match expression.start() {
            Start::Call(start) => {
                self.path.push(Segment::Start);
                let takes_expressions = takes_expressions(start);
                // What a function that takes expressions gives is said only where each argument's
                // dtype is known, as for a method: the tables are not asked of one that is not, and
                // Polars may refuse it for its dtype.
                let class = self.start_class(start).filter(|_| {
                    !takes_expressions
                        || start_arguments
                            .iter()
                            .all(|(_, operand)| operand.dtype.is_some())
                });
                self.blind_to_a_missing_column(start);
                // A function that takes expressions is judged for what it is given, and gives as
                // many values as its widest argument.
                let given: Vec<(&str, Representative)> = start_arguments
                    .iter()
                    .filter_map(|(name, operand)| operand.dtype.map(|dtype| (name.as_str(), dtype)))
                    .collect();
                let unclassed: Vec<&str> = start_arguments
                    .iter()
                    .filter(|(_, operand)| operand.dtype.is_none())
                    .map(|(name, _)| name.as_str())
                    .collect();
                let (words, _) = option_words(start);
                let words: Vec<(&str, &str)> = words
                    .iter()
                    .map(|(name, word)| (name.as_str(), word.as_str()))
                    .collect();
                let time_zones = start.time_zones();
                let chosen = Chosen::default();
                let chosen = match class {
                    Some(dtype) => chosen.giving(dtype),
                    None => chosen,
                };
                let chosen = if takes_expressions {
                    let chosen = chosen
                        .with_arguments(&given)
                        .with_unclassed(&unclassed)
                        .with_options(&words)
                        .with_time_zones(&time_zones)
                        .following(self.start_unit(start));
                    if start_arguments
                        .iter()
                        .all(|(_, operand)| operand.shape == Shape::One)
                    {
                        chosen.giving_one()
                    } else {
                        chosen.giving_many()
                    }
                } else {
                    chosen
                };
                let verdict = match allowed::rule_forbids_a_folded_function_in_a_join_key(context) {
                    Err(refusal) if is_folded_into_a_literal(start) => Verdict::Refused(refusal),
                    _ => allowed::offer(
                        start.entry(),
                        &OfferPlace::new(context, After::Start),
                        &chosen,
                    ),
                };
                match &verdict {
                    Verdict::Refused(refusal) => self.refuse(refusal, false),
                    Verdict::Allowed(allowance) => {
                        last_end = Some(allowance.can_end().clone());
                        // What the tables are not told is not guessed: an argument whose dtype is
                        // not known leaves whether it may end, and fits, not known.
                        if takes_expressions && !unclassed.is_empty() {
                            self.blind(format!(
                                "the dtype of an argument of {} is not known",
                                start.entry().id().as_str()
                            ));
                            last_end = None;
                        }
                        // An expansion has a dtype for each column, so whether it
                        // may end is not known, whatever the table measured.
                        if Self::names_several(start) && last_end == Some(Ending::Allowed) {
                            last_end = Some(Ending::NotKnown(Unknown::TheStartsDtype));
                        }
                        cur = if start.entry().id().as_str() == "polars.when" {
                            Cur::Condition {
                                state: ConditionState::When,
                                values: Vec::new(),
                                zones: Vec::new(),
                                single: start_arguments
                                    .iter()
                                    .all(|(_, operand)| operand.shape == Shape::One),
                                values_single: true,
                                known: start_arguments.iter().all(|(_, operand)| operand.known),
                            }
                        } else {
                            Cur::Value {
                                dtype: class,
                                shape: allowance.shape(),
                            }
                        };
                    }
                }
                calls.push(Standing {
                    follows: Some(After::Start),
                    verdict: Some(verdict),
                });
                self.path.pop();
            }
            Start::Hole => {
                // Nothing is picked, so nothing is asked of what follows.
                calls.push(Standing::not_asked());
                cur = Cur::Value {
                    dtype: None,
                    shape: Shape::Many,
                };
            }
            Start::Input(name) => {
                cur = Cur::Value {
                    dtype: None,
                    shape: Shape::Many,
                };
                let standing = self.input_start(name, context, &mut cur, &mut last_end);
                calls.push(standing);
            }
        }
        // Values that are one expression make the condition that expression, and
        // its predicates are dropped.
        let statements: Vec<&Expression> = expression
            .links()
            .iter()
            .filter_map(|link| match link.argument("statement") {
                Some(Value::Expression(statement)) => Some(statement),
                _ => None,
            })
            .collect();
        let mut branches_are_one =
            statements.len() > 1 && statements.windows(2).all(|pair| pair[0] == pair[1]);
        let mut widened = false;
        let mut unit = match expression.start() {
            Start::Call(start) => self.start_unit(start),
            _ => None,
        };
        let mut zone = match expression.start() {
            Start::Call(start) => self.start_zone(start),
            _ => None,
        };
        for (at, link) in expression.links().iter().enumerate() {
            self.path.push(Segment::Link(at));
            let arguments = self.arguments(link, values_end);
            // The first link follows the start, which is a number typed without a dtype when
            // it is one.
            let receiver_loose = match expression.start() {
                Start::Call(start) if at == 0 => loose_number(start),
                _ => None,
            };
            // A condition whose predicates and values are all one value gives one value whether
            // or not Polars drops its predicates, so what follows it need not wait on that.
            // Values that are many are many whether or not Polars drops the predicates.
            let all_single = matches!(
                &cur,
                Cur::Condition { single: true, .. }
                    | Cur::Condition {
                        values_single: false,
                        ..
                    }
            );
            let receiver_shape = match &cur {
                Cur::Value { shape, .. } => Some(*shape),
                _ => None,
            };
            let (next, standing) = self.link(
                link,
                &arguments,
                context,
                cur,
                &mut last_end,
                Around {
                    branches_are_one,
                    receiver_loose,
                    unit,
                    widened,
                    zone,
                },
            );
            unit = unit_after(link, unit, &standing);
            zone = self.zone_after(link, zone, &standing);
            widened = widens(link, &arguments, receiver_shape, &next, widened);
            if branches_are_one
                && all_single
                && matches!(
                    link.entry().id().as_str(),
                    "Then.otherwise" | "ChainedThen.otherwise"
                )
            {
                branches_are_one = false;
            }
            // A method that gives one value of the many that a condition of identical values may
            // give is one value however the optimizer treats the condition. It is one by its own
            // nature, as an aggregate is, where the one value after a method that follows the
            // condition and gives a value for each of what it follows is only the one that the
            // condition may be, which is not counted.
            if branches_are_one && is_one_value(link) {
                branches_are_one = false;
            }
            cur = next;
            calls.push(standing);
            self.path.pop();
        }
        // A condition that has no `otherwise` and holds a number typed without a dtype has no
        // dtype to give it: Polars cannot resolve the number, and refuses where it runs.
        if let Cur::Condition { values, .. } = &cur
            && values.iter().any(|(_, loose)| loose.is_some())
        {
            last_end = Some(Ending::NotKnown(Unknown::TheResultsDtype));
        }
        let mut ending = None;
        if held.is_some() && !matches!(cur, Cur::Dead) {
            // A value of a condition in a filter need be a Boolean only where
            // the condition ends the predicate.
            let asked = held == Some(true)
                || context != Context::inside(Outer::Filter, Nested::ConditionValue);
            if asked {
                ending = last_end;
            }
            if let Some(Ending::Refused(refusal)) = &ending {
                self.refuse(&refusal.clone(), true);
            } else if let Some(Ending::NotKnown(unknown)) = &ending {
                self.blind(format!("whether it may end is not known: {unknown}"));
            }
            // A join's key that holds an aggregate beside a column, or a column
            // compared with a null, is not elementwise, which only the whole key
            // shows.
            // A column of the dtype Null holds a null in every row, so what compares it with a
            // null or a column leaves no value to replace, though it is a column to an aggregate
            // beside it.
            let as_read = self
                .read_against()
                .and_then(|schema| with_null_columns_as_literals(expression, schema));
            if is_not_elementwise(expression, as_read.as_ref().unwrap_or(expression))
                && let Err(refusal) = allowed::rule_forbids_a_key_known_not_elementwise(context)
            {
                self.refuse(&refusal, true);
                ending = Some(Ending::Refused(refusal));
            }
        }
        let next = match &cur {
            Cur::Value {
                dtype: Some(dtype),
                shape,
            } => Some(After::Value {
                dtype: *dtype,
                shape: *shape,
            }),
            Cur::Condition { state, .. } => Some(After::Condition(*state)),
            _ => None,
        };
        self.walked.push(WalkedExpression {
            place,
            context: Some(context),
            calls,
            next,
            ending,
        });
        match cur {
            Cur::Value { dtype, shape } => Operand {
                dtype,
                loose: match expression.start() {
                    Start::Call(start) if expression.links().is_empty() => loose_number(start),
                    _ => None,
                },
                shape,
                // What was not asked has no shape that can be counted on.
                known: dtype.is_some(),
                counted: dtype.is_some() && !branches_are_one,
                zone,
            },
            Cur::Condition { values, .. } => Operand {
                dtype: unify(&values),
                loose: None,
                shape: Shape::Many,
                known: true,
                counted: !branches_are_one,
                zone: None,
            },
            Cur::Dead => unsaid,
        }
    }

    /// Asks the tables about a chain that starts from the connected input
    /// `name`, which the run makes a literal of its dtype. `cur` and `last_end`
    /// are what the chain has afterwards, as for any other start.
    fn input_start(
        &mut self,
        name: &str,
        context: Context,
        cur: &mut Cur,
        last_end: &mut Option<Ending>,
    ) -> Standing {
        let Some(literal) = self.inputs.start_literal(name) else {
            self.blind(format!(
                "there is no connected value or list called {name:?}"
            ));
            return Standing::not_asked();
        };
        let Some(class) = literal.class else {
            self.blind(format!(
                "the dtype of the connected input {name:?} is not one the tables cover"
            ));
            return Standing::not_asked();
        };
        let verdict = allowed::offer_input(
            &OfferPlace::new(context, After::Start),
            class,
            literal.shape,
        );
        match &verdict {
            Verdict::Refused(refusal) => {
                self.refuse(refusal, false);
                *cur = Cur::Dead;
            }
            Verdict::Allowed(allowance) => {
                *last_end = Some(allowance.can_end().clone());
                *cur = Cur::Value {
                    dtype: Some(class),
                    shape: allowance.shape(),
                };
            }
        }
        Standing {
            follows: Some(After::Start),
            verdict: Some(verdict),
        }
    }

    /// Asks whether `link` may stand after what `cur` has, and gives what the
    /// chain has after it.
    fn link(
        &mut self,
        link: &Call,
        arguments: &[(String, Operand)],
        context: Context,
        cur: Cur,
        last_end: &mut Option<Ending>,
        around: Around,
    ) -> (Cur, Standing) {
        let Around {
            branches_are_one,
            receiver_loose,
            unit,
            widened,
            zone,
        } = around;
        let entry = link.entry();
        // A window is refused a value of one row that a part widened, whose output is longer than
        // its input.
        if widened
            && entry.id().as_str() == "Expr.over"
            && let Err(refusal) = allowed::rule_forbids_a_window_over_a_widened_value()
        {
            self.refuse(&refusal, false);
            return (
                Cur::Dead,
                Standing {
                    follows: None,
                    verdict: Some(Verdict::Refused(refusal)),
                },
            );
        }
        // Moments in two zones that a method is given to compare, subtract, fill or bind by one
        // another have no dtype in common.
        let clash = self
            .zone_clash(entry, arguments, zone)
            .or_else(|| self.condition_zone_clash(entry, &cur, arguments));
        let clash_of_zones = clash
            .as_ref()
            .map(|(first, second)| (first.as_str(), second.as_str()));
        // The zone of the moment the call follows, which a length of time given to it is read in.
        let zone_name = match zone {
            Some(Zone::Named(at)) => Some(self.zones[at].clone()),
            _ => None,
        };
        // A number typed without a dtype that a method takes another operand beside adapts to
        // it, as the dtype of that operand where it fits it, or keeps the dtype it has alone, as
        // the base of a power does. Where there is no such operand the tables, measured with typed
        // columns, cannot say what it follows.
        let cur = match (cur, receiver_loose) {
            (
                Cur::Value {
                    shape,
                    dtype: alone,
                },
                Some(number),
            ) if takes_an_operand(entry) => {
                let meetings: Vec<(Receiver, &Operand)> = arguments
                    .iter()
                    .filter_map(|(name, operand)| {
                        receiver_beside(entry, name, number).map(|meeting| (meeting, operand))
                    })
                    .collect();
                let keeps = !meetings.is_empty()
                    && meetings
                        .iter()
                        .all(|(meeting, _)| *meeting == Receiver::Keeps);
                let dtype = if keeps {
                    alone
                } else {
                    meetings
                        .iter()
                        .filter(|(meeting, operand)| {
                            *meeting == Receiver::Adapts && operand.loose.is_none()
                        })
                        .map(|(_, operand)| operand.dtype)
                        .collect::<Option<Vec<_>>>()
                        .and_then(|dtypes| match dtypes.as_slice() {
                            [dtype] => number.beside(*dtype),
                            _ => None,
                        })
                };
                Cur::Value { dtype, shape }
            }
            (other, _) => other,
        };
        let after = match &cur {
            Cur::Dead => return (Cur::Dead, Standing::not_asked()),
            Cur::Value { dtype: None, shape } => {
                self.blind(format!(
                    "the dtype before {} is not known",
                    entry.id().as_str()
                ));
                *last_end = None;
                return (
                    Cur::Value {
                        dtype: None,
                        shape: *shape,
                    },
                    Standing::not_asked(),
                );
            }
            Cur::Value {
                dtype: Some(dtype),
                shape,
            } => After::Value {
                dtype: *dtype,
                shape: *shape,
            },
            Cur::Condition { state, .. } => After::Condition(*state),
        };
        // A number typed without a dtype that a method takes as an operand adapts to what it
        // follows. Its dtype is that of what the method follows where it fits it, and otherwise
        // not known to the tables, unless it counts rows.
        let receiver = match &cur {
            Cur::Value { dtype, .. } if receiver_loose.is_none() => *dtype,
            _ => None,
        };
        let resolved: Vec<(&str, Option<Representative>, Option<Loose>)> = arguments
            .iter()
            .map(|(name, operand)| {
                let counts = operand.loose.is_some_and(|number| {
                    number.is_whole() && !is_refused_as_a_count(entry, name, number)
                });
                let hidden = entry.applies() == Applies::Expression
                    && operand.loose.is_some()
                    && !(counts && is_count(entry, name));
                let dtype = if hidden {
                    operand
                        .loose
                        .zip(receiver)
                        .filter(|_| operand_adapts(entry, name))
                        .filter(|(number, receiver)| {
                            !is_left_unresolved(entry, name, *number, *receiver)
                        })
                        .and_then(|(number, receiver)| adapted_dtype(entry, name, number, receiver))
                } else {
                    operand.dtype
                };
                (name.as_str(), dtype, operand.loose.filter(|_| hidden))
            })
            .collect();
        let given: Vec<(&str, Representative)> = resolved
            .iter()
            .filter_map(|(name, dtype, _)| dtype.map(|dtype| (*name, dtype)))
            .collect();
        // Where every argument the tables are not told is a number given to a parameter that must
        // be given, what the call gives may be the same whatever dtype the number takes.
        let missing: Vec<&str> = resolved
            .iter()
            .filter(|(_, dtype, _)| dtype.is_none())
            .map(|(name, _, _)| *name)
            .collect();
        let only_a_number = match missing.as_slice() {
            [name] => {
                resolved
                    .iter()
                    .any(|(given, _, number)| given == name && number.is_some())
                    && operand_adapts(entry, name)
                    && is_required(entry, name)
                    // A number that adapts to numbers only is no number of any dtype beside
                    // another, which Polars refuses for some methods.
                    && (!adapts_numbers_only(entry, name)
                        || receiver.is_some_and(could_be_taken_by_a_number))
            }
            _ => false,
        };
        let (words, unrepresented) = option_words(link);
        let words: Vec<(&str, &str)> = words
            .iter()
            .map(|(name, word)| (name.as_str(), word.as_str()))
            .collect();
        let patterns = link.patterns();
        let durations = link.durations();
        let time_zones = link.time_zones();
        let place = OfferPlace::new(context, after);
        // Two arguments that meet in the supertype of their dtypes are judged each apart, so
        // where their dtypes differ, whether Polars finds a supertype is not known, and what
        // the tables say of either beside a companion of the dtype of the other is not
        // what Polars says of the two.
        let class_of_given = |name: &str| {
            given
                .iter()
                .find(|(given, _)| *given == name)
                .map(|(_, class)| *class)
        };
        let meeting: Vec<&str> = entry
            .parameters()
            .iter()
            .filter_map(|parameter| {
                let other = parameter.shares_a_dtype_with()?;
                match (class_of_given(parameter.name()), class_of_given(other)) {
                    (Some(first), Some(second)) if first != second => {
                        Some([parameter.name(), other])
                    }
                    _ => None,
                }
            })
            .flatten()
            .collect();
        // A null literal of another dtype than what it fills is folded away by Polars where it
        // runs the rows, which then give the dtype of what it fills, where its plan gives the
        // supertype of the two, so what the call gives is not known.
        let fills_with_a_null_of_another_dtype = entry.id().as_str() == "Expr.fill_null"
            && matches!(link.argument("value"), Some(Value::Expression(filler)) if is_a_null(filler))
            && matches!(
                (class_of_given("value"), receiver),
                (Some(filler), Some(follows)) if filler != follows
            );
        let mut meeting = meeting;
        if fills_with_a_null_of_another_dtype {
            meeting.push("value");
        }
        let supertyped = !meeting.is_empty();
        // What the tables are not told is not guessed: an argument whose dtype
        // is not known leaves the call to be offered for any dtype that fits,
        // which says it stands, not that this argument fits. The dtype a call is given is
        // an argument too, which a cast to a dtype the tables do not cover leaves unknown.
        if given.len() < arguments.len() || unrepresented || supertyped {
            self.blind(format!(
                "the dtype of an argument of {} is not known",
                entry.id().as_str()
            ));
            *last_end = None;
            // The call is still offered, for a refusal is a refusal whatever
            // the argument, but nothing is said of what it gives.
            let judged: Vec<(&str, Representative)> = given
                .iter()
                .filter(|(name, _)| !meeting.contains(name))
                .copied()
                .collect();
            let unjudged: Vec<&str> = arguments
                .iter()
                .map(|(name, _)| name.as_str())
                .filter(|name| !judged.iter().any(|(judged, _)| judged == name))
                .collect();
            let verdict = allowed::offer(
                entry,
                &place,
                &Chosen::default()
                    .with_arguments(&judged)
                    .with_unclassed(&unjudged)
                    .with_options(&words)
                    .with_patterns(&patterns)
                    .with_durations(&durations)
                    .with_time_zones(&time_zones)
                    .following(unit)
                    .in_zone(zone_name.as_deref())
                    .with_zone_clash(clash_of_zones),
            );
            let standing = Standing {
                follows: Some(after),
                verdict: Some(verdict.clone()),
            };
            if let Verdict::Refused(refusal) = &verdict {
                self.refuse_blind(refusal);
                return (Cur::Dead, standing);
            }
            let same = match (&verdict, missing.as_slice()) {
                (Verdict::Allowed(_), [name]) if only_a_number => allowed::same_for_any_number(
                    entry,
                    &place,
                    &Chosen::default()
                        .with_arguments(&given)
                        .with_options(&words),
                    name,
                ),
                _ => None,
            };
            let (dtype, shape) = match same {
                Some((result, shape)) => (result.class(), shape),
                None => (None, Shape::Many),
            };
            return (Cur::Value { dtype, shape }, standing);
        }
        let statement = arguments
            .iter()
            .find(|(name, _)| name == "statement")
            .map(|(_, operand)| *operand);
        let statement_class: Valued = (
            statement.and_then(|operand| operand.dtype),
            statement.and_then(|operand| operand.loose),
        );
        // Whether the predicates and the value this link is given give one
        // value.
        let given_known = arguments
            .iter()
            .filter(|(name, _)| matches!(name.as_str(), "predicates" | "statement"))
            .all(|(_, operand)| operand.known);
        let given_values_single = arguments
            .iter()
            .filter(|(name, _)| name == "statement")
            .all(|(_, operand)| operand.shape == Shape::One);
        let given_single = arguments
            .iter()
            .filter(|(name, _)| matches!(name.as_str(), "predicates" | "statement"))
            .all(|(_, operand)| operand.shape == Shape::One);
        // A condition that ends gives the dtype its values have, when they
        // have one.
        let gives = match (&cur, entry.id().as_str()) {
            (Cur::Condition { values, .. }, "Then.otherwise" | "ChainedThen.otherwise") => {
                let mut all = values.clone();
                all.push(statement_class);
                unify(&all)
            }
            _ => None,
        };
        let unclassed: Vec<&str> = arguments
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| !given.iter().any(|(given, _)| given == name))
            .collect();
        let chosen = Chosen::default()
            .with_arguments(&given)
            .with_unclassed(&unclassed)
            .with_options(&words)
            .with_patterns(&patterns)
            .with_durations(&durations)
            .with_time_zones(&time_zones)
            .following(unit)
            .in_zone(zone_name.as_deref())
            .with_zone_clash(clash_of_zones);
        let chosen = match gives {
            Some(dtype) => chosen.giving(dtype),
            None => chosen,
        };
        let verdict = allowed::offer(entry, &place, &chosen);
        let standing = Standing {
            follows: Some(after),
            verdict: Some(verdict.clone()),
        };
        let allowance = match &verdict {
            Verdict::Refused(refusal) => {
                self.refuse(refusal, false);
                return (Cur::Dead, standing);
            }
            Verdict::Allowed(allowance) => allowance,
        };
        // Polars refuses the rows of a replacement given values of other numbers than it takes,
        // which the shapes of the arguments say.
        if let Cur::Value { shape, .. } = &cur
            && !branches_are_one
            && gives_other_numbers_of_values(entry, arguments, *shape)
            && let Err(refusal) = allowed::rule_forbids_replacement_values_of_other_numbers()
        {
            self.refuse(&refusal, false);
            return (
                Cur::Dead,
                Standing {
                    follows: standing.follows,
                    verdict: Some(Verdict::Refused(refusal)),
                },
            );
        }
        *last_end = Some(allowance.can_end().clone());
        let next = match (entry.applies(), cur) {
            (Applies::Expression, _) => Cur::Value {
                dtype: allowance.result().and_then(allowed::ResultDtype::class),
                shape: broadcast(link, arguments, allowance.shape()),
            },
            (
                Applies::Condition,
                Cur::Condition {
                    mut values,
                    mut zones,
                    single,
                    values_single,
                    known,
                    ..
                },
            ) => {
                let single = single && given_single;
                let values_single = values_single && given_values_single;
                let known = known && given_known;
                let state = match entry.id().as_str() {
                    "When.then" => {
                        values.push(statement_class);
                        zones.push(statement.and_then(|operand| operand.zone));
                        ConditionState::Then
                    }
                    "ChainedWhen.then" => {
                        values.push(statement_class);
                        zones.push(statement.and_then(|operand| operand.zone));
                        ConditionState::ChainedThen
                    }
                    "Then.when" | "ChainedThen.when" => ConditionState::ChainedWhen,
                    _ => {
                        values.push(statement_class);
                        let dtype = if known { unify(&values) } else { None };
                        // Numbers typed without a dtype that have no typed value beside them
                        // have none to give, and Polars cannot always resolve them where it
                        // runs, so whether the expression may end here is not known.
                        if dtype.is_none() && values.iter().any(|(_, loose)| loose.is_some()) {
                            *last_end = Some(Ending::NotKnown(Unknown::TheResultsDtype));
                        }
                        return (
                            Cur::Value {
                                dtype,
                                // Where the values are one expression the predicates
                                // are dropped, and the values give the shape.
                                shape: if (branches_are_one && values_single)
                                    || (!branches_are_one && single)
                                {
                                    Shape::One
                                } else {
                                    Shape::Many
                                },
                            },
                            standing,
                        );
                    }
                };
                Cur::Condition {
                    state,
                    values,
                    zones,
                    single,
                    values_single,
                    known,
                }
            }
            _ => Cur::Dead,
        };
        (next, standing)
    }
}

/// The number of values a method gives, which is many where it gives one value of the value it
/// follows and is given an operand of many: the one value is met by each of them, as the count of
/// nulls is by the base of a logarithm that is a column.
fn broadcast(link: &Call, arguments: &[(String, Operand)], shape: Shape) -> Shape {
    let entry = link.entry();
    // A window gives each row of its group the value it gives the group, so one value over a
    // window is as many values as the table has rows, unless the window explodes, which gives
    // one row for each group, and a partitioned one as many as the groups of the rows.
    if entry.id().as_str() == "Expr.over" && shape == Shape::One {
        let explodes = matches!(
            link.argument("mapping_strategy"),
            Some(Value::Choice(word)) if word == "explode"
        );
        // With nothing to partition by the table is one group, so the window gives one value;
        // with a partition it gives one for each group, as many as the rows decide.
        let partitions = ["partition_by", "more_exprs"].iter().any(|name| {
            matches!(link.argument(name), Some(Value::Many(items)) if !items.is_empty())
                || matches!(link.argument(name), Some(Value::Expression(_)))
        });
        return if explodes && !partitions {
            shape
        } else {
            Shape::Many
        };
    }
    // Polars meets a receiver of one value with each value of an operand of many, whatever the
    // parameter is: a fact about the method, which the catalogue holds apart from how a number
    // typed without a dtype adapts. What a replacement lists is a table of values, which gives no
    // more values than what it follows, and a shift takes one value only: neither broadcasts.
    let given_many = arguments
        .iter()
        .any(|(name, operand)| operand.shape == Shape::Many && broadcasts(entry, name));
    if shape == Shape::One && given_many {
        Shape::Many
    } else {
        shape
    }
}

/// Whether the call `entry` is given an expression that gives many values for a parameter that
/// takes as many as another, or one, where that is one value: `new` beside an `old` of one, and a
/// strict replacement's `default` beside a value of one. An expression that was not walked to its
/// end has no shape that can be counted on, and is not judged.
fn gives_other_numbers_of_values(
    entry: &Entry,
    arguments: &[(String, Operand)],
    receiver: Shape,
) -> bool {
    let operand = |name: &str| {
        arguments
            .iter()
            .find(|(given, _)| given == name)
            .map(|(_, operand)| operand)
            .filter(|operand| operand.known && operand.counted)
    };
    entry.parameters().iter().any(|parameter| {
        let gives_many = parameter.as_many_as().is_some()
            && operand(parameter.name()).is_some_and(|mine| mine.shape == Shape::Many);
        if !gives_many {
            return false;
        }
        match parameter.as_many_as() {
            Some(AsManyAs::TheReceiver) => receiver == Shape::One,
            Some(AsManyAs::Parameter(other)) => {
                operand(other).is_some_and(|other| other.shape == Shape::One)
            }
            None => false,
        }
    })
}

// ---------------------------------------------------------------------------
// Keys

/// The expressions a call is given.
fn operands(call: &Call) -> Vec<&Expression> {
    fn collect<'a>(value: &'a Value, found: &mut Vec<&'a Expression>) {
        match value {
            Value::Expression(expression) => found.push(expression),
            Value::Many(items) => items.iter().for_each(|item| collect(item, found)),
            Value::Pairs(pairs) => pairs.iter().for_each(|(key, entry)| {
                collect(key, found);
                collect(entry, found);
            }),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for given in call.arguments() {
        collect(given.value(), &mut found);
    }
    found
}

pub(super) const ONE_VALUE: [&str; 11] = [
    "Expr.sum",
    "Expr.min",
    "Expr.max",
    "Expr.len",
    "Expr.first",
    "Expr.last",
    "Expr.has_nulls",
    "Expr.null_count",
    "Expr.n_unique",
    "Expr.count",
    "Expr.str.join",
];

fn is_one_value(call: &Call) -> bool {
    ONE_VALUE.contains(&call.entry().id().as_str())
}

/// Whether the first `upto` links of `expression` give one value and nothing
/// beside them does not: an aggregate among them with only single values after
/// it; with `literals`, a start made of literals too; or a condition whose parts
/// are single.
fn wholly_single(expression: &Expression, upto: usize, literals: bool) -> bool {
    let links = &expression.links()[..upto.min(expression.links().len())];
    let single = |call: &Call| {
        // What a replacement lists is a table of values, which gives no more values than what it
        // follows, and its default is as many as that or one.
        is_a_replacement(call)
            // An interval with a null bound that has no dtype is folded whole into a null, and
            // what its other bound reads is not met.
            || (call.entry().id().as_str() == "Expr.is_between"
                && operands(call).into_iter().any(is_an_untyped_null))
            || operands(call)
                .into_iter()
                .all(|inside| wholly_single(inside, inside.links().len(), true))
    };
    if let Some(last) = links.iter().rposition(is_one_value) {
        return links[last + 1..].iter().all(single);
    }
    match expression.start() {
        Start::Call(start) if start.entry().id().as_str() == "polars.lit" => {
            literals && links.iter().all(single)
        }
        Start::Call(start) if start.entry().id().as_str() == "polars.when" => {
            single(start) && links.iter().all(single)
        }
        // A function that makes a moment of what it is given is one value where each of them is.
        Start::Call(start) if takes_expressions(start) => single(start) && links.iter().all(single),
        _ => false,
    }
}

/// Whether `start` is a function that starts an expression and takes expressions, as the ones
/// that make a date, a moment or a length of time do: it gives as many values as its widest
/// argument.
fn takes_expressions(start: &Call) -> bool {
    start.entry().applies() == Applies::Function
        && start
            .entry()
            .parameters()
            .iter()
            .any(|parameter| matches!(parameter.kind(), Kind::Expression(_)))
}

/// Whether `link` is a comparison, which with a null literal gives a null. `is_between` is two: the
/// value compared with each of its bounds, so a bound that is a null literal with no dtype is a
/// comparison with one. A null bound of a dtype is one Polars folds for some dtypes of what it
/// bounds and not for others, which the document does not decide, so it is left to a limit.
/// `eq_missing` and `ne_missing` are not comparisons: they take a null as a value.
fn is_a_comparison(link: &Call) -> bool {
    is_a_comparison_of_its_receiver(link)
        || (link.entry().id().as_str() == "Expr.is_between"
            && operands(link).into_iter().any(is_an_untyped_null))
}

/// Whether an expression is a null literal and no more, which is given no dtype.
fn is_an_untyped_null(expression: &Expression) -> bool {
    is_a_null(expression)
        && matches!(expression.start(), Start::Call(start) if start.argument("dtype").is_none())
}

/// Whether `link` compares what it follows with what it is given, so that a null literal it
/// follows, and not only one it is given, gives a null: the six comparisons do, and `is_between`,
/// which compares each of its bounds with what it follows, does not.
fn is_a_comparison_of_its_receiver(link: &Call) -> bool {
    Comparison::of_entry(link.entry().id().as_str()).is_some()
}

/// Whether `link` is the expression it follows, as `and_` and `or_` are where they are given no
/// operand: Polars does not make a node of the call, so what comes after it meets what comes
/// before it.
fn is_the_expression_it_follows(link: &Call) -> bool {
    matches!(link.entry().id().as_str(), "Expr.and_" | "Expr.or_")
        && match link.argument("others") {
            None => true,
            Some(Value::Many(items)) => items.is_empty(),
            Some(_) => false,
        }
}

/// Whether the calls of `expression` before the link `at` leave its start as it was, for Polars
/// does not make a node of a call that is the expression it follows, and simplifies a not of a
/// not away: the link `at` is then the first it meets after the start.
fn follows_the_start(expression: &Expression, at: usize) -> bool {
    let mut nots = 0_usize;
    for link in &expression.links()[..at] {
        if link.entry().id().as_str() == "Expr.not_" {
            nots += 1;
        } else if !is_the_expression_it_follows(link) {
            return false;
        }
    }
    nots.is_multiple_of(2)
}

/// The calls of an expression: its start, then its links.
fn calls_of(expression: &Expression) -> impl Iterator<Item = &Call> {
    std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) | Start::Hole => None,
        })
        .chain(expression.links())
}

/// Whether an expression holds, at any depth, a comparison with a null literal:
/// one given it, or one that follows it at the start. Polars folds it into a
/// null.
fn folds_a_null(expression: &Expression) -> bool {
    let from_a_null = starts_from_a_null(expression);
    expression.links().iter().enumerate().any(|(at, link)| {
        (is_a_comparison_of_its_receiver(link) && from_a_null && follows_the_start(expression, at))
            || (is_a_comparison(link) && operands(link).into_iter().any(is_a_null))
    }) || calls_of(expression).any(|call| operands(call).into_iter().any(folds_a_null))
}

/// Whether a null fold is in the first `upto` calls of an expression, or in what
/// they are given.
fn folds_a_null_before(expression: &Expression, upto: usize) -> bool {
    let from_a_null = starts_from_a_null(expression);
    let links = &expression.links()[..upto];
    links.iter().enumerate().any(|(at, link)| {
        (is_a_comparison_of_its_receiver(link) && from_a_null && follows_the_start(expression, at))
            || (is_a_comparison(link) && operands(link).into_iter().any(is_a_null))
    }) || calls_of(expression)
        .take(upto + 1)
        .any(|call| operands(call).into_iter().any(folds_a_null))
}

/// Whether the comparison `at` of an expression is one that a null reaches:
/// Polars folds it into a null. It is reached where it is given a null or what
/// folds into one, follows a fold, or starts from a null. What Polars folds
/// depends on dtypes, so any fold that can reach a comparison counts.
fn is_reached_by_a_null(expression: &Expression, at: usize) -> bool {
    is_a_comparison(&expression.links()[at])
        && (operands(&expression.links()[at])
            .into_iter()
            .any(|operand| is_a_null(operand) || folds_a_null(operand))
            || (is_a_comparison_of_its_receiver(&expression.links()[at])
                && starts_from_a_null(expression)
                && follows_the_start(expression, at))
            || folds_a_null_before(expression, at))
}

/// Whether an expression holds an aggregate that Polars evaluates. See
/// [`holds_what_polars_evaluates`]. Where a join's key is judged elementwise a subtraction with a
/// null drops nothing: the key `k != f.len() - null` is not elementwise.
fn holds_an_aggregate(expression: &Expression) -> bool {
    holds_what_polars_evaluates(expression, &is_one_value)
}

/// Whether an expression holds a call that `is_held` says yes to and that Polars evaluates, at any
/// depth. A comparison that a null reaches is a null. The first of them drops what it follows and
/// is given where that is one value or literals alone, and the comparisons after it compare a
/// null, so they drop only what they are given where it is one value or literals alone, except
/// one that is given a null, which drops what it follows too where that is one value or literals
/// alone: what comes before the last that does is not evaluated. A side that is more is
/// evaluated, and what it holds counts.
fn holds_what_polars_evaluates(expression: &Expression, is_held: &dyn Fn(&Call) -> bool) -> bool {
    let evaluated = |call: &Call| {
        is_held(call)
            || operands(call)
                .into_iter()
                .any(|operand| holds_what_polars_evaluates(operand, is_held))
    };
    let start = match expression.start() {
        Start::Call(call) => Some(call),
        Start::Input(_) | Start::Hole => None,
    };
    let links = expression.links();
    let Some(first) = (0..links.len()).find(|at| is_reached_by_a_null(expression, *at)) else {
        return start.into_iter().chain(links).any(evaluated);
    };
    // A comparison that is given a null drops what it follows where that is one
    // value or literals alone, as the first fold does, so what a later one drops
    // is not evaluated either: the last of them that does is where the key is
    // judged from.
    let first = (first..links.len())
        .rev()
        .find(|at| {
            is_a_comparison(&links[*at])
                && operands(&links[*at]).into_iter().any(is_a_null)
                && wholly_single(expression, *at, true)
        })
        .unwrap_or(first);
    // What the first comparison follows is the start and the links before it,
    // dropped as a whole where they are one value or literals alone.
    let followed = !wholly_single(expression, first, true)
        && start.into_iter().chain(&links[..first]).any(evaluated);
    // What a comparison that a null reaches is given is dropped where it is one
    // value or literals alone, and what is not reached is evaluated.
    let after = links[first..].iter().enumerate().any(|(at, link)| {
        if is_reached_by_a_null(expression, first + at) {
            // An interval with a null bound is folded whole, so what its other bound holds is
            // not met, where a comparison evaluates what it is given that is more than one value.
            link.entry().id().as_str() != "Expr.is_between"
                && operands(link).into_iter().any(|operand| {
                    !wholly_single(operand, operand.links().len(), true)
                        && holds_what_polars_evaluates(operand, is_held)
                })
        } else {
            evaluated(link)
        }
    });
    followed || after
}

/// Whether an expression reads a column: it starts from one, or a call in it is
/// given an expression that does.
fn reads_a_column(expression: &Expression) -> bool {
    let calls = std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) | Start::Hole => None,
        })
        .chain(expression.links());
    matches!(
        expression.start(),
        Start::Call(start) if start.entry().id().as_str() == "polars.col"
    ) || calls
        .into_iter()
        .any(|call| operands(call).into_iter().any(reads_a_column))
}

/// Whether an expression is a null literal and no more, as it is where only calls that leave it
/// as it was come after it: calls that are the expression they follow, and nots that undo each
/// other.
fn is_a_null(expression: &Expression) -> bool {
    follows_the_start(expression, expression.links().len()) && starts_from_a_null(expression)
}

/// Whether an expression starts from a null literal.
fn starts_from_a_null(expression: &Expression) -> bool {
    matches!(
        expression.start(),
        Start::Call(start)
            if start.entry().id().as_str() == "polars.lit"
                && matches!(start.argument("value"), Some(Value::Flag(None)))
    )
}

/// Whether an expression holds, at any depth, a comparison of a null literal
/// with what reads a column and is not one value. Polars folds the comparison
/// into a null, and a key with a null where a column was is not elementwise. A
/// comparison with what an earlier one folded is left to a limit: what Polars
/// does with it depends on dtypes that the document does not decide.
fn holds_a_null_comparison(expression: &Expression) -> bool {
    let start = match expression.start() {
        Start::Call(call) => Some(call),
        Start::Input(_) | Start::Hole => None,
    };
    let mut reads = matches!(
        start,
        Some(start) if start.entry().id().as_str() == "polars.col"
    ) || start.is_some_and(|start| operands(start).into_iter().any(reads_a_column));
    // A comparison with a null that follows one value or literals alone folds into a null and
    // drops what it follows, the calls before it and what they are given, unevaluated: a
    // comparison of a column with a null among them is not one Polars meets.
    let dropped = expression
        .links()
        .iter()
        .enumerate()
        .filter(|(at, link)| {
            is_a_comparison(link)
                && (operands(link).into_iter().any(is_a_null)
                    || (is_a_comparison_of_its_receiver(link)
                        && starts_from_a_null(expression)
                        && follows_the_start(expression, *at)))
                && wholly_single(expression, *at, true)
        })
        .map(|(at, _)| at + usize::from(start.is_some()))
        .max()
        .unwrap_or(0);
    for (at, link) in expression.links().iter().enumerate() {
        // What a later fold drops is not met, so the comparisons before the last of them, which
        // are among the calls it drops, are not asked about.
        if at + usize::from(start.is_some()) < dropped {
            reads |= operands(link).into_iter().any(reads_a_column);
            continue;
        }
        if is_a_comparison(link) {
            let given = operands(link);
            let null_given = given.iter().any(|operand| is_a_null(operand));
            let column_given = given.iter().any(|operand| {
                reads_a_column(operand) && !wholly_single(operand, operand.links().len(), true)
            });
            let column_before = reads && !wholly_single(expression, at, true);
            let null_start = is_a_comparison_of_its_receiver(link)
                && follows_the_start(expression, at)
                && starts_from_a_null(expression);
            if (null_given && column_before) || (null_start && column_given) {
                return true;
            }
        }
        reads |= operands(link).into_iter().any(reads_a_column);
    }
    // A comparison with a null is a null whatever it is given, so what it is given that is one
    // value or literals alone is not evaluated either, and a comparison of a column with a null
    // in it is not one Polars meets.
    let folds = |at: usize, link: &Call| {
        is_a_comparison(link)
            && (operands(link).into_iter().any(is_a_null)
                || (is_a_comparison_of_its_receiver(link)
                    && starts_from_a_null(expression)
                    && follows_the_start(expression, at)))
    };
    start
        .into_iter()
        .map(|call| (call, false))
        .chain(
            expression
                .links()
                .iter()
                .enumerate()
                .map(|(at, link)| (link, folds(at, link))),
        )
        .skip(dropped)
        .any(|(call, folded)| {
            operands(call)
                .into_iter()
                .filter(|operand| !(folded && wholly_single(operand, operand.links().len(), true)))
                .any(holds_a_null_comparison)
        })
}

/// Whether a key is not elementwise, as Polars takes it: it holds an aggregate
/// beside what is not one value, or a column compared with a null. A key that is
/// wholly one value is the whole key, which the tables measured, and Polars
/// allows it. It takes the null literal beside a column as elementwise, and an
/// aggregate as not. The second expression is the key with each column of the dtype Null made the
/// null literal, which only the comparison with a null reads.
fn is_not_elementwise(key: &Expression, with_nulls_for_null_columns: &Expression) -> bool {
    !wholly_single(key, key.links().len(), true)
        && !holds_a_comparison_that_polars_may_fold(key)
        && (holds_an_aggregate(key)
            || holds_a_null_comparison(with_nulls_for_null_columns)
            || holds_a_replacement_of_columns(key))
}

/// `expression` with each column of the dtype Null made a literal, which reads no column, in the
/// calls it holds and what they are given: every value of such a column is a null, so what Polars
/// compares it with a null or with a column leaves no value of it to be replaced. `None` when
/// there is no such column in it, or an expression cannot be rebuilt.
fn with_null_columns_as_literals(expression: &Expression, schema: &Schema) -> Option<Expression> {
    fn value_of(value: &Value, schema: &Schema, changed: &mut bool) -> Option<Value> {
        Some(match value {
            Value::Expression(inside) => Value::Expression(expression_of(inside, schema, changed)?),
            Value::Many(items) => Value::Many(
                items
                    .iter()
                    .map(|item| value_of(item, schema, changed))
                    .collect::<Option<Vec<_>>>()?,
            ),
            Value::Pairs(pairs) => Value::Pairs(
                pairs
                    .iter()
                    .map(|(key, entry)| {
                        Some((
                            value_of(key, schema, changed)?,
                            value_of(entry, schema, changed)?,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?,
            ),
            other => other.clone(),
        })
    }
    fn call_of(call: &Call, schema: &Schema, changed: &mut bool) -> Option<Call> {
        if call.entry().id().as_str() == "polars.col"
            && let Some(Value::Columns(names)) = call.argument("name")
            && let [name] = names.as_slice()
            && schema
                .get(name)
                .is_some_and(|dtype| matches!(dtype, polars::prelude::DataType::Null))
        {
            *changed = true;
            return Call::new(
                crate::catalog::entry("polars.lit")?,
                [("value", Value::Integer(1))],
            )
            .ok();
        }
        let given = call
            .arguments()
            .iter()
            .map(|given| Some((given.parameter(), value_of(given.value(), schema, changed)?)))
            .collect::<Option<Vec<_>>>()?;
        Call::new(call.entry(), given).ok()
    }
    fn expression_of(
        expression: &Expression,
        schema: &Schema,
        changed: &mut bool,
    ) -> Option<Expression> {
        let start = match expression.start() {
            Start::Call(call) => Start::Call(call_of(call, schema, changed)?),
            other => other.clone(),
        };
        let links = expression
            .links()
            .iter()
            .map(|link| call_of(link, schema, changed))
            .collect::<Option<Vec<_>>>()?;
        Expression::new(start, links).ok()
    }
    let mut changed = false;
    let rebuilt = expression_of(expression, schema, &mut changed)?;
    changed.then_some(rebuilt)
}

/// Whether an expression holds, at any depth, a comparison that Polars may fold: an interval with
/// a null bound of a dtype, which it folds into a null for some dtypes of what it bounds and not
/// for others, or with a negative whole number as a bound, which it decides at once where what it
/// bounds is unsigned, as a count is, and keeps where it is not; or an `eq_missing` or an
/// `ne_missing` of a value and a whole number that some dtype cannot hold, which it decides at
/// once where the value's dtype cannot hold it, as a count's cannot hold a number below zero, and
/// keeps where it can. The other comparisons are kept whatever the number is. The fold leaves
/// nothing in the key that is not elementwise. The document does not decide which dtype it is, so
/// what a key that holds one is judged to be is left to a limit.
fn holds_a_comparison_that_polars_may_fold(expression: &Expression) -> bool {
    let starts_from_a_number = matches!(expression.start(), Start::Call(start)
        if start.entry().id().as_str() == "polars.lit"
            && matches!(start.argument("value"), Some(Value::Integer(number)) if some_dtype_lacks(*number)));
    expression.links().iter().enumerate().any(|(at, link)| {
        is_a_comparison_of_missing_values(link)
            && (operands(link).into_iter().any(is_a_number_some_dtype_lacks)
                || (starts_from_a_number && follows_the_start(expression, at)))
    }) || calls_of(expression).any(|call| {
        (call.entry().id().as_str() == "Expr.is_between"
            && operands(call).into_iter().any(|operand| {
                (is_a_null(operand)
                    && matches!(operand.start(), Start::Call(start) if start.argument("dtype").is_some()))
                    || is_a_negative_whole_number(operand)
            }))
            || operands(call)
                .into_iter()
                .any(holds_a_comparison_that_polars_may_fold)
    })
}

/// Whether `link` is an `eq_missing` or an `ne_missing`, which compare a missing value as a
/// value.
fn is_a_comparison_of_missing_values(link: &Call) -> bool {
    matches!(
        link.entry().id().as_str(),
        "Expr.eq_missing" | "Expr.ne_missing"
    )
}

/// Whether a whole number is one that some dtype cannot hold: one below zero, which no unsigned
/// dtype holds, or above 127, which an `Int8` does not. A number from zero to 127 is held by
/// every dtype, and Polars never decides a comparison with one at once.
fn some_dtype_lacks(number: i64) -> bool {
    !(0..=127).contains(&number)
}

/// Whether an expression is a whole number that some dtype cannot hold and no more.
fn is_a_number_some_dtype_lacks(expression: &Expression) -> bool {
    expression.links().is_empty()
        && matches!(expression.start(), Start::Call(start)
            if start.entry().id().as_str() == "polars.lit"
                && matches!(start.argument("value"), Some(Value::Integer(number)) if some_dtype_lacks(*number)))
}

/// Whether an expression is a whole number below zero and no more.
fn is_a_negative_whole_number(expression: &Expression) -> bool {
    expression.links().is_empty()
        && matches!(expression.start(), Start::Call(start)
            if start.entry().id().as_str() == "polars.lit"
                && matches!(start.argument("value"), Some(Value::Integer(number)) if *number < 0))
}

/// Whether `call` is a replacement of values.
fn is_a_replacement(call: &Call) -> bool {
    matches!(
        call.entry().id().as_str(),
        "Expr.replace" | "Expr.replace_strict"
    )
}

/// Whether an expression holds, at any depth, a replacement given what it replaces, or what
/// replaces it, that reads a column, and that Polars evaluates. Polars calls a replacement
/// elementwise only where they are literals, which is how one is written, and refuses a join's
/// key that is given a column. Its default may be a column, which it takes a value of for each
/// row. A comparison with a null that Polars folds drops what it follows and is given where
/// that is one value, as it does an aggregate, so a replacement there is never met.
fn holds_a_replacement_of_columns(expression: &Expression) -> bool {
    holds_what_polars_evaluates(expression, &|call| {
        is_a_replacement(call)
            && ["old", "new"].iter().any(|name| {
                matches!(call.argument(name), Some(Value::Expression(value)) if reads_a_column(value))
            })
    })
}
