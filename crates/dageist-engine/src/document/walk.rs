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
//! join's key whether it holds an aggregate beside a column. The options a step is
//! given are asked of [`allowed::options`].
//!
//! The tables are measured, so what they cannot say is not guessed: a chain
//! whose dtype is not known where a method follows it is not asked from there
//! on, and the walk says it cannot tell.

use polars::prelude::{Schema, SchemaRef};

use crate::allowed::{
    self, After, Chosen, ConditionState, Context, Ending, Nested, Outer, Place as OfferPlace,
    Position as Slot, Refusal, Representative, Shape, Unknown, Verdict,
};
use crate::catalog::{Applies, Kind};

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
        let other = self
            .call()
            .argument("other")
            .and_then(|value| match value {
                Value::Table(name) => inputs.table(name),
                _ => None,
            })
            .and_then(|mut table| table.collect_schema().ok());
        let mut walker = Walker {
            step: self.call(),
            source,
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
    shape: Shape,
    /// Whether the expression was walked to its end: a refused or unwalked one
    /// has no shape either.
    known: bool,
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
        values: Vec<Option<Representative>>,
        /// Whether every predicate and value so far gives one value, which a
        /// condition of them gives too.
        single: bool,
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
    other: Option<SchemaRef>,
    base: Place,
    path: Vec<Segment>,
    slots: Vec<Slot>,
    walked: Vec<WalkedExpression>,
    refusal: Option<WalkRefusal>,
    not_known: Option<String>,
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

/// The options `call` was given, each by the parameter and its word as the
/// option table writes it.
fn option_words(call: &Call) -> Vec<(String, String)> {
    let entry = call.entry();
    call.arguments()
        .iter()
        .filter(|given| {
            entry.parameters().iter().any(|parameter| {
                parameter.name() == given.parameter()
                    && matches!(parameter.kind(), Kind::Flag | Kind::Choice(_))
            })
        })
        .filter_map(|given| {
            option_word(given.value()).map(|word| (given.parameter().to_owned(), word))
        })
        .collect()
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
        _ => None,
    }
}

/// The class every value of a condition has, when they have the same one. A
/// condition of values of different dtypes has the supertype, which the tables
/// do not give.
fn unify(values: &[Option<Representative>]) -> Option<Representative> {
    let first = (*values.first()?)?;
    values
        .iter()
        .all(|value| *value == Some(first))
        .then_some(first)
}

