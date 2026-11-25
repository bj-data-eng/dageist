//! Commands that answer what the step editor may offer: which calls may stand
//! at a place, which values fit a slot, how a text typed into a slot reads,
//! what an entry's options may be set to, and what a new step starts with.
//!
//! The engine's tables of allowed combinations decide every answer, and the
//! engine reads every typed text; this module translates what the engine says
//! into serializable views and the view's requests into what the engine is
//! asked. The shapes mirror `OffersView`, `FitsView`, `ReadingsView` and
//! `OptionsView` in `ui/packages/host/src/contract.ts`, and `StepStartsView` for
//! the starts of new steps.
//!
//! A request names a place the way the engine's walk reports one: the
//! positions that hold the expression, outermost first, and what the
//! expression has so far. The positions are the words the catalogue gives each
//! expression parameter, so the view never works out which context a slot
//! stands in.

use dageist_engine::allowed::{
    self, After, ArgumentSlot, Candidate, ConditionState, Context, Literal, Nested, Outer, Place,
    Position, Representative, Shape, Slot, Typed,
};
use dageist_engine::catalog::{self, Applies, Entry, StepFrame, ValueType};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::report::{
    ConditionStateView, OfferPlaceView, RefusalView, ValueShapeView, VerdictView, offer_place_view,
    refusal_view, verdict_view,
};
use crate::sample_table::{ErrorKind, ErrorView};

/// Reads the position named by the word `name` out of `all`, by the word the
/// engine's tables give it.
fn named<'de, D: Deserializer<'de>, T: Copy>(
    deserializer: D,
    all: &[T],
    name_of: fn(T) -> &'static str,
    what: &str,
) -> Result<T, D::Error> {
    let name = String::deserialize(deserializer)?;
    all.iter()
        .copied()
        .find(|candidate| name_of(*candidate) == name)
        .ok_or_else(|| D::Error::custom(format!("{name:?} is not {what}")))
}

fn outer_named<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Outer, D::Error> {
    named(
        deserializer,
        &Outer::ALL,
        Outer::name,
        "a position of a step",
    )
}

fn nested_named<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Nested, D::Error> {
    named(
        deserializer,
        &Nested::ALL,
        Nested::name,
        "a position inside an expression",
    )
}

fn class_named<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Representative, D::Error> {
    let name = String::deserialize(deserializer)?;
    Representative::of_name(&name)
        .ok_or_else(|| D::Error::custom(format!("{name:?} is not a dtype the tables cover")))
}

/// A position that holds an expression: of a step, or inside an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum PositionRequest {
    /// A position of a step.
    Outer {
        #[serde(deserialize_with = "outer_named")]
        name: Outer,
    },
    /// A position inside an expression.
    Nested {
        #[serde(deserialize_with = "nested_named")]
        name: Nested,
    },
}

impl PositionRequest {
    const fn position(self) -> Position {
        match self {
            Self::Outer { name } => Position::Outer(name),
            Self::Nested { name } => Position::Nested(name),
        }
    }
}

/// The context the expression found by following `positions`, outermost first,
/// stands in.
fn context_of(positions: &[PositionRequest]) -> Result<Context, ErrorView> {
    let positions: Vec<Position> = positions
        .iter()
        .copied()
        .map(PositionRequest::position)
        .collect();
    Context::from_positions(&positions).ok_or_else(|| {
        ErrorView::of(
            ErrorKind::UnknownName,
            "an expression is found by a position of a step first",
        )
    })
}

/// What an expression has so far, as a request names it; the same words as
/// the `after` the engine's walk reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum AfterRequest {
    /// Nothing: a call would start the expression.
    Start,
    /// A value of a class of dtype, and a shape.
    Value {
        #[serde(deserialize_with = "class_named")]
        dtype: Representative,
        shape: ValueShapeView,
    },
    /// A condition that is not finished.
    Condition { state: ConditionStateView },
}

