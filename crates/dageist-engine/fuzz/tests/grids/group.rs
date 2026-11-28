//! Grouping a table and aggregating each group, held to Polars over grids, both ways.
//!
//! Every column of the world of a class is grouped and aggregated, and every cell is run on its
//! rows:
//!
//! - a key of every class in every form: the column, named, and followed by every call of the
//!   catalogue, with `maintain_order` each way;
//! - every aggregate of the catalogue ([`axes::aggregates`]) over every class, given by position
//!   and by name;
//! - every call that does not aggregate ([`axes::not_aggregating`]) over every class, alone and
//!   before and after the first value, which gives a list for each group whose dtype the walk
//!   holds to Polars';
//! - every finishing call of a group by with every setting of its options ([`axes::settings`]),
//!   over every class, grouped by a column of whole numbers and by the column itself;
//! - a `having` predicate of every class: the column, its first value and a null and a literal of
//!   every class, followed by every call, given once and twice;
//! - no key or no aggregate, and chains that do not end in one finishing call.
//!
//! At normal scale the calls keep the first of each family, the classes every one.

use dageist_engine::allowed::Representative;
use dageist_engine::catalog::{Applies, Entry};
use dageist_engine_fuzz::axes;
use dageist_engine_fuzz::doc::{call, column, column_then, many, named, then};
use dageist_engine_fuzz::judge::{Case, Connections, Depth, Kind, Outcome, Question};
use dageist_engine_fuzz::literals::{self, Written};
use dageist_engine_fuzz::world::{Column, World};
use dageist_fuzz::grid::{Choices, Grid, Known, Pattern, Scale, Verdicts};
use serde_json::{Value, json};

/// One column of each class of the world.
fn classes() -> Choices<Column> {
    World::get().classes("class")
}

fn by(keys: Vec<Value>) -> Value {
    json!({ "by": many(keys) })
}

fn len() -> Value {
    call("LazyGroupBy.len", json!({}))
}

/// The call of `entry` after a value of `column`, each needed parameter given a value.
fn called(entry: &'static Entry, column: &Column) -> Result<Value, String> {
    axes::call_of(entry, Some(column))
}

fn question() -> Question {
    Question::any_refusal().at(Depth::Rows)
}

// ---------------------------------------------------------------------------
// The lines the grids share

const NESTED: [&str; 3] = ["List(", "Array(", "Struct("];

/// A number typed without a dtype, which the grid gives a method's other arguments, beside a list,
/// an array or a struct.
fn known_a_number_beside_what_nests(axis: &'static str) -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new().contains_one_of(axis, NESTED),
        "a number typed without a dtype that meets a list, an array or a struct, compared with it, \
         replacing in it or bounding it, is refused by Polars, which compares, casts and bounds \
         none of them by a number; the walk allows it (candidate to file)",
    )
    .saying_one_of(&[
        "could not evaluate comparison between",
        "to list type is not supported",
        "to FixedSizeList",
        "must specify one field in the struct",
    ])
}

/// What the tables measured in a select that a group by does not do, which the walk keeps.
fn known_measured_in_a_select() -> Known<Kind> {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new(),
        "in a select Polars works out a schema of another dtype than the rows give for these, which \
         the tables refuse; in a group by it does not, and the walk keeps the refusal the tables \
         record for the method",
    )
    .saying("the schema says")
}

/// What the tables do not measure on a list or an array of an element dtype.
fn known_elements_not_measured() -> Known<Kind> {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new().contains_one_of("class", NESTED),
        "the tables do not measure the method on a list or an array of that element dtype, which \
         Polars may panic on, so the rule element-dtype-is-measured leaves it out",
    )
    .saying("rule element-dtype-is-measured")
}

/// A text read as a number where the tables were measured with the text `1d`.
fn known_texts_measured_as_1d() -> Known<Kind> {
    Known::new(
        Kind::WalkRefusesOnly,
        Pattern::new(),
        "the tables were measured with the text 1d, which is no number, and Polars reads the text of \
         a number, or a null of text, where the walk keeps the refusal of the tables",
    )
    .saying_one_of(&[
        "tables of Polars at Rows: conversion from `str`",
        "tables of Polars at Rows: cannot unpack series of type `str`",
        "tables of Polars at Rows: casting from Utf",
    ])
}

