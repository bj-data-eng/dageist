//! Writing a document as Python Polars source: the code that builds the same
//! tables with Python Polars.
//!
//! [`Document::source`] writes a whole module: a function `run` that is given
//! the table the document starts from and each input its steps name, typed, and
//! gives the lazy frame the steps build. [`Step::source`] writes one step as
//! the method call it stands for, such as `.sort("a", descending=True)`, which
//! is also what its line of the function holds.
//!
//! What is written follows from the entry a call uses and the arguments it was
//! given, never from what the step means:
//!
//! - An entry is called by its Python name: a function of `polars` as
//!   `pl.col(...)`, anything else as the method `.name(...)`.
//! - The first parameter is written without its name when it is given and
//!   Python takes it by position. The items of a starred parameter follow it,
//!   with the default of any parameter that stands between them, because
//!   Python takes them by position only. A list that stands just before a
//!   starred parameter is written as its items, which Python reads the same.
//!   Every other argument is written with its name, in the order of Python's
//!   signature.
//! - The entries of a double-starred parameter that writes columns are written
//!   as `expression.alias("name")`; those of any other are a dictionary, as
//!   `**{"name": value}`.
//! - A field alone is written as its name where a frame's method takes a list
//!   of expressions, as `.sort("a")`; an expression is written in full
//!   anywhere else, so that a text is never taken for a column name.
//! - Values are written as Python writes them: a whole float as `1.0`, text
//!   quoted, a flag as `True`, `False` or `None`, a dtype as `pl.Int64`.
//! - A connected table, value or list is a parameter of `run`, named by the
//!   input's name as a Python name. A table is a lazy frame, so a pivot's
//!   `on_columns`, which Python takes as a data frame, is written
//!   `name.collect()`. A value is a literal of its dtype, as `pl.lit(name,
//!   dtype=pl.Int64)`, and is typed by the Python type of that dtype.
//!
//! A module that cannot run as written is marked: it starts with a comment for
//! each reason, and [`Source::not_runnable`] lists them. A step that holds
//! something still to be picked is written with `None` there; an input that is
//! not connected is a parameter with no type; names that are the same Python
//! name cannot be told apart.

use std::collections::BTreeSet;
use std::fmt::{self, Write as _};

use polars::prelude::DataType;

use crate::catalog::{Applies, Entry, Kind, Parameter, ParameterDefault, Variadic, Writes};

use super::dtype::Dtype;
use super::inputs::{Binding, Inputs};
use super::model::{Call, Document, Expression, Start, Step, StepId, Value};

/// The parameters whose argument Python takes as a data frame, which a table,
/// held as a lazy frame, is collected to: a pivot's `on_columns`. The engine
/// collects it too when it builds the step.
const EAGER_TABLES: [(&str, &str); 1] = [("LazyFrame.pivot", "on_columns")];

/// What the Python names `run` and the imports use, which no input may be
/// called.
const RESERVED: [&str; 6] = ["pl", "run", "date", "datetime", "time", "timedelta"];

/// Python's keywords, which no name may be.
const KEYWORDS: [&str; 35] = [
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// A line of a [`Source`]: its text, and the step it is the call of when it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLine {
    text: String,
    step: Option<StepId>,
}

impl SourceLine {
    /// The line, without its end.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The step the line is the call of, or the step a note about it is for.
    #[must_use]
    pub const fn step(&self) -> Option<&StepId> {
        self.step.as_ref()
    }
}

/// Why source cannot run as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotRunnableReason {
    /// The step holds something that is still to be picked, written as
    /// `None`.
    Unfinished,
    /// The name is not a table, value or list connected to the node, or is not
    /// the kind the step takes there.
    NotConnected(String),
    /// Two names are the same Python name.
    SameName(String, String),
    /// The name is also the Python name of the table the document starts from.
    NamedLikeFrame(String),
}

impl fmt::Display for NotRunnableReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unfinished => formatter.write_str("something in it is still to be picked"),
            Self::NotConnected(name) => write!(
                formatter,
                "{name:?} is not connected as what the step takes there"
            ),
            Self::SameName(first, second) => write!(
                formatter,
                "{first:?} and {second:?} are the same name in Python"
            ),
            Self::NamedLikeFrame(name) => write!(
                formatter,
                "{name:?} is also the name of the table the document starts from"
            ),
        }
    }
}

