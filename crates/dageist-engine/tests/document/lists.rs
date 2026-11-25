//! The methods of the `list` and the `arr` namespaces that take no expression argument, on a
//! column of lists or arrays of every scalar element dtype.
//!
//! Every expectation is what Python Polars 2.1.0 does, which each sweep asks by running the same
//! document on the rows of a table, and on a table with no rows, as the tables of allowed
//! combinations count a combination. The walk must say the same of the same document, and give
//! the dtype Polars gives after the last call and how many values it is. A sweep collects every
//! disagreement before it asserts, and a case that is not asked, or that the editor refuses more
//! strictly than Polars on purpose, is listed with its reason in [`EXCEPTIONS`].
//!
//! Each method is asked with a column of lists as its receiver, with one value of it
//! (`first()`), and with a count of it (`head(2)`); in a `select`, beside the rows in
//! `with_columns`, in a filter, as a sort key, as a join's key, in a window, in a window that
//! joins, as the value a `shift` fills with, and in a pivot's aggregate; and with lists of nulls,
//! empty lists, and lists beyond one level, which have no class.

use std::collections::{BTreeMap, BTreeSet};

use dageist_engine::allowed::{
    After, Count, Inner, RefusalSource, Representative, Shape, dtype_name,
};
use dageist_engine::catalog;
use dageist_engine::document::{
    Document, Inputs, Outcome, Report, Resolution, RunCause, Segment, StepId, StepRows,
};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{
    call, chain, col_call, document, field, field_then, many, named, step, whole,
};

/// The methods under test, each with the words that set its options, as Python builds the call.
/// A method that takes an option is asked with its default and with each word.
fn methods() -> Vec<(&'static str, &'static str, Value)> {
    let mut all = Vec::new();
    for namespace in ["list", "arr"] {
        for name in [
            "len", "sum", "mean", "median", "min", "max", "first", "last", "n_unique", "arg_max",
            "arg_min",
        ] {
            all.push((namespace, name, json!({})));
        }
        // Polars panics on an array reversed where an aggregate follows, so `arr.reverse` is left
        // out of the catalogue with its reason.
        if namespace == "list" {
            all.push((namespace, "reverse", json!({})));
        }
        for name in ["std", "var"] {
            all.push((namespace, name, json!({})));
            all.push((namespace, name, json!({ "ddof": { "integer": 0 } })));
        }
        for name in ["any", "all"] {
            all.push((namespace, name, json!({})));
            all.push((
                namespace,
                name,
                json!({ "ignore_nulls": { "flag": false } }),
            ));
        }
        all.push((namespace, "sort", json!({})));
        all.push((namespace, "sort", json!({ "descending": { "flag": true } })));
        all.push((namespace, "sort", json!({ "nulls_last": { "flag": true } })));
        all.push((namespace, "unique", json!({})));
        all.push((
            namespace,
            "unique",
            json!({ "maintain_order": { "flag": true } }),
        ));
    }
    all.push(("list", "drop_nulls", json!({})));
    all.push(("arr", "to_list", json!({})));
    all
}

/// The 37 methods, by the identifiers of the catalogue.
fn identifiers() -> Vec<String> {
    let mut ids: Vec<String> = methods()
        .into_iter()
        .map(|(namespace, name, _)| format!("Expr.{namespace}.{name}"))
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

// ---------------------------------------------------------------------------
// The table

/// The column of lists or arrays of the class `class`: a list of two values, a null, and a list
/// of one value, where the values of an array are two in every row. Polars builds them from
/// whole numbers, which every dtype but `Null` takes by a cast.
fn nested(class: Representative, name: &str) -> Series {
    let inner = class.element().expect("a list or an array");
    let is_array = matches!(class, Representative::Array(_));
    let rows: [Option<Vec<i64>>; 3] = [
        Some(vec![1, 3]),
        None,
        Some(if is_array { vec![3, 1] } else { vec![3] }),
    ];
    let lists = if inner == Inner::Null {
        ListChunked::from_iter(
            rows.map(|row| row.map(|row| Series::new_null("".into(), row.len()))),
        )
    } else {
        ListChunked::from_iter(rows.map(|row| row.map(|row| Series::new("".into(), row))))
    };
    lists
        .into_series()
        .with_name(name.into())
        .cast(&class.dtype())
        .unwrap_or_else(|error| panic!("whole numbers cast to {class}: {error}"))
}

/// The name of the column that holds lists or arrays of `class`.
fn column_of(class: Representative) -> String {
    class.name().replace(['(', ')', ','], "_").replace(' ', "")
}

/// The column of arrays of three whole numbers.
const WIDE: &str = "arr_wide";

/// The columns that are no class of the tables: lists of lists, of arrays and of structs.
const BEYOND: [&str; 3] = ["deep_list", "deep_array", "deep_struct"];

fn frame() -> DataFrame {
    let mut columns: Vec<Column> = Vec::new();
    for class in Representative::ALL
        .into_iter()
        .filter(|class| class.is_nested())
    {
        columns.push(nested(class, &column_of(class)).into());
    }
    let ints = |name: &str, rows: [Option<i64>; 3]| Series::new(name.into(), rows);
    // A list of lists, of arrays and of structs: no class, and the methods are asked of them to
    // see that the walk says nothing.
    columns.push(
        ListChunked::from_iter([
            Some(ListChunked::from_iter([Some(Series::new("".into(), [1_i64, 3]))]).into_series()),
            None,
            Some(ListChunked::from_iter([Some(Series::new("".into(), [3_i64]))]).into_series()),
        ])
        .into_series()
        .with_name("deep_list".into())
        .into(),
    );
    // Lists of arrays and lists of structs: a row holds the one array, or the one struct.
    let arrays = nested(Representative::Array(Inner::Int64), "");
    columns.push(
        ListChunked::from_iter([Some(arrays.slice(0, 1)), None, Some(arrays.slice(2, 1))])
            .into_series()
            .with_name("deep_array".into())
            .into(),
    );
    let one_struct = |x: i64| {
        StructChunked::from_series("".into(), 1, [Series::new("x".into(), [x])].iter())
            .expect("a struct")
            .into_series()
    };
    columns.push(
        ListChunked::from_iter([Some(one_struct(1)), None, Some(one_struct(3))])
            .into_series()
            .with_name("deep_struct".into())
            .into(),
    );
    // Lists with nothing in them.
    columns.push(
        ListChunked::from_iter([
            Some(Series::new("".into(), Vec::<i64>::new())),
            None,
            Some(Series::new("".into(), Vec::<i64>::new())),
        ])
        .into_series()
        .with_name("empty_lists".into())
        .into(),
    );
    // Arrays of three, which are not the width the tables measured arrays at.
    columns.push(
        ListChunked::from_iter([
            Some(Series::new("".into(), [1_i64, 3, 2])),
            None,
            Some(Series::new("".into(), [3_i64, 1, 2])),
        ])
        .into_series()
        .with_name(WIDE.into())
        .cast(&DataType::Array(Box::new(DataType::Int64), 3))
        .expect("lists of three cast to arrays of three")
        .into(),
    );
    columns.extend(depth_two_columns());
    columns.push(ints("a", [Some(1), Some(2), Some(3)]).into());
    // Receivers of a dtype with nothing in it and of dates, beside the whole numbers and the text.
    columns.push(Series::new_null("n".into(), 3).into());
    columns.push(
        ints("d", [Some(1), Some(2), Some(3)])
            .cast(&DataType::Date)
            .expect("whole numbers cast to dates")
            .into(),
    );
    // One group of all three rows, so that the first two values of it, which a window refuses where
    // they are not as many as the group, are not by chance as many as the group has.
    columns.push(ints("k", [Some(1), Some(1), Some(1)]).into());
    columns.push(Series::new("g".into(), ["x", "y", "x"]).into());
    columns.push(Series::new("f".into(), [true, false, true]).into());
    DataFrame::new(3, columns).expect("columns of three rows")
}

/// The columns each place is asked of beside every class: lists and arrays of a whole number, a
/// float, text, nothing, a date and a decimal, and lists with nothing in them.
fn sampled() -> Vec<String> {
    let mut columns: Vec<String> = [
        Representative::List(Inner::Int64),
        Representative::List(Inner::Float64),
        Representative::List(Inner::String),
        Representative::List(Inner::Null),
        Representative::Array(Inner::Int64),
        Representative::Array(Inner::Date),
        Representative::Array(Inner::Decimal),
    ]
    .map(column_of)
    .to_vec();
    columns.push("empty_lists".to_owned());
    columns.push(WIDE.to_owned());
    columns
}

fn inputs() -> Inputs {
    let mut inputs = Inputs::new();
    inputs
        .add_table(
            "other",
            "Other",
            df!("ra" => ["x", "y", "z"], "rk" => [1_i64, 2, 3])
                .expect("the other table")
                .lazy(),
        )
        .expect("a first input");
    inputs
        .add_table(
            "on_g",
            "Values of g",
            df!("g" => ["x", "y"]).expect("the values").lazy(),
        )
        .expect("a second input");
    inputs
}

// ---------------------------------------------------------------------------
// What Polars and the walk say

/// What Polars and the walk say of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Said {
    /// Polars runs the step on the rows and on a table with no rows, or the walk finds nothing
    /// refused.
    Accepts,
    /// It is refused, with the words or the rule that refuse it.
    Refuses(String),
    /// The walk does not know.
    NotKnown,
}

