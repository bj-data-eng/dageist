//! How many rows each step kept of those it received, and the first values of
//! the columns it added or replaced, where the rows were run.

use polars::prelude::{DataFrame, PolarsError};

use super::changes::ColumnChange;
use super::model::StepId;
use super::values::FirstValues;

/// The first values of one column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnValues {
    name: String,
    values: FirstValues,
}

impl ColumnValues {
    /// The column's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its first values, as text.
    #[must_use]
    pub const fn values(&self) -> &FirstValues {
        &self.values
    }
}

/// The rows a step received and kept when it ran on the working window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counted {
    received: usize,
    kept: usize,
    first: Vec<ColumnValues>,
}

impl Counted {
    /// The count of a step that was given `received` rows and gave `given`,
    /// with the first values of each column it changed.
    pub(super) fn of(
        received: usize,
        given: &DataFrame,
        changes: &[ColumnChange],
    ) -> Result<Self, PolarsError> {
        let mut first = Vec::with_capacity(changes.len());
        for change in changes {
            first.push(ColumnValues {
                name: change.name().to_owned(),
                values: FirstValues::of_column(given.column(change.name())?)?,
            });
        }
        Ok(Self {
            received,
            kept: given.height(),
            first,
        })
    }

    /// How many rows the step was given.
    #[must_use]
    pub const fn received(&self) -> usize {
        self.received
    }

    /// How many rows the step gave.
    #[must_use]
    pub const fn kept(&self) -> usize {
        self.kept
    }

    /// The first values of each column the step added or replaced, in the order
    /// of the table it gives.
    #[must_use]
    pub fn first(&self) -> &[ColumnValues] {
        &self.first
    }
}

/// What running a step on the rows of the working window came to.
#[derive(Debug, Clone)]
pub enum StepRows {
    /// The step was not run on rows: the run read no data, or a step before it
    /// was refused or incomplete, or the step was itself refused or incomplete.
    NotCounted,
    /// Polars accepts the step, and it was not run on the rows because the step `failed` before
    /// it, which Polars accepted and could not collect on them, as [`StepRows::Failed`] says.
    NotReached {
        /// The first step that failed on the rows.
        failed: StepId,
    },
    /// The step ran on the rows.
    Counted(Counted),
    /// Polars accepted the step, and could not collect its rows.
    Failed(PolarsError),
}