/// Source that cannot run as written, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotRunnable {
    step: Option<StepId>,
    reason: NotRunnableReason,
}

impl NotRunnable {
    /// The step it is about, where it is about one.
    #[must_use]
    pub const fn step(&self) -> Option<&StepId> {
        self.step.as_ref()
    }

    /// Why.
    #[must_use]
    pub const fn reason(&self) -> &NotRunnableReason {
        &self.reason
    }
}

/// A document as the module of Python source that builds the same tables,
/// line by line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    lines: Vec<SourceLine>,
    not_runnable: Vec<NotRunnable>,
}

impl Source {
    /// The lines of the module.
    #[must_use]
    pub fn lines(&self) -> &[SourceLine] {
        &self.lines
    }

    /// Everything that keeps the source from running as written; empty when it
    /// runs.
    #[must_use]
    pub fn not_runnable(&self) -> &[NotRunnable] {
        &self.not_runnable
    }

    /// The module as text, each line ended.
    #[must_use]
    pub fn text(&self) -> String {
        self.lines.iter().fold(String::new(), |mut text, line| {
            text.push_str(&line.text);
            text.push('\n');
            text
        })
    }
}

impl Step {
    /// The step as the Python method call it stands for, such as
    /// `.with_columns(pl.col("a").shift(1).alias("b"))`. `inputs` say which
    /// connected names are values, so that a value is written with its dtype.
    #[must_use]
    pub fn source(&self, inputs: &Inputs) -> String {
        Writer { inputs }.call(self.call())
    }
}

impl Document {
    /// The document as a module of Python source: the function `run`, which is
    /// given `frame`, a lazy frame, and every connected input the steps name,
    /// and gives the lazy frame the steps build, each step called on the one
    /// before. Collect it to get the table the document gives.
    ///
    /// `inputs` say what each connected name is: a table, a value of a dtype,
    /// or a list.
    #[must_use]
    pub fn source(&self, frame: &str, inputs: &Inputs) -> Source {
        let writer = Writer { inputs };
        let frame_name = identifier(frame);
        let mut uses = Uses::default();
        let mut not_runnable = Vec::new();
        for step in self.steps() {
            if step.first_hole().is_some() {
                not_runnable.push(NotRunnable {
                    step: Some(step.id().clone()),
                    reason: NotRunnableReason::Unfinished,
                });
            }
            uses.call(step.id(), step.call());
        }
        // Everything the steps name stands as a parameter, in the order they first name it.
        let mut parameters = vec![(
            frame_name.clone(),
            "pl.LazyFrame".to_owned(),
            frame.to_owned(),
        )];
        let mut imports = BTreeSet::new();
        let mut declared: Vec<&str> = Vec::new();
        for (name, step_id, expects) in &uses.names {
            let (annotation, import, fits) = match (inputs.binding(name), expects) {
                (Some(Binding::Table), Expects::Table) => ("pl.LazyFrame".to_owned(), None, true),
                (Some(Binding::Value(dtype)), Expects::Value) => {
                    let (annotation, import) = python_type(&dtype);
                    (annotation.to_owned(), import, true)
                }
                (Some(Binding::List(_)), Expects::Value) => ("pl.Series".to_owned(), None, true),
                _ => (String::new(), None, false),
            };
            if !fits {
                not_runnable.push(NotRunnable {
                    step: Some(step_id.clone()),
                    reason: NotRunnableReason::NotConnected(name.clone()),
                });
            }
            // A name a later step uses again is a parameter already.
            if declared.contains(&name.as_str()) {
                continue;
            }
            declared.push(name);
            if identifier(name) == frame_name {
                not_runnable.push(NotRunnable {
                    step: None,
                    reason: NotRunnableReason::NamedLikeFrame(name.clone()),
                });
                continue;
            }
            imports.extend(import);
            parameters.push((identifier(name), annotation, name.clone()));
        }
        let mut taken: Vec<(&str, &str)> = Vec::new();
        for (name, _, original) in &parameters {
            if let Some((_, other)) = taken.iter().find(|(seen, _)| seen == name)
                && other != original
            {
                not_runnable.push(NotRunnable {
                    step: None,
                    reason: NotRunnableReason::SameName((*other).to_owned(), original.clone()),
                });
            }
            taken.push((name, original));
        }

        let mut lines = Vec::new();
        let mut line = |text: String, step: Option<StepId>| lines.push(SourceLine { text, step });
        for problem in &not_runnable {
            line(
                format!("# Not runnable as written: {}.", note(problem)),
                problem.step.clone(),
            );
        }
        for module in imports
            .iter()
            .map(|(module, _)| *module)
            .collect::<BTreeSet<_>>()
        {
            let names: Vec<&str> = imports
                .iter()
                .filter(|(from, _)| *from == module)
                .map(|(_, name)| *name)
                .collect();
            line(format!("from {module} import {}", names.join(", ")), None);
        }
        if !imports.is_empty() {
            line(String::new(), None);
        }
        line("import polars as pl".to_owned(), None);
        line(String::new(), None);
        line(String::new(), None);
        line("def run(".to_owned(), None);
        for (name, annotation, _) in &parameters {
            let typed = if annotation.is_empty() {
                name.clone()
            } else {
                format!("{name}: {annotation}")
            };
            line(format!("    {typed},"), None);
        }
        line(") -> pl.LazyFrame:".to_owned(), None);
        if self.steps().is_empty() {
            line(format!("    return {frame_name}"), None);
        } else {
            line("    return (".to_owned(), None);
            line(format!("        {frame_name}"), None);
            for step in self.steps() {
                line(
                    format!("        {}", writer.call(step.call())),
                    Some(step.id().clone()),
                );
            }
            line("    )".to_owned(), None);
        }
        Source {
            lines,
            not_runnable,
        }
    }
}

