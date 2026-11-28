//! Asks Polars about a step directly, without the step engine.
//!
//! The step engine does not run a step whose walk is refused: it reports the walk's words and
//! neither plans the step nor collects its rows. A test that checks the tables against what
//! Polars does with a step the tables refuse needs Polars' own answer, or it could never find a
//! table that is stricter than Polars. So the step is built here, through the catalogue's own
//! build functions and without any of the engine's refusals, and Polars is asked for its schema
//! and for its rows.
//!
//! What the step engine adds to Polars' answer is not asked here: its own refusals of what Polars
//! panics on, except that `element` stands only in a pivot's aggregate, which Polars panics on
//! everywhere else, and the names of the columns a step writes. A pattern and a duration typed in are
//! asked of Polars before the step is built, as the engine asks them, for Polars refuses them only
//! when the rows run, or panics. A step that holds a hole cannot be asked.
//!
//! Polars panics on some of what the tables refuse, and a panic that reaches the caller is
//! Polars' answer: the callers run these under `catch_unwind`. A panic on a thread of Polars' own
//! does not reach it.

#![allow(dead_code)]

use dageist_engine::allowed::{self, Context, Position, Representative};
use dageist_engine::catalog::{
    Argument, Arguments, Build, BuildError, Conditional, DefaultValue, DurationUse, Kind,
    any_moment, check_duration, check_pattern,
};
use dageist_engine::document::{Call, Expression, Place, Segment, Start, Step, Value};
use polars::prelude::{
    DataFrame, DataType, Engine, Expr, IdxSize, IntoLazy, LazyFrame, PolarsError, SchemaRef,
};
use polars_core::query_result::QueryResult;

/// What is connected to the step, by name: its tables, and the values and lists that start an
/// expression, as the literals they are.
#[derive(Clone, Copy)]
pub struct Connected<'a> {
    tables: &'a dyn Fn(&str) -> Option<LazyFrame>,
    literals: &'a dyn Fn(&str) -> Option<Expr>,
}

impl<'a> Connected<'a> {
    /// The connected tables, and no value or list.
    pub fn tables(tables: &'a dyn Fn(&str) -> Option<LazyFrame>) -> Self {
        Self {
            tables,
            literals: &|_| None,
        }
    }

    /// The connected tables, values and lists.
    pub fn new(
        tables: &'a dyn Fn(&str) -> Option<LazyFrame>,
        literals: &'a dyn Fn(&str) -> Option<Expr>,
    ) -> Self {
        Self { tables, literals }
    }
}

type Tables<'a> = Connected<'a>;

/// What an expression has, up to one of its links, as Polars works it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// A single column of this dtype, its name as the tables write it, and the class it is of.
    Dtype(DataType, String, Option<Representative>),
    /// More than one column, a condition that is not finished, or an expression from `element`,
    /// which has no dtype outside a pivot.
    Unsaid,
    /// Polars refuses the expression up to here, in these words.
    Refused(String),
}

/// What Polars said of an expression of the step: where it is, and what it had at its start and
/// after each link that Polars could work out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Traced {
    pub place: Place,
    pub start: Said,
    pub links: Vec<Said>,
}

/// Where Polars refused the step, and its words, the first line of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub place: Place,
    pub reason: String,
    /// Whether the catalogue refused to build the step, and Polars was not asked: a value that
    /// does not cast to the dtype it is given, a pattern that is no pattern.
    pub at_build: bool,
}

/// What Polars did when the step was planned.
#[derive(Debug, Clone)]
pub struct Planned {
    /// The schema of the table the step gives, or why Polars refused to work it out.
    pub schema: Result<SchemaRef, Refused>,
    /// The expressions of the step, those inside another expression's arguments first.
    pub traces: Vec<Traced>,
}

/// Runs `ask`, a question put to Polars about `step`, and fails the test with what Polars said, and
/// the step it was asked of, where it panics: a test that asks Polars of a step the tables refuse
/// meets what Polars panics on, which is then a failure of the test and no unwinding through it.
///
/// # Panics
///
/// When Polars panics in `ask`.
pub fn guarded<T>(step: &Step, ask: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(ask)).unwrap_or_else(|payload| {
        let said = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "something that is no text".to_owned());
        panic!(
            "Polars panicked when it was asked of the step {:?} of {}: {}",
            step.id().as_str(),
            step.call().entry().id().as_str(),
            first_line(&said)
        )
    })
}

