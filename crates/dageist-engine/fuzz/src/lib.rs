//! What holds the engine to Polars: the one way Polars is asked of a step, the grids that judge
//! every combination of the catalogue's axes, and the campaign that samples their compositions.
//!
//! The framework mechanics are [`dageist_fuzz`]'s: its [`grid`](dageist_fuzz::grid) for grids
//! and its property runner for campaigns. This package owns what they are about:
//!
//! - [`ask`] asks Polars of a step directly, built through the catalogue's own build functions
//!   and without the engine's refusals, so a refusal the engine makes can be held to Polars.
//! - [`judge`] judges a grid's cell: the [`Case`](judge::Case) of one step, asked of the walk and
//!   of Polars, and sorted into one [`Outcome`](judge::Outcome).
//! - [`world`] is the one table every grid runs on, a column of every class and dtype.
//! - [`literals`] are the literals a document writes, at the edges of every class.
//! - [`doc`] writes the JSON of a step document.
//! - [`places`] are every place an expression stands, and the case of a step that holds one there.
//! - [`axes`] are what the catalogue and the tables give a grid to cross: methods, parameters by
//!   their role, calls by what they do, the orders of two operands.
//!
//! The grids are in `tests/grids/`, and the campaign target in `tests/allowed_fuzz/`. No
//! production crate depends on this package.

#![forbid(unsafe_code)]

pub mod ask;
pub mod axes;
pub mod doc;
pub mod judge;
pub mod literals;
pub mod places;
pub mod world;
