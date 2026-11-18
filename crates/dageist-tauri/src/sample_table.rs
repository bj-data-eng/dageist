//! Commands that open the sample table and serve blocks of it to the view.
//!
//! The engine owns the table. The view can look at it two ways: as a working
//! window of whole keys held in memory, or as the stored result, read from
//! its Parquet files one block at a time. These commands hold whichever is
//! open between calls, report its shape as a serializable view and answer
//! blocks as the raw bytes of a frame: the number of each row and a report of
//! the block, written as text here, and then the block's Arrow IPC stream, which
//! the view reads. The frame is in
//! [the reference](../../../docs/reference.md#the-block-frame).
//!
//! The commands are thin: the functions beside them answer, so that a host
//! other than Tauri can serve the same views.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use dageist_engine::{
    Block, ColumnInfo, EngineError, ParquetTable, SampleSpec, Table, claims_generated,
    generate_claims,
};
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::block::frame;
use crate::figures::{elapsed, quantity};

/// The sample generated on first use: about two million rows of 300 columns,
/// roughly 2 GB of Parquet.
const SAMPLE: SampleSpec = SampleSpec {
    keys: 400_000,
    filler_columns: 292,
};
/// The column whose values are kept whole in a working window.
const KEY_COLUMN: &str = "dcn";
/// The most keys a working window may hold. A window lives in memory, so it
/// has to stay a sample.
const MAX_WINDOW_KEYS: u32 = 50_000;
/// The most rows one block may hold. A block is what is near the screen.
const MAX_BLOCK_ROWS: usize = 10_000;

/// The table the view is looking at.
#[derive(Debug)]
enum Opened {
    /// Whole keys held in memory.
    Window(Table),
    /// The stored result, read from its Parquet files on demand.
    Stored(ParquetTable),
}

impl Opened {
    fn height(&self) -> usize {
        match self {
            Self::Window(table) => table.height(),
            Self::Stored(table) => table.height(),
        }
    }

    fn columns(&self) -> &[ColumnInfo] {
        match self {
            Self::Window(table) => table.columns(),
            Self::Stored(table) => table.columns(),
        }
    }

    /// The table's shape, which it took `opened` to open.
    fn view(&self, opened: Duration) -> TableView {
        TableView::new(self.height(), self.columns(), opened)
    }

    fn block(
        &self,
        offset: usize,
        length: usize,
        columns: &[String],
    ) -> Result<Block, EngineError> {
        match self {
            Self::Window(table) => table.block(offset, length, columns),
            Self::Stored(table) => table.block(offset, length, columns),
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
    numeric: bool,
}

/// The figures of the open table, written as text for the view to show as they
/// are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FiguresView {
    rows: String,
    columns: String,
    opened: String,
}

/// The shape of the open table, as presented to the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableView {
    rows: usize,
    columns: Vec<ColumnView>,
    figures: FiguresView,
}

impl TableView {
    fn new(rows: usize, columns: &[ColumnInfo], opened: Duration) -> Self {
        Self {
            rows,
            columns: columns
                .iter()
                .map(|column| ColumnView {
                    name: column.name.clone(),
                    data_type: column.data_type.clone(),
                    numeric: column.numeric,
                })
                .collect(),
            figures: FiguresView {
                rows: quantity(rows, "row", "rows"),
                columns: quantity(columns.len(), "column", "columns"),
                opened: elapsed(opened),
            },
        }
    }
}

/// What a failed command ended in, a closed list the view decides on instead
/// of reading the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    /// The document has no step of the name asked for.
    NoSuchStep,
    /// A step before the one asked for was refused, so the one asked for was
    /// not run.
    NotRun,
    /// The step asked for was refused.
    Refused,
    /// More was asked for than the window of a command holds.
    Limit,
    /// The text sent is not a document the engine reads.
    Unreadable,
    /// The request names an entry or a parameter the catalogue lacks, or a way
    /// down to an expression that starts at no position of a step. A position
    /// or a dtype that the engine has no word for is not an argument at all, and
    /// Tauri and the bridge refuse it as one.
    UnknownName,
    /// Anything else: the command could not do what it was asked.
    Failed,
}

