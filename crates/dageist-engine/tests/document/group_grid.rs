//! A step that groups held to Polars over grids, both ways, on the shared helper in `grid.rs`.
//!
//! The table has a column of every class the tables of allowed combinations know, `k` (text with a
//! repeat) and `n` (whole numbers with a repeat). Each step is built by the engine's own functions
//! and asked of Polars, schema first and then its rows, and the walk is held to what Polars does,
//! in each cell: the walk accepts what Polars accepts and refuses what Polars refuses, and every
//! other cell is a line of the test, by its axes, with its reason.
//!
//! - **Keys**: a key of every dtype in every form it can be given in, a column, a named key, an
//!   expression, an aggregate, a literal, two of one name, a window, with `maintain_order` set and
//!   unset.
//! - **Aggregates**: every method of the family of aggregates, and the first, the last and the
//!   gathering of the values, in `agg`, over every class, given by position and by name.
//! - **Expressions that do not aggregate**: a column and what is made of its values, in `agg`,
//!   which give a list for each group, over every class.
//! - **Shortcuts**: each of the shortcuts, with each of its options, over every class, which
//!   aggregates every column that is not a key.
//! - **Predicates**: aggregating and not, of every class, in a `having`.
//! - **Chains that are not well formed**: refused before Polars is asked, by the document, which
//!   makes each cell unbuildable with the reason.
//!
//! The grids are judged on every core and take a minute or less with the engine's tests.

use std::sync::LazyLock;

use dageist_engine::allowed::Representative;
use dageist_engine::catalog::{self, Applies, Family, Kind as ParameterKind, Role};
use polars::prelude::*;
use serde_json::{Value, json};

use crate::grid::{Case, Connections, Depth, Grid, Kind, Known, Pattern, Question};
use crate::membership_grid::values_of;
use crate::support::{call, field, field_then, many, named, whole};

/// A column of the table.
#[derive(Clone)]
struct Col {
    label: String,
    name: String,
}

struct World {
    frame: DataFrame,
    columns: Vec<Col>,
    unmade: Vec<String>,
}

fn name_of(label: &str) -> String {
    format!(
        "c_{}",
        label
            .replace(['(', ')', ',', '\'', ' '], "_")
            .replace('μ', "u")
    )
}

static WORLD: LazyLock<World> = LazyLock::new(|| {
    let mut series = vec![
        Series::new("k".into(), ["a", "b", "a"]),
        Series::new("n".into(), [1_i64, 1, 2]),
    ];
    let mut columns = Vec::new();
    let mut unmade = Vec::new();
    for class in Representative::ALL {
        let label = class.name().to_owned();
        let name = name_of(&label);
        match values_of(class, &name) {
            Some(values) if values.dtype() == &class.dtype() => {
                series.push(values);
                columns.push(Col { label, name });
            }
            _ => unmade.push(format!(
                "{class}: no column of it can be made of whole numbers"
            )),
        }
    }
    let frame = DataFrame::new(3, series.into_iter().map(Column::from).collect())
        .expect("columns of three rows");
    World {
        frame,
        columns,
        unmade,
    }
});

fn classes() -> Vec<(String, Col)> {
    WORLD
        .columns
        .iter()
        .map(|column| (column.label.clone(), column.clone()))
        .collect()
}

/// The table of the key `n` and the one column `column`, which a shortcut aggregates.
fn of_one(column: &Col) -> DataFrame {
    WORLD
        .frame
        .select(["n", column.name.as_str()])
        .expect("the columns of the world")
}

fn by(keys: Vec<Value>) -> Value {
    json!({ "by": many(keys) })
}

fn finishing(entry: &str, arguments: Value) -> Value {
    call(entry, arguments)
}

fn len() -> Value {
    finishing("LazyGroupBy.len", json!({}))
}

fn of(column: &Col, links: Vec<Value>) -> Value {
    field_then(&column.name, links)
}

fn flag(value: bool) -> Value {
    json!({ "flag": value })
}

fn lit(value: Value) -> Value {
    crate::support::chain(call("polars.lit", json!({ "value": value })), vec![])
}

