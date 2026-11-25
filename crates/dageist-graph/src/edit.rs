//! The edits that change a graph, and why an edit is refused.

use std::fmt;

use crate::graph::Graph;
use crate::names::{InputName, Name, NodeId, ParameterName};
use crate::node::{Body, File, Input, InputKind, Kind, Node, Parameter, Reference, Slot};

/// One change to a graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Adds a node of `kind`, named after its kind. With `follows`, a
    /// transform starts from that node's table and an output writes it.
    Add {
        /// What the node is.
        kind: Kind,
        /// The node whose table the new node reads, if any.
        follows: Option<NodeId>,
    },
    /// Removes a node, and clears every reference to it.
    Remove {
        /// The node.
        node: NodeId,
    },
    /// Gives a node another name.
    Rename {
        /// The node.
        node: NodeId,
        /// Its new name.
        name: Name,
    },
    /// Sets or clears the file of a source or an output.
    SetFile {
        /// The source or output.
        node: NodeId,
        /// The file, or nothing to clear it.
        file: Option<File>,
    },
    /// Fills a slot of a node from an output.
    Connect {
        /// The node whose slot is filled.
        node: NodeId,
        /// The slot.
        slot: Slot,
        /// What fills it.
        from: Reference,
    },
    /// Adds an input to a transform and fills it. It is named after the node
    /// it reads, or after the parameter it reads, with a number when the
    /// transform has an input of that name.
    ConnectNew {
        /// The transform.
        node: NodeId,
        /// What feeds the new input.
        from: Reference,
    },
    /// Empties a slot of a node.
    Disconnect {
        /// The node.
        node: NodeId,
        /// The slot.
        slot: Slot,
    },
    /// Removes a named input from a transform.
    RemoveInput {
        /// The transform.
        node: NodeId,
        /// The input.
        name: InputName,
    },
    /// Sets one value of a Parameters node, adding it when it is new.
    SetParameter {
        /// The Parameters node.
        node: NodeId,
        /// The value's name.
        name: ParameterName,
        /// Its dtype and text.
        parameter: Parameter,
    },
    /// Removes one value of a Parameters node, and clears every reference to
    /// it.
    RemoveParameter {
        /// The Parameters node.
        node: NodeId,
        /// The value's name.
        name: ParameterName,
    },
}

/// Why an edit was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The graph has no node with this id.
    NoSuchNode(NodeId),
    /// The node is of a kind the edit does not apply to.
    WrongKind {
        /// The node.
        node: NodeId,
        /// What it is.
        kind: Kind,
    },
    /// The node has no such slot.
    NoSuchSlot {
        /// The node.
        node: NodeId,
        /// The slot.
        slot: Slot,
    },
    /// The node read does not give this output.
    NoSuchOutput(Reference),
    /// The slot takes another kind of input than the reference gives.
    KindMismatch {
        /// What the slot takes.
        takes: InputKind,
        /// What the reference gives.
        gives: InputKind,
    },
    /// The node would read itself, directly or through the nodes it reads.
    Cycle {
        /// The node whose slot was to be filled.
        node: NodeId,
        /// The node it was to read.
        from: NodeId,
    },
    /// Another node already has this name, or one that differs only in case.
    NameTaken(Name),
    /// The transform has no input of this name.
    NoSuchInput(InputName),
    /// The Parameters node has no value of this name.
    NoSuchParameter(ParameterName),
    /// The column named is empty.
    EmptyColumn,
    /// A node of this kind reads no table, so it cannot follow another node.
    ReadsNoTable(Kind),
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchNode(node) => write!(formatter, "there is no node {node}"),
            Self::WrongKind { node, kind } => {
                write!(
                    formatter,
                    "node {node} is a {kind:?} node, which this edit does not apply to"
                )
            }
            Self::NoSuchSlot { node, slot } => {
                write!(formatter, "node {node} has no slot {slot:?}")
            }
            Self::NoSuchOutput(reference) => {
                write!(
                    formatter,
                    "node {} does not give {reference:?}",
                    reference.node()
                )
            }
            Self::KindMismatch { takes, gives } => {
                write!(
                    formatter,
                    "the slot takes a {takes:?} and this gives a {gives:?}"
                )
            }
            Self::Cycle { node, from } => write!(
                formatter,
                "node {node} cannot read node {from}, which reads node {node}"
            ),
            Self::NameTaken(name) => write!(formatter, "another node is named {name:?}"),
            Self::NoSuchInput(name) => write!(formatter, "there is no input {name}"),
            Self::NoSuchParameter(name) => write!(formatter, "there is no parameter {name}"),
            Self::EmptyColumn => formatter.write_str("the column has no name"),
            Self::ReadsNoTable(kind) => {
                write!(
                    formatter,
                    "a {kind:?} node reads no table, so it cannot follow a node"
                )
            }
        }
    }
}