#[cfg(test)]
impl ErrorKind {
    /// Every kind, in the order the contract lists them, for the vocabulary
    /// snapshot.
    pub(crate) const ALL: [Self; 7] = [
        Self::NoSuchStep,
        Self::NotRun,
        Self::Refused,
        Self::Limit,
        Self::Unreadable,
        Self::UnknownName,
        Self::Failed,
    ];
}

/// A failed command, as presented to the view: its kind, and the message that
/// says what went wrong in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorView {
    kind: ErrorKind,
    message: String,
}

impl ErrorView {
    /// A failure of no particular kind.
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Failed, message)
    }

    /// A failure of `kind`.
    pub(crate) fn of(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl From<EngineError> for ErrorView {
    fn from(error: EngineError) -> Self {
        Self::new(error.to_string())
    }
}

/// Lets one command at a time check for the sample and generate it, so two
/// commands arriving together never write it at once or read it half written.
#[derive(Debug, Default)]
pub struct SampleGate {
    generating: Mutex<()>,
}

impl SampleGate {
    /// Generates the sample into `directory` unless it is already there.
    fn ensure(&self, directory: &Path, spec: SampleSpec) -> Result<(), EngineError> {
        let _held = self
            .generating
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !claims_generated(directory) {
            generate_claims(directory, spec)?;
        }
        Ok(())
    }
}

/// Loads a working window of `keys` whole keys from the sample in `directory`.
fn open_window(
    gate: &SampleGate,
    directory: &Path,
    spec: SampleSpec,
    keys: u32,
) -> Result<Opened, EngineError> {
    gate.ensure(directory, spec)?;
    Table::load_keys(directory, KEY_COLUMN, keys).map(Opened::Window)
}

/// Opens the whole sample in `directory` where it is stored.
fn open_stored(
    gate: &SampleGate,
    directory: &Path,
    spec: SampleSpec,
) -> Result<Opened, EngineError> {
    gate.ensure(directory, spec)?;
    ParquetTable::open(directory).map(Opened::Stored)
}

/// Where the sample table is stored and how big it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    directory: PathBuf,
    spec: SampleSpec,
}

impl Sample {
    /// The sample table of `spec` kept in `directory`.
    pub const fn new(directory: PathBuf, spec: SampleSpec) -> Self {
        Self { directory, spec }
    }

    /// The sample the application serves, kept in the folder of `cache` that its
    /// size names, so that a different size gets its own folder.
    pub fn in_cache(cache: &Path) -> Self {
        let columns = SampleSpec::CLAIM_COLUMNS.len() + usize::from(SAMPLE.filler_columns);
        let name = format!("claims-{}-keys-{columns}-columns", SAMPLE.keys);
        Self::new(cache.join(name), SAMPLE)
    }

    /// The sample the application serves, kept in its Tauri application cache.
    fn of_app<R: Runtime>(app: &AppHandle<R>) -> Result<Self, ErrorView> {
        let cache = app
            .path()
            .app_cache_dir()
            .map_err(|error| ErrorView::new(format!("finding the application cache: {error}")))?;
        Ok(Self::in_cache(&cache))
    }
}

fn show(open: &OpenTable, table: Opened, opened: Duration) -> TableView {
    let view = table.view(opened);
    open.replace(Arc::new(table));
    view
}

/// Opens a working window of `keys` whole keys of `sample`, held in memory,
/// keeps it open in `open` and reports its shape. A window holds at most
/// `MAX_WINDOW_KEYS`.
///
/// The first call generates the sample.
pub fn sample_window_view(
    gate: &SampleGate,
    open: &OpenTable,
    sample: &Sample,
    keys: u32,
) -> Result<TableView, ErrorView> {
    let started = Instant::now();
    if keys > MAX_WINDOW_KEYS {
        return Err(ErrorView::of(
            ErrorKind::Limit,
            format!("a working window holds at most {MAX_WINDOW_KEYS} keys"),
        ));
    }
    let table = open_window(gate, &sample.directory, sample.spec, keys)?;
    Ok(show(open, table, started.elapsed()))
}

