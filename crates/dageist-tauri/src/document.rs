//! The node's step document, which the host holds, and the edits that change it.
//!
//! The host owns the document and its revision. A window asks for both with
//! `node_document`, and changes the document by sending an edit with the revision
//! it was made against: the host refuses one made against another revision, so a
//! place from an earlier document never lands on a step that has taken its identifier.
//! Each edit is a call of the engine's editing API, which the engine checks as it makes
//! it; this module only reads the request and writes the answer. The shapes mirror
//! `DocumentStateView`, `EditAnswerView` and the types beside them in
//! `ui/packages/host/src/contract.ts`.
//!
//! One command takes every edit, tagged by its kind, so that the check of the revision,
//! the run of the edited document and the answer are written once, not for each of the
//! nineteen edits the engine makes.
//!
//! The document the view receives is not its JSON text: it is a tree of the calls the
//! document holds, with the words the engine shows each number as, so that the view
//! formats no value and never reads a number it cannot hold.

use std::sync::{Mutex, PoisonError};

use dageist_engine::allowed::{Literal, Place as OfferPlace};
use dageist_engine::catalog::StepFrame;
use dageist_engine::claim_timing::ClaimTiming;
use dageist_engine::document::{
    Call, Document, EditError, EditProblem, Expression, Start, Thing, Value, ValueKind, Wanted,
};
use serde::{Deserialize, Serialize};

use crate::offers::PlaceRequest;
use crate::report::{PlaceView, RunView, place_view};
use crate::sample_table::{ErrorKind, ErrorView};

/// One call of the document: the entry of the catalogue it uses and the arguments it was
/// given, each by Python's name for its parameter, in the order they were given.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CallView {
    entry: &'static str,
    arguments: Vec<GivenView>,
}

/// The argument given for one parameter of a call.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GivenView {
    parameter: String,
    value: ArgumentView,
}

/// Where an expression starts.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartView {
    /// A call of a function that makes an expression, or `polars.when`.
    Call { call: CallView },
    /// A connected value or list, by name.
    Input { name: String },
    /// Nothing picked yet.
    Hole,
}

/// One entry of pairs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EntryView {
    key: ArgumentView,
    value: ArgumentView,
}

/// What an argument is. A number is not sent as a number: the view draws the words the
/// engine shows it as, and never needs to read it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ArgumentView {
    /// A chain: where it starts, then the links after it.
    Expression {
        start: StartView,
        links: Vec<CallView>,
    },
    /// Names of input columns.
    Columns { names: Vec<String> },
    /// A whole number, as the engine shows it.
    Integer { shown: String },
    /// A number with a fraction, as the engine shows it.
    Float { shown: String },
    /// Text.
    String { value: String },
    /// True, false, or unset.
    Flag { value: Option<bool> },
    /// One word of a closed set.
    Choice { word: String },
    /// A dtype, as the document spells it.
    Dtype { name: String },
    /// A duration string.
    Duration { text: String },
    /// A regular expression, as typed.
    Pattern { text: String },
    /// A time zone as typed, or none.
    TimeZone { zone: Option<String> },
    /// The name of a connected table.
    Table { name: String },
    /// A file, folder or cloud location.
    Path { text: String },
    /// Key and value entries.
    Pairs { entries: Vec<EntryView> },
    /// An ordered list.
    Many { items: Vec<ArgumentView> },
}

/// One step of the document.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepView {
    id: String,
    call: CallView,
}

/// The steps of a document, in the order they run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocumentView {
    steps: Vec<StepView>,
}

fn call_view(call: &Call) -> CallView {
    CallView {
        entry: call.entry().id().as_str(),
        arguments: call
            .arguments()
            .iter()
            .map(|given| GivenView {
                parameter: given.parameter().to_owned(),
                value: argument_view(given.value()),
            })
            .collect(),
    }
}