// ---------------------------------------------------------------------------
// Keys

#[derive(Default)]
struct Key {
    column: Option<Col>,
    form: &'static str,
    order: Option<bool>,
}

/// The arguments of `group_by` for a key of `column` in `form`.
fn key_arguments(column: &Col, form: &str, order: Option<bool>) -> Value {
    let name = column.name.as_str();
    let mut arguments = match form {
        "column" => by(vec![field(name)]),
        "named" => json!({ "named_by": named(vec![("key", field(name))]) }),
        "named, a name given twice" => {
            json!({ "named_by": named(vec![("key", field(name)), ("key", field("n"))]) })
        }
        "is_null" => by(vec![of(column, vec![call("Expr.is_null", json!({}))])]),
        "first, an aggregate" => by(vec![of(column, vec![call("Expr.first", json!({}))])]),
        "sorted" => by(vec![of(column, vec![call("Expr.sort", json!({}))])]),
        "head, some of the values" => by(vec![of(
            column,
            vec![call("Expr.head", json!({ "n": whole(1) }))],
        )]),
        "implode, one list" => by(vec![of(column, vec![call("Expr.implode", json!({}))])]),
        "window by n" => by(vec![of(
            column,
            vec![call(
                "Expr.over",
                json!({ "partition_by": many(vec![field("n")]) }),
            )],
        )]),
        "literal" => by(vec![lit(json!({ "integer": 1 }))]),
        "the column and n" => by(vec![field(name), field("n")]),
        "the column twice" => by(vec![field(name), field(name)]),
        other => panic!("no form {other}"),
    };
    if let Some(keep) = order {
        arguments["maintain_order"] = flag(keep);
    }
    arguments
}

const KEY_FORMS: [&str; 12] = [
    "column",
    "named",
    "named, a name given twice",
    "is_null",
    "first, an aggregate",
    "sorted",
    "head, some of the values",
    "implode, one list",
    "window by n",
    "literal",
    "the column and n",
    "the column twice",
];

fn key_case(point: &Key) -> Result<Case, String> {
    let column = point.column.as_ref().ok_or("no column")?;
    Case::of_grouping(
        key_arguments(column, point.form, point.order),
        vec![len()],
        WORLD.frame.clone(),
        Connections::new(),
    )
}

fn labelled<T: Clone>(items: &[(&str, T)]) -> Vec<(String, T)> {
    items
        .iter()
        .map(|(label, item)| ((*label).to_owned(), item.clone()))
        .collect()
}

fn keys_known() -> Vec<Known> {
    Vec::new()
}