/// Opens the whole of `sample` where it is stored, without loading it, keeps
/// it open in `open` and reports its shape.
///
/// The first call generates the sample.
pub fn sample_result_view(
    gate: &SampleGate,
    open: &OpenTable,
    sample: &Sample,
) -> Result<TableView, ErrorView> {
    let started = Instant::now();
    let table = open_stored(gate, &sample.directory, sample.spec)?;
    Ok(show(open, table, started.elapsed()))
}

/// Returns `length` rows of the table open in `open` starting at `offset`,
/// limited to `columns`, as the bytes of a frame: the number of each row and a
/// report of the block, and the block as an Arrow IPC stream of text. A block
/// holds at most `MAX_BLOCK_ROWS` rows.
pub fn table_block_bytes(
    open: &OpenTable,
    offset: usize,
    length: usize,
    columns: &[String],
) -> Result<Vec<u8>, ErrorView> {
    if length > MAX_BLOCK_ROWS {
        return Err(ErrorView::of(
            ErrorKind::Limit,
            format!("a block holds at most {MAX_BLOCK_ROWS} rows"),
        ));
    }
    let table = open
        .current()
        .ok_or_else(|| ErrorView::new("no table is open"))?;
    let started = Instant::now();
    let block = table.block(offset, length, columns)?;
    let built = started.elapsed();
    frame(offset, &block, built)
}

/// Opens a working window of `keys` whole keys of the sample table, held in
/// memory, and reports its shape. A window holds at most `MAX_WINDOW_KEYS`.
///
/// The first call generates the sample into the application cache.
#[tauri::command(async)]
pub fn open_sample_window<R: Runtime>(
    app: AppHandle<R>,
    gate: State<'_, SampleGate>,
    open: State<'_, OpenTable>,
    keys: u32,
) -> Result<TableView, ErrorView> {
    sample_window_view(&gate, &open, &Sample::of_app(&app)?, keys)
}

/// Opens the whole sample table where it is stored, without loading it, and
/// reports its shape.
///
/// The first call generates the sample into the application cache.
#[tauri::command(async)]
pub fn open_sample_result<R: Runtime>(
    app: AppHandle<R>,
    gate: State<'_, SampleGate>,
    open: State<'_, OpenTable>,
) -> Result<TableView, ErrorView> {
    sample_result_view(&gate, &open, &Sample::of_app(&app)?)
}

