//! Working out the graph: each node in order, from the tables and values the
//! nodes before it give, without reading rows.
//!
//! A source scans its file and reads the file's schema. A Parameters node
//! reads each of its values as its dtype. A transform binds the table its
//! steps start from and each of its named inputs to what feeds them, and its
//! steps are run as a plan, which gives the table it gives. An output takes
//! the columns of the table it would write. What stops a node is reported on
//! it as a [`Problem`], and the nodes that read it report that it gives
//! nothing.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use dageist_engine::document::{ColumnDescription, Inputs, Report};
use dageist_engine::{FileFormat, Frame};
use dageist_graph::{Body, Format, Gap, InputName, NodeId, ParameterName, Reference, Slot};

use crate::session::Session;

/// Something that keeps a node from being worked out, or that it would do
/// wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The node lacks a connection or a file.
    Gap(Gap),
    /// A source's file could not be read; the text says why.
    FileUnreadable(String),
    /// A value of a Parameters node is not a value of its dtype.
    ParameterUnreadable {
        /// The value.
        name: ParameterName,
        /// Why.
        reason: String,
    },
    /// A slot reads a node that gives nothing, because of its own problems.
    NothingGiven {
        /// The slot.
        slot: Slot,
        /// The node read.
        from: NodeId,
    },
    /// A slot reads a column the table it names does not have.
    NoSuchColumn {
        /// The slot.
        slot: Slot,
        /// The column.
        column: String,
    },
    /// A slot's input could not be connected; the text says why.
    InputUnreadable {
        /// The slot.
        slot: Slot,
        /// Why.
        reason: String,
    },
    /// A step of the transform is refused or still to be completed, so the
    /// transform gives no table. Its steps' report says which and why.
    StepsStopped,
    /// The engine could not run the transform's steps; the text says why.
    RunFailed(String),
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let slot = |slot: &Slot| match slot {
            Slot::Start => "the table the steps start from".to_owned(),
            Slot::Input(name) => format!("the input {name}"),
            Slot::Written => "the table written".to_owned(),
        };
        match self {
            Self::Gap(Gap::Unconnected(unconnected)) => {
                write!(formatter, "{} is not connected", slot(unconnected))
            }
            Self::Gap(Gap::NoFile) => formatter.write_str("no file is chosen"),
            Self::FileUnreadable(reason) => write!(formatter, "the file cannot be read: {reason}"),
            Self::ParameterUnreadable { name, reason } => write!(formatter, "{name}: {reason}"),
            Self::NothingGiven { slot: read, from } => {
                write!(
                    formatter,
                    "{} reads {from}, which gives nothing",
                    slot(read)
                )
            }
            Self::NoSuchColumn { slot: read, column } => {
                write!(
                    formatter,
                    "{} reads a column {column:?} that is not there",
                    slot(read)
                )
            }
            Self::InputUnreadable { slot: read, reason } => {
                write!(formatter, "{}: {reason}", slot(read))
            }
            Self::StepsStopped => formatter.write_str("a step is refused or still to be completed"),
            Self::RunFailed(reason) => write!(formatter, "the steps could not run: {reason}"),
        }
    }
}

/// What a transform's steps run on: the table they start from and their
/// named inputs.
#[derive(Debug, Clone)]
pub struct Bindings {
    /// The name of the node whose table the steps start from.
    pub start_name: String,
    /// That table.
    pub start: Frame,
    /// The other inputs, each bound to what feeds it.
    pub inputs: Inputs,
}

/// What working out one node gave.
#[derive(Debug, Default)]
pub struct Worked {
    /// The columns of the table the node gives, or the columns an output
    /// would write, when they are known.
    pub columns: Option<Vec<ColumnDescription>>,
    /// What keeps the node from being worked out, in the order found.
    pub problems: Vec<Problem>,
    /// A transform's bindings, once the table its steps start from is known.
    pub bindings: Option<Bindings>,
    /// A transform's report of its steps, once they have run.
    pub report: Option<Report>,
    output: Option<Frame>,
}

/// What working out the graph gave, node by node.
#[derive(Debug, Default)]
pub struct Workings {
    nodes: BTreeMap<NodeId, Worked>,
}

impl Workings {
    /// What working out `node` gave.
    #[must_use]
    pub fn node(&self, node: &NodeId) -> Option<&Worked> {
        self.nodes.get(node)
    }

    /// Takes what working out `node` gave.
    pub fn take(&mut self, node: &NodeId) -> Option<Worked> {
        self.nodes.remove(node)
    }

    fn frame(&self, node: &NodeId) -> Option<&Frame> {
        self.nodes
            .get(node)
            .and_then(|worked| worked.output.as_ref())
    }

    fn has_column(&self, node: &NodeId, column: &str) -> bool {
        self.nodes
            .get(node)
            .and_then(|worked| worked.columns.as_ref())
            .is_some_and(|columns| columns.iter().any(|known| known.name() == column))
    }
}

const fn engine_format(format: Format) -> FileFormat {
    match format {
        Format::Parquet => FileFormat::Parquet,
        Format::Csv => FileFormat::Csv,
    }
}

impl Session {
    /// Works out every node of the graph in order. No rows are read, except
    /// the column a list input takes and what a step that needs a collect
    /// reads.
    #[must_use]
    pub fn work_out(&self) -> Workings {
        let mut workings = Workings::default();
        for id in self.graph().order() {
            let worked = self.work_out_node(id, &workings);
            workings.nodes.insert(id.clone(), worked);
        }
        workings
    }