/// The first line of `text`, as the step engine's reports are read.
pub fn first_line(text: &str) -> String {
    text.split("\n\n")
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The expression `step` holds at `place`, the way down from the step to the expression: the
/// arguments of its call, or of a call that follows it, and the items of the lists and the
/// expressions that they hold.
pub fn expression_at<'a>(step: &'a Step, place: &Place) -> Option<&'a Expression> {
    enum Node<'a> {
        Call(&'a Call),
        Value(&'a Value),
        Expression(&'a Expression),
    }
    fn enter(value: &Value) -> Node<'_> {
        match value {
            Value::Expression(expression) => Node::Expression(expression),
            other => Node::Value(other),
        }
    }
    let mut node = Node::Call(step.call());
    for (at, segment) in place.path().iter().enumerate() {
        node = match (node, segment) {
            (Node::Call(call), Segment::Parameter(name)) => enter(call.argument(name)?),
            (Node::Call(_), Segment::Link(link)) if at == 0 => Node::Call(step.links().get(*link)?),
            (Node::Value(Value::Many(items)), Segment::Item(item)) => enter(items.get(*item)?),
            (Node::Value(Value::Pairs(pairs)), Segment::Key(item)) => enter(&pairs.get(*item)?.0),
            (Node::Value(Value::Pairs(pairs)), Segment::Value(item)) => enter(&pairs.get(*item)?.1),
            (Node::Expression(expression), Segment::Start) => match expression.start() {
                Start::Call(call) => Node::Call(call),
                Start::Input(_) | Start::Hole => return None,
            },
            (Node::Expression(expression), Segment::Link(link)) => {
                Node::Call(expression.links().get(*link)?)
            }
            _ => return None,
        };
    }
    match node {
        Node::Expression(expression) => Some(expression),
        _ => None,
    }
}

/// Whether `expression` starts from a number typed without a dtype: a literal of a whole number
/// or of a number with a fraction that is given none.
pub fn starts_from_an_open_number(expression: &Expression) -> bool {
    matches!(expression.start(), Start::Call(call)
        if call.entry().id().as_str() == "polars.lit"
            && call.argument("dtype").is_none()
            && matches!(call.argument("value"), Some(Value::Integer(_) | Value::Float(_))))
}

/// Plans `step`, the `index`th of a document, on `source`, and works out the dtypes of its
/// expressions; `None` for a step that cannot be asked.
pub fn plan(step: &Step, index: usize, source: &DataFrame, tables: Tables) -> Option<Planned> {
    let mut asking = Asking::new(step, index, source, tables, true);
    let built = match asking.step(step, source.clone().lazy()) {
        Ok(frame) => frame,
        Err(Stop::CannotAsk) => return None,
        Err(Stop::Refused(refused)) => {
            return Some(Planned {
                schema: Err(refused),
                traces: asking.traces,
            });
        }
    };
    let schema = built
        .clone()
        .collect_schema()
        .map_err(|error| asking.polars_refusal(&error));
    Some(Planned {
        schema,
        traces: asking.traces,
    })
}

/// Whether Polars accepts `step`, the `index`th of a document, on the schema of `source` when it
/// works out the schema: nothing is traced and no row is read, so a grid of many cells asks it
/// cheaply. `None` for a step that cannot be asked.
pub fn schema_of(
    step: &Step,
    index: usize,
    source: &DataFrame,
    tables: Tables,
) -> Option<Result<SchemaRef, Refused>> {
    let mut asking = Asking::new(step, index, source, tables, false);
    let built = match asking.step(step, source.clone().lazy()) {
        Ok(frame) => frame,
        Err(Stop::CannotAsk) => return None,
        Err(Stop::Refused(refused)) => return Some(Err(refused)),
    };
    Some(
        built
            .clone()
            .collect_schema()
            .map_err(|error| asking.polars_refusal(&error)),
    )
}

/// The first `limit` rows of the table `step` gives on `working`, or the first line of why not.
pub fn collect(
    step: &Step,
    index: usize,
    working: &DataFrame,
    tables: Tables,
    limit: usize,
) -> Result<DataFrame, String> {
    collect_on(Engine::Auto, step, index, working, tables, limit)
}