fn note(problem: &NotRunnable) -> String {
    match &problem.step {
        Some(step) => format!("step {:?}, {}", step.as_str(), problem.reason),
        None => problem.reason.to_string(),
    }
}

/// What a connected name is used as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expects {
    Table,
    Value,
}

/// The connected names a document uses, in the order it first uses them, with
/// the step that does and what it uses them as; a name used as another kind
/// is noted again.
#[derive(Default)]
struct Uses {
    names: Vec<(String, StepId, Expects)>,
}

impl Uses {
    fn note(&mut self, step: &StepId, name: &str, expects: Expects) {
        if !self
            .names
            .iter()
            .any(|(seen, _, kind)| seen == name && *kind == expects)
        {
            self.names.push((name.to_owned(), step.clone(), expects));
        }
    }

    fn call(&mut self, step: &StepId, call: &Call) {
        for given in call.arguments() {
            self.value(step, given.value());
        }
    }

    fn value(&mut self, step: &StepId, value: &Value) {
        match value {
            Value::Table(name) => self.note(step, name, Expects::Table),
            Value::Expression(expression) => {
                match expression.start() {
                    Start::Call(start) => self.call(step, start),
                    Start::Input(name) => self.note(step, name, Expects::Value),
                    Start::Hole => {}
                }
                for link in expression.links() {
                    self.call(step, link);
                }
            }
            Value::Many(items) => items.iter().for_each(|item| self.value(step, item)),
            Value::Pairs(pairs) => pairs.iter().for_each(|(key, entry)| {
                self.value(step, key);
                self.value(step, entry);
            }),
            _ => {}
        }
    }
}

/// The Python type of a value of `dtype`, and what imports it.
fn python_type(dtype: &DataType) -> (&'static str, Option<(&'static str, &'static str)>) {
    match dtype {
        DataType::Boolean => ("bool", None),
        DataType::String => ("str", None),
        DataType::Float32 | DataType::Float64 => ("float", None),
        DataType::Date => ("date", Some(("datetime", "date"))),
        DataType::Datetime(..) => ("datetime", Some(("datetime", "datetime"))),
        DataType::Time => ("time", Some(("datetime", "time"))),
        DataType::Duration(_) => ("timedelta", Some(("datetime", "timedelta"))),
        dtype if dtype.is_integer() => ("int", None),
        _ => ("object", None),
    }
}

