//! The census of every variant the step editor has to handle, written for the view's tests.
//!
//! The view draws what the host sends and decides nothing about it, so each thing the engine can
//! send is a variant the view needs a plan for: a kind of start, of parameter, of option, a class of
//! dtype, a cause of a refusal, a state of a step and of its rows, a kind of input and of value the
//! rows show. This module lists them all, from the Rust types, for the view's test to hold against
//! what it can draw.
//!
//! Each list comes from the type that owns it, and a variant added to that type must reach the list
//! without a hand edit:
//!
//! - A type that has a list of every variant of its own, such as `Rule::ALL`, is read through it.
//! - Any other is walked from its first variant by a function that names each variant, with no
//!   wildcard arm, `chain`: a variant added to the type does not compile until that function gives
//!   it a place, and the test of each chain checks that the walk reached every variant its
//!   function names.
//!
//! A word in a list is the word the host serializes for that variant, read from the serialized
//! sample and not written again here, so that the view is given the words it receives.

use std::collections::BTreeSet;

use dageist_engine::allowed::{After, ConditionState, Literal, Representative, Rule, Shape, Stage};
use dageist_engine::catalog::{self, Build, Kind, ValueType};
use dageist_engine::document::{Call, Expression, Place, Start, Value as Held};
use serde::Serialize;
use serde_json::{Value, json};

use super::{
    CauseView, CountView, OutcomeView, PlaceView, RefusalSourceView, RefusalView, StageView,
    ValuesView, condition_state_view, place_view, shape_view, stage_view,
};
use crate::node::{InputView, OriginView};
use crate::offers::{TypedRequest, ValueTypeRequest, read_view};

/// The variants of a type reached from `first` by `next`, which ends the walk with `None`.
///
/// `next` names every variant of the type with no wildcard arm, so a variant added to the type does
/// not compile until `next` places it. A variant placed in no walk from `first` is not reached, and
/// the test of the chain, which compares it with the words of a second list, fails.
fn chain<T>(first: T, next: impl Fn(&T) -> Option<T>) -> Vec<T> {
    /// More than any list here has, so that a walk that loops fails and does not hang.
    const MOST: usize = 128;
    let mut walked = vec![first];
    while let Some(following) = walked.last().and_then(&next) {
        assert!(walked.len() < MOST, "the chain does not end");
        walked.push(following);
    }
    walked
}

/// What a view's serialization says under `key`: the word the view receives for the variant.
fn word(view: &impl Serialize, key: &str) -> String {
    match &serde_json::to_value(view).expect("a view serializes")[key] {
        Value::String(word) => word.clone(),
        other => panic!("the view has no word under {key}: {other}"),
    }
}

/// The words of `views`, each under `key`.
fn words(views: &[impl Serialize], key: &str) -> Vec<String> {
    views.iter().map(|view| word(view, key)).collect()
}

/// What a view that serializes as one word, such as a unit variant, is written as.
fn plain(view: &impl Serialize) -> String {
    match serde_json::to_value(view).expect("a view serializes") {
        Value::String(word) => word,
        other => panic!("the view is not one word: {other}"),
    }
}

/// The words of `views`, each a unit variant.
fn plains(views: &[impl Serialize]) -> Vec<String> {
    views.iter().map(plain).collect()
}

/// A refusal made of `source`, which has some words.
fn refusal(source: RefusalSourceView) -> RefusalView {
    RefusalView {
        source,
        reason: "why".to_owned(),
    }
}

// What a refusal came from.

const fn next_source(source: &RefusalSourceView) -> Option<RefusalSourceView> {
    match source {
        RefusalSourceView::Polars { .. } => Some(RefusalSourceView::Rule {
            rule: Rule::ALL[0].name(),
        }),
        RefusalSourceView::Rule { .. } => Some(RefusalSourceView::Structure),
        RefusalSourceView::Structure => None,
    }
}

fn sources() -> Vec<RefusalSourceView> {
    chain(
        RefusalSourceView::Polars {
            stage: StageView::Schema,
        },
        next_source,
    )
}

