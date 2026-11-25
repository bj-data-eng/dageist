//! The graph: its nodes, the order they are worked in, what fits each slot,
//! and what is still missing from each node.

use std::collections::{BTreeMap, BTreeSet};

use crate::names::{Name, NodeId};
use crate::node::{Body, InputKind, Kind, Node, Reference, Slot};

/// An output a slot can be filled from. A list is filled from a column of a
/// table, which only the table's schema lists, so a table that fits a list
/// slot stands for every column of it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fit {
    /// The table `node` gives, whole or by one of its columns.
    Table(NodeId),
    /// One value of a Parameters node.
    Parameter(Reference),
}

/// Something a node still lacks, which keeps it from being worked out.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Gap {
    /// A slot is not connected.
    Unconnected(Slot),
    /// A source or an output has no file chosen.
    NoFile,
}

/// A pipeline's graph: nodes that read each other's outputs, with no cycle.
///
/// A graph is changed only by [`edit`](Self::edit), which checks the edit and
/// gives a new graph or refuses it, leaving this one as it was.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    pub(crate) nodes: BTreeMap<NodeId, Node>,
}

impl Graph {
    /// A graph with no nodes.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The node `id`, when the graph has it.
    #[must_use]
    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// Every node, in the order of their ids.
    pub fn nodes(&self) -> impl Iterator<Item = (&NodeId, &Node)> {
        self.nodes.iter()
    }

    /// How many nodes the graph has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the graph has no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Every node in an order in which each comes after every node it reads:
    /// of the nodes whose reads are all placed, the one with the least id
    /// goes next, so the order is the same for the same graph.
    #[must_use]
    pub fn order(&self) -> Vec<&NodeId> {
        let mut waiting: BTreeMap<&NodeId, BTreeSet<&NodeId>> = self
            .nodes
            .iter()
            .map(|(id, node)| {
                let reads = node
                    .references()
                    .map(Reference::node)
                    .filter(|read| self.nodes.contains_key(*read))
                    .collect();
                (id, reads)
            })
            .collect();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(next) = waiting
            .iter()
            .find(|(_, reads)| reads.is_empty())
            .map(|(id, _)| *id)
        {
            waiting.remove(next);
            for reads in waiting.values_mut() {
                reads.remove(next);
            }
            order.push(next);
        }
        // Edits refuse every cycle, so every node is placed.
        debug_assert!(waiting.is_empty(), "the graph has a cycle");
        order
    }

    /// The nodes that read `id`'s outputs, each with the slot it reads through.
    #[must_use]
    pub fn readers(&self, id: &NodeId) -> Vec<(&NodeId, Slot)> {
        self.nodes
            .iter()
            .flat_map(|(reader, node)| {
                node.slots()
                    .into_iter()
                    .filter(|(_, _, from)| from.is_some_and(|from| from.node() == id))
                    .map(move |(slot, _, _)| (reader, slot))
            })
            .collect()
    }

    /// Whether `ancestor` is `node` or a node `node` reads, directly or
    /// through other nodes.
    pub(crate) fn reaches(&self, node: &NodeId, ancestor: &NodeId) -> bool {
        let mut seen = BTreeSet::new();
        let mut stack = vec![node];
        while let Some(next) = stack.pop() {
            if next == ancestor {
                return true;
            }
            if seen.insert(next)
                && let Some(read) = self.nodes.get(next)
            {
                stack.extend(read.references().map(Reference::node));
            }
        }
        false
    }

    /// Every output that fits a slot taking `kind` on `node` without closing a
    /// cycle: tables for a table or a list, parameters for a value. With
    /// `kind` left out, every output that fits any new input.
    #[must_use]
    pub fn fits(&self, node: &NodeId, kind: Option<InputKind>) -> Vec<Fit> {
        let mut fits = Vec::new();
        for (id, candidate) in &self.nodes {
            if self.reaches(id, node) {
                continue;
            }
            let wants = |wanted: InputKind| kind.is_none_or(|kind| kind == wanted);
            match &candidate.body {
                Body::Source { .. } | Body::Transform { .. }
                    if wants(InputKind::Table) || wants(InputKind::List) =>
                {
                    fits.push(Fit::Table(id.clone()));
                }
                Body::Parameters { values } if wants(InputKind::Value) => {
                    fits.extend(values.keys().map(|name| {
                        Fit::Parameter(Reference::Parameter {
                            node: id.clone(),
                            name: name.clone(),
                        })
                    }));
                }
                _ => {}
            }
        }
        fits
    }

    /// What `id` still lacks before it can be worked out: its unconnected
    /// slots, in order, and then a missing file. Nothing for a node the graph
    /// lacks.
    #[must_use]
    pub fn gaps(&self, id: &NodeId) -> Vec<Gap> {
        let Some(node) = self.nodes.get(id) else {
            return Vec::new();
        };
        let mut gaps: Vec<Gap> = node
            .slots()
            .into_iter()
            .filter(|(_, _, from)| from.is_none())
            .map(|(slot, _, _)| Gap::Unconnected(slot))
            .collect();
        if matches!(
            node.body,
            Body::Source { file: None } | Body::Output { file: None, .. }
        ) {
            gaps.push(Gap::NoFile);
        }
        gaps
    }

    /// Whether another node than `except` has a name that differs from
    /// `name` only in case.
    pub(crate) fn name_taken(&self, name: &Name, except: Option<&NodeId>) -> bool {
        self.nodes
            .iter()
            .any(|(id, node)| Some(id) != except && node.name.same_as(name))
    }

    /// The first name of `kind`'s default name, then with ` 2`, ` 3` and so on,
    /// that no node has.
    pub(crate) fn fresh_name(&self, kind: Kind) -> Name {
        let base = kind.default_name();
        (1..)
            .map(|number| {
                let text = if number == 1 {
                    base.to_owned()
                } else {
                    format!("{base} {number}")
                };
                Name::new(&text).expect("a kind's default name is a name")
            })
            .find(|name| !self.name_taken(name, None))
            .expect("some number is free")
    }
}
