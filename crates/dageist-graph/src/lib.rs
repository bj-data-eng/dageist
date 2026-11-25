//! The graph of a dageist pipeline, without data and without Polars.
//!
//! A [`Graph`] holds nodes of four [kinds](Kind): sources that read a file,
//! transforms that run a step document, outputs that write a file, and
//! Parameters nodes that hold named values. A node reads other nodes through
//! [references](Reference) held in the node itself: a transform names the
//! table its steps start from and what feeds each of its named inputs, and an
//! output names the table it writes. There is no separate list of
//! connections; a wire on the canvas draws a reference.
//!
//! A graph changes only through [`Graph::edit`], which checks an [`Edit`] as
//! it makes it and refuses one that names something the graph lacks, fills a
//! slot from an output of another kind, or would make a node read itself.
//! What a reference cannot be checked against without data, such as whether
//! a column exists, is left to whoever works out the schemas, which this
//! crate knows nothing of. [`Graph::gaps`] says what a node still lacks, and
//! [`Graph::order`] gives the order nodes are worked out in.
//!
//! The [`Layout`] holds where each node's box stands, apart from the graph,
//! so that moving a box is not a change to what the pipeline does.

#![forbid(unsafe_code)]

mod edit;
mod graph;
mod layout;
mod names;
mod node;

pub use edit::{Edit, EditError, Edited};
pub use graph::{Fit, Gap, Graph};
pub use layout::{Frame, Layout, NotInGraph, Place, Size};
pub use names::{InputName, Name, NameError, NodeId, ParameterName};
pub use node::{Body, File, Format, Input, InputKind, Kind, Node, Parameter, Reference, Slot};

#[cfg(test)]
mod tests;
