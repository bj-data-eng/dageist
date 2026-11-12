//! A table that stays in its Parquet files and is read one block at a time.

use std::path::{Path, PathBuf};

use polars::prelude::*;

use crate::block::{ColumnInfo, check_columns, column_infos, slice_bounds, write_ipc};
use crate::error::EngineError;

/// A table kept in Parquet files, however large.
///
/// Opening it reads only the files' metadata. Each block is read from disk
/// when it is asked for, so memory use depends on the block, not the table.
#[derive(Debug, Clone)]
pub struct ParquetTable {
    pattern: PathBuf,
    height: usize,
    columns: Vec<ColumnInfo>,
}

impl ParquetTable {
    /// Opens the Parquet files under `directory` as one table, in file name
    /// order.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory holds no readable Parquet files or
    /// the files do not share one schema.
    pub fn open(directory: &Path) -> Result<Self, EngineError> {
        let pattern = directory.join("*.parquet");
        let opening = |source| EngineError::polars("opening the stored table", source);
        let schema = scan(&pattern)
            .and_then(|mut frame| frame.collect_schema())
            .map_err(opening)?;
        let counted = scan(&pattern)
            .and_then(|frame| frame.select([len().alias("rows")]).collect())
            .map_err(opening)?;
        let height = counted
            .column("rows")
            .and_then(|rows| rows.get(0))
            .and_then(|rows| rows.try_extract::<usize>())
            .map_err(opening)?;
        Ok(Self {
            pattern,
            height,
            columns: column_infos(&schema),
        })
    }

    /// Returns the number of rows in the files.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns every column's name and type, in table order.
    #[must_use]
    pub fn columns(&self) -> &[ColumnInfo] {
        &self.columns
    }

    /// Reads `length` rows starting at `offset`, limited to `columns`, and
    /// returns them as an Arrow IPC stream.
    ///
    /// Rows past the end are left out, so the result can hold fewer rows than
    /// asked for. An empty `columns` selects every column.
    ///
    /// # Errors
    ///
    /// Returns an error when a named column does not exist or is named twice,
    /// or when the files cannot be read.
    pub fn block_ipc(
        &self,
        offset: usize,
        length: usize,
        columns: &[String],
    ) -> Result<Vec<u8>, EngineError> {
        check_columns(columns, &self.columns)?;
        let (offset, length) = slice_bounds(offset, length)?;
        let reading = |source| EngineError::polars("reading the block", source);
        let mut frame = scan(&self.pattern).map_err(reading)?;
        if !columns.is_empty() {
            frame = frame.select(
                columns
                    .iter()
                    .map(|name| col(name.as_str()))
                    .collect::<Vec<_>>(),
            );
        }
        write_ipc(frame.slice(offset, length).collect().map_err(reading)?)
    }
}

fn scan(pattern: &Path) -> PolarsResult<LazyFrame> {
    LazyFrame::scan_parquet(
        PlRefPath::try_from_path(pattern)?,
        ScanArgsParquet::default(),
    )
}
