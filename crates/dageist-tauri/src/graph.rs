//! Commands that report and edit the graph the host holds: the graph view the
//! canvas draws, the edits of the graph and its layout, and the system's file
//! dialog that picks a source's or an output's file.
//!
//! The host holds one [`Workspace`]: the core's session, which holds the graph,
//! its layout and each transform's step document, and the workspace on disk every
//! edit is saved to as a commit. Each command locks it for as long as it answers,
//! and an edit is saved before the lock is let go, so the commits are in the order
//! the edits were made. The commands are thin: the functions beside them answer,
//! so that the bridge serves the same views. The shapes mirror `GraphView`,
//! `GraphEditRequest`, `GraphEditAnswerView` and `FrameView` in
//! `ui/packages/host/src/contract.ts`.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use dageist::{Problem, Session, Workings};
use dageist_graph::{
    Body, Edit, File, Fit, Format, Frame, InputKind, InputName, Kind, Name, NodeId, Parameter,
    ParameterName, Place, Reference, Size, Slot,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::report::FieldView;
use crate::sample_table::{ErrorKind, ErrorView};

/// Where the host saves the session's edits.
#[derive(Debug)]
enum Keeping {
    /// Nowhere, as the tests hold a session.
    #[cfg(test)]
    Nowhere,
    /// In a workspace on disk.
    In(Box<dageist::workspace::Workspace>),
    /// In a workspace that was closed as the application exits; an edit made after it is
    /// refused its save.
    Closed,
}

/// What the host holds while a graph is edited: the core's session, and the workspace on disk
/// its edits are saved to. Tauri keeps it as managed state; the bridge keeps it for as long as it
/// runs.
#[derive(Debug)]
pub struct Workspace {
    session: Mutex<Session>,
    /// The workspace on disk, when the session is kept in one. It is locked only while the
    /// session is, after it, so that saves are made in the order of the edits.
    kept: Mutex<Keeping>,
}

impl Workspace {
    /// A workspace holding `session`, saved nowhere, as the tests hold one.
    #[cfg(test)]
    pub const fn of(session: Session) -> Self {
        Self {
            session: Mutex::new(session),
            kept: Mutex::new(Keeping::Nowhere),
        }
    }

    /// A workspace holding `session`, every edit of which is saved to `kept`.
    pub fn kept(session: Session, kept: dageist::workspace::Workspace) -> Self {
        Self {
            session: Mutex::new(session),
            kept: Mutex::new(Keeping::In(Box::new(kept))),
        }
    }

    /// The workspace the application opens on: the sample workspace in the data folder `data`,
    /// made there from the files the application carries the first time, and opened as it was
    /// left after that. Every revision of its session starts at `first_revision`.
    ///
    /// # Errors
    ///
    /// When the sample cannot be made, or the workspace in its folder cannot be opened.
    pub fn sample(data: &Path, first_revision: u64) -> Result<Self, ErrorView> {
        let root = crate::sample_workspace::sample_root(data);
        crate::sample_workspace::unpack(&root)?;
        let (kept, saved) = dageist::workspace::Workspace::open(
            &root,
            dageist::unsigned_author(),
            dageist::workspace::Options::default(),
        )
        .map_err(|error| ErrorView::new(format!("opening {}: {error}", root.display())))?;
        let session = Session::from_saved(saved, first_revision)
            .map_err(|error| ErrorView::new(format!("opening {}: {error}", root.display())))?;
        Ok(Self::kept(session, kept))
    }

    /// Saves the session, as it is after an edit, to the workspace on disk with `message`. It
    /// takes the session's guard, so that a save is made only under the lock the edit was made
    /// under, and saves keep the edits' order.
    ///
    /// # Errors
    ///
    /// When the workspace cannot be written, or has been closed. The edit is held either way, and
    /// a later save of an open workspace holds it too.
    pub(crate) fn save(
        &self,
        session: &MutexGuard<'_, Session>,
        message: &str,
    ) -> Result<(), ErrorView> {
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        match &mut *kept {
            #[cfg(test)]
            Keeping::Nowhere => Ok(()),
            Keeping::In(kept) => kept
                .save(&session.saved(), message)
                .map(drop)
                .map_err(|error| ErrorView::new(format!("saving the workspace: {error}"))),
            Keeping::Closed => Err(ErrorView::new(
                "the workspace is closed, so the edit is not saved",
            )),
        }
    }

    /// Opens the workspace on disk in `folder` in place of the one held, with every revision of
    /// its session starting at `first_revision`, and closes the one held, packing it whole. The
    /// workspace is read before anything is let go, so one that cannot be opened leaves the one
    /// held as it was. Opening the workspace held keeps it as it is.
    ///
    /// # Errors
    ///
    /// When `folder` holds no workspace this application reads, or the application is closing.
    pub(crate) fn open(&self, folder: &Path, first_revision: u64) -> Result<(), ErrorView> {
        let closing = || ErrorView::new("the application is closing, so no workspace is opened");
        {
            let kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
            if matches!(*kept, Keeping::Closed) {
                return Err(closing());
            }
            // The workspace held is open already: opening it again keeps it as it is.
            if let (Keeping::In(held), Ok(asked)) = (&*kept, folder.canonicalize())
                && held.root() == asked
            {
                return Ok(());
            }
        }
        // The workspace asked for is read with nothing of the host's locked, so other commands go
        // on while it waits on a lock another process holds.
        let (opened, saved) = match dageist::workspace::Workspace::open(
            folder,
            dageist::unsigned_author(),
            dageist::workspace::Options::default(),
        ) {
            Ok(opened) => opened,
            // An opening of the same folder made at the same time took it first, so it is the
            // workspace held now, which opening again keeps as it is.
            Err(dageist::workspace::WorkspaceError::Held(_)) if self.holds(folder) => {
                return Ok(());
            }
            Err(error) => {
                return Err(ErrorView::new(format!(
                    "opening {}: {error}",
                    folder.display()
                )));
            }
        };
        let read = Session::from_saved(saved, first_revision)
            .map_err(|error| ErrorView::new(format!("opening {}: {error}", folder.display())))?;
        let mut session = self.lock();
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*kept, Keeping::Closed) {
            return Err(closing());
        }
        *session = read;
        let held = std::mem::replace(&mut *kept, Keeping::In(Box::new(opened)));
        if let Keeping::In(held) = held {
            // The workspace let go is closed as the application would close it on exit. One that
            // cannot be packed keeps its loose objects, which its next close packs; the opening
            // has been made all the same, so the failure is told, not answered.
            if let Err(error) = held.close() {
                eprintln!("dageist: the workspace let go was not packed: {error}");
            }
        }
        Ok(())
    }

    /// Whether the workspace held is the one in `folder`.
    fn holds(&self, folder: &Path) -> bool {
        let kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        matches!((&*kept, folder.canonicalize()), (Keeping::In(held), Ok(asked)) if held.root() == asked)
    }

    /// Closes the workspace on disk, packing it whole, once the edit being made is done. An edit
    /// made after it is refused its save.
    ///
    /// # Errors
    ///
    /// When the workspace cannot be packed.
    pub fn close(&self) -> Result<(), ErrorView> {
        let _session = self.lock();
        let kept = std::mem::replace(
            &mut *self.kept.lock().unwrap_or_else(PoisonError::into_inner),
            Keeping::Closed,
        );
        match kept {
            Keeping::In(kept) => kept
                .close()
                .map_err(|error| ErrorView::new(format!("closing the workspace: {error}"))),
            _ => Ok(()),
        }
    }

    /// The session, locked for as long as the guard is held. Hold it only to read or make an
    /// edit; work a graph out on a [`snapshot`](Self::snapshot), so that reading files does not
    /// keep other commands waiting.
    pub(crate) fn lock(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A copy of the session as it is now, to work out or read without holding the lock. Every
    /// view of it carries the revisions it was taken at, so a later edit made on it is checked
    /// against the session as it is then.
    pub(crate) fn snapshot(&self) -> Session {
        self.lock().clone()
    }
}

// -- Views ---------------------------------------------------------------------------------

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KindView {
    /// Reads a table from a file.
    Source,
    /// Runs steps on a table.
    Transform,
    /// Writes a table to a file.
    Output,
    /// Holds named values.
    Parameters,
}

impl KindView {
    const fn of(kind: Kind) -> Self {
        match kind {
            Kind::Source => Self::Source,
            Kind::Transform => Self::Transform,
            Kind::Output => Self::Output,
            Kind::Parameters => Self::Parameters,
        }
    }

    const fn kind(self) -> Kind {
        match self {
            Self::Source => Kind::Source,
            Self::Transform => Kind::Transform,
            Self::Output => Kind::Output,
            Self::Parameters => Kind::Parameters,
        }
    }
}

/// What an input takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TakesView {
    /// A whole table.
    Table,
    /// One value.
    Value,
    /// The values of one column.
    List,
}

