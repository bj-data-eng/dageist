//! A working window of whole keys held in memory, served out in blocks.

use std::path::Path;

use polars::prelude::*;

use crate::block::{Block, ColumnInfo, check_columns, column_infos, slice_bounds, write_block};
use crate::error::EngineError;

/// Rows of a table held in memory so that blocks can be served instantly.
///
/// A table loaded with [`Table::load_keys`] holds every row of each key it
/// contains, so grouped and windowed expressions give the same answer on it
/// as on the full data for those keys.
#[derive(Debug, Clone)]
pub struct Table {
    frame: DataFrame,
    columns: Vec<ColumnInfo>,
}

impl Table {
    /// Loads every row of the first `keys` distinct values of `key_column`
    /// from the Parquet files under `directory`.
    ///
    /// # Errors
    ///
    /// Returns an error when the files cannot be scanned or read, or when
    /// `key_column` does not exist.
    pub fn load_keys(directory: &Path, key_column: &str, keys: u32) -> Result<Self, EngineError> {
        crate::startup::ensure();
        let pattern = directory.join("*.parquet");
        let scan = || {
            LazyFrame::scan_parquet(
                PlRefPath::try_from_path(&pattern)?,
                ScanArgsParquet::default(),
            )
        };
        let wanted = scan()
            .map_err(|source| EngineError::polars("scanning the table", source))?
            .select([col(key_column)])
            .unique_stable(None, UniqueKeepStrategy::First)
            .limit(keys);
        let frame = scan()
            .map_err(|source| EngineError::polars("scanning the table", source))?
            .join(
                wanted,
                [col(key_column)],
                [col(key_column)],
                JoinArgs::new(JoinType::Semi),
            )
            .map_err(|source| EngineError::polars("joining the working window's keys", source))?
            .collect()
            .map_err(|source| EngineError::polars("loading the working window", source))?;
        let columns = column_infos(frame.schema());
        Ok(Self { frame, columns })
    }

    /// Returns the number of rows held.
    #[must_use]
    pub fn height(&self) -> usize {
        self.frame.height()
    }

    /// Returns every column's name and type, in table order.
    #[must_use]
    pub fn columns(&self) -> &[ColumnInfo] {
        &self.columns
    }

    /// Returns `length` rows starting at `offset`, limited to `columns`, as a
    /// [`Block`] of text: every cell is written as Polars writes the value, as in
    /// a preview, and a null is a missing cell.
    ///
    /// Rows past the end are left out, so the result can hold fewer rows than
    /// asked for. An empty `columns` selects every column.
    ///
    /// # Errors
    ///
    /// Returns an error when a named column does not exist or is named twice,
    /// or when the block cannot be written.
    pub fn block(
        &self,
        offset: usize,
        length: usize,
        columns: &[String],
    ) -> Result<Block, EngineError> {
        check_columns(columns, &self.columns)?;
        let selected = if columns.is_empty() {
            self.frame.clone()
        } else {
            self.frame
                .select(columns.iter().map(String::as_str))
                .map_err(|source| EngineError::polars("selecting block columns", source))?
        };
        let (offset, _) = slice_bounds(offset, length)?;
        write_block(&selected.slice(offset, length))
    }
}
