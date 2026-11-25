//! Commands that report a node: what is connected to it, the document it holds with what running
//! it reports, the edits of that document, and the rows after one of its steps.
//!
//! The node is the engine's claim-timing sample until the graph supplies real
//! ones. The document and its revision are the host's, kept in [`Documents`]; each command builds
//! the node it runs on and keeps nothing else between calls. The commands are thin:
//! the functions beside them answer, so that a host other than Tauri can
//! serve the same views. The shapes mirror `NodeView`, `DocumentStateView`, `EditAnswerView` and
//! `RowsView` in `ui/packages/host/src/contract.ts`.

use dageist_engine::claim_timing::ClaimTiming;
use dageist_engine::document::{Document, InputKind, Inputs, Origin, RowsError, StepId};
use serde::Serialize;
use tauri::State;

use crate::document::{
    DocumentStateView, Documents, EditAnswerView, EditRequest, document_state_view, edit_view,
};
use crate::report::{FieldView, RunView, ValuesView};
use crate::sample_table::{ErrorKind, ErrorView};

/// The most rows a preview may hold. A preview is what a step list shows
/// beside its steps, so it stays a window.
const MAX_PREVIEW_ROWS: usize = 1_000;

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
    name: &'static str,
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

fn node() -> Result<ClaimTiming, ErrorView> {
    ClaimTiming::new().map_err(ErrorView::from)
}

/// What the node is connected to: the table its steps start from, and every
/// other input with what supplies it and its columns, its value or its first
/// values.
pub fn node_view() -> Result<NodeView, ErrorView> {
    let node = node()?;
    Ok(NodeView {
        source: SourceView {
            name: ClaimTiming::SOURCE_NAME,
            from: origin_view(node.source_origin()),
            columns: FieldView::of_columns(&node.source_columns()),
        },
        inputs: inputs_view(node.inputs())?,
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

/// Runs `document` on the node and reports each step.
///
/// # Errors
///
/// When the sample node cannot be made or Polars cannot run the document.
pub fn run_view(document: &Document) -> Result<RunView, ErrorView> {
    let node = node()?;
    let report = node
        .run(document)
        .map_err(|error| failed("running the document", error))?;
    Ok(RunView::of_report(&report))
}

/// The rows after the step `step` of the document the host holds at `revision`, at most `limit`
/// of them.
///
/// A failure says which kind it is: a `limit` over what a preview holds, a document at another
/// revision that is `stale`, a step the document does not have, a step that was `refused`, a step
/// that was `notRun` because one before it was refused, or otherwise `failed`.
pub fn rows_view(
    documents: &Documents,
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
    rows_of(&documents.at(revision)?, step, limit)
}

/// The rows after the step `step` of `document`, at most `limit` of them.
fn rows_of(document: &Document, step: &str, limit: usize) -> Result<RowsView, ErrorView> {
    let step = StepId::new(step).map_err(|error| {
        ErrorView::of(ErrorKind::NoSuchStep, format!("naming the step: {error}"))
    })?;
    let preview = node()?.preview(document, &step, limit).map_err(|error| {
        let kind = match &error {
            RowsError::NoSuchStep(_) => ErrorKind::NoSuchStep,
            RowsError::Refused(refusal) if refusal.place().step_id() == Some(&step) => {
                ErrorKind::Refused
            }
            RowsError::Refused(_) => ErrorKind::NotRun,
            RowsError::Incomplete(place) if place.step_id() == Some(&step) => ErrorKind::Refused,
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

/// Reports what the node is connected to.
#[tauri::command(async)]
pub fn node_inputs() -> Result<NodeView, ErrorView> {
    node_view()
}

/// Reports the node's document, the revision it is at, and what running it reported.
#[tauri::command(async)]
pub fn node_document(documents: State<'_, Documents>) -> Result<DocumentStateView, ErrorView> {
    document_state_view(&documents)
}

/// Makes the edit `edit` on the node's document, which must be at `revision`, and reports the
/// edited document with its new revision and what running it reported, or why the engine
/// refused the edit.
#[tauri::command(async)]
pub fn edit_document(
    documents: State<'_, Documents>,
    revision: u64,
    edit: EditRequest,
) -> Result<EditAnswerView, ErrorView> {
    edit_view(&documents, revision, &edit)
}

/// Gives the rows after the step `step` of the node's document at `revision`, at most `limit` of
/// them, which is at most `MAX_PREVIEW_ROWS`.
#[tauri::command(async)]
pub fn rows_after_step(
    documents: State<'_, Documents>,
    revision: u64,
    step: String,
    limit: usize,
) -> Result<RowsView, ErrorView> {
    rows_view(&documents, revision, &step, limit)
}

#[cfg(test)]
pub(crate) mod tests;