impl Walker<'_> {
    fn here(&self) -> Place {
        self.path
            .iter()
            .cloned()
            .fold(self.base.clone(), Place::then)
    }

    fn refuse(&mut self, refusal: &Refusal, at_end: bool) {
        if self.refusal.is_none() {
            self.refusal = Some(WalkRefusal {
                place: self.here(),
                refusal: refusal.clone(),
                at_end,
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
        let chosen = option_words(step);
        let chosen: Vec<(&str, &str)> = chosen
            .iter()
            .map(|(name, word)| (name.as_str(), word.as_str()))
            .collect();
        if let Err(refusal) = allowed::options(step.entry(), &chosen) {
            self.refuse(&refusal, false);
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
        let against_other = self.slots.first() == Some(&Slot::Outer(Outer::JoinRight));
        let schema = if against_other {
            self.other.as_deref()?
        } else {
            self.source
        };
        Representative::of(schema.get(names.first()?)?)
    }

    /// The class of what a pivot's group holds, which `element` stands for: the
    /// first column the pivot takes values from.
    fn group_of(&self) -> Option<Representative> {
        if self.step.entry().id().as_str() != "LazyFrame.pivot" {
            return None;
        }
        let Some(Value::Columns(names)) = self.step.argument("values") else {
            return None;
        };
        Representative::of(self.source.get(names.first()?)?)
    }

    /// The class a start gives, where its arguments decide it.
    fn start_class(&self, start: &Call) -> Option<Representative> {
        match start.entry().id().as_str() {
            "polars.col" => self.column_of(start),
            "polars.lit" => literal_class(start),
            "polars.element" => self.group_of(),
            _ => None,
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
            shape: Shape::Many,
            known: false,
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
            Start::Input(_) => self.blind("a connected input starts the chain"),
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
                let class = self.start_class(start);
                let chosen = Chosen::default();
                let chosen = match class {
                    Some(dtype) => chosen.giving(dtype),
                    None => chosen,
                };
                let verdict = allowed::offer(
                    start.entry(),
                    &OfferPlace::new(context, After::Start),
                    &chosen,
                );
                match &verdict {
                    Verdict::Refused(refusal) => self.refuse(refusal, false),
                    Verdict::Allowed(allowance) => {
                        last_end = Some(allowance.can_end().clone());
                        // An expansion has a dtype for each column, so whether it
                        // may end is not known, whatever the table measured.
                        if Self::names_several(start) && last_end == Some(Ending::Allowed) {
                            last_end = Some(Ending::NotKnown(Unknown::TheStartsDtype));
                        }
                        cur = if start.entry().id().as_str() == "polars.when" {
                            Cur::Condition {
                                state: ConditionState::When,
                                values: Vec::new(),
                                single: start_arguments
                                    .iter()
                                    .all(|(_, operand)| operand.shape == Shape::One),
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
            Start::Input(_) => {
                calls.push(Standing::not_asked());
                cur = Cur::Value {
                    dtype: None,
                    shape: Shape::Many,
                };
            }
        }
        for (at, link) in expression.links().iter().enumerate() {
            self.path.push(Segment::Link(at));
            let arguments = self.arguments(link, values_end);
            let (next, standing) = self.link(link, &arguments, context, cur, &mut last_end);
            cur = next;
            calls.push(standing);
            self.path.pop();
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
            if is_not_elementwise(expression)
                && let Err(refusal) = allowed::rule_forbids_mixed_key(context)
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
                shape,
                // What was not asked has no shape that can be counted on.
                known: dtype.is_some(),
            },
            Cur::Condition { values, .. } => Operand {
                dtype: unify(&values),
                shape: Shape::Many,
                known: true,
            },
            Cur::Dead => unsaid,
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
    ) -> (Cur, Standing) {
        let entry = link.entry();
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
        let given: Vec<(&str, Representative)> = arguments
            .iter()
            .filter_map(|(name, operand)| operand.dtype.map(|dtype| (name.as_str(), dtype)))
            .collect();
        let words = option_words(link);
        let words: Vec<(&str, &str)> = words
            .iter()
            .map(|(name, word)| (name.as_str(), word.as_str()))
            .collect();
        let place = OfferPlace::new(context, after);
        // What the tables are not told is not guessed: an argument whose dtype
        // is not known leaves the call to be offered for any dtype that fits,
        // which says it stands, not that this argument fits.
        if given.len() < arguments.len() {
            self.blind(format!(
                "the dtype of an argument of {} is not known",
                entry.id().as_str()
            ));
            *last_end = None;
            // The call is still offered, for a refusal is a refusal whatever
            // the argument, but nothing is said of what it gives.
            let verdict = allowed::offer(
                entry,
                &place,
                &Chosen::default()
                    .with_arguments(&given)
                    .with_options(&words),
            );
            let standing = Standing {
                follows: Some(after),
                verdict: Some(verdict.clone()),
            };
            if let Verdict::Refused(refusal) = &verdict {
                self.refuse(refusal, false);
                return (Cur::Dead, standing);
            }
            return (
                Cur::Value {
                    dtype: None,
                    shape: Shape::Many,
                },
                standing,
            );
        }
        let statement = arguments
            .iter()
            .find(|(name, _)| name == "statement")
            .map(|(_, operand)| *operand);
        let statement_class = statement.and_then(|operand| operand.dtype);
        // Whether the predicates and the value this link is given give one
        // value.
        let given_known = arguments
            .iter()
            .filter(|(name, _)| matches!(name.as_str(), "predicates" | "statement"))
            .all(|(_, operand)| operand.known);
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
        let chosen = Chosen::default()
            .with_arguments(&given)
            .with_options(&words);
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
        *last_end = Some(allowance.can_end().clone());
        let next = match (entry.applies(), cur) {
            (Applies::Expression, _) => Cur::Value {
                dtype: allowance.result().and_then(allowed::ResultDtype::class),
                shape: allowance.shape(),
            },
            (
                Applies::Condition,
                Cur::Condition {
                    mut values,
                    single,
                    known,
                    ..
                },
            ) => {
                let single = single && given_single;
                let known = known && given_known;
                let state = match entry.id().as_str() {
                    "When.then" => {
                        values.push(statement_class);
                        ConditionState::Then
                    }
                    "ChainedWhen.then" => {
                        values.push(statement_class);
                        ConditionState::ChainedThen
                    }
                    "Then.when" | "ChainedThen.when" => ConditionState::ChainedWhen,
                    _ => {
                        values.push(statement_class);
                        return (
                            Cur::Value {
                                dtype: if known { unify(&values) } else { None },
                                shape: if single { Shape::One } else { Shape::Many },
                            },
                            standing,
                        );
                    }
                };
                Cur::Condition {
                    state,
                    values,
                    single,
                    known,
                }
            }
            _ => Cur::Dead,
        };
        (next, standing)
    }
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

const ONE_VALUE: [&str; 6] = [
    "Expr.sum",
    "Expr.min",
    "Expr.max",
    "Expr.len",
    "Expr.first",
    "Expr.last",
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
        operands(call)
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
        _ => false,
    }
}

/// The comparisons, which with a null literal give a null.
const COMPARISONS: [&str; 6] = [
    "Expr.eq", "Expr.ne", "Expr.gt", "Expr.ge", "Expr.lt", "Expr.le",
];

/// Whether `call` compares with a null literal.
fn compares_with_a_null(call: &Call) -> bool {
    COMPARISONS.contains(&call.entry().id().as_str())
        && operands(call).into_iter().any(|operand| {
            operand.links().is_empty()
                && matches!(
                    operand.start(),
                    Start::Call(start)
                        if start.entry().id().as_str() == "polars.lit"
                            && matches!(start.argument("value"), Some(Value::Flag(None)))
                )
        })
}

/// Whether an expression holds an aggregate. What a comparison with a null
/// literal follows is dropped, for the comparison is a null, and so is any
/// aggregate in it.
fn holds_an_aggregate(expression: &Expression) -> bool {
    let from = expression
        .links()
        .iter()
        .rposition(compares_with_a_null)
        .map_or(0, |at| at + 1);
    let links = &expression.links()[from..];
    let start = match expression.start() {
        Start::Call(call) if from == 0 => Some(call),
        _ => None,
    };
    links.iter().any(is_one_value)
        || start
            .into_iter()
            .chain(links)
            .any(|call| operands(call).into_iter().any(holds_an_aggregate))
}

/// Whether an expression reads a column: it starts from one, or a call in it is
/// given an expression that does.
fn reads_a_column(expression: &Expression) -> bool {
    let calls = std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) => None,
        })
        .chain(expression.links());
    matches!(
        expression.start(),
        Start::Call(start) if start.entry().id().as_str() == "polars.col"
    ) || calls
        .into_iter()
        .any(|call| operands(call).into_iter().any(reads_a_column))
}

/// Whether an expression is a null literal and no more.
fn is_a_null(expression: &Expression) -> bool {
    expression.links().is_empty() && starts_from_a_null(expression)
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
        Start::Input(_) => None,
    };
    let mut reads = matches!(
        start,
        Some(start) if start.entry().id().as_str() == "polars.col"
    ) || start.is_some_and(|start| operands(start).into_iter().any(reads_a_column));
    for (at, link) in expression.links().iter().enumerate() {
        if COMPARISONS.contains(&link.entry().id().as_str()) {
            let given = operands(link);
            let null_given = given.iter().any(|operand| is_a_null(operand));
            let column_given = given.iter().any(|operand| {
                reads_a_column(operand) && !wholly_single(operand, operand.links().len(), true)
            });
            let column_before = reads && !wholly_single(expression, at, true);
            let null_start = at == 0 && starts_from_a_null(expression);
            if (null_given && column_before) || (null_start && column_given) {
                return true;
            }
        }
        reads |= operands(link).into_iter().any(reads_a_column);
    }
    start
        .into_iter()
        .chain(expression.links())
        .any(|call| operands(call).into_iter().any(holds_a_null_comparison))
}

/// Whether a key is not elementwise, as Polars takes it: it holds an aggregate
/// beside what is not one value, or a column compared with a null. A key that is
/// wholly one value is the whole key, which the tables measured, and Polars
/// allows it. It takes the null literal beside a column as elementwise, and an
/// aggregate as not.
fn is_not_elementwise(key: &Expression) -> bool {
    !wholly_single(key, key.links().len(), true)
        && (holds_an_aggregate(key) || holds_a_null_comparison(key))
}