#[test]
fn a_key_of_every_dtype_in_every_form_is_what_polars_says() {
    eprintln!("not made as columns:\n  {}", WORLD.unmade.join("\n  "));
    let forms: Vec<(String, &'static str)> = KEY_FORMS
        .iter()
        .map(|form| ((*form).to_owned(), *form))
        .collect();
    let verdicts = Grid::<Key>::new()
        .axis("class", classes(), |point, column| {
            point.column = Some(column.clone());
        })
        .axis("form", forms, |point, form| point.form = form)
        .axis(
            "maintain_order",
            labelled(&[
                ("default", None),
                ("true", Some(true)),
                ("false", Some(false)),
            ]),
            |point, order| point.order = *order,
        )
        .judge(&Question::any_refusal().at(Depth::Rows), key_case);
    verdicts.assert_agree(&keys_known(), 500);
}

// ---------------------------------------------------------------------------
// Aggregates

/// The arguments an entry must be given, as a document holds them, or why a cell cannot be made.
fn required_arguments(entry: &catalog::Entry) -> Result<Value, String> {
    let mut given = serde_json::Map::new();
    for parameter in entry
        .parameters()
        .iter()
        .filter(|parameter| parameter.is_needed())
    {
        let value = match parameter.kind() {
            ParameterKind::Expression(_) if parameter.role() == Role::Share => {
                lit(json!({ "float": 0.5 }))
            }
            ParameterKind::Expression(_) => whole(1),
            ParameterKind::Value(catalog::ValueType::Integer) => json!({ "integer": 1 }),
            ParameterKind::Value(catalog::ValueType::Float) => json!({ "float": 0.5 }),
            ParameterKind::Value(catalog::ValueType::String) => json!({ "string": "a" }),
            ParameterKind::Choice(words) => json!({ "choice": words[0] }),
            ParameterKind::Flag => flag(true),
            _ => {
                return Err(format!(
                    "{} needs {}, which the grid has no argument for",
                    entry.id().as_str(),
                    parameter.name()
                ));
            }
        };
        given.insert(parameter.name().to_owned(), value);
    }
    Ok(Value::Object(given))
}

/// The methods an aggregate is made of: the family of aggregates, and the values of a group taken
/// whole or by its first and last.
fn aggregates() -> Vec<(String, &'static catalog::Entry)> {
    catalog::entries()
        .iter()
        .filter(|entry| {
            entry.applies() == Applies::Expression
                && (entry.family() == Family::Aggregate
                    || matches!(
                        entry.id().as_str(),
                        "Expr.first" | "Expr.last" | "Expr.implode"
                    ))
        })
        .map(|entry| (entry.id().as_str().to_owned(), entry))
        .collect()
}

#[derive(Default)]
struct Aggregated {
    column: Option<Col>,
    entry: Option<&'static catalog::Entry>,
    named: bool,
}

fn aggregate_case(point: &Aggregated) -> Result<Case, String> {
    let column = point.column.as_ref().ok_or("no column")?;
    let entry = point.entry.ok_or("no entry")?;
    let link = call(entry.id().as_str(), required_arguments(entry)?);
    let aggregate = of(column, vec![link]);
    let aggs = if point.named {
        json!({ "named_aggs": named(vec![("out", aggregate)]) })
    } else {
        json!({ "aggs": many(vec![aggregate]) })
    };
    Case::of_grouping(
        by(vec![field("n")]),
        vec![finishing("LazyGroupBy.agg", aggs)],
        WORLD.frame.clone(),
        Connections::new(),
    )
}

fn aggregates_known() -> Vec<Known> {
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("method", "Expr.approx_quantile"),
            "the tables do not measure approx_quantile on a list or an array of that element dtype, which Polars may panic on, so the rule element-dtype-is-measured leaves it out",
        )
        .saying("rule element-dtype-is-measured"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new()
                .one_of("method", &["Expr.kurtosis", "Expr.skew"])
                .is("class", "String"),
            "in a select Polars converts the text to numbers strictly and refuses when the rows run, which the tables record; in a group by it gives a result, and the walk keeps the refusal the tables record for the method",
        )
        .saying("conversion from"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().one_of(
                "method",
                &[
                    "Expr.arr.mean",
                    "Expr.arr.median",
                    "Expr.arr.std",
                    "Expr.arr.var",
                    "Expr.list.std",
                    "Expr.list.var",
                ],
            ),
            "in a select Polars works out a schema of the element's own dtype for these and gives floats when the rows run, which the tables refuse; in a group by it does neither, and the walk keeps the refusal the tables record for the method",
        )
        .saying("the schema says"),
    ]
}

#[test]
fn every_aggregate_over_every_class_is_what_polars_says_in_agg() {
    let verdicts = Grid::<Aggregated>::new()
        .axis("method", aggregates(), |point, entry| {
            point.entry = Some(entry)
        })
        .axis("class", classes(), |point, column| {
            point.column = Some(column.clone());
        })
        .axis(
            "given",
            labelled(&[("by position", false), ("by name", true)]),
            |point, named| point.named = *named,
        )
        .judge(&Question::any_refusal().at(Depth::Rows), aggregate_case);
    verdicts.assert_agree(&aggregates_known(), 1_000);
}

// ---------------------------------------------------------------------------
// Expressions that do not aggregate

#[derive(Default)]
struct Listed {
    column: Option<Col>,
    links: &'static str,
}

const NOT_AGGREGATING: [&str; 16] = [
    "the column",
    "sort",
    "reverse",
    "unique",
    "drop_nulls",
    "is_null",
    "head 1",
    "shift 1",
    "cum_count",
    "implode",
    "sort then head 1 then first",
    "is_null then not then sum",
    "first then drop_nulls",
    "first then head 1",
    "first then sort",
    "first then unique",
];

