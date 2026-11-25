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
    Call, Document, Dtype, Expression, Inputs, Start, Step, StepId, Value,
};
use polars::prelude::*;

use crate::ask_polars::{self, Connected};

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
            // Shares, which a quantile takes.
            Series::new("q".into(), [0.1_f64, 0.5, 0.9]).into(),
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

/// What Polars says of the document, asked apart from the step engine, which does not run a step
/// the walk refuses.
fn polars_says(document: &Document, frame: &DataFrame) -> Said {
    let nothing = |_: &str| None;
    let connected = Connected::tables(&nothing);
    let step = &document.steps()[0];
    let Some(planned) = ask_polars::plan(step, 0, frame, connected) else {
        return Said::Other("not run".to_owned());
    };
    if let Err(refused) = planned.schema {
        return Said::Other(refused.reason);
    }
    match ask_polars::collect(step, 0, frame, connected, 100) {
        Ok(_) => Said::Accepts,
        Err(text) if text.contains("matching group lengths") => Said::RefusesTheWindow,
        Err(text) => Said::Other(text),
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
        // A rule is named, and Polars' words are given as they were, so what refused it is plain.
        Some(refusal) => Said::Other(match refusal.refusal().source() {
            RefusalSource::Rule(rule) => rule.name().to_owned(),
            _ => refusal.refusal().reason().to_owned(),
        }),
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

/// What was found of one parameter in a window, and as the fill of a `shift`, which Polars reads as
/// one value that it can tell is one before the rows run.
struct Checked {
    window: Found,
    fill: Found,
}

fn check(entry: &'static Entry, tested: &Parameter, frame: &DataFrame) -> Checked {
    if Counting::of(entry, &[]).is_some() {
        return Checked {
            window: Found::ItselfTaken,
            fill: Found::ItselfTaken,
        };
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
            // The call of one value given `given` for the parameter, windowed or not.
            let make = |given: &Expression, windowed: bool| -> Option<Expression> {
                let given_arguments = arguments(entry, tested, given, &filler, &receiver_dtype)?;
                let called = Call::new(entry, given_arguments).ok()?;
                let mut links = if is_a_start {
                    Vec::new()
                } else {
                    vec![call("Expr.first", Vec::new())]
                };
                let start = if is_a_start {
                    Start::Call(called)
                } else {
                    links.push(called);
                    Start::Call(column(receiver?))
                };
                if windowed {
                    links.push(over());
                }
                Expression::new(start, links).ok()
            };
            let build =
                |given: &Expression| -> Option<Document> { make(given, true).map(document_of) };
            // The call of one value, with the call given `given` as the fill of its own shift.
            let as_a_fill = |given: &Expression| -> Option<Document> {
                let control = make(&filler, false)?;
                let mut links = control.links().to_vec();
                links.push(
                    Call::new(
                        function("Expr.shift"),
                        vec![
                            ("n", Value::Expression(chain(number(1), Vec::new()))),
                            ("fill_value", Value::Expression(make(given, false)?)),
                        ],
                    )
                    .ok()?,
                );
                Expression::new(control.start().clone(), links)
                    .ok()
                    .map(document_of)
            };
            let Some(control) = build(&filler) else {
                why = "the arguments of the call could not be built".to_owned();
                continue;
            };
            let fill = scalar_found(&as_a_fill, &filler, &taken(&filler), frame);
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
                return Checked {
                    window: Found::Disagrees(format!(
                        "the walk refuses the call of one value {receiver:?} {filler_column:?}: {:?}",
                        walk_says(&control, frame)
                    )),
                    fill,
                };
            }
            let Some(taken_document) = build(&taken(&filler)) else {
                return Checked {
                    window: Found::Unbuildable("the taken value could not be given".to_owned()),
                    fill,
                };
            };
            let (polars, walk) = (
                polars_says(&taken_document, frame),
                walk_says(&taken_document, frame),
            );
            let window = match (&polars, &walk) {
                // Both refuse it, by another reason than the window's, which the walk names.
                (Said::Other(_), Said::Other(reason)) => {
                    Found::TakenRefusedOtherwise(reason.clone())
                }
                _ if polars == walk => Found::Agrees,
                _ => Found::Disagrees(format!(
                    "Polars {polars:?} and the walk {walk:?}, with {receiver:?} and {filler_column:?}"
                )),
            };
            return Checked { window, fill };
        }
    }
    Checked {
        window: Found::Unbuildable(why.clone()),
        fill: Found::Unbuildable(why),
    }
}

