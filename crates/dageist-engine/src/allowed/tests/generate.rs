//! Making the tables: running each combination of the catalogue in Polars.
//!
//! Every call is built through the catalogue's own build functions, as a
//! document's steps are, and run on the sample tables of [`super::sample`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use polars::prelude::{DataType, Expr, IntoLazy, LazyFrame};

use crate::allowed::context::{Context, Nested, Outer};
use crate::allowed::dtype::Representative;
use crate::allowed::offers::default_word;
use crate::allowed::rules::Rule;
use crate::allowed::table::{
    Chain, ContextLine, DtypeKey, DtypeLine, OptionLine, OptionValue, Outcome, Produces, Reason,
    Stage,
};
use crate::catalog::{
    self, Applies, Argument, BuildError, Entry, Kind, Parameter, ParameterDefault,
    Shape as ParamShape,
};

use super::build::{
    column, condition_with, element, entry, expression, expressions, flag, integer, method,
    null_of, one_of, step,
};
use super::probe::{Plan, measure, outcome};
use super::sample::Frames;

/// Everything the tables are made of.
pub(super) struct Generated {
    pub dtypes: Vec<DtypeLine>,
    pub contexts: Vec<ContextLine>,
    pub options: Vec<OptionLine>,
}

/// Runs `work` on every item, on as many threads as the machine offers up to
/// four, and gives the results in the order of the items.
fn in_parallel<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(4);
    let next = AtomicUsize::new(0);
    let mut done: Vec<(usize, R)> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(at) else { break };
                        mine.push((at, work(item)));
                    }
                    mine
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| match handle.join() {
                Ok(mine) => mine,
                Err(payload) => std::panic::resume_unwind(payload),
            })
            .collect()
    });
    done.sort_by_key(|(at, _)| *at);
    done.into_iter().map(|(_, result)| result).collect()
}

pub(super) fn generate() -> Generated {
    let dtypes = dtype_lines();
    let index = DtypeIndex::new(&dtypes);
    let options = option_lines(&index);
    let contexts = context_lines(&index, &OptionIndex::new(&options));
    Generated {
        dtypes,
        contexts,
        options,
    }
}

// ---------------------------------------------------------------------------
// Arguments