fn expression_view(expression: &Expression) -> ArgumentView {
    ArgumentView::Expression {
        start: match expression.start() {
            Start::Call(call) => StartView::Call {
                call: call_view(call),
            },
            Start::Input(name) => StartView::Input { name: name.clone() },
            Start::Hole => StartView::Hole,
        },
        links: expression.links().iter().map(call_view).collect(),
    }
}

fn argument_view(value: &Value) -> ArgumentView {
    match value {
        Value::Expression(expression) => expression_view(expression),
        Value::Columns(names) => ArgumentView::Columns {
            names: names.clone(),
        },
        Value::Integer(number) => ArgumentView::Integer {
            shown: Literal::Integer(*number).shown(),
        },
        Value::Float(number) => ArgumentView::Float {
            shown: Literal::Float(*number).shown(),
        },
        Value::String(value) => ArgumentView::String {
            value: value.clone(),
        },
        Value::Flag(value) => ArgumentView::Flag { value: *value },
        Value::Choice(word) => ArgumentView::Choice { word: word.clone() },
        Value::Dtype(dtype) => ArgumentView::Dtype {
            name: dtype.to_string(),
        },
        Value::Duration(text) => ArgumentView::Duration { text: text.clone() },
        Value::Pattern(text) => ArgumentView::Pattern { text: text.clone() },
        Value::TimeZone(zone) => ArgumentView::TimeZone { zone: zone.clone() },
        Value::Table(name) => ArgumentView::Table { name: name.clone() },
        Value::Path(text) => ArgumentView::Path { text: text.clone() },
        Value::Pairs(pairs) => ArgumentView::Pairs {
            entries: pairs
                .iter()
                .map(|(key, value)| EntryView {
                    key: argument_view(key),
                    value: argument_view(value),
                })
                .collect(),
        },
        Value::Many(items) => ArgumentView::Many {
            items: items.iter().map(argument_view).collect(),
        },
    }
}

/// The view of `document`.
pub fn document_view(document: &Document) -> DocumentView {
    DocumentView {
        steps: document
            .steps()
            .iter()
            .map(|step| StepView {
                id: step.id().as_str().to_owned(),
                call: call_view(step.call()),
            })
            .collect(),
    }
}

/// What a window needs to draw the node's steps: the document, the revision it is at, and what
/// running it reported.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocumentStateView {
    revision: u64,
    document: DocumentView,
    run: RunView,
}

// -- What an edit is asked for -------------------------------------------------------------

/// A typed number or text that an expression is made of.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum LiteralRequest {
    Integer { value: i64 },
    Float { value: f64 },
    Text { value: String },
}

impl LiteralRequest {
    fn literal(&self) -> Literal {
        match self {
            Self::Integer { value } => Literal::Integer(*value),
            Self::Float { value } => Literal::Float(*value),
            Self::Text { value } => Literal::String(value.clone()),
        }
    }
}

/// An expression of one word, which an edit puts where an expression stands.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum ExpressionRequest {
    /// Nothing picked yet.
    Hole,
    /// The column called `name`.
    Field { name: String },
    /// The connected input called `name`.
    Input { name: String },
    /// A number or text.
    Literal { literal: LiteralRequest },
}

impl ExpressionRequest {
    fn expression(&self) -> Result<Expression, ErrorView> {
        match self {
            Self::Hole => Ok(Expression::hole()),
            Self::Field { name } => Expression::field(name.as_str()),
            Self::Input { name } => Ok(Expression::input(name.as_str())),
            Self::Literal { literal } => Expression::literal(&literal.literal()),
        }
        .map_err(|refusal| ErrorView::of(ErrorKind::Failed, refusal.to_string()))
    }
}

/// What an expression starts from, for an edit that changes only that.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum StartRequest {
    Hole,
    Input { name: String },
    Field { name: String },
}

impl StartRequest {
    fn start(&self) -> Result<Start, ErrorView> {
        match self {
            Self::Hole => Ok(Start::Hole),
            Self::Input { name } => Ok(Start::Input(name.clone())),
            Self::Field { name } => Start::field(name.as_str())
                .map_err(|refusal| ErrorView::of(ErrorKind::Failed, refusal.to_string())),
        }
    }
}

