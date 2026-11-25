//! Commands that report a transform of the graph: what is connected to it, the document it holds
//! with what running it reports, the edits of that document, and the rows after one of its steps.
//!
//! Each command names the transform by its id. Its document and revision are the core's
//! session's, held in the host's [`Workspace`]; what its steps run on is what working the graph
//! out binds to its inputs, and its working rows are the first rows of the table its steps start
//! from. The commands are thin: the functions beside them answer, so that a host other than
//! Tauri can serve the same views. The shapes mirror `NodeView`, `DocumentStateView`, `EditAnswerView` and
//! `RowsView` in `ui/packages/host/src/contract.ts`.

use dageist::{Bindings, Session};
use dageist_engine::document::{Document, InputKind, Inputs, Origin, RowsError, StepId};
use dageist_graph::NodeId;
use serde::Serialize;
use tauri::State;

use crate::document::{
    DocumentStateView, EditAnswerView, EditRequest, document_error, document_state_view, edit_view,
};
use crate::graph::{Workspace, known_id};
use crate::report::{FieldView, RunView, ValuesView};
use crate::sample_table::{ErrorKind, ErrorView};

/// The most rows a preview may hold. A preview is what a step list shows
/// beside its steps, so it stays a window.
const MAX_PREVIEW_ROWS: usize = 1_000;

/// The most rows of the table a transform's steps start from that its steps are counted and
/// previewed on.
const WORKING_ROWS: usize = 10_000;

/// What supplies an input to the node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OriginView {
    /// The table a node gives, by the node's name.
    Node { name: String },
    /// One column of the table a node gives.
    Column { node: String, column: String },
    /// A value set on the graph itself, which no node gives.
    Parameter,
}

fn origin_view(origin: &Origin) -> OriginView {
    match origin {
        Origin::Node(name) => OriginView::Node { name: name.clone() },
        Origin::Column { node, column } => OriginView::Column {
            node: node.clone(),
            column: column.clone(),
        },
        Origin::Parameter => OriginView::Parameter,
    }
}

/// The table the steps start from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceView {
    name: String,
    from: OriginView,
    columns: Vec<FieldView>,
}

/// One input of the node: a table, a single value or a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InputView {
    /// A connected table, with its columns.
    Table {
        name: String,
        from: OriginView,
        columns: Vec<FieldView>,
    },
    /// A connected single value, as Polars writes it, or `null` where it is
    /// null.
    Value {
        name: String,
        from: OriginView,
        dtype: String,
        value: Option<String>,
    },
    /// A connected list of values of one dtype, with its first values.
    List {
        name: String,
        from: OriginView,
        dtype: String,
        values: ValuesView,
    },
}

/// What the node is connected to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeView {
    /// The table the steps start from.
    source: SourceView,
    /// Every other input, in the order of their names.
    inputs: Vec<InputView>,
}

/// The rows after a step, every cell as text and a null as `null`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowsView {
    columns: Vec<FieldView>,
    rows: Vec<Vec<Option<String>>>,
}

fn failed(what: &str, error: impl std::fmt::Display) -> ErrorView {
    ErrorView::new(format!("{what}: {error}"))
}

/// What the transform `node`'s steps run on, as working the graph out binds it.
///
/// # Errors
///
/// When the graph has no such transform, or the table its steps start from is not connected or
/// gives nothing.
pub(crate) fn bindings_of(session: &Session, node: &NodeId) -> Result<Bindings, ErrorView> {
    if session.document(node).is_none() {
        return Err(document_error(dageist::DocumentError::NoSuchTransform(
            node.clone(),
        )));
    }
    let mut workings = session.work_out();
    let worked = workings.take(node).unwrap_or_default();
    worked.bindings.ok_or_else(|| {
        let why = worked
            .problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        ErrorView::new(format!("the steps have no table to start from: {why}"))
    })
}

/// What the transform `node` is connected to: the table its steps start from, and every other
/// input with what supplies it and its columns, its value or its first values.
///
/// # Errors
///
/// When the graph has no such transform, or its steps have no table to start from.
pub fn node_view(workspace: &Workspace, node: &str) -> Result<NodeView, ErrorView> {
    let node = known_id(node)?;
    let bindings = bindings_of(&workspace.lock(), &node)?;
    let columns = bindings.start.columns().map_err(ErrorView::from)?;
    Ok(NodeView {
        source: SourceView {
            // The table the steps start from is called as an input would be.
            name: dageist_graph::InputName::made_from(&bindings.start_name).to_string(),
            from: origin_view(&Origin::Node(bindings.start_name.clone())),
            columns: FieldView::of_columns(&columns),
        },
        inputs: inputs_view(&bindings.inputs)?,
    })
}

