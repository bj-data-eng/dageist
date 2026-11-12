//! Generates a claims table shaped like a real workload and times the steps a
//! live preview and a stored result depend on. Run it optimised:
//!
//! ```sh
//! cargo run --release -p dageist-engine --example measure -- tmp/sample-claims
//! ```

use std::env;
use std::path::PathBuf;
use std::time::Instant;

use dageist_engine::{EngineError, ParquetTable, SampleSpec, Table, generate_claims};

const KEYS: u32 = 200_000;
const FILLER_COLUMNS: u16 = 292;
const WINDOW_KEYS: u32 = 500;
const VISIBLE_ROWS: usize = 60;
const VISIBLE_COLUMNS: usize = 14;

fn main() -> Result<(), EngineError> {
    let directory = env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("tmp/sample-claims"), PathBuf::from);

    let started = Instant::now();
    let files = generate_claims(
        &directory,
        SampleSpec {
            keys: KEYS,
            filler_columns: FILLER_COLUMNS,
        },
    )?;
    println!(
        "generated {} files in {:.1?}",
        files.len(),
        started.elapsed()
    );

    let started = Instant::now();
    let table = Table::load_keys(&directory, "dcn", WINDOW_KEYS)?;
    let columns = table.columns();
    println!(
        "loaded a window of {WINDOW_KEYS} keys: {} rows x {} columns in {:.1?}",
        table.height(),
        columns.len(),
        started.elapsed()
    );

    let visible: Vec<String> = columns
        .iter()
        .take(VISIBLE_COLUMNS)
        .map(|column| column.name.clone())
        .collect();
    let middle = table.height() / 2;
    let started = Instant::now();
    let rounds = 200;
    let mut bytes = 0;
    for round in 0..rounds {
        bytes = table
            .block_ipc(middle + round, VISIBLE_ROWS, &visible)?
            .len();
    }
    println!(
        "served {VISIBLE_ROWS} rows x {VISIBLE_COLUMNS} columns as Arrow: {bytes} bytes, {:.1?} each",
        started.elapsed() / rounds as u32
    );

    let started = Instant::now();
    let everything = table.block_ipc(0, table.height(), &[])?;
    println!(
        "served the whole window as Arrow: {:.1} MB in {:.1?}",
        everything.len() as f64 / 1_048_576.0,
        started.elapsed()
    );

    let started = Instant::now();
    let stored = ParquetTable::open(&directory)?;
    println!(
        "opened the stored table: {} rows in {:.1?}",
        stored.height(),
        started.elapsed()
    );
    for (place, offset) in [
        ("start", 0),
        ("middle", stored.height() / 2),
        ("end", stored.height().saturating_sub(VISIBLE_ROWS)),
    ] {
        let started = Instant::now();
        let rounds = 20;
        for round in 0..rounds {
            bytes = stored
                .block_ipc(offset.saturating_sub(round), VISIBLE_ROWS, &visible)?
                .len();
        }
        println!(
            "read {VISIBLE_ROWS} rows x {VISIBLE_COLUMNS} columns from Parquet at the {place}: {bytes} bytes, {:.1?} each",
            started.elapsed() / rounds as u32
        );
    }
    Ok(())
}