fn links_of(words: &str) -> Vec<Value> {
    let one = |entry: &str| call(entry, json!({}));
    match words {
        "the column" => vec![],
        "sort" => vec![one("Expr.sort")],
        "reverse" => vec![one("Expr.reverse")],
        "unique" => vec![one("Expr.unique")],
        "drop_nulls" => vec![one("Expr.drop_nulls")],
        "is_null" => vec![one("Expr.is_null")],
        "head 1" => vec![call("Expr.head", json!({ "n": whole(1) }))],
        "shift 1" => vec![call("Expr.shift", json!({ "n": whole(1) }))],
        "cum_count" => vec![one("Expr.cum_count")],
        "implode" => vec![one("Expr.implode")],
        "sort then head 1 then first" => vec![
            one("Expr.sort"),
            call("Expr.head", json!({ "n": whole(1) })),
            one("Expr.first"),
        ],
        "is_null then not then sum" => vec![one("Expr.is_null"), one("Expr.not_"), one("Expr.sum")],
        "first then drop_nulls" => vec![one("Expr.first"), one("Expr.drop_nulls")],
        "first then head 1" => vec![
            one("Expr.first"),
            call("Expr.head", json!({ "n": whole(1) })),
        ],
        "first then sort" => vec![one("Expr.first"), one("Expr.sort")],
        "first then unique" => vec![one("Expr.first"), one("Expr.unique")],
        other => panic!("no links {other}"),
    }
}

fn listed_case(point: &Listed) -> Result<Case, String> {
    let column = point.column.as_ref().ok_or("no column")?;
    let value = of(column, links_of(point.links));
    Case::of_grouping(
        by(vec![field("n")]),
        vec![finishing(
            "LazyGroupBy.agg",
            json!({ "named_aggs": named(vec![("out", value)]) }),
        )],
        WORLD.frame.clone(),
        Connections::new(),
    )
}

fn listed_known() -> Vec<Known> {
    vec![
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("then", "reverse"),
            "Polars panics, on a thread of its own that no test can catch, on the reverse of an array whose element is no whole number or float, which the rule element-dtype-is-implemented refuses before Polars is asked",
        )
        .saying("rule element-dtype-is-implemented"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("then", "reverse"),
            "the tables do not measure the reverse of a list or an array of that element dtype, which Polars may panic on, so the rule element-dtype-is-measured leaves it out",
        )
        .saying("rule element-dtype-is-measured"),
    ]
}

