//! Whether a window refuses a value that a method took of one value, for every parameter of the
//! catalogue that takes an expression, as Polars does.
//!
//! The value `k.sum().head(1)` is one value that `head` took of one: no row of a group takes it as
//! the value of the group, so Python Polars 2.1.0 refuses a window over a call that is given it,
//! wherever the call meets it, with "expressions must have matching group lengths", and accepts the
//! same call given `k.sum()` or the first value of what `head` took. This builds, for each
//! expression parameter of each method and each function that starts an expression, the call of one
//! value given the taken value for that parameter and the one value `k.first()`-like for the rest,
//! windows it over `k`, and asks Polars and the walk. They must agree. A parameter whose call cannot
//! be built, or does not run in Polars even given one value, is listed with why and checked by name,
//! and a case is never skipped without being said.

use std::collections::BTreeMap;

use dageist_engine::allowed::{RefusalSource, Rule};
use dageist_engine::catalog::{self, Applies, Counting, Dtypes, Entry, Kind, Parameter};
use dageist_engine::document::{
    Call, Document, Dtype, Expression, Inputs, Start, Step, StepId, StepRows, Value,
};
use polars::prelude::*;

/// The sample: a column for each dtype a method takes, of three rows, and `k` to window by.
fn sample() -> DataFrame {
    let ints = |name: &str| Series::new(name.into(), [1_i64, 2, 3]);
    let cast = |name: &str, dtype: DataType| {
        ints(name)
            .cast(&dtype)
            .unwrap_or_else(|error| panic!("whole numbers cast to {dtype}: {error}"))
    };
    DataFrame::new(
        3,
        vec![
            ints("a").into(),
            Series::new("s".into(), ["1d", "2d", "3d"]).into(),
            Series::new("f".into(), [true, false, true]).into(),
            // Two rows in a group of two and one in a group of one, so a value of one row is not the
            // length of every group.
            Series::new("k".into(), [1_i64, 1, 2]).into(),
            cast("ts", DataType::Datetime(TimeUnit::Microseconds, None)).into(),
            cast("d", DataType::Date).into(),
            cast("tm", DataType::Time).into(),
            cast("dur", DataType::Duration(TimeUnit::Microseconds)).into(),
        ],
    )
    .expect("columns of three rows")
}

/// The columns a receiver is tried on, in order.
const RECEIVERS: [&str; 8] = ["a", "s", "f", "ts", "d", "tm", "dur", "k"];

fn function(id: &str) -> &'static Entry {
    catalog::entry(id).unwrap_or_else(|| panic!("the catalogue has no {id}"))
}

