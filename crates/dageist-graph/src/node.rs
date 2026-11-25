//! A node of the graph: its kind, what it is set to, and the references
//! through which it reads other nodes' outputs.

use std::collections::BTreeMap;

use crate::names::{InputName, Name, NodeId, ParameterName};

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// Reads a table from a file.
    Source,
    /// Runs a step document on the table it starts from and its other inputs.
    Transform,
    /// Writes a table to a file.
    Output,
    /// Holds named values for other nodes to read.
    Parameters,
}

impl Kind {
    /// The name a node of this kind is given when it is added, before any
    /// number that keeps it apart from another node's name.
    #[must_use]
    pub const fn default_name(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Transform => "Transform",
            Self::Output => "Output",
            Self::Parameters => "Parameters",
        }
    }

    /// Whether a node of this kind gives a table other nodes can read.
    #[must_use]
    pub const fn gives_table(self) -> bool {
        matches!(self, Self::Source | Self::Transform)
    }
}

/// How a file is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Format {
    /// Apache Parquet.
    Parquet,
    /// Comma-separated text with a header row.
    Csv,
}

/// A file a source reads or an output writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Where the file is, as the system's file dialog gave it.
    pub path: String,
    /// How it is written.
    pub format: Format,
}

/// What an input of a node takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputKind {
    /// A whole table another node gives.
    Table,
    /// One value of a Parameters node.
    Value,
    /// The values of one column of a table another node gives.
    List,
}

/// What a node reads: another node's table, one column of it as a list, or
/// one value of a Parameters node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reference {
    /// The table `node` gives.
    Table {
        /// The node that gives the table.
        node: NodeId,
    },
    /// The values of the column `column` of the table `node` gives.
    Column {
        /// The node that gives the table.
        node: NodeId,
        /// The column, by its name in that table.
        column: String,
    },
    /// The value `name` of the Parameters node `node`.
    Parameter {
        /// The Parameters node.
        node: NodeId,
        /// The value, by its name.
        name: ParameterName,
    },
}

impl Reference {
    /// The node read.
    #[must_use]
    pub const fn node(&self) -> &NodeId {
        match self {
            Self::Table { node } | Self::Column { node, .. } | Self::Parameter { node, .. } => node,
        }
    }

    /// The kind of input this reference can feed.
    #[must_use]
    pub const fn kind(&self) -> InputKind {
        match self {
            Self::Table { .. } => InputKind::Table,
            Self::Column { .. } => InputKind::List,
            Self::Parameter { .. } => InputKind::Value,
        }
    }
}

/// A named input of a transform: what its steps take, and what feeds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// What the input takes.
    pub kind: InputKind,
    /// What feeds it, or nothing while it is not connected.
    pub from: Option<Reference>,
}

/// One value of a Parameters node, as the engine reads it: the text is read as
/// the dtype, and the graph does not check either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// The dtype, as Polars names it, such as `Int64` or `Date`.
    pub dtype: String,
    /// The value as it is typed, such as `1` or `2026-03-02`.
    pub text: String,
}

/// A place on a node that a reference can fill.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    /// The table a transform's steps start from.
    Start,
    /// A named input of a transform.
    Input(InputName),
    /// The table an output writes.
    Written,
}

/// What a node of each kind is set to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// A source: the file it reads, once it is chosen.
    Source {
        /// The file, or nothing until one is chosen.
        file: Option<File>,
    },
    /// A transform: the table its steps start from and its named inputs. Its
    /// steps are a step document held beside the graph.
    Transform {
        /// The table the steps start from, or nothing while it is not
        /// connected.
        start: Option<Reference>,
        /// The other inputs, by the names the steps use.
        inputs: BTreeMap<InputName, Input>,
    },
    /// An output: the table it writes and the file it writes it to.
    Output {
        /// The table, or nothing while it is not connected.
        written: Option<Reference>,
        /// The file, or nothing until one is chosen.
        file: Option<File>,
    },
    /// A Parameters node: its values, by name.
    Parameters {
        /// The values.
        values: BTreeMap<ParameterName, Parameter>,
    },
}

