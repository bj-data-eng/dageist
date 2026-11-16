//! Dataframe engine for dageist.
//!
//! This crate owns everything that touches Polars: generating sample data,
//! loading a working window of whole keys into memory, reading a stored table
//! straight from its Parquet files, and handing blocks of either to a host as
//! Arrow IPC bytes. It also owns the catalogue of Polars operations the step
//! editor offers, and the step document built from it: its reader, its writer
//! and running it. It has no dependency on any host.

#![forbid(unsafe_code)]

mod block;
pub mod catalog;
pub mod document;
mod error;
mod sample;
mod stored;
mod table;

pub use block::ColumnInfo;
pub use error::EngineError;
pub use sample::{SampleSpec, claims_generated, generate_claims};
pub use stored::ParquetTable;
pub use table::Table;

/// Polars' own allocator is the global allocator of every program that links
/// the engine, as it is in the Python Polars release the engine is built
/// like. It keeps count of the memory Polars uses, which Polars' streaming
/// engine reads to decide when to spill to disk, and allocates through
/// jemalloc on macOS and Linux and mimalloc elsewhere. Because the engine
/// declares it, a program that links the engine cannot choose another one.
#[global_allocator]
static ALLOCATOR: polars_ooc::Allocator = polars_ooc::Allocator;