fn call(id: &str, arguments: Vec<(&'static str, Value)>) -> Call {
    Call::new(function(id), arguments).unwrap_or_else(|error| panic!("{id}: {error}"))
}

fn column(name: &str) -> Call {
    call(
        "polars.col",
        vec![("name", Value::Columns(vec![name.to_owned()]))],
    )
}

fn number(value: i64) -> Call {
    call("polars.lit", vec![("value", Value::Integer(value))])
}

fn chain(start: Call, links: Vec<Call>) -> Expression {
    Expression::new(Start::Call(start), links).expect("a chain")
}

/// One value that stands for a value of a dtype: the first value of a column, or a number.
fn one_value(column_name: Option<&str>) -> Expression {
    match column_name {
        Some(name) => chain(column(name), vec![call("Expr.first", Vec::new())]),
        None => chain(number(1), Vec::new()),
    }
}

/// `expression` with one value taken of it: its first value, by `head(1)`.
fn taken(expression: &Expression) -> Expression {
    let Start::Call(start) = expression.start() else {
        panic!("a start")
    };
    let mut links = expression.links().to_vec();
    links.push(call(
        "Expr.head",
        vec![("n", Value::Expression(chain(number(1), Vec::new())))],
    ));
    Expression::new(Start::Call(start.clone()), links).expect("a chain")
}

/// The window over `k` that ends the expression.
fn over() -> Call {
    call(
        "Expr.over",
        vec![(
            "partition_by",
            Value::Many(vec![Value::Expression(chain(column("k"), Vec::new()))]),
        )],
    )
}

/// A value for a parameter that is not an expression and must be given, if one fits.
fn plain(parameter: &Parameter, receiver_dtype: &DataType) -> Option<Value> {
    Some(match parameter.kind() {
        Kind::Value(catalog::ValueType::Integer) => Value::Integer(2),
        Kind::Value(catalog::ValueType::Float) => Value::Float(0.5),
        Kind::Value(catalog::ValueType::String) => Value::String("iso".to_owned()),
        Kind::Flag => Value::Flag(Some(false)),
        Kind::Choice(words) => Value::Choice((*words.first()?).to_owned()),
        Kind::Dtype => Value::Dtype(Dtype::new(receiver_dtype.clone()).ok()?),
        Kind::Duration => Value::Duration("1d".to_owned()),
        Kind::Pattern => Value::Pattern("(?<g>a)".to_owned()),
        Kind::TimeZone => Value::TimeZone(Some("UTC".to_owned())),
        _ => return None,
    })
}

/// The arguments of a call of `entry` that give `given` for the expression parameter `tested`, one
/// value for each other expression parameter that must be given, and what the others need.
fn arguments(
    entry: &'static Entry,
    tested: &Parameter,
    given: &Expression,
    filler: &Expression,
    receiver_dtype: &DataType,
) -> Option<Vec<(&'static str, Value)>> {
    let mut all = Vec::new();
    for parameter in entry.parameters() {
        let value = if parameter.name() == tested.name() {
            Value::Expression(given.clone())
        } else if !parameter.is_needed() {
            continue;
        } else if matches!(parameter.kind(), Kind::Expression(_)) {
            Value::Expression(filler.clone())
        } else {
            plain(parameter, receiver_dtype)?
        };
        let value = if parameter.shape() == catalog::Shape::Many {
            Value::Many(vec![value])
        } else {
            value
        };
        all.push((parameter.name(), value));
    }
    Some(all)
}

/// The document of one `select` of `expression`.
fn document_of(expression: Expression) -> Document {
    let select = call(
        "LazyFrame.select",
        vec![("exprs", Value::Many(vec![Value::Expression(expression)]))],
    );
    let step = Step::new(StepId::new("s").expect("an id"), select).expect("a step");
    Document::new(vec![step]).expect("a document")
}

/// What Polars and the walk say of a document.
#[derive(Debug, PartialEq, Eq)]
enum Said {
    /// Polars runs it, or the walk finds nothing refused.
    Accepts,
    /// Polars refuses the lengths of the group, or the walk refuses the window.
    RefusesTheWindow,
    /// Refused for another reason, with the words.
    Other(String),
}

fn polars_says(document: &Document, frame: &DataFrame) -> Said {
    let report = match document.run_on(&Inputs::new(), frame) {
        Ok(report) => report,
        Err(error) => return Said::Other(error.to_string()),
    };
    let step = &report.steps()[0];
    match (step.outcome(), step.rows()) {
        (dageist_engine::document::Outcome::Valid { .. }, StepRows::Counted(_)) => Said::Accepts,
        (dageist_engine::document::Outcome::Valid { .. }, StepRows::Failed(error)) => {
            let text = error.to_string();
            if text.contains("matching group lengths") {
                Said::RefusesTheWindow
            } else {
                Said::Other(text.lines().next().unwrap_or_default().to_owned())
            }
        }
        (dageist_engine::document::Outcome::Refused(refusal), _) => Said::Other(
            refusal
                .to_string()
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned(),
        ),
        _ => Said::Other("not run".to_owned()),
    }
}

fn walk_says(document: &Document, frame: &DataFrame) -> Said {
    let walk = document.steps()[0].walk(0, frame.schema(), &Inputs::new());
    match walk.refusal() {
        None => Said::Accepts,
        Some(refusal)
            if refusal.refusal().source() == RefusalSource::Rule(Rule::WindowInputIsNotCounted) =>
        {
            Said::RefusesTheWindow
        }
        Some(refusal) => Said::Other(refusal.refusal().reason().to_owned()),
    }
}

/// The groups of dtypes that a method of the catalogue accepts, as the columns that are tried for
/// its receiver, the ones of that dtype first.
fn receivers_of(entry: &Entry) -> Vec<&'static str> {
    let first: &[&str] = match entry.accepts().first() {
        Some(Dtypes::Boolean) => &["f"],
        Some(Dtypes::String) => &["s"],
        Some(Dtypes::Datetime) => &["ts"],
        Some(Dtypes::Date) => &["d"],
        Some(Dtypes::Time) => &["tm"],
        Some(Dtypes::Duration) => &["dur"],
        Some(Dtypes::Temporal) => &["ts", "d", "tm", "dur"],
        _ => &[],
    };
    first
        .iter()
        .chain(RECEIVERS.iter())
        .copied()
        .fold(Vec::new(), |mut found, name| {
            if !found.contains(&name) {
                found.push(name);
            }
            found
        })
}