/// One entry of pairs in a request.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EntryRequest {
    key: ValueRequest,
    value: ValueRequest,
}

/// An argument that an edit gives a parameter. Its kind is the kind of the parameter it is
/// for, as the catalogue gives it; the model refuses an argument of another kind.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum ValueRequest {
    Expression { expression: ExpressionRequest },
    Columns { names: Vec<String> },
    Integer { value: i64 },
    Float { value: f64 },
    String { value: String },
    Flag { value: Option<bool> },
    Choice { word: String },
    Dtype { name: String },
    Duration { text: String },
    Pattern { text: String },
    TimeZone { zone: Option<String> },
    Table { name: String },
    Path { text: String },
    Pairs { entries: Vec<EntryRequest> },
    Many { items: Vec<ValueRequest> },
}

impl ValueRequest {
    fn value(&self) -> Result<Value, ErrorView> {
        Ok(match self {
            Self::Expression { expression } => Value::Expression(expression.expression()?),
            Self::Columns { names } => Value::Columns(names.clone()),
            Self::Integer { value } => Value::Integer(*value),
            Self::Float { value } => Value::Float(*value),
            Self::String { value } => Value::String(value.clone()),
            Self::Flag { value } => Value::Flag(*value),
            Self::Choice { word } => Value::Choice(word.clone()),
            Self::Dtype { name } => Value::Dtype(name.parse().map_err(|error| {
                ErrorView::of(
                    ErrorKind::UnknownName,
                    format!("{name:?} is not a dtype: {error}"),
                )
            })?),
            Self::Duration { text } => Value::Duration(text.clone()),
            Self::Pattern { text } => Value::Pattern(text.clone()),
            Self::TimeZone { zone } => Value::TimeZone(zone.clone()),
            Self::Table { name } => Value::Table(name.clone()),
            Self::Path { text } => Value::Path(text.clone()),
            Self::Pairs { entries } => Value::Pairs(
                entries
                    .iter()
                    .map(|entry| Ok((entry.key.value()?, entry.value.value()?)))
                    .collect::<Result<_, ErrorView>>()?,
            ),
            Self::Many { items } => Value::Many(
                items
                    .iter()
                    .map(ValueRequest::value)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}

/// The table a new step starts on, and the tables it can bring in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FrameRequest {
    columns: Vec<String>,
    tables: Vec<String>,
}

/// One edit of the document, by the engine's name for it, with the place it is made at.
/// Each is a call of the engine's editing API on the document the host holds.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum EditRequest {
    /// A new step of an entry, started as the catalogue starts it, as the `at`th.
    AddStep {
        at: usize,
        entry: String,
        frame: FrameRequest,
    },
    /// Another entry for a step, started afresh.
    SwapStep {
        step: PlaceView,
        entry: String,
        frame: FrameRequest,
    },
    /// A step taken away.
    RemoveStep { step: PlaceView },
    /// A step moved to the position `to`.
    MoveStep { step: PlaceView, to: usize },
    /// An argument set.
    SetArgument {
        place: PlaceView,
        value: ValueRequest,
    },
    /// An argument taken away.
    ClearArgument { place: PlaceView },
    /// An option added with what the catalogue starts it with.
    AddOption { call: PlaceView, parameter: String },
    /// One value of a parameter that holds one for each item of another.
    SetForItem {
        call: PlaceView,
        parameter: String,
        item: usize,
        value: ValueRequest,
    },
    /// An item put into a list.
    InsertItem {
        list: PlaceView,
        at: usize,
        item: ValueRequest,
    },
    /// An item taken out of a list.
    RemoveItem { item: PlaceView },
    /// A new test put into a list of conditions.
    InsertTest { list: PlaceView, at: usize },
    /// An entry put into pairs.
    InsertEntry {
        pairs: PlaceView,
        at: usize,
        key: ValueRequest,
        value: ValueRequest,
    },
    /// An entry taken out of pairs.
    RemoveEntry { entry: PlaceView },
    /// What an expression starts from, changed.
    SetStart {
        expression: PlaceView,
        start: StartRequest,
    },
    /// An expression made to start from a function.
    StartWith {
        expression: PlaceView,
        entry: String,
    },
    /// A method put into an expression.
    InsertLink {
        expression: PlaceView,
        at: usize,
        entry: String,
        standing: Option<PlaceRequest>,
    },
    /// A link made a link of another method.
    SwapLink {
        link: PlaceView,
        entry: String,
        standing: Option<PlaceRequest>,
    },
    /// A link taken away.
    RemoveLink { link: PlaceView },
    /// A link moved to the position `to`.
    MoveLink { link: PlaceView, to: usize },
}

fn bad_place(error: impl std::fmt::Display) -> ErrorView {
    ErrorView::of(ErrorKind::UnknownName, format!("a place: {error}"))
}

/// The result of an edit: the edited document, or why the engine refused it. A request the host
/// cannot read as an edit is not either: it is a failed command.
type Made = Result<Document, EditError>;

impl EditRequest {
    /// Makes the edit on `document`.
    ///
    /// # Errors
    ///
    /// When the request names a place, a dtype or a position the engine has no word for: that is
    /// no edit of the engine's to refuse.
    fn make(&self, document: &Document) -> Result<Made, ErrorView> {
        let place = |view: &PlaceView| view.place().map_err(bad_place);
        let standing = |request: &Option<PlaceRequest>| -> Result<Option<OfferPlace>, ErrorView> {
            request.as_ref().map(PlaceRequest::place).transpose()
        };
        Ok(match self {
            Self::AddStep { at, entry, frame } => document.add_step(
                *at,
                entry,
                &StepFrame {
                    columns: &frame.columns,
                    tables: &frame.tables,
                },
            ),
            Self::SwapStep { step, entry, frame } => document.swap_step(
                &place(step)?,
                entry,
                &StepFrame {
                    columns: &frame.columns,
                    tables: &frame.tables,
                },
            ),
            Self::RemoveStep { step } => document.remove_step(&place(step)?),
            Self::MoveStep { step, to } => document.move_step(&place(step)?, *to),
            Self::SetArgument { place: at, value } => {
                document.set_argument(&place(at)?, value.value()?)
            }
            Self::ClearArgument { place: at } => document.clear_argument(&place(at)?),
            Self::AddOption { call, parameter } => document.add_option(&place(call)?, parameter),
            Self::SetForItem {
                call,
                parameter,
                item,
                value,
            } => document.set_for_item(&place(call)?, parameter, *item, value.value()?),
            Self::InsertItem { list, at, item } => {
                document.insert_item(&place(list)?, *at, item.value()?)
            }
            Self::RemoveItem { item } => document.remove_item(&place(item)?),
            Self::InsertTest { list, at } => document.insert_test(&place(list)?, *at),
            Self::InsertEntry {
                pairs,
                at,
                key,
                value,
            } => document.insert_entry(&place(pairs)?, *at, key.value()?, value.value()?),
            Self::RemoveEntry { entry } => document.remove_entry(&place(entry)?),
            Self::SetStart { expression, start } => {
                document.set_start(&place(expression)?, start.start()?)
            }
            Self::StartWith { expression, entry } => {
                document.start_with(&place(expression)?, entry)
            }
            Self::InsertLink {
                expression,
                at,
                entry,
                standing: at_place,
            } => document.insert_link(
                &place(expression)?,
                *at,
                entry,
                standing(at_place)?.as_ref(),
            ),
            Self::SwapLink {
                link,
                entry,
                standing: at_place,
            } => document.swap_link(&place(link)?, entry, standing(at_place)?.as_ref()),
            Self::RemoveLink { link } => document.remove_link(&place(link)?),
            Self::MoveLink { link, to } => document.move_link(&place(link)?, *to),
        })
    }
}

// -- Why an edit was refused ---------------------------------------------------------------

/// What a value is, by the word the engine's list gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ValueKindView {
    Expression,
    Columns,
    Integer,
    Float,
    String,
    Flag,
    Choice,
    Dtype,
    Duration,
    Pattern,
    TimeZone,
    Table,
    Path,
    Pairs,
    List,
}

const fn value_kind_view(kind: ValueKind) -> ValueKindView {
    match kind {
        ValueKind::Expression => ValueKindView::Expression,
        ValueKind::Columns => ValueKindView::Columns,
        ValueKind::Integer => ValueKindView::Integer,
        ValueKind::Float => ValueKindView::Float,
        ValueKind::String => ValueKindView::String,
        ValueKind::Flag => ValueKindView::Flag,
        ValueKind::Choice => ValueKindView::Choice,
        ValueKind::Dtype => ValueKindView::Dtype,
        ValueKind::Duration => ValueKindView::Duration,
        ValueKind::Pattern => ValueKindView::Pattern,
        ValueKind::TimeZone => ValueKindView::TimeZone,
        ValueKind::Table => ValueKindView::Table,
        ValueKind::Path => ValueKindView::Path,
        ValueKind::Pairs => ValueKindView::Pairs,
        ValueKind::List => ValueKindView::List,
    }
}

#[cfg(test)]
impl ValueKindView {
    /// Every kind, in the order the contract lists them, for the vocabulary snapshot.
    pub(crate) const ALL: [Self; 15] = [
        Self::Expression,
        Self::Columns,
        Self::Integer,
        Self::Float,
        Self::String,
        Self::Flag,
        Self::Choice,
        Self::Dtype,
        Self::Duration,
        Self::Pattern,
        Self::TimeZone,
        Self::Table,
        Self::Path,
        Self::Pairs,
        Self::List,
    ];
}

/// What a place names, by the word the engine's list gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum ThingView {
    Step,
    Call,
    Parameter,
    Item,
    Key,
    EntryValue,
    Start,
    Link,
    ExpressionWithoutCall,
    /// An argument of this kind.
    #[serde(rename_all = "camelCase")]
    Value {
        of: ValueKindView,
    },
}

