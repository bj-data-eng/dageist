//! What holds the engine to Polars: the one way Polars is asked of a step, and the campaign that
//! samples the compositions of the catalogue's calls.
//!
//! The framework mechanics are [`dageist_fuzz`]'s. This package owns what they are about:
//!
//! - [`ask`] asks Polars of a step directly, built through the catalogue's own build functions
//!   and without the engine's refusals, so a refusal the engine makes can be held to Polars.
//!
//! The campaign target is in `tests/allowed_fuzz/`. No production crate depends on this package.

#![forbid(unsafe_code)]

pub mod ask;