impl From<AfterRequest> for After {
    fn from(after: AfterRequest) -> Self {
        match after {
            AfterRequest::Start => Self::Start,
            AfterRequest::Value { dtype, shape } => Self::Value {
                dtype,
                shape: match shape {
                    ValueShapeView::One => Shape::One,
                    ValueShapeView::Many => Shape::Many,
                    ValueShapeView::Counted => Shape::Counted,
                },
            },
            AfterRequest::Condition { state } => Self::Condition(match state {
                ConditionStateView::When => ConditionState::When,
                ConditionStateView::Then => ConditionState::Then,
                ConditionStateView::ChainedWhen => ConditionState::ChainedWhen,
                ConditionStateView::ChainedThen => ConditionState::ChainedThen,
            }),
        }
    }
}

/// Where a call would stand: the positions that hold the expression, and what
/// it has so far.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlaceRequest {
    positions: Vec<PositionRequest>,
    after: AfterRequest,
}

impl PlaceRequest {
    pub(crate) fn place(&self) -> Result<Place, ErrorView> {
        Ok(Place::new(context_of(&self.positions)?, self.after.into()))
    }
}

/// A value that a list holds, that may be picked for a slot: the dtype of a field
/// or an input by the short name the engine reports.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum CandidateRequest {
    /// A column of the table the expression is read against.
    Field { dtype: String },
    /// A connected value or list.
    Input { dtype: String, list: bool },
}

/// The argument a candidate would be given: a parameter of a method that
/// stands at a place.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArgumentRequest {
    /// The method, by the identifier the catalogue gives it.
    entry: String,
    /// The parameter, by Python's name.
    parameter: String,
    /// Where the method stands.
    place: PlaceRequest,
}

/// One option of an entry that is already chosen.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChosenOptionRequest {
    /// The parameter, by Python's name.
    parameter: String,
    /// The word as the option table writes it: `true`, `false`, `unset`, or the
    /// word of a choice.
    word: String,
}

/// The type of literal a parameter takes, by the word the catalogue gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ValueTypeRequest {
    Integer,
    Float,
    String,
    Date,
    Any,
}

impl From<ValueTypeRequest> for ValueType {
    fn from(value_type: ValueTypeRequest) -> Self {
        match value_type {
            ValueTypeRequest::Integer => Self::Integer,
            ValueTypeRequest::Float => Self::Float,
            ValueTypeRequest::String => Self::String,
            ValueTypeRequest::Date => Self::Date,
            ValueTypeRequest::Any => Self::Any,
        }
    }
}

/// What a text is typed into: the kinds of parameter that take a typed value,
/// written as the catalogue writes the kind of a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum TypedRequest {
    /// An expression: a number or text stands in it.
    Expression,
    /// A literal of a type.
    Value {
        #[serde(rename = "type")]
        value_type: ValueTypeRequest,
    },
    /// A Polars duration string.
    Duration,
    /// A regular expression.
    Pattern,
    /// A time zone.
    TimeZone,
    /// A file, folder or cloud location.
    Path,
}

impl From<TypedRequest> for Typed {
    fn from(typed: TypedRequest) -> Self {
        match typed {
            TypedRequest::Expression => Self::Expression,
            TypedRequest::Value { value_type } => Self::Value(value_type.into()),
            TypedRequest::Duration => Self::Duration,
            TypedRequest::Pattern => Self::Pattern,
            TypedRequest::TimeZone => Self::TimeZone,
            TypedRequest::Path => Self::Path,
        }
    }
}

/// A number or text that a call is given, with the words the engine shows it as: what the view
/// puts in a block, since the view words no value of its own.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum LiteralView {
    Integer { value: i64, shown: String },
    Float { value: f64, shown: String },
    Text { value: String, shown: String },
}

fn literal_view(literal: &Literal) -> LiteralView {
    let shown = literal.shown();
    match literal {
        Literal::Integer(value) => LiteralView::Integer {
            value: *value,
            shown,
        },
        Literal::Float(value) => LiteralView::Float {
            value: *value,
            shown,
        },
        Literal::String(value) => LiteralView::Text {
            value: value.clone(),
            shown,
        },
    }
}

/// What a method is given for its first parameter when it is added, when the
/// tables allow Polars' default there.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct DefaultArgumentView {
    parameter: &'static str,
    literal: LiteralView,
}

