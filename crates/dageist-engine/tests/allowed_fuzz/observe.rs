//! What Polars does with a case, through the step document's own runner: the
//! step built and its schema worked out ([`Document::run`]), then its rows on
//! three rows with a null in them ([`Document::rows`]), then on a table with no
//! rows. A case is allowed only when all three succeed, as the tables count it. A step the walk
//! refuses is not run by the step document, so [`asked`] asks Polars of it directly, the same
//! three times.

use std::panic::{AssertUnwindSafe, catch_unwind};

use dageist_engine::allowed::Representative;
use dageist_engine::document::{
    Document, Expression, Outcome, Place, Resolution, RunCause, Segment, Start, Value,
};
use polars::prelude::{DataType, Engine, IntoLazy};

use super::ask_polars::{self, Connected};
use super::asked::{self, Walked};
use super::case::Case;
use super::frames::{Frames, key_column_index};
use super::structs::Structs;

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
    /// Where the step engine and [`asked`] differ in what they say of a step the walk allows, when
    /// the case was observed with that check: they must say the same, or what the target takes of a
    /// step the walk refuses, which only [`asked`] runs, is not what the engine would say.
    pub drift: Option<String>,
}

pub fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
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
///
/// The step engine does not run a step whose walk is refused, so Polars is asked of such a step
/// directly, by [`asked`]: that is how a table that is stricter than Polars is found. Where the
/// walk allows the step and `check` is set, [`asked`] is asked of it too, and the two answers are
/// compared, since [`asked`] copies what the engine builds.
fn run_on(document: &Document, frames: &Frames, check: bool) -> Observed {
    let walked = asked::walked(document, frames);
    if walked == Walked::AskPolars
        && let Some(observed) = asked::ask(document, frames)
    {
        return observed;
    }
    let (mut observed, guarded) = run_on_engine(document, frames);
    if check
        && walked == Walked::Allows
        && !guarded
        && let Some(asked) = asked::ask(document, frames)
    {
        observed.drift = drift_between(&observed, &asked);
    }
    observed
}

/// How the answer of the step engine and the answer of [`asked`] to a step the walk allows
/// differ, if they do: in whether and where Polars refuses it, and in the dtypes it gives each
/// expression, which both report.
pub fn drift_between(engine: &Observed, asked: &Observed) -> Option<String> {
    // Polars names the values that a row failed on, and which row it meets first is not the same
    // from run to run: the words of a refusal of the rows are compared by their shape, with what
    // is between brackets, which is the values, left out, and every digit the same. A refusal of
    // the schema is compared exactly, for there a dtype or a width that differs by a digit is a
    // difference.
    let shaped = |refusal: &Option<Refusal>| {
        refusal.as_ref().map(|refusal| Refusal {
            reason: if refusal.stage == Stage::Schema {
                refusal.reason.clone()
            } else {
                one_wording_of_a_replacement(&shape_of_a_failure_of_rows(&refusal.reason))
            },
            ..refusal.clone()
        })
    };
    if shaped(&engine.refusal) != shaped(&asked.refusal) {
        return Some(format!(
            "the step engine says {:?} and the asker says {:?}",
            engine.refusal, asked.refusal
        ));
    }
    if engine.expressions != asked.expressions {
        return Some(format!(
            "the step engine gives the expressions {:?} and the asker gives {:?}",
            engine.expressions, asked.expressions
        ));
    }
    None
}

/// `reason` with the refusal of a text replacement of more than the first match worded as a regex
/// replacement, which Polars words as a multivalue replacement where it splits its work another
/// way, so that the two wordings of the one refusal compare as the same.
fn one_wording_of_a_replacement(reason: &str) -> String {
    reason
        .strip_prefix("multivalue replacement with 'n > ")
        .map_or_else(
            || reason.to_owned(),
            |rest| format!("regex replacement with 'n > {rest}"),
        )
}

/// `reason` with the values of the rows it names, which are between brackets, left out and every
/// other digit made the same.
fn shape_of_a_failure_of_rows(reason: &str) -> String {
    let mut shaped = String::with_capacity(reason.len());
    let mut depth = 0_usize;
    for character in reason.chars() {
        match character {
            '[' => {
                depth += 1;
                if depth == 1 {
                    shaped.push_str("[#]");
                }
            }
            ']' if depth > 0 => depth -= 1,
            _ if depth > 0 => {}
            digit if digit.is_ascii_digit() => shaped.push('#'),
            other => shaped.push(other),
        }
    }
    shaped
}

