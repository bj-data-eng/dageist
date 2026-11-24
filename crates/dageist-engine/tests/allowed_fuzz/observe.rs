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
use super::frames::{Frames, key_column_index};

/// How a reason that is the target's own comparison of a step's schema with its
/// rows begins: it is no refusal of Polars, and shows only where the method that
/// the schema is of is the output of the step.
pub const SCHEMA_SAYS: &str = "the schema says";

/// How the reason begins when the sample table cannot be given a schema.
const NO_SCHEMA: &str = "the sample table has no schema";

/// Whether `reason` is something the target says of a case, from its own
/// comparison or its own tables, and not what Polars said when it refused it.
pub fn is_the_targets_own(reason: &str) -> bool {
    reason.starts_with(SCHEMA_SAYS) || reason.starts_with(NO_SCHEMA)
}

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
    /// Where in the document, when Polars said.
    pub place: Option<Place>,
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
    /// Whether a column of the other table is paired with keys that Polars gives different
    /// dtypes. Polars refuses the join for that when no dtype of the column fits them all, which
    /// the tables are not asked about, and joins whole numbers of different widths.
    pub keys_conflict: bool,
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
                place: None,
                reason: panic_text(payload.as_ref()),
            });
            return observed;
        }
        Ok(Err(error)) => {
            observed.refusal = Some(Refusal {
                stage: Stage::Schema,
                at: "the table".to_owned(),
                place: None,
                reason: format!("{NO_SCHEMA}: {error}"),
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
            place: Some(refusal.place().clone()),
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
            place: None,
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
        "{SCHEMA_SAYS} {} and the rows give {}",
        names(schema),
        names(ran)
    ))
}

/// Which key column of [`Frames`] `expression` is, when it is only that column:
/// the column the other side is matched against.
fn key_column_of(expression: &Expression) -> Option<usize> {
    if !expression.links().is_empty() {
        return None;
    }
    let Start::Call(call) = expression.start() else {
        return None;
    };
    if call.entry().id().as_str() != "polars.col" {
        return None;
    }
    match call.argument("name") {
        Some(Value::Columns(names)) => match names.as_slice() {
            [name] => key_column_index(name),
            _ => None,
        },
        _ => None,
    }
}

/// Whether `expression` is only a join key column the other side is matched
/// against.
pub fn is_bare_key(expression: &Expression) -> bool {
    key_column_of(expression).is_some()
}

