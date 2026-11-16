//! What a document's steps can use besides the table they start from: the
//! values the graph connects to the node.

use std::collections::BTreeMap;
use std::fmt;

use polars::prelude::{Expr, LazyFrame, Scalar, Series, lit};

/// An input name given twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateInput(pub String);

impl fmt::Display for DuplicateInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "there is already an input named {:?}", self.0)
    }
}

impl std::error::Error for DuplicateInput {}

#[derive(Clone)]
enum Input {
    Table(Box<LazyFrame>),
    Value(Scalar),
    List(Series),
}

/// The inputs of a node by name: tables, single values and lists. A name
/// belongs to one input.
///
/// A table is what a `{ "table": name }` argument names, as the other table
/// of a join does. A single value or a list is what an expression that starts
/// from `{ "input": name }` starts with, as a literal of its dtype.
#[derive(Clone, Default)]
pub struct Inputs {
    inputs: BTreeMap<String, Input>,
}

impl fmt::Debug for Inputs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut shown = formatter.debug_map();
        for (name, input) in &self.inputs {
            shown.entry(
                name,
                &match input {
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

    fn add(&mut self, name: String, input: Input) -> Result<(), DuplicateInput> {
        if self.inputs.contains_key(&name) {
            return Err(DuplicateInput(name));
        }
        self.inputs.insert(name, input);
        Ok(())
    }

    /// Connects a table.
    ///
    /// # Errors
    ///
    /// When `name` is already an input.
    pub fn add_table(
        &mut self,
        name: impl Into<String>,
        table: LazyFrame,
    ) -> Result<(), DuplicateInput> {
        self.add(name.into(), Input::Table(Box::new(table)))
    }

    /// Connects a single value, which has its own dtype.
    ///
    /// # Errors
    ///
    /// When `name` is already an input.
    pub fn add_value(
        &mut self,
        name: impl Into<String>,
        value: Scalar,
    ) -> Result<(), DuplicateInput> {
        self.add(name.into(), Input::Value(value))
    }

    /// Connects a list of values of one dtype.
    ///
    /// # Errors
    ///
    /// When `name` is already an input.
    pub fn add_list(
        &mut self,
        name: impl Into<String>,
        values: Series,
    ) -> Result<(), DuplicateInput> {
        self.add(name.into(), Input::List(values))
    }

    /// The table called `name`, if there is one.
    pub(super) fn table(&self, name: &str) -> Option<LazyFrame> {
        match self.inputs.get(name) {
            Some(Input::Table(table)) => Some(LazyFrame::clone(table)),
            _ => None,
        }
    }

    /// The single value or list called `name` as an expression: a literal of
    /// its dtype.
    pub(super) fn literal(&self, name: &str) -> Option<Expr> {
        match self.inputs.get(name) {
            Some(Input::Value(value)) => Some(lit(value.clone())),
            Some(Input::List(values)) => Some(lit(values.clone())),
            _ => None,
        }
    }
}
