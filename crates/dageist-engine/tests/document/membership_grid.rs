//! `is_in` and `implode` held to Polars over a grid, both ways, on the shared helper in `grid.rs`.
//!
//! A receiver of every dtype the tables know, and some they do not (the units of a duration and of
//! a moment, a decimal of another precision, a binary, a categorical and an enum), is looked up in
//! every form that can be given for `other`: a column of every class, each column imploded once and
//! twice, a typed literal of every class and each imploded, a null typed as every class and untyped,
//! a column of empty lists, and a connected list as `document/run.rs` builds it, gathered into one
//! list. Each is asked with `nulls_equal` true and false, in a `select` and as a join's key, where
//! the walk folds what it takes for an aggregate. Each step is built by the engine's own functions
//! and asked of Polars, schema first and then its rows, and the walk is held to what Polars does,
//! in each cell:
//!
//! - the walk accepts what Polars accepts;
//! - the walk refuses what Polars refuses;
//! - the only exception is the deprecated form, which Polars accepts with a warning where what is
//!   looked up in is nested as the value is, and which the rule `is-in-takes-a-list` refuses;
//! - every other cell that differs is a line of the test, by receiver and form, with its reason.
//!
//! The labels of the receiver and of the form begin with the depth of nesting of the dtype, which
//! the line of the deprecated form names. A cell that cannot be built is named with its reason. A
//! list typed in has no cell: the editor has no kind for one, as the explanation says of a Python
//! list given as `other`, which waits for a list kind.
//!
//! `implode` is asked on a column of every dtype, imploded once and twice, with `maintain_order`
//! both ways, for where Polars refuses it, and for the dtype it gives.
//!
//! The grid is judged on every core, and takes a minute or less with the engine's tests.

use std::sync::LazyLock;

use dageist_engine::allowed::{After, Inner, Representative};
use dageist_engine::document::Document;
use polars::prelude::*;
use serde_json::{Value, json};

use crate::ask_polars::{self, Connected};
use crate::grid::{Case, Connections, Depth, Grid, Kind, Known, Pattern, Question};
use crate::support::{call, chain, document, field, field_then, many, step};

/// A column of the table, as a receiver and as a form of what is looked up in.
#[derive(Clone)]
struct Col {
    /// What the receiver is called in a report.
    label: String,
    name: String,
    dtype: DataType,
}

fn name_of(label: &str) -> String {
    format!(
        "c_{}",
        label
            .replace(['(', ')', ',', '\'', ' '], "_")
            .replace("μ", "u")
    )
}