impl std::error::Error for EditError {}

/// A graph after an edit, with the id of the node the edit added, if it added
/// one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    /// The edited graph.
    pub graph: Graph,
    /// The node [`Edit::Add`] added.
    pub added: Option<NodeId>,
}

impl Graph {
    /// Makes `edit` on a copy of this graph and gives the copy, or refuses the
    /// edit and leaves nothing changed.
    ///
    /// # Errors
    ///
    /// When the edit names a node, slot, input or value the graph lacks,
    /// applies to another kind of node, fills a slot from an output of
    /// another kind or one that would make a node read itself, or gives a
    /// name another node has.
    pub fn edit(&self, edit: &Edit) -> Result<Edited, EditError> {
        let mut graph = self.clone();
        let added = graph.apply(edit)?;
        Ok(Edited { graph, added })
    }

    fn node_mut(&mut self, id: &NodeId) -> Result<&mut Node, EditError> {
        self.nodes
            .get_mut(id)
            .ok_or_else(|| EditError::NoSuchNode(id.clone()))
    }

    fn apply(&mut self, edit: &Edit) -> Result<Option<NodeId>, EditError> {
        match edit {
            Edit::Add { kind, follows } => self.add(*kind, follows.as_ref()).map(Some),
            Edit::Remove { node } => {
                if self.nodes.remove(node).is_none() {
                    return Err(EditError::NoSuchNode(node.clone()));
                }
                self.retired.insert(node.clone());
                for other in self.nodes.values_mut() {
                    other.clear_references(|reference| reference.node() != node);
                }
                Ok(None)
            }
            Edit::Rename { node, name } => {
                if self.name_taken(name, Some(node)) {
                    return Err(EditError::NameTaken(name.clone()));
                }
                self.node_mut(node)?.name = name.clone();
                Ok(None)
            }
            Edit::SetFile { node, file } => {
                let target = self.node_mut(node)?;
                let kind = target.kind();
                match &mut target.body {
                    Body::Source { file: slot } | Body::Output { file: slot, .. } => {
                        slot.clone_from(file);
                        Ok(None)
                    }
                    Body::Transform { .. } | Body::Parameters { .. } => Err(EditError::WrongKind {
                        node: node.clone(),
                        kind,
                    }),
                }
            }
            Edit::Connect { node, slot, from } => {
                let takes = self.slot_kind(node, slot)?;
                self.check_reference(node, from)?;
                if takes != from.kind() {
                    return Err(EditError::KindMismatch {
                        takes,
                        gives: from.kind(),
                    });
                }
                self.fill(node, slot, Some(from.clone()))?;
                Ok(None)
            }
            Edit::ConnectNew { node, from } => {
                self.check_reference(node, from)?;
                // A table or a column is named after the node it reads, and a
                // value after the parameter.
                let base = match from {
                    Reference::Table { node: read } | Reference::Column { node: read, .. } => {
                        InputName::made_from(self.nodes[read].name.as_str())
                    }
                    Reference::Parameter { name, .. } => InputName::made_from(name.as_str()),
                };
                let target = self.node_mut(node)?;
                let kind = target.kind();
                let Body::Transform { inputs, .. } = &mut target.body else {
                    return Err(EditError::WrongKind {
                        node: node.clone(),
                        kind,
                    });
                };
                let name = (1..)
                    .map(|number| {
                        if number == 1 {
                            base.clone()
                        } else {
                            base.numbered(number)
                        }
                    })
                    .find(|name| !inputs.contains_key(name))
                    .expect("some number is free");
                inputs.insert(
                    name,
                    Input {
                        kind: from.kind(),
                        from: Some(from.clone()),
                    },
                );
                Ok(None)
            }
            Edit::Disconnect { node, slot } => {
                self.slot_kind(node, slot)?;
                self.fill(node, slot, None)?;
                Ok(None)
            }
            Edit::RemoveInput { node, name } => {
                let target = self.node_mut(node)?;
                let kind = target.kind();
                let Body::Transform { inputs, .. } = &mut target.body else {
                    return Err(EditError::WrongKind {
                        node: node.clone(),
                        kind,
                    });
                };
                inputs
                    .remove(name)
                    .map(|_| None)
                    .ok_or_else(|| EditError::NoSuchInput(name.clone()))
            }
            Edit::SetParameter {
                node,
                name,
                parameter,
            } => {
                let target = self.node_mut(node)?;
                let kind = target.kind();
                let Body::Parameters { values } = &mut target.body else {
                    return Err(EditError::WrongKind {
                        node: node.clone(),
                        kind,
                    });
                };
                values.insert(name.clone(), parameter.clone());
                Ok(None)
            }
            Edit::RemoveParameter { node, name } => {
                let target = self.node_mut(node)?;
                let kind = target.kind();
                let Body::Parameters { values } = &mut target.body else {
                    return Err(EditError::WrongKind {
                        node: node.clone(),
                        kind,
                    });
                };
                if values.remove(name).is_none() {
                    return Err(EditError::NoSuchParameter(name.clone()));
                }
                let removed = Reference::Parameter {
                    node: node.clone(),
                    name: name.clone(),
                };
                for other in self.nodes.values_mut() {
                    other.clear_references(|reference| *reference != removed);
                }
                Ok(None)
            }
        }
    }