fn first_line(text: &impl ToString) -> String {
    text.to_string()
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn polars_says(report: &Report) -> Said {
    let step = &report.steps()[0];
    match (step.outcome(), step.rows()) {
        (Outcome::Valid { .. }, StepRows::Counted(_)) => Said::Accepts,
        (Outcome::Valid { .. }, StepRows::Failed(error)) => Said::Refuses(first_line(error)),
        (Outcome::Refused(refusal), _) => Said::Refuses(first_line(refusal)),
        (other, _) => Said::Refuses(format!("{other:?}")),
    }
}

fn walk_says(report: &Report) -> Said {
    let walk = report.steps()[0].walk();
    match (walk.refusal(), walk.not_known()) {
        (Some(refusal), _) => Said::Refuses(match refusal.refusal().source() {
            RefusalSource::Rule(rule) => rule.name().to_owned(),
            _ => first_line(&refusal.refusal().reason()),
        }),
        (None, Some(_)) => Said::NotKnown,
        (None, None) => Said::Accepts,
    }
}

/// What Polars says of the document on a table with no rows, as the tables count a combination.
fn empty_says(document: &Document) -> Said {
    let empty = frame().clear();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        document.run_on(&inputs(), &empty)
    })) {
        Ok(Ok(report)) => polars_says(&report),
        Ok(Err(error)) => Said::Refuses(first_line(&error)),
        Err(_) => Said::Refuses("Polars panicked".to_owned()),
    }
}

// ---------------------------------------------------------------------------
// The documents

/// What a method follows: the column, one value of it or a count of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Receiver {
    Column,
    OneValue,
    Count,
}

impl Receiver {
    const ALL: [Self; 3] = [Self::Column, Self::OneValue, Self::Count];

    fn links(self) -> Vec<Value> {
        match self {
            Self::Column => Vec::new(),
            Self::OneValue => vec![call("Expr.first", json!({}))],
            Self::Count => vec![call("Expr.head", json!({ "n": whole(2) }))],
        }
    }
}

/// Where the expression under test stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Place {
    Select,
    WithColumns,
    FilterContinued,
    FilterEnds,
    Sort,
    JoinKey,
    Window,
    WindowThatJoins,
    ShiftFill,
    PivotAggregate,
}

impl Place {
    const ALL: [Self; 10] = [
        Self::Select,
        Self::WithColumns,
        Self::FilterContinued,
        Self::FilterEnds,
        Self::Sort,
        Self::JoinKey,
        Self::Window,
        Self::WindowThatJoins,
        Self::ShiftFill,
        Self::PivotAggregate,
    ];
}

/// The links of the expression: the receiver's, the method, and what the place needs after it.
fn links_of(
    receiver: Receiver,
    namespace: &str,
    name: &str,
    arguments: &Value,
    place: Place,
) -> Vec<Value> {
    let mut links = receiver.links();
    links.push(call(&format!("Expr.{namespace}.{name}"), arguments.clone()));
    match place {
        Place::FilterContinued => links.push(call("Expr.is_not_null", json!({}))),
        Place::JoinKey => links.push(call("Expr.cast", json!({ "dtype": { "dtype": "Int64" } }))),
        Place::Window => links.push(call(
            "Expr.over",
            json!({ "partition_by": many(vec![field("k")]) }),
        )),
        Place::WindowThatJoins => links.push(call(
            "Expr.over",
            json!({
                "partition_by": many(vec![field("k")]),
                "mapping_strategy": { "choice": "join" },
            }),
        )),
        Place::PivotAggregate => links.push(call("Expr.first", json!({}))),
        _ => {}
    }
    links
}

/// The step that holds the expression, and the way from the step to the expression.
fn step_of(place: Place, column: &str, links: Vec<Value>) -> (Value, Vec<Segment>) {
    let held = || chain(col_call(column), links.clone());
    let item = |parameter: &str| vec![Segment::Parameter(parameter.to_owned()), Segment::Item(0)];
    match place {
        Place::Select => (
            step(
                "s",
                "LazyFrame.select",
                json!({ "exprs": many(vec![held()]) }),
            ),
            item("exprs"),
        ),
        Place::WithColumns => (
            step(
                "s",
                "LazyFrame.with_columns",
                json!({ "named_exprs": named(vec![("out", held())]) }),
            ),
            vec![
                Segment::Parameter("named_exprs".to_owned()),
                Segment::Item(0),
            ],
        ),
        Place::FilterContinued | Place::FilterEnds => (
            step(
                "s",
                "LazyFrame.filter",
                json!({ "predicates": many(vec![held()]) }),
            ),
            item("predicates"),
        ),
        Place::Sort => (
            step("s", "LazyFrame.sort", json!({ "by": many(vec![held()]) })),
            item("by"),
        ),
        Place::JoinKey => (
            step(
                "s",
                "LazyFrame.join",
                json!({
                    "other": { "table": "other" },
                    "left_on": many(vec![held()]),
                    "right_on": many(vec![field("rk")]),
                }),
            ),
            item("left_on"),
        ),
        Place::Window | Place::WindowThatJoins => (
            step(
                "s",
                "LazyFrame.select",
                json!({ "exprs": many(vec![held()]) }),
            ),
            item("exprs"),
        ),
        Place::ShiftFill => (
            step(
                "s",
                "LazyFrame.select",
                json!({ "exprs": many(vec![chain(
                    col_call("a"),
                    vec![call("Expr.shift", json!({ "fill_value": held() }))],
                )]) }),
            ),
            vec![
                Segment::Parameter("exprs".to_owned()),
                Segment::Item(0),
                Segment::Link(0),
                Segment::Parameter("fill_value".to_owned()),
            ],
        ),
        Place::PivotAggregate => (
            step(
                "s",
                "LazyFrame.pivot",
                json!({
                    "on": { "columns": ["g"] },
                    "on_columns": { "table": "on_g" },
                    "index": { "columns": ["k"] },
                    "values": { "columns": [column] },
                    "aggregate_function": chain(
                        call("polars.element", json!({})),
                        links,
                    ),
                }),
            ),
            vec![Segment::Parameter("aggregate_function".to_owned())],
        ),
    }
}

/// One document of the sweep: where it stands, what it holds, and the text to say so.
struct Case {
    place: Place,
    column: String,
    receiver: Receiver,
    namespace: &'static str,
    name: &'static str,
    arguments: Value,
}

impl Case {
    fn label(&self) -> String {
        format!(
            "{} {:?} of {} in {:?}: {}.{} {}",
            self.column,
            self.receiver,
            self.column,
            self.place,
            self.namespace,
            self.name,
            self.arguments
        )
    }

    fn document(&self) -> (Value, Vec<Segment>) {
        let links = links_of(
            self.receiver,
            self.namespace,
            self.name,
            &self.arguments,
            self.place,
        );
        step_of(self.place, &self.column, links)
    }
}

/// What Polars and the walk say of one case, and the dtype and the number of values each gives
/// at the end of the expression, in a `select`.
struct Found {
    polars: Said,
    walk: Said,
    polars_dtype: Option<DataType>,
    polars_values: Option<usize>,
    walked: Option<After>,
    /// The dtype the walk writes for the result of the last call.
    walked_name: Option<String>,
    /// How many values the walk says the expression gives, where it says.
    walked_count: Option<Count>,
}

