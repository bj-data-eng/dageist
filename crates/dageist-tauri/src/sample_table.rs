//! Commands that open the sample table and serve blocks of it to the view.
//!
//! The engine owns the table. The view can look at it two ways: as a working
//! window of whole keys held in memory, or as the stored result, read from
//! its Parquet files one block at a time. These commands hold whichever is
//! open between calls, report its shape as a serializable view and pass
//! blocks through as raw Arrow IPC bytes, which the view decodes itself.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use dageist_engine::{
    ColumnInfo, EngineError, ParquetTable, SampleSpec, Table, claims_generated, generate_claims,
};
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, Runtime, State};

/// The sample generated on first use: about two million rows of 300 columns,
/// roughly 2 GB of Parquet.
const SAMPLE: SampleSpec = SampleSpec {
    keys: 400_000,
    filler_columns: 292,
};
/// The column whose values are kept whole in a working window.
const KEY_COLUMN: &str = "dcn";

/// The table the view is looking at.
#[derive(Debug)]
enum Opened {
    /// Whole keys held in memory.
    Window(Table),
    /// The stored result, read from its Parquet files on demand.
    Stored(ParquetTable),
}

impl Opened {
    fn view(&self) -> TableView {
        match self {
            Self::Window(table) => TableView::new(table.height(), &table.columns()),
            Self::Stored(table) => TableView::new(table.height(), table.columns()),
        }
    }

    fn block_ipc(
        &self,
        offset: usize,
        length: usize,
        columns: &[String],
    ) -> Result<Vec<u8>, EngineError> {
        match self {
            Self::Window(table) => table.block_ipc(offset, length, columns),
            Self::Stored(table) => table.block_ipc(offset, length, columns),
        }
    }
}

/// The table the view has open, if any.
#[derive(Debug, Default)]
pub struct OpenTable {
    table: Mutex<Option<Arc<Opened>>>,
}

impl OpenTable {
    fn replace(&self, table: Arc<Opened>) {
        *self.table.lock().unwrap_or_else(PoisonError::into_inner) = Some(table);
    }

    fn current(&self) -> Option<Arc<Opened>> {
        self.table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// One column of the open table, as presented to the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnView {
    name: String,
    data_type: String,
}

/// The shape of the open table, as presented to the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableView {
    rows: usize,
    columns: Vec<ColumnView>,
}

impl TableView {
    fn new(rows: usize, columns: &[ColumnInfo]) -> Self {
        Self {
            rows,
            columns: columns
                .iter()
                .map(|column| ColumnView {
                    name: column.name.clone(),
                    data_type: column.data_type.clone(),
                })
                .collect(),
        }
    }
}

/// A failed command, as presented to the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorView {
    message: String,
}

impl ErrorView {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<EngineError> for ErrorView {
    fn from(error: EngineError) -> Self {
        Self::new(error.to_string())
    }
}

/// Generates the sample into `directory` unless it is already there.
fn ensure_sample(directory: &Path, spec: SampleSpec) -> Result<(), EngineError> {
    if !claims_generated(directory) {
        generate_claims(directory, spec)?;
    }
    Ok(())
}

/// Loads a working window of `keys` whole keys from the sample in `directory`.
fn open_window(directory: &Path, spec: SampleSpec, keys: u32) -> Result<Opened, EngineError> {
    ensure_sample(directory, spec)?;
    Table::load_keys(directory, KEY_COLUMN, keys).map(Opened::Window)
}

/// Opens the whole sample in `directory` where it is stored.
fn open_stored(directory: &Path, spec: SampleSpec) -> Result<Opened, EngineError> {
    ensure_sample(directory, spec)?;
    ParquetTable::open(directory).map(Opened::Stored)
}

/// Returns the folder in the application cache that holds the sample. Its
/// name carries the sample's size, so a different size gets its own folder.
fn sample_directory<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, ErrorView> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|error| ErrorView::new(format!("finding the application cache: {error}")))?;
    let columns = SampleSpec::CLAIM_COLUMNS.len() + usize::from(SAMPLE.filler_columns);
    Ok(cache.join(format!("claims-{}-keys-{columns}-columns", SAMPLE.keys)))
}