const fn thing_view(thing: Thing) -> ThingView {
    match thing {
        Thing::Step => ThingView::Step,
        Thing::Call => ThingView::Call,
        Thing::Parameter => ThingView::Parameter,
        Thing::Item => ThingView::Item,
        Thing::Key => ThingView::Key,
        Thing::EntryValue => ThingView::EntryValue,
        Thing::Start => ThingView::Start,
        Thing::Link => ThingView::Link,
        Thing::ExpressionWithoutCall => ThingView::ExpressionWithoutCall,
        Thing::Value(kind) => ThingView::Value {
            of: value_kind_view(kind),
        },
    }
}

#[cfg(test)]
impl ThingView {
    /// Every word that is not a kind of value, in the order the contract lists them.
    pub(crate) const KINDS: [&'static str; 10] = [
        "step",
        "call",
        "parameter",
        "item",
        "key",
        "entryValue",
        "start",
        "link",
        "expressionWithoutCall",
        "value",
    ];
}

/// What an edit needs the place to name, by the word the engine's list gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WantedView {
    Call,
    Argument,
    Parameter,
    Item,
    EntryPart,
    Link,
    Expression,
    List,
    Pairs,
    PartOfStep,
}

const fn wanted_view(wanted: Wanted) -> WantedView {
    match wanted {
        Wanted::Call => WantedView::Call,
        Wanted::Argument => WantedView::Argument,
        Wanted::Parameter => WantedView::Parameter,
        Wanted::Item => WantedView::Item,
        Wanted::EntryPart => WantedView::EntryPart,
        Wanted::Link => WantedView::Link,
        Wanted::Expression => WantedView::Expression,
        Wanted::List => WantedView::List,
        Wanted::Pairs => WantedView::Pairs,
        Wanted::PartOfStep => WantedView::PartOfStep,
    }
}

