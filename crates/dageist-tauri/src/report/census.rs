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
//! - Any other is listed by `every_variant!`, which takes one arm for each variant, by name, and
//!   expands both a `match` with no wildcard arm and the list the arms give. A variant added to the
//!   type does not compile until it has an arm, and no arm can stand for two variants, because it
//!   names one. An arm gives the samples of its variant, and one that gives none panics when the
//!   list is made, so each variant has a sample in the list. A test of the words checks that they
//!   are distinct.
//!
//! A word in a list is, where the view type is serialized, the word the host serializes for the
//! variant, read from the serialized sample. The words of a few engine types are written here by
//! an exhaustive `match`, which a test holds against the committed fixtures of the host:
//! `kind_word`, `value_type_word`, `shape_word`, `typed_word`, `literal_word`, `before_word` and
//! `slot_json`.

use std::collections::BTreeSet;

use dageist_engine::allowed::{After, ConditionState, Literal, Representative, Rule, Shape, Stage};
use dageist_engine::catalog::{self, Build, Kind, Setting, Shape as ShapeOf, ValueType};
use dageist_engine::document::{Call, Change, Expression, Place, Start, Value as Held, ValueKind};
use serde::Serialize;
use serde_json::{Value, json};

use super::{
    CauseView, CountView, OutcomeView, PlaceView, RefusalSourceView, RefusalView, ResolutionView,
    StageView, ValuesView, change_view, condition_state_view, place_view, shape_view, stage_view,
};
use crate::catalogue::{DefaultView, setting_view};
use crate::document::{
    EditAnswerView, ProblemView, ThingView, ValueKindView, WantedView, value_kind_view,
};
use crate::node::{InputView, OriginView};
use crate::offers::{TypedRequest, ValueTypeRequest, read_view};