fn ask(case: &Case) -> Result<Found, String> {
    let (value, path) = case.document();
    let document =
        Document::read(&document(vec![value])).map_err(|error| format!("unreadable: {error}"))?;
    // A panic of Polars' is a refusal where the rows run, as the tables count it.
    let (report, panicked) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        document.run_on(&inputs(), &frame())
    })) {
        Ok(report) => (
            report.map_err(|error| format!("no schema: {error}"))?,
            false,
        ),
        Err(_) => (
            document
                .run(&inputs(), frame().lazy())
                .map_err(|error| format!("no schema: {error}"))?,
            true,
        ),
    };
    let mut polars = if panicked {
        Said::Refuses("Polars panicked".to_owned())
    } else {
        polars_says(&report)
    };
    if polars == Said::Accepts && empty_says(&document) != Said::Accepts {
        polars = Said::Refuses("the table with no rows".to_owned());
    }
    // What the rows give, which the tables count as the dtype when the schema and the rows agree
    // in class, as Polars gives `std` of a list of text the schema of the text and a float.
    let rows = (polars == Said::Accepts)
        .then(|| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                document.rows(
                    &inputs(),
                    &frame(),
                    &StepId::new("s").expect("an identifier"),
                    10,
                )
            }))
            .ok()
            .and_then(Result::ok)
        })
        .flatten();
    if let (Some(rows), Some(schema)) = (&rows, report.steps()[0].schema()) {
        let class = |dtype: &DataType| Representative::of(dtype);
        let differs = rows.schema().len() != schema.len()
            || rows
                .schema()
                .iter()
                .zip(schema.iter())
                .any(|((_, ran), (_, planned))| ran != planned && class(ran) != class(planned));
        if differs {
            polars = Said::Refuses("the schema and the rows differ".to_owned());
        }
    }
    let walk = walk_says(&report);
    let expression = report.steps()[0]
        .expressions()
        .iter()
        .find(|expression| expression.place().path() == path);
    let polars_dtype = expression.and_then(|expression| match expression.links().last()? {
        Resolution::Dtype(dtype) => Some(dtype.clone()),
        _ => None,
    });
    let walked = report.steps()[0]
        .walk()
        .expressions()
        .iter()
        .find(|expression| expression.place().path() == path)
        .and_then(|expression| expression.places().last().copied().flatten())
        .map(|place| place.after());
    let walked_count = report.steps()[0]
        .walk()
        .expressions()
        .iter()
        .find(|expression| expression.place().path() == path)
        .and_then(|expression| expression.count());
    let walked_name = report.steps()[0]
        .walk()
        .expressions()
        .iter()
        .find(|expression| expression.place().path() == path)
        .and_then(|expression| expression.calls().last()?.verdict()?.allowance())
        .and_then(|allowance| allowance.result().map(|result| result.name().to_owned()));
    let polars_values = (case.place == Place::Select)
        .then(|| rows.as_ref().map(|rows| rows.height()))
        .flatten();
    Ok(Found {
        polars,
        walk,
        polars_dtype,
        polars_values,
        walked,
        walked_name,
        walked_count,
    })
}

/// What the walk refuses where Polars runs the document, and why it is right to: the words its
/// refusal begins with, and the reason.
const STRICTER: &[(&str, &str)] = &[
    (
        "the schema says",
        "a method whose schema and whose rows differ in dtype is refused wherever it stands, as the \
         tables count it, though a call after it that reads no dtype hides the difference",
    ),
    (
        "conversion from `str`",
        "the tables are measured on text that is a duration, which converts to no whole number, and \
         the text of a column of digits does: what the data decide",
    ),
    (
        "conversion from `i64` to `null`",
        "the tables stand the number 1 in for a value of the dtype Null where Polars reads one \
         value, which no number converts to, and a null the column holds is one",
    ),
];

/// Whether the walk's refusal `reason` of `case`, which Polars runs, is one that is named.
fn stricter_on_purpose(case: &Case, reason: &str) -> bool {
    // Polars drops a sort key of one value without meeting it.
    (case.place == Place::Sort && case.receiver == Receiver::OneValue)
        || STRICTER.iter().any(|(words, _)| reason.starts_with(words))
}

/// A case that is not asked, or that the editor refuses more strictly than Polars on purpose:
/// the column's kind, the method (or `*`), the place (or `*`), and why.
const EXCEPTIONS: &[(&str, &str, &str, &str)] = &[];

fn excepted(case: &Case) -> bool {
    EXCEPTIONS.iter().any(|(column, method, place, _)| {
        (*column == "*" || case.column.starts_with(column))
            && (*method == "*" || format!("{}.{}", case.namespace, case.name) == *method)
            && (*place == "*" || format!("{:?}", case.place) == *place)
    })
}

/// One disagreement: what kind it is, with the place and the method, and the line that says what
/// differed in the case.
type Disagreement = (String, String);

fn kind(case: &Case, kind: &str) -> String {
    format!("{:?} {}.{}, {kind}", case.place, case.namespace, case.name)
}

/// The disagreements of `cases`, each of a kind and with a line that says what differed.
fn disagreements(cases: &[Case]) -> (Vec<Disagreement>, usize) {
    let mut found: Vec<Disagreement> = Vec::new();
    let mut asked = 0;
    for case in cases.iter().filter(|case| !excepted(case)) {
        asked += 1;
        let said = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ask(case))) {
            Ok(Ok(said)) => said,
            Ok(Err(error)) => {
                found.push((
                    kind(case, "unreadable"),
                    format!("{}: {error}", case.label()),
                ));
                continue;
            }
            Err(_) => {
                found.push((kind(case, "Polars panicked"), case.label()));
                continue;
            }
        };
        let class = |dtype: &DataType| Representative::of(dtype);
        match (&said.polars, &said.walk) {
            (Said::Accepts, Said::Accepts | Said::NotKnown)
            | (Said::Refuses(_), Said::Refuses(_) | Said::NotKnown) => {}
            // The walk is stricter than Polars on purpose, for a reason that is named.
            (Said::Accepts, Said::Refuses(reason)) if stricter_on_purpose(case, reason) => {
                continue;
            }
            (polars, walk) => {
                let words = |said: &Said| match said {
                    Said::Accepts => "accepts".to_owned(),
                    Said::Refuses(_) => "refuses".to_owned(),
                    Said::NotKnown => "does not know".to_owned(),
                };
                found.push((
                    kind(
                        case,
                        &format!("Polars {}, the walk {}", words(polars), words(walk)),
                    ),
                    format!("{}: Polars {polars:?}, the walk {walk:?}", case.label()),
                ));
                continue;
            }
        }
        if said.walk == Said::NotKnown {
            // Only what has no class is not known.
            if !BEYOND.contains(&case.column.as_str()) {
                found.push((kind(case, "the walk does not know"), case.label()));
            }
            continue;
        }
        // The dtype after the last call, and in a select the number of values; in a window, even one
        // that joins the values of a group into a list, the dtype it gives.
        if said.polars == Said::Accepts
            && matches!(
                case.place,
                Place::Select | Place::Window | Place::WindowThatJoins
            )
        {
            let dtype = said.polars_dtype.as_ref().and_then(class);
            let walked = said.walked.map(|after| match after {
                After::Value { dtype, shape } => (Some(dtype), Some(shape)),
                _ => (None, None),
            });
            let walked_class = walked.and_then(|(dtype, _)| dtype);
            let shape = said.polars_values.map(|height| match height {
                1 => Shape::One,
                3 => Shape::Many,
                _ => Shape::Counted,
            });
            // An array is written with its own width, which is not the one the tables measured.
            // What the walk does not know after a window of one value that joins is no claim.
            let silent = case.place != Place::Select && walked_class.is_none() && dtype.is_some();
            let name_differs = matches!(said.polars_dtype, Some(DataType::Array(..)))
                && said.walked_name.is_some()
                && said.walked_name.as_deref()
                    != said.polars_dtype.as_ref().map(dtype_name).as_deref();
            if name_differs {
                found.push((
                    kind(case, "the width of an array"),
                    format!(
                        "{}: Polars gives {:?}, the walk writes {:?}",
                        case.label(),
                        said.polars_dtype,
                        said.walked_name
                    ),
                ));
            } else if !silent && walked_class != dtype {
                found.push((
                    kind(case, "the dtype"),
                    format!(
                        "{}: Polars gives {:?}, the walk {:?}",
                        case.label(),
                        said.polars_dtype,
                        walked
                    ),
                ));
            } else if case.place == Place::Select
                && case.receiver == Receiver::Count
                && said.walked_count != Some(Count::AtMost(2))
            {
                found.push((
                    kind(case, "the count of values"),
                    format!(
                        "{}: the first two values give at most two, the walk says {:?}",
                        case.label(),
                        said.walked_count
                    ),
                ));
            } else if let (Some(shape), Some((_, Some(walked)))) = (shape, walked)
                && case.place == Place::Select
                && shape != walked
            {
                found.push((
                    kind(case, "the number of values"),
                    format!(
                        "{}: Polars gives {shape:?}, the walk {walked:?}",
                        case.label()
                    ),
                ));
            }
        }
    }
    (found, asked)
}

