//! The filled-in arguments an entry is built from, and why building fails.

use std::collections::BTreeMap;

use polars::prelude::{DataType, Expr, LazyFrame, PolarsError};

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
            Self::Polars(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
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

    /// These arguments with `parameter` set to `argument`.
    #[must_use]
    pub fn with(mut self, parameter: &'static str, argument: Argument) -> Self {
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
