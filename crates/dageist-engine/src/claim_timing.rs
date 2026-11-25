//! The claim-timing sample node: the steps the step editor opens with, the
//! seven claim rows they run on and what else is connected to the node.
//!
//! It is the one copy of the sample, served by the host to the view and run by
//! the engine's tests. The steps sort a few claims by key and time, number
//! each key's steps, carry the time of the step before, work out the days a
//! claim stayed open and what an owner was responsible for, join the allowed
//! days of each step, flag late and holiday rows, and pivot the days of each
//! step into columns of one row for each key.

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use polars::prelude::{
    AnyValue, DataFrame, DataType, IntoLazy, ParquetWriter, PolarsResult, Scalar, StrptimeOptions,
    TimeUnit, col, df, lit,
};

use crate::document::{
    ColumnDescription, Document, Inputs, ListOrigin, Origin, Preview, Report, RowsError, StepId,
};
use crate::error::EngineError;

/// The seven claim rows, in no useful order, as text. Two claims, `A-1001` and
/// `A-1002`, each with the steps it went through, the owner it was with and
/// the days it spent there.
const TIMESTAMPS: [&str; 7] = [
    "2026-03-02 13:30:00",
    "2026-03-05 07:50:00",
    "2026-03-06 10:00:00",
    "2026-03-02 09:15:00",
    "2026-03-04 17:20:00",
    "2026-03-09 11:05:00",
    "2026-03-03 16:40:00",
];

/// The claim-timing sample node.
#[derive(Debug)]
pub struct ClaimTiming {
    source: DataFrame,
    source_origin: Origin,
    inputs: Inputs,
    steps: Document,
}

impl ClaimTiming {
    /// The name of the table the steps start from, as the view calls it.
    pub const SOURCE_NAME: &'static str = "claims";

    /// The steps the editor opens with, as the JSON text of the sample's own
    /// file: a [document](Document) of version 1.
    pub const STEPS_JSON: &'static str = include_str!("claim_timing/steps.json");

    /// The sample node.
    ///
    /// # Errors
    ///
    /// When Polars cannot build the sample's tables, or the steps are not a
    /// document the engine reads. Neither happens in a built program: the
    /// engine's tests build it.
    pub fn new() -> Result<Self, EngineError> {
        crate::startup::ensure();
        Ok(Self {
            source: source().map_err(|error| EngineError::polars("building the claims", error))?,
            source_origin: Origin::Node("Claims".to_owned()),
            inputs: inputs()?,
            steps: Document::read(Self::STEPS_JSON).map_err(|error| {
                EngineError::request("reading the sample steps", error.to_string())
            })?,
        })
    }

    /// The table the steps start from: the working window, which is all seven
    /// claim rows.
    #[must_use]
    pub const fn source(&self) -> &DataFrame {
        &self.source
    }

    /// Writes the sample's tables as Parquet files in `directory`, which is
    /// made when it is missing, so that a graph can read them as sources.
    ///
    /// # Errors
    ///
    /// When the directory or a file cannot be written.
    pub fn write_tables(directory: &Path) -> Result<SampleFiles, EngineError> {
        crate::startup::ensure();
        fs::create_dir_all(directory)
            .map_err(|error| EngineError::io("making the sample's folder", error))?;
        let write = |name: &str, table: PolarsResult<DataFrame>| {
            let polars = |error| EngineError::polars("writing a sample table", error);
            let mut table = table.map_err(polars)?;
            let path = directory.join(format!("{name}.parquet"));
            let file = File::create(&path)
                .map_err(|error| EngineError::io("writing a sample table", error))?;
            ParquetWriter::new(file)
                .finish(&mut table)
                .map_err(polars)?;
            Ok::<_, EngineError>(path)
        };
        Ok(SampleFiles {
            claims: write("claims", source())?,
            step_sla: write("step_sla", step_sla())?,
            step_names: write("step_names", step_names())?,
            holidays: write("holidays", holidays())?,
        })
    }

    /// The columns of [`source`](Self::source) with their dtypes.
    #[must_use]
    pub fn source_columns(&self) -> Vec<ColumnDescription> {
        ColumnDescription::of_schema(self.source.schema())
    }

    /// What supplies [`source`](Self::source) to the node.
    #[must_use]
    pub const fn source_origin(&self) -> &Origin {
        &self.source_origin
    }

    /// What else is connected to the node: the table of the days each step may
    /// take, the table of step names a pivot gives its columns from, a cutoff
    /// and an as-of date, and a list of holidays.
    #[must_use]
    pub const fn inputs(&self) -> &Inputs {
        &self.inputs
    }

