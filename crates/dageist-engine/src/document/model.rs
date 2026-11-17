//! The step document as a value: steps, the calls they are made of, and the
//! arguments of those calls.
//!
//! Every value here has been checked on the way in, whether it was read from
//! text or put together in code: an entry stands only where it can, its
//! arguments are accepted by [`Entry::arguments`], and the links of a
//! condition follow one another in the order Polars needs. What can only be
//! known with tables, such as whether a column exists, is left to running the
//! document.

use std::fmt;

use crate::catalog::{Argument, Build, Entry, Kind, Parameter, Shape, ValueType};

use super::dtype::Dtype;
use super::error::{Cause, Place, Position, Refusal, Segment};
use super::{MAX_NESTING, write};

/// The identifier of a step: its name in the document, which no other step
/// has.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StepId(String);

/// A step identifier with no characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyStepId;

impl fmt::Display for EmptyStepId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a step needs an identifier")
    }
}

impl std::error::Error for EmptyStepId {}

impl StepId {
    /// The identifier `text`.
    ///
    /// # Errors
    ///
    /// When `text` is empty.
    pub fn new(text: impl Into<String>) -> Result<Self, EmptyStepId> {
        let text = text.into();
        if text.is_empty() {
            Err(EmptyStepId)
        } else {
            Ok(Self(text))
        }
    }

    /// The identifier as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StepId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One argument of a call, as a document holds it. Each variant is what
/// [`Argument`] holds for the same kind, except that a table and an expression
/// are held as the names and calls they are made from.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A chain: where it starts, then methods.
    Expression(Expression),
    /// Names of input columns.
    Columns(Vec<String>),
    /// A whole number.
    Integer(i64),
    /// A number. It is finite.
    Float(f64),
    /// Text.
    String(String),
    /// True, false, or unset, which leaves the choice to Polars.
    Flag(Option<bool>),
    /// One word of a closed set.
    Choice(String),
    /// A dtype.
    Dtype(Dtype),
    /// A Polars duration string, such as `1mo`.
    Duration(String),
    /// The name of a connected table.
    Table(String),
    /// A file, folder or cloud location.
    Path(String),
    /// Key and value entries.
    Pairs(Vec<(Value, Value)>),
    /// An ordered list.
    Many(Vec<Value>),
}

impl Value {
    /// What [`Entry::arguments`] is asked about this value: an argument of the
    /// same kind, with nothing behind a table or an expression.
    fn placeholder(&self) -> Argument {
        match self {
            Self::Expression(_) => {
                Argument::Expression(polars::prelude::lit(polars::prelude::NULL))
            }
            Self::Columns(names) => Argument::Columns(names.clone()),
            Self::Integer(value) => Argument::Integer(*value),
            Self::Float(value) => Argument::Float(*value),
            Self::String(text) => Argument::String(text.clone()),
            Self::Flag(flag) => Argument::Flag(*flag),
            Self::Choice(word) => Argument::Choice(word.clone()),
            Self::Dtype(dtype) => Argument::Dtype(dtype.as_polars().clone()),
            Self::Duration(text) => Argument::Duration(text.clone()),
            Self::Table(_) => Argument::Table(polars::prelude::LazyFrame::default()),
            Self::Path(text) => Argument::Path(text.clone()),
            Self::Pairs(pairs) => Argument::Pairs(
                pairs
                    .iter()
                    .map(|(key, value)| (key.placeholder(), value.placeholder()))
                    .collect(),
            ),
            Self::Many(items) => Argument::Many(items.iter().map(Self::placeholder).collect()),
        }
    }
}

/// The argument given for one parameter of a call.
#[derive(Debug, Clone, PartialEq)]
pub struct Given {
    parameter: String,
    value: Value,
}

impl Given {
    /// Python's name for the parameter.
    #[must_use]
    pub fn parameter(&self) -> &str {
        &self.parameter
    }

    /// The argument.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }
}

/// One use of an entry of the catalogue, with the arguments it was given. A
/// parameter left out takes its default.
#[derive(Debug, Clone)]
pub struct Call {
    entry: &'static Entry,
    arguments: Vec<Given>,
}

impl PartialEq for Call {
    fn eq(&self, other: &Self) -> bool {
        self.entry.id() == other.entry.id() && self.arguments == other.arguments
    }
}