/// Asserts there is no disagreement, listing each kind with how many cases and one of them.
fn report(found: &[Disagreement], asked: usize, what: &str) {
    let mut kinds: BTreeMap<&str, (usize, &str)> = BTreeMap::new();
    for (kind, line) in found {
        kinds.entry(kind).or_insert((0, line)).0 += 1;
    }
    assert!(
        found.is_empty(),
        "{} of {asked} cases of {what} disagree, of {} kinds:\n{}",
        found.len(),
        kinds.len(),
        kinds
            .iter()
            .map(|(kind, (count, line))| format!("{count} x {kind}, as {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// ---------------------------------------------------------------------------
// The sweeps

#[test]
fn the_catalogue_holds_the_thirty_seven_methods() {
    let ids = identifiers();
    assert_eq!(ids.len(), 37, "{ids:?}");
    let missing: Vec<&String> = ids
        .iter()
        .filter(|id| catalog::entry(id).is_none())
        .collect();
    assert!(
        missing.is_empty(),
        "{} are not entries: {missing:?}",
        missing.len()
    );
}

fn cases_in(places: &[Place], columns: &[String], receivers: &[Receiver]) -> Vec<Case> {
    let mut cases = Vec::new();
    for place in places {
        for column in columns {
            for receiver in receivers {
                for (namespace, name, arguments) in methods() {
                    cases.push(Case {
                        place: *place,
                        column: column.clone(),
                        receiver: *receiver,
                        namespace,
                        name,
                        arguments,
                    });
                }
            }
        }
    }
    cases
}

#[test]
fn each_method_on_each_class_is_what_polars_says_in_a_select() {
    let mut columns: Vec<String> = Representative::ALL
        .into_iter()
        .filter(|class| class.is_nested())
        .map(column_of)
        .collect();
    columns.extend([
        "empty_lists".to_owned(),
        WIDE.to_owned(),
        "a".to_owned(),
        "g".to_owned(),
    ]);
    columns.extend(BEYOND.map(str::to_owned));
    let cases = cases_in(&[Place::Select], &columns, &Receiver::ALL);
    let (found, asked) = disagreements(&cases);
    report(&found, asked, "every method on every class in a select");
}

#[test]
fn each_method_is_what_polars_says_in_each_place() {
    let columns = sampled();
    let places: Vec<Place> = Place::ALL
        .into_iter()
        .filter(|place| *place != Place::Select)
        .collect();
    let cases = cases_in(&places, &columns, &Receiver::ALL);
    let (found, asked) = disagreements(&cases);
    report(&found, asked, "every method in every place");
}

#[test]
fn the_walk_names_the_classes_of_the_columns_by_the_tables() {
    // The frame holds a column of each class, and a column of lists of lists, which has none.
    let classes: BTreeMap<String, Option<Representative>> = frame()
        .schema()
        .iter()
        .map(|(name, dtype)| (name.to_string(), Representative::of(dtype)))
        .collect();
    for class in Representative::ALL
        .into_iter()
        .filter(|class| class.is_nested())
    {
        assert_eq!(classes[&column_of(class)], Some(class));
    }
    for beyond in BEYOND {
        assert_eq!(classes[beyond], None, "{beyond} is beyond one level");
    }
}

/// A call of a method whose answer depends on the element, after a list or an array that the tables
/// are not measured on for it, is not run: Polars panics on some of them, as it does on `diff` of an
/// array of `Null` and on arithmetic of a list of `Null` with a null or a `true`.
#[test]
fn a_call_the_tables_do_not_measure_is_refused_before_polars_is_asked() {
    let flag = |value: Option<bool>| {
        chain(
            call("polars.lit", json!({ "value": { "flag": value } })),
            vec![],
        )
    };
    let mut reached = Vec::new();
    let mut asked = 0;
    for class in [
        Representative::List(Inner::Null),
        Representative::Array(Inner::Null),
        Representative::List(Inner::Time),
        Representative::Array(Inner::Duration),
    ] {
        let column = column_of(class);
        let mut calls = vec![("diff".to_owned(), call("Expr.diff", json!({})))];
        for method in ["add", "sub", "mul", "floordiv", "mod"] {
            for (name, other) in [("null", flag(None)), ("true", flag(Some(true)))] {
                calls.push((
                    format!("{method} {name}"),
                    call(&format!("Expr.{method}"), json!({ "other": other })),
                ));
            }
        }
        for (label, link) in calls {
            asked += 1;
            let held = chain(col_call(&column), vec![link]);
            let document = Document::read(&document(vec![step(
                "s",
                "LazyFrame.select",
                json!({ "exprs": many(vec![held]) }),
            )]))
            .expect("a document");
            let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                document.run_on(&inputs(), &frame())
            }));
            let refused_before_polars = matches!(
                &ran,
                Ok(Ok(report)) if matches!(
                    report.steps()[0].outcome(),
                    Outcome::Refused(refusal) if matches!(refusal.cause(), RunCause::NotAllowed(_))
                )
            );
            if !refused_before_polars {
                reached.push(format!("{column} {label}"));
            }
        }
    }
    assert!(
        reached.is_empty(),
        "{} of {asked} calls are not refused before Polars is asked: {reached:?}",
        reached.len()
    );
}

/// The calls that follow a method, each by what it says: every method of an expression that needs
/// no argument, and the few that cut or group by one the test gives.
fn followers() -> Vec<(String, Value)> {
    let mut all: Vec<(String, Value)> = catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == catalog::Applies::Expression)
        .filter(|entry| {
            entry
                .parameters()
                .iter()
                .all(|parameter| !parameter.is_needed())
        })
        .map(|entry| {
            (
                entry.id().as_str().to_owned(),
                call(entry.id().as_str(), json!({})),
            )
        })
        .collect();
    let given =
        |entry: &str, arguments: Value| (format!("{entry} {arguments}"), call(entry, arguments));
    all.extend([
        given("Expr.head", json!({ "n": whole(1) })),
        given("Expr.tail", json!({ "n": whole(1) })),
        given("Expr.limit", json!({ "n": whole(1) })),
        given("Expr.shift", json!({ "n": whole(1) })),
        given("Expr.gather_every", json!({ "n": whole(1) })),
        given("Expr.gather", json!({ "indices": whole(0) })),
        given(
            "Expr.slice",
            json!({ "offset": whole(0), "length": whole(2) }),
        ),
        given("Expr.top_k", json!({ "k": whole(1) })),
        given("Expr.filter", json!({ "predicate": field("f") })),
        given(
            "Expr.over",
            json!({ "partition_by": many(vec![field("g")]) }),
        ),
        given(
            "Expr.over join",
            json!({
                "partition_by": many(vec![field("g")]),
                "mapping_strategy": { "choice": "join" },
            }),
        ),
    ]);
    all
}

