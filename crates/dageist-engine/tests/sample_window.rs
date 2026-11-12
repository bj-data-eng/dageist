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

    let names: Vec<String> = table
        .columns()
        .into_iter()
        .map(|column| column.name)
        .collect();

    assert_eq!(
        names.len(),
        SampleSpec::CLAIM_COLUMNS.len() + usize::from(FILLER_COLUMNS)
    );
    assert_eq!(
        names[..SampleSpec::CLAIM_COLUMNS.len()],
        SampleSpec::CLAIM_COLUMNS
    );
    assert_eq!(names.last().map(String::as_str), Some("measure_012"));
}

#[test]
fn window_holds_whole_keys_with_rising_timestamps() {
    let sample = SampleDirectory::generate("keys");
    let wanted_keys = 30;
    let table = Table::load_keys(&sample.path, "dcn", wanted_keys).expect("the table loads");
    let everything = read_ipc(
        table
            .block_ipc(0, table.height(), &[])
            .expect("the whole window is written"),
    );

    let dcn_column = everything.column("dcn").expect("dcn");
    let stamp_column = everything
        .column("timestamp")
        .expect("timestamp")
        .cast(&DataType::Int64)
        .expect("timestamps as integers");
    let dcns: Vec<String> = (0..everything.height())
        .map(|row| dcn_column.get(row).expect("a key").to_string())
        .collect();
    let stamps: Vec<i64> = (0..everything.height())
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
    assert_eq!(rows_per_key.len(), wanted_keys as usize);
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