/// The expressions a join key parameter holds, as a list of items.
fn key_expressions<'a>(document: &'a Document, parameter: &str) -> Vec<&'a Expression> {
    match document.steps()[0].call().argument(parameter) {
        Some(Value::Many(items)) => items
            .iter()
            .filter_map(|item| match item {
                Value::Expression(expression) => Some(expression),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The dtype of each key column the other side is matched against, which is the
/// dtype the chain it is paired with has: the join of a key with another dtype is
/// refused for that, which is no matter of the tables. It is found by building the
/// step once, as the report says what the chain has even when the join is
/// refused. A column that no pair makes is of whole numbers. A column that two
/// keys of different dtypes are paired with has the first's, and the join is said
/// to conflict.
fn key_dtypes(case: &Case, first: &Observed) -> (Vec<DataType>, bool) {
    let document = &case.document;
    let mut found: Vec<Option<DataType>> = Vec::new();
    let mut conflict = false;
    let sides = ["left_on", "right_on"];
    for (side, parameter) in sides.into_iter().enumerate() {
        let held_parameter = sides[1 - side];
        let (bare, held) = (
            key_expressions(document, parameter),
            key_expressions(document, held_parameter),
        );
        for (at, key) in bare.iter().enumerate() {
            let (Some(column), Some(partner)) = (key_column_of(key), held.get(at)) else {
                continue;
            };
            if key_column_of(partner).is_some() {
                continue;
            }
            let wanted = [
                Segment::Parameter(held_parameter.to_owned()),
                Segment::Item(at),
            ];
            let dtype = first
                .expressions
                .iter()
                .find(|expression| expression.place.path() == wanted)
                .and_then(|expression| {
                    match expression.links.last().unwrap_or(&expression.start) {
                        Seen::Dtype(dtype, ..) => Some(dtype.clone()),
                        _ => None,
                    }
                });
            if found.len() <= column {
                found.resize(column + 1, None);
            }
            if let (Some(known), Some(other)) = (&found[column], &dtype)
                && known != other
            {
                conflict = true;
            }
            found[column].get_or_insert(dtype.unwrap_or(DataType::Int64));
        }
    }
    if found.is_empty() {
        return (vec![DataType::Int64], conflict);
    }
    (
        found
            .into_iter()
            .map(|dtype| dtype.unwrap_or(DataType::Int64))
            .collect(),
        conflict,
    )
}

/// How many columns that stand in for a value a probe may read.
pub const MAX_STAND_INS: usize = 4;

/// How the tables a probe runs on differ from the case's.
#[derive(Debug, Clone, Copy, Default)]
pub struct Tables {
    /// The other table is the one the step starts from, for an expression that
    /// a join's `right_on` holds, which reads that table's columns.
    pub other_as_left: bool,
    /// A column of each of these classes stands in both tables, for a value of a
    /// dtype that the case's tables lack, named by
    /// [`stand_in_column`](super::frames::stand_in_column) in order.
    pub stand_ins: [Option<Representative>; MAX_STAND_INS],
    /// The rows both tables hold, which a control changes where a count is the rows by chance.
    pub rows: Rows,
}

/// Which rows the tables a case runs on hold.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Rows {
    /// The three rows of the sample.
    #[default]
    Sample,
    /// The three rows again, [`MORE_ROWS`] times over, one after another: twelve rows, where the
    /// values that differ and the values that occur most are as few as they were.
    Repeated,
    /// The second and third rows, twice over: four rows in which every value that the sample's
    /// columns hold twice or once occurs as often as the other one does, so that the values that
    /// occur most are two, and the values that differ are two, which no column's sample has.
    Tied,
}

/// How many times the three rows of [`Rows::Repeated`] are given.
pub const MORE_ROWS: usize = 4;

impl Tables {
    /// The tables with a column of each of `classes` standing in, or `None` for
    /// more than a probe may read.
    pub fn standing(other_as_left: bool, classes: &[Representative]) -> Option<Self> {
        if classes.len() > MAX_STAND_INS {
            return None;
        }
        let mut stand_ins = [None; MAX_STAND_INS];
        for (slot, class) in stand_ins.iter_mut().zip(classes) {
            *slot = Some(*class);
        }
        Some(Self {
            other_as_left,
            stand_ins,
            rows: Rows::Sample,
        })
    }

    fn of(self, case: &Case, keys: &[DataType]) -> Frames {
        let frames = Frames::new(case.receiver, case.argument, keys);
        let frames = if self.other_as_left {
            frames.other_as_left()
        } else {
            frames
        };
        let classes: Vec<Representative> = self.stand_ins.into_iter().flatten().collect();
        let frames = frames.with_stand_ins(&classes);
        match self.rows {
            Rows::Sample => frames,
            Rows::Repeated => frames.picked(&(0..MORE_ROWS).flat_map(|_| 0..3).collect::<Vec<_>>()),
            Rows::Tied => frames.picked(&[1, 2, 1, 2]),
        }
    }
}

/// What Polars does with a probe: the case's document, run once on the case's
/// tables as `tables` changes them.
pub fn observe_probe(case: &Case, tables: Tables) -> Observed {
    observe_on(case, tables)
}

pub fn observe(case: &Case) -> Observed {
    observe_on(case, Tables::default())
}

fn observe_on(case: &Case, tables: Tables) -> Observed {
    let observed = run_on(&case.document, &tables.of(case, &[DataType::Int64]));
    let is_join = case.document.steps()[0].call().entry().id().as_str() == "LazyFrame.join";
    if !is_join {
        return observed;
    }
    let (keys, conflict) = key_dtypes(case, &observed);
    if keys == [DataType::Int64] {
        return Observed {
            keys_conflict: conflict,
            ..observed
        };
    }
    Observed {
        keys_conflict: conflict,
        ..run_on(&case.document, &tables.of(case, &keys))
    }
}