/// No method of these, followed by any call, is run into a panic of Polars': a combination that
/// panics is refused before Polars is asked, by a rule or by what the tables say, or the method is
/// not offered. A panic on a thread of Polars' own can leave its streaming executor broken for every
/// query after it, and the host does not catch one.
#[test]
fn no_method_followed_by_any_call_is_run_into_a_panic_of_polars() {
    let classes: Vec<Representative> = [
        Inner::Int64,
        Inner::String,
        Inner::Float64,
        Inner::Date,
        Inner::Null,
    ]
    .into_iter()
    .flat_map(|inner| [Representative::List(inner), Representative::Array(inner)])
    .collect();
    let mut panicked: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut asked = 0;
    let followers = followers();
    for class in classes {
        let column = column_of(class);
        for (namespace, name, arguments) in methods() {
            let base = call(&format!("Expr.{namespace}.{name}"), arguments.clone());
            for (follower, link) in &followers {
                for place in [Place::Select, Place::Sort, Place::PivotAggregate] {
                    // The places are asked of the calls that cut or aggregate: a sort key and a
                    // pivot's aggregate are one value or the rows.
                    if place != Place::Select
                        && ![
                            "first", "last", "head", "tail", "n_unique", "sum", "min", "unique",
                        ]
                        .iter()
                        .any(|word| follower.to_lowercase().contains(&format!("expr.{word}")))
                    {
                        continue;
                    }
                    asked += 1;
                    let links = vec![base.clone(), link.clone()];
                    let (value, _) = step_of(place, &column, links);
                    let Ok(document) = Document::read(&document(vec![value])) else {
                        continue;
                    };
                    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        document.run_on(&inputs(), &frame())
                    }));
                    if ran.is_err() {
                        panicked
                            .entry(format!("{namespace}.{name} {arguments} then {follower}"))
                            .or_default()
                            .push(format!("{column} in {place:?}"));
                    }
                }
            }
        }
    }
    let mut by_method: BTreeMap<&str, (usize, BTreeSet<&str>, &str)> = BTreeMap::new();
    for (chain, where_) in &panicked {
        let (method, follower) = chain.split_once(" then ").unwrap_or((chain, ""));
        let slot = by_method
            .entry(method)
            .or_insert_with(|| (0, BTreeSet::new(), where_[0].as_str()));
        slot.0 += 1;
        slot.1.extend(where_.iter().map(String::as_str).take(1));
        slot.2 = follower;
    }
    assert!(
        panicked.is_empty(),
        "{} chains of {asked} reach a panic of Polars', of {} methods:\n{}",
        panicked.len(),
        by_method.len(),
        by_method
            .iter()
            .map(|(method, (count, classes, follower))| format!(
                "{method}: {count} chains, as {follower} on {classes:?}"
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// What the walk claims after the expression, and what Polars gives: the class the walk says the
/// last call gives (none where it refused or does not know), and the dtype Polars resolved.
fn claimed_and_given(expression: Value) -> (Option<Representative>, Option<DataType>) {
    let path = vec![Segment::Parameter("exprs".to_owned()), Segment::Item(0)];
    let document = Document::read(&document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![expression]) }),
    )]))
    .expect("a document");
    let report = document
        .run_on(&inputs(), &frame())
        .expect("the table has a schema");
    let walked = report.steps()[0]
        .walk()
        .expressions()
        .iter()
        .find(|walked| walked.place().path() == path)
        .and_then(|walked| walked.places().last().copied().flatten())
        .and_then(|place| match place.after() {
            After::Value { dtype, .. } => Some(dtype),
            _ => None,
        });
    let given = report.steps()[0]
        .expressions()
        .iter()
        .find(|traced| traced.place().path() == path)
        .and_then(|traced| match traced.links().last()? {
            Resolution::Dtype(dtype) => Some(dtype.clone()),
            _ => report.steps()[0]
                .schema()
                .and_then(|schema| schema.iter().next().map(|(_, dtype)| dtype.clone())),
        });
    (walked, given)
}

