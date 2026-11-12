//! Generates a made-up claims table for development and measurement.
//!
//! Polars has no dataset generator, so the table is built from row numbers:
//! every value is a seeded hash of its row or key, which makes the output the
//! same on every run. Each key (`dcn`) owns between two and eight step rows
//! with rising timestamps.

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use polars::prelude::*;

use crate::error::EngineError;

/// The most step rows one key can have.
const SLOTS_PER_KEY: i64 = 8;
/// The fewest step rows one key can have.
const MIN_STEPS: u64 = 2;
/// Keys written to each Parquet file.
const KEYS_PER_FILE: i64 = 25_000;
/// 2025-01-01T00:00:00Z in microseconds since the Unix epoch.
const BASE_MICROS: i64 = 1_735_689_600_000_000;
const MICROS_PER_SECOND: i64 = 1_000_000;
const SECONDS_PER_DAY: u64 = 86_400;

const STEPS: [&str; 8] = [
    "Intake", "Triage", "Review", "Pending", "Approval", "Payment", "Audit", "Complete",
];
const WORKFLOWS: [&str; 4] = ["Standard", "Expedited", "Appeal", "Manual"];
const DOCTYPES: [&str; 6] = ["Claim", "Invoice", "Referral", "Appeal", "Letter", "Form"];

/// The size of a generated claims table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleSpec {
    /// How many keys (`dcn` values) to generate.
    pub keys: u32,
    /// How many numeric columns to add after the claim columns.
    pub filler_columns: u16,
}

impl SampleSpec {
    /// The claim columns every generated table starts with.
    pub const CLAIM_COLUMNS: [&'static str; 8] = [
        "dcn",
        "timestamp",
        "step_from",
        "step_to",
        "workflow_from",
        "workflow_to",
        "doctype",
        "date_of_receipt",
    ];
}

fn seeded(value: Expr, seed: u64) -> Expr {
    value.hash(seed, seed.wrapping_mul(31), 7, 11)
}

fn pick(value: Expr, seed: u64, choices: &[&str], name: &str) -> PolarsResult<(Expr, LazyFrame)> {
    let index_name = format!("{name}_index");
    let size = choices.len() as u64;
    let index = (seeded(value, seed) % lit(size))
        .cast(DataType::Int64)
        .alias(&index_name);
    let lookup = df![
        index_name.as_str() => (0..choices.len() as i64).collect::<Vec<_>>(),
        name => choices.to_vec(),
    ]?;
    Ok((index, lookup.lazy()))
}