// Why a step was refused.

fn next_cause(cause: &CauseView) -> Option<CauseView> {
    Some(match cause {
        CauseView::UnknownTable { .. } => CauseView::UnknownInput {
            name: String::new(),
        },
        CauseView::UnknownInput { .. } => CauseView::Build {
            reason: String::new(),
        },
        CauseView::Build { .. } => CauseView::NotAllowed {
            refusal: refusal(RefusalSourceView::Structure),
        },
        CauseView::NotAllowed { .. } => CauseView::Polars {
            reason: String::new(),
        },
        CauseView::Polars { .. } => return None,
    })
}

fn causes() -> Vec<CauseView> {
    chain(
        CauseView::UnknownTable {
            name: String::new(),
        },
        next_cause,
    )
}

// What running a step on the rows came to.

fn next_count(count: &CountView) -> Option<CountView> {
    Some(match count {
        CountView::NotCounted => CountView::Counted {
            received: 0,
            kept: 0,
            first: Vec::new(),
        },
        CountView::Counted { .. } => CountView::Failed {
            reason: String::new(),
        },
        CountView::Failed { .. } => return None,
    })
}

fn counts() -> Vec<CountView> {
    chain(CountView::NotCounted, next_count)
}

// What running a step came to.

fn here() -> PlaceView {
    place_view(&Place::of_step(0, None))
}

fn next_outcome(outcome: &OutcomeView) -> Option<OutcomeView> {
    Some(match outcome {
        OutcomeView::Valid { .. } => OutcomeView::Refused {
            place: here(),
            cause: CauseView::Polars {
                reason: String::new(),
            },
        },
        OutcomeView::Refused { .. } => OutcomeView::Incomplete { place: here() },
        OutcomeView::Incomplete { .. } => OutcomeView::NotRun {
            blocked_by: String::new(),
        },
        OutcomeView::NotRun { .. } => return None,
    })
}

fn outcomes() -> Vec<OutcomeView> {
    chain(
        OutcomeView::Valid {
            columns: Vec::new(),
            changes: Vec::new(),
        },
        next_outcome,
    )
}

/// What the steps before a step reported, as far as it decides what the step reports: the step
/// before it ran well, or it was refused, was incomplete, failed on the rows, or was itself not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Before {
    /// There is no step before, or it was accepted and its rows were counted or not read.
    Nothing,
    /// It was refused, so what follows is not run.
    Refused,
    /// It holds something still to be picked, so what follows is not run.
    Incomplete,
    /// Polars accepted it and could not run it on the rows, so what follows is not counted.
    Failed,
    /// It was not run, because a step before it was refused or incomplete.
    NotRun,
}

const fn next_before(before: &Before) -> Option<Before> {
    match before {
        Before::Nothing => Some(Before::Refused),
        Before::Refused => Some(Before::Incomplete),
        Before::Incomplete => Some(Before::Failed),
        Before::Failed => Some(Before::NotRun),
        Before::NotRun => None,
    }
}

const fn before_word(before: Before) -> &'static str {
    match before {
        Before::Nothing => "nothing",
        Before::Refused => "refused",
        Before::Incomplete => "incomplete",
        Before::Failed => "failed",
        Before::NotRun => "notRun",
    }
}

/// What a step with `outcome` can follow: a step is not run after one that was refused or was
/// incomplete, so it is not run then and only then, and any other can follow one that ran or
/// failed on its rows.
const fn follows(outcome: &OutcomeView, before: Before) -> bool {
    match outcome {
        OutcomeView::Valid { .. }
        | OutcomeView::Refused { .. }
        | OutcomeView::Incomplete { .. } => {
            matches!(before, Before::Nothing | Before::Failed)
        }
        OutcomeView::NotRun { .. } => {
            matches!(
                before,
                Before::Refused | Before::Incomplete | Before::NotRun
            )
        }
    }
}

