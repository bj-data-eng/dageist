//! What running a step document reported, as the view receives it.
//!
//! The engine runs the document and reports each step: whether it is valid
//! and the table it gives, the dtype after each link of each expression, and
//! what the tables of allowed combinations say of every call. This module
//! translates that into one serializable value. It mirrors `RunView` and the
//! types beside it in `ui/packages/host/src/contract.ts`; the view decodes it
//! with the decoders next to the contract.
//!
//! Every name that stands for something the engine knows, such as a context,
//! a rule or a stage, is the word the engine's tables use. Dtypes are the
//! engine's short names, such as `Datetime(us)`.

use dageist_engine::allowed::{
    After, ConditionState, Context, Ending, Nested, Outer, Place as OfferPlace, Refusal,
    RefusalSource, Shape, Stage, Verdict, dtype_name,
};
use dageist_engine::catalog::Laziness;
use dageist_engine::document::{
    Change, ColumnChange, ColumnDescription, ColumnValues, EmptyStepId, ExpressionDtypes,
    FirstValues, Outcome, Place, Report, Resolution, RunCause, Segment, StepId, StepReport,
    StepRows, Walk, WalkRefusal, WalkedExpression,
};
use serde::{Deserialize, Serialize};

/// A column and the short name of its dtype.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldView {
    name: String,
    dtype: String,
}

impl FieldView {
    /// The fields of a table, in its order.
    pub fn of_columns(columns: &[ColumnDescription]) -> Vec<Self> {
        columns
            .iter()
            .map(|column| Self {
                name: column.name().to_owned(),
                dtype: column.dtype().to_owned(),
            })
            .collect()
    }
}

/// The first values of a column or a list, each as the text Polars writes it
/// and a null as `null`, and how many more follow them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValuesView {
    shown: Vec<Option<String>>,
    more: usize,
}

impl ValuesView {
    /// The view of `values`.
    pub fn of(values: &FirstValues) -> Self {
        Self {
            shown: values.shown().to_vec(),
            more: values.more(),
        }
    }
}

/// How a step changed a column of the table it was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ChangeView {
    /// The table the step gives has the column and the one it was given did
    /// not.
    Added,
    /// Both have the column, and the step wrote it or gave it another dtype.
    Replaced,
}

const fn change_view(change: Change) -> ChangeView {
    match change {
        Change::Added => ChangeView::Added,
        Change::Replaced => ChangeView::Replaced,
    }
}

/// A column a step added or replaced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ColumnChangeView {
    name: String,
    change: ChangeView,
}

fn column_change_view(change: &ColumnChange) -> ColumnChangeView {
    ColumnChangeView {
        name: change.name().to_owned(),
        change: change_view(change.change()),
    }
}

/// The first values of one column a step changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ColumnValuesView {
    name: String,
    values: ValuesView,
}

fn column_values_view(column: &ColumnValues) -> ColumnValuesView {
    ColumnValuesView {
        name: column.name().to_owned(),
        values: ValuesView::of(column.values()),
    }
}

/// What running a step on the rows of the working window came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum CountView {
    /// The step was not run on rows: the run read none, or a step before it
    /// was refused, incomplete or failed on the rows.
    NotCounted,
    /// The step was given `received` rows and kept `kept`.
    Counted {
        received: usize,
        kept: usize,
        /// The first values of each column the step added or replaced.
        first: Vec<ColumnValuesView>,
    },
    /// Polars accepted the step and could not run it on the rows, for this
    /// reason.
    Failed { reason: String },
}

fn count_view(rows: &StepRows) -> CountView {
    match rows {
        StepRows::NotCounted => CountView::NotCounted,
        StepRows::Counted(count) => CountView::Counted {
            received: count.received(),
            kept: count.kept(),
            first: count.first().iter().map(column_values_view).collect(),
        },
        StepRows::Failed(error) => CountView::Failed {
            reason: error.to_string(),
        },
    }
}

