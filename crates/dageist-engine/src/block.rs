//! What every kind of table shares when it serves a block: column
//! descriptions, row ranges and the Arrow IPC encoding.

use polars::prelude::*;

use crate::error::EngineError;

/// The name and type of one column, for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnInfo {
    /// The column's name.
    pub name: String,
    /// The column's Polars type, as Polars prints it.
    pub data_type: String,
}

pub(crate) fn column_infos(schema: &Schema) -> Vec<ColumnInfo> {
    schema
        .iter()
        .map(|(name, data_type)| ColumnInfo {
            name: name.to_string(),
            data_type: data_type.to_string(),
        })
        .collect()
}

/// Converts a requested row range into the offset and length Polars slices
/// by. A length too large for Polars means "to the end".
pub(crate) fn row_range(offset: usize, length: usize) -> Result<(i64, IdxSize), EngineError> {
    let offset = i64::try_from(offset)
        .map_err(|_| EngineError::request("slicing the block", "the offset is too large"))?;
    Ok((offset, IdxSize::try_from(length).unwrap_or(IdxSize::MAX)))
}

/// Encodes `frame` as an Arrow IPC stream.
pub(crate) fn write_ipc(mut frame: DataFrame) -> Result<Vec<u8>, EngineError> {
    let mut bytes = Vec::new();
    IpcStreamWriter::new(&mut bytes)
        .finish(&mut frame)
        .map_err(|source| EngineError::polars("writing the block", source))?;
    Ok(bytes)
}