impl Call {
    /// A use of `entry` with `arguments`, each given with the name of its
    /// parameter.
    ///
    /// # Errors
    ///
    /// When [`Entry::arguments`] refuses them, or a number is not finite. The
    /// place is the argument, down to the item or entry that is wrong.
    pub fn new<'a>(
        entry: &'static Entry,
        arguments: impl IntoIterator<Item = (&'a str, Value)>,
    ) -> Result<Self, Refusal> {
        let given: Vec<(&str, Value)> = arguments.into_iter().collect();
        entry
            .arguments(
                given
                    .iter()
                    .map(|(name, value)| (*name, value.placeholder())),
            )
            .map_err(|error| arguments_refused(entry, &given, error))?;
        for (name, value) in &given {
            if let Some(inside) = non_finite(value) {
                return Err(Refusal::new(Cause::NotFinite)
                    .under_all(inside)
                    .under(Segment::Parameter((*name).to_owned())));
            }
        }
        Ok(Self {
            entry,
            arguments: given
                .into_iter()
                .map(|(name, value)| Given {
                    parameter: name.to_owned(),
                    value,
                })
                .collect(),
        })
    }

    /// The entry the call uses.
    #[must_use]
    pub const fn entry(&self) -> &'static Entry {
        self.entry
    }

    /// The arguments, in the order they were given.
    #[must_use]
    pub fn arguments(&self) -> &[Given] {
        &self.arguments
    }

    /// The argument given for `parameter`, if there is one.
    #[must_use]
    pub fn argument(&self, parameter: &str) -> Option<&Value> {
        self.arguments
            .iter()
            .find(|given| given.parameter == parameter)
            .map(|given| &given.value)
    }
}

/// The refusal for the arguments `error` says [`Entry::arguments`] refused.
fn arguments_refused(
    entry: &'static Entry,
    given: &[(&str, Value)],
    error: crate::catalog::BuildError,
) -> Refusal {
    use crate::catalog::BuildError;
    let (name, inside) = match &error {
        BuildError::UnknownParameter { parameter } => (Some(parameter.clone()), Vec::new()),
        BuildError::Repeated { parameter } | BuildError::Missing { parameter } => {
            (Some((*parameter).to_owned()), Vec::new())
        }
        BuildError::WrongKind { parameter, .. } | BuildError::UnknownChoice { parameter, .. } => {
            let found = entry
                .parameters()
                .iter()
                .find(|known| known.name() == *parameter)
                .zip(given.iter().find(|(name, _)| name == parameter));
            (
                Some((*parameter).to_owned()),
                found.map_or_else(Vec::new, |(known, (_, value))| locate(known, value)),
            )
        }
        BuildError::OutOfOrder { .. } | BuildError::Polars(_) => (None, Vec::new()),
    };
    let refusal = Refusal::new(Cause::Arguments(error)).under_all(inside);
    match name {
        Some(name) => refusal.under(Segment::Parameter(name)),
        None => refusal,
    }
}

/// Whether `value` is one value of `kind`, as [`Entry::arguments`] decides it.
/// It only finds where a refused argument is wrong; the catalogue decides
/// whether it is.
fn fits(kind: Kind, value: &Value) -> bool {
    match (kind, value) {
        (Kind::Expression(_), Value::Expression(_))
        | (Kind::Column, Value::Columns(_))
        | (Kind::Value(ValueType::Integer), Value::Integer(_))
        | (Kind::Value(ValueType::Float), Value::Float(_))
        | (Kind::Value(ValueType::String), Value::String(_))
        | (
            Kind::Value(ValueType::Any),
            Value::Integer(_) | Value::Float(_) | Value::String(_) | Value::Flag(_),
        )
        | (Kind::Flag, Value::Flag(_))
        | (Kind::Dtype, Value::Dtype(_))
        | (Kind::Duration, Value::Duration(_))
        | (Kind::Table, Value::Table(_))
        | (Kind::Path, Value::Path(_)) => true,
        (Kind::Choice(words), Value::Choice(word)) => words.contains(&word.as_str()),
        (Kind::Pairs(key, entry), Value::Pairs(pairs)) => pairs
            .iter()
            .all(|(first, second)| fits(*key, first) && fits(*entry, second)),
        _ => false,
    }
}

/// The way from a parameter's argument to the item or entry of it that does
/// not fit.
fn locate(parameter: &Parameter, value: &Value) -> Vec<Segment> {
    match (parameter.shape(), value) {
        (Shape::Many | Shape::PerItem(_), Value::Many(items)) => items
            .iter()
            .position(|item| !fits(parameter.kind(), item))
            .map_or_else(Vec::new, |at| {
                let mut path = vec![Segment::Item(at)];
                path.extend(locate_in(parameter.kind(), &items[at]));
                path
            }),
        _ => locate_in(parameter.kind(), value),
    }
}

fn locate_in(kind: Kind, value: &Value) -> Vec<Segment> {
    let (Kind::Pairs(key, entry), Value::Pairs(pairs)) = (kind, value) else {
        return Vec::new();
    };
    for (at, (first, second)) in pairs.iter().enumerate() {
        if !fits(*key, first) {
            let mut path = vec![Segment::Key(at)];
            path.extend(locate_in(*key, first));
            return path;
        }
        if !fits(*entry, second) {
            let mut path = vec![Segment::Value(at)];
            path.extend(locate_in(*entry, second));
            return path;
        }
    }
    Vec::new()
}