/// Whether a step with `outcome` after `before` can be reported as `count`: only a step Polars
/// accepts, after steps that ran or were not read, is run on the rows, and counts them or fails on
/// them; every other step is not counted.
const fn counts_as(outcome: &OutcomeView, before: Before, count: &CountView) -> bool {
    match count {
        CountView::NotCounted => true,
        CountView::Counted { .. } | CountView::Failed { .. } => {
            matches!(outcome, OutcomeView::Valid { .. }) && matches!(before, Before::Nothing)
        }
    }
}

/// Every state a step can be reported in, as its outcome, the cause of its refusal if it was
/// refused, how its rows came out and what came before it.
fn states() -> Vec<Value> {
    let mut found = Vec::new();
    for outcome in outcomes() {
        // A refusal has one state for each cause: its words are the cause's.
        let held: Vec<(Option<String>, OutcomeView)> = match outcome {
            OutcomeView::Refused { place, .. } => causes()
                .into_iter()
                .map(|cause| {
                    (
                        Some(word(&cause, "kind")),
                        OutcomeView::Refused {
                            place: place.clone(),
                            cause,
                        },
                    )
                })
                .collect(),
            other @ (OutcomeView::Valid { .. }
            | OutcomeView::Incomplete { .. }
            | OutcomeView::NotRun { .. }) => vec![(None, other)],
        };
        for (cause, outcome) in held {
            for before in chain(Before::Nothing, next_before) {
                if !follows(&outcome, before) {
                    continue;
                }
                for count in counts() {
                    if counts_as(&outcome, before, &count) {
                        found.push(json!({
                            "outcome": word(&outcome, "status"),
                            "cause": cause,
                            "rows": word(&count, "kind"),
                            "after": before_word(before),
                        }));
                    }
                }
            }
        }
    }
    found
}

// Where an expression starts.

fn next_input(input: &InputView) -> Option<InputView> {
    let from = || OriginView::Parameter;
    Some(match input {
        InputView::Table { .. } => InputView::Value {
            name: String::new(),
            from: from(),
            dtype: String::new(),
            value: None,
        },
        InputView::Value { .. } => InputView::List {
            name: String::new(),
            from: from(),
            dtype: String::new(),
            values: ValuesView {
                shown: Vec::new(),
                more: 0,
            },
        },
        InputView::List { .. } => return None,
    })
}

fn inputs() -> Vec<InputView> {
    chain(
        InputView::Table {
            name: String::new(),
            from: OriginView::Parameter,
            columns: Vec::new(),
        },
        next_input,
    )
}

/// What an expression that starts from the input is, in the editor's words: a table is no start,
/// because a step brings it in whole and no expression reads it.
const fn input_start(input: &InputView) -> Option<&'static str> {
    match input {
        InputView::Table { .. } => None,
        InputView::Value { .. } => Some("connectedValue"),
        InputView::List { .. } => Some("connectedList"),
    }
}

fn next_origin(origin: &OriginView) -> Option<OriginView> {
    Some(match origin {
        OriginView::Node { .. } => OriginView::Column {
            node: String::new(),
            column: String::new(),
        },
        OriginView::Column { .. } => OriginView::Parameter,
        OriginView::Parameter => return None,
    })
}

fn origins() -> Vec<OriginView> {
    chain(
        OriginView::Node {
            name: String::new(),
        },
        next_origin,
    )
}

/// Where a chain in a document starts, as a word of the host's own.
const fn start_word(start: &Start) -> &'static str {
    match start {
        Start::Call(_) => "call",
        Start::Input(_) => "input",
        Start::Hole => "hole",
    }
}

fn next_start(start: &Start) -> Option<Start> {
    Some(match start {
        Start::Call(_) => Start::Input(String::new()),
        Start::Input(_) => Start::Hole,
        Start::Hole => return None,
    })
}

fn starts_of_document() -> Vec<Start> {
    let field = Expression::field("a").expect("a field");
    chain(field.start().clone(), next_start)
}