/// Lists the samples of every variant of a type, from the one place that names them.
///
/// Each arm is `Variant => samples`, where `samples` is anything that iterates the values that
/// stand for the variant. The macro expands a `match` over the type with one arm for each name and
/// no wildcard, so a variant the invocation lacks does not compile, and an arm names one variant,
/// so none is left standing for another. An arm that yields no sample panics, naming its variant,
/// so a variant cannot be named and left out. It returns the samples of every arm, in order.
macro_rules! every_variant {
    ($ty:ident; $($variant:ident => $samples:expr),+ $(,)?) => {{
        let _every_variant_is_named = |placed: &$ty| match placed {
            $( $ty::$variant { .. } => () ),+
        };
        let mut all = Vec::new();
        $({
            let samples: Vec<_> = IntoIterator::into_iter($samples).collect();
            assert!(
                !samples.is_empty(),
                "the arm for {} yields no sample",
                stringify!($variant)
            );
            all.extend(samples);
        })+
        all
    }};
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

fn sources() -> Vec<RefusalSourceView> {
    every_variant!(RefusalSourceView;
        Polars => [RefusalSourceView::Polars { stage: StageView::Schema }],
        Rule => [RefusalSourceView::Rule { rule: Rule::ALL[0].name() }],
        Structure => [RefusalSourceView::Structure],
    )
}

// Why a step was refused.

fn causes() -> Vec<CauseView> {
    every_variant!(CauseView;
        UnknownTable => [CauseView::UnknownTable { name: String::new() }],
        UnknownInput => [CauseView::UnknownInput { name: String::new() }],
        Build => [CauseView::Build { reason: String::new() }],
        NotAllowed => [CauseView::NotAllowed { refusal: refusal(RefusalSourceView::Structure) }],
        Polars => [CauseView::Polars { reason: String::new() }],
    )
}

// What running a step on the rows came to.

fn counts() -> Vec<CountView> {
    every_variant!(CountView;
        NotCounted => [CountView::NotCounted],
        Counted => [CountView::Counted { received: 0, kept: 0, first: Vec::new() }],
        Failed => [CountView::Failed { reason: String::new() }],
    )
}

// What running a step came to.

fn here() -> PlaceView {
    place_view(&Place::of_step(0, None))
}

fn outcomes() -> Vec<OutcomeView> {
    every_variant!(OutcomeView;
        Valid => [OutcomeView::Valid { columns: Vec::new(), changes: Vec::new() }],
        Refused => [OutcomeView::Refused {
            place: here(),
            cause: CauseView::Polars { reason: String::new() },
        }],
        Incomplete => [OutcomeView::Incomplete { place: here() }],
        NotRun => [OutcomeView::NotRun { blocked_by: String::new() }],
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

/// Every state of what came before, from the one place that names them.
fn befores() -> Vec<Before> {
    every_variant!(Before;
        Nothing => [Before::Nothing],
        Refused => [Before::Refused],
        Incomplete => [Before::Incomplete],
        Failed => [Before::Failed],
        NotRun => [Before::NotRun],
    )
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
            for before in befores() {
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

fn inputs() -> Vec<InputView> {
    every_variant!(InputView;
        Table => [InputView::Table {
            name: String::new(),
            from: OriginView::Parameter,
            columns: Vec::new(),
        }],
        Value => [InputView::Value {
            name: String::new(),
            from: OriginView::Parameter,
            dtype: String::new(),
            value: None,
        }],
        List => [InputView::List {
            name: String::new(),
            from: OriginView::Parameter,
            dtype: String::new(),
            values: ValuesView { shown: Vec::new(), more: 0 },
        }],
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

fn origins() -> Vec<OriginView> {
    every_variant!(OriginView;
        Node => [OriginView::Node { name: String::new() }],
        Column => [OriginView::Column { node: String::new(), column: String::new() }],
        Parameter => [OriginView::Parameter],
    )
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

fn held_values() -> Vec<Held> {
    let field = Expression::field("a").expect("a field");
    every_variant!(Held;
        Expression => [Held::Expression(field)],
        Columns => [Held::Columns(Vec::new())],
        Integer => [Held::Integer(0)],
        Float => [Held::Float(0.0)],
        String => [Held::String(String::new())],
        Flag => [Held::Flag(Some(true))],
        Choice => [Held::Choice(String::new())],
        Dtype => [Held::Dtype("Int64".parse().expect("a dtype"))],
        Duration => [Held::Duration(String::new())],
        Pattern => [Held::Pattern(String::new())],
        TimeZone => [Held::TimeZone(None)],
        Table => [Held::Table(String::new())],
        Path => [Held::Path(String::new())],
        Pairs => [Held::Pairs(Vec::new())],
        Many => [Held::Many(Vec::new())],
    )
}

const fn literal_word(literal: &Literal) -> &'static str {
    match literal {
        Literal::Integer(_) => "integer",
        Literal::Float(_) => "float",
        Literal::String(_) => "text",
    }
}

fn literals() -> Vec<Literal> {
    every_variant!(Literal;
        Integer => [Literal::Integer(0)],
        Float => [Literal::Float(0.0)],
        String => [Literal::String(String::new())],
    )
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

/// The types of literal a slot may take, from the one place that names them.
fn value_types() -> Vec<ValueTypeRequest> {
    every_variant!(ValueTypeRequest;
        Integer => [ValueTypeRequest::Integer],
        Float => [ValueTypeRequest::Float],
        String => [ValueTypeRequest::String],
        Date => [ValueTypeRequest::Date],
        Any => [ValueTypeRequest::Any],
    )
}

fn typed_slots() -> Vec<TypedRequest> {
    every_variant!(TypedRequest;
        Expression => [TypedRequest::Expression],
        Value => value_types().into_iter().map(|value_type| TypedRequest::Value { value_type }),
        Duration => [TypedRequest::Duration],
        Pattern => [TypedRequest::Pattern],
        TimeZone => [TypedRequest::TimeZone],
        Path => [TypedRequest::Path],
    )
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

fn kinds() -> Vec<Kind> {
    every_variant!(Kind;
        Expression => [Kind::Expression(&[])],
        Column => [Kind::Column],
        Value => [Kind::Value(ValueType::Integer)],
        Flag => [Kind::Flag],
        Choice => [Kind::Choice(&[])],
        Dtype => [Kind::Dtype],
        Duration => [Kind::Duration],
        Pattern => [Kind::Pattern],
        TimeZone => [Kind::TimeZone],
        Table => [Kind::Table],
        Path => [Kind::Path],
        Pairs => [Kind::Pairs(&Kind::Column, &Kind::Column)],
    )
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

fn shapes() -> Vec<catalog::Shape> {
    every_variant!(ShapeOf;
        One => [catalog::Shape::One],
        Many => [catalog::Shape::Many],
        PerItem => [catalog::Shape::PerItem("")],
    )
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

fn afters() -> Vec<After> {
    every_variant!(After;
        Start => [After::Start],
        Value => [After::Value { dtype: Representative::ALL[0], shape: Shape::One }],
        Condition => [After::Condition(ConditionState::When)],
    )
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

// What the host sends besides the start of a chain.

fn resolutions() -> Vec<ResolutionView> {
    every_variant!(ResolutionView;
        Dtype => [ResolutionView::Dtype { dtype: String::new() }],
        Several => [ResolutionView::Several { dtypes: Vec::new() }],
        Unfinished => [ResolutionView::Unfinished],
        NoContext => [ResolutionView::NoContext],
        Refused => [ResolutionView::Refused { reason: String::new() }],
    )
}

fn defaults() -> Vec<DefaultView> {
    every_variant!(DefaultView;
        Required => [DefaultView::Required],
        Unset => [DefaultView::Unset],
        Flag => [DefaultView::Flag { value: false }],
        Integer => [DefaultView::Integer { value: 0 }],
        Float => [DefaultView::Float { value: 0.0 }],
        Text => [DefaultView::Text { value: String::new() }],
        Empty => [DefaultView::Empty],
        Other => [DefaultView::Other],
    )
}

/// One setting of every kind the engine reports.
fn settings() -> Vec<Setting> {
    every_variant!(Setting;
        ToGive => [Setting::ToGive],
        Flag => [Setting::Flag(true)],
        Integer => [Setting::Integer(1)],
        Float => [Setting::Float(1.5)],
        Text => [Setting::Text(String::new())],
        Word => [Setting::Word(String::new())],
        Field => [Setting::Field(String::new())],
        Element => [Setting::Element("first")],
        Comparison => [Setting::Comparison {
            entry: "Expr.eq",
            field: Box::new(Setting::ToGive),
            other: Box::new(Setting::ToGive),
        }],
        List => [Setting::List(Vec::new())],
        Pairs => [Setting::Pairs(Vec::new())],
    )
}

fn problems() -> Vec<ProblemView> {
    every_variant!(ProblemView;
        NoStep => [ProblemView::NoStep { steps: 0 }],
        StepChanged => [ProblemView::StepChanged { found: String::new() }],
        Nothing => [ProblemView::Nothing],
        DoesNotFit => [ProblemView::DoesNotFit { found: ThingView::Step }],
        WrongPlace => [ProblemView::WrongPlace { needs: WantedView::Call, found: ThingView::Step }],
        NotPerItem => [ProblemView::NotPerItem],
        NoDefault => [ProblemView::NoDefault],
        OutOfRange => [ProblemView::OutOfRange { index: 0, len: 0 }],
        UnknownEntry => [ProblemView::UnknownEntry { entry: String::new() }],
        NoStart => [ProblemView::NoStart { entry: "" }],
        CannotAdd => [ProblemView::CannotAdd { reason: String::new() }],
        NeedsValue => [ProblemView::NeedsValue { parameter: "" }],
        Refused => [ProblemView::Refused { reason: String::new() }],
    )
}

/// What a place can name, one for each thing and one for each kind of value.
fn things() -> Vec<ThingView> {
    every_variant!(ThingView;
        Step => [ThingView::Step],
        Call => [ThingView::Call],
        Parameter => [ThingView::Parameter],
        Item => [ThingView::Item],
        Key => [ThingView::Key],
        EntryValue => [ThingView::EntryValue],
        Start => [ThingView::Start],
        Link => [ThingView::Link],
        ExpressionWithoutCall => [ThingView::ExpressionWithoutCall],
        Value => [ThingView::Value { of: ValueKindView::Expression }],
    )
}

/// What a value an argument holds is, one for each variant of the document's value.
fn value_kinds() -> Vec<ValueKindView> {
    held_values()
        .iter()
        .map(|value| value_kind_view(ValueKind::of(value)))
        .collect()
}

fn wanted() -> Vec<WantedView> {
    every_variant!(WantedView;
        Call => [WantedView::Call],
        Argument => [WantedView::Argument],
        Parameter => [WantedView::Parameter],
        Item => [WantedView::Item],
        EntryPart => [WantedView::EntryPart],
        Link => [WantedView::Link],
        Expression => [WantedView::Expression],
        List => [WantedView::List],
        Pairs => [WantedView::Pairs],
        PartOfStep => [WantedView::PartOfStep],
    )
}

/// What an edit came to, by the word of its status.
fn edit_answers() -> Vec<&'static str> {
    every_variant!(EditAnswerView;
        Edited => ["edited"],
        Refused => ["refused"],
    )
}

/// What a chain in a document starts from, as the editor names the kinds of start.
fn starts() -> Vec<Value> {
    every_variant!(Start;
        Call => {
            // A field, a number, text or flag typed in, or another function that starts a chain.
            let mut called = vec![json!({ "kind": "field" })];
            called.extend(
                typed_values()
                    .iter()
                    .map(|value| json!({ "kind": "typed", "literal": value["literal"] })),
            );
            called.extend(
                catalog::entries()
                    .iter()
                    .filter(|entry| {
                        starts_a_chain(entry.build())
                            && !["polars.col", "polars.lit"].contains(&entry.id().as_str())
                    })
                    .map(|entry| json!({ "kind": "function", "entry": entry.id().as_str() })),
            );
            called
        },
        Input => inputs()
            .iter()
            .filter_map(input_start)
            .map(|kind| json!({ "kind": kind })),
        Hole => [json!({ "kind": "hole" })],
    )
}

/// Every variant the step editor has to handle, as the view's census test reads it.
pub(crate) fn census() -> Value {
    json!({
        "starts": starts(),
        "typedValues": typed_values(),
        "typedReadings": typed_readings(),
        "parameterKinds": kinds().into_iter().map(kind_word).collect::<Vec<_>>(),
        "valueTypes": ValueType::ALL.map(value_type_word),
        "parameterShapes": shapes().into_iter().map(shape_word).collect::<Vec<_>>(),
        "parameterForms": parameter_forms(),
        "optionKinds": kinds().into_iter().filter_map(option_word).collect::<Vec<_>>(),
        "defaultKinds": words(&defaults(), "kind"),
        "settingKinds": settings().iter().map(|setting| word(&setting_view(setting), "kind")).collect::<Vec<_>>(),
        "dtypeClasses": Representative::ALL.map(Representative::name).to_vec(),
        "valueShapes": plains(&Shape::ALL.map(shape_view)),
        "afterKinds": afters().into_iter().map(after_word).collect::<Vec<_>>(),
        "conditionStates": plains(&ConditionState::ALL.map(condition_state_view)),
        "resolutions": words(&resolutions(), "kind"),
        "causes": words(&causes(), "kind"),
        "refusalSources": words(&sources(), "by"),
        "stages": plains(&Stage::ALL.map(stage_view)),
        "rules": Rule::ALL.map(|rule| json!({ "name": rule.name(), "statement": rule.statement() })),
        "outcomes": words(&outcomes(), "status"),
        "rowsStates": words(&counts(), "kind"),
        "changes": plains(&Change::ALL.map(change_view)),
        "states": states(),
        "editAnswers": edit_answers(),
        "problemKinds": words(&problems(), "kind"),
        "thingKinds": words(&things(), "kind"),
        "valueKinds": plains(&value_kinds()),
        "wanted": plains(&wanted()),
        "inputKinds": words(&inputs(), "kind"),
        "origins": words(&origins(), "kind"),
        "cells": cells(),
    })
}

#[cfg(test)]
mod tests;