    /// The steps the editor opens with.
    #[must_use]
    pub const fn steps(&self) -> &Document {
        &self.steps
    }

    /// Runs `document` on the node: its first step on [`source`](Self::source),
    /// with [`inputs`](Self::inputs) connected, counting the rows each step
    /// keeps. It is [`Document::run_on`].
    ///
    /// # Errors
    ///
    /// When Polars cannot work out the schema of the source.
    pub fn run(&self, document: &Document) -> PolarsResult<Report> {
        document.run_on(&self.inputs, &self.source)
    }

    /// The rows after the step `through` of `document`, at most `limit`, as
    /// text, with the steps run on the node. It is [`Document::preview`].
    ///
    /// # Errors
    ///
    /// As [`Document::preview`] is.
    pub fn preview(
        &self,
        document: &Document,
        through: &StepId,
        limit: usize,
    ) -> Result<Preview, RowsError> {
        document.preview(&self.inputs, &self.source, through, limit)
    }
}

fn source() -> PolarsResult<DataFrame> {
    df!(
        "dcn" => ["A-1002", "A-1001", "A-1002", "A-1001", "A-1002", "A-1001", "A-1001"],
        "timestamp" => TIMESTAMPS,
        "step_to" => ["Intake", "Pending", "Complete", "Intake", "Review", "Complete", "Review"],
        "owner" => [
            "Company A", "Company A", "Company B", "Company A",
            "Company A", "Company B", "Company B",
        ],
        "row_days" => [0_i64, 2, 2, 0, 2, 2, 1],
    )?
    .lazy()
    .with_column(col("timestamp").str().to_datetime(
        Some(TimeUnit::Microseconds),
        None,
        StrptimeOptions::default(),
        lit("raise"),
    ))
    .collect()
}

/// The allowed days of each step.
fn step_sla() -> PolarsResult<DataFrame> {
    df!(
        "step" => ["Intake", "Review", "Pending", "Complete"],
        "sla_days" => [1_i64, 1, 2, 3],
    )
}

/// The names of the steps.
fn step_names() -> PolarsResult<DataFrame> {
    df!("step_to" => ["Intake", "Review", "Pending", "Complete"])
}

/// The holiday calendar: one column of dates, `date`.
fn holidays() -> PolarsResult<DataFrame> {
    dates(&["2026-03-03", "2026-03-05"])
}

/// The files [`ClaimTiming::write_tables`] writes the sample's tables to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleFiles {
    /// The claim rows the steps start from.
    pub claims: PathBuf,
    /// The allowed days of each step: `step` and `sla_days`.
    pub step_sla: PathBuf,
    /// The names of the steps: `step_to`.
    pub step_names: PathBuf,
    /// The holiday calendar: `date`.
    pub holidays: PathBuf,
}

/// The dates in `texts`, as a column of dtype `Date`.
fn dates(texts: &[&str]) -> PolarsResult<DataFrame> {
    df!("date" => texts)?
        .lazy()
        .with_column(col("date").str().to_date(StrptimeOptions::default()))
        .collect()
}

/// A single date, from its text.
fn date_value(text: &str) -> PolarsResult<Scalar> {
    let frame = dates(&[text])?;
    Ok(Scalar::new(
        DataType::Date,
        frame.column("date")?.get(0)?.into_static(),
    ))
}

fn inputs() -> Result<Inputs, EngineError> {
    let polars = |error| EngineError::polars("building the sample inputs", error);
    let duplicate = |error: crate::document::DuplicateInput| {
        EngineError::request("building the sample inputs", error.to_string())
    };
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "step_sla",
            "SLA by step",
            step_sla().map_err(polars)?.lazy(),
        )
        .map_err(duplicate)?;
    inputs
        .add_table(
            "step_names",
            "Step names",
            step_names().map_err(polars)?.lazy(),
        )
        .map_err(duplicate)?;
    inputs
        .add_value(
            "sla_cutoff",
            Scalar::new(DataType::Int64, AnyValue::Int64(1)),
        )
        .map_err(duplicate)?;
    inputs
        .add_value("as_of", date_value("2026-03-02").map_err(polars)?)
        .map_err(duplicate)?;
    inputs
        .add_list(
            "holidays",
            ListOrigin::Column {
                node: "Holiday calendar".to_owned(),
                column: "date".to_owned(),
            },
            holidays()
                .and_then(|frame| {
                    frame
                        .column("date")
                        .map(|column| column.as_materialized_series().clone())
                })
                .map_err(polars)?,
        )
        .map_err(duplicate)?;
    Ok(inputs)
}
