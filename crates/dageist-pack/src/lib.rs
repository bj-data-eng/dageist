//! Git packs whose new objects are stored as deltas against bases the caller names.
//!
//! gix writes packs but creates no deltas: an object that is not already in a pack is stored
//! whole. This crate makes the deltas and writes them through gix's own pack writer, so that a
//! history in which each object is a small change of the one before it is stored small.
//!
//! [`write_pack`] takes a list of [objects](Object), each naming an earlier one of its kind as
//! its base, or none. It stores an object as a [delta](delta::encode) against its base when that
//! is smaller than storing it whole, up to a depth of deltas the [options](Options) set, and
//! writes the pack and its index into a folder, where git reads them as it reads its own.
//!
//! The caller chooses each base. A file's previous version is the natural one, so no search for
//! a similar object is made. Each base must be in the same pack, because git's packs on disk
//! hold every base their deltas need.

#![forbid(unsafe_code)]

pub mod delta;
mod pack;

pub use pack::{Object, Options, PackError, Written, write_pack};