/// Whether an entry built as `build` is where a chain starts: a function that makes an expression,
/// or `when`, which starts a condition.
const fn starts_a_chain(build: Build) -> bool {
    match build {
        Build::Function(_) | Build::Condition(_) => true,
        Build::Expression(_)
        | Build::Frame(_)
        | Build::ConditionStep(_)
        | Build::ConditionEnd(_) => false,
    }
}

// The literals a document holds, and the host reads.

/// What a value a document holds is, where it is a number, text or a flag typed in, in the words
/// of the editor, and `None` for every other kind of value.
const fn typed_word(value: &Held) -> Option<&'static str> {
    match value {
        Held::Integer(_) => Some("integer"),
        Held::Float(_) => Some("float"),
        Held::String(_) => Some("text"),
        Held::Flag(_) => Some("flag"),
        Held::Expression(_)
        | Held::Columns(_)
        | Held::Choice(_)
        | Held::Dtype(_)
        | Held::Duration(_)
        | Held::Pattern(_)
        | Held::TimeZone(_)
        | Held::Table(_)
        | Held::Path(_)
        | Held::Pairs(_)
        | Held::Many(_) => None,
    }
}

fn next_held(value: &Held) -> Option<Held> {
    Some(match value {
        Held::Expression(_) => Held::Columns(Vec::new()),
        Held::Columns(_) => Held::Integer(0),
        Held::Integer(_) => Held::Float(0.0),
        Held::Float(_) => Held::String(String::new()),
        Held::String(_) => Held::Flag(Some(true)),
        Held::Flag(_) => Held::Choice(String::new()),
        Held::Choice(_) => Held::Dtype("Int64".parse().expect("a dtype")),
        Held::Dtype(_) => Held::Duration(String::new()),
        Held::Duration(_) => Held::Pattern(String::new()),
        Held::Pattern(_) => Held::TimeZone(None),
        Held::TimeZone(_) => Held::Table(String::new()),
        Held::Table(_) => Held::Path(String::new()),
        Held::Path(_) => Held::Pairs(Vec::new()),
        Held::Pairs(_) => Held::Many(Vec::new()),
        Held::Many(_) => return None,
    })
}

fn held_values() -> Vec<Held> {
    let field = Expression::field("a").expect("a field");
    chain(Held::Expression(field), next_held)
}

const fn literal_word(literal: &Literal) -> &'static str {
    match literal {
        Literal::Integer(_) => "integer",
        Literal::Float(_) => "float",
        Literal::String(_) => "text",
    }
}

fn next_literal(literal: &Literal) -> Option<Literal> {
    Some(match literal {
        Literal::Integer(_) => Literal::Float(0.0),
        Literal::Float(_) => Literal::String(String::new()),
        Literal::String(_) => return None,
    })
}

fn literals() -> Vec<Literal> {
    chain(Literal::Integer(0), next_literal)
}

/// The class of dtype Polars gives a number, text or flag typed in, as a document holds it. A
/// number or text is a literal the host reads, which the engine asks Polars about; a flag is a
/// Boolean.
fn typed_class(value: &Held) -> Option<Representative> {
    match value {
        Held::Integer(whole) => Literal::Integer(*whole).class(),
        Held::Float(number) => Literal::Float(*number).class(),
        Held::String(text) => Literal::String(text.clone()).class(),
        Held::Flag(_) => Some(Representative::Boolean),
        Held::Expression(_)
        | Held::Columns(_)
        | Held::Choice(_)
        | Held::Dtype(_)
        | Held::Duration(_)
        | Held::Pattern(_)
        | Held::TimeZone(_)
        | Held::Table(_)
        | Held::Path(_)
        | Held::Pairs(_)
        | Held::Many(_) => None,
    }
}