/// One step of the way from a step to the thing inside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum SegmentView {
    /// The argument for a parameter, by Python's name for it.
    Parameter { name: String },
    /// An item of a list.
    Item { index: usize },
    /// The key of an entry of pairs.
    Key { index: usize },
    /// The value of an entry of pairs.
    Value { index: usize },
    /// The start of an expression.
    Start,
    /// A link of an expression, counted from the first after its start.
    Link { index: usize },
}

fn segment_view(segment: &Segment) -> SegmentView {
    match segment {
        Segment::Parameter(name) => SegmentView::Parameter { name: name.clone() },
        Segment::Item(index) => SegmentView::Item { index: *index },
        Segment::Key(index) => SegmentView::Key { index: *index },
        Segment::Value(index) => SegmentView::Value { index: *index },
        Segment::Start => SegmentView::Start,
        Segment::Link(index) => SegmentView::Link { index: *index },
    }
}

/// A place in a document: a step, then the way down through its arguments.
/// Both are absent for a place in the document outside its steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceView {
    step_index: Option<usize>,
    step_id: Option<String>,
    path: Vec<SegmentView>,
}

/// A place an edit is asked at: a step by its number and its identifier, then the way down
/// through its arguments, as the report gives a place. An edit works inside a step, so a request
/// that names no step, or names one without its identifier, is not a request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EditPlace {
    step_index: usize,
    step_id: String,
    path: Vec<SegmentView>,
}

impl EditPlace {
    /// The place this view names.
    ///
    /// # Errors
    ///
    /// When it names a step by an empty identifier.
    pub(crate) fn place(&self) -> Result<Place, EmptyStepId> {
        let id = StepId::new(&self.step_id)?;
        let place = Place::of_step(self.step_index, Some(id));
        Ok(self.path.iter().fold(place, |place, segment| {
            place.then(match segment {
                SegmentView::Parameter { name } => Segment::Parameter(name.clone()),
                SegmentView::Item { index } => Segment::Item(*index),
                SegmentView::Key { index } => Segment::Key(*index),
                SegmentView::Value { index } => Segment::Value(*index),
                SegmentView::Start => Segment::Start,
                SegmentView::Link { index } => Segment::Link(*index),
            })
        }))
    }
}

pub(crate) fn place_view(place: &Place) -> PlaceView {
    PlaceView {
        step_index: place.step_index(),
        step_id: place.step_id().map(|id| id.as_str().to_owned()),
        path: place.path().iter().map(segment_view).collect(),
    }
}

/// What an expression has, up to one of its links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ResolutionView {
    /// A single column of this dtype.
    Dtype { dtype: String },
    /// More than one column, with their dtypes.
    Several { dtypes: Vec<String> },
    /// A condition that still lacks its `then`.
    Unfinished,
    /// An expression that starts from `element`, which has no dtype outside
    /// the context that evaluates it.
    NoContext,
    /// Polars refuses the expression up to here, with this reason.
    Refused { reason: String },
}

fn resolution_view(resolution: &Resolution) -> ResolutionView {
    match resolution {
        Resolution::Dtype(dtype) => ResolutionView::Dtype {
            dtype: dtype_name(dtype),
        },
        Resolution::Several(dtypes) => ResolutionView::Several {
            dtypes: dtypes.iter().map(dtype_name).collect(),
        },
        Resolution::Unfinished => ResolutionView::Unfinished,
        Resolution::NoContext => ResolutionView::NoContext,
        Resolution::Refused(reason) => ResolutionView::Refused {
            reason: reason.to_string(),
        },
    }
}

/// The dtype of an expression at its start and after each of its links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ExpressionDtypesView {
    place: PlaceView,
    start: ResolutionView,
    links: Vec<ResolutionView>,
}

