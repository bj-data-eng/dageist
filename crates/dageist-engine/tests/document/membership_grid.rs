//! `is_in` and `implode` held to Polars over a grid: a receiver of each dtype the tables measure,
//! and for `other` each form that can be given: a column of every dtype, the same imploded, a
//! literal of the receiver's dtype, a null, and a column of empty lists. Each key is a step that is
//! built by the engine's own build functions and asked of Polars, as the step engine would run it,
//! and the walk's refusals are held to what Polars does.
//!
//! The walk may refuse what Polars accepts only where it is stricter by design: a collection nested
//! as the receiver is, the form Polars deprecates, and a list that the tables measure no class of.
//! It may never accept what Polars refuses. `implode` is asked on a column of every dtype, for the
//! dtype it gives and for where it is refused.

use std::collections::BTreeMap;

use dageist_engine::allowed::{After, Inner, RefusalSource, Representative, Rule};
use dageist_engine::document::{Document, Inputs};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::ask_polars::{self, Connected};
use crate::support::{call, chain, document, field, field_then, many, step};

/// The name of the column that holds values of `class`.
fn column_of(class: Representative) -> String {
    format!(
        "c_{}",
        class.name().replace(['(', ')', ','], "_").replace(' ', "")
    )
}

/// Three values of `class`: one, a null and another, where the dtype can be made of whole numbers.
fn values_of(class: Representative) -> Option<Series> {
    let name = column_of(class);
    let name = name.as_str();
    if let Some(inner) = class.element() {
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
        return lists
            .into_series()
            .with_name(name.into())
            .cast(&class.dtype())
            .ok();
    }
    match class {
        Representative::Struct => {
            let fields = [
                Series::new("x".into(), [Some(1_i64), None, Some(3)]),
                Series::new("y".into(), [Some("a"), None, Some("c")]),
            ];
            StructChunked::from_series(name.into(), 3, fields.iter())
                .ok()
                .map(IntoSeries::into_series)
        }
        Representative::Null => Some(Series::new_null(name.into(), 3)),
        _ => Series::new(name.into(), [Some(1_i64), None, Some(3)])
            .cast(&class.dtype())
            .ok(),
    }
}

/// The table: a column of every class that can be made, and one of empty lists.
fn table() -> (DataFrame, Vec<Representative>) {
    let mut columns: Vec<Column> = Vec::new();
    let mut classes = Vec::new();
    for class in Representative::ALL {
        if let Some(series) = values_of(class)
            && series.dtype() == &class.dtype()
        {
            columns.push(series.into());
            classes.push(class);
        }
    }
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
    (
        DataFrame::new(3, columns).expect("columns of three rows"),
        classes,
    )
}

fn implode() -> Value {
    call("Expr.implode", json!({}))
}

/// A literal of the dtype of `class`, where a document can type one in.
fn literal_of(class: Representative) -> Option<Value> {
    let value = match class {
        Representative::Int8
        | Representative::Int16
        | Representative::Int32
        | Representative::Int64
        | Representative::UInt8
        | Representative::UInt16
        | Representative::UInt32
        | Representative::UInt64 => {
            json!({ "value": { "integer": 1 }, "dtype": { "dtype": class.name() } })
        }
        Representative::Float64 => {
            json!({ "value": { "float": 1.0 }, "dtype": { "dtype": "Float64" } })
        }
        Representative::String => json!({ "value": { "string": "1" } }),
        Representative::Boolean => json!({ "value": { "flag": true } }),
        _ => return None,
    };
    Some(chain(call("polars.lit", value), vec![]))
}

/// What Polars says of the step: `None` where it accepts it and runs it on the rows, and the first
/// line of why not otherwise, a panic included.
fn polars_says(document: &Document, frame: &DataFrame) -> Option<String> {
    let step = &document.steps()[0];
    let none = |_: &str| None;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ask_polars::refusal_of(step, 0, frame, Connected::tables(&none))
    }))
    .unwrap_or_else(|_| Some("Polars panics".to_owned()))
}

/// The dtype Polars gives the one column the step selects, where it plans the step.
fn polars_dtype(document: &Document, frame: &DataFrame) -> Option<DataType> {
    let step = &document.steps()[0];
    let none = |_: &str| None;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ask_polars::plan(step, 0, frame, Connected::tables(&none))
    }))
    .ok()
    .flatten()
    .and_then(|planned| planned.schema.ok())
    .and_then(|schema| schema.iter().next().map(|(_, dtype)| dtype.clone()))
}

fn selecting(expression: Value) -> Document {
    Document::read(&document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "exprs": many(vec![expression]) }),
    )]))
    .expect("a document")
}

/// What the walk says of the step: refused by a source, not known, or allowed with the class and
/// shape of the last call.
enum Walked {
    Refused(RefusalSource),
    NotKnown,
    Allowed(Option<After>),
}