/// What `inputs` hold, each with what supplies it and its columns, its value or its first values.
pub(crate) fn inputs_view(inputs: &Inputs) -> Result<Vec<InputView>, ErrorView> {
    Ok(inputs
        .describe()
        .map_err(|error| failed("describing the inputs", error))?
        .into_iter()
        .map(|input| {
            let name = input.name().to_owned();
            let from = origin_view(input.origin());
            match input.kind() {
                InputKind::Table(columns) => InputView::Table {
                    name,
                    from,
                    columns: FieldView::of_columns(columns),
                },
                InputKind::Value { dtype, value } => InputView::Value {
                    name,
                    from,
                    dtype: dtype.clone(),
                    value: value.clone(),
                },
                InputKind::List { dtype, values } => InputView::List {
                    name,
                    from,
                    dtype: dtype.clone(),
                    values: ValuesView::of(values),
                },
            }
        })
        .collect())
}

/// Runs `document` on its working rows with `bindings`, and reports each step.
///
/// # Errors
///
/// When the working rows cannot be read or Polars cannot run the document.
pub fn run_view(document: &Document, bindings: &Bindings) -> Result<RunView, ErrorView> {
    let working = bindings
        .start
        .working(WORKING_ROWS)
        .map_err(ErrorView::from)?;
    let report = document
        .run_on(&bindings.inputs, &working)
        .map_err(|error| failed("running the document", error))?;
    Ok(RunView::of_report(&report))
}

/// The rows after the step `step` of the document of the transform `node` at `revision`, at
/// most `limit` of them.
///
/// A failure says which kind it is: a `limit` over what a preview holds, a document at another
/// revision that is `stale`, a step the document does not have, a step that was `refused`, a step
/// that was `notRun` because one before it was refused, or otherwise `failed`.
pub fn rows_view(
    workspace: &Workspace,
    node: &str,
    revision: u64,
    step: &str,
    limit: usize,
) -> Result<RowsView, ErrorView> {
    if limit > MAX_PREVIEW_ROWS {
        return Err(ErrorView::of(
            ErrorKind::Limit,
            format!("a preview holds at most {MAX_PREVIEW_ROWS} rows"),
        ));
    }
    let node = known_id(node)?;
    let session = workspace.lock();
    let (document, current) = session
        .document(&node)
        .ok_or_else(|| document_error(dageist::DocumentError::NoSuchTransform(node.clone())))?;
    if current != revision {
        return Err(document_error(dageist::DocumentError::Stale(
            dageist::Stale {
                asked: revision,
                current,
            },
        )));
    }
    let bindings = bindings_of(&session, &node)?;
    rows_of(document, &bindings, step, limit)
}

/// The rows after the step `step` of `document` run with `bindings`, at most `limit` of them.
fn rows_of(
    document: &Document,
    bindings: &Bindings,
    step: &str,
    limit: usize,
) -> Result<RowsView, ErrorView> {
    let step = StepId::new(step).map_err(|error| {
        ErrorView::of(ErrorKind::NoSuchStep, format!("naming the step: {error}"))
    })?;
    let working = bindings
        .start
        .working(WORKING_ROWS)
        .map_err(ErrorView::from)?;
    let preview = document
        .preview(&bindings.inputs, &working, &step, limit)
        .map_err(|error| {
            let kind = match &error {
                RowsError::NoSuchStep(_) => ErrorKind::NoSuchStep,
                RowsError::Refused(refusal) if refusal.place().step_id() == Some(&step) => {
                    ErrorKind::Refused
                }
                RowsError::Refused(_) => ErrorKind::NotRun,
                RowsError::Incomplete(place) if place.step_id() == Some(&step) => {
                    ErrorKind::Refused
                }
                RowsError::Incomplete(_) => ErrorKind::NotRun,
                RowsError::Polars(_) => ErrorKind::Failed,
            };
            ErrorView::of(kind, format!("previewing the rows: {error}"))
        })?;
    Ok(RowsView {
        columns: FieldView::of_columns(preview.columns()),
        rows: preview.rows().to_vec(),
    })
}

/// Reports what the transform `node` is connected to.
#[tauri::command(async)]
pub fn node_inputs(workspace: State<'_, Workspace>, node: String) -> Result<NodeView, ErrorView> {
    node_view(&workspace, &node)
}

/// Reports the document of the transform `node`, the revision it is at, and what running it
/// reported.
#[tauri::command(async)]
pub fn node_document(
    workspace: State<'_, Workspace>,
    node: String,
) -> Result<DocumentStateView, ErrorView> {
    document_state_view(&workspace, &node)
}

/// Makes the edit `edit` on the document of the transform `node`, which must be at `revision`,
/// and reports the edited document with its new revision and what running it reported, or why
/// the engine refused the edit.
#[tauri::command(async)]
pub fn edit_document(
    workspace: State<'_, Workspace>,
    node: String,
    revision: u64,
    edit: EditRequest,
) -> Result<EditAnswerView, ErrorView> {
    edit_view(&workspace, &node, revision, &edit)
}

/// Gives the rows after the step `step` of the document of the transform `node` at `revision`,
/// at most `limit` of them, which is at most `MAX_PREVIEW_ROWS`.
#[tauri::command(async)]
pub fn rows_after_step(
    workspace: State<'_, Workspace>,
    node: String,
    revision: u64,
    step: String,
    limit: usize,
) -> Result<RowsView, ErrorView> {
    rows_view(&workspace, &node, revision, &step, limit)
}

#[cfg(test)]
pub(crate) mod tests;