/// Three values of the dtype of `class`: one, a null and another, where it can be made of whole
/// numbers.
pub fn values_of(class: Representative, name: &str) -> Option<Series> {
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

/// The dtypes the tables have no class of, or have one of for several units and zones, which a
/// receiver is also asked for: the walk reads them as the class that stands for them, or does not
/// judge them.
fn extra_dtypes() -> Vec<(String, DataType)> {
    let zone = Some(TimeZone::UTC);
    let mut extra = vec![
        (
            "Duration(ns)".to_owned(),
            DataType::Duration(TimeUnit::Nanoseconds),
        ),
        (
            "Duration(ms)".to_owned(),
            DataType::Duration(TimeUnit::Milliseconds),
        ),
        (
            "Datetime(ns)".to_owned(),
            DataType::Datetime(TimeUnit::Nanoseconds, None),
        ),
        (
            "Datetime(ms)".to_owned(),
            DataType::Datetime(TimeUnit::Milliseconds, None),
        ),
        (
            "Datetime(ns, UTC)".to_owned(),
            DataType::Datetime(TimeUnit::Nanoseconds, zone.clone()),
        ),
        (
            "Datetime(ms, UTC)".to_owned(),
            DataType::Datetime(TimeUnit::Milliseconds, zone),
        ),
        ("Decimal(38, 10)".to_owned(), DataType::Decimal(38, 10)),
        ("Binary".to_owned(), DataType::Binary),
    ];
    if let Ok(enumeration) = FrozenCategories::new(["1", "3"]) {
        extra.push((
            "Enum".to_owned(),
            DataType::from_frozen_categories(enumeration),
        ));
    }
    extra.push((
        "Categorical".to_owned(),
        DataType::from_categories(Categories::global()),
    ));
    extra
}

/// The table, with a column for each dtype that can be made, and the columns.
fn table() -> (DataFrame, Vec<Col>, Vec<String>) {
    let mut series: Vec<Series> = Vec::new();
    let mut columns: Vec<Col> = Vec::new();
    let mut unmade: Vec<String> = Vec::new();
    for class in Representative::ALL {
        let label = class.name().to_owned();
        let name = name_of(&label);
        match values_of(class, &name) {
            Some(values) if values.dtype() == &class.dtype() => {
                columns.push(Col {
                    label,
                    name,
                    dtype: values.dtype().clone(),
                });
                series.push(values);
            }
            _ => unmade.push(format!(
                "{class}: no column of it can be made of whole numbers"
            )),
        }
    }
    for (label, dtype) in extra_dtypes() {
        let name = name_of(&label);
        let base = if dtype == DataType::Binary
            || matches!(dtype, DataType::Enum(..) | DataType::Categorical(..))
        {
            Series::new(name.as_str().into(), [Some("1"), None, Some("3")])
        } else {
            Series::new(name.as_str().into(), [Some(1_i64), None, Some(3)])
        };
        match base.cast(&dtype) {
            Ok(values) => {
                columns.push(Col {
                    label,
                    name,
                    dtype: values.dtype().clone(),
                });
                series.push(values);
            }
            Err(error) => unmade.push(format!("{label}: {error}")),
        }
    }
    let empty = ListChunked::from_iter([
        Some(Series::new("".into(), Vec::<i64>::new())),
        None,
        Some(Series::new("".into(), Vec::<i64>::new())),
    ])
    .into_series()
    .with_name("empty_lists".into());
    columns.push(Col {
        label: "empty lists".to_owned(),
        name: "empty_lists".to_owned(),
        dtype: empty.dtype().clone(),
    });
    series.push(empty);
    let frame = DataFrame::new(3, series.into_iter().map(Column::from).collect())
        .expect("columns of three rows");
    (frame, columns, unmade)
}

fn implode() -> Value {
    call("Expr.implode", json!({}))
}

/// The word a document writes the dtype of `class` with, where it holds one.
fn document_word(class: Representative) -> Option<String> {
    dageist_engine::document::Dtype::new(class.dtype())
        .ok()
        .map(|dtype| dtype.to_string())
}

/// A literal that a document types in, of a dtype of `class`.
fn typed_literal(class: Representative) -> Option<Value> {
    if class.is_nested() || class == Representative::Struct {
        return None;
    }
    let dtype = json!({ "dtype": document_word(class)? });
    let value = match class {
        Representative::Int8
        | Representative::Int16
        | Representative::Int32
        | Representative::Int64
        | Representative::Int128
        | Representative::UInt8
        | Representative::UInt16
        | Representative::UInt32
        | Representative::UInt64
        | Representative::UInt128
        | Representative::Date
        | Representative::Time
        | Representative::Datetime
        | Representative::DatetimeZoned
        | Representative::Duration
        | Representative::Decimal => json!({ "value": { "integer": 1 }, "dtype": dtype }),
        Representative::Float16 | Representative::Float32 | Representative::Float64 => {
            json!({ "value": { "float": 1.0 }, "dtype": dtype })
        }
        Representative::String => json!({ "value": { "string": "1" } }),
        Representative::Boolean => json!({ "value": { "flag": true } }),
        Representative::Null => json!({ "value": { "flag": null }, "dtype": dtype }),
        _ => return None,
    };
    Some(call("polars.lit", value))
}

/// A null typed as `class`, where a document can type one.
fn typed_null(class: Representative) -> Option<Value> {
    if class.is_nested() || class == Representative::Struct {
        return None;
    }
    Some(call(
        "polars.lit",
        json!({ "value": { "flag": null }, "dtype": { "dtype": document_word(class)? } }),
    ))
}

/// A form of what is looked up in.
#[derive(Clone)]
struct Form {
    /// The label, which begins with how deeply what it gives is nested.
    label: String,
    /// The expression the step is given, or the name of the connected list.
    other: Value,
    /// The connected list, where the form is one.
    list: Option<(String, Series)>,
}

/// A receiver, which is a column of the table.
#[derive(Clone)]
struct Receiver {
    name: String,
}

struct World {
    frame: DataFrame,
    columns: Vec<Col>,
    forms: Vec<Form>,
    /// What cannot be given, with why.
    unbuildable: Vec<String>,
}

/// The label of a receiver or a form that gives `dtype`: how deeply it is nested, and a question
/// mark before that where no class of the tables is the dtype, which the walk cannot judge.
fn labelled(dtype: &DataType, what: &str) -> String {
    let level = dtype.nesting_level();
    if Representative::of(dtype).is_some() {
        format!("{level} {what}")
    } else {
        format!("?{level} {what}")
    }
}

fn gathered(dtype: &DataType) -> DataType {
    DataType::List(Box::new(dtype.clone()))
}

fn connected_name(class: Representative) -> String {
    format!("list_of_{}", name_of(class.name()))
}

static WORLD: LazyLock<World> = LazyLock::new(|| {
    let (frame, columns, mut unbuildable) = table();
    let mut forms = Vec::new();
    for column in &columns {
        let label = &column.label;
        forms.push(Form {
            label: labelled(&column.dtype, &format!("a column of {label}")),
            other: field(&column.name),
            list: None,
        });
        forms.push(Form {
            label: labelled(
                &gathered(&column.dtype),
                &format!("a column of {label} imploded"),
            ),
            other: field_then(&column.name, vec![implode()]),
            list: None,
        });
        forms.push(Form {
            label: labelled(
                &gathered(&gathered(&column.dtype)),
                &format!("a column of {label} imploded twice"),
            ),
            other: field_then(&column.name, vec![implode(), implode()]),
            list: None,
        });
    }
    for class in Representative::ALL {
        if class.is_nested() || class == Representative::Struct {
            unbuildable.push(format!(
                "a literal of {class}: the editor types a literal in of a scalar dtype only"
            ));
        }
        if !(class.is_nested() || class == Representative::Struct) && document_word(class).is_none()
        {
            unbuildable.push(format!(
                "a literal and a null typed as {class}: a document holds no dtype word for it"
            ));
        }
        if let Some(literal) = typed_literal(class) {
            forms.push(Form {
                label: labelled(&class.dtype(), &format!("a literal of {class}")),
                other: chain(literal.clone(), vec![]),
                list: None,
            });
            forms.push(Form {
                label: labelled(
                    &gathered(&class.dtype()),
                    &format!("a literal of {class} imploded"),
                ),
                other: chain(literal, vec![implode()]),
                list: None,
            });
        }
        if let Some(null) = typed_null(class) {
            forms.push(Form {
                label: labelled(&class.dtype(), &format!("a null typed as {class}")),
                other: chain(null, vec![]),
                list: None,
            });
        }
        // A connected list of the values of each class that can be made, as the run builds it.
        if class != Representative::Struct
            && let Some(list) = values_of(class, "list")
            && list.dtype() == &class.dtype()
        {
            let name = connected_name(class);
            forms.push(Form {
                label: labelled(
                    &gathered(&class.dtype()),
                    &format!("a connected list of {class}"),
                ),
                other: chain(json!({ "input": name }), vec![]),
                list: Some((name, list)),
            });
        }
    }
    // A literal that a call is made of, and then gathered: Polars folds some of these into a literal,
    // which a join's key may hold, and not others.
    let whole = typed_literal(Representative::Int64).expect("a whole number typed in");
    let made = |links: Vec<Value>| chain(whole.clone(), links);
    let cast = || call("Expr.cast", json!({ "dtype": { "dtype": "Int64" } }));
    let list = gathered(&DataType::Int64);
    for (what, links) in [
        ("cast", vec![cast(), implode()]),
        ("cast twice", vec![cast(), cast(), implode()]),
        (
            "added to a literal",
            vec![
                call("Expr.add", json!({ "other": chain(whole.clone(), vec![]) })),
                implode(),
            ],
        ),
        (
            "made absolute",
            vec![call("Expr.abs", json!({})), implode()],
        ),
    ] {
        forms.push(Form {
            label: labelled(&list, &format!("a literal {what}, imploded")),
            other: made(links),
            list: None,
        });
    }
    forms.push(Form {
        label: labelled(&DataType::Null, "a null"),
        other: chain(
            call("polars.lit", json!({ "value": { "flag": null } })),
            vec![],
        ),
        list: None,
    });
    forms.push(Form {
        label: labelled(&gathered(&DataType::Int64), "empty lists"),
        other: field("empty_lists"),
        list: None,
    });
    unbuildable.push(
        "a list typed in, fitting, unfitting or empty: the editor has no kind for one".to_owned(),
    );
    World {
        frame,
        columns,
        forms,
        unbuildable,
    }
});

/// What a cell chooses.
#[derive(Default)]
struct Point {
    receiver: Option<Receiver>,
    form: Option<Form>,
    nulls_equal: bool,
    key: bool,
}

fn right_table() -> DataFrame {
    df!("rf" => [true, false, true]).expect("the other table")
}

fn is_in_given(point: &Point) -> Result<Case, String> {
    let receiver = point.receiver.as_ref().ok_or("no receiver")?;
    let form = point.form.as_ref().ok_or("no form")?;
    let looked_up = field_then(
        &receiver.name,
        vec![call(
            "Expr.is_in",
            json!({ "other": form.other, "nulls_equal": { "flag": point.nulls_equal } }),
        )],
    );
    let (entry, arguments) = if point.key {
        (
            "LazyFrame.join",
            json!({
                "other": { "table": "other" },
                "left_on": many(vec![looked_up]),
                "right_on": many(vec![field("rf")]),
            }),
        )
    } else {
        (
            "LazyFrame.select",
            json!({ "exprs": many(vec![looked_up]) }),
        )
    };
    let mut connections = Connections::new().table("other", right_table());
    if let Some((name, list)) = &form.list {
        connections = connections.membership_list(name, list.clone());
    }
    Case::of_step(entry, arguments, WORLD.frame.clone(), connections)
}

fn is_in_grid() -> Grid<Point> {
    let receivers: Vec<(String, Receiver)> = WORLD
        .columns
        .iter()
        .map(|column| {
            let label = labelled(&column.dtype, &column.label);
            (
                label.clone(),
                Receiver {
                    name: column.name.clone(),
                },
            )
        })
        .collect();
    let forms: Vec<(String, Form)> = WORLD
        .forms
        .iter()
        .map(|form| (form.label.clone(), form.clone()))
        .collect();
    Grid::<Point>::new()
        .axis("receiver", receivers, |point, receiver| {
            point.receiver = Some(receiver.clone());
        })
        .axis("form", forms, |point, form| {
            point.form = Some(form.clone());
        })
        .axis(
            "nulls_equal",
            vec![("false".to_owned(), false), ("true".to_owned(), true)],
            |point, equal| point.nulls_equal = *equal,
        )
        .axis(
            "place",
            vec![("select".to_owned(), false), ("key".to_owned(), true)],
            |point, key| point.key = *key,
        )
}

/// What the grid excuses and names, which is the deprecated form and nothing else unless a line
/// says so, with the reason.
fn known() -> Vec<Known> {
    let mut known = Vec::new();
    // The deprecated form: what is looked up in is nested as the value is, which Polars accepts with
    // a warning that it is ambiguous (pola-rs/polars#22149). The rule refuses it.
    for level in ["0 ", "1 "] {
        known.push(
            Known::new(
                Kind::WalkRefusesOnly,
                Pattern::new()
                    .starts("receiver", level)
                    .starts("form", level),
                "the deprecated form, nested as the value is, which the rule is-in-takes-a-list refuses and Polars accepts with a warning (pola-rs/polars#22149)",
            )
            .saying("rule is-in-takes-a-list"),
        );
    }
    // A dtype that no class of the tables is: a list of lists, of structs, and a binary, a
    // categorical and an enum. The walk cannot tell what a collection of it is looked up in, or what
    // is looked up in it, and leaves the call to Polars, which refuses it when the step runs where it
    // does. Stated here to be filed: the walk would need the depth and the element of a dtype that
    // has no class.
    known.push(
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("form", "?"),
            "what is looked up in is of a dtype with no class of the tables (a list of lists, of structs, a binary, a categorical, an enum), so the walk cannot tell that Polars refuses it and leaves it to the step's run",
        )
        .saying("cannot check for"),
    );
    known.push(
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("receiver", "?"),
            "the value is of a dtype with no class of the tables (a binary, a categorical, an enum), so the walk cannot tell what it may be looked up in and leaves it to the step's run",
        )
        .saying("cannot check for"),
    );
    // The deprecated form where one side is of such a dtype: the walk does not know the dtype, so it
    // cannot tell that the nesting is equal, and only the step's run refuses it by the rule. Polars
    // injects an implode for the deprecated form, which leaves a join's key not elementwise.
    for (receiver, form) in [("?0", "?0"), ("?0", "0 "), ("0 ", "?0")] {
        known.push(
            Known::new(
                Kind::PolarsRefusesOnly,
                Pattern::new()
                    .is("place", "key")
                    .starts("receiver", receiver)
                    .starts("form", form),
                "the deprecated form with a dtype of no class on one side: Polars gathers what is looked up in into a list for it, which leaves a join's key not elementwise, and the walk, which does not know the dtype, cannot tell that it is the deprecated form",
            )
            .saying("elementwise"),
        );
    }
    known
}

