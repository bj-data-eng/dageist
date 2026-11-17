//! Commands that report a node: what is connected to it, what running a step
//! document on it reports, and the rows after one of its steps.
//!
//! The node is the engine's claim-timing sample until the graph supplies real
//! ones. Every command works from the document text the view sends, builds
//! the node it runs on and keeps nothing between calls. The commands are thin:
//! the functions beside them answer, so that a host other than Tauri can
//! serve the same views. The shapes mirror `NodeView`, `RunView` and
//! `RowsView` in `ui/packages/host/src/contract.ts`.

use dageist_engine::claim_timing::ClaimTiming;
use dageist_engine::document::{Document, InputKind, StepId};
use serde::Serialize;

use crate::report::{FieldView, RunView};
use crate::sample_table::ErrorView;

/// The most rows a preview may hold. A preview is what a step list shows
/// beside its steps, so it stays a window.
const MAX_PREVIEW_ROWS: usize = 1_000;

/// The table the steps start from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceView {
    name: &'static str,
    columns: Vec<FieldView>,
}

/// One input of the node: a table, a single value or a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InputView {
    /// A connected table, with its columns.
    Table {
        name: String,
        columns: Vec<FieldView>,
    },
    /// A connected single value.
    Value { name: String, dtype: String },
    /// A connected list of values of one dtype.
    List { name: String, dtype: String },
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
/// other input with its columns or its dtype.
pub fn node_view() -> Result<NodeView, ErrorView> {
    let node = node()?;
    let inputs = node
        .inputs()
        .describe()
        .map_err(|error| failed("describing the inputs", error))?
        .into_iter()
        .map(|input| {
            let name = input.name().to_owned();
            match input.kind() {
                InputKind::Table(columns) => InputView::Table {
                    name,
                    columns: FieldView::of_columns(columns),
                },
                InputKind::Value(dtype) => InputView::Value {
                    name,
                    dtype: dtype.clone(),
                },
                InputKind::List(dtype) => InputView::List {
                    name,
                    dtype: dtype.clone(),
                },
            }
        })
        .collect();
    Ok(NodeView {
        source: SourceView {
            name: ClaimTiming::SOURCE_NAME,
            columns: FieldView::of_columns(&node.source_columns()),
        },
        inputs,
    })
}

/// Reads `document` and runs it on the node, reporting each step. A text
/// that is not a document the engine reads is reported as such, not as a
/// failure of the command.
pub fn run_view(document: &str) -> Result<RunView, ErrorView> {
    let node = node()?;
    let document = match Document::read(document) {
        Ok(document) => document,
        Err(error) => return Ok(RunView::unreadable(&error)),
    };
    let report = node
        .run(&document)
        .map_err(|error| failed("running the document", error))?;
    Ok(RunView::of_report(&report))
}

/// The rows after the step `step` of `document`, at most `limit` of them.
pub fn rows_view(document: &str, step: &str, limit: usize) -> Result<RowsView, ErrorView> {
    if limit > MAX_PREVIEW_ROWS {
        return Err(ErrorView::new(format!(
            "a preview holds at most {MAX_PREVIEW_ROWS} rows"
        )));
    }
    let document =
        Document::read(document).map_err(|error| failed("reading the document", error))?;
    let step = StepId::new(step).map_err(|error| failed("naming the step", error))?;
    let preview = node()?
        .preview(&document, &step, limit)
        .map_err(|error| failed("previewing the rows", error))?;
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

/// Runs the document `document`, a step document's JSON text, on the node and
/// reports each of its steps.
#[tauri::command(async)]
pub fn run_document(document: String) -> Result<RunView, ErrorView> {
    run_view(&document)
}

/// Gives the rows after the step `step` of `document`, at most `limit` of
/// them, which is at most `MAX_PREVIEW_ROWS`.
#[tauri::command(async)]
pub fn rows_after_step(
    document: String,
    step: String,
    limit: usize,
) -> Result<RowsView, ErrorView> {
    rows_view(&document, &step, limit)
}

#[cfg(test)]
mod tests;