/// `name` as a Python name: letters, digits and underscores, not starting with
/// a digit, and not a keyword or a name the module uses.
fn identifier(name: &str) -> String {
    let mut written: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if written.is_empty() || written.starts_with(|c: char| c.is_ascii_digit()) {
        written.insert(0, '_');
    }
    if KEYWORDS.contains(&written.as_str()) || RESERVED.contains(&written.as_str()) {
        written.push('_');
    }
    written
}

struct Writer<'a> {
    inputs: &'a Inputs,
}

impl Writer<'_> {
    /// Python's text for the call of `call`: its name and its arguments, in
    /// brackets.
    fn call(&self, call: &Call) -> String {
        format!(
            "{}({})",
            python_name(call.entry()),
            self.arguments(call).join(", ")
        )
    }

    /// The arguments of `call`, each as it is written in Python's call.
    fn arguments(&self, call: &Call) -> Vec<String> {
        let entry = call.entry();
        let parameters = entry.parameters();
        let given = |parameter: &Parameter| call.argument(parameter.name());
        let items_of = |parameter: &Parameter| match given(parameter) {
            Some(Value::Many(items)) => items.as_slice(),
            _ => &[],
        };
        // The positional arguments run to the starred parameter that is given items, or else
        // to the first parameter alone.
        let starred_at = parameters
            .iter()
            .position(|parameter| is_starred(parameter) && !items_of(parameter).is_empty());
        let positional_to = starred_at.map_or_else(
            || {
                parameters
                    .first()
                    .filter(|first| given(first).is_some() && positional(first))
                    .map(|_| 0)
            },
            Some,
        );
        let mut written = Vec::new();
        let mut by_position = vec![false; parameters.len()];
        if let Some(last) = positional_to {
            for (at, parameter) in parameters.iter().enumerate().take(last + 1) {
                by_position[at] = true;
                let next_is_starred = parameters.get(at + 1).is_some_and(is_starred);
                written.extend(self.positional_argument(
                    entry,
                    parameter,
                    given(parameter),
                    next_is_starred,
                ));
            }
        }
        // What a double-starred parameter writes as columns stands among the positional
        // arguments.
        for (at, parameter) in parameters.iter().enumerate() {
            if is_double_starred(parameter) && parameter.writes() == Writes::Columns {
                by_position[at] = true;
                if let Some(Value::Pairs(pairs)) = given(parameter) {
                    written.extend(pairs.iter().map(|(name, expression)| {
                        format!(
                            "{}.alias({})",
                            self.value(entry, expression, false),
                            self.value(entry, name, false)
                        )
                    }));
                }
            }
        }
        for (at, parameter) in parameters.iter().enumerate() {
            if by_position[at] {
                continue;
            }
            let Some(argument) = given(parameter) else {
                continue;
            };
            if is_double_starred(parameter) {
                if let Value::Pairs(pairs) = argument
                    && !pairs.is_empty()
                {
                    written.push(format!("**{}", self.pairs(entry, pairs)));
                }
            } else {
                written.push(format!(
                    "{}={}",
                    parameter.name(),
                    self.parameter_value(entry, parameter, argument)
                ));
            }
        }
        written
    }

    /// What is written by position for `parameter`: the items of a starred
    /// parameter, one argument each; a list just before one, the same; and
    /// otherwise the one argument, which is the default where nothing was given
    /// for a parameter that stands before one that is.
    fn positional_argument(
        &self,
        entry: &Entry,
        parameter: &Parameter,
        argument: Option<&Value>,
        before_starred: bool,
    ) -> Vec<String> {
        let items = |items: &[Value]| {
            items
                .iter()
                .map(|item| self.value(entry, item, true))
                .collect::<Vec<_>>()
        };
        match argument {
            Some(Value::Many(list)) if is_starred(parameter) => items(list),
            Some(Value::Many(list))
                if before_starred && matches!(parameter.kind(), Kind::Expression(_)) =>
            {
                items(list)
            }
            Some(argument) => vec![self.parameter_value(entry, parameter, argument)],
            None if is_starred(parameter) => Vec::new(),
            None => vec![match parameter.default() {
                ParameterDefault::Python(text) => text.to_owned(),
                ParameterDefault::Required => "None".to_owned(),
            }],
        }
    }

    /// `argument` as the value of `parameter`. A list of one expression is that
    /// expression, which Python reads the same; a table is collected where
    /// Python takes a data frame.
    fn parameter_value(&self, entry: &Entry, parameter: &Parameter, argument: &Value) -> String {
        match argument {
            Value::Many(list) if matches!(parameter.kind(), Kind::Expression(_)) => {
                match list.as_slice() {
                    [only] => self.value(entry, only, true),
                    _ => format!(
                        "[{}]",
                        list.iter()
                            .map(|item| self.value(entry, item, true))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                }
            }
            Value::Table(name)
                if EAGER_TABLES.contains(&(entry.id().as_str(), parameter.name())) =>
            {
                format!("{}.collect()", identifier(name))
            }
            other => self.value(entry, other, false),
        }
    }

    /// `argument` as Python writes it. `in_list` is where an item of a list of
    /// expressions stands, and a frame's method takes a field alone there as its
    /// name.
    fn value(&self, entry: &Entry, argument: &Value, in_list: bool) -> String {
        match argument {
            Value::Expression(inner) => match field_alone(inner) {
                Some(name) if in_list && entry.applies() == Applies::Frame => string(name),
                _ => self.expression(inner),
            },
            Value::Pairs(pairs) => self.pairs(entry, pairs),
            Value::Many(items) => format!(
                "[{}]",
                items
                    .iter()
                    .map(|item| self.value(entry, item, false))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Value::Table(name) => identifier(name),
            plain => scalar(plain).unwrap_or_default(),
        }
    }

    /// The dictionary of `pairs`.
    fn pairs(&self, entry: &Entry, pairs: &[(Value, Value)]) -> String {
        let entries: Vec<String> = pairs
            .iter()
            .map(|(name, entry_value)| {
                format!(
                    "{}: {}",
                    self.value(entry, name, false),
                    self.value(entry, entry_value, false)
                )
            })
            .collect();
        format!("{{{}}}", entries.join(", "))
    }

    /// An expression: where it starts, then each link.
    fn expression(&self, expression: &Expression) -> String {
        let mut text = match expression.start() {
            Start::Call(start) => self.call(start),
            // A connected value is a literal of its dtype, and a list is a series that Polars
            // takes as one.
            Start::Input(name) => match self.inputs.binding(name) {
                Some(Binding::Value(dtype)) => match Dtype::new(dtype) {
                    Ok(dtype) => format!(
                        "pl.lit({}, dtype={})",
                        identifier(name),
                        dtype_text(&dtype.to_string())
                    ),
                    Err(_) => format!("pl.lit({})", identifier(name)),
                },
                _ => format!("pl.lit({})", identifier(name)),
            },
            Start::Hole => "None".to_owned(),
        };
        for link in expression.links() {
            text.push_str(&self.call(link));
        }
        text
    }
}

/// How Python names the entry: a function of `polars` as `pl.name`, and
/// anything else as the method after the dot that follows its owner.
fn python_name(entry: &Entry) -> String {
    let id = entry.id().as_str();
    match id.split_once('.') {
        Some(("polars", name)) => format!("pl.{name}"),
        Some((_, method)) => format!(".{method}"),
        None => id.to_owned(),
    }
}

fn is_starred(parameter: &Parameter) -> bool {
    parameter.variadic() == Some(Variadic::Args)
}

fn is_double_starred(parameter: &Parameter) -> bool {
    parameter.variadic() == Some(Variadic::Kwargs)
}

/// Whether Python takes `parameter` by position: it is not one that follows a
/// bare `*` or a starred parameter, and it is not a double-starred one.
fn positional(parameter: &Parameter) -> bool {
    !parameter.is_keyword_only() && !is_double_starred(parameter)
}

/// `value` as Python writes it where it is not made of other values or an
/// expression, or `None` where it is. A table is its own name.
pub(super) fn scalar(value: &Value) -> Option<String> {
    Some(match value {
        Value::Columns(names) => match names.as_slice() {
            [only] => string(only),
            _ => format!(
                "[{}]",
                names
                    .iter()
                    .map(|name| string(name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
        Value::Integer(number) => number.to_string(),
        Value::Float(number) => float(*number),
        Value::String(text) | Value::Choice(text) | Value::Duration(text) | Value::Path(text) => {
            string(text)
        }
        Value::Flag(Some(true)) => "True".to_owned(),
        Value::Flag(Some(false)) => "False".to_owned(),
        Value::Flag(None) => "None".to_owned(),
        Value::Dtype(dtype) => dtype_text(&dtype.to_string()),
        Value::Table(name) => identifier(name),
        Value::Expression(_) | Value::Pairs(_) | Value::Many(_) => return None,
    })
}

/// The name of the column `expression` is, when it is a field and nothing more.
pub(super) fn field_alone(expression: &Expression) -> Option<&str> {
    let Start::Call(start) = expression.start() else {
        return None;
    };
    if start.entry().id().as_str() != "polars.col" || !expression.links().is_empty() {
        return None;
    }
    let Some(Value::Columns(names)) = start.argument("name") else {
        return None;
    };
    // `more_names` makes it several columns.
    match (names.as_slice(), start.argument("more_names")) {
        ([only], None) => Some(only),
        ([only], Some(Value::Many(more))) if more.is_empty() => Some(only),
        _ => None,
    }
}

/// `text` as a Python string literal in double quotes.
pub(super) fn string(text: &str) -> String {
    let mut written = String::with_capacity(text.len() + 2);
    written.push('"');
    for character in text.chars() {
        match character {
            '"' => written.push_str("\\\""),
            '\\' => written.push_str("\\\\"),
            '\n' => written.push_str("\\n"),
            '\r' => written.push_str("\\r"),
            '\t' => written.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(written, "\\x{:02x}", u32::from(control));
            }
            other => written.push(other),
        }
    }
    written.push('"');
    written
}

/// `number` as Python's `repr` writes it: the shortest digits that read back
/// as the same number, with a fraction or an exponent so that it is a float,
/// and an exponent only where the number is below 1e-4 or from 1e16 on.
pub(super) fn float(number: f64) -> String {
    if number == 0.0 {
        return if number.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    let sign = if number < 0.0 { "-" } else { "" };
    let scientific = format!("{:e}", number.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .unwrap_or((scientific.as_str(), "0"));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let exponent: i32 = exponent.parse().unwrap_or(0);
    // The number is 0.DIGITS times ten to this power.
    let point = exponent + 1;
    let length = i32::try_from(digits.len()).unwrap_or(i32::MAX);
    let body = if (-3..=16).contains(&point) {
        if point <= 0 {
            format!(
                "0.{}{digits}",
                "0".repeat(usize::try_from(-point).unwrap_or(0))
            )
        } else if point >= length {
            format!(
                "{digits}{}.0",
                "0".repeat(usize::try_from(point - length).unwrap_or(0))
            )
        } else {
            let (whole, fraction) = digits.split_at(usize::try_from(point).unwrap_or(0));
            format!("{whole}.{fraction}")
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let fraction = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        let power = exponent;
        format!(
            "{first}{fraction}e{}{:02}",
            if power < 0 { '-' } else { '+' },
            power.abs()
        )
    };
    format!("{sign}{body}")
}

/// The dtype `text`, which a document writes as Python prints it, as Python
/// source: each dtype's name is the attribute of `pl`, as in
/// `pl.List(pl.Int64)`, and a name before `=` or in quotes stays as it is.
fn dtype_text(text: &str) -> String {
    let mut written = String::with_capacity(text.len() + 8);
    let characters: Vec<char> = text.chars().collect();
    let mut at = 0;
    while at < characters.len() {
        let character = characters[at];
        if character == '\'' || character == '"' {
            // A quoted text, such as a time zone, is copied up to its closing quote.
            let close = characters[at + 1..]
                .iter()
                .position(|candidate| *candidate == character)
                .map_or(characters.len() - 1, |found| at + 1 + found);
            written.extend(&characters[at..=close]);
            at = close + 1;
        } else if character.is_ascii_alphabetic() || character == '_' {
            let end = characters[at..]
                .iter()
                .position(|candidate| !(candidate.is_ascii_alphanumeric() || *candidate == '_'))
                .map_or(characters.len(), |found| at + found);
            let word: String = characters[at..end].iter().collect();
            let is_name = characters.get(end) == Some(&'=') || word == "None";
            if !is_name {
                written.push_str("pl.");
            }
            written.push_str(&word);
            at = end;
        } else {
            written.push(character);
            at += 1;
        }
    }
    written
}
