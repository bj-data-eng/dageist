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

use std::collections::BTreeMap;

use dageist_engine::allowed::{
    After, Count, Inner, RefusalSource, Representative, Shape, dtype_name,
};
use dageist_engine::catalog;
use dageist_engine::document::{
    Document, Inputs, Outcome, Report, Resolution, Segment, StepId, StepRows,
};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::support::{call, chain, col_call, document, field, many, named, step, whole};

/// The methods under test, each with the words that set its options, as Python builds the call.
/// A method that takes an option is asked with its default and with each word.
fn methods() -> Vec<(&'static str, &'static str, Value)> {
    let mut all = Vec::new();
    for namespace in ["list", "arr"] {
        for name in [
            "len", "sum", "mean", "median", "min", "max", "first", "last", "n_unique", "arg_max",
            "arg_min", "reverse",
        ] {
            all.push((namespace, name, json!({})));
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

/// The 38 methods, by the identifiers of the catalogue.
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
    columns.push(ints("a", [Some(1), Some(2), Some(3)]).into());
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
fn the_catalogue_holds_the_thirty_eight_methods() {
    let ids = identifiers();
    assert_eq!(ids.len(), 38, "{ids:?}");
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
