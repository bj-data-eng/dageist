//! The filled-in arguments an entry is built from, and why building fails.

use std::collections::BTreeMap;

use polars::prelude::{DataType, Expr, LazyFrame, PolarsError, TimeZone};

use super::{
    DurationError, Entry, Kind, Parameter, PatternError, Shape, TimeZoneError, ValueType,
    check_pattern, check_time_zone,
};

/// What the text of a pattern stands for: a regular expression, or, where a flag such as `literal`
/// says so, plain text that matches itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternMeans {
    /// A regular expression, which must be one Polars can compile.
    RegularExpression,
    /// Plain text, which is never refused.
    PlainText,
}

/// One filled-in argument, ready for Polars. Its variant follows the
/// parameter's kind; a parameter of shape many or per item holds `Many`.
#[derive(Clone)]
pub enum Argument {
    /// A built expression.
    Expression(Expr),
    /// Names of input columns.
    Columns(Vec<String>),
    /// A whole number typed in.
    Integer(i64),
    /// A floating-point number typed in.
    Float(f64),
    /// Text typed in.
    String(String),
    /// True, false, or unset, which leaves the choice to Polars.
    Flag(Option<bool>),
    /// One word of a closed set.
    Choice(String),
    /// A Polars dtype.
    Dtype(DataType),
    /// A Polars duration string.
    Duration(String),
    /// A regular expression, as typed: whether it is one is asked when the entry is built.
    Pattern(String),
    /// A time zone as typed, or none: whether it is one is asked when the entry is built.
    TimeZone(Option<String>),
    /// A frame from the graph.
    Table(LazyFrame),
    /// A file, folder or cloud location.
    Path(String),
    /// Key and value entries.
    Pairs(Vec<(Argument, Argument)>),
    /// An ordered list.
    Many(Vec<Argument>),
}

impl Argument {
    const fn kind_name(&self) -> &'static str {
        match self {
            Self::Expression(_) => "an expression",
            Self::Columns(_) => "column names",
            Self::Integer(_) => "a whole number",
            Self::Float(_) => "a number",
            Self::String(_) => "text",
            Self::Flag(_) => "a flag",
            Self::Choice(_) => "a choice",
            Self::Dtype(_) => "a dtype",
            Self::Duration(_) => "a duration",
            Self::Pattern(_) => "a pattern",
            Self::TimeZone(_) => "a time zone",
            Self::Table(_) => "a table",
            Self::Path(_) => "a path",
            Self::Pairs(_) => "pairs",
            Self::Many(_) => "a list",
        }
    }
}