impl TakesView {
    const fn of(kind: InputKind) -> Self {
        match kind {
            InputKind::Table => Self::Table,
            InputKind::Value => Self::Value,
            InputKind::List => Self::List,
        }
    }

    const fn kind(self) -> InputKind {
        match self {
            Self::Table => InputKind::Table,
            Self::Value => InputKind::Value,
            Self::List => InputKind::List,
        }
    }
}

/// A place on a node a reference fills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum SlotView {
    /// The table a transform's steps start from.
    Start,
    /// A named input of a transform.
    Input { name: String },
    /// The table an output writes.
    Written,
}

impl SlotView {
    fn of(slot: &Slot) -> Self {
        match slot {
            Slot::Start => Self::Start,
            Slot::Input(name) => Self::Input {
                name: name.as_str().to_owned(),
            },
            Slot::Written => Self::Written,
        }
    }

    fn slot(&self) -> Result<Slot, ErrorView> {
        Ok(match self {
            Self::Start => Slot::Start,
            Self::Input { name } => Slot::Input(input_name(name)?),
            Self::Written => Slot::Written,
        })
    }
}

/// What a node reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ReferenceView {
    /// The table a node gives.
    Table { node: String },
    /// The values of one column of the table a node gives.
    Column { node: String, column: String },
    /// One value of a Parameters node.
    Parameter { node: String, name: String },
}