/// What the step engine does with the case's one step on `frames`, and whether it refused the
/// step by what it holds of its own, which Polars is not asked of.
fn run_on_engine(document: &Document, frames: &Frames) -> (Observed, bool) {
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
            return (observed, false);
        }
        Ok(Err(error)) => {
            observed.refusal = Some(Refusal {
                stage: Stage::Schema,
                at: "the table".to_owned(),
                place: None,
                reason: format!("{NO_SCHEMA}: {error}"),
            });
            return (observed, false);
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
        return (observed, matches!(refusal.cause(), RunCause::NotAllowed(_)));
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
        return (observed, false);
    }
    (observed, false)
}

/// What the schema says and the rows give, when they are not the same dtypes
/// of the same columns, as the tables count it: two dtypes are the same when
/// they are equal or of one class.
pub fn schema_disagrees(
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
        let frames = Frames::new(case.receiver, case.argument, &case.structs, keys);
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
    observe_on(case, tables, false)
}

/// Why Polars' in-memory engine refuses the first rows of the case's step on the sample, which the
/// step engine runs on the streaming engine, or `None` where it runs them: the control of a case
/// that one of the two engines runs and the other refuses.
pub fn refused_in_memory(case: &Case) -> Option<String> {
    let frames = Tables::default().of(case, &[DataType::Int64]);
    let tables = |name: &str| frames.table(name, false);
    catch_unwind(AssertUnwindSafe(|| {
        ask_polars::collect_on(
            Engine::InMemory,
            &case.document.steps()[0],
            0,
            &frames.rows,
            Connected::tables(&tables),
            100,
        )
    }))
    .ok()?
    .err()
}

pub fn observe(case: &Case) -> Observed {
    observe_on(case, Tables::default(), false)
}

/// What Polars does with the case, as [`observe`] says, and whether the step engine and [`asked`]
/// differ in what they say of a step the walk allows, in [`Observed::drift`].
pub fn observe_checked(case: &Case) -> Observed {
    observe_on(case, Tables::default(), true)
}

fn observe_on(case: &Case, tables: Tables, check: bool) -> Observed {
    let observed = run_on(&case.document, &tables.of(case, &[DataType::Int64]), check);
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
    let drift = observed.drift;
    let second = run_on(&case.document, &tables.of(case, &keys), check);
    Observed {
        keys_conflict: conflict,
        drift: drift.or(second.drift.clone()),
        ..second
    }
}

#[cfg(test)]
mod tests {
    //! The answer of [`asked`] to a step is the step engine's where the walk allows the step, and
    //! it keeps the checks of a pattern, a duration and a time zone that the engine makes before
    //! it asks Polars, which a step the walk refuses never meets in the engine.

    use dageist_engine::allowed::Representative;
    use dageist_engine::document::Segment;

    use super::super::predict::{Source, Verdict, predict};
    use super::*;

    fn refusal(stage: Stage, reason: &str) -> Option<Refusal> {
        Some(Refusal {
            stage,
            at: "the rows".to_owned(),
            place: None,
            reason: reason.to_owned(),
        })
    }