    fn name_of(&self, node: &NodeId) -> String {
        self.graph()
            .node(node)
            .map_or_else(|| node.to_string(), |read| read.name().to_string())
    }

    fn work_out_node(&self, id: &NodeId, workings: &Workings) -> Worked {
        let Some(node) = self.graph().node(id) else {
            return Worked::default();
        };
        let mut worked = Worked {
            problems: self
                .graph()
                .gaps(id)
                .into_iter()
                .map(Problem::Gap)
                .collect(),
            ..Worked::default()
        };
        match node.body() {
            Body::Source { file } => {
                if let Some(file) = file {
                    let scanned = Frame::scan(Path::new(&file.path), engine_format(file.format))
                        .and_then(|frame| frame.columns().map(|columns| (frame, columns)));
                    match scanned {
                        Ok((frame, columns)) => {
                            worked.columns = Some(columns);
                            worked.output = Some(frame);
                        }
                        Err(error) => worked
                            .problems
                            .push(Problem::FileUnreadable(error.to_string())),
                    }
                }
            }
            Body::Parameters { values } => {
                for (name, parameter) in values {
                    let mut scratch = Inputs::new();
                    if let Err(error) =
                        scratch.add_parameter(name.as_str(), &parameter.dtype, &parameter.text)
                    {
                        worked.problems.push(Problem::ParameterUnreadable {
                            name: name.clone(),
                            reason: error.to_string(),
                        });
                    }
                }
            }
            Body::Output { written, .. } => {
                if let Some(reference) = written {
                    match workings
                        .nodes
                        .get(reference.node())
                        .and_then(|read| read.columns.clone())
                    {
                        Some(columns) => worked.columns = Some(columns),
                        None => worked.problems.push(Problem::NothingGiven {
                            slot: Slot::Written,
                            from: reference.node().clone(),
                        }),
                    }
                }
            }
            Body::Transform { start, inputs } => {
                self.work_out_transform(id, start.as_ref(), inputs, workings, &mut worked);
            }
        }
        worked
    }

    fn work_out_transform(
        &self,
        id: &NodeId,
        start: Option<&Reference>,
        inputs: &BTreeMap<InputName, dageist_graph::Input>,
        workings: &Workings,
        worked: &mut Worked,
    ) {
        let mut bound = Inputs::new();
        for (name, input) in inputs {
            let Some(reference) = &input.from else {
                continue;
            };
            let slot = Slot::Input(name.clone());
            if let Err(problem) = self.bind(&mut bound, name, &slot, reference, workings) {
                worked.problems.push(problem);
            }
        }
        let Some(start) = start else {
            return;
        };
        let Some(frame) = workings.frame(start.node()) else {
            worked.problems.push(Problem::NothingGiven {
                slot: Slot::Start,
                from: start.node().clone(),
            });
            return;
        };
        let Some((document, _)) = self.document(id) else {
            return;
        };
        match document.run_frame(&bound, frame) {
            Ok(report) => {
                match report.output() {
                    Some(output) => match output.columns() {
                        Ok(columns) => {
                            worked.columns = Some(columns);
                            worked.output = Some(output);
                        }
                        Err(error) => worked.problems.push(Problem::RunFailed(error.to_string())),
                    },
                    None => worked.problems.push(Problem::StepsStopped),
                }
                worked.report = Some(report);
            }
            Err(error) => worked.problems.push(Problem::RunFailed(error.to_string())),
        }
        worked.bindings = Some(Bindings {
            start_name: self.name_of(start.node()),
            start: frame.clone(),
            inputs: bound,
        });
    }

    /// Binds the input `name` of a transform to what `reference` names.
    fn bind(
        &self,
        bound: &mut Inputs,
        name: &InputName,
        slot: &Slot,
        reference: &Reference,
        workings: &Workings,
    ) -> Result<(), Problem> {
        let unreadable = |reason: String| Problem::InputUnreadable {
            slot: slot.clone(),
            reason,
        };
        let read = reference.node();
        match reference {
            Reference::Parameter { name: value, .. } => {
                let parameter = match self.graph().node(read).map(dageist_graph::Node::body) {
                    Some(Body::Parameters { values }) => values.get(value),
                    _ => None,
                };
                let Some(parameter) = parameter else {
                    return Err(unreadable(format!("there is no parameter {value}")));
                };
                bound
                    .add_parameter(name.as_str(), &parameter.dtype, &parameter.text)
                    .map_err(|error| unreadable(error.to_string()))
            }
            Reference::Table { .. } | Reference::Column { .. } => {
                let Some(frame) = workings.frame(read) else {
                    return Err(Problem::NothingGiven {
                        slot: slot.clone(),
                        from: read.clone(),
                    });
                };
                if let Reference::Column { column, .. } = reference {
                    if !workings.has_column(read, column) {
                        return Err(Problem::NoSuchColumn {
                            slot: slot.clone(),
                            column: column.clone(),
                        });
                    }
                    bound
                        .add_column(name.as_str(), self.name_of(read), column, frame)
                        .map_err(|error| unreadable(error.to_string()))
                } else {
                    bound
                        .add_frame(name.as_str(), self.name_of(read), frame)
                        .map_err(|error| unreadable(error.to_string()))
                }
            }
        }
    }
}