impl ReferenceView {
    fn of(reference: &Reference) -> Self {
        match reference {
            Reference::Table { node } => Self::Table {
                node: node.to_string(),
            },
            Reference::Column { node, column } => Self::Column {
                node: node.to_string(),
                column: column.clone(),
            },
            Reference::Parameter { node, name } => Self::Parameter {
                node: node.to_string(),
                name: name.to_string(),
            },
        }
    }

    fn reference(&self) -> Result<Reference, ErrorView> {
        Ok(match self {
            Self::Table { node } => Reference::Table {
                node: known_id(node)?,
            },
            Self::Column { node, column } => Reference::Column {
                node: known_id(node)?,
                column: column.clone(),
            },
            Self::Parameter { node, name } => Reference::Parameter {
                node: known_id(node)?,
                name: ParameterName::new(name.as_str()).map_err(unknown)?,
            },
        })
    }
}

/// An output a slot can be filled from: a node's table, whole or by a column, or one value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FitView {
    /// The table a node gives.
    Table { node: String },
    /// One value of a Parameters node.
    Parameter { node: String, name: String },
}

impl FitView {
    fn of(fit: &Fit) -> Self {
        match fit {
            Fit::Table(node) => Self::Table {
                node: node.to_string(),
            },
            Fit::Parameter(reference) => match ReferenceView::of(reference) {
                ReferenceView::Parameter { node, name } => Self::Parameter { node, name },
                ReferenceView::Table { node } | ReferenceView::Column { node, .. } => {
                    Self::Table { node }
                }
            },
        }
    }
}