/// The kinds of number, text or flag that `polars.lit` takes typed in a document, each with the
/// class of dtype Polars gives it and whether the host reads typed text as one.
fn typed_values() -> Vec<Value> {
    let lit = catalog::entry("polars.lit").expect("the catalogue has lit");
    let readable: BTreeSet<&str> = literals().iter().map(literal_word).collect();
    held_values()
        .into_iter()
        .filter_map(|value| {
            let literal = typed_word(&value)?;
            Call::new(lit, [("value", value.clone())]).ok()?;
            Some(json!({
                "literal": literal,
                "dtype": typed_class(&value).map(Representative::name),
                "readable": readable.contains(literal),
            }))
        })
        .collect()
}

// What typed text is read as.

const fn next_typed(typed: &TypedRequest) -> Option<TypedRequest> {
    use ValueTypeRequest as Of;
    Some(match typed {
        TypedRequest::Expression => TypedRequest::Value {
            value_type: Of::Integer,
        },
        TypedRequest::Value { value_type } => match value_type {
            Of::Integer => TypedRequest::Value {
                value_type: Of::Float,
            },
            Of::Float => TypedRequest::Value {
                value_type: Of::String,
            },
            Of::String => TypedRequest::Value {
                value_type: Of::Date,
            },
            Of::Date => TypedRequest::Value {
                value_type: Of::Any,
            },
            Of::Any => TypedRequest::Duration,
        },
        TypedRequest::Duration => TypedRequest::Pattern,
        TypedRequest::Pattern => TypedRequest::TimeZone,
        TypedRequest::TimeZone => TypedRequest::Path,
        TypedRequest::Path => return None,
    })
}

fn typed_slots() -> Vec<TypedRequest> {
    chain(TypedRequest::Expression, next_typed)
}

/// The slot a text is typed into, as the contract names it.
fn slot_json(typed: TypedRequest) -> Value {
    match typed {
        TypedRequest::Expression => json!({ "kind": "expression" }),
        TypedRequest::Value { value_type } => json!({
            "kind": "value",
            "type": match value_type {
                ValueTypeRequest::Integer => "integer",
                ValueTypeRequest::Float => "float",
                ValueTypeRequest::String => "string",
                ValueTypeRequest::Date => "date",
                ValueTypeRequest::Any => "any",
            },
        }),
        TypedRequest::Duration => json!({ "kind": "duration" }),
        TypedRequest::Pattern => json!({ "kind": "pattern" }),
        TypedRequest::TimeZone => json!({ "kind": "timeZone" }),
        TypedRequest::Path => json!({ "kind": "path" }),
    }
}

/// Texts typed into a slot, one for each way the slot can read, or fail to read, what is typed.
const fn texts(typed: TypedRequest) -> &'static [&'static str] {
    match typed {
        TypedRequest::Expression
        | TypedRequest::Value {
            value_type: ValueTypeRequest::Any,
        } => &["12", "1.5", "text"],
        TypedRequest::Value {
            value_type: ValueTypeRequest::Integer,
        } => &["12", "1.5"],
        TypedRequest::Value {
            value_type: ValueTypeRequest::Float,
        } => &["1.5"],
        TypedRequest::Value {
            value_type: ValueTypeRequest::String,
        } => &["text"],
        TypedRequest::Value {
            value_type: ValueTypeRequest::Date,
        } => &["2026-03-10", "not a date"],
        TypedRequest::Duration => &["1mo", "not a duration"],
        TypedRequest::Pattern => &["a+"],
        TypedRequest::TimeZone => &["UTC"],
        TypedRequest::Path => &["a/b.csv"],
    }
}

/// How each slot that takes typed text reads it: the kind of literal and the class of dtype of
/// each reading, or none for a text it does not read.
fn typed_readings() -> Vec<Value> {
    let mut found: Vec<Value> = Vec::new();
    for typed in typed_slots() {
        for text in texts(typed) {
            let readings = serde_json::to_value(read_view(text, typed, None, None).expect("read"))
                .expect("readings serialize");
            for reading in readings["readings"].as_array().expect("a list") {
                let one = if reading["kind"] == "read" {
                    json!({
                        "slot": slot_json(typed),
                        "literal": reading["literal"]["kind"],
                        "dtype": reading["dtype"],
                    })
                } else {
                    json!({ "slot": slot_json(typed), "literal": null, "dtype": null })
                };
                if !found.contains(&one) {
                    found.push(one);
                }
            }
        }
    }
    found
}