/// The first `limit` rows of the table `step` gives on `working`, run by `engine`, or the first
/// line of why not.
pub fn collect_on(
    engine: Engine,
    step: &Step,
    index: usize,
    working: &DataFrame,
    tables: Tables,
    limit: usize,
) -> Result<DataFrame, String> {
    let mut asking = Asking::new(step, index, working, tables, true);
    let frame = match asking.step(step, working.clone().lazy()) {
        Ok(frame) => frame,
        Err(Stop::Refused(refused)) => return Err(refused.reason),
        Err(Stop::CannotAsk) => return Err("the step cannot be asked".to_owned()),
    };
    frame
        .limit(IdxSize::try_from(limit).unwrap_or(IdxSize::MAX))
        .collect_with_engine(engine)
        .map(|result| match result {
            QueryResult::Single(frame) => frame,
            QueryResult::Multiple(_) => DataFrame::empty(),
        })
        .map_err(|error| first_line(&error.to_string()))
}

/// What Polars said when it refused `step`, the `index`th of a document, on `working`: where it
/// worked out the schema, or when it ran the first rows of the table; `None` where it did neither.
/// A step that cannot be asked is none that Polars refused.
pub fn refusal_of(
    step: &Step,
    index: usize,
    working: &DataFrame,
    tables: Tables,
) -> Option<String> {
    let planned = plan(step, index, working, tables)?;
    if let Err(refused) = planned.schema {
        return Some(refused.reason);
    }
    collect(step, index, working, tables, 100).err()
}

/// Why the step was not built.
enum Stop {
    Refused(Refused),
    /// It holds what cannot be asked: a hole.
    CannotAsk,
}

/// An expression partway built: an expression, or a condition that waits for its next link.
enum Built {
    Expr(Expr),
    Condition(Conditional),
}

struct Asking<'a> {
    tables: Tables<'a>,
    /// An empty table of the schema the step starts from, when the dtypes of expressions are
    /// wanted.
    probe: Option<LazyFrame>,
    base: Place,
    path: Vec<Segment>,
    /// The named positions, of a step and of the expressions inside it, that hold the expression
    /// being built, outermost first.
    positions: Vec<Position>,
    traces: Vec<Traced>,
    /// The keys of the group by the step makes, once its call is built: an aggregate and a predicate
    /// after it are evaluated on the rows of each group.
    grouping: Option<Vec<Expr>>,
}

impl<'a> Asking<'a> {
    fn new(
        step: &Step,
        index: usize,
        source: &DataFrame,
        tables: Tables<'a>,
        traced: bool,
    ) -> Self {
        Self {
            tables,
            probe: traced.then(|| DataFrame::empty_with_schema(source.schema()).lazy()),
            base: Place::of_step(index, Some(step.id().clone())),
            path: Vec::new(),
            positions: Vec::new(),
            traces: Vec::new(),
            grouping: None,
        }
    }

    fn here(&self) -> Place {
        self.path
            .iter()
            .cloned()
            .fold(self.base.clone(), Place::then)
    }

    fn refused<T>(&self, reason: impl ToString) -> Result<T, Stop> {
        Err(Stop::Refused(Refused {
            place: self.here(),
            reason: first_line(&reason.to_string()),
            at_build: false,
        }))
    }

    /// The catalogue's result, with its error placed at the parameter it names.
    fn built<T>(&self, result: Result<T, BuildError>) -> Result<T, Stop> {
        result.map_err(|error| {
            let mut path = self.path.clone();
            let parameter = match &error {
                BuildError::Missing { parameter }
                | BuildError::WrongKind { parameter, .. }
                | BuildError::UnknownChoice { parameter, .. }
                | BuildError::OutOfRange { parameter, .. }
                | BuildError::NotAPattern { parameter, .. }
                | BuildError::NotADuration { parameter, .. }
                | BuildError::NotATimeZone { parameter, .. }
                | BuildError::Repeated { parameter } => Some((*parameter).to_owned()),
                BuildError::UnknownParameter { parameter } => Some(parameter.clone()),
                BuildError::OutOfOrder { .. } | BuildError::Polars(_) => None,
            };
            path.extend(parameter.map(Segment::Parameter));
            Stop::Refused(Refused {
                place: path.into_iter().fold(self.base.clone(), Place::then),
                reason: first_line(&error.to_string()),
                at_build: true,
            })
        })
    }