/// A flag, text, or a null of no dtype met by a number typed without a dtype.
fn known_a_number_beside_flags_text_and_nulls() -> Known<Kind> {
    Known::new(
        Kind::PolarsRefusesOnly,
        Pattern::new(),
        "Polars refuses a number typed without a dtype met bit by bit with a flag or a null of no \
         dtype, or compared with text, which the walk leaves to the tables (candidate to file)",
    )
    .saying_one_of(&[
        "is not supported Hint: cast the Boolean",
        "`bitand` operation not supported for dtype `null`",
        "cannot compare string with numeric",
    ])
}

// ---------------------------------------------------------------------------
// Keys

/// A form of a key: a named shape, or the column followed by a call of the catalogue.
#[derive(Clone)]
enum KeyForm {
    Column,
    Named,
    NamedTwice,
    Literal,
    TheColumnAndN,
    TheColumnTwice,
    Called(&'static Entry),
    CalledAndN(&'static Entry),
}

fn key_forms(scale: Scale) -> Choices<KeyForm> {
    let calls =
        axes::entries("form", Applies::Expression)
            .one_of_each(scale, "family", |_, entry| entry.family());
    let mut axis = Choices::of(
        "form",
        [
            ("column", KeyForm::Column),
            ("named", KeyForm::Named),
            ("named, a name given twice", KeyForm::NamedTwice),
            ("literal", KeyForm::Literal),
            ("the column and n", KeyForm::TheColumnAndN),
            ("the column twice", KeyForm::TheColumnTwice),
        ],
    );
    for (label, entry) in calls.iter() {
        axis = axis.with(label, KeyForm::Called(entry)).with(
            format!("{label} and n, two keys"),
            KeyForm::CalledAndN(entry),
        );
    }
    axis
}

#[derive(Default)]
struct Key {
    column: Option<Column>,
    form: Option<KeyForm>,
    order: Option<bool>,
}

fn key_case(point: &Key) -> Result<Case, String> {
    let held = point.column.as_ref().ok_or("no column")?;
    let name = held.name.as_str();
    let mut arguments = match point.form.as_ref().ok_or("no form")? {
        KeyForm::Column => by(vec![column(name)]),
        KeyForm::Named => json!({ "named_by": named(vec![("key", column(name))]) }),
        KeyForm::NamedTwice => {
            json!({ "named_by": named(vec![("key", column(name)), ("key", column("n"))]) })
        }
        KeyForm::Literal => by(vec![Written::Integer(1).expression(None)]),
        KeyForm::TheColumnAndN => by(vec![column(name), column("n")]),
        KeyForm::TheColumnTwice => by(vec![column(name), column(name)]),
        KeyForm::Called(entry) => by(vec![column_then(name, vec![called(entry, held)?])]),
        KeyForm::CalledAndN(entry) => by(vec![
            column_then(name, vec![called(entry, held)?]),
            column("n"),
        ]),
    };
    if let Some(keep) = point.order {
        arguments["maintain_order"] = json!({ "flag": keep });
    }
    Case::of_grouping(
        arguments,
        vec![len()],
        World::get().frame().clone(),
        Connections::new(),
    )
}

fn maintain_order() -> Choices<Option<bool>> {
    Choices::of(
        "maintain_order",
        [
            ("default", None),
            ("true", Some(true)),
            ("false", Some(false)),
        ],
    )
}

fn keys_verdicts() -> Verdicts<Outcome> {
    let question = question();
    Grid::<Key>::new("group-keys")
        .axis(classes(), |point, column| {
            point.column = Some(column.clone())
        })
        .axis(key_forms(Scale::Normal), |point, form| {
            point.form = Some(form.clone())
        })
        .axis(maintain_order(), |point, order| point.order = *order)
        .judge(|point| question.judge(key_case(point)))
}

fn keys_known() -> Vec<Known<Kind>> {
    vec![
        known_a_number_beside_what_nests("class"),
        known_a_number_beside_flags_text_and_nulls(),
        Known::new(
            Kind::BuildRefusesOnly,
            Pattern::new().contains_one_of("form", ["Expr.over"]),
            "over needs one of partition_by and order_by, which the catalogue marks neither of as \
             needed and refuses when it builds the step; the walk allows it (candidate to file)",
        )
        .saying("At least one of `partition_by` and `order_by`"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().contains_one_of("form", ["and n, two keys"]),
            "the walk cannot know how many values a key that gives some of them gives, which the \
             data decide, and refuses it beside a key of the rows; Polars allows it by chance of \
             this table (the limit of a count the data decide)",
        )
        .saying("rule values-meet-in-count"),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().is("maintain_order", "true"),
            "the walk cannot know how many values a key that takes some of them gives, which the \
             data decide, so where the order is kept it refuses it by design, and Polars allows it \
             by chance of this table: its filter keeps one row, which it broadcasts, and its values \
             that differ are as many as the rows",
        )
        .saying("kept-order-needs-keys-of-the-rows"),
    ]
}

#[test]
fn a_key_of_every_class_in_every_form_is_what_polars_says() {
    keys_verdicts().assert_agree(&keys_known(), 3_000);
}

// ---------------------------------------------------------------------------
// Aggregates

#[derive(Default)]
struct Aggregated {
    column: Option<Column>,
    entry: Option<&'static Entry>,
    named: bool,
}

fn aggregate_case(point: &Aggregated) -> Result<Case, String> {
    let held = point.column.as_ref().ok_or("no column")?;
    let entry = point.entry.ok_or("no entry")?;
    let aggregate = column_then(&held.name, vec![called(entry, held)?]);
    let aggs = if point.named {
        json!({ "named_aggs": named(vec![("out", aggregate)]) })
    } else {
        json!({ "aggs": many(vec![aggregate]) })
    };
    Case::of_grouping(
        by(vec![column("n")]),
        vec![call("LazyGroupBy.agg", aggs)],
        World::get().frame().clone(),
        Connections::new(),
    )
}

fn aggregates_verdicts() -> Verdicts<Outcome> {
    let question = question().with_dtypes();
    Grid::<Aggregated>::new("group-aggregates")
        .axis(axes::aggregates("method"), |point, entry| {
            point.entry = Some(entry)
        })
        .axis(classes(), |point, column| {
            point.column = Some(column.clone())
        })
        .axis(
            Choices::of("given", [("by position", false), ("by name", true)]),
            |point, named| point.named = *named,
        )
        .judge(|point| question.judge(aggregate_case(point)))
}

#[test]
fn every_aggregate_over_every_class_is_what_polars_says_in_agg() {
    let known = vec![
        known_elements_not_measured(),
        known_texts_measured_as_1d(),
        known_measured_in_a_select(),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("method", "Expr.gather"),
            "an index the grid gives lies beyond a group of one row, which the data decide",
        )
        .saying("out of bounds"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("method", "Expr.gather"),
            "Polars takes only whole numbers as the indices of gather and refuses a number with a \
             fraction, which the walk allows (candidate to file)",
        )
        .saying("expected: integer type"),
    ];
    aggregates_verdicts().assert_agree(&known, 1_000);
}