/// One slot of a node: what it takes, what fills it, and what could.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlotStateView {
    slot: SlotView,
    takes: TakesView,
    from: Option<ReferenceView>,
    fits: Vec<FitView>,
}

/// A file a source reads or an output writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileView {
    path: String,
    format: FormatView,
}

/// How a file is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormatView {
    /// Apache Parquet.
    Parquet,
    /// Comma-separated text.
    Csv,
}

impl FileView {
    /// The file at `path`, CSV when its extension says so and Parquet otherwise.
    pub(crate) fn chosen(path: &Path) -> Self {
        Self {
            path: path.to_string_lossy().into_owned(),
            format: if crate::dialog::is_csv(path) {
                FormatView::Csv
            } else {
                FormatView::Parquet
            },
        }
    }

    fn of(file: &File) -> Self {
        Self {
            path: file.path.clone(),
            format: match file.format {
                Format::Parquet => FormatView::Parquet,
                Format::Csv => FormatView::Csv,
            },
        }
    }

    fn file(&self) -> File {
        File {
            path: self.path.clone(),
            format: match self.format {
                FormatView::Parquet => Format::Parquet,
                FormatView::Csv => Format::Csv,
            },
        }
    }
}

/// One value of a Parameters node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParameterView {
    name: String,
    dtype: String,
    text: String,
}

/// Where a node's box stands and, once resized, its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameView {
    x: i32,
    y: i32,
    width: Option<u32>,
    height: Option<u32>,
}

impl FrameView {
    const fn of(frame: Frame) -> Self {
        Self {
            x: frame.place.x,
            y: frame.place.y,
            width: match frame.size {
                Some(size) => Some(size.width),
                None => None,
            },
            height: match frame.size {
                Some(size) => Some(size.height),
                None => None,
            },
        }
    }

    /// The frame this asks for.
    ///
    /// # Errors
    ///
    /// When it gives only one of a width and a height: a size has both, or neither while the box
    /// takes the size its content needs.
    fn frame(self) -> Result<Frame, ErrorView> {
        let size = match (self.width, self.height) {
            (Some(width), Some(height)) => Some(Size { width, height }),
            (None, None) => None,
            _ => {
                return Err(ErrorView::new(
                    "placing the box: a size gives both its width and its height",
                ));
            }
        };
        Ok(Frame {
            place: Place {
                x: self.x,
                y: self.y,
            },
            size,
        })
    }
}

/// One node as the canvas draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeBoxView {
    id: String,
    name: String,
    kind: KindView,
    /// What the node is set to, in a few words.
    summary: String,
    slots: Vec<SlotStateView>,
    /// For a transform, what may feed a new input; nothing for other kinds.
    new_input: Option<Vec<FitView>>,
    /// The columns of the table the node gives or would write, when they are known.
    columns: Option<Vec<FieldView>>,
    /// A Parameters node's values; none for other kinds.
    parameters: Vec<ParameterView>,
    file: Option<FileView>,
    /// What keeps the node from being worked out, in words.
    problems: Vec<String>,
    frame: Option<FrameView>,
}

/// The graph as the canvas draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphView {
    revision: u64,
    layout_revision: u64,
    /// Every node, each after the nodes it reads.
    nodes: Vec<NodeBoxView>,
    /// The dtypes a parameter may have.
    parameter_dtypes: Vec<&'static str>,
}

fn file_name(file: Option<&File>) -> String {
    file.map_or_else(
        || "No file chosen".to_owned(),
        |file| {
            Path::new(&file.path).file_name().map_or_else(
                || file.path.clone(),
                |name| name.to_string_lossy().into_owned(),
            )
        },
    )
}

fn count(number: usize, one: &str, many: &str) -> String {
    if number == 1 {
        format!("1 {one}")
    } else {
        format!("{number} {many}")
    }
}