fn expression_dtypes_view(expression: &ExpressionDtypes) -> ExpressionDtypesView {
    ExpressionDtypesView {
        place: place_view(expression.place()),
        start: resolution_view(expression.start()),
        links: expression.links().iter().map(resolution_view).collect(),
    }
}

/// How many values a chain gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ValueShapeView {
    One,
    Many,
    /// Another number of values than one or the table's rows, which the data or the arguments
    /// decide.
    Counted,
}

const fn shape_view(shape: Shape) -> ValueShapeView {
    match shape {
        Shape::One => ValueShapeView::One,
        Shape::Many => ValueShapeView::Many,
        Shape::Counted => ValueShapeView::Counted,
    }
}

/// How far a condition has come: the call that was last made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ConditionStateView {
    When,
    Then,
    ChainedWhen,
    ChainedThen,
}

const fn condition_state_view(state: ConditionState) -> ConditionStateView {
    match state {
        ConditionState::When => ConditionStateView::When,
        ConditionState::Then => ConditionStateView::Then,
        ConditionState::ChainedWhen => ConditionStateView::ChainedWhen,
        ConditionState::ChainedThen => ConditionStateView::ChainedThen,
    }
}

/// What an expression has so far, where a call would follow it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum AfterView {
    /// Nothing: the call starts an expression.
    Start,
    /// A value of a dtype class, which an expression method follows.
    Value {
        /// The class of the dtype, as the tables name it.
        dtype: &'static str,
        shape: ValueShapeView,
    },
    /// A condition that is not finished.
    Condition { state: ConditionStateView },
}

pub(crate) const fn after_view(after: After) -> AfterView {
    match after {
        After::Start => AfterView::Start,
        After::Value { dtype, shape } => AfterView::Value {
            dtype: dtype.name(),
            shape: shape_view(shape),
        },
        After::Condition(state) => AfterView::Condition {
            state: condition_state_view(state),
        },
    }
}

/// Where an expression stands: the position of a step that holds it, and when
/// it is inside another expression, the position in that one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ContextView {
    /// The position of the step, as the tables name it.
    outer: &'static str,
    /// The position inside the expression, as the tables name it.
    nested: Option<&'static str>,
}

pub(crate) fn context_view(context: Context) -> ContextView {
    ContextView {
        outer: Outer::name(context.outer()),
        nested: context.nested().map(Nested::name),
    }
}

/// Where a call would stand: in which context, and after what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OfferPlaceView {
    context: ContextView,
    after: AfterView,
}

pub(crate) fn offer_place_view(place: &OfferPlace) -> OfferPlaceView {
    OfferPlaceView {
        context: context_view(place.context()),
        after: after_view(place.after()),
    }
}

/// Where Polars refused a combination, as the tables record it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum StageView {
    Schema,
    Rows,
    Empty,
}

const fn stage_view(stage: Stage) -> StageView {
    match stage {
        Stage::Schema => StageView::Schema,
        Stage::Rows => StageView::Rows,
        Stage::Empty => StageView::Empty,
    }
}

/// Who refused a call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "by", rename_all = "camelCase")]
pub(crate) enum RefusalSourceView {
    /// Polars, as the tables recorded it, at this stage.
    Polars { stage: StageView },
    /// A rule Polars does not hold, by the name the tables give it.
    Rule { rule: &'static str },
    /// The shape of an expression.
    Structure,
}

/// Why a call may not stand somewhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RefusalView {
    source: RefusalSourceView,
    reason: String,
}

pub(crate) fn refusal_view(refusal: &Refusal) -> RefusalView {
    RefusalView {
        source: match refusal.source() {
            RefusalSource::Polars(stage) => RefusalSourceView::Polars {
                stage: stage_view(stage),
            },
            RefusalSource::Rule(rule) => RefusalSourceView::Rule { rule: rule.name() },
            RefusalSource::Structure => RefusalSourceView::Structure,
        },
        reason: refusal.reason().to_owned(),
    }
}

