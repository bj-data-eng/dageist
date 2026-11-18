//! What a document's steps can use besides the table they start from: the
//! values the graph connects to the node.

use std::collections::BTreeMap;
use std::fmt;

use polars::prelude::{Expr, LazyFrame, PolarsError, Scalar, Series, lit};

use crate::allowed::{Representative, Shape, dtype_name};

use super::columns::ColumnDescription;
use super::values::{FirstValues, text_of};

/// An input name given twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateInput(pub String);

impl fmt::Display for DuplicateInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "there is already an input named {:?}", self.0)
    }
}

impl std::error::Error for DuplicateInput {}

/// Where an input comes from in the graph: what supplies it to the node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// The table a node gives, by the node's name.
    Node(String),
    /// One column of the table a node gives, by the node's name and the
    /// column's.
    Column {
        /// The node that gives the table.
        node: String,
        /// The column of its table.
        column: String,
    },
    /// A value set on the graph itself, which no node gives.
    Parameter,
}

/// What supplies a list to the node: a column of the table a node gives, or
/// the graph itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListOrigin {
    /// One column of the table a node gives, by the node's name and the
    /// column's.
    Column {
        /// The node that gives the table.
        node: String,
        /// The column of its table.
        column: String,
    },
    /// A list set on the graph itself, which no node gives.
    Parameter,
}

impl From<ListOrigin> for Origin {
    fn from(origin: ListOrigin) -> Self {
        match origin {
            ListOrigin::Column { node, column } => Self::Column { node, column },
            ListOrigin::Parameter => Self::Parameter,
        }
    }
}

/// What an input holds, as the view is told of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    /// A table, with the columns it has.
    Table(Vec<ColumnDescription>),
    /// A single value.
    Value {
        /// The short name of its dtype.
        dtype: String,
        /// The value as Polars writes it, or `None` where it is null.
        value: Option<String>,
    },
    /// A list of values.
    List {
        /// The short name of their dtype.
        dtype: String,
        /// The first of the values as Polars writes them.
        values: FirstValues,
    },
}

/// An input of a node: its name, where it comes from and what it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDescription {
    name: String,
    origin: Origin,
    kind: InputKind,
}

impl InputDescription {
    /// The name a document refers to the input by.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What supplies the input to the node.
    #[must_use]
    pub const fn origin(&self) -> &Origin {
        &self.origin
    }

    /// What the input holds.
    #[must_use]
    pub const fn kind(&self) -> &InputKind {
        &self.kind
    }
}

#[derive(Clone)]
enum Input {
    Table(Box<LazyFrame>),
    Value(Scalar),
    List(Series),
}

/// An input and what supplies it.
#[derive(Clone)]
struct Connected {
    origin: Origin,
    input: Input,
}

/// What a connected value or list is as the start of an expression: the class
/// of its dtype, when the tables cover it, and whether it holds many values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StartLiteral {
    pub(super) class: Option<Representative>,
    pub(super) shape: Shape,
}

/// The inputs of a node by name: tables, single values and lists. A name
/// belongs to one input.
///
/// A table is what a `{ "table": name }` argument names, as the other table
/// of a join does. A single value or a list is what an expression that starts
/// from `{ "input": name }` starts with, as a literal of its dtype.
#[derive(Clone, Default)]
pub struct Inputs {
    inputs: BTreeMap<String, Connected>,
}

impl fmt::Debug for Inputs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut shown = formatter.debug_map();
        for (name, connected) in &self.inputs {
            shown.entry(
                name,
                &match connected.input {
                    Input::Table(_) => "table",
                    Input::Value(_) => "value",
                    Input::List(_) => "list",
                },
            );
        }
        shown.finish()
    }
}

impl Inputs {
    /// No inputs.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn add(&mut self, name: String, origin: Origin, input: Input) -> Result<(), DuplicateInput> {
        if self.inputs.contains_key(&name) {
            return Err(DuplicateInput(name));
        }
        self.inputs.insert(name, Connected { origin, input });
        Ok(())
    }

    /// Connects the table that the node called `node` gives.
    ///
    /// # Errors
    ///
    /// When `name` is already an input.
    pub fn add_table(
        &mut self,
        name: impl Into<String>,
        node: impl Into<String>,
        table: LazyFrame,
    ) -> Result<(), DuplicateInput> {
        self.add(
            name.into(),
            Origin::Node(node.into()),
            Input::Table(Box::new(table)),
        )
    }

    /// Connects a single value, which has its own dtype. A value is set on
    /// the graph itself, so its origin is [`Origin::Parameter`].
    ///
    /// # Errors
    ///
    /// When `name` is already an input.
    pub fn add_value(
        &mut self,
        name: impl Into<String>,
        value: Scalar,
    ) -> Result<(), DuplicateInput> {
        self.add(name.into(), Origin::Parameter, Input::Value(value))
    }

    /// Connects a list of values of one dtype that `origin` supplies.
    ///
    /// # Errors
    ///
    /// When `name` is already an input.
    pub fn add_list(
        &mut self,
        name: impl Into<String>,
        origin: ListOrigin,
        values: Series,
    ) -> Result<(), DuplicateInput> {
        self.add(name.into(), origin.into(), Input::List(values))
    }

    /// What each input holds and what supplies it, in the order of their
    /// names: a table's columns with their dtypes, a value or the first values
    /// of a list with their dtype. Dtypes are given by their
    /// [short names](crate::allowed::dtype_name), such as `Datetime(us)`, and
    /// values as Polars writes them. No table is collected.
    ///
    /// # Errors
    ///
    /// When Polars cannot work out the schema of a connected table, or cannot
    /// read a value.
    pub fn describe(&self) -> Result<Vec<InputDescription>, PolarsError> {
        crate::startup::ensure();
        self.inputs
            .iter()
            .map(|(name, connected)| {
                let kind = match &connected.input {
                    Input::Table(table) => {
                        let schema = LazyFrame::clone(table).collect_schema()?;
                        InputKind::Table(ColumnDescription::of_schema(&schema))
                    }
                    Input::Value(value) => InputKind::Value {
                        dtype: dtype_name(value.dtype()),
                        value: text_of(value.value()),
                    },
                    Input::List(values) => InputKind::List {
                        dtype: dtype_name(values.dtype()),
                        values: FirstValues::of_series(values)?,
                    },
                };
                Ok(InputDescription {
                    name: name.clone(),
                    origin: connected.origin.clone(),
                    kind,
                })
            })
            .collect()
    }

    /// The table called `name`, if there is one.
    pub(super) fn table(&self, name: &str) -> Option<LazyFrame> {
        match self.inputs.get(name).map(|connected| &connected.input) {
            Some(Input::Table(table)) => Some(LazyFrame::clone(table)),
            _ => None,
        }
    }

    /// What the single value or list called `name` is as the start of an
    /// expression, or `None` when no value or list has the name.
    pub(super) fn start_literal(&self, name: &str) -> Option<StartLiteral> {
        let (dtype, shape) = match &self.inputs.get(name)?.input {
            Input::Value(value) => (value.dtype(), Shape::One),
            Input::List(values) => (values.dtype(), Shape::Many),
            Input::Table(_) => return None,
        };
        Some(StartLiteral {
            class: Representative::of(dtype),
            shape,
        })
    }

    /// The single value or list called `name` as an expression: a literal of
    /// its dtype.
    pub(super) fn literal(&self, name: &str) -> Option<Expr> {
        match self.inputs.get(name).map(|connected| &connected.input) {
            Some(Input::Value(value)) => Some(lit(value.clone())),
            Some(Input::List(values)) => Some(lit(values.clone())),
            _ => None,
        }
    }
}
