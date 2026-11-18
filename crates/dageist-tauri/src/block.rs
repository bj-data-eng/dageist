//! The frame a block of a table travels in.
//!
//! The engine writes a block as the bytes of an Arrow IPC stream whose cells
//! are text. The view also shows the number of each row and a report of the
//! block, and writes neither: both are text from here. A frame holds them in
//! front of the Arrow bytes:
//!
//! 1. four bytes, the length of the header, as a big-endian number;
//! 2. the header, a JSON object with the fields of [`Header`];
//! 3. the Arrow IPC stream, to the end.

use std::time::Duration;

use serde::Serialize;

use crate::figures::{count, elapsed, quantity, size};
use crate::sample_table::ErrorView;

/// The content type of a frame, which the bridge answers a block with.
pub(crate) const CONTENT_TYPE: &str = "application/vnd.dageist.block";

/// What a frame says of its block.
#[derive(Debug, Serialize)]
struct Header {
    /// The number of each row of the block, counting from 1, as text.
    numbers: Vec<String>,
    /// The block in a line: its rows and columns, the size of its Arrow bytes
    /// and the time the engine took to build them.
    report: String,
}

/// The frame of the block `arrow`, which holds `rows` rows from the row at
/// `offset` and `columns` columns, and which the engine took `built` to write.
pub(crate) fn frame(
    offset: usize,
    rows: usize,
    columns: usize,
    arrow: &[u8],
    built: Duration,
) -> Result<Vec<u8>, ErrorView> {
    let header = Header {
        numbers: (0..rows).map(|at| count(offset + at + 1)).collect(),
        report: format!(
            "{} × {} · {} · built in {}",
            quantity(rows, "row", "rows"),
            quantity(columns, "column", "columns"),
            size(arrow.len()),
            elapsed(built),
        ),
    };
    let header = serde_json::to_vec(&header)
        .map_err(|error| ErrorView::new(format!("writing the block's header: {error}")))?;
    let length = u32::try_from(header.len())
        .map_err(|_too_long| ErrorView::new("the block's header is too long"))?;
    let mut frame = Vec::with_capacity(4 + header.len() + arrow.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(arrow);
    Ok(frame)
}