/// An entry and whether it may stand at the place.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntryOfferView {
    entry: &'static str,
    verdict: VerdictView,
    /// What a method is given when it is added: only for a method that may
    /// stand, and has a default that the tables allow as its argument.
    argument: Option<DefaultArgumentView>,
    /// The first parameter a method that may stand starts without giving, which holds what
    /// Polars gives it when it is not given: the method is complete as it starts.
    left_out: Option<&'static str>,
    /// The parameters a method that may stand starts with as slots to fill, besides its argument:
    /// those a step must give and Polars gives no default.
    slots: Vec<&'static str>,
}

/// Every call that stands in an expression, offered or refused at a place.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct OffersView {
    /// The place that was asked about, as the engine took it.
    place: OfferPlaceView,
    /// One for each entry that stands in an expression, ordered by identifier.
    entries: Vec<EntryOfferView>,
}

/// What the tables say of a candidate in a slot.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FitView {
    /// The candidate has this class of dtype, and the tables judged it.
    #[serde(rename_all = "camelCase")]
    Judged {
        dtype: &'static str,
        verdict: VerdictView,
    },
    /// The candidate has no class of dtype the tables cover, so they say
    /// nothing of it.
    Unjudged { reason: String },
}

/// What the tables say of each candidate, in the order they were asked.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct FitsView {
    fits: Vec<FitView>,
}

/// One way a typed text reads.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ReadingView {
    /// The text reads as this literal, which has this dtype where Polars gives it one, and
    /// what the tables say of it where a slot was given. The slot judges the literal the
    /// document would hold, which the document writes as text where it holds no date, so a
    /// date reads with the dtype `Date` and is judged as the `String` that carries it.
    Read {
        literal: LiteralView,
        dtype: Option<&'static str>,
        fit: Option<FitView>,
    },
    /// The text is not what it was typed into, for this reason.
    Unread { reason: String },
}

/// How a text typed into a slot reads, each way in the order the slot offers it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ReadingsView {
    readings: Vec<ReadingView>,
}

/// One word an option may be set to, and why Polars refuses it, if it does.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct OptionWordView {
    word: String,
    refusal: Option<RefusalView>,
}

/// An option of an entry: its default, and the words it may be set to.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct OptionView {
    parameter: &'static str,
    /// The word the option has when it is not set.
    default: Option<String>,
    /// The word the option starts with when it is added: the first that is not
    /// the default and that Polars allows beside the options chosen; `None`
    /// where there is none.
    starter: Option<String>,
    /// Why the option cannot be added, where it has no starter.
    stuck: Option<RefusalView>,
    words: Vec<OptionWordView>,
}

/// The flags and choices of an entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct OptionsView {
    options: Vec<OptionView>,
}

/// An entry of the list that starts a step.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct StepStartView {
    entry: &'static str,
}

/// Which steps of the list can be added to a table.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct StepStartsView {
    /// One for each step that can, ordered by identifier. What a new step holds is the engine's to
    /// say when it is added, as an edit, so this says only that it can be.
    starts: Vec<StepStartView>,
}

fn entry_named(id: &str) -> Result<&'static Entry, ErrorView> {
    catalog::entry(id).ok_or_else(|| {
        ErrorView::of(
            ErrorKind::UnknownName,
            format!("the catalogue has no entry {id}"),
        )
    })
}

/// Every call that stands in an expression, offered or refused where an
/// expression found by following `positions` has `after`.
pub fn offers_view(
    positions: &[PositionRequest],
    after: AfterRequest,
) -> Result<OffersView, ErrorView> {
    let place = Place::new(context_of(positions)?, after.into());
    let entries = allowed::offers(&place)
        .into_iter()
        .map(|offer| {
            let entry = offer.entry();
            let argument = offer
                .verdict()
                .is_allowed()
                .then(|| allowed::default_argument(entry, &place))
                .flatten()
                .map(|default| DefaultArgumentView {
                    parameter: default.parameter(),
                    literal: literal_view(default.literal()),
                });
            let left_out = offer
                .verdict()
                .is_allowed()
                .then(|| allowed::left_out_at_start(entry))
                .flatten();
            let slots = if offer.verdict().is_allowed() {
                allowed::slots_at_start(entry)
            } else {
                Vec::new()
            };
            EntryOfferView {
                entry: entry.id().as_str(),
                verdict: verdict_view(offer.verdict()),
                argument,
                left_out,
                slots,
            }
        })
        .collect();
    Ok(OffersView {
        place: offer_place_view(&place),
        entries,
    })
}