/// What was found of one parameter.
enum Found {
    Agrees,
    Disagrees(String),
    /// The call of one value does not run in Polars with any receiver and filler tried, or cannot
    /// be built, so the taken value was not asked.
    Unbuildable(String),
    /// The call of one value runs, and the taken value gives another refusal than the window's, in
    /// Polars or in the walk.
    TakenRefusedOtherwise(String),
    /// The method takes some of the values it follows, so its call is a value that was taken, or a
    /// count, whatever it is given, and a window refuses it before it is given one: what it meets
    /// cannot be told apart, and the tests of those methods are above.
    ItselfTaken,
}

fn check(entry: &'static Entry, tested: &Parameter, frame: &DataFrame) -> Found {
    if Counting::of(entry, &[]).is_some() {
        return Found::ItselfTaken;
    }
    let is_a_start = entry.applies() == Applies::Function;
    let mut why = String::from("no receiver and filler gave a call that Polars runs");
    let receivers: Vec<Option<&str>> = if is_a_start {
        vec![None]
    } else {
        receivers_of(entry).into_iter().map(Some).collect()
    };
    for receiver in receivers {
        let receiver_dtype = receiver
            .and_then(|name| frame.schema().get(name).cloned())
            .unwrap_or(DataType::Int64);
        for filler_column in [
            receiver,
            Some("a"),
            None,
            Some("f"),
            Some("s"),
            Some("dur"),
            Some("tm"),
        ] {
            let filler = one_value(filler_column);
            let build = |given: &Expression| -> Option<Document> {
                let given_arguments = arguments(entry, tested, given, &filler, &receiver_dtype)?;
                let called = Call::new(entry, given_arguments).ok()?;
                let expression = if is_a_start {
                    let Start::Call(_) = Start::Call(called.clone()) else {
                        return None;
                    };
                    Expression::new(Start::Call(called), vec![over()]).ok()?
                } else {
                    let name = receiver?;
                    Expression::new(
                        Start::Call(column(name)),
                        vec![call("Expr.first", Vec::new()), called, over()],
                    )
                    .ok()?
                };
                Some(document_of(expression))
            };
            let Some(control) = build(&filler) else {
                why = "the arguments of the call could not be built".to_owned();
                continue;
            };
            match polars_says(&control, frame) {
                Said::Accepts => {}
                Said::Other(text) => {
                    why = text;
                    continue;
                }
                Said::RefusesTheWindow => {
                    why = "Polars refuses the window of the call of one value".to_owned();
                    continue;
                }
            }
            if walk_says(&control, frame) != Said::Accepts {
                return Found::Disagrees(format!(
                    "the walk refuses the call of one value {receiver:?} {filler_column:?}: {:?}",
                    walk_says(&control, frame)
                ));
            }
            let Some(taken_document) = build(&taken(&filler)) else {
                return Found::Unbuildable("the taken value could not be given".to_owned());
            };
            let (polars, walk) = (
                polars_says(&taken_document, frame),
                walk_says(&taken_document, frame),
            );
            return match (&polars, &walk) {
                (Said::Other(text), _) | (_, Said::Other(text)) => {
                    Found::TakenRefusedOtherwise(text.clone())
                }
                _ if polars == walk => Found::Agrees,
                _ => Found::Disagrees(format!(
                    "Polars {polars:?} and the walk {walk:?}, with {receiver:?} and {filler_column:?}"
                )),
            };
        }
    }
    Found::Unbuildable(why)
}