fn summary(session: &Session, id: &NodeId, body: &Body) -> String {
    match body {
        Body::Source { file } | Body::Output { file, .. } => file_name(file.as_ref()),
        Body::Transform { .. } => session
            .document(id)
            .map_or_else(String::new, |(document, _)| {
                count(document.steps().len(), "step", "steps")
            }),
        Body::Parameters { values } => count(values.len(), "value", "values"),
    }
}

fn problem_text(problem: &Problem, session: &Session) -> String {
    let named = |node: &NodeId| {
        session
            .graph()
            .node(node)
            .map_or_else(|| node.to_string(), |read| read.name().to_string())
    };
    match problem {
        Problem::NothingGiven { from, .. } => format!("{} gives nothing yet", named(from)),
        other => other.to_string(),
    }
}

fn graph_view_of(session: &Session, workings: &Workings) -> GraphView {
    let graph = session.graph();
    let nodes = graph
        .order()
        .into_iter()
        .filter_map(|id| graph.node(id).map(|node| (id, node)))
        .map(|(id, node)| {
            let worked = workings.node(id);
            let slots = node
                .slots()
                .into_iter()
                .map(|(slot, kind, from)| SlotStateView {
                    slot: SlotView::of(&slot),
                    takes: TakesView::of(kind),
                    from: from.map(ReferenceView::of),
                    fits: graph.fits(id, Some(kind)).iter().map(FitView::of).collect(),
                })
                .collect();
            let (parameters, file) = match node.body() {
                Body::Parameters { values } => (
                    values
                        .iter()
                        .map(|(name, value)| ParameterView {
                            name: name.to_string(),
                            dtype: value.dtype.clone(),
                            text: value.text.clone(),
                        })
                        .collect(),
                    None,
                ),
                Body::Source { file } | Body::Output { file, .. } => {
                    (Vec::new(), file.as_ref().map(FileView::of))
                }
                Body::Transform { .. } => (Vec::new(), None),
            };
            NodeBoxView {
                id: id.to_string(),
                name: node.name().to_string(),
                kind: KindView::of(node.kind()),
                summary: summary(session, id, node.body()),
                slots,
                new_input: (node.kind() == Kind::Transform)
                    .then(|| graph.fits(id, None).iter().map(FitView::of).collect()),
                columns: worked
                    .and_then(|worked| worked.columns.as_deref())
                    .map(FieldView::of_columns),
                parameters,
                file,
                problems: worked.map_or_else(Vec::new, |worked| {
                    worked
                        .problems
                        .iter()
                        .map(|problem| problem_text(problem, session))
                        .collect()
                }),
                frame: session.layout().frame(id).map(FrameView::of),
            }
        })
        .collect();
    GraphView {
        revision: session.graph_revision(),
        layout_revision: session.layout_revision(),
        nodes,
        parameter_dtypes: dageist_engine::PARAMETER_DTYPES.to_vec(),
    }
}

/// The graph the host holds, worked out, as the canvas draws it.
pub fn graph_view(workspace: &Workspace) -> GraphView {
    let session = workspace.snapshot();
    let workings = session.work_out();
    graph_view_of(&session, &workings)
}

// -- Edits ---------------------------------------------------------------------------------

fn unknown(error: impl std::fmt::Display) -> ErrorView {
    ErrorView::of(ErrorKind::UnknownName, error.to_string())
}

/// `text` read as a node id.
pub(crate) fn known_id(text: &str) -> Result<NodeId, ErrorView> {
    NodeId::new(text).map_err(unknown)
}

fn input_name(text: &str) -> Result<InputName, ErrorView> {
    InputName::new(text).map_err(unknown)
}

