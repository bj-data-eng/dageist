//! The session: the graph being edited, its layout, and each transform's step
//! document, each at a revision of its own.
//!
//! An edit names the revision it was made against and is refused as stale
//! when what it edits has moved on, so that an edit made on an old view is
//! never made on a newer state. The graph, the layout and each document count
//! their revisions apart: moving a box does not make an open step edit stale,
//! and editing one transform's steps does not make an edit of the graph stale.

use std::collections::BTreeMap;
use std::fmt;

use dageist_engine::document::{Document, Refusal};
use dageist_graph::{Edit, EditError, Frame, Graph, Kind, Layout, NodeId, NotInGraph};

/// An edit made against a revision that is no longer the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stale {
    /// The revision the edit was made against.
    pub asked: u64,
    /// The revision it is at now.
    pub current: u64,
}

impl fmt::Display for Stale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "it is at revision {} now, not {}",
            self.current, self.asked
        )
    }
}

impl std::error::Error for Stale {}

/// Why a transform's document could not be edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentError {
    /// The graph has no transform with this id.
    NoSuchTransform(NodeId),
    /// The document has moved on to another revision.
    Stale(Stale),
}

impl fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchTransform(node) => write!(formatter, "there is no transform {node}"),
            Self::Stale(stale) => write!(formatter, "the document {stale}"),
        }
    }
}

impl std::error::Error for DocumentError {}

/// A transform's step document and the revision it is at.
#[derive(Debug, Clone)]
struct Held {
    document: Document,
    revision: u64,
}

/// What the session holds while a graph is edited.
#[derive(Debug, Clone)]
pub struct Session {
    graph: Graph,
    graph_revision: u64,
    layout: Layout,
    layout_revision: u64,
    documents: BTreeMap<NodeId, Held>,
    first_revision: u64,
}

/// The document a transform starts with: no steps.
fn no_steps() -> Document {
    Document::new(Vec::new()).unwrap_or_else(|refusal: Refusal| {
        unreachable!("a document with no steps is a document: {refusal}")
    })
}

impl Session {
    /// A session with an empty graph. The graph, the layout and every
    /// document start at `first_revision` and count up from it by one with
    /// each edit; a host starts each session at a revision a view of an
    /// earlier session cannot hold.
    #[must_use]
    pub fn new(first_revision: u64) -> Self {
        Self {
            graph: Graph::new(),
            graph_revision: first_revision,
            layout: Layout::new(),
            layout_revision: first_revision,
            documents: BTreeMap::new(),
            first_revision,
        }
    }

    /// The graph.
    #[must_use]
    pub const fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The revision the graph is at.
    #[must_use]
    pub const fn graph_revision(&self) -> u64 {
        self.graph_revision
    }

    /// The layout.
    #[must_use]
    pub const fn layout(&self) -> &Layout {
        &self.layout
    }

    /// The revision the layout is at.
    #[must_use]
    pub const fn layout_revision(&self) -> u64 {
        self.layout_revision
    }

    /// The step document of the transform `node` and the revision it is at.
    #[must_use]
    pub fn document(&self, node: &NodeId) -> Option<(&Document, u64)> {
        self.documents
            .get(node)
            .map(|held| (&held.document, held.revision))
    }

    fn check(asked: u64, current: u64) -> Result<(), Stale> {
        if asked == current {
            Ok(())
        } else {
            Err(Stale { asked, current })
        }
    }

    /// Makes `edit` on the graph, which must be at `revision`, and gives the
    /// node it added, if any. A transform it adds starts with no steps; a
    /// node it removes takes its document and its place in the layout with
    /// it. A refused edit leaves the session as it was.
    ///
    /// # Errors
    ///
    /// When the graph is at another revision. The inner result is the
    /// graph's answer: the node added, or why the edit was refused.
    pub fn edit_graph(
        &mut self,
        revision: u64,
        edit: &Edit,
    ) -> Result<Result<Option<NodeId>, EditError>, Stale> {
        Self::check(revision, self.graph_revision)?;
        let edited = match self.graph.edit(edit) {
            Ok(edited) => edited,
            Err(error) => return Ok(Err(error)),
        };
        self.graph = edited.graph;
        self.graph_revision += 1;
        if let Some(added) = &edited.added
            && self.graph.node(added).map(dageist_graph::Node::kind) == Some(Kind::Transform)
        {
            self.documents.insert(
                added.clone(),
                Held {
                    document: no_steps(),
                    revision: self.first_revision,
                },
            );
        }
        self.documents
            .retain(|node, _| self.graph.node(node).is_some());
        if self
            .layout
            .frames()
            .any(|(node, _)| self.graph.node(node).is_none())
        {
            self.layout.keep_to(&self.graph);
            self.layout_revision += 1;
        }
        Ok(Ok(edited.added))
    }

    /// Sets where the box of `node` stands, on the layout at `revision`.
    ///
    /// # Errors
    ///
    /// When the layout is at another revision. The inner result says
    /// whether the graph has the node.
    pub fn place(
        &mut self,
        revision: u64,
        node: &NodeId,
        frame: Frame,
    ) -> Result<Result<(), NotInGraph>, Stale> {
        Self::check(revision, self.layout_revision)?;
        let placed = self.layout.set(&self.graph, node, frame);
        if placed.is_ok() {
            self.layout_revision += 1;
        }
        Ok(placed)
    }

    /// Makes an edit on the document of the transform `node`, which must be
    /// at `revision`: `make` gives the edited document or refuses, and an
    /// edited document is held at the next revision, which is given.
    ///
    /// # Errors
    ///
    /// When the graph has no such transform or its document is at another
    /// revision. The inner result is `make`'s refusal, when it refused.
    pub fn edit_document<E>(
        &mut self,
        node: &NodeId,
        revision: u64,
        make: impl FnOnce(&Document) -> Result<Document, E>,
    ) -> Result<Result<u64, E>, DocumentError> {
        let held = self
            .documents
            .get_mut(node)
            .ok_or_else(|| DocumentError::NoSuchTransform(node.clone()))?;
        Self::check(revision, held.revision).map_err(DocumentError::Stale)?;
        Ok(make(&held.document).map(|document| {
            held.document = document;
            held.revision += 1;
            held.revision
        }))
    }
}
