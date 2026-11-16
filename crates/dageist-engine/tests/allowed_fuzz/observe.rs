//! What Polars does with a case, through the step document's own runner: the
//! step built and its schema worked out ([`Document::run`]), then its rows on
//! three rows with a null in them ([`Document::rows`]), then on a table with no
//! rows. A case is allowed only when all three succeed, as the tables count it.

use std::panic::{AssertUnwindSafe, catch_unwind};

use dageist_engine::allowed::Representative;
use dageist_engine::document::{
    Document, Expression, Outcome, Place, Resolution, Segment, Start, Value,
};
use polars::prelude::{DataType, IntoLazy};

use super::case::Case;
use super::frames::Frames;

/// Where Polars refused a case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Building the step or working out its schema.
    Schema,
    /// Collecting the rows.
    Rows,
    /// Collecting a table with no rows.
    Empty,
}

impl Stage {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Rows => "rows",
            Self::Empty => "empty",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub stage: Stage,
    pub at: String,
    pub reason: String,
}

/// What an expression had at its start or after a link, when it was built
/// against the table's schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    /// A single column of this dtype, its name as the tables write it, and
    /// the class it is of.
    Dtype(DataType, String, Option<Representative>),
    /// Something the tables say nothing of: a condition that is not finished,
    /// several columns, or an expression from `element`, which has no dtype
    /// outside a pivot.
    Unsaid,
    /// Polars refused the expression up to here.
    Refused(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenExpression {
    pub place: Place,
    pub start: Seen,
    pub links: Vec<Seen>,
}

#[derive(Debug, Clone, Default)]
pub struct Observed {
    /// Why Polars refused the case, if it did.
    pub refusal: Option<Refusal>,
    /// The dtypes of the expressions, as far as they could be built.
    pub expressions: Vec<SeenExpression>,
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .map(|text| format!("Polars panicked: {}", first_line(&text)))
        .unwrap_or_else(|| "Polars panicked".to_owned())
}

fn first_line(text: &str) -> String {
    text.split("\n\n")
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn seen(resolution: &Resolution) -> Seen {
    match resolution {
        Resolution::Dtype(dtype) => Seen::Dtype(
            dtype.clone(),
            dageist_engine::allowed::dtype_name(dtype),
            Representative::of(dtype),
        ),
        Resolution::Several(_) | Resolution::Unfinished | Resolution::NoContext => Seen::Unsaid,
        Resolution::Refused(error) => Seen::Refused(first_line(&error.to_string())),
    }
}

/// Runs the case's one step on `frames`: the schema, then the rows, then the
/// empty table.
fn run_on(document: &Document, frames: &Frames) -> Observed {
    let id = document.steps()[0].id().clone();
    let mut observed = Observed::default();
    let rows_inputs = frames.inputs(false);
    let report = catch_unwind(AssertUnwindSafe(|| {
        document.run(&rows_inputs, frames.rows.clone().lazy())
    }));
    let report = match report {
        Err(payload) => {
            observed.refusal = Some(Refusal {
                stage: Stage::Schema,
                at: "the step".to_owned(),
                reason: panic_text(payload.as_ref()),
            });
            return observed;
        }
        Ok(Err(error)) => {
            observed.refusal = Some(Refusal {
                stage: Stage::Schema,
                at: "the table".to_owned(),
                reason: format!("the sample table has no schema: {error}"),
            });
            return observed;
        }
        Ok(Ok(report)) => report,
    };
    let step = &report.steps()[0];
    observed.expressions = step
        .expressions()
        .iter()
        .map(|expression| SeenExpression {
            place: expression.place().clone(),
            start: seen(expression.start()),
            links: expression.links().iter().map(seen).collect(),
        })
        .collect();
    if let Outcome::Refused(refusal) = step.outcome() {
        observed.refusal = Some(Refusal {
            stage: Stage::Schema,
            at: refusal.place().to_string(),
            reason: first_line(&refusal.cause().to_string()),
        });
        return observed;
    }
    for (stage, inputs, working) in [
        (Stage::Rows, rows_inputs, &frames.rows),
        (Stage::Empty, frames.inputs(true), &frames.empty),
    ] {
        let collected = catch_unwind(AssertUnwindSafe(|| {
            document.rows(&inputs, working, &id, 100)
        }));
        let reason = match collected {
            Err(payload) => panic_text(payload.as_ref()),
            Ok(Err(error)) => first_line(&error.to_string()),
            Ok(Ok(rows)) => match (stage.clone(), step.schema()) {
                // The tables count a schema that the rows do not give as a
                // refusal, for what the schema says is then not what runs.
                (Stage::Rows, Some(schema)) => match schema_disagrees(schema, &rows) {
                    Some(reason) => reason,
                    None => continue,
                },
                _ => continue,
            },
        };
        observed.refusal = Some(Refusal {
            stage,
            at: "the rows".to_owned(),
            reason,
        });
        return observed;
    }
    observed
}

/// What the schema says and the rows give, when they are not the same dtypes
/// of the same columns, as the tables count it: two dtypes are the same when
/// they are equal or of one class.
fn schema_disagrees(
    schema: &polars::prelude::Schema,
    rows: &polars::prelude::DataFrame,
) -> Option<String> {
    let same = |planned: &DataType, ran: &DataType| {
        planned == ran
            || (Representative::of(planned).is_some()
                && Representative::of(planned) == Representative::of(ran))
    };
    let ran = rows.schema();
    if schema.len() == ran.len()
        && schema
            .iter()
            .zip(ran.iter())
            .all(|((_, planned), (_, ran))| same(planned, ran))
    {
        return None;
    }
    let names = |schema: &polars::prelude::Schema| {
        schema
            .iter()
            .map(|(_, dtype)| dageist_engine::allowed::dtype_name(dtype))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Some(format!(
        "the schema says {} and the rows give {}",
        names(schema),
        names(ran)
    ))
}

/// Whether `expression` is only the join key column the other side is
/// matched against.
fn is_bare_key(expression: &Expression) -> bool {
    expression.links().is_empty()
        && matches!(expression.start(), Start::Call(call)
            if call.entry().id().as_str() == "polars.col"
                && matches!(call.argument("name"), Some(Value::Columns(names)) if *names == ["key"]))
}

/// The expression a join key parameter holds, as a single item.
fn key_expression<'a>(document: &'a Document, parameter: &str) -> Option<&'a Expression> {
    match document.steps()[0].call().argument(parameter)? {
        Value::Many(items) => match items.as_slice() {
            [Value::Expression(expression)] => Some(expression),
            _ => None,
        },
        _ => None,
    }
}

/// The dtype the other table's `key` must have for a join to be made on the
/// chain the case puts in a key, which is the dtype the chain has: the join
/// of a key with another dtype is refused for that, which is no matter of the
/// tables. It is found by building the step once, as the report says what the
/// chain has even when the join is refused.
fn key_dtype(case: &Case, first: &Observed) -> DataType {
    let document = &case.document;
    let held = ["left_on", "right_on"].into_iter().find(|parameter| {
        key_expression(document, parameter).is_some_and(|expression| !is_bare_key(expression))
    });
    let Some(parameter) = held else {
        return DataType::Int64;
    };
    let wanted = [Segment::Parameter(parameter.to_owned()), Segment::Item(0)];
    first
        .expressions
        .iter()
        .find(|expression| expression.place.path() == wanted)
        .and_then(
            |expression| match expression.links.last().unwrap_or(&expression.start) {
                Seen::Dtype(dtype, ..) => Some(dtype.clone()),
                _ => None,
            },
        )
        .unwrap_or(DataType::Int64)
}

pub fn observe(case: &Case) -> Observed {
    let frames = Frames::new(case.receiver, case.argument, &DataType::Int64);
    let observed = run_on(&case.document, &frames);
    let is_join = case.document.steps()[0].call().entry().id().as_str() == "LazyFrame.join";
    if !is_join {
        return observed;
    }
    let key = key_dtype(case, &observed);
    if key == DataType::Int64 {
        return observed;
    }
    run_on(
        &case.document,
        &Frames::new(case.receiver, case.argument, &key),
    )
}