/// The way to the first number in `value` that is not finite.
fn non_finite(value: &Value) -> Option<Vec<Segment>> {
    match value {
        Value::Float(number) if !number.is_finite() => Some(Vec::new()),
        Value::Many(items) => items.iter().enumerate().find_map(|(at, item)| {
            non_finite(item).map(|inside| prefixed(Segment::Item(at), inside))
        }),
        Value::Pairs(pairs) => pairs.iter().enumerate().find_map(|(at, (key, entry))| {
            non_finite(key)
                .map(|inside| prefixed(Segment::Key(at), inside))
                .or_else(|| non_finite(entry).map(|inside| prefixed(Segment::Value(at), inside)))
        }),
        _ => None,
    }
}

fn prefixed(segment: Segment, mut inside: Vec<Segment>) -> Vec<Segment> {
    inside.insert(0, segment);
    inside
}

impl Position {
    /// Whether `entry` can stand there.
    ///
    /// # Errors
    ///
    /// The cause that says it cannot.
    pub(super) fn check(self, entry: &'static Entry) -> Result<(), Cause> {
        let fits = matches!(
            (self, entry.build()),
            (Self::Step, Build::Frame(_))
                | (Self::Start, Build::Function(_) | Build::Condition(_))
                | (
                    Self::Link,
                    Build::Expression(_) | Build::ConditionStep(_) | Build::ConditionEnd(_),
                )
        );
        if fits {
            Ok(())
        } else {
            Err(Cause::WrongPlace {
                entry: entry.id().as_str(),
                applies: entry.applies(),
                position: self,
            })
        }
    }
}

/// Where an expression starts.
#[derive(Debug, Clone, PartialEq)]
pub enum Start {
    /// A function that makes an expression, such as `polars.col`, or
    /// `polars.when`, which starts a condition.
    Call(Call),
    /// A connected single value or list, by name, as a literal of its dtype.
    Input(String),
    /// Nothing yet: the editor has not picked what the expression starts
    /// from. A step that holds one is incomplete: it is walked up to the hole,
    /// and not built.
    Hole,
}

/// A chain: where it starts, then the methods and links of a condition that
/// follow.
#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    start: Start,
    links: Vec<Call>,
}

/// How far a chain has come: an expression, or a condition that waits for its
/// next link.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    Value,
    When,
    Then,
    ChainedWhen,
    ChainedThen,
}

impl Stage {
    /// The stage as Python's classes, and the catalogue's errors, name it.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Value => "an expression",
            Self::When => "when",
            Self::Then => "then",
            Self::ChainedWhen => "chained when",
            Self::ChainedThen => "chained then",
        }
    }

    /// The stage a link of a condition needs before it and the stage it leaves,
    /// which is [`Stage::Value`] where it ends the condition.
    pub(super) fn of_link(id: &str) -> Option<(Self, Self)> {
        Some(match id {
            "When.then" => (Self::When, Self::Then),
            "Then.when" => (Self::Then, Self::ChainedWhen),
            "Then.otherwise" => (Self::Then, Self::Value),
            "ChainedWhen.then" => (Self::ChainedWhen, Self::ChainedThen),
            "ChainedThen.when" => (Self::ChainedThen, Self::ChainedWhen),
            "ChainedThen.otherwise" => (Self::ChainedThen, Self::Value),
            _ => return None,
        })
    }
}

impl Step {
    /// The way to the first hole in the step, in the order the walk follows,
    /// from the step to the start of the expression that is one: `None` when
    /// the step holds none.
    #[must_use]
    pub fn first_hole(&self) -> Option<Vec<Segment>> {
        let mut path = Vec::new();
        hole_in_call(self.call(), &mut path).then_some(path)
    }
}

fn hole_in_call(call: &Call, path: &mut Vec<Segment>) -> bool {
    for given in call.arguments() {
        path.push(Segment::Parameter(given.parameter().to_owned()));
        if hole_in_value(given.value(), path) {
            return true;
        }
        path.pop();
    }
    false
}

fn hole_in_value(value: &Value, path: &mut Vec<Segment>) -> bool {
    match value {
        Value::Expression(expression) => hole_in_expression(expression, path),
        Value::Many(items) => items.iter().enumerate().any(|(at, item)| {
            path.push(Segment::Item(at));
            hole_in_value(item, path) || {
                path.pop();
                false
            }
        }),
        Value::Pairs(pairs) => pairs.iter().enumerate().any(|(at, (key, entry))| {
            path.push(Segment::Key(at));
            if hole_in_value(key, path) {
                return true;
            }
            path.pop();
            path.push(Segment::Value(at));
            hole_in_value(entry, path) || {
                path.pop();
                false
            }
        }),
        _ => false,
    }
}