    fn add(&mut self, kind: Kind, follows: Option<&NodeId>) -> Result<NodeId, EditError> {
        let mut body = Body::empty(kind);
        if let Some(followed) = follows {
            let read = self
                .nodes
                .get(followed)
                .ok_or_else(|| EditError::NoSuchNode(followed.clone()))?;
            let reference = Reference::Table {
                node: followed.clone(),
            };
            if !read.gives(&reference) {
                return Err(EditError::NoSuchOutput(reference));
            }
            match &mut body {
                Body::Transform { start, .. } => *start = Some(reference),
                Body::Output { written, .. } => *written = Some(reference),
                Body::Source { .. } | Body::Parameters { .. } => {
                    return Err(EditError::ReadsNoTable(kind));
                }
            }
        }
        let name = self.fresh_name(kind);
        let id = self.fresh_id(&name);
        self.nodes.insert(id.clone(), Node { name, body });
        Ok(id)
    }

    /// Checks that `from` names an output its node gives, and that `node`
    /// reading it closes no cycle.
    fn check_reference(&self, node: &NodeId, from: &Reference) -> Result<(), EditError> {
        if !self.nodes.contains_key(node) {
            return Err(EditError::NoSuchNode(node.clone()));
        }
        let read = self
            .nodes
            .get(from.node())
            .ok_or_else(|| EditError::NoSuchNode(from.node().clone()))?;
        if let Reference::Column { column, .. } = from
            && column.is_empty()
        {
            return Err(EditError::EmptyColumn);
        }
        if !read.gives(from) {
            return Err(EditError::NoSuchOutput(from.clone()));
        }
        if self.reaches(from.node(), node) {
            return Err(EditError::Cycle {
                node: node.clone(),
                from: from.node().clone(),
            });
        }
        Ok(())
    }

    fn slot_kind(&self, node: &NodeId, slot: &Slot) -> Result<InputKind, EditError> {
        self.nodes
            .get(node)
            .ok_or_else(|| EditError::NoSuchNode(node.clone()))?
            .slot_kind(slot)
            .ok_or_else(|| EditError::NoSuchSlot {
                node: node.clone(),
                slot: slot.clone(),
            })
    }

    fn fill(
        &mut self,
        node: &NodeId,
        slot: &Slot,
        from: Option<Reference>,
    ) -> Result<(), EditError> {
        let missing = || EditError::NoSuchSlot {
            node: node.clone(),
            slot: slot.clone(),
        };
        match (&mut self.node_mut(node)?.body, slot) {
            (Body::Transform { start, .. }, Slot::Start) => *start = from,
            (Body::Output { written, .. }, Slot::Written) => *written = from,
            (Body::Transform { inputs, .. }, Slot::Input(name)) => {
                inputs.get_mut(name).ok_or_else(missing)?.from = from;
            }
            _ => return Err(missing()),
        }
        Ok(())
    }
}