/// Arrays of two and of three that meet are not arrays: Polars refuses to add them and gives a list
/// where it fills one with the other or takes one for a value and the other for another of a
/// condition, so the walk, which carries the width of an array, never claims an array for them, and
/// claims one where the widths are the same.
#[test]
fn arrays_of_other_widths_are_not_met_as_arrays_and_those_of_one_width_are() {
    let two = column_of(Representative::Array(Inner::Int64));
    let three = WIDE;
    let on = |column: &str, links: Vec<Value>| chain(col_call(column), links);
    let true_literal = || {
        chain(
            call("polars.lit", json!({ "value": { "flag": true } })),
            vec![],
        )
    };
    let condition = |then: &str, otherwise: &str| {
        chain(
            call(
                "polars.when",
                json!({ "predicates": many(vec![true_literal()]) }),
            ),
            vec![
                call("When.then", json!({ "statement": field(then) })),
                call("Then.otherwise", json!({ "statement": field(otherwise) })),
            ],
        )
    };
    let met = [
        (
            "an array of two plus an array of three",
            on(
                &two,
                vec![call("Expr.add", json!({ "other": field(three) }))],
            ),
        ),
        (
            "an array of two filled with an array of three",
            on(
                &two,
                vec![call("Expr.fill_null", json!({ "value": field(three) }))],
            ),
        ),
        (
            "an array of three filled with an array of two",
            on(
                three,
                vec![call("Expr.fill_null", json!({ "value": field(&two) }))],
            ),
        ),
        (
            "a condition of an array of two and of three",
            condition(&two, three),
        ),
        (
            "a condition of an array of three and of two",
            condition(three, &two),
        ),
    ];
    let mut wrong = Vec::new();
    for (label, expression) in met {
        let (claimed, given) = claimed_and_given(expression);
        if let Some(claimed @ Representative::Array(_)) = claimed {
            wrong.push(format!(
                "{label}: the walk claims {claimed}, Polars gives {given:?}"
            ));
        }
    }
    // The same width is an array, as Polars gives.
    for (label, expression) in [
        (
            "an array of two filled with an array of two",
            on(
                &two,
                vec![call("Expr.fill_null", json!({ "value": field(&two) }))],
            ),
        ),
        ("a condition of two arrays of two", condition(&two, &two)),
        (
            "an array of three plus an array of three",
            on(
                three,
                vec![call("Expr.add", json!({ "other": field(three) }))],
            ),
        ),
    ] {
        let (claimed, given) = claimed_and_given(expression);
        let expected = given.as_ref().and_then(Representative::of);
        if !matches!(expected, Some(Representative::Array(_))) || claimed != expected {
            wrong.push(format!(
                "{label}: the walk claims {claimed:?}, Polars gives {given:?}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// ---------------------------------------------------------------------------
// Receivers of two levels, which have no class

/// The element dtypes under the second level.
const DEPTH_TWO_ELEMENTS: [Inner; 4] = [Inner::Null, Inner::Date, Inner::String, Inner::Int64];

/// The prefixes of the columns of two levels, where the outer one is an array or not.
const DEPTH_TWO_KINDS: [(&str, bool); 4] = [
    ("two_list_", false),
    ("two_array_", false),
    ("two_struct_", false),
    ("two_alist_", true),
];

/// The name of the column of two levels of `prefix` over the element `element`.
fn depth_two_name(prefix: &str, element: Inner) -> String {
    format!("{prefix}{element:?}")
}

/// The columns of lists of lists, of lists of arrays, of lists of structs and of arrays of lists of
/// each element dtype, where Polars builds them: a row holds one inner list, array or struct.
fn depth_two_columns() -> Vec<Column> {
    let mut columns = Vec::new();
    for element in DEPTH_TWO_ELEMENTS {
        let rows = |inner: &Series| {
            ListChunked::from_iter([Some(inner.slice(0, 1)), None, Some(inner.slice(2, 1))])
                .into_series()
        };
        let lists = nested(Representative::List(element), "");
        let arrays = nested(Representative::Array(element), "");
        let value = if element == Inner::Null {
            Series::new_null("x".into(), 1)
        } else {
            Series::new("x".into(), [1_i64])
                .cast(&element.class().dtype())
                .expect("a whole number casts to the element")
        };
        let one_struct = StructChunked::from_series("".into(), 1, [value].iter())
            .expect("a struct")
            .into_series();
        let list_rows = rows(&lists);
        let built = [
            ("two_list_", Some(list_rows.clone())),
            ("two_array_", Some(rows(&arrays))),
            (
                "two_struct_",
                Some(
                    ListChunked::from_iter([Some(one_struct.clone()), None, Some(one_struct)])
                        .into_series(),
                ),
            ),
            (
                "two_alist_",
                list_rows
                    .cast(&DataType::Array(
                        Box::new(DataType::List(Box::new(element.class().dtype()))),
                        1,
                    ))
                    .ok(),
            ),
        ];
        for (prefix, series) in built {
            if let Some(series) = series {
                columns.push(
                    series
                        .with_name(depth_two_name(prefix, element).into())
                        .into(),
                );
            }
        }
    }
    columns
}

/// Every method of an expression the test can call, and each with the arguments it needs: a null
/// and a `true` for each expression, and a whole number for a dtype's place.
fn every_method_called() -> Vec<(String, Value)> {
    let literal = |value: Option<bool>| {
        chain(
            call("polars.lit", json!({ "value": { "flag": value } })),
            vec![],
        )
    };
    let mut all = Vec::new();
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == catalog::Applies::Expression)
    {
        let id = entry.id().as_str();
        let needed: Vec<_> = entry
            .parameters()
            .iter()
            .filter(|parameter| parameter.is_needed())
            .collect();
        let supported = needed.iter().all(|parameter| {
            matches!(
                parameter.kind(),
                catalog::Kind::Expression(_) | catalog::Kind::Dtype
            )
        });
        if !supported {
            continue;
        }
        let variants: Vec<(&str, Option<bool>)> = if needed.is_empty() {
            vec![("", None)]
        } else {
            vec![(" null", None), (" true", Some(true))]
        };
        for (word, other) in variants {
            let mut arguments = serde_json::Map::new();
            for parameter in &needed {
                let value = match parameter.kind() {
                    catalog::Kind::Dtype => json!({ "dtype": "Int64" }),
                    _ if parameter.shape() == catalog::Shape::Many => many(vec![literal(other)]),
                    _ => literal(other),
                };
                arguments.insert(parameter.name().to_owned(), value);
            }
            all.push((format!("{id}{word}"), call(id, Value::Object(arguments))));
        }
    }
    all
}

/// A column of lists of lists, of lists of arrays, of lists of structs or of arrays of lists has
/// no class, so the walk does not know the dtype of what follows `list.first` of it, and the calls
/// after it were never judged. Polars panics on some of them, as on `diff` of an array of `Null`
/// and the mean of an array of dates followed by `first`. None may be run. After the first panics
/// Polars answers some later chains with an error where it would have panicked, so a failure lists
/// the chains it saw and a fix is checked by asking again.
#[test]
fn no_receiver_of_two_levels_is_run_into_a_panic_of_polars() {
    let calls = every_method_called();
    let present = frame();
    let mut panicked: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut asked = 0;
    for (prefix, outer_is_array) in DEPTH_TWO_KINDS {
        for element in DEPTH_TWO_ELEMENTS {
            let column = depth_two_name(prefix, element);
            if present.column(&column).is_err() {
                continue;
            }
            let namespace = if outer_is_array { "arr" } else { "list" };
            let mut starts: Vec<(String, Vec<Value>)> = vec![("alone".to_owned(), vec![])];
            for name in ["first", "last"] {
                starts.push((
                    format!("{namespace}.{name}"),
                    vec![call(&format!("Expr.{namespace}.{name}"), json!({}))],
                ));
            }
            for (start, before) in &starts {
                for (label, link) in &calls {
                    for (place, after) in [
                        (Place::Select, None),
                        (Place::Select, Some(call("Expr.first", json!({})))),
                        // A pivot's aggregate is one value of the values of a group.
                        (Place::PivotAggregate, Some(call("Expr.first", json!({})))),
                    ] {
                        asked += 1;
                        let mut links = before.clone();
                        links.push(link.clone());
                        links.extend(after);
                        let (value, _) = step_of(place, &column, links);
                        let Ok(document) = Document::read(&document(vec![value])) else {
                            continue;
                        };
                        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            document.run_on(&inputs(), &frame())
                        }));
                        if ran.is_err() {
                            panicked
                                .entry(format!("{start} then {label}"))
                                .or_default()
                                .push(format!("{column} in {place:?}"));
                        }
                    }
                }
            }
        }
    }
    assert!(
        panicked.is_empty(),
        "{} chains of {asked} reach a panic of Polars':\n{}",
        panicked.len(),
        panicked
            .iter()
            .take(40)
            .map(|(chain, columns)| format!("{chain}: {}", columns.join(", ")))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// ---------------------------------------------------------------------------
// Arrays of two widths that meet

/// What Polars and the walk say of `links` after `column`, in a `select`: Polars' words where it
/// refuses or panics, and the dtype it gives where it accepts.
fn polars_and_walk_say(column: &str, links: Vec<Value>) -> Option<(Said, Said, Option<DataType>)> {
    let (value, path) = step_of(Place::Select, column, links);
    let document = Document::read(&document(vec![value])).ok()?;
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        document.run_on(&inputs(), &frame())
    }));
    let Ok(Ok(report)) = ran else {
        return Some((
            Said::Refuses("Polars panicked".to_owned()),
            Said::NotKnown,
            None,
        ));
    };
    let given = report.steps()[0]
        .expressions()
        .iter()
        .find(|traced| traced.place().path() == path)
        .and_then(|traced| match traced.links().last()? {
            Resolution::Dtype(dtype) => Some(dtype.clone()),
            _ => None,
        });
    Some((polars_says(&report), walk_says(&report), given))
}