/// Whether an expression may end where it does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum EndingView {
    Allowed,
    Refused {
        refusal: RefusalView,
    },
    /// The tables do not say, for this reason.
    NotKnown {
        because: String,
    },
}

fn ending_view(ending: &Ending) -> EndingView {
    match ending {
        Ending::Allowed => EndingView::Allowed,
        Ending::Refused(refusal) => EndingView::Refused {
            refusal: refusal_view(refusal),
        },
        Ending::NotKnown(unknown) => EndingView::NotKnown {
            because: unknown.to_string(),
        },
    }
}

/// Whether a call may stand where it does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum VerdictView {
    /// It may, and gives this.
    Allowed {
        /// The dtype the chain has after it, where the call decides it.
        result: Option<String>,
        shape: ValueShapeView,
        ending: EndingView,
    },
    /// It may not.
    Refused { refusal: RefusalView },
}

pub(crate) fn verdict_view(verdict: &Verdict) -> VerdictView {
    match verdict {
        Verdict::Allowed(allowance) => VerdictView::Allowed {
            result: allowance.result().map(|result| result.name().to_owned()),
            shape: shape_view(allowance.shape()),
            ending: ending_view(allowance.can_end()),
        },
        Verdict::Refused(refusal) => VerdictView::Refused {
            refusal: refusal_view(refusal),
        },
    }
}

/// A call of an expression: what it follows, and whether it may stand there.
/// Both are absent where the tables were not asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct StandingView {
    follows: Option<AfterView>,
    verdict: Option<VerdictView>,
}

/// An expression of a step, as the tables see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WalkedExpressionView {
    place: PlaceView,
    context: Option<ContextView>,
    /// The start, then each link.
    calls: Vec<StandingView>,
    /// The place of each link, and after them the place a link added at the
    /// end would have; absent where the context or what comes before is not
    /// known.
    places: Vec<Option<OfferPlaceView>>,
    ending: Option<EndingView>,
}

fn walked_expression_view(expression: &WalkedExpression) -> WalkedExpressionView {
    WalkedExpressionView {
        place: place_view(expression.place()),
        context: expression.context().map(context_view),
        calls: expression
            .calls()
            .iter()
            .map(|standing| StandingView {
                follows: standing.follows().map(after_view),
                verdict: standing.verdict().map(verdict_view),
            })
            .collect(),
        places: expression
            .places()
            .iter()
            .map(|place| place.as_ref().map(offer_place_view))
            .collect(),
        ending: expression.ending().map(ending_view),
    }
}

/// A refusal the tables found, and where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct WalkRefusalView {
    place: PlaceView,
    refusal: RefusalView,
    /// Whether the expression there may not end where it does, and not that a
    /// call may not stand.
    at_end: bool,
}

fn walk_refusal_view(refusal: &WalkRefusal) -> WalkRefusalView {
    WalkRefusalView {
        place: place_view(refusal.place()),
        refusal: refusal_view(refusal.refusal()),
        at_end: refusal.at_end(),
    }
}

/// What the tables of allowed combinations say of a step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct WalkView {
    expressions: Vec<WalkedExpressionView>,
    refusal: Option<WalkRefusalView>,
    /// Why the tables could not be asked past a point, where they could not.
    not_known: Option<String>,
}

fn walk_view(walk: &Walk) -> WalkView {
    WalkView {
        expressions: walk
            .expressions()
            .iter()
            .map(walked_expression_view)
            .collect(),
        refusal: walk.refusal().map(walk_refusal_view),
        not_known: walk.not_known().map(str::to_owned),
    }
}

/// Why a step was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum CauseView {
    /// The step names a table that is not connected.
    UnknownTable { name: String },
    /// An expression starts from an input that is not a connected value or
    /// list.
    UnknownInput { name: String },
    /// The catalogue could not build the call.
    Build { reason: String },
    /// A named rule does not let a call stand where it does.
    NotAllowed { refusal: RefusalView },
    /// Polars refused, with this reason.
    Polars { reason: String },
}

