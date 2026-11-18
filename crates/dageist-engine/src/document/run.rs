//! Running a document: building each step through the catalogue and saying,
//! for each, whether Polars accepts it and what it gives.
//!
//! Running reads no data. A step is built in Polars, its result schema comes
//! from `collect_schema`, and the dtype after each link of an expression comes
//! from building the expression up to that link against the schema the step
//! starts from. The one exception is a step that needs a collect: building it
//! collects there, as the step editor's settled choice is.

use std::fmt;

use polars::prelude::{
    DataFrame, DataType, Expr, IdxSize, IntoLazy, LazyFrame, PolarsError, SchemaRef,
};

use crate::allowed::{self, Context, Position, Refusal};
use crate::catalog::{Argument, Arguments, Build, BuildError, Conditional, Laziness};

use super::changes::ColumnChange;
use super::counts::{Counted, StepRows};
use super::error::{Place, Segment};
use super::inputs::Inputs;
use super::model::{Call, Document, Expression, Start, Step, StepId, Value};
use super::walk::Walk;

/// What an expression has, up to one of its links.
#[derive(Debug, Clone)]
pub enum Resolution {
    /// A single column of this dtype.
    Dtype(DataType),
    /// More than one column, as `col` of several names gives, with their
    /// dtypes.
    Several(Vec<DataType>),
    /// A condition that still lacks its `then`, which has no dtype yet.
    Unfinished,
    /// An expression that starts from `element`, whose dtype is that of what
    /// the context that evaluates it supplies.
    NoContext,
    /// Polars refuses the expression up to here, with this reason.
    Refused(PolarsError),
}

/// The dtype of an expression at the start and after each of its links.
#[derive(Debug, Clone)]
pub struct ExpressionDtypes {
    place: Place,
    start: Resolution,
    links: Vec<Resolution>,
}

impl ExpressionDtypes {
    /// Where the expression is: the step, then the way to the argument that
    /// holds it.
    #[must_use]
    pub const fn place(&self) -> &Place {
        &self.place
    }

    /// What the start alone has.
    #[must_use]
    pub const fn start(&self) -> &Resolution {
        &self.start
    }

    /// What the expression has after each link, in order.
    #[must_use]
    pub fn links(&self) -> &[Resolution] {
        &self.links
    }
}

/// Why a step was refused.
#[derive(Debug)]
pub enum RunCause {
    /// The step names a table that is not connected.
    UnknownTable(String),
    /// An expression starts from an input that is not a connected value or
    /// list.
    UnknownInput(String),
    /// The catalogue could not build the call: an argument is missing, or
    /// Polars refused it.
    Build(BuildError),
    /// A named rule does not let an entry stand where it does, such as
    /// `element` outside a pivot's aggregate. Polars decides the rest, as it
    /// builds.
    NotAllowed(Refusal),
    /// Polars refused the step when it worked out its schema.
    Polars(PolarsError),
}

impl fmt::Display for RunCause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTable(name) => write!(formatter, "there is no connected table {name:?}"),
            Self::UnknownInput(name) => {
                write!(formatter, "there is no connected value or list {name:?}")
            }
            Self::Build(error) => error.fmt(formatter),
            Self::NotAllowed(refusal) => refusal.fmt(formatter),
            Self::Polars(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RunCause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Build(error) => Some(error),
            Self::NotAllowed(refusal) => Some(refusal),
            Self::Polars(error) => Some(error),
            _ => None,
        }
    }
}

/// A step that was refused, where in it, and why.
#[derive(Debug)]
pub struct StepRefusal {
    place: Place,
    cause: Box<RunCause>,
}

impl StepRefusal {
    /// Where the refusal is: the step, then the way to the argument or the
    /// link of an expression.
    #[must_use]
    pub const fn place(&self) -> &Place {
        &self.place
    }

    /// Why. When Polars refused, its own error is the reason.
    #[must_use]
    pub fn cause(&self) -> &RunCause {
        &self.cause
    }
}

impl fmt::Display for StepRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.place, self.cause)
    }
}