    #[test]
    fn two_answers_that_differ_in_what_polars_refuses_or_in_a_dtype_are_a_drift() {
        let said = Observed {
            refusal: refusal(Stage::Rows, "conversion of 1 failed"),
            expressions: vec![SeenExpression {
                place: Place::of_step(0, None),
                start: Seen::Unsaid,
                links: vec![Seen::Dtype(DataType::Int64, "Int64".to_owned(), None)],
            }],
            ..Observed::default()
        };
        assert_eq!(drift_between(&said, &said.clone()), None);
        // The value a row failed on is not the same from run to run.
        let other_row = Observed {
            refusal: refusal(Stage::Rows, "conversion of 2 failed"),
            ..said.clone()
        };
        assert_eq!(drift_between(&said, &other_row), None);
        // A refusal of the schema that differs by a digit, as `i32` and `i64` do, is a difference.
        let of_the_schema = |reason: &str| Observed {
            refusal: refusal(Stage::Schema, reason),
            ..said.clone()
        };
        assert_eq!(
            drift_between(&of_the_schema("not `i32`"), &of_the_schema("not `i32`")),
            None
        );
        assert!(drift_between(&of_the_schema("not `i32`"), &of_the_schema("not `i64`")).is_some());
        // Nor are the values it failed on, whatever they are written as.
        let other_value = Observed {
            refusal: refusal(Stage::Rows, "conversion of 1 failed in column 'b': [inf]"),
            ..said.clone()
        };
        let the_nan = Observed {
            refusal: refusal(Stage::Rows, "conversion of 1 failed in column 'b': [NaN]"),
            ..said.clone()
        };
        assert_eq!(drift_between(&other_value, &the_nan), None);
        let other_stage = Observed {
            refusal: refusal(Stage::Empty, "conversion of 1 failed"),
            ..said.clone()
        };
        assert!(drift_between(&said, &other_stage).is_some());
        let other_words = Observed {
            refusal: refusal(Stage::Rows, "another failure"),
            ..said.clone()
        };
        assert!(drift_between(&said, &other_words).is_some());
        let allowed = Observed {
            refusal: None,
            ..said.clone()
        };
        assert!(drift_between(&said, &allowed).is_some());
        let other_dtype = Observed {
            expressions: vec![SeenExpression {
                place: Place::of_step(0, None),
                start: Seen::Unsaid,
                links: vec![Seen::Dtype(DataType::Int32, "Int32".to_owned(), None)],
            }],
            ..said.clone()
        };
        assert!(drift_between(&said, &other_dtype).is_some());
        let no_expressions = Observed {
            expressions: Vec::new(),
            ..said.clone()
        };
        assert!(drift_between(&said, &no_expressions).is_some());
    }

    #[test]
    fn the_two_wordings_of_a_refusal_of_a_replacement_of_more_than_the_first_match_are_no_drift() {
        // Polars 2.2.0 words the refusal of `str.replace` with `n` above 1 as a regex replacement
        // or as a multivalue one, by how it splits the work, which is not the same from run to run
        // on two threads: it is the same refusal.
        let said = |reason: &str| Observed {
            refusal: refusal(Stage::Rows, reason),
            ..Observed::default()
        };
        let regex = said("regex replacement with 'n > 1' not yet supported");
        let multivalue = said("multivalue replacement with 'n > 1' not yet supported");
        assert_eq!(drift_between(&regex, &multivalue), None);
        assert_eq!(drift_between(&multivalue, &regex), None);
        // Another refusal is a difference.
        let other_words = said("replacement with 'n > 1' is not wanted");
        assert!(drift_between(&regex, &other_words).is_some());
    }

