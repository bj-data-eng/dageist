//! Whether a text is a pattern, which Polars says.
//!
//! A pattern is a regular expression in the syntax of Rust's `regex` crate, and Polars compiles it
//! with its own settings. The catalogue does not parse one itself: it asks Polars, through the one
//! method that compiles a pattern when it is built and not when rows run, `str.extract_groups`,
//! which needs the groups of the pattern to name the fields of the struct it gives. What that
//! refuses is, nearly always, what a method of text that takes a pattern would refuse of it when it
//! ran: `contains` takes a fast path for a pattern that is a chain of literals, which it does not
//! compile.

use std::fmt;

use polars::prelude::{
    DataFrame, DataType, Field, IntoLazy, PlSmallStr, PolarsError, Schema, col, lit,
};

/// Why a text is no pattern, in the words Polars gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError {
    reason: String,
}

impl PatternError {
    /// What Polars says of the text: where the regular expression goes wrong and how.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for PatternError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for PatternError {}

impl PatternError {
    fn of(error: &PolarsError) -> Self {
        Self {
            reason: error.to_string(),
        }
    }
}

/// Whether `text` is a regular expression Polars can compile.
///
/// # Errors
///
/// Polars' own reason, where the text is none.
pub fn check_pattern(text: &str) -> Result<(), PatternError> {
    lit("")
        .str()
        .extract_groups(text)
        .map(drop)
        .map_err(|error| PatternError::of(&error))
}

/// The dtype `str.extract_groups` gives of the pattern `text`, as Polars works it out from the
/// schema of the expression: a struct with a field of text for each group of the pattern, named
/// as the group is, or by its number where it has no name.
///
/// # Errors
///
/// Polars' own reason, where the text is no pattern, or where it cannot work out the schema.
pub fn groups_dtype(text: &str) -> Result<DataType, PatternError> {
    crate::startup::ensure();
    let name = PlSmallStr::from_static("t");
    let schema = Schema::from_iter([Field::new(name.clone(), DataType::String)]);
    let mut frame = DataFrame::empty_with_schema(&schema)
        .lazy()
        .select([col(name)
            .str()
            .extract_groups(text)
            .map_err(|error| PatternError::of(&error))?]);
    let schema = frame
        .collect_schema()
        .map_err(|error| PatternError::of(&error))?;
    schema
        .iter_values()
        .next()
        .cloned()
        .ok_or_else(|| PatternError {
            reason: "Polars gave no column for the pattern".to_owned(),
        })
}