fn walked(document: &Document, frame: &DataFrame) -> Walked {
    let walk = document.steps()[0].walk(0, frame.schema(), &Inputs::new());
    if let Some(refusal) = walk.refusal() {
        return Walked::Refused(refusal.refusal().source());
    }
    if walk.not_known().is_some() {
        return Walked::NotKnown;
    }
    Walked::Allowed(
        walk.expressions()
            .iter()
            .find(|expression| expression.place().path().len() == 2)
            .and_then(|expression| expression.places().last().copied().flatten())
            .map(|place| place.after()),
    )
}

#[test]
fn is_in_given_each_form_of_other_is_what_polars_says_or_stricter_by_design() {
    let (frame, classes) = table();
    assert!(classes.len() > 40, "{} classes were made", classes.len());
    let mut disagreements: Vec<String> = Vec::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let receivers: Vec<Representative> = Representative::MEASURED
        .into_iter()
        .filter(|class| classes.contains(class))
        .collect();
    for receiver in &receivers {
        let name = column_of(*receiver);
        let mut forms: Vec<(String, Value)> = Vec::new();
        for class in &classes {
            forms.push((format!("a column of {class}"), field(&column_of(*class))));
            forms.push((
                format!("a column of {class} imploded"),
                field_then(&column_of(*class), vec![implode()]),
            ));
        }
        forms.push((
            "a null".to_owned(),
            chain(
                call("polars.lit", json!({ "value": { "flag": null } })),
                vec![],
            ),
        ));
        forms.push(("empty lists".to_owned(), field("empty_lists")));
        if let Some(literal) = literal_of(*receiver) {
            forms.push(("a literal of its dtype".to_owned(), literal.clone()));
            let imploded = chain(literal["expression"]["start"].clone(), vec![implode()]);
            forms.push(("a literal of its dtype imploded".to_owned(), imploded));
        }
        for (form, other) in forms {
            let key = format!("{receiver} looked up in {form}");
            let document = selecting(field_then(
                &name,
                vec![call("Expr.is_in", json!({ "other": other }))],
            ));
            let polars = polars_says(&document, &frame);
            match (walked(&document, &frame), polars) {
                (Walked::Allowed(_), None) => *counts.entry("both allow").or_default() += 1,
                (Walked::Refused(_), Some(_)) => *counts.entry("both refuse").or_default() += 1,
                (Walked::NotKnown, _) => *counts.entry("the walk cannot tell").or_default() += 1,
                (Walked::Refused(RefusalSource::Rule(Rule::IsInTakesAList)), None) => {
                    *counts
                        .entry("refused by the rule that asks for a list, which Polars deprecates")
                        .or_default() += 1;
                }
                (Walked::Refused(RefusalSource::Rule(Rule::ElementDtypeIsMeasured)), None) => {
                    *counts.entry("refused as not measured").or_default() += 1;
                }
                (Walked::Refused(source), None) => {
                    disagreements.push(format!(
                        "{key}: the walk refuses by {source:?}, Polars allows it"
                    ));
                }
                (Walked::Allowed(_), Some(said)) => {
                    disagreements
                        .push(format!("{key}: the walk allows it, Polars refuses: {said}"));
                }
            }
        }
    }
    eprintln!("is_in grid: {counts:#?}");
    assert!(
        disagreements.is_empty(),
        "{} of the grid's cases disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
    assert!(
        counts.get("both allow").copied().unwrap_or(0) > 50,
        "{counts:?}"
    );
}

#[test]
fn implode_gives_the_dtype_polars_gives_and_is_refused_where_polars_refuses_it() {
    let (frame, classes) = table();
    let mut disagreements: Vec<String> = Vec::new();
    let mut checked = 0;
    for class in &classes {
        for links in [vec![implode()], vec![implode(), implode()]] {
            let document = selecting(field_then(&column_of(*class), links.clone()));
            let key = format!("{class} imploded {} time(s)", links.len());
            let polars_refusal = polars_says(&document, &frame);
            let dtype = polars_dtype(&document, &frame);
            match (walked(&document, &frame), polars_refusal, dtype) {
                (Walked::Allowed(after), None, Some(dtype)) => {
                    checked += 1;
                    let expected = Representative::of(&dtype);
                    let walked_class = after.and_then(|after| match after {
                        After::Value { dtype, .. } => Some(dtype),
                        _ => None,
                    });
                    if walked_class != expected {
                        disagreements.push(format!(
                            "{key}: Polars gives {dtype:?}, the walk {walked_class:?}"
                        ));
                    }
                }
                (Walked::Refused(_), Some(_), _) => {}
                (Walked::NotKnown, _, _) => {}
                (Walked::Refused(source), None, _) => {
                    disagreements.push(format!(
                        "{key}: the walk refuses by {source:?}, Polars allows it"
                    ));
                }
                (Walked::Allowed(_), Some(said), _) => {
                    disagreements
                        .push(format!("{key}: the walk allows it, Polars refuses: {said}"));
                }
                (Walked::Allowed(_), None, None) => {}
            }
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} of the grid's cases disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
    assert!(checked > 40, "{checked} dtypes were compared");
}
