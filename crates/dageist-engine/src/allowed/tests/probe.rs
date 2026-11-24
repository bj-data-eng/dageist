//! Running a combination: whether Polars accepts it, and why not.

use std::panic::{AssertUnwindSafe, catch_unwind};

use polars::prelude::{DataFrame, DataType, LazyFrame, PolarsError};

use crate::allowed::dtype::{Representative, ResultDtype, dtype_name};
use crate::allowed::table::{Outcome, Produces, Reason, Shape, Stage};
use crate::catalog::BuildError;

use super::sample::{Frames, SAMPLE_ROWS};

/// `text` without the number Polars counts its temporary columns by:
/// `_POLARS_TMP_PHYS_2512` is `_POLARS_TMP_PHYS_`.
fn without_counters(text: &str) -> String {
    let mut kept = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("_POLARS_") {
        kept.push_str(&rest[..at]);
        let name = &rest[at..];
        let end = name
            .find(|character: char| !(character.is_ascii_uppercase() || character == '_'))
            .unwrap_or(name.len());
        kept.push_str(&name[..end]);
        let after = &name[end..];
        let digits = after
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(after.len());
        rest = &after[digits..];
    }
    kept.push_str(rest);
    kept
}

/// Polars' reason, as the tables keep it: its message without the plan and the
/// expression it adds after a blank line, with each run of whitespace a single
/// space and no counter of temporary columns. Nothing in it is an address or a
/// time.
pub(super) fn normalised(message: &str) -> String {
    let head = message.split("\n\n").next().unwrap_or_default();
    without_counters(&head.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn polars_reason(stage: Stage, error: &PolarsError) -> Reason {
    Reason {
        stage,
        text: normalised(&error.to_string()),
    }
}

/// The message a panic carries.
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    format!("Polars panicked: {}", normalised(&message))
}

/// A call the generator built that the catalogue refuses for a reason of its
/// own, not Polars': a mistake of the generator, not a finding.
#[derive(Debug)]
pub(super) struct GeneratorBug(pub String);

/// What running a plan came to: the schema Polars resolved and the table it
/// gave on rows.
pub(super) struct Measured {
    pub schema: polars::prelude::SchemaRef,
    pub rows: DataFrame,
}

impl Measured {
    /// What the plan gives when it is one column: its dtype, and whether it is
    /// one value among the three rows or a value for each.
    pub(super) fn produces(&self) -> Option<Produces> {
        let mut columns = self.schema.iter();
        let (Some((_, dtype)), None) = (columns.next(), columns.next()) else {
            return None;
        };
        Some(Produces {
            result: ResultDtype::from_name(&dtype_name(dtype)),
            // The sample's tables have three rows. Another number of values than one or those is
            // a count that the data or the arguments decide.
            shape: match self.rows.height() {
                1 => Shape::One,
                SAMPLE_ROWS => Shape::Many,
                _ => Shape::Counted,
            },
        })
    }
}

/// Builds the plan to run on one of the sample tables.
pub(super) type Plan<'a> = &'a (dyn Fn(&Frames, bool) -> Result<LazyFrame, BuildError> + Sync);

fn built(plan: Plan<'_>, frames: &Frames, empty: bool, stage: Stage) -> Result<LazyFrame, Reason> {
    match plan(frames, empty) {
        Ok(frame) => Ok(frame),
        Err(BuildError::Polars(error)) => Err(polars_reason(stage, &error)),
        Err(other) => std::panic::panic_any(GeneratorBug(other.to_string())),
    }
}

fn run(plan: Plan<'_>, frames: &Frames) -> Result<Measured, Reason> {
    let mut frame = built(plan, frames, false, Stage::Schema)?;
    let schema = frame
        .collect_schema()
        .map_err(|error| polars_reason(Stage::Schema, &error))?;
    let rows = frame
        .collect()
        .map_err(|error| polars_reason(Stage::Rows, &error))?;
    let same = |left: &DataType, right: &DataType| {
        left == right
            || (Representative::of(left).is_some()
                && Representative::of(left) == Representative::of(right))
    };
    if rows.schema().len() != schema.len()
        || rows
            .schema()
            .iter()
            .zip(schema.iter())
            .any(|((_, planned), (_, ran))| !same(planned, ran))
    {
        return Err(Reason {
            stage: Stage::Rows,
            text: format!(
                "the schema says {} and the rows give {}",
                dtypes_of(&schema),
                dtypes_of(rows.schema())
            ),
        });
    }
    built(plan, frames, true, Stage::Empty)?
        .collect()
        .map_err(|error| polars_reason(Stage::Empty, &error))?;
    Ok(Measured { schema, rows })
}

fn dtypes_of(schema: &polars::prelude::Schema) -> String {
    schema
        .iter()
        .map(|(_, dtype)| dtype_name(dtype))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Runs `plan` on the sample tables: its schema, its rows and an empty table.
/// It is allowed only when all three succeed.
pub(super) fn measure(plan: Plan<'_>, frames: &Frames) -> Result<Measured, Reason> {
    crate::startup::ensure();
    catch_unwind(AssertUnwindSafe(|| run(plan, frames))).unwrap_or_else(|payload| {
        if let Some(bug) = payload.downcast_ref::<GeneratorBug>() {
            panic!("the generator built an invalid call: {}", bug.0);
        }
        Err(Reason {
            stage: Stage::Rows,
            text: panic_text(payload.as_ref()),
        })
    })
}

/// The outcome of a plan that gives one column.
pub(super) fn outcome(plan: Plan<'_>, frames: &Frames) -> Outcome {
    match measure(plan, frames) {
        Ok(measured) => Outcome::Allowed(measured.produces()),
        Err(reason) => Outcome::Refused(reason),
    }
}