/// The candidate a request names, or what the engine cannot judge of it.
fn candidate_of(request: &CandidateRequest) -> Result<Candidate, String> {
    let class = |name: &str| {
        Representative::of_name(name)
            .ok_or_else(|| format!("the tables cover no class of the dtype {name}"))
    };
    Ok(match request {
        CandidateRequest::Field { dtype } => Candidate::Field(class(dtype)?),
        CandidateRequest::Input { dtype, list } => Candidate::Input {
            dtype: class(dtype)?,
            shape: if *list { Shape::Counted } else { Shape::One },
        },
    })
}

/// The slot that an expression found by following `start` and the argument `argument` names
/// stand in, or `None` where neither is given.
fn slot_of(
    start: Option<&[PositionRequest]>,
    argument: Option<&ArgumentRequest>,
) -> Result<Option<Slot<'static>>, ErrorView> {
    let context = start.map(context_of).transpose()?;
    let place = argument
        .map(|argument| argument.place.place())
        .transpose()?;
    Ok(match (argument, place) {
        (Some(argument), Some(place)) => {
            let entry = entry_named(&argument.entry)?;
            let parameter = entry
                .parameters()
                .iter()
                .map(catalog::Parameter::name)
                .find(|name| *name == argument.parameter)
                .ok_or_else(|| {
                    ErrorView::of(
                        ErrorKind::UnknownName,
                        format!("{} has no parameter {}", argument.entry, argument.parameter),
                    )
                })?;
            Slot::new(context, Some(ArgumentSlot::new(entry, parameter, place)))
        }
        _ => Slot::new(context, None),
    })
}

/// What the tables say of `candidate` in `slot`.
fn judged(candidate: &Candidate, slot: &Slot<'_>) -> FitView {
    match allowed::fit(candidate, slot) {
        Some(found) => FitView::Judged {
            dtype: found.dtype().name(),
            verdict: verdict_view(found.verdict()),
        },
        None => FitView::Unjudged {
            reason: "Polars gives the literal no dtype the tables cover".to_owned(),
        },
    }
}

/// What the tables say of each of `candidates` in the slot: as an expression
/// found by following `start`, and as the argument `argument` names, when they
/// are given.
pub fn fits_view(
    start: Option<&[PositionRequest]>,
    argument: Option<&ArgumentRequest>,
    candidates: &[CandidateRequest],
) -> Result<FitsView, ErrorView> {
    let slot = slot_of(start, argument)?.ok_or_else(|| {
        ErrorView::of(
            ErrorKind::UnknownName,
            "a slot is a position of an expression, an argument of a method, or both",
        )
    })?;
    let fits = candidates
        .iter()
        .map(|request| match candidate_of(request) {
            Err(reason) => FitView::Unjudged { reason },
            Ok(candidate) => judged(&candidate, &slot),
        })
        .collect();
    Ok(FitsView { fits })
}

/// How `text` reads typed into `typed`, each way the engine reads it, and for each that reads,
/// what the tables say of it in the slot an expression found by following `start` and the
/// argument `argument` name, when either is given.
pub fn read_view(
    text: &str,
    typed: TypedRequest,
    start: Option<&[PositionRequest]>,
    argument: Option<&ArgumentRequest>,
) -> Result<ReadingsView, ErrorView> {
    let slot = slot_of(start, argument)?;
    let readings = allowed::read_typed(text, typed.into())
        .into_iter()
        .map(|reading| match reading {
            Ok(read) => ReadingView::Read {
                literal: literal_view(read.literal()),
                dtype: read.dtype().map(Representative::name),
                fit: slot
                    .as_ref()
                    .map(|slot| judged(&Candidate::Literal(read.literal().clone()), slot)),
            },
            Err(unread) => ReadingView::Unread {
                reason: unread.to_string(),
            },
        })
        .collect();
    Ok(ReadingsView { readings })
}