// What a parameter holds.

const fn kind_word(kind: Kind) -> &'static str {
    match kind {
        Kind::Expression(_) => "expression",
        Kind::Column => "column",
        Kind::Value(_) => "value",
        Kind::Flag => "flag",
        Kind::Choice(_) => "choice",
        Kind::Dtype => "dtype",
        Kind::Duration => "duration",
        Kind::Pattern => "pattern",
        Kind::TimeZone => "timeZone",
        Kind::Table => "table",
        Kind::Path => "path",
        Kind::Pairs(..) => "pairs",
    }
}

const fn value_type_word(value_type: ValueType) -> &'static str {
    match value_type {
        ValueType::Integer => "integer",
        ValueType::Float => "float",
        ValueType::String => "string",
        ValueType::Date => "date",
        ValueType::Any => "any",
    }
}

/// A kind as the catalogue's view writes it, with what makes one of its kind differ from another.
fn kind_json(kind: Kind) -> Value {
    let mut written = json!({ "kind": kind_word(kind) });
    match kind {
        Kind::Value(value_type) => written["type"] = json!(value_type_word(value_type)),
        Kind::Pairs(key, value) => {
            written["key"] = kind_json(*key);
            written["value"] = kind_json(*value);
        }
        Kind::Expression(_)
        | Kind::Column
        | Kind::Flag
        | Kind::Choice(_)
        | Kind::Dtype
        | Kind::Duration
        | Kind::Pattern
        | Kind::TimeZone
        | Kind::Table
        | Kind::Path => {}
    }
    written
}

const fn next_kind(kind: &Kind) -> Option<Kind> {
    Some(match kind {
        Kind::Expression(_) => Kind::Column,
        Kind::Column => Kind::Value(ValueType::Integer),
        Kind::Value(_) => Kind::Flag,
        Kind::Flag => Kind::Choice(&[]),
        Kind::Choice(_) => Kind::Dtype,
        Kind::Dtype => Kind::Duration,
        Kind::Duration => Kind::Pattern,
        Kind::Pattern => Kind::TimeZone,
        Kind::TimeZone => Kind::Table,
        Kind::Table => Kind::Path,
        Kind::Path => Kind::Pairs(&Kind::Column, &Kind::Column),
        Kind::Pairs(..) => return None,
    })
}

fn kinds() -> Vec<Kind> {
    chain(Kind::Expression(&[]), next_kind)
}

/// How a parameter of this kind is set as an option, in the words of the engine's table of
/// options: only a flag and a choice have words to be set to.
const fn option_word(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Flag => Some("flag"),
        Kind::Choice(_) => Some("choice"),
        Kind::Expression(_)
        | Kind::Column
        | Kind::Value(_)
        | Kind::Dtype
        | Kind::Duration
        | Kind::Pattern
        | Kind::TimeZone
        | Kind::Table
        | Kind::Path
        | Kind::Pairs(..) => None,
    }
}

const fn shape_word(shape: catalog::Shape) -> &'static str {
    match shape {
        catalog::Shape::One => "one",
        catalog::Shape::Many => "many",
        catalog::Shape::PerItem(_) => "perItem",
    }
}

const fn next_shape(shape: &catalog::Shape) -> Option<catalog::Shape> {
    Some(match shape {
        catalog::Shape::One => catalog::Shape::Many,
        catalog::Shape::Many => catalog::Shape::PerItem(""),
        catalog::Shape::PerItem(_) => return None,
    })
}

