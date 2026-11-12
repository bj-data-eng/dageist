//! Generates a small sample table, loads a working window of whole keys and
//! reads blocks of it back from Arrow IPC bytes.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use dageist_engine::{SampleSpec, Table, generate_claims};
use polars::prelude::*;

const KEYS: u32 = 120;
const FILLER_COLUMNS: u16 = 12;

struct SampleDirectory {
    path: PathBuf,
}

impl SampleDirectory {
    fn generate(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("dageist-engine-{}-{name}", std::process::id()));
        let spec = SampleSpec {
            keys: KEYS,
            filler_columns: FILLER_COLUMNS,
        };
        let written = generate_claims(&path, spec).expect("the sample is generated");
        assert_eq!(written.len(), 1, "a small sample fits in one file");
        Self { path }
    }
}

impl Drop for SampleDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn read_ipc(bytes: Vec<u8>) -> DataFrame {
    IpcStreamReader::new(Cursor::new(bytes))
        .finish()
        .expect("the bytes are an Arrow IPC stream")
}

#[test]
fn generated_table_has_claim_columns_then_filler() {
    let sample = SampleDirectory::generate("columns");
    let table = Table::load_keys(&sample.path, "dcn", KEYS).expect("the table loads");

    let names: Vec<&str> = table
        .columns()
        .iter()
        .map(|column| column.name.as_str())
        .collect();

    assert_eq!(
        names.len(),
        SampleSpec::CLAIM_COLUMNS.len() + usize::from(FILLER_COLUMNS)
    );
    assert_eq!(
        names[..SampleSpec::CLAIM_COLUMNS.len()],
        SampleSpec::CLAIM_COLUMNS
    );
    assert_eq!(names.last().copied(), Some("measure_012"));
}

/// Reads every generated file whole, as Polars alone sees it.
fn read_sample(sample: &SampleDirectory) -> DataFrame {
    LazyFrame::scan_parquet(
        PlRefPath::try_from_path(&sample.path.join("*.parquet")).expect("a path"),
        ScanArgsParquet::default(),
    )
    .and_then(LazyFrame::collect)
    .expect("the sample is read whole")
}

fn text_values(frame: &DataFrame, column: &str) -> Vec<String> {
    let values = frame.column(column).expect("the column exists");
    (0..frame.height())
        .map(|row| values.get(row).expect("a value").to_string())
        .collect()
}

#[test]
fn window_holds_every_row_of_the_first_keys_and_nothing_else() {
    let sample = SampleDirectory::generate("keys");
    let wanted_keys = 30;
    let table = Table::load_keys(&sample.path, "dcn", wanted_keys).expect("the table loads");
    let window = read_ipc(
        table
            .block_ipc(0, table.height(), &[])
            .expect("the whole window is written"),
    );

    // The sample is written in key order, so the first keys are the first
    // distinct values, and their rows are a prefix of the whole table.
    let whole = read_sample(&sample);
    let keys = text_values(&whole, "dcn");
    let mut first_keys: Vec<&str> = Vec::new();
    for key in &keys {
        if !first_keys.contains(&key.as_str()) {
            if first_keys.len() == wanted_keys as usize {
                break;
            }
            first_keys.push(key);
        }
    }
    let expected_rows = keys
        .iter()
        .filter(|key| first_keys.contains(&key.as_str()))
        .count();

    assert_eq!(first_keys.len(), wanted_keys as usize);
    assert_eq!(window.height(), expected_rows);
    assert!(window.equals(&whole.slice(0, expected_rows)));
}

#[test]
fn timestamps_rise_within_each_key() {
    let sample = SampleDirectory::generate("rising");
    let whole = read_sample(&sample);
    let dcns = text_values(&whole, "dcn");
    let stamp_column = whole
        .column("timestamp")
        .expect("timestamp")
        .cast(&DataType::Int64)
        .expect("timestamps as integers");
    let stamps: Vec<i64> = (0..whole.height())
        .map(|row| {
            stamp_column
                .get(row)
                .expect("a timestamp")
                .try_extract::<i64>()
                .expect("an integer timestamp")
        })
        .collect();

    let mut rows_per_key = BTreeMap::new();
    for dcn in &dcns {
        *rows_per_key.entry(dcn.as_str()).or_insert(0usize) += 1;
    }
    assert_eq!(rows_per_key.len(), KEYS as usize);
    assert!(rows_per_key.values().all(|count| (2..=8).contains(count)));
    for row in 1..dcns.len() {
        if dcns[row] == dcns[row - 1] {
            assert!(
                stamps[row] > stamps[row - 1],
                "timestamps rise within {}",
                dcns[row]
            );
        }
    }
}

#[test]
fn block_matches_the_same_rows_of_the_whole() {
    let sample = SampleDirectory::generate("slice");
    let table = Table::load_keys(&sample.path, "dcn", KEYS).expect("the table loads");
    let columns = [
        "dcn".to_owned(),
        "timestamp".to_owned(),
        "measure_002".to_owned(),
    ];

    let whole = read_ipc(table.block_ipc(0, table.height(), &columns).expect("whole"));
    let slice = read_ipc(table.block_ipc(40, 25, &columns).expect("slice"));

    assert_eq!(slice.width(), 3);
    assert_eq!(slice.height(), 25);
    assert!(slice.equals(&whole.slice(40, 25)));
}

#[test]
fn block_past_the_end_is_empty() {
    let sample = SampleDirectory::generate("end");
    let table = Table::load_keys(&sample.path, "dcn", 5).expect("the table loads");

    let slice = read_ipc(
        table
            .block_ipc(table.height() + 10, 50, &["dcn".to_owned()])
            .expect("an empty block is still written"),
    );

    assert_eq!(slice.height(), 0);
    assert_eq!(slice.width(), 1);
}

#[test]
fn unknown_column_is_an_invalid_request() {
    let sample = SampleDirectory::generate("unknown");
    let table = Table::load_keys(&sample.path, "dcn", 5).expect("the table loads");

    let error = table
        .block_ipc(0, 10, &["no_such_column".to_owned()])
        .expect_err("an unknown column is refused");

    assert!(error.is_invalid_request());
}

#[test]
fn column_named_twice_is_an_invalid_request() {
    let sample = SampleDirectory::generate("twice");
    let table = Table::load_keys(&sample.path, "dcn", 5).expect("the table loads");

    let error = table
        .block_ipc(0, 10, &["dcn".to_owned(), "dcn".to_owned()])
        .expect_err("a repeated column is refused");

    assert!(error.is_invalid_request());
}