impl std::error::Error for StepRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.cause)
    }
}

/// What running one step came to.
#[derive(Debug)]
pub enum Outcome {
    /// Polars accepts the step, and its table has this schema.
    Valid {
        /// The columns and dtypes of the table the step gives.
        schema: SchemaRef,
        /// The columns the step added or replaced, in the order of the table
        /// it gives.
        changes: Vec<ColumnChange>,
    },
    /// The step was refused.
    Refused(StepRefusal),
    /// The step holds a hole, where the editor has not picked something yet,
    /// so it was walked up to it and not built. A step after it is not run.
    Incomplete {
        /// The start of the expression that is a hole.
        place: Place,
    },
    /// A step before it was refused, so it was not run.
    NotRun {
        /// The first step that was refused.
        blocked_by: StepId,
    },
}

/// What running one step reported.
#[derive(Debug)]
pub struct StepReport {
    id: StepId,
    laziness: Laziness,
    expressions: Vec<ExpressionDtypes>,
    walk: Walk,
    outcome: Outcome,
    rows: StepRows,
}

impl StepReport {
    /// The step's identifier.
    #[must_use]
    pub const fn id(&self) -> &StepId {
        &self.id
    }

    /// Whether the step stays in a lazy plan. A step that needs a collect
    /// collected the data it needs when it was built.
    #[must_use]
    pub const fn laziness(&self) -> Laziness {
        self.laziness
    }

    /// The dtypes of every expression in the step's arguments, including the
    /// expressions inside other expressions' arguments, as far as they could
    /// be built. Empty for a step that was not run.
    #[must_use]
    pub fn expressions(&self) -> &[ExpressionDtypes] {
        &self.expressions
    }

    /// What the tables of allowed combinations say of the step: every
    /// expression in it, including those Polars was not asked about because it
    /// refused the step first. Empty for a step that was not run.
    #[must_use]
    pub const fn walk(&self) -> &Walk {
        &self.walk
    }

    /// What came of the step.
    #[must_use]
    pub const fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    /// The schema the step gives, when it is valid.
    #[must_use]
    pub const fn schema(&self) -> Option<&SchemaRef> {
        match &self.outcome {
            Outcome::Valid { schema, .. } => Some(schema),
            _ => None,
        }
    }

    /// How many rows the step kept of those it received, and the first values
    /// of what it added, where the run held rows: [`Document::run_on`]. A run
    /// that reads no data counts none.
    #[must_use]
    pub const fn rows(&self) -> &StepRows {
        &self.rows
    }
}

/// What running a document reported, step by step.
#[derive(Debug)]
pub struct Report {
    source: SchemaRef,
    steps: Vec<StepReport>,
}

impl Report {
    /// The schema of the table the document was run on.
    #[must_use]
    pub const fn source(&self) -> &SchemaRef {
        &self.source
    }

    /// One report for each step, in the document's order.
    #[must_use]
    pub fn steps(&self) -> &[StepReport] {
        &self.steps
    }
}

/// Why a step's rows could not be collected.
#[derive(Debug)]
pub enum RowsError {
    /// The document has no such step.
    NoSuchStep(StepId),
    /// This step, up to the one asked for, was refused.
    Refused(StepRefusal),
    /// This step, up to the one asked for, holds something still to be picked,
    /// at this place.
    Incomplete(Place),
    /// Polars could not collect the rows.
    Polars(PolarsError),
}

impl fmt::Display for RowsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchStep(id) => write!(formatter, "there is no step {:?}", id.as_str()),
            Self::Refused(refusal) => refusal.fmt(formatter),
            Self::Incomplete(place) => {
                write!(
                    formatter,
                    "{place}: something in the step is still to be picked"
                )
            }
            Self::Polars(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RowsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoSuchStep(_) => None,
            Self::Refused(refusal) => Some(refusal),
            Self::Incomplete(_) => None,
            Self::Polars(error) => Some(error),
        }
    }
}