fn claims_for_keys(first_key: i64, key_count: i64, filler_columns: u16) -> PolarsResult<LazyFrame> {
    let rows = key_count * SLOTS_PER_KEY;
    let row = col("row");
    let key = (lit(first_key) + row.clone().floor_div(lit(SLOTS_PER_KEY))).alias("key");
    let slot = (row.clone() % lit(SLOTS_PER_KEY)).alias("slot");

    let frame = LazyFrame::default()
        .select([int_range(lit(0i64), lit(rows), 1, DataType::Int64).alias("row")])
        .with_columns([key, slot])
        .with_columns([
            (seeded(col("key"), 1) % lit(SLOTS_PER_KEY as u64 - MIN_STEPS + 1) + lit(MIN_STEPS))
                .cast(DataType::Int64)
                .alias("steps"),
            (seeded(col("key"), 2) % lit(300 * SECONDS_PER_DAY))
                .cast(DataType::Int64)
                .alias("start_seconds"),
            (seeded(col("key") * lit(SLOTS_PER_KEY) + col("slot"), 3) % lit(20 * 3_600u64))
                .cast(DataType::Int64)
                .alias("jitter_seconds"),
        ])
        .filter(col("slot").lt(col("steps")));

    let (workflow_from, workflow_from_names) = pick(col("key"), 4, &WORKFLOWS, "workflow_from")?;
    let (workflow_to, workflow_to_names) = pick(
        col("key") * lit(SLOTS_PER_KEY) + col("slot"),
        5,
        &WORKFLOWS,
        "workflow_to",
    )?;
    let (doctype, doctype_names) = pick(col("key"), 6, &DOCTYPES, "doctype")?;

    let step_names = df![
        "slot" => (0..STEPS.len() as i64).collect::<Vec<_>>(),
        "step_to" => STEPS.to_vec(),
    ]?
    .lazy();
    let previous_step_names = df![
        "slot" => (0..STEPS.len() as i64).collect::<Vec<_>>(),
        "step_from" => std::iter::once("Received")
            .chain(STEPS.iter().copied().take(STEPS.len() - 1))
            .collect::<Vec<_>>(),
    ]?
    .lazy();

    let seconds =
        col("start_seconds") + col("slot") * lit(SECONDS_PER_DAY as i64) + col("jitter_seconds");
    let timestamp_type = DataType::Datetime(TimeUnit::Microseconds, None);

    let mut columns: Vec<Expr> = vec![
        concat_str(
            [
                lit("DCN-"),
                (col("key") + lit(10_000_000i64)).cast(DataType::String),
            ],
            "",
            false,
        )
        .alias("dcn"),
        (lit(BASE_MICROS) + seconds * lit(MICROS_PER_SECOND))
            .cast(timestamp_type.clone())
            .alias("timestamp"),
        col("step_from"),
        col("step_to"),
        col("workflow_from"),
        col("workflow_to"),
        col("doctype"),
        (lit(BASE_MICROS) + col("start_seconds") * lit(MICROS_PER_SECOND))
            .cast(timestamp_type)
            .cast(DataType::Date)
            .alias("date_of_receipt"),
    ];
    for index in 0..filler_columns {
        let value = seeded(
            col("row") + lit(first_key * SLOTS_PER_KEY),
            100 + u64::from(index),
        ) % lit(100_000u64);
        let name = format!("measure_{:03}", index + 1);
        columns.push(if index % 3 == 0 {
            (value.cast(DataType::Float64) / lit(100.0)).alias(&name)
        } else {
            value.cast(DataType::Int32).alias(&name)
        });
    }

    Ok(frame
        .with_columns([workflow_from, workflow_to, doctype])
        .join(
            step_names,
            [col("slot")],
            [col("slot")],
            JoinArgs::new(JoinType::Left),
        )
        .join(
            previous_step_names,
            [col("slot")],
            [col("slot")],
            JoinArgs::new(JoinType::Left),
        )
        .join(
            workflow_from_names,
            [col("workflow_from_index")],
            [col("workflow_from_index")],
            JoinArgs::new(JoinType::Left),
        )
        .join(
            workflow_to_names,
            [col("workflow_to_index")],
            [col("workflow_to_index")],
            JoinArgs::new(JoinType::Left),
        )
        .join(
            doctype_names,
            [col("doctype_index")],
            [col("doctype_index")],
            JoinArgs::new(JoinType::Left),
        )
        .sort(["key", "slot"], SortMultipleOptions::default())
        .select(columns))
}

fn file_name(index: usize) -> String {
    format!("claims-{index:04}.parquet")
}

/// Reports whether `directory` already holds a generated claims table.
#[must_use]
pub fn claims_generated(directory: &Path) -> bool {
    directory.join(file_name(0)).is_file()
}

/// Writes a generated claims table into `directory` as Parquet files and
/// returns the paths written, in key order.
///
/// Existing files with the same names are replaced. Keys are never split
/// across files.
///
/// # Errors
///
/// Returns an error when the directory or a file cannot be written, or when
/// Polars cannot build or run the generating plan.
pub fn generate_claims(directory: &Path, spec: SampleSpec) -> Result<Vec<PathBuf>, EngineError> {
    fs::create_dir_all(directory)
        .map_err(|source| EngineError::io("creating the sample directory", source))?;
    let mut written = Vec::new();
    let mut first_key = 0i64;
    let total_keys = i64::from(spec.keys);
    while first_key < total_keys {
        let key_count = KEYS_PER_FILE.min(total_keys - first_key);
        let mut frame = claims_for_keys(first_key, key_count, spec.filler_columns)
            .and_then(LazyFrame::collect)
            .map_err(|source| EngineError::polars("generating sample claims", source))?;
        let path = directory.join(file_name(written.len()));
        let file = File::create(&path)
            .map_err(|source| EngineError::io("creating a sample file", source))?;
        ParquetWriter::new(file)
            .finish(&mut frame)
            .map_err(|source| EngineError::polars("writing a sample file", source))?;
        written.push(path);
        first_key += key_count;
    }
    Ok(written)
}
