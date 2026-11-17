//! Dataframe engine for dageist.
//!
//! This crate owns everything that touches Polars: generating sample data,
//! loading a working window of whole keys into memory, reading a stored table
//! straight from its Parquet files, and handing blocks of either to a host as
//! Arrow IPC bytes. It also owns the catalogue of Polars operations the step
//! editor offers, and the step document built from it: its reader, its writer
//! and running it, and the tables of which combinations of its entries Polars
//! allows. It serves the claim-timing sample node the step editor opens with.
//! It has no dependency on any host.
//!
//! Polars is set up as the Python Polars release sets it up, for the parts of
//! that set-up that are not about Python: see [`PolarsSession`], and
//! `src/startup.rs` for what is done and what is left out. Every entry here that
//! runs Polars sets it up first, once for the process, and a host holds a
//! [`PolarsSession`] from its start to its end.

#![forbid(unsafe_code)]

pub mod allowed;
mod block;
pub mod catalog;
pub mod claim_timing;
pub mod document;
mod error;
mod sample;
mod startup;
mod stored;
mod table;

pub use block::ColumnInfo;
pub use error::EngineError;
pub use sample::{SampleSpec, claims_generated, generate_claims};
pub use startup::PolarsSession;
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