/// Why an entry could not be built.
#[derive(Debug)]
pub enum BuildError {
    /// A required argument was not given.
    Missing {
        /// The parameter's name.
        parameter: &'static str,
    },
    /// An argument of another kind was given.
    WrongKind {
        /// The parameter's name.
        parameter: &'static str,
        /// What the parameter takes.
        expected: &'static str,
        /// What it was given.
        found: &'static str,
    },
    /// A choice that is not one of the parameter's words.
    UnknownChoice {
        /// The parameter's name.
        parameter: &'static str,
        /// The word given.
        word: String,
    },
    /// An argument for a name the entry has no parameter of.
    UnknownParameter {
        /// The name given.
        parameter: String,
    },
    /// Two arguments for the same parameter.
    Repeated {
        /// The parameter's name.
        parameter: &'static str,
    },
    /// A whole number outside the range its parameter takes, as Python's binding refuses it.
    OutOfRange {
        /// The parameter's name.
        parameter: &'static str,
        /// What the parameter takes.
        expected: &'static str,
    },
    /// A link of a condition that cannot follow the state of the condition it
    /// was given, such as a `then` after a `then`.
    OutOfOrder {
        /// The state the link needs, such as `then`.
        expected: &'static str,
        /// The state it was given.
        found: &'static str,
    },
    /// A pattern that Polars cannot compile as a regular expression.
    NotAPattern {
        /// The parameter's name.
        parameter: &'static str,
        /// What Polars said of it.
        error: PatternError,
    },
    /// A time zone that Polars does not read.
    NotATimeZone {
        /// The parameter's name.
        parameter: &'static str,
        /// What Polars said of it.
        error: TimeZoneError,
    },
    /// A duration string that Polars does not read, or cannot hold.
    NotADuration {
        /// The parameter's name.
        parameter: &'static str,
        /// What Polars said of it.
        error: DurationError,
    },
    /// Polars refused the arguments.
    Polars(PolarsError),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { parameter } => write!(formatter, "{parameter} must be given"),
            Self::WrongKind {
                parameter,
                expected,
                found,
            } => write!(formatter, "{parameter} takes {expected}, not {found}"),
            Self::UnknownChoice { parameter, word } => {
                write!(formatter, "{parameter} has no choice {word:?}")
            }
            Self::UnknownParameter { parameter } => {
                write!(formatter, "there is no parameter {parameter}")
            }
            Self::Repeated { parameter } => {
                write!(formatter, "{parameter} was given more than once")
            }
            Self::OutOfRange {
                parameter,
                expected,
            } => write!(formatter, "{parameter} takes {expected}"),
            Self::OutOfOrder { expected, found } => {
                write!(
                    formatter,
                    "this needs a {expected} before it, not a {found}"
                )
            }
            Self::NotAPattern { parameter, error } => {
                write!(
                    formatter,
                    "{parameter} is not a valid regular expression: {error}"
                )
            }
            Self::NotATimeZone { parameter, error } => {
                write!(formatter, "{parameter} is not a time zone: {error}")
            }
            Self::NotADuration { parameter, error } => {
                write!(formatter, "{parameter} is not a duration: {error}")
            }
            Self::Polars(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotAPattern { error, .. } => Some(error),
            Self::NotADuration { error, .. } => Some(error),
            Self::NotATimeZone { error, .. } => Some(error),
            Self::Polars(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PolarsError> for BuildError {
    fn from(error: PolarsError) -> Self {
        Self::Polars(error)
    }
}

/// The filled-in arguments of one use of an entry, by Python parameter name.
/// A parameter left out takes its default.
#[derive(Clone, Default)]
pub struct Arguments {
    given: BTreeMap<&'static str, Argument>,
}

impl Arguments {
    /// No arguments: every parameter takes its default.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// These arguments with `parameter` set to `argument`, unchecked. The
    /// tests of the build functions use it to hand them what
    /// [`Entry::arguments`] would refuse.
    #[cfg(test)]
    #[must_use]
    pub(super) fn with(mut self, parameter: &'static str, argument: Argument) -> Self {
        self.given.insert(parameter, argument);
        self
    }

    /// The argument given for `parameter`, if any.
    #[must_use]
    pub fn get(&self, parameter: &str) -> Option<&Argument> {
        self.given.get(parameter)
    }

    /// The error for an argument of another kind than the parameter takes.
    pub(super) fn wrong(
        parameter: &'static str,
        expected: &'static str,
        found: &Argument,
    ) -> BuildError {
        BuildError::WrongKind {
            parameter,
            expected,
            found: found.kind_name(),
        }
    }

    /// The expression given for `parameter`, if one was given.
    ///
    /// # Errors
    ///
    /// When another kind of argument was given.
    pub fn expression(&self, parameter: &'static str) -> Result<Option<Expr>, BuildError> {
        match self.get(parameter) {
            None => Ok(None),
            Some(Argument::Expression(expr)) => Ok(Some(expr.clone())),
            Some(other) => Err(Self::wrong(parameter, "an expression", other)),
        }
    }

    /// The expression given for `parameter`.
    ///
    /// # Errors
    ///
    /// When none was given, or another kind of argument was.
    pub fn required_expression(&self, parameter: &'static str) -> Result<Expr, BuildError> {
        self.expression(parameter)?
            .ok_or(BuildError::Missing { parameter })
    }

    /// The expressions given for `parameter`: a list, or one expression.
    ///
    /// # Errors
    ///
    /// When anything but expressions was given.
    pub fn expressions(&self, parameter: &'static str) -> Result<Vec<Expr>, BuildError> {
        match self.get(parameter) {
            None => Ok(Vec::new()),
            Some(Argument::Expression(expr)) => Ok(vec![expr.clone()]),
            Some(Argument::Many(items)) => items
                .iter()
                .map(|item| match item {
                    Argument::Expression(expr) => Ok(expr.clone()),
                    other => Err(Self::wrong(parameter, "expressions", other)),
                })
                .collect(),
            Some(other) => Err(Self::wrong(parameter, "expressions", other)),
        }
    }

    /// The column names given for `parameter`: one list of names, or a list
    /// of them, as a parameter of shape many holds, joined in order.
    ///
    /// # Errors
    ///
    /// When another kind of argument was given.
    pub fn columns(&self, parameter: &'static str) -> Result<Vec<String>, BuildError> {
        match self.get(parameter) {
            None => Ok(Vec::new()),
            Some(Argument::Columns(names)) => Ok(names.clone()),
            Some(Argument::Many(items)) => {
                let mut names = Vec::new();
                for item in items {
                    match item {
                        Argument::Columns(more) => names.extend(more.iter().cloned()),
                        other => return Err(Self::wrong(parameter, "column names", other)),
                    }
                }
                Ok(names)
            }
            Some(other) => Err(Self::wrong(parameter, "column names", other)),
        }
    }

    /// The flag given for `parameter`, or `default` when it was not given.
    ///
    /// # Errors
    ///
    /// When another kind of argument was given.
    pub fn flag(&self, parameter: &'static str, default: bool) -> Result<bool, BuildError> {
        match self.get(parameter) {
            None | Some(Argument::Flag(None)) => Ok(default),
            Some(Argument::Flag(Some(value))) => Ok(*value),
            Some(other) => Err(Self::wrong(parameter, "a flag", other)),
        }
    }

    /// The text given for `parameter`, if any.
    ///
    /// # Errors
    ///
    /// When another kind of argument was given.
    pub fn string(&self, parameter: &'static str) -> Result<Option<String>, BuildError> {
        match self.get(parameter) {
            None => Ok(None),
            Some(Argument::String(text)) => Ok(Some(text.clone())),
            Some(other) => Err(Self::wrong(parameter, "text", other)),
        }
    }

    /// The duration given for `parameter`.
    ///
    /// # Errors
    ///
    /// When none was given, or another kind of argument was.
    pub fn duration(&self, parameter: &'static str) -> Result<String, BuildError> {
        match self.get(parameter) {
            None => Err(BuildError::Missing { parameter }),
            Some(Argument::Duration(text)) => Ok(text.clone()),
            Some(other) => Err(Self::wrong(parameter, "a duration", other)),
        }
    }

    /// The pattern given for the required `parameter`, as it was typed. Where it `means` plain text
    /// the text is no regular expression and is given as it is; otherwise Polars is asked whether
    /// it is one, which is the reason it is refused with where it is not.
    ///
    /// # Errors
    ///
    /// When none was given, another kind of argument was, or the text is a pattern that Polars
    /// cannot compile.
    pub fn pattern(
        &self,
        parameter: &'static str,
        means: PatternMeans,
    ) -> Result<String, BuildError> {
        match self.get(parameter) {
            None => Err(BuildError::Missing { parameter }),
            Some(Argument::Pattern(text)) => {
                if means == PatternMeans::RegularExpression {
                    check_pattern(text)
                        .map_err(|error| BuildError::NotAPattern { parameter, error })?;
                }
                Ok(text.clone())
            }
            Some(other) => Err(Self::wrong(parameter, "a pattern", other)),
        }
    }

    /// The time zone given for the required `parameter`, as Polars reads it: the zone, or none for a
    /// parameter that takes none and was given none.
    ///
    /// # Errors
    ///
    /// When none was given, another kind of argument was, the text is no time zone, or none was
    /// given to a parameter that needs a zone.
    pub fn time_zone(
        &self,
        parameter: &'static str,
        takes_none: bool,
    ) -> Result<Option<TimeZone>, BuildError> {
        match self.get(parameter) {
            None => Err(BuildError::Missing { parameter }),
            Some(Argument::TimeZone(Some(text))) => {
                check_time_zone(text).map_err(|error| BuildError::NotATimeZone { parameter, error })
            }
            Some(Argument::TimeZone(None)) if takes_none => Ok(None),
            Some(Argument::TimeZone(None)) => Err(BuildError::OutOfRange {
                parameter,
                expected: "a time zone, not none",
            }),
            Some(other) => Err(Self::wrong(parameter, "a time zone", other)),
        }
    }

    /// The word given for `parameter`, checked against `words`, or `default`.
    ///
    /// # Errors
    ///
    /// When another kind of argument, or a word not in `words`, was given.
    pub fn choice(
        &self,
        parameter: &'static str,
        words: &[&'static str],
        default: &'static str,
    ) -> Result<&'static str, BuildError> {
        match self.get(parameter) {
            None => Ok(default),
            Some(Argument::Choice(word)) => words
                .iter()
                .copied()
                .find(|known| known == word)
                .ok_or_else(|| BuildError::UnknownChoice {
                    parameter,
                    word: word.clone(),
                }),
            Some(other) => Err(Self::wrong(parameter, "a choice", other)),
        }
    }

    /// The table given for `parameter`.
    ///
    /// # Errors
    ///
    /// When none was given, or another kind of argument was.
    pub fn table(&self, parameter: &'static str) -> Result<LazyFrame, BuildError> {
        match self.get(parameter) {
            None => Err(BuildError::Missing { parameter }),
            Some(Argument::Table(frame)) => Ok(frame.clone()),
            Some(other) => Err(Self::wrong(parameter, "a table", other)),
        }
    }

    /// The pairs given for `parameter`.
    ///
    /// # Errors
    ///
    /// When another kind of argument was given.
    pub fn pairs(&self, parameter: &'static str) -> Result<Vec<(Argument, Argument)>, BuildError> {
        match self.get(parameter) {
            None => Ok(Vec::new()),
            Some(Argument::Pairs(pairs)) => Ok(pairs.clone()),
            Some(other) => Err(Self::wrong(parameter, "pairs", other)),
        }
    }
}

// ---------------------------------------------------------------------------
// Checking arguments against an entry

/// What an argument for a parameter of `kind` is, as the error names it.
const fn expects(kind: Kind) -> &'static str {
    match kind {
        Kind::Expression(_) => "an expression",
        Kind::Column => "column names",
        Kind::Value(ValueType::Integer) => "a whole number",
        Kind::Value(ValueType::Float) => "a number",
        Kind::Value(ValueType::String) => "text",
        // No argument holds a date yet, so no argument fits it.
        Kind::Value(ValueType::Date) => "a date",
        Kind::Value(ValueType::Any) => "a number, text or a flag",
        Kind::Flag => "a flag",
        Kind::Choice(_) => "a choice",
        Kind::Dtype => "a dtype",
        Kind::Duration => "a duration",
        Kind::Pattern => "a pattern",
        Kind::TimeZone => "a time zone",
        Kind::Table => "a table",
        Kind::Path => "a path",
        Kind::Pairs(..) => "pairs",
    }
}

/// Whether `argument` is one value of `kind`.
fn check_kind(parameter: &'static str, kind: Kind, argument: &Argument) -> Result<(), BuildError> {
    match (kind, argument) {
        (Kind::Expression(_), Argument::Expression(_))
        | (Kind::Column, Argument::Columns(_))
        | (Kind::Value(ValueType::Integer), Argument::Integer(_))
        | (Kind::Value(ValueType::Float), Argument::Float(_))
        | (Kind::Value(ValueType::String), Argument::String(_))
        | (
            Kind::Value(ValueType::Any),
            Argument::Integer(_) | Argument::Float(_) | Argument::String(_) | Argument::Flag(_),
        )
        | (Kind::Flag, Argument::Flag(_))
        | (Kind::Dtype, Argument::Dtype(_))
        | (Kind::Duration, Argument::Duration(_))
        | (Kind::Pattern, Argument::Pattern(_))
        | (Kind::TimeZone, Argument::TimeZone(_))
        | (Kind::Table, Argument::Table(_))
        | (Kind::Path, Argument::Path(_)) => Ok(()),
        (Kind::Choice(words), Argument::Choice(word)) => {
            if words.contains(&word.as_str()) {
                Ok(())
            } else {
                Err(BuildError::UnknownChoice {
                    parameter,
                    word: word.clone(),
                })
            }
        }
        (Kind::Pairs(key, value), Argument::Pairs(pairs)) => pairs.iter().try_for_each(|(k, v)| {
            check_kind(parameter, *key, k)?;
            check_kind(parameter, *value, v)
        }),
        (kind, other) => Err(Arguments::wrong(parameter, expects(kind), other)),
    }
}

/// Whether `argument` is what `parameter` holds: its kind, in its shape.
fn check(parameter: &Parameter, argument: &Argument) -> Result<(), BuildError> {
    let (name, kind) = (parameter.name(), parameter.kind());
    match (parameter.shape(), argument) {
        (Shape::Many | Shape::PerItem(_), Argument::Many(items)) => items
            .iter()
            .try_for_each(|item| check_kind(name, kind, item)),
        (Shape::Many, other) => Err(Arguments::wrong(name, "a list", other)),
        (Shape::One | Shape::PerItem(_), one) => check_kind(name, kind, one),
    }
}

impl Entry {
    /// The arguments for one use of the entry, each given by the name of the
    /// parameter it is for.
    ///
    /// Each argument must be of its parameter's kind, in its shape: a
    /// parameter of shape many takes a list, and one of shape per item takes
    /// one argument or a list. Where a kind holds words, the word must be one
    /// of them. What cannot be known without a table is not checked: the
    /// dtypes an expression may have, whether a column exists, and how many
    /// items a per-item list has. Building the entry reports those.
    ///
    /// # Errors
    ///
    /// When a name is not a parameter of the entry, a parameter is given
    /// twice, or an argument does not fit its parameter.
    pub fn arguments<'a>(
        &self,
        given: impl IntoIterator<Item = (&'a str, Argument)>,
    ) -> Result<Arguments, BuildError> {
        let mut arguments = Arguments::new();
        for (name, argument) in given {
            let parameter = self
                .parameters
                .iter()
                .find(|parameter| parameter.name() == name)
                .ok_or_else(|| BuildError::UnknownParameter {
                    parameter: name.to_owned(),
                })?;
            check(parameter, &argument)?;
            if arguments.given.insert(parameter.name(), argument).is_some() {
                return Err(BuildError::Repeated {
                    parameter: parameter.name(),
                });
            }
        }
        Ok(arguments)
    }
}