    #[test]
    fn the_asker_and_the_engine_say_the_same_of_every_kept_case_the_walk_allows() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/allowed_fuzz/regressions");
        let mut files: Vec<_> = std::fs::read_dir(directory)
            .expect("the kept cases")
            .map(|entry| entry.expect("an entry").path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "case")
            })
            .collect();
        files.sort();
        let (mut compared, mut refused_by_polars, mut dtypes) = (0, 0, 0);
        let mut differ = Vec::new();
        for path in files {
            let case = Case::decode(&std::fs::read(&path).expect("a kept case")).expect("a case");
            if predict(&case).verdict != Verdict::Allows {
                continue;
            }
            let observed = observe_checked(&case);
            compared += 1;
            refused_by_polars += usize::from(observed.refusal.is_some());
            dtypes += usize::from(
                observed
                    .expressions
                    .iter()
                    .any(|expression| !expression.links.is_empty()),
            );
            if let Some(drift) = observed.drift {
                differ.push(format!("{}: {drift}", path.display()));
            }
        }
        assert!(differ.is_empty(), "{}", differ.join("\n"));
        // The comparison is made of cases that run, that Polars refuses, and that give dtypes.
        assert!(
            compared >= 20,
            "{compared} kept cases are allowed by the walk"
        );
        assert!(
            refused_by_polars > 0 && dtypes > 0,
            "{refused_by_polars}, {dtypes}"
        );
    }

    /// The case of the one step `call` selects on a column `a` of `receiver`.
    fn selecting(receiver: &str, link: &str) -> Case {
        let receiver = Representative::from_name(receiver).expect("a class");
        let text = format!(
            r#"{{"version":1,"steps":[{{"id":"s","call":{{"entry":"LazyFrame.select","arguments":{{"exprs":{{"many":[{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["a"]}}}}}},"then":[{link}]}}}}]}}}}}}}}]}}"#
        );
        Case {
            receiver,
            argument: Representative::String,
            structs: Structs::default(),
            document: Document::read(&text).expect("a document"),
        }
    }

    /// What the tables refuse of the case by a rule, which names it, and the place at which the
    /// asker, which the target asks of Polars apart from the engine, refuses it.
    fn refused_by(case: &Case) -> (&'static str, Option<Place>) {
        let Verdict::Refuses(refused) = predict(case).verdict else {
            panic!("the tables refuse the case");
        };
        let Source::Rule(rule) = refused.source else {
            panic!("a rule refuses the case: {:?}", refused.source);
        };
        let observed = observe(case);
        let asked = observed.refusal.expect("the asker refuses the case");
        assert_eq!(asked.stage, Stage::Schema, "{}", asked.reason);
        (rule, asked.place)
    }

    fn parameter(place: &Option<Place>, name: &str) -> bool {
        place
            .as_ref()
            .and_then(|place| place.path().last())
            .is_some_and(|last| *last == Segment::Parameter(name.to_owned()))
    }

    #[test]
    fn the_asker_refuses_a_pattern_a_duration_and_a_time_zone_that_are_none_at_their_parameter() {
        // Polars refuses a pattern only when the rows run, and panics on some durations, so the
        // asker asks them of Polars before it builds the step, as the engine does.
        let (rule, place) = refused_by(&selecting(
            "String",
            r#"{"entry":"Expr.str.contains","arguments":{"pattern":{"pattern":"(Comp"}}}"#,
        ));
        assert_eq!(rule, "pattern-is-valid");
        assert!(parameter(&place, "pattern"), "{place:?}");
        let (rule, place) = refused_by(&selecting(
            "String",
            r#"{"entry":"Expr.str.split","arguments":{"by":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"string":"("}}},"then":[]}},"literal":{"flag":false}}}"#,
        ));
        assert_eq!(rule, "pattern-is-valid");
        assert!(parameter(&place, "by"), "{place:?}");
        for (entry, name) in [
            ("Expr.dt.truncate", "every"),
            ("Expr.dt.round", "every"),
            ("Expr.dt.offset_by", "by"),
        ] {
            let (rule, place) = refused_by(&selecting(
                "Datetime(us)",
                &format!(
                    r#"{{"entry":"{entry}","arguments":{{"{name}":{{"expression":{{"start":{{"entry":"polars.lit","arguments":{{"value":{{"string":"1x"}}}}}},"then":[]}}}}}}}}"#
                ),
            ));
            assert_eq!(rule, "duration-is-valid", "{entry}");
            assert!(parameter(&place, name), "{entry}: {place:?}");
        }
        let (rule, place) = refused_by(&selecting(
            "Datetime(us)",
            r#"{"entry":"Expr.dt.replace_time_zone","arguments":{"time_zone":{"time_zone":"Not/AZone"}}}"#,
        ));
        assert_eq!(rule, "time-zone-is-valid");
        assert!(parameter(&place, "time_zone"), "{place:?}");
    }

    fn kept(name: &str) -> Case {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/allowed_fuzz/regressions")
            .join(name);
        Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case")
    }

    fn splitting_by_a_bracket(literal: &str) -> Case {
        selecting(
            "String",
            &format!(
                r#"{{"entry":"Expr.str.split","arguments":{{"by":{{"expression":{{"start":{{"entry":"polars.lit","arguments":{{"value":{{"string":"("}}}}}},"then":[]}}}}{literal}}}}}"#
            ),
        )
    }

    #[test]
    fn a_separator_that_is_plain_text_by_the_flag_is_allowed_by_the_engine_and_the_asker() {
        // `(` is no regular expression, and `literal` makes it text.
        let case = splitting_by_a_bracket(r#","literal":{"flag":true}"#);
        assert_eq!(predict(&case).verdict, Verdict::Allows);
        let observed = observe_checked(&case);
        assert_eq!((&observed.refusal, &observed.drift), (&None, &None));
        // Both say so for real: the asker, run apart from the engine, allows it as well.
        let frames = Frames::with_key(
            case.receiver,
            case.argument,
            &case.structs,
            Some(&[DataType::Int64]),
        );
        let asked = asked::ask(&case.document, &frames).expect("a step that can be asked");
        assert_eq!(asked.refusal, None);
    }

    #[test]
    fn a_separator_is_plain_text_by_the_default_of_literal_and_a_pattern_refused_at_it_where_it_is_false()
     {
        // The default of `literal` of `split` is plain text, so `(` is a separator like another,
        // and the engine and the asker allow it.
        let default = splitting_by_a_bracket("");
        assert_eq!(predict(&default).verdict, Verdict::Allows);
        let observed = observe_checked(&default);
        assert_eq!((&observed.refusal, &observed.drift), (&None, &None));
        // Where the flag is false it is a pattern, which `(` is not, and both refuse it at `by`.
        let pattern = splitting_by_a_bracket(r#","literal":{"flag":false}"#);
        let (rule, place) = refused_by(&pattern);
        assert_eq!(rule, "pattern-is-valid");
        assert!(parameter(&place, "by"), "{place:?}");
    }

    /// The case of `dt.truncate` by the text `every` of a column `a` of `receiver`.
    fn truncating(receiver: &str, every: &str) -> Case {
        selecting(
            receiver,
            &format!(
                r#"{{"entry":"Expr.dt.truncate","arguments":{{"every":{{"expression":{{"start":{{"entry":"polars.lit","arguments":{{"value":{{"string":"{every}"}}}}}},"then":[]}}}}}}}}"#
            ),
        )
    }

    #[test]
    fn a_duration_is_asked_for_the_moment_it_is_given_to_by_the_asker_as_by_the_engine() {
        // Polars cuts a date by whole days only, and a moment in a zone by no length of days and
        // hours together, where a moment with no zone takes both: what the asker refuses of a
        // duration depends on the dtype of the value before the call, as the engine's does.
        let (rule, place) = refused_by(&truncating("Date", "2d12h"));
        assert_eq!(rule, "duration-is-valid");
        assert!(parameter(&place, "every"), "{place:?}");
        // A date in a zone is a moment made of parts, as `polars.datetime` makes one.
        let (rule, place) = refused_by(&kept(
            "fixed-a-length-of-days-and-hours-for-a-moment-in-a-zone.case",
        ));
        assert_eq!(rule, "duration-is-valid");
        assert!(parameter(&place, "every"), "{place:?}");
        // A moment with no zone takes it, so the step is one the walk allows, and the engine and
        // the asker agree. A moment chosen for the asker by the dtype where the duration is read
        // could not be told from the engine's by a step the walk allows, but by this one: a date
        // would refuse it.
        let plain = truncating("Datetime(us)", "2d12h");
        assert_eq!(predict(&plain).verdict, Verdict::Allows);
        let observed = observe_checked(&plain);
        assert_eq!((&observed.refusal, &observed.drift), (&None, &None));
    }

    #[test]
    fn the_asker_takes_a_pattern_that_a_flag_makes_plain_text_and_a_duration_that_is_one() {
        let plain = selecting(
            "String",
            r#"{"entry":"Expr.str.contains","arguments":{"pattern":{"pattern":"("},"literal":{"flag":true}}}"#,
        );
        assert_eq!(predict(&plain).verdict, Verdict::Allows);
        assert_eq!(observe_checked(&plain).drift, None);
        let hour = selecting(
            "Datetime(us)",
            r#"{"entry":"Expr.dt.truncate","arguments":{"every":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"string":"1h"}}},"then":[]}}}}"#,
        );
        assert_eq!(predict(&hour).verdict, Verdict::Allows);
        let observed = observe_checked(&hour);
        assert_eq!((observed.refusal, observed.drift), (None, None));
    }
}