impl Document {
    /// Builds every step in order, the first on `source`, and reports each:
    /// valid with its schema and the dtypes of its expressions, refused with
    /// its place and Polars' reason, or not run because a step before it was
    /// refused. No data is read, except by a step that needs a collect.
    ///
    /// `inputs` are what the steps' `table` arguments and `input` starts name.
    ///
    /// # Errors
    ///
    /// When Polars cannot work out the schema of `source` itself.
    pub fn run(&self, inputs: &Inputs, source: LazyFrame) -> Result<Report, PolarsError> {
        crate::startup::ensure();
        let mut frame = source;
        let mut schema = frame.clone().collect_schema()?;
        let source_schema = schema.clone();
        let mut blocked_by: Option<StepId> = None;
        let mut steps = Vec::with_capacity(self.steps().len());
        for (index, step) in self.steps().iter().enumerate() {
            let (id, laziness) = (step.id().clone(), step.call().entry().laziness());
            if let Some(blocked_by) = &blocked_by {
                steps.push(StepReport {
                    id,
                    laziness,
                    expressions: Vec::new(),
                    walk: Walk::default(),
                    outcome: Outcome::NotRun {
                        blocked_by: blocked_by.clone(),
                    },
                    rows: StepRows::NotCounted,
                });
                continue;
            }
            let run = run_step(index, step, inputs, frame.clone(), &schema, true);
            let outcome = match run.result {
                Ok((next, next_schema)) => {
                    let changes = ColumnChange::between(&schema, &next_schema, &run.written);
                    frame = next;
                    schema = next_schema.clone();
                    Outcome::Valid {
                        schema: next_schema,
                        changes,
                    }
                }
                Err(Halt::Refused(refusal)) => {
                    blocked_by = Some(id.clone());
                    Outcome::Refused(refusal)
                }
                Err(Halt::Incomplete(place)) => {
                    blocked_by = Some(id.clone());
                    Outcome::Incomplete { place }
                }
            };
            steps.push(StepReport {
                id,
                laziness,
                expressions: run.expressions,
                walk: run.walk,
                outcome,
                rows: StepRows::NotCounted,
            });
        }
        Ok(Report {
            source: source_schema,
            steps,
        })
    }

    /// Runs the document as [`run`](Self::run) does, on the rows of `working`,
    /// a table held in memory, and counts for each step the rows it was given
    /// and the rows it gave, with the first values of the columns it added or
    /// replaced.
    ///
    /// Each step is collected once, and the table it gives is the one the next
    /// step is given, so the document is run over the window once and not
    /// again for every step. That is bounded work because `working` is a
    /// window held in memory, which is the only table this takes. A step is
    /// counted only when the run accepts it and every step before it ran: a
    /// step Polars accepts but cannot collect on these rows is
    /// [`StepRows::Failed`], and the steps after it are not counted.
    ///
    /// # Errors
    ///
    /// When Polars cannot work out the schema of `working` itself.
    pub fn run_on(&self, inputs: &Inputs, working: &DataFrame) -> Result<Report, PolarsError> {
        let mut report = self.run(inputs, working.clone().lazy())?;
        let mut frame = working.clone();
        for (index, (step, reported)) in self.steps().iter().zip(&mut report.steps).enumerate() {
            let Outcome::Valid { changes, .. } = &reported.outcome else {
                break;
            };
            let schema = frame.schema().clone();
            let Ok((next, _)) =
                run_step(index, step, inputs, frame.clone().lazy(), &schema, false).result
            else {
                // The run built this step on the schema; it builds on the rows too.
                break;
            };
            let counted = next.collect().and_then(|collected| {
                Counted::of(frame.height(), &collected, changes).map(|count| (collected, count))
            });
            match counted {
                Ok((collected, count)) => {
                    reported.rows = StepRows::Counted(count);
                    frame = collected;
                }
                Err(error) => {
                    reported.rows = StepRows::Failed(error);
                    break;
                }
            }
        }
        Ok(report)
    }