/// One edit of the graph, as the view sends it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "edit", rename_all = "camelCase", deny_unknown_fields)]
pub enum GraphEditRequest {
    /// Adds a node of `kind`, reading the table of `follows` when it is given, named `name` or
    /// after its kind. The host mints its id.
    Add {
        kind: KindView,
        follows: Option<String>,
        #[serde(default)]
        name: Option<String>,
    },
    /// Removes a node and clears every reference to it.
    Remove { node: String },
    /// Gives a node another name.
    Rename { node: String, name: String },
    /// Sets or clears the file of a source or an output.
    SetFile {
        node: String,
        file: Option<FileView>,
    },
    /// Fills a slot.
    Connect {
        node: String,
        slot: SlotView,
        from: ReferenceView,
    },
    /// Adds an input to a transform, named after what feeds it, and fills it.
    ConnectNew { node: String, from: ReferenceView },
    /// Empties a slot.
    Disconnect { node: String, slot: SlotView },
    /// Adds a named input to a transform, not connected.
    AddInput {
        node: String,
        name: String,
        takes: TakesView,
    },
    /// Removes a named input from a transform.
    RemoveInput { node: String, name: String },
    /// Sets one value of a Parameters node, adding it when it is new.
    SetParameter {
        node: String,
        name: String,
        dtype: String,
        text: String,
    },
    /// Removes one value of a Parameters node.
    RemoveParameter { node: String, name: String },
}

impl GraphEditRequest {
    /// The edit this request asks for.
    ///
    /// # Errors
    ///
    /// When it names a node id, an input or a parameter that is not a name of its kind. A name
    /// the graph lacks is the graph's refusal, not an error.
    fn edit(&self) -> Result<Edit, ErrorView> {
        Ok(match self {
            Self::Add {
                kind,
                follows,
                name,
            } => Edit::Add {
                id: NodeId::mint(),
                kind: kind.kind(),
                follows: follows.as_deref().map(known_id).transpose()?,
                name: name
                    .as_deref()
                    .map(Name::new)
                    .transpose()
                    .map_err(|error| ErrorView::new(error.to_string()))?,
            },
            Self::Remove { node } => Edit::Remove {
                node: known_id(node)?,
            },
            Self::Rename { node, name } => Edit::Rename {
                node: known_id(node)?,
                name: Name::new(name).map_err(|error| ErrorView::new(error.to_string()))?,
            },
            Self::SetFile { node, file } => Edit::SetFile {
                node: known_id(node)?,
                file: file.as_ref().map(FileView::file),
            },
            Self::Connect { node, slot, from } => Edit::Connect {
                node: known_id(node)?,
                slot: slot.slot()?,
                from: from.reference()?,
            },
            Self::ConnectNew { node, from } => Edit::ConnectNew {
                node: known_id(node)?,
                from: from.reference()?,
            },
            Self::Disconnect { node, slot } => Edit::Disconnect {
                node: known_id(node)?,
                slot: slot.slot()?,
            },
            Self::AddInput { node, name, takes } => Edit::AddInput {
                node: known_id(node)?,
                name: input_name(name)?,
                kind: takes.kind(),
            },
            Self::RemoveInput { node, name } => Edit::RemoveInput {
                node: known_id(node)?,
                name: input_name(name)?,
            },
            Self::SetParameter {
                node,
                name,
                dtype,
                text,
            } => Edit::SetParameter {
                node: known_id(node)?,
                name: ParameterName::new(name.as_str()).map_err(unknown)?,
                parameter: Parameter {
                    dtype: dtype.clone(),
                    text: text.clone(),
                },
            },
            Self::RemoveParameter { node, name } => Edit::RemoveParameter {
                node: known_id(node)?,
                name: ParameterName::new(name.as_str()).map_err(unknown)?,
            },
        })
    }
}

/// What an edit of the graph came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GraphEditAnswerView {
    /// The edit was made: the graph at its new revision, and the node it added, if any.
    Edited {
        graph: GraphView,
        added: Option<String>,
    },
    /// The graph refused the edit and is still at `revision`.
    Refused { revision: u64, message: String },
}

fn stale(stale: dageist::Stale, what: &str) -> ErrorView {
    ErrorView::of(ErrorKind::Stale, format!("the {what} {stale}"))
}