/// The flags and choices of the entry `entry`, each with its default and what
/// Polars says of every word it may be set to, beside the options already
/// `chosen`.
pub fn options_view(entry: &str, chosen: &[ChosenOptionRequest]) -> Result<OptionsView, ErrorView> {
    let entry = entry_named(entry)?;
    for option in chosen {
        if !entry
            .parameters()
            .iter()
            .any(|parameter| parameter.name() == option.parameter)
        {
            return Err(ErrorView::of(
                ErrorKind::UnknownName,
                format!(
                    "{} has no parameter {}",
                    entry.id().as_str(),
                    option.parameter
                ),
            ));
        }
    }
    let chosen: Vec<(&str, &str)> = chosen
        .iter()
        .map(|option| (option.parameter.as_str(), option.word.as_str()))
        .collect();
    let options = allowed::option_offers(entry, &chosen)
        .into_iter()
        .map(|option| OptionView {
            parameter: option.parameter(),
            default: option.default_word().map(str::to_owned),
            starter: option.starter().map(str::to_owned),
            stuck: option.stuck().as_ref().map(refusal_view),
            words: option
                .words()
                .iter()
                .map(|word| OptionWordView {
                    word: word.word().to_owned(),
                    refusal: word.refusal().map(refusal_view),
                })
                .collect(),
        })
        .collect();
    Ok(OptionsView { options })
}

/// Which steps of the list can be added to a table of `columns`, with the connected `tables` it
/// can bring in. The engine decides it.
pub fn step_starts_view(columns: &[String], tables: &[String]) -> StepStartsView {
    let frame = StepFrame { columns, tables };
    let starts = catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == Applies::Frame)
        .filter(|entry| catalog::start_step(entry, &frame).is_some())
        .map(|entry| StepStartView {
            entry: entry.id().as_str(),
        })
        .collect();
    StepStartsView { starts }
}

/// Answers which calls may stand where an expression found by following
/// `positions`, outermost first, has `after`.
#[tauri::command(async)]
pub fn offers_at(
    positions: Vec<PositionRequest>,
    after: AfterRequest,
) -> Result<OffersView, ErrorView> {
    offers_view(&positions, after)
}

/// Answers which of `candidates` fit a slot: an expression that starts where
/// `start` says, the argument `argument` names, or both.
#[tauri::command(async)]
pub fn candidate_fits(
    start: Option<Vec<PositionRequest>>,
    argument: Option<ArgumentRequest>,
    candidates: Vec<CandidateRequest>,
) -> Result<FitsView, ErrorView> {
    fits_view(start.as_deref(), argument.as_ref(), &candidates)
}

/// Answers how `text` reads typed into `typed`, and where a slot is given, whether each reading
/// fits it: an expression that starts where `start` says, the argument `argument` names, or both.
#[tauri::command(async)]
pub fn read_typed(
    text: String,
    typed: TypedRequest,
    start: Option<Vec<PositionRequest>>,
    argument: Option<ArgumentRequest>,
) -> Result<ReadingsView, ErrorView> {
    read_view(&text, typed, start.as_deref(), argument.as_ref())
}

/// Answers what the flags and choices of `entry` may be set to, beside the
/// options already `chosen`.
#[tauri::command(async)]
pub fn option_offers(
    entry: String,
    chosen: Vec<ChosenOptionRequest>,
) -> Result<OptionsView, ErrorView> {
    options_view(&entry, &chosen)
}

/// Answers what each step of the list starts with when it is added to a table of `columns`,
/// with the connected `tables` it can bring in.
#[tauri::command(async)]
pub fn step_starts(columns: Vec<String>, tables: Vec<String>) -> StepStartsView {
    step_starts_view(&columns, &tables)
}

#[cfg(test)]
mod tests;