#[test]
fn is_in_given_each_form_of_other_is_what_polars_says_in_a_select_and_as_a_key() {
    assert!(
        WORLD.columns.len() > 60,
        "{} columns were made",
        WORLD.columns.len()
    );
    eprintln!(
        "not buildable as the grid's cells:\n  {}",
        WORLD.unbuildable.join("\n  ")
    );
    let verdicts = is_in_grid().judge(&Question::any_refusal().at(Depth::Rows), is_in_given);
    verdicts.assert_agree(&known(), 10_000);
}

/// The receiver and what `implode` is given.
#[derive(Default)]
struct Imploded {
    receiver: Option<Receiver>,
    times: usize,
    order: Option<bool>,
}

#[test]
fn implode_is_refused_where_polars_refuses_it_and_gives_the_dtype_polars_gives() {
    let receivers: Vec<(String, Receiver)> = WORLD
        .columns
        .iter()
        .map(|column| {
            (
                column.label.clone(),
                Receiver {
                    name: column.name.clone(),
                },
            )
        })
        .collect();
    let verdicts = Grid::<Imploded>::new()
        .axis("receiver", receivers, |point, receiver| {
            point.receiver = Some(receiver.clone());
        })
        .axis(
            "times",
            vec![("once".to_owned(), 1_usize), ("twice".to_owned(), 2)],
            |point, times| point.times = *times,
        )
        .axis(
            "maintain_order",
            vec![
                ("default".to_owned(), None),
                ("true".to_owned(), Some(true)),
                ("false".to_owned(), Some(false)),
            ],
            |point, order| point.order = *order,
        )
        .judge(&Question::any_refusal().at(Depth::Rows), |point| {
            let receiver = point.receiver.as_ref().ok_or("no receiver")?;
            let link = match point.order {
                None => call("Expr.implode", json!({})),
                Some(keep) => call(
                    "Expr.implode",
                    json!({ "maintain_order": { "flag": keep } }),
                ),
            };
            let expression = field_then(&receiver.name, vec![link; point.times]);
            Case::of_step(
                "LazyFrame.select",
                json!({ "exprs": many(vec![expression]) }),
                WORLD.frame.clone(),
                Connections::new(),
            )
        });
    verdicts.assert_agree(&[], 100);
    // The dtype it gives, asked of Polars and of the walk for each receiver.
    let mut disagreements: Vec<String> = Vec::new();
    let mut checked = 0;
    for column in &WORLD.columns {
        for times in [1_usize, 2] {
            let expression = field_then(&column.name, vec![implode(); times]);
            let document = Document::read(&document(vec![step(
                "s",
                "LazyFrame.select",
                json!({ "exprs": many(vec![expression]) }),
            )]))
            .expect("a document");
            let step = &document.steps()[0];
            let none = |_: &str| None;
            let polars = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ask_polars::plan(step, 0, &WORLD.frame, Connected::tables(&none))
            }))
            .ok()
            .flatten()
            .and_then(|planned| planned.schema.ok())
            .and_then(|schema| schema.iter().next().map(|(_, dtype)| dtype.clone()));
            let walk = step.walk(
                0,
                WORLD.frame.schema(),
                &dageist_engine::document::Inputs::new(),
            );
            let walked = walk
                .expressions()
                .iter()
                .find(|expression| expression.place().path().len() == 2)
                .and_then(|expression| expression.places().last().copied().flatten())
                .and_then(|place| match place.after() {
                    After::Value { dtype, .. } => Some(dtype),
                    _ => None,
                });
            if let Some(dtype) = polars
                && walk.refusal().is_none()
                && walk.not_known().is_none()
            {
                checked += 1;
                if walked != Representative::of(&dtype) {
                    disagreements.push(format!(
                        "{} imploded {times} time(s): Polars gives {dtype:?}, the walk {walked:?}",
                        column.label
                    ));
                }
            }
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} cells disagree on the dtype:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
    assert!(checked > 60, "{checked} dtypes were compared");
}