/// Makes `request` on the graph at `revision` and answers with the edited graph, or why the
/// graph refused the edit.
///
/// # Errors
///
/// When the graph is at another revision (`stale`), or the request names something that is not
/// a name of its kind (`unknownName`).
pub fn edit_graph_view(
    workspace: &Workspace,
    revision: u64,
    request: &GraphEditRequest,
) -> Result<GraphEditAnswerView, ErrorView> {
    let edit = request.edit()?;
    // The edit is made under the lock, and the edited graph worked out after it is let go.
    let (made, session) = {
        let mut session = workspace.lock();
        let before = session.graph().clone();
        let made = session
            .edit_graph(revision, &edit)
            .map_err(|error| stale(error, "graph"))?;
        if made.is_ok() {
            let message = dageist::graph_message(&edit, &before, session.graph());
            workspace.save(&session, &message)?;
        }
        (made, session.clone())
    };
    match made {
        Ok(added) => {
            let workings = session.work_out();
            Ok(GraphEditAnswerView::Edited {
                graph: graph_view_of(&session, &workings),
                added: added.map(|node| node.to_string()),
            })
        }
        Err(refused) => Ok(GraphEditAnswerView::Refused {
            revision: session.graph_revision(),
            message: refused.to_string(),
        }),
    }
}

/// The revision the layout is at after a box was placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedView {
    layout_revision: u64,
}

/// Sets where the box of `node` stands, on the layout at `revision`.
///
/// # Errors
///
/// When the layout is at another revision (`stale`), or the graph has no such node
/// (`unknownName`).
pub fn place_view(
    workspace: &Workspace,
    revision: u64,
    node: &str,
    frame: FrameView,
) -> Result<PlacedView, ErrorView> {
    let node = known_id(node)?;
    let frame = frame.frame()?;
    let mut session = workspace.lock();
    session
        .place(revision, &node, frame)
        .map_err(|error| stale(error, "layout"))?
        .map_err(unknown)?;
    workspace.save(&session, &dageist::place_message(session.graph(), &node))?;
    Ok(PlacedView {
        layout_revision: session.layout_revision(),
    })
}

/// Opens the workspace on disk in `folder` in place of the one the host holds, and reports its
/// graph, worked out. Its revisions start where no view of the workspace let go can be, so an edit
/// sent from such a view is stale.
///
/// # Errors
///
/// When `folder` holds no workspace this application reads.
pub fn open_workspace_view(workspace: &Workspace, folder: &str) -> Result<GraphView, ErrorView> {
    workspace.open(
        Path::new(folder),
        crate::document::first_revision(std::time::SystemTime::now()),
    )?;
    Ok(graph_view(workspace))
}

// -- Tauri commands ------------------------------------------------------------------------

/// Reports the graph, worked out.
#[tauri::command(async)]
pub fn graph(workspace: State<'_, Workspace>) -> GraphView {
    graph_view(&workspace)
}

/// Makes `edit` on the graph, which must be at `revision`.
#[tauri::command(async)]
pub fn edit_graph(
    workspace: State<'_, Workspace>,
    revision: u64,
    edit: GraphEditRequest,
) -> Result<GraphEditAnswerView, ErrorView> {
    edit_graph_view(&workspace, revision, &edit)
}

/// Places the box of `node` on the layout, which must be at `revision`.
#[tauri::command(async)]
pub fn place_node(
    workspace: State<'_, Workspace>,
    revision: u64,
    node: String,
    frame: FrameView,
) -> Result<PlacedView, ErrorView> {
    place_view(&workspace, revision, &node, frame)
}

/// Opens the workspace on disk in `folder` in place of the one held.
#[tauri::command(async)]
pub fn open_workspace(
    workspace: State<'_, Workspace>,
    folder: String,
) -> Result<GraphView, ErrorView> {
    open_workspace_view(&workspace, &folder)
}

#[cfg(test)]
pub(crate) mod tests;