#[cfg(test)]
impl WantedView {
    /// Every word, in the order the contract lists them, for the vocabulary snapshot.
    pub(crate) const ALL: [Self; 10] = [
        Self::Call,
        Self::Argument,
        Self::Parameter,
        Self::Item,
        Self::EntryPart,
        Self::Link,
        Self::Expression,
        Self::List,
        Self::Pairs,
        Self::PartOfStep,
    ];
}

/// Why the engine refused an edit.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum ProblemView {
    /// The place names a step the document does not have.
    NoStep { steps: usize },
    /// The place names a step by an identifier the step at its number does not have.
    StepChanged { found: String },
    /// Nothing is at the place.
    Nothing,
    /// What is there has no such part.
    DoesNotFit { found: ThingView },
    /// The place names another kind of thing than the edit works on.
    WrongPlace { needs: WantedView, found: ThingView },
    /// The parameter holds no value for each item of another.
    NotPerItem,
    /// The parameter has no default for the items it has no value for.
    NoDefault,
    /// A position past the end.
    OutOfRange { index: usize, len: usize },
    /// An entry the catalogue lacks.
    UnknownEntry { entry: String },
    /// An entry that starts no step.
    NoStart { entry: &'static str },
    /// An option Polars refuses every other word of.
    CannotAdd { reason: String },
    /// An option that starts with nothing a document holds: it needs its value.
    NeedsValue { parameter: &'static str },
    /// The model refuses the result.
    Refused { reason: String },
}

#[cfg(test)]
impl ProblemView {
    /// Every kind, in the order the contract lists them, for the vocabulary snapshot.
    pub(crate) const KINDS: [&'static str; 13] = [
        "noStep",
        "stepChanged",
        "nothing",
        "doesNotFit",
        "wrongPlace",
        "notPerItem",
        "noDefault",
        "outOfRange",
        "unknownEntry",
        "noStart",
        "cannotAdd",
        "needsValue",
        "refused",
    ];
}

fn problem_view(problem: &EditProblem) -> ProblemView {
    match problem {
        EditProblem::NoStep { steps } => ProblemView::NoStep { steps: *steps },
        EditProblem::StepChanged { found } => ProblemView::StepChanged {
            found: found.as_str().to_owned(),
        },
        EditProblem::Nothing => ProblemView::Nothing,
        EditProblem::DoesNotFit { found } => ProblemView::DoesNotFit {
            found: thing_view(*found),
        },
        EditProblem::WrongPlace { needs, found } => ProblemView::WrongPlace {
            needs: wanted_view(*needs),
            found: thing_view(*found),
        },
        EditProblem::NotPerItem => ProblemView::NotPerItem,
        EditProblem::NoDefault => ProblemView::NoDefault,
        EditProblem::OutOfRange { index, len } => ProblemView::OutOfRange {
            index: *index,
            len: *len,
        },
        EditProblem::UnknownEntry(entry) => ProblemView::UnknownEntry {
            entry: entry.clone(),
        },
        EditProblem::NoStart { entry } => ProblemView::NoStart { entry },
        EditProblem::CannotAdd(refusal) => ProblemView::CannotAdd {
            reason: refusal.reason().to_owned(),
        },
        EditProblem::NeedsValue { parameter } => ProblemView::NeedsValue { parameter },
        EditProblem::Refused(cause) => ProblemView::Refused {
            reason: cause.to_string(),
        },
    }
}

/// What an edit came to.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum EditAnswerView {
    /// The edit was made: the document as it is now, its new revision, and what running it
    /// reported.
    Edited(DocumentStateView),
    /// The engine refused the edit, here, for this reason, in words and as a kind. The
    /// document is as it was.
    Refused {
        revision: u64,
        place: PlaceView,
        problem: ProblemView,
        message: String,
    },
}