fn cause_view(cause: &RunCause) -> CauseView {
    match cause {
        RunCause::UnknownTable(name) => CauseView::UnknownTable { name: name.clone() },
        RunCause::UnknownInput(name) => CauseView::UnknownInput { name: name.clone() },
        RunCause::Build(error) => CauseView::Build {
            reason: error.to_string(),
        },
        RunCause::NotAllowed(refusal) => CauseView::NotAllowed {
            refusal: refusal_view(refusal),
        },
        RunCause::Polars(error) => CauseView::Polars {
            reason: error.to_string(),
        },
    }
}

/// What running one step came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
enum OutcomeView {
    /// Polars accepts the step, and its table has these columns, of which the
    /// step added or replaced these.
    Valid {
        columns: Vec<FieldView>,
        changes: Vec<ColumnChangeView>,
    },
    /// The step was refused, here, for this reason.
    #[serde(rename_all = "camelCase")]
    Refused { place: PlaceView, cause: CauseView },
    /// The step holds something still to be picked, here, so it was walked up
    /// to it and not built.
    Incomplete { place: PlaceView },
    /// A step before it was refused or incomplete, so it was not run.
    #[serde(rename_all = "camelCase")]
    NotRun { blocked_by: String },
}

fn outcome_view(outcome: &Outcome) -> OutcomeView {
    match outcome {
        Outcome::Valid { schema, changes } => OutcomeView::Valid {
            columns: FieldView::of_columns(&ColumnDescription::of_schema(schema)),
            changes: changes.iter().map(column_change_view).collect(),
        },
        Outcome::Refused(refusal) => OutcomeView::Refused {
            place: place_view(refusal.place()),
            cause: cause_view(refusal.cause()),
        },
        Outcome::Incomplete { place } => OutcomeView::Incomplete {
            place: place_view(place),
        },
        Outcome::NotRun { blocked_by } => OutcomeView::NotRun {
            blocked_by: blocked_by.as_str().to_owned(),
        },
    }
}

/// What running one step reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepReportView {
    id: String,
    /// What the step works on, in a few words.
    summary: String,
    /// Whether the step stays in a lazy plan; a step that does not collected
    /// the data it needs when it was built.
    lazy: bool,
    outcome: OutcomeView,
    /// How many rows the step kept of those it received on the working window,
    /// with the first values of what it changed, where the rows were run.
    count: CountView,
    /// The dtypes of every expression in the step's arguments, as far as they
    /// could be built.
    expressions: Vec<ExpressionDtypesView>,
    walk: WalkView,
    /// The places of the step's arguments that are what their parameter holds
    /// when it is not given, whether or not the step ran.
    at_default: Vec<PlaceView>,
}

fn step_report_view(step: &StepReport) -> StepReportView {
    StepReportView {
        id: step.id().as_str().to_owned(),
        summary: step.summary().to_owned(),
        lazy: matches!(step.laziness(), Laziness::Lazy),
        outcome: outcome_view(step.outcome()),
        count: count_view(step.rows()),
        expressions: step
            .expressions()
            .iter()
            .map(expression_dtypes_view)
            .collect(),
        walk: walk_view(step.walk()),
        at_default: step.at_default().iter().map(place_view).collect(),
    }
}

/// What running a document came to: its steps reported one by one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunView {
    /// The columns of the table the steps start from.
    source: Vec<FieldView>,
    /// One report for each step, in the document's order.
    steps: Vec<StepReportView>,
}

impl RunView {
    /// The view of what running a document reported.
    pub fn of_report(report: &Report) -> Self {
        Self {
            source: FieldView::of_columns(&ColumnDescription::of_schema(report.source())),
            steps: report.steps().iter().map(step_report_view).collect(),
        }
    }
}

#[cfg(test)]
pub(crate) mod fixtures;