/// Returns `length` rows of the open table starting at `offset`, limited to
/// `columns`, as the raw bytes of a frame: the number of each row and a report of
/// the block, and the block as an Arrow IPC stream of text. A block holds at most
/// `MAX_BLOCK_ROWS` rows.
#[tauri::command(async)]
pub fn table_block(
    open: State<'_, OpenTable>,
    offset: usize,
    length: usize,
    columns: Vec<String>,
) -> Result<Response, ErrorView> {
    Ok(Response::new(table_block_bytes(
        &open, offset, length, &columns,
    )?))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use dageist_engine::SampleSpec;
    use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
    use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
    use tauri::webview::InvokeRequest;
    use tauri::{WebviewWindow, WebviewWindowBuilder};

    use super::{
        ErrorKind, OpenTable, Opened, Sample, SampleGate, open_stored, open_window,
        sample_result_view, sample_window_view,
    };
    use crate::block::frame;
    use crate::test_support::split_block;

    const REFRESH: &str = "DAGEIST_REFRESH_TABLE_SNAPSHOTS";
    const REFRESH_COMMAND: &str = "cargo test --offline --locked -p dageist-tauri sample_table::tests::the_snapshots_are_what_the_commands_return";

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
        let view = serde_json::to_value(table.view(Duration::from_millis(12)))
            .expect("the view serializes");
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

        let table =
            open_window(&SampleGate::default(), &sample.path, SMALL, 10).expect("the window opens");

        let (rows, columns) = rows_and_columns(&table);
        assert_eq!(columns.len(), SMALL_COLUMNS);
        assert_eq!(
            columns[0],
            serde_json::json!({ "name": "dcn", "dataType": "str", "numeric": false })
        );
        // Ten keys of two to eight step rows each.
        assert!((20..=80).contains(&rows), "{rows} rows for ten keys");
    }

    #[test]
    fn stored_result_reports_every_row_of_the_sample() {
        let sample = SampleDirectory::empty("stored");

        let table =
            open_stored(&SampleGate::default(), &sample.path, SMALL).expect("the result opens");

        let (rows, columns) = rows_and_columns(&table);
        assert_eq!(columns.len(), SMALL_COLUMNS);
        // Forty keys of two to eight step rows each.
        assert!((80..=320).contains(&rows), "{rows} rows for forty keys");
    }

    #[test]
    fn existing_sample_is_opened_without_being_rewritten() {
        let sample = SampleDirectory::empty("reuse");
        open_window(&SampleGate::default(), &sample.path, SMALL, 5)
            .expect("the sample is generated");
        let file = sample.path.join("claims-0000.parquet");
        let written = fs::metadata(&file)
            .and_then(|metadata| metadata.modified())
            .expect("the sample file has a modification time");

        // A different size would produce different files if it were generated.
        let bigger = SampleSpec {
            keys: 400,
            filler_columns: 9,
        };
        let table =
            open_stored(&SampleGate::default(), &sample.path, bigger).expect("the sample reopens");

        let reopened = fs::metadata(&file)
            .and_then(|metadata| metadata.modified())
            .expect("the sample file still exists");
        assert_eq!(reopened, written);
        assert_eq!(rows_and_columns(&table).1.len(), SMALL_COLUMNS);
    }

    /// Checks that `text` is the size `bytes` as a block's report writes it: the bytes
    /// themselves below a kilobyte of 1,024 bytes, and the kilobytes to one decimal below a
    /// megabyte. The decimal may be rounded either way, so it is held to within half a tenth.
    fn assert_size(text: &str, bytes: usize) {
        if let Some(whole) = text.strip_suffix(" B") {
            assert_eq!(whole.parse::<usize>().expect("a whole number"), bytes);
            return;
        }
        let kilobytes: f64 = text
            .strip_suffix(" KB")
            .unwrap_or_else(|| panic!("{text:?} is not a size in bytes or kilobytes"))
            .parse()
            .expect("a number of kilobytes");
        assert!(bytes >= 1024, "{text:?} for {bytes} bytes");
        assert!(
            (bytes as f64 / 1024.0 - kilobytes).abs() <= 0.05 + 1e-9,
            "{text:?} is not {bytes} bytes to a tenth of a kilobyte"
        );
    }

    /// Checks a block's report: its rows and columns, the size of its Arrow
    /// bytes, and a time that ends in a unit.
    fn assert_report(report: &serde_json::Value, rows: &str, columns: &str, arrow: usize) {
        let report = report.as_str().expect("a report is text");
        let counts = format!("{rows} × {columns} · ");
        let rest = report
            .strip_prefix(&counts)
            .unwrap_or_else(|| panic!("{report:?} does not start with {counts:?}"));
        let (size, time) = rest
            .split_once(" · built in ")
            .unwrap_or_else(|| panic!("{report:?} does not say how long it was built in"));
        assert_size(size, arrow);
        assert!(
            ["µs", "ms", " s"].iter().any(|unit| time.ends_with(unit)),
            "{time:?} is not a time"
        );
    }

    fn block_of(response: Result<InvokeResponseBody, serde_json::Value>) -> Vec<u8> {
        match response {
            Ok(InvokeResponseBody::Raw(bytes)) => bytes,
            other => panic!("expected raw bytes, got {other:?}"),
        }
    }

    #[test]
    fn block_command_answers_a_window_block_with_its_row_numbers_and_arrow_bytes() {
        let sample = SampleDirectory::empty("window-block");
        let table =
            open_window(&SampleGate::default(), &sample.path, SMALL, 10).expect("the window opens");
        let columns = ["dcn".to_owned(), "timestamp".to_owned()];
        let expected = table.block(3, 7, &columns).expect("the engine slices");

        let response = tauri::test::get_ipc_response(
            &webview(Some(table)),
            block_request(serde_json::json!({
                "offset": 3,
                "length": 7,
                "columns": columns,
            })),
        );

        let bytes = block_of(response);
        let (header, arrow) = split_block(&bytes);
        assert_eq!(arrow, expected.ipc());
        // The numbers of rows 4 to 10, counting from 1.
        assert_eq!(
            header["numbers"],
            serde_json::json!(["4", "5", "6", "7", "8", "9", "10"])
        );
        assert_report(&header["report"], "7 rows", "2 columns", arrow.len());
        assert_eq!(header.as_object().expect("an object").len(), 2);
    }

    #[test]
    fn block_command_answers_a_stored_block_with_its_row_numbers_and_arrow_bytes() {
        let sample = SampleDirectory::empty("stored-block");
        let table =
            open_stored(&SampleGate::default(), &sample.path, SMALL).expect("the result opens");
        let columns = ["dcn".to_owned(), "measure_002".to_owned()];
        let expected = table.block(50, 12, &columns).expect("the engine reads");

        let response = tauri::test::get_ipc_response(
            &webview(Some(table)),
            block_request(serde_json::json!({
                "offset": 50,
                "length": 12,
                "columns": columns,
            })),
        );

        let bytes = block_of(response);
        let (header, arrow) = split_block(&bytes);
        assert_eq!(arrow, expected.ipc());
        let numbers: Vec<String> = (51..=62).map(|number| number.to_string()).collect();
        assert_eq!(header["numbers"], serde_json::json!(numbers));
        assert_report(&header["report"], "12 rows", "2 columns", arrow.len());
    }

    #[test]
    fn block_past_the_end_is_numbered_only_as_far_as_it_has_rows() {
        let sample = SampleDirectory::empty("numbers-end");
        let table =
            open_stored(&SampleGate::default(), &sample.path, SMALL).expect("the result opens");
        let rows = rows_and_columns(&table).0;
        let columns = ["dcn".to_owned()];
        let webview = webview(Some(table));

        let last = block_of(tauri::test::get_ipc_response(
            &webview,
            block_request(serde_json::json!({
                "offset": rows - 2,
                "length": 10,
                "columns": columns,
            })),
        ));
        let beyond = block_of(tauri::test::get_ipc_response(
            &webview,
            block_request(serde_json::json!({
                "offset": rows + 5,
                "length": 10,
                "columns": columns,
            })),
        ));

        assert_eq!(
            split_block(&last).0["numbers"],
            serde_json::json!([(rows - 1).to_string(), rows.to_string()])
        );
        assert_report(
            &split_block(&last).0["report"],
            "2 rows",
            "1 column",
            split_block(&last).1.len(),
        );
        assert_eq!(split_block(&beyond).0["numbers"], serde_json::json!([]));
        assert_report(
            &split_block(&beyond).0["report"],
            "0 rows",
            "1 column",
            split_block(&beyond).1.len(),
        );
    }

    #[test]
    fn an_opened_table_reports_its_figures_as_text() {
        let sample = SampleDirectory::empty("figures");
        let sample = Sample::new(sample.path.clone(), SMALL);
        let gate = SampleGate::default();

        let window = sample_window_view(&gate, &OpenTable::default(), &sample, 10)
            .expect("the window opens");
        let stored =
            sample_result_view(&gate, &OpenTable::default(), &sample).expect("the result opens");

        for view in [window, stored] {
            let view = serde_json::to_value(view).expect("the view serializes");
            let rows = view["rows"].as_u64().expect("a row count");
            // Below a thousand rows a count has no separator.
            assert!(rows < 1000);
            let figures = &view["figures"];
            assert_eq!(figures["rows"], format!("{rows} rows"));
            assert_eq!(figures["columns"], format!("{SMALL_COLUMNS} columns"));
            let opened = figures["opened"].as_str().expect("the open time is text");
            assert!(
                ["µs", "ms", " s"].iter().any(|unit| opened.ends_with(unit)),
                "{opened:?} is not a time"
            );
            assert_eq!(figures.as_object().expect("an object").len(), 3);
        }
    }

    #[test]
    fn columns_say_which_of_them_hold_numbers() {
        let sample = SampleDirectory::empty("numeric");
        let table =
            open_window(&SampleGate::default(), &sample.path, SMALL, 5).expect("the window opens");

        let (_, columns) = rows_and_columns(&table);

        let numeric: Vec<(&str, bool)> = columns
            .iter()
            .map(|column| {
                (
                    column["dataType"].as_str().expect("a dtype"),
                    column["numeric"].as_bool().expect("a flag"),
                )
            })
            .collect();
        assert!(numeric.contains(&("i32", true)), "{numeric:?}");
        assert!(numeric.contains(&("f64", true)), "{numeric:?}");
        assert!(numeric.contains(&("str", false)), "{numeric:?}");
        assert!(numeric.contains(&("date", false)), "{numeric:?}");
        assert!(
            numeric
                .iter()
                .all(|(dtype, flag)| *flag == (*dtype == "i32" || *dtype == "f64")),
            "{numeric:?}"
        );
    }

    /// The bytes as lowercase hexadecimal text.
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// A block as the view's fixture holds it: what was asked for and the frame
    /// answered, with the time the engine took fixed so that the frame is the same
    /// on every run.
    fn block_snapshot(
        table: &Opened,
        offset: usize,
        length: usize,
        columns: &[&str],
    ) -> serde_json::Value {
        let columns: Vec<String> = columns.iter().map(|name| (*name).to_owned()).collect();
        let block = table
            .block(offset, length, &columns)
            .expect("the engine writes the block");
        let frame = frame(offset, &block, Duration::from_micros(850)).expect("the block is framed");
        serde_json::json!({
            "request": { "offset": offset, "length": length, "columns": columns },
            "frame": hex(&frame),
        })
    }

    #[test]
    fn the_snapshots_are_what_the_commands_return() {
        let sample = SampleDirectory::empty("snapshots");
        let gate = SampleGate::default();
        let window = open_window(&gate, &sample.path, SMALL, 10).expect("the window opens");
        let stored = open_stored(&gate, &sample.path, SMALL).expect("the result opens");
        let opened = Duration::from_millis(12);
        let columns = [
            "dcn",
            "timestamp",
            "date_of_receipt",
            "measure_001",
            "measure_003",
        ];
        let snapshots = [
            (
                "table-window.json",
                serde_json::to_value(window.view(opened)).expect("the view serializes"),
            ),
            (
                "table-result.json",
                serde_json::to_value(stored.view(opened)).expect("the view serializes"),
            ),
            (
                "table-block-window.json",
                block_snapshot(&window, 3, 7, &columns),
            ),
            (
                "table-block-result.json",
                block_snapshot(&stored, 78, 10, &columns),
            ),
        ];
        for (file, returned) in &snapshots {
            crate::snapshot::check(file, returned, REFRESH, REFRESH_COMMAND);
        }
    }

    /// The kind that follows `kind` in the list the view's contract gives, or `None` after the
    /// last. A list cannot be checked against the enum it lists, which Rust does not let a
    /// program enumerate, so this names every variant: adding one does not compile until it has
    /// a place here, and the test below walks the chain and expects it to be `ErrorKind::ALL`.
    /// The walk reaches only the kinds linked from the first, so add a new kind inside or at the
    /// end of the chain.
    const fn next_kind(kind: ErrorKind) -> Option<ErrorKind> {
        match kind {
            ErrorKind::NoSuchStep => Some(ErrorKind::NotRun),
            ErrorKind::NotRun => Some(ErrorKind::Refused),
            ErrorKind::Refused => Some(ErrorKind::Limit),
            ErrorKind::Limit => Some(ErrorKind::Unreadable),
            ErrorKind::Unreadable => Some(ErrorKind::UnknownName),
            ErrorKind::UnknownName => Some(ErrorKind::Failed),
            ErrorKind::Failed => None,
        }
    }

    #[test]
    fn the_list_of_error_kinds_is_every_kind_in_order() {
        let mut walked = vec![ErrorKind::NoSuchStep];
        while let Some(following) = next_kind(walked[walked.len() - 1]) {
            // More than the list has, so a chain that loops fails and does not hang.
            assert!(walked.len() < 64, "the chain does not end");
            walked.push(following);
        }
        assert_eq!(walked, ErrorKind::ALL);
    }

    #[test]
    fn block_command_without_an_open_table_reports_it() {
        let response = tauri::test::get_ipc_response(
            &webview(None),
            block_request(serde_json::json!({ "offset": 0, "length": 5, "columns": [] })),
        );

        match response {
            Err(error) => assert_eq!(
                error,
                serde_json::json!({ "kind": "failed", "message": "no table is open" })
            ),
            Ok(body) => panic!("expected an error, got {body:?}"),
        }
    }

    #[test]
    fn block_command_with_an_unknown_column_reports_it() {
        let sample = SampleDirectory::empty("unknown");
        let table =
            open_window(&SampleGate::default(), &sample.path, SMALL, 5).expect("the window opens");

        let response = tauri::test::get_ipc_response(
            &webview(Some(table)),
            block_request(serde_json::json!({
                "offset": 0,
                "length": 5,
                "columns": ["no_such_column"],
            })),
        );

        let error = response.expect_err("an unknown column is refused");
        assert_eq!(error["kind"], "failed");
        let message = error["message"].as_str().expect("a message");
        assert!(
            message.starts_with("selecting block columns"),
            "unexpected message: {message}"
        );
    }

    #[test]
    fn window_command_refuses_more_keys_than_a_window_may_hold() {
        let app = mock_builder()
            .manage(SampleGate::default())
            .manage(OpenTable::default())
            .invoke_handler(tauri::generate_handler![super::open_sample_window])
            .build(mock_context(noop_assets()))
            .expect("the mock application builds");
        let window = WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
            .build()
            .expect("the mock window builds");
        let mut request = block_request(serde_json::json!({ "keys": 50_001 }));
        request.cmd = "open_sample_window".into();

        // The limit is checked before any sample is looked for or generated.
        let response = tauri::test::get_ipc_response(&window, request);

        match response {
            Err(error) => assert_eq!(
                error,
                serde_json::json!({
                    "kind": "limit",
                    "message": "a working window holds at most 50000 keys"
                })
            ),
            Ok(body) => panic!("expected an error, got {body:?}"),
        }
    }

    #[test]
    fn block_command_refuses_more_rows_than_a_block_may_hold() {
        let sample = SampleDirectory::empty("too-long");
        let table =
            open_window(&SampleGate::default(), &sample.path, SMALL, 5).expect("the window opens");

        let response = tauri::test::get_ipc_response(
            &webview(Some(table)),
            block_request(serde_json::json!({ "offset": 0, "length": 10_001, "columns": [] })),
        );

        match response {
            Err(error) => assert_eq!(
                error,
                serde_json::json!({
                    "kind": "limit",
                    "message": "a block holds at most 10000 rows"
                })
            ),
            Ok(body) => panic!("expected an error, got {body:?}"),
        }
    }

    #[test]
    fn commands_arriving_together_open_the_same_complete_sample() {
        let sample = SampleDirectory::empty("together");
        let gate = SampleGate::default();
        let start = std::sync::Barrier::new(4);

        let rows: Vec<u64> = std::thread::scope(|scope| {
            let openers: Vec<_> = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        start.wait();
                        let table =
                            open_stored(&gate, &sample.path, SMALL).expect("the result opens");
                        rows_and_columns(&table).0
                    })
                })
                .collect();
            openers
                .into_iter()
                .map(|opener| opener.join().expect("an opener finishes"))
                .collect()
        });

        // Read afterwards, alone, the sample holds what every opener saw.
        let settled = open_stored(&gate, &sample.path, SMALL).expect("the result reopens");
        let expected = rows_and_columns(&settled).0;
        assert!(
            (80..=320).contains(&expected),
            "{expected} rows for forty keys"
        );
        assert_eq!(rows, [expected; 4]);
        let left_over = fs::read_dir(sample.path.parent().expect("a parent directory"))
            .expect("the parent is listed")
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("dageist-tauri-{}-together.", std::process::id()))
            });
        assert!(
            !left_over,
            "no partly written sample is left beside the sample"
        );
    }
}
