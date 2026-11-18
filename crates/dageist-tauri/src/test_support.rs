//! What the host's tests share: reading the frame a block of a table travels in.
//!
//! A block is a frame: four bytes that say how long its header is (big-endian),
//! the header as JSON, and then the block's Arrow IPC stream to the end. The
//! reader here is written from that description, not from the code that writes
//! a frame.

use serde_json::Value;

/// Splits the frame `bytes` into its header and its Arrow bytes.
///
/// # Panics
///
/// When `bytes` is not a frame.
pub(crate) fn split_block(bytes: &[u8]) -> (Value, &[u8]) {
    let length = bytes
        .first_chunk::<4>()
        .map(|length| u32::from_be_bytes(*length))
        .expect("a frame starts with the length of its header");
    let length = usize::try_from(length).expect("a length fits in memory");
    let header = bytes.get(4..4 + length).unwrap_or_else(|| {
        panic!(
            "not a frame: its header is {length} bytes of {}",
            bytes.len()
        )
    });
    let header = serde_json::from_slice(header).expect("a frame's header is JSON");
    (header, &bytes[4 + length..])
}
