//! What every kind of table shares when it serves a block: column
//! descriptions, row ranges and the Arrow IPC encoding.
//!
//! A block carries every cell as text, written as the step editor's previews
//! write it, so that a host passes the block on and the view draws it without
//! formatting a value.

use polars::prelude::*;

use crate::error::EngineError;
use crate::text::text_of;

/// The name and type of one column, for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnInfo {
    /// The column's name.
    pub name: String,
    /// The column's Polars type, as Polars prints it.
    pub data_type: String,
    /// Whether the column holds numbers, which a table draws against the
    /// right of the cell.
    pub numeric: bool,
}

pub(crate) fn column_infos(schema: &Schema) -> Vec<ColumnInfo> {
    schema
        .iter()
        .map(|(name, data_type)| ColumnInfo {
            name: name.to_string(),
            data_type: data_type.to_string(),
            numeric: data_type.is_numeric(),
        })
        .collect()
}

/// Checks that every requested column exists and is asked for once.
pub(crate) fn check_columns(
    requested: &[String],
    available: &[ColumnInfo],
) -> Result<(), EngineError> {
    for (position, name) in requested.iter().enumerate() {
        if !available.iter().any(|column| &column.name == name) {
            return Err(EngineError::request(
                "selecting block columns",
                format!("there is no column named {name}"),
            ));
        }
        if requested[..position].contains(name) {
            return Err(EngineError::request(
                "selecting block columns",
                format!("the column {name} is asked for more than once"),
            ));
        }
    }
    Ok(())
}

/// Converts a requested row range into the offset and length Polars slices
/// by. A length too large for Polars means "to the end".
pub(crate) fn slice_bounds(offset: usize, length: usize) -> Result<(i64, IdxSize), EngineError> {
    let offset = i64::try_from(offset)
        .map_err(|_| EngineError::request("slicing the block", "the offset is too large"))?;
    Ok((offset, IdxSize::try_from(length).unwrap_or(IdxSize::MAX)))
}

/// A column of `frame` written as text: a string column as it is, any other
/// cell by Polars' own text for the value, and a null left null.
fn text_column(column: &Column) -> PolarsResult<Column> {
    if column.dtype() == &DataType::String {
        return Ok(column.clone());
    }
    let cells = (0..column.len())
        .map(|row| column.get(row).map(|value| text_of(&value)))
        .collect::<PolarsResult<Vec<Option<String>>>>()?;
    Ok(Column::new(column.name().clone(), cells))
}

/// Encodes `frame` as an Arrow IPC stream of text columns.
pub(crate) fn write_ipc(frame: &DataFrame) -> Result<Vec<u8>, EngineError> {
    let writing = |source| EngineError::polars("writing the block", source);
    let mut text = DataFrame::new(
        frame.height(),
        frame
            .columns()
            .iter()
            .map(text_column)
            .collect::<PolarsResult<_>>()
            .map_err(writing)?,
    )
    .map_err(writing)?;
    let mut bytes = Vec::new();
    IpcStreamWriter::new(&mut bytes)
        .finish(&mut text)
        .map_err(writing)?;
    Ok(bytes)
}
