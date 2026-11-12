//! Opens generated Parquet files as a stored table and compares the blocks it
//! reads from disk with the same rows read whole by Polars.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use dageist_engine::{ParquetTable, SampleSpec, generate_claims};
use polars::prelude::*;

/// Enough keys to need a second file, so blocks can cross a file boundary.
const SPEC: SampleSpec = SampleSpec {
    keys: 25_040,
    filler_columns: 0,
};

struct StoredSample {
    path: PathBuf,
    /// The first file's rows followed by the second's, read whole.
    whole: DataFrame,
    first_file_rows: usize,
}

impl StoredSample {
    fn generate(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("dageist-stored-{}-{name}", std::process::id()));
        let files = generate_claims(&path, SPEC).expect("the sample is generated");
        assert_eq!(files.len(), 2, "the sample spans two files");
        let mut frames = files.iter().map(|file| {
            LazyFrame::scan_parquet(
                PlRefPath::try_from_path(file).expect("a file path"),
                ScanArgsParquet::default(),
            )
            .and_then(LazyFrame::collect)
            .expect("the file is read whole")
        });
        let first = frames.next().expect("a first file");
        let second = frames.next().expect("a second file");
        let first_file_rows = first.height();
        let whole = first.vstack(&second).expect("the files share a schema");
        Self {
            path,
            whole,
            first_file_rows,
        }
    }
}

impl Drop for StoredSample {
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
fn stored_table_reports_the_rows_and_columns_of_every_file() {
    let sample = StoredSample::generate("shape");

    let table = ParquetTable::open(&sample.path).expect("the table opens");

    assert_eq!(table.height(), sample.whole.height());
    let names: Vec<&str> = table
        .columns()
        .iter()
        .map(|column| column.name.as_str())
        .collect();
    assert_eq!(names, SampleSpec::CLAIM_COLUMNS);
}

#[test]
fn block_across_a_file_boundary_matches_the_rows_read_whole() {
    let sample = StoredSample::generate("boundary");
    let table = ParquetTable::open(&sample.path).expect("the table opens");
    let columns = ["dcn", "timestamp", "step_to"];
    let offset = sample.first_file_rows - 6;

    let block = read_ipc(
        table
            .block_ipc(offset, 15, &columns.map(str::to_owned))
            .expect("the block is read"),
    );

    let expected = sample
        .whole
        .select(columns)
        .expect("the columns exist")
        .slice(offset as i64, 15);
    assert_eq!(block.height(), 15);
    assert!(block.equals(&expected));
}

#[test]
fn block_with_no_columns_named_holds_every_column() {
    let sample = StoredSample::generate("all");
    let table = ParquetTable::open(&sample.path).expect("the table opens");

    let block = read_ipc(table.block_ipc(40, 25, &[]).expect("the block is read"));

    assert!(block.equals(&sample.whole.slice(40, 25)));
}

#[test]
fn block_past_the_end_holds_only_the_rows_that_exist() {
    let sample = StoredSample::generate("end");
    let table = ParquetTable::open(&sample.path).expect("the table opens");
    let columns = ["dcn".to_owned()];

    let last = read_ipc(
        table
            .block_ipc(table.height() - 3, 50, &columns)
            .expect("the last rows are read"),
    );
    let beyond = read_ipc(
        table
            .block_ipc(table.height() + 10, 50, &columns)
            .expect("an empty block is still written"),
    );

    assert_eq!(last.height(), 3);
    assert_eq!(beyond.height(), 0);
    assert_eq!(beyond.width(), 1);
}

#[test]
fn unknown_column_is_an_invalid_request() {
    let sample = StoredSample::generate("unknown");
    let table = ParquetTable::open(&sample.path).expect("the table opens");

    let error = table
        .block_ipc(0, 10, &["no_such_column".to_owned()])
        .expect_err("an unknown column is refused");

    assert!(error.is_invalid_request());
}

#[test]
fn directory_without_parquet_files_does_not_open() {
    let path = std::env::temp_dir().join(format!("dageist-stored-{}-empty", std::process::id()));
    fs::create_dir_all(&path).expect("an empty directory");

    let opened = ParquetTable::open(&path);

    let _ = fs::remove_dir_all(&path);
    assert!(opened.is_err());
}
