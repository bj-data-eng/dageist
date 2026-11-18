//! A number or text typed in, and the dtype Polars gives it.

use polars::prelude::{DataFrame, DataType, IntoLazy};

use crate::catalog::{self, Argument, Build};

use super::dtype::Representative;

/// A number or text typed in, as a document writes it.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// A whole number.
    Integer(i64),
    /// A number with a fraction, or too large for a whole number.
    Float(f64),
    /// Text.
    String(String),
}

impl Literal {
    /// The class of the dtype Polars gives the literal, worked out as the
    /// schema of the literal on a table with no rows: no data is read. `None`
    /// where Polars refuses the literal or gives it a dtype that has no class.
    #[must_use]
    pub fn class(&self) -> Option<Representative> {
        crate::startup::ensure();
        let literal = catalog::entry("polars.lit")?;
        let Build::Function(build) = literal.build() else {
            return None;
        };
        let value = match self {
            Self::Integer(value) => Argument::Integer(*value),
            Self::Float(value) => Argument::Float(*value),
            Self::String(value) => Argument::String(value.clone()),
        };
        let expression = build(&literal.arguments(vec![("value", value)]).ok()?).ok()?;
        let schema = DataFrame::empty()
            .lazy()
            .select([expression])
            .collect_schema()
            .ok()?;
        let (_, dtype): (_, &DataType) = schema.iter().next()?;
        Representative::of(dtype)
    }
}