fn hole_in_expression(expression: &Expression, path: &mut Vec<Segment>) -> bool {
    path.push(Segment::Start);
    match expression.start() {
        Start::Hole => return true,
        Start::Call(start) if hole_in_call(start, path) => return true,
        Start::Call(_) | Start::Input(_) => {}
    }
    path.pop();
    for (at, link) in expression.links().iter().enumerate() {
        path.push(Segment::Link(at));
        if hole_in_call(link, path) {
            return true;
        }
        path.pop();
    }
    false
}

impl Expression {
    /// The chain that starts with `start` and continues with `links`.
    ///
    /// # Errors
    ///
    /// When a call stands where it cannot, a link of a condition does not
    /// follow what comes before it, or the chain ends in a condition that
    /// still lacks its `then`. The place is the start or the link.
    pub fn new(start: Start, links: Vec<Call>) -> Result<Self, Refusal> {
        let mut stage = match &start {
            Start::Input(_) | Start::Hole => Stage::Value,
            Start::Call(call) => {
                Position::Start
                    .check(call.entry())
                    .map_err(|cause| Refusal::new(cause).under(Segment::Start))?;
                if matches!(call.entry().build(), Build::Condition(_)) {
                    Stage::When
                } else {
                    Stage::Value
                }
            }
        };
        for (at, link) in links.iter().enumerate() {
            let refused = |cause| Refusal::new(cause).under(Segment::Link(at));
            let entry = link.entry();
            Position::Link.check(entry).map_err(refused)?;
            let out_of_order = |needs| Cause::OutOfOrder {
                entry: entry.id().as_str(),
                needs,
                found: stage.name(),
            };
            stage = match entry.build() {
                Build::Expression(_) => match stage {
                    Stage::Value | Stage::Then | Stage::ChainedThen => Stage::Value,
                    Stage::When | Stage::ChainedWhen => {
                        return Err(refused(out_of_order("then")));
                    }
                },
                _ => match Stage::of_link(entry.id().as_str()) {
                    Some((needs, leaves)) if needs == stage => leaves,
                    Some((needs, _)) => return Err(refused(out_of_order(needs.name()))),
                    None => return Err(refused(out_of_order("known link of a condition"))),
                },
            };
        }
        if matches!(stage, Stage::When | Stage::ChainedWhen) {
            let last = links
                .len()
                .checked_sub(1)
                .map_or(Segment::Start, Segment::Link);
            return Err(Refusal::new(Cause::UnfinishedCondition { needs: "then" }).under(last));
        }
        Ok(Self { start, links })
    }

    /// Where the chain starts.
    #[must_use]
    pub const fn start(&self) -> &Start {
        &self.start
    }

    /// The methods and links after the start, in order.
    #[must_use]
    pub fn links(&self) -> &[Call] {
        &self.links
    }
}

/// One step of the document: a method of a lazy frame, under a name.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    id: StepId,
    call: Call,
}

impl Step {
    /// The step `id` that makes `call`.
    ///
    /// # Errors
    ///
    /// When the call's entry is not a method of a lazy frame.
    pub fn new(id: StepId, call: Call) -> Result<Self, Refusal> {
        Position::Step.check(call.entry()).map_err(Refusal::new)?;
        Ok(Self { id, call })
    }

    /// The step's name.
    #[must_use]
    pub const fn id(&self) -> &StepId {
        &self.id
    }

    /// What the step does.
    #[must_use]
    pub const fn call(&self) -> &Call {
        &self.call
    }
}

/// A list of steps, each building on the table the one before gives, the
/// first on the table the document is run on.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    steps: Vec<Step>,
}

impl Document {
    /// The document of `steps`.
    ///
    /// # Errors
    ///
    /// When two steps have the same identifier, or a step would be written
    /// with arrays and objects nested deeper than [`MAX_NESTING`] allows
    /// inside the document. The place is the later step, or the deep one.
    pub fn new(steps: Vec<Step>) -> Result<Self, Refusal> {
        for (at, step) in steps.iter().enumerate() {
            // The document's object and its `steps` array are open around the step.
            if write::depth_within(step, MAX_NESTING - 2).is_none() {
                return Err(Refusal::at(
                    Place::of_step(at, Some(step.id.clone())),
                    Cause::TooDeep,
                ));
            }
            if steps[..at].iter().any(|before| before.id == step.id) {
                return Err(Refusal::at(
                    Place::of_step(at, Some(step.id.clone())),
                    Cause::DuplicateStep(step.id.clone()),
                ));
            }
        }
        Ok(Self { steps })
    }

    /// The steps, in the order they run.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
}