/// The value of one `shift`'s fill: the call of one value given one that a method took of one value
/// for its fill, which Polars refuses, as scalar or not; Polars and the walk must say the same.
fn scalar_found(
    as_a_fill: &dyn Fn(&Expression) -> Option<Document>,
    filler: &Expression,
    taken: &Expression,
    frame: &DataFrame,
) -> Found {
    let Some(control) = as_a_fill(filler) else {
        return Found::Unbuildable("the call could not be given as a fill".to_owned());
    };
    match polars_says(&control, frame) {
        Said::Accepts => {}
        Said::Other(text) => return Found::Unbuildable(text),
        Said::RefusesTheWindow => return Found::Unbuildable("a window".to_owned()),
    }
    if walk_says(&control, frame) != Said::Accepts {
        // The tables measure a fill by the number 1 of the dtype it fills, which a list is not.
        return Found::Unbuildable(format!(
            "the walk refuses the call of one value as a fill: {:?}",
            walk_says(&control, frame)
        ));
    }
    let Some(document) = as_a_fill(taken) else {
        return Found::Unbuildable("the taken value could not be given".to_owned());
    };
    let (polars, walk) = (polars_says(&document, frame), walk_says(&document, frame));
    match (&polars, &walk) {
        (Said::Accepts, Said::Accepts) => Found::Agrees,
        (Said::Other(_), Said::Other(reason)) => Found::TakenRefusedOtherwise(reason.clone()),
        _ => Found::Disagrees(format!("Polars {polars:?} and the walk {walk:?}")),
    }
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
    // What the fill of a `shift` says: the parameters whose taken value both accept, the number both
    // refuse, and the calls that cannot be given as a fill.
    let mut fill_accepted: Vec<String> = Vec::new();
    let mut fill_unbuildable: BTreeMap<String, String> = BTreeMap::new();
    let (mut fills_agreed, mut fills_refused) = (0, 0);
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
            let checked = check(entry, parameter, &frame);
            match checked.window {
                Found::Agrees => agreed += 1,
                Found::Disagrees(text) => disagreements.push(format!("{name}: {text}")),
                Found::Unbuildable(text) => {
                    unbuildable.insert(name.clone(), text);
                }
                Found::TakenRefusedOtherwise(text) => {
                    otherwise.insert(name.clone(), text);
                }
                Found::ItselfTaken => itself_taken.push(name.clone()),
            }
            match checked.fill {
                Found::Agrees => {
                    fills_agreed += 1;
                    fill_accepted.push(name);
                }
                Found::Disagrees(text) => disagreements.push(format!("{name} as a fill: {text}")),
                Found::Unbuildable(text) => {
                    fill_unbuildable.insert(name, text);
                }
                Found::TakenRefusedOtherwise(_) => fills_refused += 1,
                Found::ItselfTaken => {}
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
    eprintln!(
        "fills: {fills_agreed} accepted, {fills_refused} refused, accepted {fill_accepted:?}"
    );
    for (name, text) in &fill_unbuildable {
        eprintln!("fill unbuildable {name}: {text}");
    }
    // Polars reads the fill of a `shift` as a scalar, which only an exact quantile's share leaves a
    // value that a method took of one value to be, of all the parameters there are.
    assert_eq!(fill_accepted, SCALAR_EXCEPTIONS, "{fill_accepted:?}");
    assert_eq!(
        fill_unbuildable
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        UNBUILDABLE_AS_A_FILL,
        "{fill_unbuildable:?}"
    );
    // What could not be asked is said, and checked by name: these are all there is.
    assert_eq!(
        unbuildable.keys().map(String::as_str).collect::<Vec<_>>(),
        UNBUILDABLE.map(|(name, _)| name),
        "{unbuildable:?}"
    );
    assert_eq!(
        otherwise
            .iter()
            .map(|(name, reason)| (name.as_str(), reason.as_str()))
            .collect::<Vec<_>>(),
        REFUSED_BY_BOTH_OTHERWISE,
        "{otherwise:?}"
    );
    assert!(asked > 90 && agreed > 50, "asked {asked}, agreed {agreed}");
}

/// The parameters whose call of one value could not be built or does not run in Polars with any
/// receiver and filler tried, so a taken value was not asked of them, and why. None.
const UNBUILDABLE: [(&str, &str); 0] = [];

/// The parameters that Polars and the walk both refuse a taken value for, and not by the window,
/// with the rule the walk names: Polars reads each as one value that it can tell is one, with "'n'
/// must be a scalar value", which the first value that `head` took of one value is not, though it is
/// one value, and a literal and an aggregate are.
const REFUSED_BY_BOTH_OTHERWISE: [(&str, &str); 3] = [
    ("Expr.diff n", "parameter-takes-one-value"),
    ("Expr.shift fill_value", "parameter-takes-one-value"),
    ("Expr.shift n", "parameter-takes-one-value"),
];

/// The parameters that Polars reads as one value, with the column the values given to them are of
/// and whether Polars refuses even a value that a method took of one value, as `shift` and `diff`
/// do, where the others take it and refuse a count and the rows.
const ONE_VALUE_PARAMETERS: [(&str, &str, &str, bool); 7] = [
    ("Expr.shift", "n", "a", true),
    ("Expr.shift", "fill_value", "a", true),
    ("Expr.diff", "n", "a", true),
    ("Expr.pct_change", "n", "a", false),
    ("Expr.quantile", "quantile", "q", false),
    ("Expr.top_k", "k", "a", false),
    ("Expr.bottom_k", "k", "a", false),
];

#[test]
fn a_parameter_that_polars_reads_as_one_value_is_given_one_as_polars_asks_in_a_select_and_a_window()
{
    let frame = sample();
    let mut failures = Vec::new();
    for (id, parameter, name, strict) in ONE_VALUE_PARAMETERS {
        let entry = function(id);
        let first = || chain(column(name), vec![call("Expr.first", Vec::new())]);
        let forms: [(&str, Expression, bool); 4] = [
            ("one value", first(), false),
            ("a value taken of one value", taken(&first()), strict),
            (
                "a count",
                chain(
                    column(name),
                    vec![call(
                        "Expr.head",
                        vec![("n", Value::Expression(chain(number(2), Vec::new())))],
                    )],
                ),
                true,
            ),
            ("the rows", chain(column(name), Vec::new()), true),
        ];
        for (form, given, refused) in forms {
            for windowed in [false, true] {
                let mut links = vec![call("Expr.first", Vec::new())];
                links.push(
                    Call::new(
                        entry,
                        vec![(
                            parameter_name(entry, parameter),
                            Value::Expression(given.clone()),
                        )],
                    )
                    .expect("the call"),
                );
                if windowed {
                    links.push(over());
                }
                let document =
                    document_of(Expression::new(Start::Call(column("a")), links).expect("a chain"));
                let (polars, walk) = (polars_says(&document, &frame), walk_says(&document, &frame));
                let accepted = |said: &Said| *said == Said::Accepts;
                // A window refuses a value that a method took of one value even where the call
                // takes it, and a count or the rows are refused before the window is asked of.
                // The share of the exact quantile is read once, before the groups, so a window takes
                // what was taken of one value for it, and `top_k` and `bottom_k` take some of the
                // values, so a window refuses their call whatever it is given.
                let carries = id != "Expr.quantile";
                let itself_taken = matches!(id, "Expr.top_k" | "Expr.bottom_k");
                let expected_refusal = refused
                    || (windowed
                        && ((form == "a value taken of one value" && carries) || itself_taken));
                if accepted(&polars) == expected_refusal || accepted(&walk) == expected_refusal {
                    failures.push(format!(
                        "{id} {parameter} given {form}{}: Polars {polars:?}, the walk {walk:?}, \
                         expected {}",
                        if windowed { " in a window" } else { "" },
                        if expected_refusal {
                            "a refusal"
                        } else {
                            "no refusal"
                        },
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} cases disagree:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The name of the parameter `name` of `entry`, as the entry holds it.
fn parameter_name(entry: &Entry, name: &str) -> &'static str {
    entry
        .parameters()
        .iter()
        .find(|parameter| parameter.name() == name)
        .map(Parameter::name)
        .expect("the entry has the parameter")
}

/// The parameters that leave the call a scalar given a value that a method took of one value.
const SCALAR_EXCEPTIONS: [&str; 1] = ["Expr.quantile quantile"];

/// The calls that cannot be asked as the fill of a `shift` beside themselves, which Polars refuses
/// for the dtype it gives, with why.
const UNBUILDABLE_AS_A_FILL: [&str; 1] = [
    // A list: Polars fills a list with a list, where the tables measure a fill by the number 1 of
    // the dtype it fills, which a list is not, so they refuse it.
    "Expr.str.split by",
];