/// How a parameter is held, as the catalogue gives it: its shape and its kind, with the type of a
/// value and the kind of each side of pairs. The forms the catalogue's parameters have, which the
/// view draws one way each.
fn parameter_forms() -> Vec<Value> {
    let mut forms: BTreeSet<String> = BTreeSet::new();
    for entry in catalog::entries() {
        for parameter in entry
            .parameters()
            .iter()
            .filter(|parameter| parameter.offered())
        {
            let form = json!({
                "shape": shape_word(parameter.shape()),
                "kind": kind_json(parameter.kind()),
            });
            forms.insert(form.to_string());
        }
    }
    forms
        .iter()
        .map(|form| serde_json::from_str(form).expect("a form"))
        .collect()
}

// How a value is placed.

fn next_after(after: &After) -> Option<After> {
    Some(match after {
        After::Start => After::Value {
            dtype: Representative::ALL[0],
            shape: Shape::One,
        },
        After::Value { .. } => After::Condition(ConditionState::When),
        After::Condition(_) => return None,
    })
}

fn afters() -> Vec<After> {
    chain(After::Start, next_after)
}

fn after_word(after: After) -> String {
    word(&super::after_view(after), "kind")
}

/// What a cell of the rows is: text, or null, which has none.
const fn cell_word(cell: &Option<String>) -> &'static str {
    match cell {
        Some(_) => "text",
        None => "null",
    }
}

/// Every kind of cell the rows hold and the values of an input show.
fn cells() -> Vec<&'static str> {
    [Some(String::new()), None].iter().map(cell_word).collect()
}

/// Every variant the step editor has to handle, as the view's census test reads it.
pub(crate) fn census() -> Value {
    let starts: Vec<Value> = {
        let mut starts: Vec<Value> = Vec::new();
        for start in starts_of_document() {
            match start_word(&start) {
                "call" => {
                    // A chain starts from a field, a typed value or another function.
                    starts.push(json!({ "kind": "field" }));
                    for value in typed_values() {
                        starts.push(json!({ "kind": "typed", "literal": value["literal"] }));
                    }
                    for entry in catalog::entries() {
                        if starts_a_chain(entry.build())
                            && !["polars.col", "polars.lit"].contains(&entry.id().as_str())
                        {
                            starts
                                .push(json!({ "kind": "function", "entry": entry.id().as_str() }));
                        }
                    }
                }
                "input" => {
                    for input in inputs() {
                        if let Some(kind) = input_start(&input) {
                            starts.push(json!({ "kind": kind }));
                        }
                    }
                }
                "hole" => starts.push(json!({ "kind": "hole" })),
                other => panic!("a start the census does not place: {other}"),
            }
        }
        starts
    };
    let after_places = afters();
    json!({
        "starts": starts,
        "typedValues": typed_values(),
        "typedReadings": typed_readings(),
        "parameterKinds": kinds().into_iter().map(kind_word).collect::<Vec<_>>(),
        "valueTypes": ValueType::ALL.map(value_type_word),
        "parameterShapes": chain(catalog::Shape::One, next_shape).into_iter().map(shape_word).collect::<Vec<_>>(),
        "parameterForms": parameter_forms(),
        "optionKinds": kinds().into_iter().filter_map(option_word).collect::<Vec<_>>(),
        "dtypeClasses": Representative::ALL.map(Representative::name).to_vec(),
        "valueShapes": plains(&Shape::ALL.map(shape_view)),
        "afterKinds": after_places.into_iter().map(after_word).collect::<Vec<_>>(),
        "conditionStates": plains(&ConditionState::ALL.map(condition_state_view)),
        "causes": words(&causes(), "kind"),
        "refusalSources": words(&sources(), "by"),
        "stages": plains(&Stage::ALL.map(stage_view)),
        "rules": Rule::ALL.map(|rule| json!({ "name": rule.name(), "statement": rule.statement() })),
        "outcomes": words(&outcomes(), "status"),
        "rowsStates": words(&counts(), "kind"),
        "states": states(),
        "inputKinds": words(&inputs(), "kind"),
        "origins": words(&origins(), "kind"),
        "cells": cells(),
    })
}

#[cfg(test)]
mod tests;
