//! Where each node's box stands on the canvas, kept apart from the graph so
//! that moving a box is not a change to what the pipeline does.

use std::collections::BTreeMap;

use crate::graph::Graph;
use crate::names::NodeId;

/// Where a box's top-left corner stands, in canvas units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Place {
    /// Rightwards.
    pub x: i32,
    /// Downwards.
    pub y: i32,
}

/// A box's size, in canvas units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Size {
    /// Across.
    pub width: u32,
    /// Down.
    pub height: u32,
}

/// One node's box: where it stands, and its size once it has been resized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Frame {
    /// Where it stands.
    pub place: Place,
    /// Its size, or nothing while it takes the size its content needs.
    pub size: Option<Size>,
}

/// The layout of a graph: a frame for each node that has been placed. A node
/// with no frame is placed by whoever draws it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    frames: BTreeMap<NodeId, Frame>,
}

/// The graph has no node with this id, so it has no box to place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotInGraph(pub NodeId);

impl std::fmt::Display for NotInGraph {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "there is no node {}", self.0)
    }
}

impl std::error::Error for NotInGraph {}

impl Layout {
    /// A layout with no frames.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The frame of `node`, when it has been placed.
    #[must_use]
    pub fn frame(&self, node: &NodeId) -> Option<Frame> {
        self.frames.get(node).copied()
    }

    /// Every frame, by node id.
    pub fn frames(&self) -> impl Iterator<Item = (&NodeId, &Frame)> {
        self.frames.iter()
    }

    /// Sets the frame of `node`, a node of `graph`.
    ///
    /// # Errors
    ///
    /// When `graph` has no node `node`.
    pub fn set(&mut self, graph: &Graph, node: &NodeId, frame: Frame) -> Result<(), NotInGraph> {
        if graph.node(node).is_none() {
            return Err(NotInGraph(node.clone()));
        }
        self.frames.insert(node.clone(), frame);
        Ok(())
    }

    /// Drops the frames of nodes `graph` does not have.
    pub fn keep_to(&mut self, graph: &Graph) {
        self.frames.retain(|node, _| graph.node(node).is_some());
    }
}