#[test]
fn a_window_refuses_a_value_taken_of_one_value_as_polars_does_for_every_parameter_of_an_expression()
{
    let frame = sample();
    let mut disagreements = Vec::new();
    let mut unbuildable: BTreeMap<String, String> = BTreeMap::new();
    let mut otherwise: BTreeMap<String, String> = BTreeMap::new();
    let mut itself_taken: Vec<String> = Vec::new();
    let (mut agreed, mut asked) = (0, 0);
    for entry in catalog::entries() {
        let applies = entry.applies();
        // A window is the value under test, and a condition is built link by link, which the tests
        // of conditions above ask. The steps hold no expression of one value to meet.
        if applies == Applies::Frame
            || applies == Applies::Condition
            || entry.id().as_str() == "Expr.over"
            || (applies == Applies::Function
                && !entry
                    .parameters()
                    .iter()
                    .any(|parameter| matches!(parameter.kind(), Kind::Expression(_))))
        {
            continue;
        }
        for parameter in entry
            .parameters()
            .iter()
            .filter(|parameter| matches!(parameter.kind(), Kind::Expression(_)))
        {
            let name = format!("{} {}", entry.id().as_str(), parameter.name());
            asked += 1;
            match check(entry, parameter, &frame) {
                Found::Agrees => agreed += 1,
                Found::Disagrees(text) => disagreements.push(format!("{name}: {text}")),
                Found::Unbuildable(text) => {
                    unbuildable.insert(name, text);
                }
                Found::TakenRefusedOtherwise(text) => {
                    otherwise.insert(name, text);
                }
                Found::ItselfTaken => itself_taken.push(name),
            }
        }
    }
    eprintln!(
        "asked {asked}, agreed {agreed}, {} of methods that take some of the values",
        itself_taken.len()
    );
    assert!(
        disagreements.is_empty(),
        "{} parameters disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
    // What could not be asked is said, and checked by name: these are all there is.
    assert_eq!(
        unbuildable.keys().map(String::as_str).collect::<Vec<_>>(),
        UNBUILDABLE.map(|(name, _)| name),
        "{unbuildable:?}"
    );
    assert_eq!(
        otherwise.keys().map(String::as_str).collect::<Vec<_>>(),
        REFUSED_BY_BOTH_OTHERWISE.map(|(name, _)| name),
        "{otherwise:?}"
    );
    assert!(asked > 90 && agreed > 50, "asked {asked}, agreed {agreed}");
}

/// The parameters whose call of one value could not be built or does not run in Polars with any
/// receiver and filler tried, so a taken value was not asked of them, and why. None.
const UNBUILDABLE: [(&str, &str); 0] = [];

/// The parameters that Polars and the walk refuse a taken value for, and not by the window: they
/// are read as one value, which a value that was taken of one value is, but Polars refuses a call
/// that is given an expression of a length that is not that of the group before it asks of the
/// window.
const REFUSED_BY_BOTH_OTHERWISE: [(&str, &str); 3] = [
    ("Expr.diff n", "'n' must be a scalar value"),
    (
        "Expr.shift fill_value",
        "'fill_value' must be a scalar value",
    ),
    ("Expr.shift n", "'n' must be a scalar value"),
];
