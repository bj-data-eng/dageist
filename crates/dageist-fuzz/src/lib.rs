//! Shared fuzzing contracts, bounded execution and reproducible failure handling.
//!
//! Domain cases and semantic expectations belong to each consuming crate.
#![forbid(unsafe_code)]

pub mod artifact;
mod contract;
mod error;
pub mod generation;
mod identity;
pub mod registry;
mod source;
pub use artifact::*;
#[cfg(feature = "execution")]
pub mod campaign;
pub mod engine;
#[cfg(feature = "execution")]
mod engine_identity;
#[cfg(feature = "execution")]
pub mod engine_minimization;
#[cfg(feature = "execution")]
pub mod execution;
pub mod reduction;
#[cfg(all(feature = "execution", unix))]
mod retention;
pub mod worker;

pub use contract::*;
pub use error::*;
pub use identity::*;
pub use source::{RelativePath, Sha256Digest, SourceRevision};