    /// The refusal for a step Polars refused when it worked out the schema: at the first link of
    /// an expression Polars refused on its own if there is one, and at the step otherwise.
    fn polars_refusal(&self, error: &PolarsError) -> Refused {
        for trace in &self.traces {
            let refused = std::iter::once((Segment::Start, &trace.start))
                .chain(
                    trace
                        .links
                        .iter()
                        .enumerate()
                        .map(|(at, link)| (Segment::Link(at), link)),
                )
                .find_map(|(segment, said)| match said {
                    Said::Refused(reason) => Some((segment, reason.clone())),
                    _ => None,
                });
            if let Some((segment, reason)) = refused {
                return Refused {
                    place: trace.place.clone().then(segment),
                    reason,
                    at_build: false,
                };
            }
        }
        Refused {
            place: self.base.clone(),
            reason: first_line(&error.to_string()),
            at_build: false,
        }
    }

    /// Whether the function that starts an expression may stand in the named positions that hold
    /// the expression: `element` stands only in a pivot's aggregate, and Polars panics where it
    /// stands anywhere else.
    fn stands_here(&self, call: &Call) -> Result<(), Stop> {
        let Some(context) = Context::from_positions(&self.positions) else {
            return Ok(());
        };
        match allowed::rule_forbids(call.entry(), context) {
            Ok(()) => Ok(()),
            Err(refusal) => self.refused(refusal),
        }
    }

    fn step(&mut self, step: &Step, input: LazyFrame) -> Result<LazyFrame, Stop> {
        let call = step.call();
        let arguments = self.arguments(call)?;
        match call.entry().build() {
            Build::Frame(build) => self.built(build(input, &arguments)),
            // The calls of a group by follow it, the last of which gives the table.
            Build::Grouping(build) => {
                // The keys by position, then those given by name, each named for its key.
                let mut keys = self.built(arguments.expressions("by"))?;
                for (name, key) in self.built(arguments.pairs("named_by"))? {
                    if let Argument::Columns(names) = name
                        && let [name] = names.as_slice()
                        && let Argument::Expression(key) = key
                    {
                        keys.push(key.alias(name.as_str()));
                    }
                }
                self.grouping = Some(keys);
                let mut group = self.built(build(input, &arguments))?;
                for (at, link) in step.links().iter().enumerate() {
                    self.path.push(Segment::Link(at));
                    let arguments = self.arguments(link)?;
                    match link.entry().build() {
                        Build::GroupedStep(build) => {
                            group = self.built(build(group, &arguments))?
                        }
                        Build::GroupedEnd(build) => return self.built(build(group, &arguments)),
                        _ => return self.refused("a call of a group by follows group_by"),
                    }
                    self.path.pop();
                }
                self.refused("a step that groups is finished by its last call")
            }
            _ => self.refused("a step is built from a frame"),
        }
    }

    /// The arguments of `call`, built.
    fn arguments(&mut self, call: &Call) -> Result<Arguments, Stop> {
        let entry = call.entry().id().as_str();
        let mut given: Vec<(&str, Argument)> = Vec::new();
        for argument in call.arguments() {
            self.path
                .push(Segment::Parameter(argument.parameter().to_owned()));
            let position = Position::of_parameter(entry, argument.parameter());
            self.positions.extend(position);
            // An expression in a position that is evaluated against another table's columns, as a
            // join's `right_on` is, sees that table's.
            let table = match position {
                Some(Position::Outer(outer)) => outer
                    .evaluated_against()
                    .and_then(|table| call.argument(table))
                    .and_then(|table| match table {
                        Value::Table(name) => (self.tables.tables)(name),
                        _ => None,
                    }),
                _ => None,
            };
            let built = match table {
                Some(table) => {
                    let own = self.probe.take();
                    self.probe = own.as_ref().and_then(|_| probe_of(table));
                    let built = self.argument(argument.value());
                    self.probe = own;
                    built?
                }
                None => self.argument(argument.value())?,
            };
            given.push((argument.parameter(), built));
            if position.is_some() {
                self.positions.pop();
            }
            self.path.pop();
        }
        // A separator typed in is a pattern where the call takes it so, and is asked of Polars as
        // one: Polars would refuse it only when the rows run.
        for (parameter, text) in patterns_of(call) {
            let typed_expression = call.entry().parameters().iter().any(|known| {
                known.name() == parameter && matches!(known.kind(), Kind::Expression(_))
            });
            if typed_expression && let Err(error) = check_pattern(text) {
                return self.built(Err(BuildError::NotAPattern { parameter, error }));
            }
        }
        self.built(call.entry().arguments(given))
    }

