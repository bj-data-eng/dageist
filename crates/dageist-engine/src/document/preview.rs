//! The rows after a step, written out as text: what a host sends to the view
//! so that it formats no value of its own.

use polars::prelude::{DataFrame, PolarsError};

use super::columns::ColumnDescription;
use super::inputs::Inputs;
use super::model::{Document, StepId};
use super::run::RowsError;

/// The rows after a step: its columns, and every cell as the text Polars
/// writes it.
///
/// A cell is `None` where the value is null, which no text stands for, and
/// otherwise the text Polars gives the value: a string as it is, a date as
/// `2026-03-02`, a duration as `1d 7h 25m`, a list as `[1, 2]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    columns: Vec<ColumnDescription>,
    rows: Vec<Vec<Option<String>>>,
}

impl Preview {
    /// The columns, in order, each with its dtype.
    #[must_use]
    pub fn columns(&self) -> &[ColumnDescription] {
        &self.columns
    }

    /// The rows, each with one cell for every column.
    #[must_use]
    pub fn rows(&self) -> &[Vec<Option<String>>] {
        &self.rows
    }

    fn of(frame: &DataFrame) -> Result<Self, PolarsError> {
        let mut rows = Vec::with_capacity(frame.height());
        for index in 0..frame.height() {
            let mut cells = Vec::with_capacity(frame.width());
            for column in frame.columns() {
                let value = column.get(index)?;
                cells.push((!value.is_null()).then(|| value.str_value().into_owned()));
            }
            rows.push(cells);
        }
        Ok(Self {
            columns: ColumnDescription::of_schema(frame.schema()),
            rows,
        })
    }
}

impl Document {
    /// The rows of the table the step `through` gives, at most `limit` of
    /// them, written as text, with the steps up to it run on `working`: a
    /// table held in memory. It is [`Document::rows`] with every cell
    /// formatted.
    ///
    /// # Errors
    ///
    /// When the document has no such step, a step up to it is refused, or
    /// Polars cannot collect or read the rows.
    pub fn preview(
        &self,
        inputs: &Inputs,
        working: &DataFrame,
        through: &StepId,
        limit: usize,
    ) -> Result<Preview, RowsError> {
        let rows = self.rows(inputs, working, through, limit)?;
        Preview::of(&rows).map_err(RowsError::Polars)
    }
}