impl Body {
    /// An empty body of `kind`: no file, no references, no values.
    #[must_use]
    pub const fn empty(kind: Kind) -> Self {
        match kind {
            Kind::Source => Self::Source { file: None },
            Kind::Transform => Self::Transform {
                start: None,
                inputs: BTreeMap::new(),
            },
            Kind::Output => Self::Output {
                written: None,
                file: None,
            },
            Kind::Parameters => Self::Parameters {
                values: BTreeMap::new(),
            },
        }
    }

    /// The kind of node this is the body of.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Self::Source { .. } => Kind::Source,
            Self::Transform { .. } => Kind::Transform,
            Self::Output { .. } => Kind::Output,
            Self::Parameters { .. } => Kind::Parameters,
        }
    }
}

/// A node of the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub(crate) name: Name,
    pub(crate) body: Body,
}

impl Node {
    /// The name the node shows.
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// What the node is set to.
    #[must_use]
    pub const fn body(&self) -> &Body {
        &self.body
    }

    /// What the node is.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.body.kind()
    }

    /// The node's slots in order, each with the kind it takes and what fills
    /// it: a transform's start and then its inputs by name, or an output's
    /// table. Sources and Parameters nodes have none.
    #[must_use]
    pub fn slots(&self) -> Vec<(Slot, InputKind, Option<&Reference>)> {
        match &self.body {
            Body::Transform { start, inputs } => {
                let mut slots = vec![(Slot::Start, InputKind::Table, start.as_ref())];
                slots.extend(inputs.iter().map(|(name, input)| {
                    (Slot::Input(name.clone()), input.kind, input.from.as_ref())
                }));
                slots
            }
            Body::Output { written, .. } => {
                vec![(Slot::Written, InputKind::Table, written.as_ref())]
            }
            Body::Source { .. } | Body::Parameters { .. } => Vec::new(),
        }
    }

    /// The kind `slot` takes, when the node has it.
    #[must_use]
    pub fn slot_kind(&self, slot: &Slot) -> Option<InputKind> {
        match (&self.body, slot) {
            (Body::Transform { .. }, Slot::Start) | (Body::Output { .. }, Slot::Written) => {
                Some(InputKind::Table)
            }
            (Body::Transform { inputs, .. }, Slot::Input(name)) => {
                inputs.get(name).map(|input| input.kind)
            }
            _ => None,
        }
    }

    /// The references the node reads through, in the order of its slots.
    pub fn references(&self) -> impl Iterator<Item = &Reference> {
        let (one, many): (Option<&Reference>, Option<&BTreeMap<InputName, Input>>) =
            match &self.body {
                Body::Transform { start, inputs } => (start.as_ref(), Some(inputs)),
                Body::Output { written, .. } => (written.as_ref(), None),
                Body::Source { .. } | Body::Parameters { .. } => (None, None),
            };
        one.into_iter().chain(
            many.into_iter()
                .flat_map(|inputs| inputs.values().filter_map(|input| input.from.as_ref())),
        )
    }

    /// Whether the node gives `reference`'s output, leaving aside whether a
    /// column exists, which only the schema says.
    pub(crate) fn gives(&self, reference: &Reference) -> bool {
        match reference {
            Reference::Table { .. } | Reference::Column { .. } => self.kind().gives_table(),
            Reference::Parameter { name, .. } => {
                matches!(&self.body, Body::Parameters { values } if values.contains_key(name))
            }
        }
    }

    /// Clears every slot whose reference `keep` does not keep.
    pub(crate) fn clear_references(&mut self, keep: impl Fn(&Reference) -> bool) {
        let clear = |slot: &mut Option<Reference>| {
            if slot.as_ref().is_some_and(|reference| !keep(reference)) {
                *slot = None;
            }
        };
        match &mut self.body {
            Body::Transform { start, inputs } => {
                clear(start);
                for input in inputs.values_mut() {
                    clear(&mut input.from);
                }
            }
            Body::Output { written, .. } => clear(written),
            Body::Source { .. } | Body::Parameters { .. } => {}
        }
    }
}