    fn argument(&mut self, value: &Value) -> Result<Argument, Stop> {
        Ok(match value {
            Value::Expression(expression) => Argument::Expression(self.expression(expression)?),
            Value::Columns(names) => Argument::Columns(names.clone()),
            Value::Field(name) => Argument::Field(name.clone()),
            Value::Integer(number) => Argument::Integer(*number),
            Value::Float(number) => Argument::Float(*number),
            Value::String(text) => Argument::String(text.clone()),
            Value::Flag(flag) => Argument::Flag(*flag),
            Value::Choice(word) => Argument::Choice(word.clone()),
            Value::Dtype(dtype) => Argument::Dtype(dtype.as_polars().clone()),
            Value::Duration(text) => Argument::Duration(text.clone()),
            Value::Pattern(text) => Argument::Pattern(text.clone()),
            Value::TimeZone(zone) => Argument::TimeZone(zone.clone()),
            Value::Path(text) => Argument::Path(text.clone()),
            Value::Table(name) => match (self.tables.tables)(name) {
                Some(table) => Argument::Table(table),
                None => return self.refused(format!("there is no connected table {name:?}")),
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

    /// Builds `expression` link by link and, when the dtypes are wanted, works out the dtype of
    /// each prefix of it.
    fn expression(&mut self, expression: &Expression) -> Result<Expr, Stop> {
        let place = self.here();
        let from_element = matches!(
            expression.start(),
            Start::Call(call) if call.entry().id().as_str() == "polars.element"
        );
        self.path.push(Segment::Start);
        let mut state = match expression.start() {
            Start::Call(call) => {
                self.stands_here(call)?;
                let arguments = self.arguments(call)?;
                match call.entry().build() {
                    Build::Function(build) => Built::Expr(self.built(build(&arguments))?),
                    Build::Condition(build) => Built::Condition(self.built(build(&arguments))?),
                    _ => return self.refused("a call that cannot stand here"),
                }
            }
            Start::Input(name) => match (self.tables.literals)(name) {
                Some(literal) => Built::Expr(literal),
                None => {
                    return self.refused(format!("there is no connected value or list {name:?}"));
                }
            },
            Start::Hole => return Err(Stop::CannotAsk),
        };
        self.path.pop();
        let start = self.traced(&state, from_element);
        let mut links = Vec::with_capacity(expression.links().len());
        for (at, link) in expression.links().iter().enumerate() {
            self.path.push(Segment::Link(at));
            // A duration typed in is a duration string where the call takes it so, and is asked
            // of Polars for the moment it is given to.
            let durations = durations_of(link);
            if !durations.is_empty() {
                let moment = match self.resolve(&state, from_element) {
                    Some(Said::Dtype(dtype @ (DataType::Datetime(..) | DataType::Date), ..)) => {
                        dtype
                    }
                    _ => any_moment(),
                };
                for (parameter, text, using) in durations {
                    if let Err(error) = check_duration(text, using, &moment) {
                        return self.built(Err(BuildError::NotADuration { parameter, error }));
                    }
                }
            }
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
                _ => return self.refused("a call that cannot stand here"),
            };
            self.path.pop();
            links.push(self.traced(&state, from_element));
        }
        if let (Some(start), true) = (start, self.probe.is_some()) {
            self.traces.push(Traced {
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

    /// What `state` has where a report says it: what it gives where it stands, which in an aggregate
    /// or a predicate of a group by is what it gives for each group, a value for a chain that
    /// aggregates and a list of the values of the group for one that does not.
    fn traced(&self, state: &Built, from_element: bool) -> Option<Said> {
        let in_a_group = matches!(
            Context::from_positions(&self.positions).map(|context| context.outer()),
            Some(allowed::Outer::Aggregate | allowed::Outer::Having)
        );
        let (Some(keys), Some(probe), true, false) =
            (&self.grouping, &self.probe, in_a_group, from_element)
        else {
            return self.resolve(state, from_element);
        };
        let expr = match state {
            Built::Expr(expr) => expr.clone(),
            Built::Condition(condition) => match condition.clone().into_expr() {
                Ok(expr) => expr,
                Err(_) => return Some(Said::Unsaid),
            },
        };
        let mut keyed = probe.clone().group_by(keys.clone()).agg(Vec::<Expr>::new());
        let kept = match keyed.collect_schema() {
            Ok(schema) => schema.len(),
            Err(error) => return Some(Said::Refused(first_line(&error.to_string()))),
        };
        // Named apart from the keys, as a key that it reads would be named as the key is.
        let mut frame = probe
            .clone()
            .group_by(keys.clone())
            .agg([expr.name().prefix("traced ")]);
        Some(match frame.collect_schema() {
            Ok(schema) => {
                let dtypes: Vec<&DataType> =
                    schema.iter().skip(kept).map(|(_, dtype)| dtype).collect();
                match dtypes.as_slice() {
                    [dtype] => Said::Dtype(
                        (*dtype).clone(),
                        allowed::dtype_name(dtype),
                        Representative::of(dtype),
                    ),
                    _ => Said::Unsaid,
                }
            }
            Err(error) => Said::Refused(first_line(&error.to_string())),
        })
    }

    /// What `state` has, when the dtypes are wanted: the dtype Polars gives it, or its refusal.
    fn resolve(&self, state: &Built, from_element: bool) -> Option<Said> {
        let probe = self.probe.as_ref()?;
        if from_element {
            return Some(Said::Unsaid);
        }
        let expr = match state {
            Built::Expr(expr) => expr.clone(),
            Built::Condition(condition) => match condition.clone().into_expr() {
                Ok(expr) => expr,
                Err(_) => return Some(Said::Unsaid),
            },
        };
        let mut frame = probe.clone().select([expr]);
        Some(match frame.collect_schema() {
            Ok(schema) => {
                let dtypes: Vec<&DataType> = schema.iter().map(|(_, dtype)| dtype).collect();
                match dtypes.as_slice() {
                    [dtype] => Said::Dtype(
                        (*dtype).clone(),
                        allowed::dtype_name(dtype),
                        Representative::of(dtype),
                    ),
                    _ => Said::Unsaid,
                }
            }
            Err(error) => Said::Refused(first_line(&error.to_string())),
        })
    }
}

/// An empty table of the schema of `table`, if Polars can work it out.
fn probe_of(table: LazyFrame) -> Option<LazyFrame> {
    let mut table = table;
    let schema: SchemaRef = table.collect_schema().ok()?;
    Some(DataFrame::empty_with_schema(&schema).lazy())
}

/// The text `expression` is when it is a text typed in and nothing more: a literal with no dtype
/// and no methods after it.
fn typed_text(expression: &Expression) -> Option<&String> {
    if !expression.links().is_empty() {
        return None;
    }
    let Start::Call(call) = expression.start() else {
        return None;
    };
    if call.entry().id().as_str() != "polars.lit" || call.arguments().len() != 1 {
        return None;
    }
    match call.argument("value")? {
        Value::String(text) => Some(text),
        _ => None,
    }
}

/// The patterns `call` was given that are regular expressions, each by its parameter's name and as
/// typed: a value of the pattern kind, and the separator of a split, which is an expression, where
/// it is text typed in and nothing more, unless a flag makes it plain text.
fn patterns_of(call: &Call) -> Vec<(&'static str, &str)> {
    let entry = call.entry();
    entry
        .parameters()
        .iter()
        .filter_map(|parameter| {
            let text = match (call.argument(parameter.name())?, parameter.kind()) {
                (Value::Pattern(text), Kind::Pattern) => text,
                (Value::Expression(expression), Kind::Expression(_)) => typed_text(expression)?,
                _ => return None,
            };
            let plain = parameter
                .plain_text_flag()
                .is_some_and(|flag| match call.argument(flag) {
                    Some(Value::Flag(Some(set))) => *set,
                    _ => entry
                        .parameters()
                        .iter()
                        .find(|other| other.name() == flag)
                        .is_some_and(|other| other.default().value() == DefaultValue::Flag(true)),
                });
            let may_be_one =
                parameter.kind() == Kind::Pattern || parameter.plain_text_flag().is_some();
            (may_be_one && !plain).then_some((parameter.name(), text.as_str()))
        })
        .collect()
}

/// The durations `call` was given that are duration strings, each by its parameter's name and as
/// typed: the parameters that hold a duration string where an expression is text typed in and
/// nothing more.
fn durations_of(call: &Call) -> Vec<(&'static str, &str, DurationUse)> {
    call.entry()
        .parameters()
        .iter()
        .filter_map(|parameter| {
            let using = parameter.duration_string()?;
            match call.argument(parameter.name())? {
                Value::Expression(expression) => {
                    Some((parameter.name(), typed_text(expression)?.as_str(), using))
                }
                _ => None,
            }
        })
        .collect()
}