/// The parameters of `entry` that hold an expression.
fn expression_parameters(entry: &'static Entry) -> Vec<&'static Parameter> {
    entry
        .parameters()
        .iter()
        .filter(|parameter| matches!(parameter.kind(), Kind::Expression(_)))
        .collect()
}

/// An argument for the parameter `parameter` of `entry`: the expression, in a
/// list when the parameter holds a list.
fn argument_for(entry: &Entry, parameter: &str, value: Expr) -> Argument {
    let parameter = entry
        .parameters()
        .iter()
        .find(|candidate| candidate.name() == parameter)
        .expect("the entry has the parameter");
    match parameter.shape() {
        ParamShape::Many => expressions(vec![value]),
        ParamShape::One | ParamShape::PerItem(_) => expression(value),
    }
}

/// The parameters Polars takes only as a single value, not as a column.
const SCALAR_ONLY: [(&str, &str); 2] = [("Expr.shift", "n"), ("Expr.shift", "fill_value")];

/// The parameters whose values a context is measured with as literals: Polars calls a replacement
/// elementwise, as a join's key must be, only where what it replaces and replaces by, and the
/// default of a strict one, are literals, which is how a replacement is written, and refuses one
/// given a column. The dtype table measures them as columns, as every argument is.
const LITERAL_IN_CONTEXTS: [(&str, &str); 5] = [
    ("Expr.replace", "old"),
    ("Expr.replace", "new"),
    ("Expr.replace_strict", "old"),
    ("Expr.replace_strict", "new"),
    ("Expr.replace_strict", "default"),
];

/// The expression that stands for an argument of `dtype` for `parameter` of
/// the entry `entry`: a column of that dtype; the number 1 of that dtype where
/// Polars takes one value only; or a null of that dtype where the expression
/// is built for a group, which has no other column.
fn argument_expression(
    entry: &str,
    parameter: &str,
    dtype: Representative,
    scope: Scope,
) -> Result<Expr, BuildError> {
    if scope.group {
        null_of(&dtype.dtype())
    } else if SCALAR_ONLY.contains(&(entry, parameter))
        || (scope.literals && LITERAL_IN_CONTEXTS.contains(&(entry, parameter)))
    {
        one_of(&dtype.dtype())
    } else {
        Ok(column(scope.argument))
    }
}

/// What a call needs besides the parameter under test to mean anything.
/// Python's `over` is refused without something to group or order by, and
/// `*more_exprs` extends `partition_by`, so unless `partition_by` or `order_by`
/// is what is tested, which each stand alone, it is given the key. A rounding to significant
/// figures is given two, which a number of the dtypes has. A cast is given the dtype of its
/// receiver, which it converts to itself. A replacement is given what it replaces by and what it
/// replaces, columns of the receiver's dtype, and a default where it is strict, as the column
/// that stands for an argument, so that a value the columns do not list is not refused; an
/// interval is given the bound that is not tested. What a strict replacement of an array is given
/// that replaces it is not an array, for Polars finds no dtype for an array and a default that
/// is one, which would refuse every dtype that is tested for what it replaces.
fn companions(
    entry: &str,
    parameter: Option<&str>,
    scope: Scope,
    receiver: Representative,
    argument: Representative,
) -> Vec<(&'static str, Argument)> {
    let own = || expression(scope.start());
    let stands_in = |name: &'static str| argument_expression(entry, name, argument, scope);
    // The value that stands for what replaces the dtype of the receiver, or for its default.
    let value = || {
        if receiver == Representative::Array && entry == "Expr.replace_strict" {
            expression(scope.partition())
        } else {
            own()
        }
    };
    // What replaces, where what is replaced is tested: one value for a `Boolean`, which Polars
    // replaces by without the join it makes of many. That join meets the values of what is
    // replaced cast to the `Boolean` twice where they are the same there, as whole numbers are,
    // and refuses for it, which no value that is listed shows.
    let replacing = || {
        if receiver == Representative::Boolean {
            one_of(&receiver.dtype()).map_or_else(|_| value(), expression)
        } else {
            value()
        }
    };
    let mut given: Vec<(&'static str, Argument)> = match (entry, parameter) {
        ("Expr.over", Some("partition_by" | "order_by")) => Vec::new(),
        ("Expr.over", _) => vec![("partition_by", expressions(vec![scope.partition()]))],
        ("Expr.round_sig_figs", _) => vec![("digits", Argument::Integer(2))],
        ("Expr.cast", _) => vec![("dtype", Argument::Dtype(receiver.dtype()))],
        ("Expr.replace" | "Expr.replace_strict", Some("old")) => vec![("new", replacing())],
        ("Expr.replace" | "Expr.replace_strict", Some("new")) => vec![("old", own())],
        // The default and what replaces meet in one dtype, so each is tested with the other of
        // the dtype the argument has.
        ("Expr.replace_strict", Some("default")) => vec![("old", own())],
        // A slice that is tested for its length starts at a place: the second character.
        ("Expr.str.slice", Some("length")) => vec![("offset", expression(integer(1)))],
        ("Expr.is_between", Some("lower_bound")) => vec![("upper_bound", own())],
        ("Expr.is_between", Some("upper_bound")) => vec![("lower_bound", own())],
        _ => Vec::new(),
    };
    if entry == "Expr.replace_strict" {
        // Where what is replaced is tested, the default is the dtype of the receiver, as what
        // replaces it is.
        let default = match parameter {
            Some("old") => Some(value()),
            Some("default") => None,
            _ => stands_in("default").ok().map(expression),
        };
        given.extend(default.map(|default| ("default", default)));
        if parameter == Some("default") {
            given.extend(stands_in("new").ok().map(|new| ("new", expression(new))));
        }
    }
    given
}

// ---------------------------------------------------------------------------
// The dtype table

struct DtypeJob {
    entry: &'static Entry,
    parameter: Option<&'static str>,
    receiver: Representative,
    argument: Option<Representative>,
}

fn dtype_jobs() -> Vec<DtypeJob> {
    let mut jobs = Vec::new();
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
    {
        let parameters = expression_parameters(entry);
        let required: Vec<_> = parameters
            .iter()
            .filter(|parameter| parameter.is_needed())
            .map(|parameter| parameter.name())
            .collect();
        if required.is_empty() {
            for receiver in Representative::ALL {
                jobs.push(DtypeJob {
                    entry,
                    parameter: None,
                    receiver,
                    argument: None,
                });
            }
        }
        // Python requires at most one; the rest that are needed stand for one another, as the
        // bounds of `clip` do, and are measured one at a time.
        let python_required: Vec<_> = parameters
            .iter()
            .filter(|parameter| parameter.default() == ParameterDefault::Required)
            .map(|parameter| parameter.name())
            .collect();
        for parameter in &parameters {
            // A parameter that Python requires and that is not the one varied is given a fitting
            // default, which `companions` says.
            let supplied: Vec<&str> = companions(
                entry.id().as_str(),
                Some(parameter.name()),
                LEFT,
                Representative::Int64,
                Representative::Int64,
            )
            .iter()
            .map(|(name, _)| *name)
            .collect();
            assert!(
                python_required
                    .iter()
                    .all(|name| *name == parameter.name() || supplied.contains(name)),
                "{} has two parameters that must be given; teach the generator a fitting default \
                 for the one it does not vary",
                entry.id().as_str()
            );
            for receiver in Representative::ALL {
                for argument in Representative::ALL {
                    jobs.push(DtypeJob {
                        entry,
                        parameter: Some(parameter.name()),
                        receiver,
                        argument: Some(argument),
                    });
                }
            }
        }
    }
    jobs
}

fn dtype_line(job: &DtypeJob) -> DtypeLine {
    let argument = job.argument.unwrap_or(job.receiver);
    let frames = Frames::new(job.receiver, argument, &DataType::Int64);
    let plan = |frames: &Frames, empty: bool| -> Result<LazyFrame, BuildError> {
        let mut given = companions(
            job.entry.id().as_str(),
            job.parameter,
            LEFT,
            job.receiver,
            argument,
        );
        if let (Some(parameter), Some(dtype)) = (job.parameter, job.argument) {
            let value = argument_expression(job.entry.id().as_str(), parameter, dtype, LEFT)?;
            given.push((parameter, argument_for(job.entry, parameter, value)));
        }
        let call = method(job.entry, column("a"), given)?;
        step(
            "LazyFrame.select",
            frames.left(empty),
            vec![("exprs", expressions(vec![call]))],
        )
    };
    DtypeLine {
        entry: job.entry.id().as_str(),
        parameter: job.parameter,
        receiver: job.receiver,
        argument: job.argument,
        outcome: outcome(&plan, &frames),
    }
}

fn dtype_lines() -> Vec<DtypeLine> {
    in_parallel(&dtype_jobs(), dtype_line)
}

/// A receiver and the arguments that make a method run, with what it gives.
#[derive(Clone)]
struct Candidate {
    receiver: Representative,
    /// Each expression parameter that must be given, and the dtype of its argument.
    arguments: Vec<(&'static str, Representative)>,
    produces: Option<Produces>,
}

struct DtypeIndex {
    lines: HashMap<DtypeKey, Outcome>,
}

impl DtypeIndex {
    fn new(lines: &[DtypeLine]) -> Self {
        Self {
            lines: lines
                .iter()
                .map(|line| {
                    (
                        (line.entry, line.parameter, line.receiver, line.argument),
                        line.outcome.clone(),
                    )
                })
                .collect(),
        }
    }

    /// The receivers a method runs on, the one that fits best first: `Int64`,
    /// then the others in the table's order. Each is given, for each parameter
    /// that must be given, an argument of its own dtype if that runs, and
    /// otherwise the first that does.
    fn candidates(&self, entry: &'static Entry) -> Vec<Candidate> {
        let order = std::iter::once(Representative::Int64).chain(
            Representative::ALL
                .into_iter()
                .filter(|dtype| *dtype != Representative::Int64),
        );
        let required: Vec<&'static str> = expression_parameters(entry)
            .into_iter()
            .filter(|parameter| parameter.is_needed())
            .map(Parameter::name)
            .collect();
        let id = entry.id().as_str();
        let mut found = Vec::new();
        'receivers: for receiver in order {
            let mut produces = None;
            if required.is_empty() {
                match self.lines.get(&(id, None, receiver, None)) {
                    Some(Outcome::Allowed(allowed)) => produces = allowed.clone(),
                    _ => continue,
                }
            }
            let mut arguments = Vec::new();
            for parameter in &required {
                let tried = std::iter::once(receiver).chain(
                    Representative::ALL
                        .into_iter()
                        .filter(|dtype| *dtype != receiver),
                );
                let fitting = tried.into_iter().find_map(|argument| {
                    match self
                        .lines
                        .get(&(id, Some(*parameter), receiver, Some(argument)))
                    {
                        Some(Outcome::Allowed(allowed)) => Some((argument, allowed.clone())),
                        _ => None,
                    }
                });
                let Some((argument, allowed)) = fitting else {
                    continue 'receivers;
                };
                if arguments.is_empty() {
                    produces = allowed;
                }
                arguments.push((*parameter, argument));
            }
            found.push(Candidate {
                receiver,
                arguments,
                produces,
            });
        }
        // What a method gives may depend on the dtype of its argument, as `fill_nan` of a `Null`
        // gives the dtype of what fills it, so the receivers and arguments that give a `Boolean`
        // follow, for a context that needs one measures such a method where it gives one. They
        // come last, where the ones that fit best are still found first.
        if let [parameter] = required.as_slice() {
            let booleans: Vec<Candidate> = Representative::ALL
                .into_iter()
                .flat_map(|receiver| {
                    Representative::ALL
                        .into_iter()
                        .map(move |argument| (receiver, argument))
                })
                .filter_map(|(receiver, argument)| {
                    match self
                        .lines
                        .get(&(id, Some(*parameter), receiver, Some(argument)))
                    {
                        Some(Outcome::Allowed(Some(allowed)))
                            if allowed.result.class() == Some(Representative::Boolean)
                                && !found.iter().any(|candidate| {
                                    candidate.receiver == receiver
                                        && candidate.arguments == [(*parameter, argument)]
                                }) =>
                        {
                            Some(Candidate {
                                receiver,
                                arguments: vec![(*parameter, argument)],
                                produces: Some(allowed.clone()),
                            })
                        }
                        _ => None,
                    }
                })
                .take(1)
                .collect();
            found.extend(booleans);
        }
        found
    }
}

// ---------------------------------------------------------------------------
// Where an expression is built

/// The names of the columns an expression is built from, and whether it is
/// the values of a group.
#[derive(Clone, Copy)]
struct Scope {
    receiver: &'static str,
    argument: &'static str,
    boolean: &'static str,
    key: &'static str,
    /// The expression starts from `element()`, and its arguments are nulls of
    /// their dtype, as a group has no other column.
    group: bool,
    /// The values of a replacement are literals, which a context is measured with.
    literals: bool,
}

const LEFT: Scope = Scope {
    receiver: "a",
    argument: "b",
    boolean: "f",
    key: "k",
    group: false,
    literals: false,
};
const OTHER: Scope = Scope {
    receiver: "ra",
    argument: "rb",
    boolean: "rf",
    key: "rk",
    group: false,
    literals: false,
};
const GROUP: Scope = Scope {
    group: true,
    ..LEFT
};

impl Scope {
    fn start(&self) -> Expr {
        if self.group {
            element()
        } else {
            column(self.receiver)
        }
    }

    /// An expression to partition by that has one value.
    fn partition(&self) -> Expr {
        if self.group {
            integer(1)
        } else {
            column(self.key)
        }
    }
}

/// The method of `candidate` applied to what `scope` starts from, with
/// `extra` arguments.
fn method_on(
    entry: &'static Entry,
    candidate: &Candidate,
    scope: Scope,
    extra: Vec<(&'static str, Argument)>,
) -> Result<Expr, BuildError> {
    let id = entry.id().as_str();
    let argument = candidate
        .arguments
        .first()
        .map_or(candidate.receiver, |(_, dtype)| *dtype);
    let mut given = companions(id, None, scope, candidate.receiver, argument);
    // What is given for a parameter stands in place of what it would be given otherwise.
    given.retain(|(name, _)| !extra.iter().any(|(given, _)| given == name));
    given.extend(extra);
    for (parameter, dtype) in &candidate.arguments {
        let value = argument_expression(id, parameter, *dtype, scope)?;
        given.push((parameter, argument_for(entry, parameter, value)));
    }
    method(entry, scope.start(), given)
}

/// Whether the expression under test must be `Boolean` where it stands, as
/// the rules say.
fn needs_boolean(context: Context) -> bool {
    Rule::ALL
        .into_iter()
        .any(|rule| rule.needs_boolean_end(context))
}

/// The dtype a join key is cast to, where the chain goes on from an entry in
/// a join key: the dtype of the other table's key, which the sample tables
/// give as `key`.
const KEY: DataType = DataType::Int64;

/// What follows the expression under test where the chain goes on from it,
/// so that the context does not refuse it for how it ends: a test for a value
/// where a `Boolean` is required, which any dtype can be given.
fn continued_subject(context: Context, subject: Expr) -> Result<Expr, BuildError> {
    if needs_boolean(context) {
        method(entry("Expr.is_not_null"), subject, Vec::new())
    } else {
        Ok(subject)
    }
}

/// What follows the whole expression that stands at a position of a step,
/// where the chain goes on from the entry under test: a cast to the key's
/// dtype in a join key, which the catalogue has no entry for yet, and the
/// first value of a group in a pivot's aggregate.
fn continued_held(outer: Outer, held: Expr) -> Result<Expr, BuildError> {
    match outer {
        Outer::JoinLeft | Outer::JoinRight => Ok(held.cast(KEY)),
        Outer::PivotAggregate => method(entry("Expr.first"), held, Vec::new()),
        Outer::Select | Outer::Filter | Outer::Sort => Ok(held),
    }
}

const fn scope_of(outer: Outer) -> Scope {
    match outer {
        Outer::JoinRight => OTHER,
        Outer::PivotAggregate => GROUP,
        _ => LEFT,
    }
}

/// `subject`, standing in `nested`, in the expression that holds it.
fn wrap(nested: Nested, subject: Expr, scope: Scope) -> Result<Expr, BuildError> {
    let base = if scope.group {
        element()
    } else {
        column(scope.boolean)
    };
    let over = entry("Expr.over");
    match nested {
        Nested::PartitionBy => method(
            over,
            base,
            vec![("partition_by", expressions(vec![subject]))],
        ),
        Nested::OrderBy => method(
            over,
            base,
            vec![
                ("partition_by", expressions(vec![scope.partition()])),
                ("order_by", expressions(vec![subject])),
            ],
        ),
        Nested::ConditionPredicate => conditional(subject, flag(true), flag(false)),
        Nested::ConditionValue => conditional(flag(true), subject.clone(), subject),
    }
}

/// `when(predicate).then(then).otherwise(otherwise)`.
fn conditional(predicate: Expr, then: Expr, otherwise: Expr) -> Result<Expr, BuildError> {
    use crate::catalog::Build;
    let build = |id: &str, given: Vec<(&'static str, Argument)>| {
        let entry = entry(id);
        entry
            .arguments(given)
            .map(|arguments| (entry.build(), arguments))
    };
    let (when, arguments) = build(
        "polars.when",
        vec![("predicates", expressions(vec![predicate]))],
    )?;
    let Build::Condition(when) = when else {
        panic!("polars.when starts a condition")
    };
    let state = when(&arguments)?;
    let (then_build, arguments) = build("When.then", vec![("statement", expression(then))])?;
    let Build::ConditionStep(then_build) = then_build else {
        panic!("When.then continues a condition")
    };
    let state = then_build(state, &arguments)?;
    let (end, arguments) = build("Then.otherwise", vec![("statement", expression(otherwise))])?;
    let Build::ConditionEnd(end) = end else {
        panic!("Then.otherwise ends a condition")
    };
    end(state, &arguments)
}

/// The step of `outer` holding `held`, built on the sample tables.
fn host(outer: Outer, held: Expr, frames: &Frames, empty: bool) -> Result<LazyFrame, BuildError> {
    let left = frames.left(empty);
    let other = || Argument::Table(frames.other(empty));
    match outer {
        Outer::Select => step(
            "LazyFrame.select",
            left,
            vec![("exprs", expressions(vec![held]))],
        ),
        Outer::Filter => step(
            "LazyFrame.filter",
            left,
            vec![("predicates", expressions(vec![held]))],
        ),
        Outer::Sort => step(
            "LazyFrame.sort",
            left,
            vec![("by", expressions(vec![held]))],
        ),
        Outer::JoinLeft => step(
            "LazyFrame.join",
            left,
            vec![
                ("other", other()),
                ("left_on", expressions(vec![held])),
                ("right_on", expressions(vec![column("key")])),
            ],
        ),
        Outer::JoinRight => step(
            "LazyFrame.join",
            left,
            vec![
                ("other", other()),
                ("left_on", expressions(vec![column("key")])),
                ("right_on", expressions(vec![held])),
            ],
        ),
        Outer::PivotAggregate => step(
            "LazyFrame.pivot",
            left,
            vec![
                ("on", Argument::Columns(vec!["h".to_owned()])),
                ("on_columns", Argument::Table(frames.on_h.clone().lazy())),
                ("index", Argument::Columns(vec!["k".to_owned()])),
                ("values", Argument::Columns(vec!["a".to_owned()])),
                ("aggregate_function", expression(held)),
            ],
        ),
    }
}

/// Builds the expression under test in a scope, owned.
type Maker = Box<dyn Fn(Scope) -> Result<Expr, BuildError> + Sync>;

/// Builds the expression under test in a scope.
type Subject<'a> = &'a (dyn Fn(Scope) -> Result<Expr, BuildError> + Sync);

/// The plan of the expression `subject` standing in `context`, with the chain
/// ending at it, or going on from it.
fn context_plan(
    context: Context,
    chain: Chain,
    subject: Subject<'_>,
    frames: &Frames,
    empty: bool,
) -> Result<LazyFrame, BuildError> {
    let scope = scope_of(context.outer());
    let mut held = subject(scope)?;
    if chain == Chain::Continues {
        held = continued_subject(context, held)?;
    }
    if let Some(nested) = context.nested() {
        held = wrap(nested, held, scope)?;
    }
    if chain == Chain::Continues {
        held = continued_held(context.outer(), held)?;
    }
    host(context.outer(), held, frames, empty)
}

/// The dtype the expression that stands at `context` has, which a join key
/// must be matched by.
fn held_dtype(context: Context, subject: Subject<'_>, probe: &Frames) -> DataType {
    let build = || -> Result<DataType, BuildError> {
        let mut held = subject(LEFT)?;
        if let Some(nested) = context.nested() {
            held = wrap(nested, held, LEFT)?;
        }
        let mut frame = step(
            "LazyFrame.select",
            probe.left(false),
            vec![("exprs", expressions(vec![held]))],
        )?;
        let schema = frame.collect_schema().map_err(BuildError::Polars)?;
        let mut columns = schema.iter();
        Ok(columns
            .next()
            .map_or(DataType::Int64, |(_, dtype)| dtype.clone()))
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(build))
        .ok()
        .and_then(Result::ok)
        .unwrap_or(DataType::Int64)
}

// ---------------------------------------------------------------------------
// The context table

/// The lines of the option table, by what they measured, which the context table reads to find
/// the dtype a form of an entry runs on.
struct OptionIndex {
    lines: HashMap<(&'static str, Option<Representative>, Vec<OptionValue>), Outcome>,
}

impl OptionIndex {
    fn new(lines: &[OptionLine]) -> Self {
        Self {
            lines: lines
                .iter()
                .map(|line| {
                    (
                        (line.entry, line.receiver, line.options.clone()),
                        line.outcome.clone(),
                    )
                })
                .collect(),
        }
    }
}

/// An option given in place of another parameter, and the word it is set to.
type Form = (&'static str, &'static str);

/// The forms of an entry that the context table measures apart, for each of them differs from the
/// entry measured with the parameter it takes the place of: Polars runs a fill by a strategy
/// that looks at other rows, such as `forward`, where a join's key must not, and a fill by a value
/// where it may.
fn forms(entry: &'static Entry) -> Vec<Form> {
    let mut forms = Vec::new();
    for parameter in entry.parameters() {
        if parameter.in_place_of().is_none() {
            continue;
        }
        let Kind::Choice(words) = parameter.kind() else {
            panic!(
                "{} {} is given in place of another parameter and is no choice",
                entry.id().as_str(),
                parameter.name()
            );
        };
        forms.extend(words.iter().map(|word| (parameter.name(), *word)));
    }
    forms
}

/// The receivers a form of `entry` is measured on in a context: those the option table allows
/// the form on, `Int64` before the others, with what it gives on each.
fn form_candidates_of(entry: &'static Entry, form: Form, options: &OptionIndex) -> Vec<Candidate> {
    let order = std::iter::once(Representative::Int64).chain(
        Representative::ALL
            .into_iter()
            .filter(|dtype| *dtype != Representative::Int64),
    );
    order
        .into_iter()
        .filter_map(|receiver| {
            let key = (
                entry.id().as_str(),
                Some(receiver),
                vec![(form.0, form.1.to_owned())],
            );
            match options.lines.get(&key) {
                Some(Outcome::Allowed(produces)) => Some(Candidate {
                    receiver,
                    arguments: Vec::new(),
                    produces: produces.clone(),
                }),
                _ => None,
            }
        })
        .collect()
}

/// Whether Polars drops a value from what the entry follows when it holds a null, so that the
/// column has fewer values than the table has rows: `drop_nulls` and `drop_nans`. The tables
/// measure the entry in a context on a column that holds none, which they drop nothing from:
/// how many rows a column has is for the rows to say, and a column that holds a null in a step
/// whose other values are as long as the table is refused by Polars for its lengths.
fn drops_values(entry: &Entry) -> bool {
    matches!(entry.id().as_str(), "Expr.drop_nulls" | "Expr.drop_nans")
}

/// The column of the sample that has no null in it and is of the dtype `class`, if it has one: `u`
/// holds whole numbers and `f` and its twin on the other table booleans.
fn without_nulls(scope: Scope, class: Representative) -> Option<&'static str> {
    match class {
        Representative::Int64 => Some("u"),
        Representative::Boolean => Some(scope.boolean),
        _ => None,
    }
}

fn context_lines(index: &DtypeIndex, options: &OptionIndex) -> Vec<ContextLine> {
    let mut jobs = Vec::new();
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() != Applies::Frame)
    {
        let forms = if entry.applies() == Applies::Expression {
            forms(entry)
        } else {
            Vec::new()
        };
        for context in Context::all() {
            for chain in Chain::ALL {
                jobs.push((entry, None, context, chain));
                for form in &forms {
                    jobs.push((entry, Some(*form), context, chain));
                }
            }
        }
    }
    let candidates: HashMap<&'static str, Vec<Candidate>> = catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
        .map(|entry| (entry.id().as_str(), index.candidates(entry)))
        .collect();
    let form_candidates: HashMap<(&'static str, Form), Vec<Candidate>> = catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Expression)
        .flat_map(|entry| {
            forms(entry).into_iter().map(|form| {
                (
                    (entry.id().as_str(), form),
                    form_candidates_of(entry, form, options),
                )
            })
        })
        .collect();
    in_parallel(&jobs, |(entry, form, context, chain)| {
        context_line(
            entry,
            *form,
            *context,
            *chain,
            &candidates,
            &form_candidates,
        )
    })
}

fn context_line(
    entry: &'static Entry,
    form: Option<Form>,
    context: Context,
    chain: Chain,
    candidates: &HashMap<&'static str, Vec<Candidate>>,
    form_candidates: &HashMap<(&'static str, Form), Vec<Candidate>>,
) -> ContextLine {
    let id = entry.id().as_str();
    let rules = Rule::governing(id, context);
    let form_options: Vec<OptionValue> = form
        .into_iter()
        .map(|(name, word)| (name, word.to_owned()))
        .collect();
    let (candidate, subject): (Option<Candidate>, Maker) = match entry.applies() {
        Applies::Expression => {
            let all = match form {
                Some(form) => &form_candidates[&(id, form)],
                None => &candidates[id],
            };
            // Where the chain ends, a context that needs a `Boolean` is given
            // a method that gives one. Where it goes on, a test for a value
            // follows, so the method that fits best is measured.
            let chosen = if chain == Chain::Ends && needs_boolean(context) {
                all.iter()
                    .find(|candidate| {
                        candidate.produces.as_ref().is_some_and(|produces| {
                            produces.result.class() == Some(Representative::Boolean)
                        })
                    })
                    .or_else(|| all.first())
            } else {
                all.first()
            };
            let Some(chosen) = chosen.cloned() else {
                return ContextLine {
                    entry: id,
                    form: form_options,
                    context,
                    chain,
                    receiver: None,
                    argument: None,
                    rules,
                    outcome: Outcome::Refused(Reason {
                        stage: Stage::Schema,
                        text: "no dtype of the dtype table fits".to_owned(),
                    }),
                };
            };
            let for_subject = chosen.clone();
            let given: Vec<(&'static str, Argument)> = form
                .into_iter()
                .map(|(name, word)| (name, Argument::Choice(word.to_owned())))
                .collect();
            (
                Some(chosen),
                Box::new(move |scope| {
                    let receiver = without_nulls(scope, for_subject.receiver)
                        .filter(|_| drops_values(entry) && !scope.group);
                    let scope = match receiver {
                        Some(receiver) => Scope { receiver, ..scope },
                        None => scope,
                    };
                    let scope = Scope {
                        literals: LITERAL_IN_CONTEXTS.iter().any(|(owner, _)| *owner == id),
                        ..scope
                    };
                    method_on(entry, &for_subject, scope, given.clone())
                }),
            )
        }
        Applies::Function => (
            None,
            Box::new(move |scope| match id {
                "polars.col" if chain == Chain::Ends && needs_boolean(context) => {
                    Ok(column(scope.boolean))
                }
                "polars.col" => Ok(column(scope.receiver)),
                "polars.lit" if chain == Chain::Ends && needs_boolean(context) => Ok(flag(true)),
                "polars.lit" => Ok(integer(1)),
                "polars.element" => Ok(element()),
                other => panic!("the generator does not know the function {other}"),
            }),
        ),
        Applies::Condition => (None, Box::new(move |_| condition_with(id))),
        Applies::Frame => unreachable!("steps do not stand in a context"),
    };
    let receiver = candidate.as_ref().map(|candidate| candidate.receiver);
    let argument = candidate
        .as_ref()
        .and_then(|candidate| candidate.arguments.first().map(|(_, dtype)| *dtype));
    let (receiver_dtype, argument_dtype) = (
        receiver.unwrap_or(Representative::Int64),
        argument.unwrap_or_else(|| receiver.unwrap_or(Representative::Int64)),
    );
    let probe = Frames::new(receiver_dtype, argument_dtype, &DataType::Int64);
    // Where the chain ends in a join key, the other table's key has the dtype
    // the key has. Where it goes on, the key is cast to the dtype of the other.
    let key =
        if chain == Chain::Ends && matches!(context.outer(), Outer::JoinLeft | Outer::JoinRight) {
            held_dtype(context, &*subject, &probe)
        } else {
            KEY
        };
    let frames = Frames::new(receiver_dtype, argument_dtype, &key);
    let plan =
        |frames: &Frames, empty: bool| context_plan(context, chain, &*subject, frames, empty);
    let measured = measure(&plan, &frames);
    let outcome = match measured {
        Err(reason) => Outcome::Refused(reason),
        Ok(_) => Outcome::Allowed(match (&candidate, entry.applies()) {
            (Some(candidate), _) => candidate.produces.clone(),
            (None, Applies::Function) => standalone(&*subject, &frames),
            (None, _) => None,
        }),
    };
    ContextLine {
        entry: id,
        form: form_options,
        context,
        chain,
        receiver,
        argument,
        rules,
        outcome,
    }
}

/// What an expression gives on its own in a `select`, if it runs there.
fn standalone(subject: Subject<'_>, frames: &Frames) -> Option<Produces> {
    let plan = |frames: &Frames, empty: bool| {
        step(
            "LazyFrame.select",
            frames.left(empty),
            vec![("exprs", expressions(vec![subject(LEFT)?]))],
        )
    };
    let plan: Plan<'_> = &plan;
    measure(plan, frames)
        .ok()
        .and_then(|measured| measured.produces())
}

// ---------------------------------------------------------------------------
// The option table

/// One option of an entry and the values it is given.
struct OptionParameter {
    name: &'static str,
    values: Vec<(String, Argument)>,
    /// Whether every call needs it to mean anything, as a cast needs the dtype it gives.
    needed: bool,
    /// The word of the option that leaves the call as it is when it is not given.
    default: Option<String>,
}

/// The classes a document can hold as a dtype, which a parameter of a dtype takes as its words.
fn dtype_classes() -> Vec<Representative> {
    Representative::ALL
        .into_iter()
        .filter(|class| crate::document::Dtype::new(class.dtype()).is_ok())
        .collect()
}

fn option_parameters(entry: &'static Entry) -> Vec<OptionParameter> {
    entry
        .parameters()
        .iter()
        // The dtype a method converts values to is measured by the cast, for what it accepts is
        // what a cast accepts.
        .filter(|parameter| parameter.converts().is_empty())
        .filter_map(|parameter| {
            let values = match parameter.kind() {
                Kind::Flag => {
                    let mut values = vec![
                        ("true".to_owned(), Argument::Flag(Some(true))),
                        ("false".to_owned(), Argument::Flag(Some(false))),
                    ];
                    if parameter.default() == ParameterDefault::Python("None") {
                        values.push(("unset".to_owned(), Argument::Flag(None)));
                    }
                    values
                }
                Kind::Choice(words) => words
                    .iter()
                    .map(|word| ((*word).to_owned(), Argument::Choice((*word).to_owned())))
                    .collect(),
                // The dtype a call gives is a word of its own, which the class names.
                Kind::Dtype => dtype_classes()
                    .into_iter()
                    .map(|class| (class.name().to_owned(), Argument::Dtype(class.dtype())))
                    .collect(),
                _ => return None,
            };
            Some(OptionParameter {
                name: parameter.name(),
                values,
                needed: parameter.kind() == Kind::Dtype
                    && parameter.default() == ParameterDefault::Required,
                default: default_word(parameter),
            })
        })
        .collect()
}

/// One set of options, each by the parameter, the word the table writes and the argument.
type OptionSet = Vec<(&'static str, String, Argument)>;

/// The sets of options to run: every value of the option when an entry has
/// one, every pair of values of two options when it has more. An option that a call needs, as a
/// cast needs its dtype, is in every set, alone and with each value that is not the default of
/// each of the other options.
fn option_sets(parameters: &[OptionParameter]) -> Vec<OptionSet> {
    let single = |parameter: &OptionParameter| -> Vec<OptionSet> {
        parameter
            .values
            .iter()
            .map(|(text, argument)| vec![(parameter.name, text.clone(), argument.clone())])
            .collect()
    };
    let (needed, others): (Vec<_>, Vec<_>) =
        parameters.iter().partition(|parameter| parameter.needed);
    if let [needed] = needed.as_slice() {
        let with = |first: &OptionSet, other: Option<(&OptionParameter, &(String, Argument))>| {
            let mut set = first.clone();
            if let Some((parameter, (text, argument))) = other {
                set.push((parameter.name, text.clone(), argument.clone()));
            }
            set
        };
        let mut sets = Vec::new();
        for first in single(needed) {
            sets.push(with(&first, None));
            for other in &others {
                for value in &other.values {
                    if other.default.as_deref() != Some(value.0.as_str()) {
                        sets.push(with(&first, Some((other, value))));
                    }
                }
            }
        }
        return sets;
    }
    if let [only] = parameters {
        return single(only);
    }
    let mut sets = Vec::new();
    for (at, first) in parameters.iter().enumerate() {
        for second in &parameters[at + 1..] {
            for (first_text, first_value) in &first.values {
                for (second_text, second_value) in &second.values {
                    sets.push(vec![
                        (first.name, first_text.clone(), first_value.clone()),
                        (second.name, second_text.clone(), second_value.clone()),
                    ]);
                }
            }
        }
    }
    sets
}

/// A step with what it needs besides its options to run on the sample
/// tables, and the options.
fn option_step(
    id: &str,
    frames: &Frames,
    empty: bool,
    options: Vec<(&'static str, Argument)>,
) -> Result<LazyFrame, BuildError> {
    let mut given = match id {
        "LazyFrame.join" => vec![
            ("other", Argument::Table(frames.other(empty))),
            ("on", expressions(vec![column("u")])),
        ],
        "LazyFrame.pivot" => vec![
            ("on", Argument::Columns(vec!["g".to_owned()])),
            ("on_columns", Argument::Table(frames.on_g.clone().lazy())),
            ("index", Argument::Columns(vec!["k".to_owned()])),
            ("values", Argument::Columns(vec!["a".to_owned()])),
        ],
        "LazyFrame.rename" => vec![(
            "mapping",
            Argument::Pairs(vec![(
                Argument::Columns(vec!["a".to_owned()]),
                Argument::String("renamed".to_owned()),
            )]),
        )],
        "LazyFrame.sort" => vec![("by", expressions(vec![column("a")]))],
        other => panic!("teach the generator what {other} needs to run, as it has options"),
    };
    given.extend(options);
    step(id, frames.left(empty), given)
}

/// The candidate of `entry` that follows `receiver` for the option table: the one the dtype table
/// says runs there, and otherwise one whose arguments are of the receiver's own dtype, for the
/// option table measures a method on every dtype, whether or not it runs there without options.
fn option_candidate(
    entry: &'static Entry,
    receiver: Representative,
    index: &DtypeIndex,
) -> Candidate {
    index
        .candidates(entry)
        .into_iter()
        .find(|candidate| candidate.receiver == receiver)
        .unwrap_or_else(|| Candidate {
            receiver,
            arguments: expression_parameters(entry)
                .into_iter()
                .filter(|parameter| parameter.is_needed())
                .map(|parameter| (parameter.name(), receiver))
                .collect(),
            produces: None,
        })
}

fn option_line(
    entry: &'static Entry,
    set: &[(&'static str, String, Argument)],
    candidate: Option<&Candidate>,
) -> OptionLine {
    let id = entry.id().as_str();
    let receiver = candidate.map(|candidate| candidate.receiver);
    let receiver_dtype = receiver.unwrap_or(Representative::Int64);
    let argument = candidate
        .and_then(|candidate| candidate.arguments.first().map(|(_, dtype)| *dtype))
        .unwrap_or(receiver_dtype);
    let frames = Frames::new(receiver_dtype, argument, &DataType::Int64);
    let chosen: Vec<(&'static str, Argument)> = set
        .iter()
        .map(|(name, _, argument)| (*name, argument.clone()))
        .collect();
    // What an option is given in place of is not given with it.
    let stood_in: Vec<&str> = entry
        .parameters()
        .iter()
        .filter(|parameter| chosen.iter().any(|(name, _)| *name == parameter.name()))
        .filter_map(|parameter| parameter.in_place_of())
        .collect();
    let candidate = candidate.map(|candidate| Candidate {
        arguments: candidate
            .arguments
            .iter()
            .filter(|(name, _)| !stood_in.contains(name))
            .copied()
            .collect(),
        ..candidate.clone()
    });
    let plan = |frames: &Frames, empty: bool| -> Result<LazyFrame, BuildError> {
        if let Some(candidate) = &candidate {
            let mut extra = chosen.clone();
            if id == "Expr.over" {
                extra.push(("order_by", expressions(vec![column("k")])));
            }
            let call = method_on(entry, candidate, LEFT, extra)?;
            step(
                "LazyFrame.select",
                frames.left(empty),
                vec![("exprs", expressions(vec![call]))],
            )
        } else {
            option_step(id, frames, empty, chosen.clone())
        }
    };
    let outcome = match measure(&plan, &frames) {
        Ok(measured) => Outcome::Allowed(candidate.as_ref().and_then(|_| measured.produces())),
        Err(reason) => Outcome::Refused(reason),
    };
    let options: Vec<OptionValue> = set
        .iter()
        .map(|(name, text, _)| (*name, text.clone()))
        .collect();
    OptionLine {
        entry: id,
        receiver,
        options,
        outcome,
    }
}

fn option_lines(index: &DtypeIndex) -> Vec<OptionLine> {
    let mut jobs = Vec::new();
    for entry in catalog::entries()
        .iter()
        .filter(|entry| matches!(entry.applies(), Applies::Expression | Applies::Frame))
    {
        let parameters = option_parameters(entry);
        if parameters.is_empty() {
            continue;
        }
        let sets = option_sets(&parameters);
        // A method is measured on each dtype, for what an option allows depends on it, as the
        // strategy of a fill and the dtype of a cast do; a step follows none.
        let candidates: Vec<Option<Candidate>> = if entry.applies() == Applies::Expression {
            Representative::ALL
                .into_iter()
                .map(|receiver| Some(option_candidate(entry, receiver, index)))
                .collect()
        } else {
            vec![None]
        };
        for candidate in candidates {
            for set in &sets {
                jobs.push((entry, set.clone(), candidate.clone()));
            }
        }
    }
    in_parallel(&jobs, |(entry, set, candidate)| {
        option_line(entry, set, candidate.as_ref())
    })
}
