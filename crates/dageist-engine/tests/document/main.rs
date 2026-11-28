//! The step document through its public interface: reading and writing it,
//! and running it on a small documents table.

// The tests that still ask Polars here read the asker of the package that owns it, until they
// move onto its grids; each uses a part of it.
#[allow(dead_code)]
#[path = "../../fuzz/src/ask.rs"]
mod ask_polars;
mod counted;
mod counts;
mod dates;
mod defaults;
mod documents;
mod dtypes;
mod edits;
mod floats;
mod frames;
mod groupings;
mod holes;
mod inputs;
mod join_keys;
mod lists;
mod membership;
mod moments;
mod moments_walked;
mod nesting;
mod not_run;
mod nulls;
mod omitted;
mod order;
mod patterns;
mod pivot_groups;
mod preview;
mod ranges;
mod reading;
mod refusals;
mod shown;
mod statistics;
mod structs;
mod summary;
mod support;
mod syntax;
mod taken;
mod text;
mod untyped;
mod walk;
mod writing;