#[test]
fn what_does_not_aggregate_in_agg_is_what_polars_says_and_gives_a_list() {
    let links: Vec<(String, &'static str)> = NOT_AGGREGATING
        .iter()
        .map(|words| ((*words).to_owned(), *words))
        .collect();
    let verdicts = Grid::<Listed>::new()
        .axis("class", classes(), |point, column| {
            point.column = Some(column.clone());
        })
        .axis("then", links, |point, links| point.links = links)
        .judge(&Question::any_refusal().at(Depth::Rows), listed_case);
    verdicts.assert_agree(&listed_known(), 300);
}

// ---------------------------------------------------------------------------
// Shortcuts

#[derive(Default)]
struct Shortcut {
    column: Option<Col>,
    entry: &'static str,
    arguments: Value,
    by_column: bool,
}

/// Every shortcut with each of its options: the entry and the arguments it is given.
fn shortcuts() -> Vec<(String, (&'static str, Value))> {
    let mut found: Vec<(String, (&'static str, Value))> = Vec::new();
    let mut add = |label: String, entry: &'static str, arguments: Value| {
        found.push((label, (entry, arguments)));
    };
    add("len".to_owned(), "LazyGroupBy.len", json!({}));
    add(
        "len named rows".to_owned(),
        "LazyGroupBy.len",
        json!({ "name": { "string": "rows" } }),
    );
    add(
        "len named n".to_owned(),
        "LazyGroupBy.len",
        json!({ "name": { "string": "n" } }),
    );
    for entry in [
        ("sum", "LazyGroupBy.sum"),
        ("mean", "LazyGroupBy.mean"),
        ("median", "LazyGroupBy.median"),
        ("min", "LazyGroupBy.min"),
        ("max", "LazyGroupBy.max"),
        ("n_unique", "LazyGroupBy.n_unique"),
        ("all", "LazyGroupBy.all"),
    ] {
        add(entry.0.to_owned(), entry.1, json!({}));
    }
    for (label, entry) in [("first", "LazyGroupBy.first"), ("last", "LazyGroupBy.last")] {
        add(label.to_owned(), entry, json!({}));
        add(
            format!("{label}, ignore_nulls"),
            entry,
            json!({ "ignore_nulls": flag(true) }),
        );
        add(
            format!("{label}, not ignore_nulls"),
            entry,
            json!({ "ignore_nulls": flag(false) }),
        );
    }
    for (share, method) in [
        (0.5, "nearest"),
        (0.0, "linear"),
        (1.0, "higher"),
        (0.25, "lower"),
        (0.75, "midpoint"),
        (0.5, "equiprobable"),
        (1.5, "nearest"),
        (-0.5, "linear"),
    ] {
        add(
            format!("quantile {share} {method}"),
            "LazyGroupBy.quantile",
            json!({ "quantile": { "float": share }, "interpolation": { "choice": method } }),
        );
    }
    for entry in [("head", "LazyGroupBy.head"), ("tail", "LazyGroupBy.tail")] {
        add(entry.0.to_owned(), entry.1, json!({}));
        for rows in [0_i64, 1, 5] {
            add(
                format!("{} {rows}", entry.0),
                entry.1,
                json!({ "n": { "integer": rows } }),
            );
        }
    }
    found
}

fn shortcut_case(point: &Shortcut) -> Result<Case, String> {
    let column = point.column.as_ref().ok_or("no column")?;
    let key = if point.by_column {
        field(&column.name)
    } else {
        field("n")
    };
    Case::of_grouping(
        by(vec![key]),
        vec![finishing(point.entry, point.arguments.clone())],
        of_one(column),
        Connections::new(),
    )
}

fn shortcuts_known() -> Vec<Known> {
    ["quantile 1.5 nearest", "quantile -0.5 linear"]
        .into_iter()
        .map(|shortcut| {
            Known::new(
                Kind::PolarsRefusesOnly,
                Pattern::new().is("shortcut", shortcut),
                "Polars refuses a share outside 0 to 1 as it builds the step, and the walk reads no number of a share, so only the run says so",
            )
            .saying("should be between")
        })
        .collect()
}

#[test]
fn every_shortcut_over_every_class_is_what_polars_says() {
    let verdicts = Grid::<Shortcut>::new()
        .axis("shortcut", shortcuts(), |point, (entry, arguments)| {
            point.entry = entry;
            point.arguments = arguments.clone();
        })
        .axis("class", classes(), |point, column| {
            point.column = Some(column.clone());
        })
        .axis(
            "grouped by",
            labelled(&[("n", false), ("the column", true)]),
            |point, by_column| point.by_column = *by_column,
        )
        .judge(&Question::any_refusal().at(Depth::Rows), shortcut_case);
    verdicts.assert_agree(&shortcuts_known(), 1_000);
}

// ---------------------------------------------------------------------------
// Predicates

#[derive(Default)]
struct Predicated {
    column: Option<Col>,
    form: &'static str,
    predicates: usize,
}

const PREDICATES: [&str; 17] = [
    "the values",
    "all null",
    "first",
    "first is null",
    "head 1",
    "sum",
    "n_unique",
    "any",
    "len above 1",
    "literal true",
    "literal 1",
    "literal text",
    "sorted",
    "first then drop_nulls",
    "first then head 1",
    "first then sort",
    "literal null then drop_nulls",
];

fn predicate_of(column: &Col, form: &str) -> Value {
    let one = |entry: &str| call(entry, json!({}));
    match form {
        "the values" => of(column, vec![]),
        "all null" => of(column, vec![one("Expr.is_null"), one("Expr.all")]),
        "first" => of(column, vec![one("Expr.first")]),
        "first is null" => of(column, vec![one("Expr.first"), one("Expr.is_null")]),
        "head 1" => of(column, vec![call("Expr.head", json!({ "n": whole(1) }))]),
        "sum" => of(column, vec![one("Expr.sum")]),
        "n_unique" => of(column, vec![one("Expr.n_unique")]),
        "any" => of(column, vec![one("Expr.any")]),
        "len above 1" => of(
            column,
            vec![
                one("Expr.len"),
                call("Expr.gt", json!({ "other": whole(1) })),
            ],
        ),
        "literal true" => lit(json!({ "flag": true })),
        "literal 1" => lit(json!({ "integer": 1 })),
        "literal text" => lit(json!({ "string": "a" })),
        "sorted" => of(column, vec![one("Expr.sort"), one("Expr.is_null")]),
        // What takes some of the values of one value gives a list for each group, which a having
        // cannot cast; what keeps the one value whole stays one value.
        "first then drop_nulls" => of(column, vec![one("Expr.first"), one("Expr.drop_nulls")]),
        "first then head 1" => of(
            column,
            vec![
                one("Expr.first"),
                call("Expr.head", json!({ "n": whole(1) })),
            ],
        ),
        "first then sort" => of(column, vec![one("Expr.first"), one("Expr.sort")]),
        "literal null then drop_nulls" => crate::support::chain(
            call("polars.lit", json!({ "value": { "flag": null } })),
            vec![one("Expr.drop_nulls")],
        ),
        other => panic!("no predicate {other}"),
    }
}

fn predicate_case(point: &Predicated) -> Result<Case, String> {
    let column = point.column.as_ref().ok_or("no column")?;
    let predicate = predicate_of(column, point.form);
    let having = finishing(
        "LazyGroupBy.having",
        json!({ "predicates": many(vec![predicate; point.predicates]) }),
    );
    Case::of_grouping(
        by(vec![field("n")]),
        vec![having, len()],
        WORLD.frame.clone(),
        Connections::new(),
    )
}

fn predicates_known() -> Vec<Known> {
    Vec::new()
}

#[test]
fn a_having_predicate_of_every_class_is_what_polars_says() {
    let forms: Vec<(String, &'static str)> = PREDICATES
        .iter()
        .map(|form| ((*form).to_owned(), *form))
        .collect();
    let verdicts = Grid::<Predicated>::new()
        .axis("class", classes(), |point, column| {
            point.column = Some(column.clone());
        })
        .axis("predicate", forms, |point, form| point.form = form)
        .axis(
            "predicates",
            labelled(&[("none", 0_usize), ("one", 1), ("two", 2)]),
            |point, count| point.predicates = *count,
        )
        .judge(&Question::any_refusal().at(Depth::Rows), predicate_case);
    verdicts.assert_agree(&predicates_known(), 300);
}

// ---------------------------------------------------------------------------
// Chains that are not well formed

#[derive(Default)]
struct Chained {
    then: &'static str,
}

fn calls_of(words: &str) -> Vec<Value> {
    let having = || {
        finishing(
            "LazyGroupBy.having",
            json!({ "predicates": many(vec![lit(json!({ "flag": true }))]) }),
        )
    };
    match words {
        "len" => vec![len()],
        "having, len" => vec![having(), len()],
        "no call" => vec![],
        "having alone" => vec![having()],
        "len, len" => vec![len(), len()],
        "len, sum" => vec![len(), finishing("LazyGroupBy.sum", json!({}))],
        "len, having" => vec![len(), having()],
        "having, having, len" => vec![having(), having(), len()],
        "agg, tail" => vec![
            finishing("LazyGroupBy.agg", json!({})),
            finishing("LazyGroupBy.tail", json!({})),
        ],
        "an expression method" => vec![finishing("Expr.sum", json!({}))],
        other => panic!("no chain {other}"),
    }
}

const CHAINS: [&str; 10] = [
    "len",
    "having, len",
    "no call",
    "having alone",
    "len, len",
    "len, sum",
    "len, having",
    "having, having, len",
    "agg, tail",
    "an expression method",
];

#[test]
fn a_chain_that_does_not_end_in_one_finishing_call_is_refused_before_polars_is_asked() {
    let chains: Vec<(String, &'static str)> = CHAINS
        .iter()
        .map(|words| ((*words).to_owned(), *words))
        .collect();
    let verdicts = Grid::<Chained>::new()
        .axis("calls", chains, |point, then| point.then = then)
        .judge(&Question::any_refusal(), |point| {
            Case::of_grouping(
                by(vec![field("n")]),
                calls_of(point.then),
                WORLD.frame.clone(),
                Connections::new(),
            )
        });
    // A cell that the document refuses is no cell Polars can be asked, and it says why.
    let mut known = Vec::new();
    for words in [
        "no call",
        "having alone",
        "len, len",
        "len, sum",
        "len, having",
        "having, having, len",
        "agg, tail",
    ] {
        known.push(
            Known::new(
                Kind::Unbuildable,
                Pattern::new().is("calls", words),
                "the document refuses a chain that does not end in exactly one finishing call, with the reason, before Polars is asked",
            )
            .saying("not a document"),
        );
    }
    known.push(
        Known::new(
            Kind::Unbuildable,
            Pattern::new().is("calls", "an expression method"),
            "a method of an expression is no call of a group by, which the document refuses where it stands",
        )
        .saying("cannot be"),
    );
    verdicts.assert_agree(&known, 2);
}

// ---------------------------------------------------------------------------
// What a group gives

#[test]
fn a_column_that_is_not_aggregated_is_a_list_of_its_dtype_for_each_group_and_an_aggregate_is_not() {
    use dageist_engine::document::{Document, Inputs, Outcome, Resolution, Segment};
    let mut checked = 0;
    for column in &WORLD.columns {
        let dtype = WORLD
            .frame
            .column(&column.name)
            .expect("the column")
            .dtype()
            .clone();
        let first = of(column, vec![call("Expr.first", json!({}))]);
        let text = crate::support::document(vec![crate::support::grouping(
            "g",
            by(vec![field("n")]),
            vec![finishing(
                "LazyGroupBy.agg",
                json!({ "named_aggs": named(vec![("all", of(column, vec![])), ("one", first)]) }),
            )],
        )]);
        let document = Document::read(&text).expect("a document");
        let report = document
            .run_on(&Inputs::new(), &WORLD.frame)
            .expect("the table has a schema");
        let step = &report.steps()[0];
        let Outcome::Valid { schema, .. } = step.outcome() else {
            panic!("{}: {:?}", column.label, step.outcome())
        };
        // Polars' own words for the dtypes, written by hand: a list of what the column has for the
        // values of each group, and what the column has for its first.
        assert_eq!(
            schema.get("all"),
            Some(&DataType::List(Box::new(dtype.clone()))),
            "{}",
            column.label
        );
        assert_eq!(schema.get("one"), Some(&dtype), "{}", column.label);
        // The report says the same of the column alone and of its first, where they stand.
        let trace = |at: usize, links: usize| {
            let place = dageist_engine::document::Place::of_step(
                0,
                Some(dageist_engine::document::StepId::new("g").expect("an id")),
            )
            .then(Segment::Link(0))
            .then(Segment::Parameter("named_aggs".to_owned()))
            .then(Segment::Value(at));
            let expression = step
                .expressions()
                .iter()
                .find(|expression| expression.place() == &place)
                .unwrap_or_else(|| panic!("{}: no expression at {place}", column.label));
            match if links == 0 {
                expression.start()
            } else {
                &expression.links()[links - 1]
            } {
                Resolution::Dtype(dtype) => dtype.clone(),
                other => panic!("{}: {other:?}", column.label),
            }
        };
        assert_eq!(
            trace(0, 0),
            DataType::List(Box::new(dtype.clone())),
            "{}",
            column.label
        );
        assert_eq!(trace(1, 0), DataType::List(Box::new(dtype.clone())));
        assert_eq!(trace(1, 1), dtype, "{}", column.label);
        checked += 1;
    }
    assert!(checked > 40, "{checked} classes were checked");
}