fn show(open: &OpenTable, table: Opened) -> TableView {
    let view = table.view();
    open.replace(Arc::new(table));
    view
}

/// Opens a working window of `keys` whole keys of the sample table, held in
/// memory, and reports its shape.
///
/// The first call generates the sample into the application cache.
#[tauri::command(async)]
pub fn open_sample_window<R: Runtime>(
    app: AppHandle<R>,
    open: State<'_, OpenTable>,
    keys: u32,
) -> Result<TableView, ErrorView> {
    let table = open_window(&sample_directory(&app)?, SAMPLE, keys)?;
    Ok(show(&open, table))
}

/// Opens the whole sample table where it is stored, without loading it, and
/// reports its shape.
///
/// The first call generates the sample into the application cache.
#[tauri::command(async)]
pub fn open_sample_result<R: Runtime>(
    app: AppHandle<R>,
    open: State<'_, OpenTable>,
) -> Result<TableView, ErrorView> {
    let table = open_stored(&sample_directory(&app)?, SAMPLE)?;
    Ok(show(&open, table))
}

/// Returns `length` rows of the open table starting at `offset`, limited to
/// `columns`, as the raw bytes of an Arrow IPC stream.
#[tauri::command(async)]
pub fn table_block(
    open: State<'_, OpenTable>,
    offset: usize,
    length: usize,
    columns: Vec<String>,
) -> Result<Response, ErrorView> {
    let table = open
        .current()
        .ok_or_else(|| ErrorView::new("no table is open"))?;
    Ok(Response::new(table.block_ipc(offset, length, &columns)?))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    use dageist_engine::SampleSpec;
    use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
    use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
    use tauri::webview::InvokeRequest;
    use tauri::{WebviewWindow, WebviewWindowBuilder};

    use super::{OpenTable, Opened, open_stored, open_window};

    const SMALL: SampleSpec = SampleSpec {
        keys: 40,
        filler_columns: 4,
    };
    const SMALL_COLUMNS: usize = SampleSpec::CLAIM_COLUMNS.len() + 4;

    struct SampleDirectory {
        path: PathBuf,
    }

    impl SampleDirectory {
        fn empty(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("dageist-tauri-{}-{name}", std::process::id()));
            Self { path }
        }
    }

    impl Drop for SampleDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn rows_and_columns(table: &Opened) -> (u64, Vec<serde_json::Value>) {
        let view = serde_json::to_value(table.view()).expect("the view serializes");
        (
            view["rows"].as_u64().expect("a row count"),
            view["columns"].as_array().expect("a column list").clone(),
        )
    }

    fn webview(table: Option<Opened>) -> WebviewWindow<MockRuntime> {
        let open = OpenTable::default();
        if let Some(table) = table {
            open.replace(Arc::new(table));
        }
        let app = mock_builder()
            .manage(open)
            .invoke_handler(tauri::generate_handler![super::table_block])
            .build(mock_context(noop_assets()))
            .expect("the mock application builds");
        WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
            .build()
            .expect("the mock window builds")
    }

    fn block_request(arguments: serde_json::Value) -> InvokeRequest {
        InvokeRequest {
            cmd: "table_block".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("a valid origin"),
            body: InvokeBody::Json(arguments),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: INVOKE_KEY.to_owned(),
        }
    }

    #[test]
    fn empty_directory_gets_a_sample_and_a_window_of_whole_keys() {
        let sample = SampleDirectory::empty("generate");

        let table = open_window(&sample.path, SMALL, 10).expect("the window opens");

        let (rows, columns) = rows_and_columns(&table);
        assert_eq!(columns.len(), SMALL_COLUMNS);
        assert_eq!(
            columns[0],
            serde_json::json!({ "name": "dcn", "dataType": "str" })
        );
        // Ten keys of two to eight step rows each.
        assert!((20..=80).contains(&rows), "{rows} rows for ten keys");
    }

    #[test]
    fn stored_result_reports_every_row_of_the_sample() {
        let sample = SampleDirectory::empty("stored");

        let table = open_stored(&sample.path, SMALL).expect("the result opens");

        let (rows, columns) = rows_and_columns(&table);
        assert_eq!(columns.len(), SMALL_COLUMNS);
        // Forty keys of two to eight step rows each.
        assert!((80..=320).contains(&rows), "{rows} rows for forty keys");
    }

    #[test]
    fn existing_sample_is_opened_without_being_rewritten() {
        let sample = SampleDirectory::empty("reuse");
        open_window(&sample.path, SMALL, 5).expect("the sample is generated");
        let file = sample.path.join("claims-0000.parquet");
        let written = fs::metadata(&file)
            .and_then(|metadata| metadata.modified())
            .expect("the sample file has a modification time");

        // A different size would produce different files if it were generated.
        let bigger = SampleSpec {
            keys: 400,
            filler_columns: 9,
        };
        let table = open_stored(&sample.path, bigger).expect("the sample reopens");

        let reopened = fs::metadata(&file)
            .and_then(|metadata| metadata.modified())
            .expect("the sample file still exists");
        assert_eq!(reopened, written);
        assert_eq!(rows_and_columns(&table).1.len(), SMALL_COLUMNS);
    }

    #[test]
    fn block_command_answers_with_the_raw_arrow_bytes_of_a_window() {
        let sample = SampleDirectory::empty("window-block");
        let table = open_window(&sample.path, SMALL, 10).expect("the window opens");
        let columns = ["dcn".to_owned(), "timestamp".to_owned()];
        let expected = table.block_ipc(3, 7, &columns).expect("the engine slices");

        let response = tauri::test::get_ipc_response(
            &webview(Some(table)),
            block_request(serde_json::json!({
                "offset": 3,
                "length": 7,
                "columns": columns,
            })),
        );

        match response {
            Ok(InvokeResponseBody::Raw(bytes)) => assert_eq!(bytes, expected),
            other => panic!("expected raw bytes, got {other:?}"),
        }
    }

    #[test]
    fn block_command_answers_with_the_raw_arrow_bytes_of_a_stored_result() {
        let sample = SampleDirectory::empty("stored-block");
        let table = open_stored(&sample.path, SMALL).expect("the result opens");
        let columns = ["dcn".to_owned(), "measure_002".to_owned()];
        let expected = table.block_ipc(50, 12, &columns).expect("the engine reads");

        let response = tauri::test::get_ipc_response(
            &webview(Some(table)),
            block_request(serde_json::json!({
                "offset": 50,
                "length": 12,
                "columns": columns,
            })),
        );

        match response {
            Ok(InvokeResponseBody::Raw(bytes)) => assert_eq!(bytes, expected),
            other => panic!("expected raw bytes, got {other:?}"),
        }
    }

    #[test]
    fn block_command_without_an_open_table_reports_it() {
        let response = tauri::test::get_ipc_response(
            &webview(None),
            block_request(serde_json::json!({ "offset": 0, "length": 5, "columns": [] })),
        );

        match response {
            Err(error) => assert_eq!(error, serde_json::json!({ "message": "no table is open" })),
            Ok(body) => panic!("expected an error, got {body:?}"),
        }
    }

    #[test]
    fn block_command_with_an_unknown_column_reports_it() {
        let sample = SampleDirectory::empty("unknown");
        let table = open_window(&sample.path, SMALL, 5).expect("the window opens");

        let response = tauri::test::get_ipc_response(
            &webview(Some(table)),
            block_request(serde_json::json!({
                "offset": 0,
                "length": 5,
                "columns": ["no_such_column"],
            })),
        );

        let error = response.expect_err("an unknown column is refused");
        let message = error["message"].as_str().expect("a message");
        assert!(
            message.starts_with("selecting block columns"),
            "unexpected message: {message}"
        );
    }
}