    /// The rows of the table the step `through` gives, at most `limit` of
    /// them, with the steps up to it run on `working`: a table held in memory.
    ///
    /// # Errors
    ///
    /// When the document has no such step, a step up to it is refused, or
    /// Polars cannot collect the rows.
    pub fn rows(
        &self,
        inputs: &Inputs,
        working: &DataFrame,
        through: &StepId,
        limit: usize,
    ) -> Result<DataFrame, RowsError> {
        crate::startup::ensure();
        let last = self
            .steps()
            .iter()
            .position(|step| step.id() == through)
            .ok_or_else(|| RowsError::NoSuchStep(through.clone()))?;
        let mut frame = working.clone().lazy();
        let mut schema = working.schema().clone();
        for (index, step) in self.steps()[..=last].iter().enumerate() {
            let (next, next_schema) = run_step(index, step, inputs, frame, &schema, false)
                .result
                .map_err(|halt| match halt {
                    Halt::Refused(refusal) => RowsError::Refused(refusal),
                    Halt::Incomplete(place) => RowsError::Incomplete(place),
                })?;
            frame = next;
            schema = next_schema;
        }
        frame
            .limit(IdxSize::try_from(limit).unwrap_or(IdxSize::MAX))
            .collect()
            .map_err(RowsError::Polars)
    }
}

/// What building one step came to, and what was learned of its expressions on
/// the way.
struct StepRun {
    expressions: Vec<ExpressionDtypes>,
    walk: Walk,
    /// The names of the columns the step writes, when the run is traced.
    written: Vec<String>,
    result: Result<(LazyFrame, SchemaRef), Halt>,
}

/// Why a step was not built.
enum Halt {
    /// It was refused.
    Refused(StepRefusal),
    /// It holds a hole, at this place.
    Incomplete(Place),
}

fn run_step(
    index: usize,
    step: &Step,
    inputs: &Inputs,
    input: LazyFrame,
    schema: &SchemaRef,
    trace: bool,
) -> StepRun {
    let base = Place::of_step(index, Some(step.id().clone()));
    let mut builder = Builder {
        inputs,
        probe: trace.then(|| DataFrame::empty_with_schema(schema).lazy()),
        base: base.clone(),
        path: Vec::new(),
        positions: Vec::new(),
        traces: Vec::new(),
        written: Vec::new(),
    };
    // What the tables say of the step does not depend on Polars building it.
    let walk = if trace {
        step.walk(index, schema, inputs)
    } else {
        Walk::default()
    };
    // A step with a hole is not built: it is walked, and says where it is incomplete.
    if let Some(path) = step.first_hole() {
        return StepRun {
            result: Err(Halt::Incomplete(path.into_iter().fold(base, Place::then))),
            expressions: Vec::new(),
            walk,
            written: Vec::new(),
        };
    }
    let frame = match builder.step(step.call(), input) {
        Ok(frame) => frame,
        Err(stop) => {
            let place = stop.place(&base);
            return StepRun {
                result: Err(match stop.cause {
                    Some(cause) => Halt::Refused(StepRefusal { place, cause }),
                    None => Halt::Incomplete(place),
                }),
                expressions: builder.traces,
                walk,
                written: Vec::new(),
            };
        }
    };
    let result = match frame.clone().collect_schema() {
        Ok(schema) => Ok((frame, schema)),
        Err(error) => Err(Halt::Refused(builder.polars_refusal(error))),
    };
    let mut result = result;
    let mut written = Vec::new();
    if trace && result.is_ok() {
        match builder.written_names(schema) {
            Ok(names) => written = names,
            // The step is Polars' to refuse, as it is for any schema it cannot work out.
            Err(error) => {
                result = Err(Halt::Refused(StepRefusal {
                    place: base,
                    cause: Box::new(RunCause::Polars(error)),
                }));
            }
        }
    }
    StepRun {
        expressions: builder.traces,
        walk,
        written,
        result,
    }
}

/// An empty table of the schema of `table`, if Polars can work it out.
fn probe_of(table: LazyFrame) -> Option<LazyFrame> {
    let mut table = table;
    let schema = table.collect_schema().ok()?;
    Some(DataFrame::empty_with_schema(&schema).lazy())
}

