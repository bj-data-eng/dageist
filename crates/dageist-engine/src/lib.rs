//! Dataframe engine for dageist.
//!
//! This crate owns everything that touches Polars: generating sample data,
//! loading a working window of whole keys into memory, reading a stored table
//! straight from its Parquet files, and handing blocks of either to a host as
//! Arrow IPC bytes. It has no dependency on any host.

#![forbid(unsafe_code)]

mod block;
mod error;
mod sample;
mod stored;
mod table;

pub use block::ColumnInfo;
pub use error::EngineError;
pub use sample::{SampleSpec, claims_generated, generate_claims};
pub use stored::ParquetTable;
pub use table::Table;