// -- What the host holds -------------------------------------------------------------------

/// A document and the revision it is at.
#[derive(Debug)]
struct Held {
    document: Document,
    revision: u64,
}

/// The documents the host holds, which is one: the sample node's. Tauri keeps it as managed
/// state; the bridge keeps it for as long as it runs.
#[derive(Debug)]
pub struct Documents {
    held: Mutex<Held>,
}

impl Documents {
    /// The documents of a node whose steps are `document`, at the first revision.
    pub fn of(document: Document) -> Self {
        Self {
            held: Mutex::new(Held {
                document,
                revision: 1,
            }),
        }
    }

    /// The documents of the sample node, which starts with its own steps.
    ///
    /// # Errors
    ///
    /// When the sample node cannot be made.
    pub fn sample() -> Result<Self, ErrorView> {
        let node = ClaimTiming::new().map_err(ErrorView::from)?;
        Ok(Self::of(node.steps().clone()))
    }

    /// The document and its revision, together.
    pub(crate) fn snapshot(&self) -> (Document, u64) {
        let held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        (held.document.clone(), held.revision)
    }

    /// Holds the revision `revision`, for a test that starts a document at another.
    #[cfg(test)]
    pub(crate) fn set_revision(&self, revision: u64) {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .revision = revision;
    }