/// Where building stopped, and why.
struct Stop {
    path: Vec<Segment>,
    /// Why, or `None` where it stopped at a hole.
    cause: Option<Box<RunCause>>,
}

impl Stop {
    fn place(&self, base: &Place) -> Place {
        self.path.iter().cloned().fold(base.clone(), Place::then)
    }
}

/// An expression partway built: an expression, or a condition that waits for
/// its next link.
enum Built {
    Expr(Expr),
    Condition(Conditional),
}

struct Builder<'a> {
    inputs: &'a Inputs,
    /// An empty table of the schema the step starts from, when the dtypes of
    /// expressions are wanted.
    probe: Option<LazyFrame>,
    base: Place,
    path: Vec<Segment>,
    /// The named positions, of a step and of the expressions inside it, that
    /// hold the expression being built, outermost first.
    positions: Vec<Position>,
    traces: Vec<ExpressionDtypes>,
    /// The expressions the step writes as columns, named as they are written.
    written: Vec<Expr>,
}

impl Builder<'_> {
    fn stop<T>(&self, cause: RunCause) -> Result<T, Stop> {
        Err(Stop {
            path: self.path.clone(),
            cause: Some(Box::new(cause)),
        })
    }

    /// The catalogue's result, with its error placed at the parameter it names.
    fn built<T>(&self, result: Result<T, BuildError>) -> Result<T, Stop> {
        result.map_err(|error| {
            let mut path = self.path.clone();
            let parameter = match &error {
                BuildError::Missing { parameter }
                | BuildError::WrongKind { parameter, .. }
                | BuildError::UnknownChoice { parameter, .. }
                | BuildError::Repeated { parameter } => Some((*parameter).to_owned()),
                BuildError::UnknownParameter { parameter } => Some(parameter.clone()),
                BuildError::OutOfOrder { .. } | BuildError::Polars(_) => None,
            };
            path.extend(parameter.map(Segment::Parameter));
            Stop {
                path,
                cause: Some(Box::new(RunCause::Build(error))),
            }
        })
    }

    /// Whether the function that starts an expression may stand in the named
    /// positions that hold the expression, as the named rules beside the
    /// tables of allowed combinations say: `element` stands only in a pivot's
    /// aggregate. Nothing else is asked of the tables here. A row of the
    /// context table was measured with the chain ending at one entry, with one
    /// argument, such as a bare `element()` or an untyped `lit(1)`, so it
    /// cannot judge a start that other arguments or more links follow. Where
    /// no named position holds the expression, there is nothing to ask. Polars
    /// decides the rest when the step is built.
    fn stands_here(&self, call: &Call) -> Result<(), Stop> {
        let Some(context) = Context::from_positions(&self.positions) else {
            return Ok(());
        };
        match allowed::rule_forbids(call.entry(), context) {
            Ok(()) => Ok(()),
            Err(refusal) => self.stop(RunCause::NotAllowed(refusal)),
        }
    }

    /// A call of an entry that cannot stand where the document put it. A
    /// document cannot hold one, so this is not reached.
    fn misplaced<T>(&self, call: &Call) -> Result<T, Stop> {
        self.built(Err(BuildError::OutOfOrder {
            expected: "a call that can stand here",
            found: call.entry().id().as_str(),
        }))
    }

    fn step(&mut self, call: &Call, input: LazyFrame) -> Result<LazyFrame, Stop> {
        let arguments = self.arguments(call)?;
        match call.entry().build() {
            Build::Frame(build) => {
                if self.probe.is_some() {
                    self.written = self.built(call.entry().writes(&arguments))?;
                }
                self.built(build(input, &arguments))
            }
            _ => self.misplaced(call),
        }
    }

    /// The names of the columns the step writes, as Polars names the
    /// expressions it was given against the table the step was given.
    ///
    /// # Errors
    ///
    /// When Polars cannot name them, which the step it accepted is not
    /// expected to come to.
    fn written_names(&self, given: &SchemaRef) -> Result<Vec<String>, PolarsError> {
        if self.written.is_empty() {
            return Ok(Vec::new());
        }
        DataFrame::empty_with_schema(given)
            .lazy()
            .select(self.written.clone())
            .collect_schema()
            .map(|schema| schema.iter_names().map(ToString::to_string).collect())
    }

    /// The arguments of `call`, built, and checked as the catalogue checks them.
    fn arguments(&mut self, call: &Call) -> Result<Arguments, Stop> {
        let entry = call.entry().id().as_str();
        let mut given: Vec<(&str, Argument)> = Vec::new();
        for argument in call.arguments() {
            self.path
                .push(Segment::Parameter(argument.parameter().to_owned()));
            let position = Position::of_parameter(entry, argument.parameter());
            self.positions.extend(position);
            // An expression in a position that is evaluated against another
            // table's columns, as a join's `right_on` is, sees that table's.
            let from_table = match position {
                Some(Position::Outer(outer)) => outer
                    .evaluated_against()
                    .and_then(|table| call.argument(table))
                    .and_then(|table| match table {
                        Value::Table(name) => self.inputs.table(name),
                        _ => None,
                    }),
                _ => None,
            };
            let built = if let Some(table) = from_table {
                let own = self.probe.take();
                self.probe = own.as_ref().and_then(|_| probe_of(table));
                let built = self.argument(argument.value());
                self.probe = own;
                built?
            } else {
                self.argument(argument.value())?
            };
            given.push((argument.parameter(), built));
            if position.is_some() {
                self.positions.pop();
            }
            self.path.pop();
        }
        self.built(call.entry().arguments(given))
    }

    fn argument(&mut self, value: &Value) -> Result<Argument, Stop> {
        Ok(match value {
            Value::Expression(expression) => Argument::Expression(self.expression(expression)?),
            Value::Columns(names) => Argument::Columns(names.clone()),
            Value::Integer(number) => Argument::Integer(*number),
            Value::Float(number) => Argument::Float(*number),
            Value::String(text) => Argument::String(text.clone()),
            Value::Flag(flag) => Argument::Flag(*flag),
            Value::Choice(word) => Argument::Choice(word.clone()),
            Value::Dtype(dtype) => Argument::Dtype(dtype.as_polars().clone()),
            Value::Duration(text) => Argument::Duration(text.clone()),
            Value::Path(text) => Argument::Path(text.clone()),
            Value::Table(name) => match self.inputs.table(name) {
                Some(table) => Argument::Table(table),
                None => return self.stop(RunCause::UnknownTable(name.clone())),
            },
            Value::Pairs(pairs) => {
                let mut built = Vec::with_capacity(pairs.len());
                for (at, (key, entry)) in pairs.iter().enumerate() {
                    self.path.push(Segment::Key(at));
                    let key = self.argument(key)?;
                    self.path.pop();
                    self.path.push(Segment::Value(at));
                    let entry = self.argument(entry)?;
                    self.path.pop();
                    built.push((key, entry));
                }
                Argument::Pairs(built)
            }
            Value::Many(items) => {
                let mut built = Vec::with_capacity(items.len());
                for (at, item) in items.iter().enumerate() {
                    self.path.push(Segment::Item(at));
                    built.push(self.argument(item)?);
                    self.path.pop();
                }
                Argument::Many(built)
            }
        })
    }

    /// Builds `expression` link by link, and, when the dtypes are wanted,
    /// works out the dtype of each prefix of it.
    fn expression(&mut self, expression: &Expression) -> Result<Expr, Stop> {
        let place = self
            .path
            .iter()
            .cloned()
            .fold(self.base.clone(), Place::then);
        let from_element = matches!(
            expression.start(),
            Start::Call(call) if call.entry().id().as_str() == "polars.element"
        );
        self.path.push(Segment::Start);
        let mut state = match expression.start() {
            Start::Input(name) => match self.inputs.literal(name) {
                Some(literal) => Built::Expr(literal),
                None => return self.stop(RunCause::UnknownInput(name.clone())),
            },
            // A step with a hole is not built.
            Start::Hole => {
                return Err(Stop {
                    path: self.path.clone(),
                    cause: None,
                });
            }
            Start::Call(call) => {
                self.stands_here(call)?;
                let arguments = self.arguments(call)?;
                match call.entry().build() {
                    Build::Function(build) => Built::Expr(self.built(build(&arguments))?),
                    Build::Condition(build) => Built::Condition(self.built(build(&arguments))?),
                    _ => return self.misplaced(call),
                }
            }
        };
        self.path.pop();
        let start = self.resolve(&state, from_element);
        let mut links = Vec::with_capacity(expression.links().len());
        for (at, link) in expression.links().iter().enumerate() {
            self.path.push(Segment::Link(at));
            let arguments = self.arguments(link)?;
            state = match (link.entry().build(), state) {
                (Build::Expression(build), Built::Expr(receiver)) => {
                    Built::Expr(self.built(build(receiver, &arguments))?)
                }
                (Build::Expression(build), Built::Condition(condition)) => {
                    let receiver = self.built(condition.into_expr())?;
                    Built::Expr(self.built(build(receiver, &arguments))?)
                }
                (Build::ConditionStep(build), Built::Condition(condition)) => {
                    Built::Condition(self.built(build(condition, &arguments))?)
                }
                (Build::ConditionEnd(build), Built::Condition(condition)) => {
                    Built::Expr(self.built(build(condition, &arguments))?)
                }
                _ => return self.misplaced(link),
            };
            self.path.pop();
            links.push(self.resolve(&state, from_element));
        }
        if let (Some(start), true) = (start, self.probe.is_some()) {
            self.traces.push(ExpressionDtypes {
                place,
                start,
                links: links.into_iter().flatten().collect(),
            });
        }
        match state {
            Built::Expr(expr) => Ok(expr),
            Built::Condition(condition) => self.built(condition.into_expr()),
        }
    }

    /// The dtype `state` has, when the dtypes are wanted.
    fn resolve(&self, state: &Built, from_element: bool) -> Option<Resolution> {
        let probe = self.probe.as_ref()?;
        if from_element {
            return Some(Resolution::NoContext);
        }
        let expr = match state {
            Built::Expr(expr) => expr.clone(),
            Built::Condition(condition) => match condition.clone().into_expr() {
                Ok(expr) => expr,
                Err(_) => return Some(Resolution::Unfinished),
            },
        };
        let mut frame = probe.clone().select([expr]);
        Some(match frame.collect_schema() {
            Ok(schema) => {
                let mut dtypes: Vec<DataType> =
                    schema.iter().map(|(_, dtype)| dtype.clone()).collect();
                if dtypes.len() == 1 {
                    Resolution::Dtype(dtypes.remove(0))
                } else {
                    Resolution::Several(dtypes)
                }
            }
            Err(error) => Resolution::Refused(error),
        })
    }

    /// The refusal for a step Polars refused: at the first link of an
    /// expression Polars refused on its own if there is one, and Polars' reason
    /// for that link; at the step otherwise, with Polars' reason for it.
    fn polars_refusal(&self, error: PolarsError) -> StepRefusal {
        for trace in &self.traces {
            let refused = std::iter::once((Segment::Start, &trace.start))
                .chain(
                    trace
                        .links
                        .iter()
                        .enumerate()
                        .map(|(at, link)| (Segment::Link(at), link)),
                )
                .find_map(|(segment, resolution)| match resolution {
                    Resolution::Refused(reason) => Some((segment, reason.clone())),
                    _ => None,
                });
            if let Some((segment, reason)) = refused {
                return StepRefusal {
                    place: trace.place.clone().then(segment),
                    cause: Box::new(RunCause::Polars(reason)),
                };
            }
        }
        StepRefusal {
            place: self.base.clone(),
            cause: Box::new(RunCause::Polars(error)),
        }
    }
}