/// Every call that takes an expression and is given an array of two values and an array of three
/// is what Polars says of it: where Polars refuses it though it takes two arrays of two, the walk
/// refuses it too, and where Polars takes it, the walk refuses it only for what it would claim of
/// the result, an array, where Polars gives a list. Polars refuses to add arrays of two widths, to
/// look one up in the other and to replace by them, and compares them.
#[test]
fn every_call_that_meets_arrays_of_two_widths_is_what_polars_says() {
    let two = column_of(Representative::Array(Inner::Int64));
    let three = WIDE;
    let mut agree_on_the_width = Vec::new();
    let mut asked = 0;
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == catalog::Applies::Expression)
    {
        let id = entry.id().as_str();
        let expressions: Vec<_> = entry
            .parameters()
            .iter()
            .filter(|parameter| matches!(parameter.kind(), catalog::Kind::Expression(_)))
            .collect();
        let needs_more = entry.parameters().iter().any(|parameter| {
            parameter.is_needed()
                && !matches!(
                    parameter.kind(),
                    catalog::Kind::Expression(_) | catalog::Kind::Dtype
                )
        });
        if expressions.is_empty() || needs_more {
            continue;
        }
        let needed: Vec<_> = expressions
            .iter()
            .filter(|parameter| parameter.is_needed())
            .collect();
        // The parameters that are given the other array: the ones the call needs, or each of the
        // others by itself where it needs none.
        let sets: Vec<Vec<&catalog::Parameter>> = if needed.is_empty() {
            expressions
                .iter()
                .map(|parameter| vec![*parameter])
                .collect()
        } else {
            vec![needed.iter().map(|parameter| **parameter).collect()]
        };
        for set in sets {
            for (receiver, other_for_two, other_for_three) in [
                (two.as_str(), two.as_str(), three),
                (three, two.as_str(), three),
            ] {
                let call_with = |other: &str| {
                    let mut arguments = serde_json::Map::new();
                    for parameter in entry.parameters().iter().filter(|parameter| {
                        parameter.is_needed() && matches!(parameter.kind(), catalog::Kind::Dtype)
                    }) {
                        arguments.insert(parameter.name().to_owned(), json!({ "dtype": "Int64" }));
                    }
                    for parameter in &set {
                        let value = if parameter.shape() == catalog::Shape::Many {
                            many(vec![field(other)])
                        } else {
                            field(other)
                        };
                        arguments.insert(parameter.name().to_owned(), value);
                    }
                    vec![call(id, Value::Object(arguments))]
                };
                // The other array is of the width of the receiver, then of the other width.
                let same = if receiver == two {
                    other_for_two
                } else {
                    other_for_three
                };
                let other = if receiver == two {
                    other_for_three
                } else {
                    other_for_two
                };
                asked += 1;
                let (Some((alike, _, _)), Some((met, walked, given))) = (
                    polars_and_walk_say(receiver, call_with(same)),
                    polars_and_walk_say(receiver, call_with(other)),
                ) else {
                    continue;
                };
                let label = format!(
                    "{id} {} on {receiver} beside {other}",
                    set.iter()
                        .map(|parameter| parameter.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                match (&alike, &met, &walked) {
                    // Polars refuses the widths, and the walk says it does not.
                    (Said::Accepts, Said::Refuses(_), Said::Accepts | Said::NotKnown) => {
                        agree_on_the_width.push(format!("{label}: the walk accepts, {met:?}"));
                    }
                    // Polars takes the widths, and the walk refuses them where Polars gives no
                    // array, which it would have claimed.
                    (_, Said::Accepts, Said::Refuses(_))
                        if matches!(given, Some(DataType::Array(..))) =>
                    {
                        agree_on_the_width.push(format!(
                            "{label}: the walk refuses {walked:?}, Polars gives {given:?}"
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
    assert!(
        agree_on_the_width.is_empty(),
        "{} of {asked} calls differ from Polars:\n{}",
        agree_on_the_width.len(),
        agree_on_the_width.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Values of two levels, given as an argument

/// The names of the methods whose answer depends on the element dtype of a list or an array.
const SENSITIVE_METHODS: [&str; 23] = [
    "Expr.add",
    "Expr.and_",
    "Expr.cast",
    "Expr.diff",
    "Expr.eq",
    "Expr.eq_missing",
    "Expr.fill_nan",
    "Expr.fill_null",
    "Expr.floordiv",
    "Expr.is_close",
    "Expr.is_in",
    "Expr.mod",
    "Expr.mul",
    "Expr.ne",
    "Expr.ne_missing",
    "Expr.or_",
    "Expr.replace",
    "Expr.replace_strict",
    "Expr.reverse",
    "Expr.search_sorted",
    "Expr.sub",
    "Expr.truediv",
    "Expr.xor",
];

/// The connected input that holds the column of two levels `column`, as a list of values.
fn input_of(column: &str) -> String {
    format!("in_{column}")
}

/// The connected inputs, and a list of values of each dtype of two levels beside them.
fn inputs_of_two_levels() -> Inputs {
    let mut inputs = inputs();
    let present = frame();
    for (prefix, _) in DEPTH_TWO_KINDS {
        for element in DEPTH_TWO_ELEMENTS {
            let column = depth_two_name(prefix, element);
            if let Ok(values) = present.column(&column) {
                inputs
                    .add_list(
                        input_of(&column),
                        dageist_engine::document::ListOrigin::Parameter,
                        values.as_materialized_series().clone(),
                    )
                    .expect("a name that is new");
            }
        }
    }
    inputs
}

/// Every method of an expression that takes one, each given `argument` as its expression: the one
/// it needs, or each it does not need by itself, with a dtype for a dtype's place.
fn every_method_given(argument: &Value) -> Vec<(String, Value)> {
    let mut all = Vec::new();
    for entry in catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == catalog::Applies::Expression)
    {
        let id = entry.id().as_str();
        let expressions: Vec<_> = entry
            .parameters()
            .iter()
            .filter(|parameter| matches!(parameter.kind(), catalog::Kind::Expression(_)))
            .collect();
        let needs_more = entry.parameters().iter().any(|parameter| {
            parameter.is_needed()
                && !matches!(
                    parameter.kind(),
                    catalog::Kind::Expression(_) | catalog::Kind::Dtype
                )
        });
        if expressions.is_empty() || needs_more {
            continue;
        }
        let needed: Vec<_> = expressions
            .iter()
            .filter(|parameter| parameter.is_needed())
            .collect();
        let sets: Vec<Vec<&catalog::Parameter>> = if needed.is_empty() {
            expressions
                .iter()
                .map(|parameter| vec![*parameter])
                .collect()
        } else {
            vec![needed.iter().map(|parameter| **parameter).collect()]
        };
        for set in sets {
            let mut arguments = serde_json::Map::new();
            for parameter in entry.parameters().iter().filter(|parameter| {
                parameter.is_needed() && matches!(parameter.kind(), catalog::Kind::Dtype)
            }) {
                arguments.insert(parameter.name().to_owned(), json!({ "dtype": "Int64" }));
            }
            for parameter in &set {
                let value = if parameter.shape() == catalog::Shape::Many {
                    many(vec![argument.clone()])
                } else {
                    argument.clone()
                };
                arguments.insert(parameter.name().to_owned(), value);
            }
            let names: Vec<_> = set.iter().map(|parameter| parameter.name()).collect();
            all.push((
                format!("{id}({})", names.join(", ")),
                call(id, Value::Object(arguments)),
            ));
        }
    }
    all
}

/// A value of two levels given as the argument of a method is not run into a panic of Polars, and a
/// method that depends on the element dtype is not run with one the tables do not judge: the dtype
/// of an argument that has no class is not known to them, as it is not for a receiver. The
/// argument is a column of lists of lists, of arrays, of structs or of arrays of lists, or one of
/// them taken a value of, or a connected input of one, given to every method that takes an
/// expression, after a column of whole numbers, of nothing, of dates, of text and of lists.
#[test]
fn no_argument_of_two_levels_is_run_into_a_panic_of_polars() {
    let receivers = [
        "a".to_owned(),
        "n".to_owned(),
        "d".to_owned(),
        "g".to_owned(),
        column_of(Representative::List(Inner::Int64)),
    ];
    let present = frame();
    let mut arguments: Vec<(String, Value, bool)> = Vec::new();
    for (prefix, outer_is_array) in DEPTH_TWO_KINDS {
        for element in DEPTH_TWO_ELEMENTS {
            let column = depth_two_name(prefix, element);
            if present.column(&column).is_err() {
                continue;
            }
            let namespace = if outer_is_array { "arr" } else { "list" };
            arguments.push((column.clone(), field(&column), true));
            arguments.push((
                format!("{column} {namespace}.first"),
                field_then(
                    &column,
                    vec![call(&format!("Expr.{namespace}.first"), json!({}))],
                ),
                false,
            ));
            arguments.push((
                format!("connected {column}"),
                chain(json!({ "input": input_of(&column) }), vec![]),
                true,
            ));
        }
    }
    let mut panicked: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut accepted: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut asked = 0;
    for (label, argument, has_no_class) in &arguments {
        let calls = every_method_given(argument);
        for receiver in &receivers {
            for (what, link) in &calls {
                let sensitive = SENSITIVE_METHODS.iter().any(|name| what.starts_with(name));
                for place in [Place::Select, Place::PivotAggregate] {
                    if place == Place::PivotAggregate && !sensitive {
                        continue;
                    }
                    asked += 1;
                    let links = if place == Place::PivotAggregate {
                        vec![link.clone(), call("Expr.first", json!({}))]
                    } else {
                        vec![link.clone()]
                    };
                    let (value, _) = step_of(place, receiver, links);
                    let Ok(document) = Document::read(&document(vec![value])) else {
                        continue;
                    };
                    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        document.run_on(&inputs_of_two_levels(), &frame())
                    }));
                    let key = format!("{what} given {label}");
                    match ran {
                        Err(_) => panicked
                            .entry(key)
                            .or_default()
                            .push(format!("{receiver} in {place:?}")),
                        Ok(Ok(report))
                            if sensitive
                                && *has_no_class
                                && polars_says(&report) == Said::Accepts =>
                        {
                            accepted
                                .entry(key)
                                .or_default()
                                .push(format!("{receiver} in {place:?}"));
                        }
                        Ok(_) => {}
                    }
                }
            }
        }
    }
    let list = |found: &BTreeMap<String, Vec<String>>| {
        found
            .iter()
            .take(40)
            .map(|(chain, places)| format!("{chain}: {}", places.join(", ")))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(
        panicked.is_empty() && accepted.is_empty(),
        "{} chains of {asked} reach a panic of Polars':\n{}\n{} are run with an argument the tables \
         do not judge:\n{}",
        panicked.len(),
        list(&panicked),
        accepted.len(),
        list(&accepted)
    );
}

// ---------------------------------------------------------------------------
// A refused call of lists or arrays is not built

/// The expression `when(g is not) .then(inner) .otherwise(inner) is null` as the second predicate of
/// a condition whose values are text without the suffix an array holds, where the shared `inner`
/// is a call that Polars refuses and that its optimizer then derives the schema of again.
fn a_condition_that_holds_twice(inner: &Value, array: &str) -> Value {
    let value = || {
        chain(
            col_call("g"),
            vec![call(
                "Expr.str.strip_suffix",
                json!({ "suffix": field(array) }),
            )],
        )
    };
    let held = chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![chain(
                col_call("g"),
                vec![call("Expr.not_", json!({}))],
            )]) }),
        ),
        vec![
            call("When.then", json!({ "statement": inner.clone() })),
            call("Then.otherwise", json!({ "statement": inner.clone() })),
            call("Expr.is_null", json!({})),
        ],
    );
    chain(
        call(
            "polars.when",
            json!({ "predicates": many(vec![field("f")]) }),
        ),
        vec![
            call("When.then", json!({ "statement": value() })),
            call("Then.when", json!({ "predicates": many(vec![held]) })),
            call("ChainedWhen.then", json!({ "statement": value() })),
            call("ChainedThen.otherwise", json!({ "statement": value() })),
        ],
    )
}

/// A call of the `list` or the `arr` namespace that the tables refuse, and a call after a list or
/// an array class that they refuse, is not built, so what Polars does with the plan is not asked:
/// it works out the schema of a condition without the call that is shared by both of its branches
/// and accepts it, and its optimizer then panics on the plan, as it does of a `list` method given
/// an array. Every method, of every class, is shared so by a condition after a column, and after
/// one value taken of a column of two levels, which the tables lose the dtype of.
#[test]
fn a_refused_call_of_lists_or_arrays_is_not_run_into_a_panic_of_polars() {
    let array = column_of(Representative::Array(Inner::String));
    let mut starts: Vec<(String, Vec<Value>)> = Vec::new();
    for class in Representative::ALL
        .into_iter()
        .filter(|class| class.is_nested())
    {
        starts.push((column_of(class), Vec::new()));
    }
    for (prefix, outer_is_array) in DEPTH_TWO_KINDS {
        let namespace = if outer_is_array { "arr" } else { "list" };
        for element in [Inner::Null, Inner::String] {
            starts.push((
                depth_two_name(prefix, element),
                vec![call(&format!("Expr.{namespace}.first"), json!({}))],
            ));
        }
    }
    let mut panicked: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    let mut asked = 0;
    for (column, before) in &starts {
        for (namespace, name, arguments) in methods() {
            let mut links = before.clone();
            links.push(call(&format!("Expr.{namespace}.{name}"), arguments.clone()));
            let inner = chain(col_call(column), links);
            let held = a_condition_that_holds_twice(&inner, &array);
            let value = step(
                "s",
                "LazyFrame.select",
                json!({ "exprs": many(vec![held]) }),
            );
            let Ok(read) = Document::read(&document(vec![value])) else {
                continue;
            };
            asked += 1;
            let label = format!("{column} {before:?} {namespace}.{name} {arguments}");
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                read.run_on(&inputs(), &frame())
            }))
            .is_err()
            {
                panicked.entry(label.clone()).or_default().push("run_on");
            }
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                read.rows(
                    &inputs(),
                    &frame(),
                    &StepId::new("s").expect("an identifier"),
                    10,
                )
            }))
            .is_err()
            {
                panicked.entry(label).or_default().push("rows");
            }
        }
    }
    assert!(
        panicked.is_empty(),
        "{} of {asked} documents reach a panic of Polars':\n{}",
        panicked.len(),
        panicked
            .iter()
            .take(40)
            .map(|(label, paths)| format!("{label}: {}", paths.join(", ")))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// A call that Polars refuses after a list or an array is not run into a panic where both branches
/// of a condition share it, whatever it is given: one that is given nothing, one that is given an
/// expression, as a sum with a column that no list takes, and one that is given a column of a
/// dtype that no list takes. Every call that needs no argument, the few that take one, and every
/// method outside the two namespaces that needs expressions alone, given each of a whole number,
/// text, a boolean, a list of whole numbers, a list of nothing, an array of text and an array of
/// three, are shared so after each class of a list or an array, and after a value taken of a
/// column of two levels.
#[test]
fn a_refused_call_after_a_list_or_an_array_is_not_run_into_a_panic_of_polars() {
    let array = column_of(Representative::Array(Inner::String));
    let mut starts: Vec<(String, Vec<Value>)> = Vec::new();
    for class in Representative::ALL
        .into_iter()
        .filter(|class| class.is_nested())
    {
        starts.push((column_of(class), Vec::new()));
    }
    for (prefix, outer_is_array) in DEPTH_TWO_KINDS {
        let namespace = if outer_is_array { "arr" } else { "list" };
        for element in [Inner::Null, Inner::String, Inner::Date] {
            starts.push((
                depth_two_name(prefix, element),
                vec![call(&format!("Expr.{namespace}.first"), json!({}))],
            ));
        }
    }
    let arguments = [
        "a".to_owned(),
        "g".to_owned(),
        "f".to_owned(),
        column_of(Representative::List(Inner::Int64)),
        column_of(Representative::List(Inner::Null)),
        array.clone(),
        WIDE.to_owned(),
    ];
    let mut links: Vec<(String, Value)> = followers();
    links.extend([
        (
            "str.len_chars".to_owned(),
            call("Expr.str.len_chars", json!({})),
        ),
        ("dt.year".to_owned(), call("Expr.dt.year", json!({}))),
    ]);
    for entry in catalog::entries().iter().filter(|entry| {
        entry.applies() == catalog::Applies::Expression
            && !entry.id().as_str().starts_with("Expr.list.")
            && !entry.id().as_str().starts_with("Expr.arr.")
    }) {
        let needed: Vec<_> = entry
            .parameters()
            .iter()
            .filter(|parameter| parameter.is_needed())
            .collect();
        if needed.is_empty()
            || !needed
                .iter()
                .all(|parameter| matches!(parameter.kind(), catalog::Kind::Expression(_)))
        {
            continue;
        }
        for column in &arguments {
            let mut given = serde_json::Map::new();
            for parameter in &needed {
                let value = if parameter.shape() == catalog::Shape::Many {
                    many(vec![field(column)])
                } else {
                    field(column)
                };
                given.insert(parameter.name().to_owned(), value);
            }
            links.push((
                format!("{} given {column}", entry.id().as_str()),
                call(entry.id().as_str(), Value::Object(given)),
            ));
        }
    }
    let mut found = Vec::new();
    let mut asked = 0;
    for (column, before) in &starts {
        for (label, link) in &links {
            let mut chained = before.clone();
            chained.push(link.clone());
            let inner = chain(col_call(column), chained);
            let held = a_condition_that_holds_twice(&inner, &array);
            let value = step(
                "s",
                "LazyFrame.select",
                json!({ "exprs": many(vec![held]) }),
            );
            let Ok(read) = Document::read(&document(vec![value])) else {
                continue;
            };
            asked += 1;
            let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                read.run_on(&inputs(), &frame())
            }));
            if ran.is_err() {
                found.push(format!("{column} {before:?} then {label}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} of {asked} documents reach a panic of Polars':\n{}",
        found.len(),
        found
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// A step that holds a refused method of lists or arrays is not built, and says what the walk says
/// of it: where the call stands and the words that refuse it, as a step that Polars refused would,
/// beside the walk's own refusal.
#[test]
fn a_refused_method_of_lists_is_reported_where_it_stands_with_the_words_of_the_walk() {
    let column = column_of(Representative::Array(Inner::Int64));
    let (value, path) = step_of(
        Place::Select,
        &column,
        vec![call("Expr.list.max", json!({}))],
    );
    let read = Document::read(&document(vec![value])).expect("a document");
    let report = read.run_on(&inputs(), &frame()).expect("a schema");
    let step = &report.steps()[0];
    let walked = step.walk().refusal().expect("the walk refuses it");
    let Outcome::Refused(refusal) = step.outcome() else {
        panic!("the step is refused: {:?}", step.outcome());
    };
    let RunCause::NotAllowed(said) = refusal.cause() else {
        panic!("the step is refused by the walk: {}", refusal.cause());
    };
    let mut at = path;
    at.push(Segment::Link(0));
    assert_eq!(refusal.place().path(), at);
    assert_eq!(walked.place().path(), at);
    assert_eq!(said.reason(), walked.refusal().reason());
    assert!(
        said.reason().contains("expected List data type"),
        "{}",
        said.reason()
    );
}