// ---------------------------------------------------------------------------
// What does not aggregate

/// What follows the column: nothing, a call that does not aggregate, and the first value before
/// or after one.
#[derive(Clone)]
enum Listed {
    TheColumn,
    Alone(&'static Entry),
    FirstThen(&'static Entry),
    ThenFirst(&'static Entry),
}

fn listed_forms(scale: Scale) -> Choices<Listed> {
    let calls = axes::not_aggregating("then", Representative::Int64).one_of_each(
        scale,
        "family",
        |_, entry| entry.family(),
    );
    let mut axis = Choices::of("then", [("the column", Listed::TheColumn)]);
    for (label, entry) in calls.iter() {
        axis = axis
            .with(label, Listed::Alone(entry))
            .with(format!("Expr.first, {label}"), Listed::FirstThen(entry))
            .with(format!("{label}, Expr.first"), Listed::ThenFirst(entry));
    }
    axis
}

#[derive(Default)]
struct Listing {
    column: Option<Column>,
    then: Option<Listed>,
}

fn listed_case(point: &Listing) -> Result<Case, String> {
    let held = point.column.as_ref().ok_or("no column")?;
    let first = || call("Expr.first", json!({}));
    let links = match point.then.as_ref().ok_or("no links")? {
        Listed::TheColumn => Vec::new(),
        Listed::Alone(entry) => vec![called(entry, held)?],
        Listed::FirstThen(entry) => vec![first(), called(entry, held)?],
        Listed::ThenFirst(entry) => vec![called(entry, held)?, first()],
    };
    Case::of_grouping(
        by(vec![column("n")]),
        vec![call(
            "LazyGroupBy.agg",
            json!({ "named_aggs": named(vec![("out", column_then(&held.name, links))]) }),
        )],
        World::get().frame().clone(),
        Connections::new(),
    )
}

fn listed_verdicts() -> Verdicts<Outcome> {
    let question = question().with_dtypes();
    Grid::<Listing>::new("group-lists")
        .axis(classes(), |point, column| {
            point.column = Some(column.clone())
        })
        .axis(listed_forms(Scale::Normal), |point, then| {
            point.then = Some(then.clone())
        })
        .judge(|point| question.judge(listed_case(point)))
}

#[test]
fn what_does_not_aggregate_in_agg_is_what_polars_says_and_gives_a_list_of_its_dtype() {
    let known = vec![
        known_elements_not_measured(),
        known_measured_in_a_select(),
        known_a_number_beside_what_nests("class"),
        Known::new(
            Kind::DtypeDiffers,
            Pattern::new().has_one_of("then", ["Expr.arg_unique"]),
            "the walk takes the places of the first of each value as one value where it follows one, \
             which Polars gives as a list of them for each group (candidate to file)",
        )
        .saying("Polars gives List("),
    ];
    listed_verdicts().assert_agree(&known, 300);
}

// ---------------------------------------------------------------------------
// The finishing calls of a group by

#[derive(Default)]
struct Shortcut {
    column: Option<Column>,
    entry: Option<&'static Entry>,
    arguments: Value,
    by_column: bool,
}

/// Every call that finishes a group by but `agg` and `having`, which the other grids cross, with
/// every setting of its options.
fn shortcuts(scale: Scale) -> Choices<(&'static Entry, Value)> {
    let mut axis = Choices::new("shortcut");
    for (label, entry) in axes::entries("shortcut", Applies::Grouped).iter() {
        if matches!(label, "LazyGroupBy.agg" | "LazyGroupBy.having") {
            axis = axis.leave_out(label, "the grids of aggregates and of predicates cross it");
            continue;
        }
        match axes::settings(entry, None, scale) {
            Ok(settings) => {
                for (setting, arguments) in settings {
                    axis = axis.with(format!("{label} {setting}"), (*entry, arguments));
                }
            }
            Err(why) => axis = axis.leave_out(label, why),
        }
    }
    axis
}

fn shortcut_case(point: &Shortcut) -> Result<Case, String> {
    let held = point.column.as_ref().ok_or("no column")?;
    let key = if point.by_column {
        column(&held.name)
    } else {
        column("n")
    };
    Case::of_grouping(
        by(vec![key]),
        vec![call(
            point.entry.ok_or("no entry")?.id().as_str(),
            point.arguments.clone(),
        )],
        World::get().of(&[&held.label]),
        Connections::new(),
    )
}

fn shortcuts_verdicts() -> Verdicts<Outcome> {
    let question = question();
    Grid::<Shortcut>::new("group-shortcuts")
        .axis(shortcuts(Scale::Normal), |point, (entry, arguments)| {
            point.entry = Some(entry);
            point.arguments = arguments.clone();
        })
        .axis(classes(), |point, column| {
            point.column = Some(column.clone())
        })
        .axis(
            Choices::of("grouped by", [("n", false), ("the column", true)]),
            |point, by_column| point.by_column = *by_column,
        )
        .judge(|point| question.judge(shortcut_case(point)))
}

fn shortcuts_known() -> Vec<Known<Kind>> {
    vec![
        Known::new(
            Kind::BuildRefusesOnly,
            Pattern::new(),
            "the catalogue holds a whole number typed in to the range the parameter of Python's \
             binding takes, a count of rows below nothing, and refuses one outside it when it builds \
             the step, as Python does; the walk reads no value typed in (candidate to file)",
        )
        .saying("takes a whole number from"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().starts("shortcut", "LazyGroupBy.quantile"),
            "Polars refuses a share outside 0 to 1 as it builds the step, and the walk reads no \
             number of a share, so only the run says so (the values no table holds are tracked by \
             #224)",
        )
        .saying("should be between")
        .shared(),
    ]
}

#[test]
fn every_finishing_call_with_every_option_over_every_class_is_what_polars_says() {
    shortcuts_verdicts().assert_agree(&shortcuts_known(), 1_000);
}

// ---------------------------------------------------------------------------
// Predicates of `having`

/// A predicate of a group: what it starts from, and the call that follows.
#[derive(Clone)]
enum Start {
    TheColumn,
    TheFirst,
    Null(Representative),
    Literal(Representative),
}

#[derive(Clone)]
struct Predicate {
    start: Start,
    then: Option<&'static Entry>,
}

fn predicates(scale: Scale) -> Choices<Predicate> {
    let calls: Vec<(String, &'static Entry)> = axes::entries("predicate", Applies::Expression)
        .one_of_each(scale, "family", |_, entry| entry.family())
        .iter()
        .map(|(label, entry)| (label.to_owned(), *entry))
        .collect();
    let mut starts = vec![
        ("the values".to_owned(), Start::TheColumn),
        ("the first".to_owned(), Start::TheFirst),
    ];
    let classes = Choices::of(
        "class",
        Representative::MEASURED
            .iter()
            .map(|class| (class.name(), *class)),
    )
    .one_of_each(scale, "kind of class", |_, class| {
        axes::kind_of_class(*class)
    });
    for (label, class) in classes.iter() {
        if literals::null_of(*class).is_some() {
            starts.push((format!("a null of {label}"), Start::Null(*class)));
        }
        if literals::one_of(*class).is_some() {
            starts.push((format!("a literal of {label}"), Start::Literal(*class)));
        }
    }
    let mut axis = Choices::new("predicate");
    for (start_label, start) in starts {
        axis = axis.with(
            start_label.clone(),
            Predicate {
                start: start.clone(),
                then: None,
            },
        );
        for (label, entry) in &calls {
            axis = axis.with(
                format!("{start_label}, {label}"),
                Predicate {
                    start: start.clone(),
                    then: Some(entry),
                },
            );
        }
    }
    axis
}

#[derive(Default)]
struct Predicated {
    column: Option<Column>,
    predicate: Option<Predicate>,
    count: usize,
}

fn predicate_value(held: &Column, predicate: &Predicate) -> Result<Value, String> {
    let start = match &predicate.start {
        Start::TheColumn => column(&held.name),
        Start::TheFirst => column_then(&held.name, vec![call("Expr.first", json!({}))]),
        Start::Null(class) => literals::null_of(*class).ok_or("no null")?.expression(),
        Start::Literal(class) => literals::one_of(*class).ok_or("no literal")?.expression(),
    };
    Ok(match predicate.then {
        Some(entry) => then(start, [called(entry, held)?]),
        None => start,
    })
}

fn predicate_case(point: &Predicated) -> Result<Case, String> {
    let held = point.column.as_ref().ok_or("no column")?;
    let value = predicate_value(held, point.predicate.as_ref().ok_or("no predicate")?)?;
    Case::of_grouping(
        by(vec![column("n")]),
        vec![
            call(
                "LazyGroupBy.having",
                json!({ "predicates": many(vec![value; point.count]) }),
            ),
            len(),
        ],
        World::get().frame().clone(),
        Connections::new(),
    )
}

fn predicates_verdicts() -> Verdicts<Outcome> {
    let question = question();
    Grid::<Predicated>::new("group-predicates")
        .axis(classes(), |point, column| {
            point.column = Some(column.clone())
        })
        .axis(predicates(Scale::Normal), |point, predicate| {
            point.predicate = Some(predicate.clone());
        })
        .axis(
            Choices::of("predicates", [("one", 1_usize), ("two", 2)]),
            |point, count| point.count = *count,
        )
        .judge(|point| question.judge(predicate_case(point)))
}

#[test]
fn a_having_predicate_of_every_class_is_what_polars_says() {
    let known = vec![
        known_measured_in_a_select(),
        known_texts_measured_as_1d(),
        known_a_number_beside_what_nests("class"),
        known_a_number_beside_flags_text_and_nulls(),
        Known::new(
            Kind::WalkRefusesOnly,
            Pattern::new().starts("predicate", "a null of"),
            "a null kept through a fill is a null, which Polars casts to a Boolean whatever its \
             dtype; the rule having-casts-to-boolean takes a null through the calls that keep one \
             value and not through a fill (candidate to file)",
        )
        .saying("rule having-casts-to-boolean"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("predicates", "two"),
            "two predicates of a having are met as flags, which Polars refuses for a value that is \
             no flag, where it casts one predicate alone; the walk casts each as it casts one \
             (candidate to file)",
        )
        .saying("filter predicate must be of type `Boolean`"),
        Known::new(
            Kind::PolarsRefusesOnly,
            Pattern::new().is("predicate", "the values, Expr.eq"),
            "the values of a group of dates, moments, lengths of time or decimals compared with a \
             number are a list of flags for each group, which Polars cannot cast to a flag, and the \
             walk allows it (candidate to file)",
        )
        .saying("cannot cast List type"),
    ];
    predicates_verdicts().assert_agree(&known, 3_000);
}

// ---------------------------------------------------------------------------
// No key, no aggregate, and chains that are not well formed

#[derive(Default)]
struct Bare {
    keys: Vec<Value>,
    finish: Value,
}

#[test]
fn a_group_by_with_no_key_or_no_aggregate_is_what_polars_says() {
    let mut finishes = Choices::new("finish")
        .with("an empty agg", call("LazyGroupBy.agg", json!({})))
        .with(
            "an agg of a count",
            call(
                "LazyGroupBy.agg",
                json!({ "aggs": many(vec![column_then("n", vec![call("Expr.len", json!({}))])]) }),
            ),
        );
    for (label, entry) in axes::entries("finish", Applies::Grouped).iter() {
        if matches!(label, "LazyGroupBy.agg" | "LazyGroupBy.having") {
            continue;
        }
        if let Ok(called) = axes::call_of(entry, None) {
            finishes = finishes.with(label, called);
        }
    }
    let question = question();
    Grid::<Bare>::new("group-bare")
        .axis(
            Choices::of(
                "keys",
                [
                    ("no key", Vec::new()),
                    ("a key", vec![column("k")]),
                    ("two keys", vec![column("k"), column("n")]),
                ],
            ),
            |point, keys| point.keys.clone_from(keys),
        )
        .axis(finishes, |point, finish| point.finish = finish.clone())
        .judge(|point| {
            let keys = if point.keys.is_empty() {
                json!({})
            } else {
                by(point.keys.clone())
            };
            question.judge(Case::of_grouping(
                keys,
                vec![point.finish.clone()],
                World::get().of(&[]),
                Connections::new(),
            ))
        })
        .assert_agree(&[], 30);
}

#[test]
fn a_chain_that_does_not_end_in_one_finishing_call_is_refused_before_polars_is_asked() {
    let having = || {
        call(
            "LazyGroupBy.having",
            json!({ "predicates": many(vec![Written::Flag(true).expression(None)]) }),
        )
    };
    let chains = Choices::of(
        "calls",
        [
            ("len", vec![len()]),
            ("having, len", vec![having(), len()]),
            ("no call", Vec::new()),
            ("having alone", vec![having()]),
            ("len, len", vec![len(), len()]),
            ("len, sum", vec![len(), call("LazyGroupBy.sum", json!({}))]),
            ("len, having", vec![len(), having()]),
            ("having, having, len", vec![having(), having(), len()]),
            (
                "agg, tail",
                vec![
                    call("LazyGroupBy.agg", json!({})),
                    call("LazyGroupBy.tail", json!({})),
                ],
            ),
            ("an expression method", vec![call("Expr.sum", json!({}))]),
        ],
    );
    let question = Question::any_refusal();
    let verdicts = Grid::<Option<Vec<Value>>>::new("group-chains")
        .axis(chains, |point, calls| *point = Some(calls.clone()))
        .judge(|point| {
            question.judge(Case::of_grouping(
                by(vec![column("n")]),
                point.clone().unwrap_or_default(),
                World::get().frame().clone(),
                Connections::new(),
            ))
        });
    // A cell that the document refuses is no cell Polars can be asked, and it says why.
    let known = vec![
        Known::new(
            Kind::Unbuildable,
            Pattern::new().one_of(
                "calls",
                [
                    "no call",
                    "having alone",
                    "len, len",
                    "len, sum",
                    "len, having",
                    "having, having, len",
                    "agg, tail",
                ],
            ),
            "the document refuses a chain that does not end in exactly one finishing call, with \
             the reason, before Polars is asked",
        )
        .saying("not a document"),
        Known::new(
            Kind::Unbuildable,
            Pattern::new().is("calls", "an expression method"),
            "a method of an expression is no call of a group by, which the document refuses where \
             it stands",
        )
        .saying("cannot be"),
    ];
    verdicts.assert_agree(&known, 2);
}