    /// The document at `revision`.
    ///
    /// # Errors
    ///
    /// When the document is at another revision.
    pub fn at(&self, revision: u64) -> Result<Document, ErrorView> {
        let (document, current) = self.snapshot();
        if current == revision {
            Ok(document)
        } else {
            Err(stale(revision, current))
        }
    }

    /// Makes `request` on the document at `revision`, and holds the edited document at the next
    /// revision.
    fn edit(
        &self,
        revision: u64,
        request: &EditRequest,
    ) -> Result<Result<(Document, u64), Refused>, ErrorView> {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        if held.revision != revision {
            return Err(stale(revision, held.revision));
        }
        match request.make(&held.document)? {
            Ok(edited) => {
                held.document = edited.clone();
                held.revision += 1;
                Ok(Ok((edited, held.revision)))
            }
            Err(error) => Ok(Err(Refused {
                revision: held.revision,
                error,
            })),
        }
    }
}

/// An edit the engine refused, and the revision the document stays at.
#[derive(Debug)]
struct Refused {
    revision: u64,
    error: EditError,
}

fn stale(asked: u64, current: u64) -> ErrorView {
    ErrorView::of(
        ErrorKind::Stale,
        format!("the document is at revision {current} now, not {asked}"),
    )
}

/// The document the host holds, its revision, and what running it reported.
pub fn document_state_view(documents: &Documents) -> Result<DocumentStateView, ErrorView> {
    let (document, revision) = documents.snapshot();
    state_of(&document, revision)
}

fn state_of(document: &Document, revision: u64) -> Result<DocumentStateView, ErrorView> {
    Ok(DocumentStateView {
        revision,
        document: document_view(document),
        run: crate::node::run_view(document)?,
    })
}

/// Makes `request` on the document at `revision` and reports it: the edited document at its new
/// revision with what running it reported, or why the engine refused.
///
/// # Errors
///
/// A command that fails is a `stale` revision, a request that names a place or a dtype the engine
/// has no word for, or a failure to run the document.
pub fn edit_view(
    documents: &Documents,
    revision: u64,
    request: &EditRequest,
) -> Result<EditAnswerView, ErrorView> {
    match documents.edit(revision, request)? {
        Ok((document, revision)) => Ok(EditAnswerView::Edited(state_of(&document, revision)?)),
        Err(Refused { revision, error }) => Ok(EditAnswerView::Refused {
            revision,
            place: place_view(error.place()),
            problem: problem_view(error.problem()),
            message: error.to_string(),
        }),
    }
}

#[cfg(test)]
pub(crate) mod tests;
