//! Making documents by the engine's own edits, as the editor makes them: a step with a place
//! that holds an expression, the start of a chain and its methods, and what the arguments of a
//! method start with.

use dageist_engine::allowed::{
    Context, Nested, Outer, Place as OfferPlace, Position, Representative,
};
use dageist_engine::catalog::{self, Shape as ParameterShape};
use dageist_engine::document::{
    Call, Document, Expression, Place, Segment, Start, Step, StepId, Value,
};

use super::arguments::{settle, with_required};
use super::source::{ON_COLUMNS, OTHER, column_of, right_column_of};

/// A document with one place that holds an expression with nothing picked yet.
#[derive(Debug, Clone)]
pub(super) struct Hosted {
    pub document: Document,
    /// Where the expression is.
    pub at: Place,
}

/// The entry `id` of the catalogue.
pub(super) fn entry(id: &str) -> &'static catalog::Entry {
    catalog::entry(id).unwrap_or_else(|| panic!("the catalogue has no entry {id}"))
}

/// The call of `id` given `given`.
pub(super) fn call(id: &str, given: Vec<(&str, Value)>) -> Call {
    Call::new(entry(id), given).unwrap_or_else(|refusal| {
        panic!("the corpus built a call of {id} the document refuses: {refusal}")
    })
}

/// The expression that reads the column `name`, as an argument.
pub(super) fn field(name: &str) -> Value {
    Value::Expression(Expression::field(name).expect("a field"))
}

/// An expression with nothing picked, as an argument.
pub(super) fn hole() -> Value {
    Value::Expression(Expression::hole())
}

/// The name a step of `id` has in the document: the entry's own name.
fn step_id(id: &str) -> StepId {
    let name = id.rsplit('.').next().unwrap_or(id);
    StepId::new(name).expect("an identifier")
}

/// A document of one step that makes `id` with `given`.
pub(super) fn one_step(id: &str, given: Vec<(&str, Value)>) -> Document {
    let step = Step::new(step_id(id), call(id, given)).expect("a step of a lazy frame");
    Document::new(vec![step]).expect("one step")
}

/// The place of the only step of a document made by [`one_step`], of `id`.
pub(super) fn the_step(id: &str) -> Place {
    Place::of_step(0, Some(step_id(id)))
}

/// What the corpus reads a column of `class` by in `outer`: a join's right keys read the other
/// table.
pub(super) fn field_name(outer: Outer, class: Representative) -> String {
    if outer == Outer::JoinRight {
        right_column_of(class)
    } else {
        column_of(class)
    }
}

/// The document that holds an expression with nothing picked in the position `outer` of a step.
/// `values` is the class of the column a pivot groups, which is what its aggregate is given.
pub(super) fn host(outer: Outer, values: Representative) -> Hosted {
    let text = column_of(Representative::String);
    let whole = column_of(Representative::Int64);
    let one = |name: &str| Value::Many(vec![field(name)]);
    match outer {
        Outer::Select => hosted_in_list("LazyFrame.select", "exprs", vec![]),
        Outer::Filter => hosted_in_list("LazyFrame.filter", "predicates", vec![]),
        Outer::Sort => hosted_in_list("LazyFrame.sort", "by", vec![]),
        Outer::JoinLeft => hosted_in_list(
            "LazyFrame.join",
            "left_on",
            vec![
                ("other", Value::Table(OTHER.to_owned())),
                ("right_on", one(&right_column_of(Representative::String))),
            ],
        ),
        Outer::JoinRight => hosted_in_list(
            "LazyFrame.join",
            "right_on",
            vec![
                ("other", Value::Table(OTHER.to_owned())),
                ("left_on", one(&text)),
            ],
        ),
        Outer::PivotAggregate => {
            let given = vec![
                ("on", Value::Columns(vec![text])),
                ("on_columns", Value::Table(ON_COLUMNS.to_owned())),
                ("index", Value::Columns(vec![whole])),
                ("values", Value::Columns(vec![column_of(values)])),
                ("aggregate_function", hole()),
            ];
            let document = one_step("LazyFrame.pivot", given);
            let at = the_step("LazyFrame.pivot")
                .then(Segment::Parameter("aggregate_function".to_owned()));
            Hosted { document, at }
        }
    }
}

/// A step `id` whose parameter `parameter` is a list of one expression with nothing picked, given
/// `others` as well.
fn hosted_in_list(id: &str, parameter: &str, others: Vec<(&str, Value)>) -> Hosted {
    let mut given = others;
    given.push((parameter, Value::Many(vec![hole()])));
    let document = one_step(id, given);
    let at = the_step(id)
        .then(Segment::Parameter(parameter.to_owned()))
        .then(Segment::Item(0));
    Hosted { document, at }
}

/// The document that holds an expression with nothing picked in `context`: in the position of a
/// step it stands in, and inside the method or the condition of another expression where the
/// context has a nested position.
pub(super) fn host_in(context: Context, values: Representative) -> Hosted {
    let outer = host(context.outer(), values);
    let Some(nested) = context.nested() else {
        return outer;
    };
    // The expression that holds the nested one starts from what the outer position starts from.
    let base = start_for(context.outer(), values);
    let Hosted { document, at } = outer;
    match nested {
        Nested::PartitionBy | Nested::OrderBy => {
            let (id, parameter) = nested.host();
            let document = with_start(&document, &at, &base);
            let document = document
                .insert_link(&at, 0, id, None)
                .unwrap_or_else(|error| panic!("{context}: over is not added: {error}"));
            let link = at.clone().then(Segment::Link(0));
            let parameter_place = link.then(Segment::Parameter(parameter.to_owned()));
            let shape = entry(id)
                .parameters()
                .iter()
                .find(|held| held.name() == parameter)
                .map(catalog::Parameter::shape)
                .expect("over has the parameter");
            let (document, inside) = if shape == ParameterShape::Many {
                (
                    document
                        .set_argument(&parameter_place, Value::Many(vec![hole()]))
                        .unwrap_or_else(|error| panic!("{context}: {error}")),
                    parameter_place.then(Segment::Item(0)),
                )
            } else {
                (
                    document
                        .set_argument(&parameter_place, hole())
                        .unwrap_or_else(|error| panic!("{context}: {error}")),
                    parameter_place,
                )
            };
            Hosted {
                document,
                at: inside,
            }
        }
        Nested::ConditionPredicate => {
            let document = document
                .start_with(&at, "polars.when")
                .unwrap_or_else(|error| panic!("{context}: when does not start: {error}"));
            let inside = at
                .then(Segment::Start)
                .then(Segment::Parameter("predicates".to_owned()))
                .then(Segment::Item(0));
            let document = document
                .set_argument(&inside, hole())
                .unwrap_or_else(|error| panic!("{context}: {error}"));
            Hosted {
                document,
                at: inside,
            }
        }
        Nested::ConditionValue => {
            let document = document
                .start_with(&at, "polars.when")
                .unwrap_or_else(|error| panic!("{context}: when does not start: {error}"));
            let inside = at
                .then(Segment::Link(0))
                .then(Segment::Parameter("statement".to_owned()));
            Hosted {
                document,
                at: inside,
            }
        }
    }
}

/// Where a chain starts.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum StartPlan {
    /// A field of the table in `outer`.
    Field(Outer, Representative),
    /// A function that starts a chain, by its entry.
    Function(&'static str),
}

/// What an expression that holds another in `outer` starts from: a pivot's aggregate starts from
/// `element`, and anything else from the field of `class`.
fn start_for(outer: Outer, class: Representative) -> StartPlan {
    if outer == Outer::PivotAggregate {
        StartPlan::Function("polars.element")
    } else {
        StartPlan::Field(outer, class)
    }
}

/// The start a chain of the class `class` in `outer` begins with.
pub(super) fn source_of(outer: Outer, class: Representative) -> StartPlan {
    start_for(outer, class)
}

/// The document with the expression at `at` made to start from `start`, a function given what it
/// must be given.
pub(super) fn with_start(document: &Document, at: &Place, start: &StartPlan) -> Document {
    match start {
        StartPlan::Field(outer, class) => document
            .set_start(
                at,
                Start::field(field_name(*outer, *class)).expect("a field"),
            )
            .unwrap_or_else(|error| panic!("a field does not start the expression: {error}")),
        StartPlan::Function(id) => {
            let started = document
                .start_with(at, id)
                .unwrap_or_else(|error| panic!("{id} does not start the expression: {error}"));
            let call = at.clone().then(Segment::Start);
            // The outer position of the expression is where the arguments of the start stand.
            let outer = context_of(&started, at.step_index().unwrap_or_default(), at.path())
                .map_or(Outer::Select, |context| context.outer());
            let given = with_required(&started, &call, outer);
            settle(&given, at, 0, &call, None, outer)
        }
    }
}

/// What a place in `document` holds.
pub(super) enum Held<'a> {
    Call(&'a Call),
    Value(&'a Value),
}

/// What `held` holds one segment further in, or `None` where it holds nothing there.
fn descend<'a>(held: Held<'a>, segment: &Segment) -> Option<Held<'a>> {
    Some(match (held, segment) {
        (Held::Call(call), Segment::Parameter(name)) => Held::Value(call.argument(name)?),
        (Held::Value(Value::Many(items)), Segment::Item(at)) => Held::Value(items.get(*at)?),
        (Held::Value(Value::Pairs(entries)), Segment::Key(at)) => Held::Value(&entries.get(*at)?.0),
        (Held::Value(Value::Pairs(entries)), Segment::Value(at)) => {
            Held::Value(&entries.get(*at)?.1)
        }
        (Held::Value(Value::Expression(expression)), Segment::Start) => match expression.start() {
            Start::Call(call) => Held::Call(call),
            Start::Input(_) | Start::Hole => return None,
        },
        (Held::Value(Value::Expression(expression)), Segment::Link(at)) => {
            Held::Call(expression.links().get(*at)?)
        }
        _ => return None,
    })
}

/// What `place` of `document` holds, or `None` where it holds nothing.
pub(super) fn held_at<'a>(document: &'a Document, place: &Place) -> Option<Held<'a>> {
    let step = document.steps().get(place.step_index()?)?;
    place
        .path()
        .iter()
        .try_fold(Held::Call(step.call()), descend)
}

/// The call at `place` of `document`.
pub(super) fn call_at<'a>(document: &'a Document, place: &Place) -> &'a Call {
    match held_at(document, place) {
        Some(Held::Call(call)) => call,
        _ => panic!("{place:?} is no call"),
    }
}

/// The document with the method `id` added as the `at`th link of the expression at `expression`,
/// where it stands at `standing`, given what it must be given, and each argument of it that is
/// still to be picked given what the walk allows it with.
pub(super) fn with_link(
    document: &Document,
    expression: &Place,
    at: usize,
    id: &str,
    standing: &OfferPlace,
    receiver: Representative,
) -> Document {
    let added = document
        .insert_link(expression, at, id, Some(standing))
        .unwrap_or_else(|error| panic!("{id} is not added at {standing:?}: {error}"));
    let link = expression.clone().then(Segment::Link(at));
    let outer = standing.context().outer();
    let given = with_required(&added, &link, outer);
    settle(&given, expression, at + 1, &link, Some(receiver), outer)
}

/// The context of the expression that holds `hole`, which is a way through the step at `step`:
/// the named positions the way passes through, outermost first.
pub(super) fn context_of(document: &Document, step: usize, way: &[Segment]) -> Option<Context> {
    let mut positions: Vec<Position> = Vec::new();
    let mut held = Held::Call(document.steps().get(step)?.call());
    for segment in way {
        if let (Held::Call(call), Segment::Parameter(name)) = (&held, segment)
            && let Some(position) = Position::of_parameter(call.entry().id().as_str(), name)
        {
            positions.push(position);
        }
        held = descend(held, segment)?;
    }
    Context::from_positions(&positions)
}

/// The document with every hole left in it filled, so that the report is of a step with nothing
/// still to be picked: a field of the class the position of the hole takes. A position that needs
/// a `Boolean` is given one, a pivot's aggregate starts from `element`.
pub(super) fn filled(document: &Document) -> Document {
    let mut document = document.clone();
    loop {
        let Some((index, path)) = document
            .steps()
            .iter()
            .enumerate()
            .find_map(|(index, step)| step.first_hole().map(|path| (index, path)))
        else {
            return document;
        };
        let step = &document.steps()[index];
        // The place of the expression is the way without its last segment, the start.
        let way = &path[..path.len() - 1];
        let expression = way.iter().fold(
            Place::of_step(index, Some(step.id().clone())),
            |place, segment| place.then(segment.clone()),
        );
        let context = context_of(&document, index, way);
        let start = match context {
            Some(context) if context.outer() == Outer::PivotAggregate => {
                StartPlan::Function("polars.element")
            }
            Some(context) => {
                let wants_boolean = context.nested() == Some(Nested::ConditionPredicate)
                    || (context.outer() == Outer::Filter && context.nested().is_none());
                let class = if wants_boolean {
                    Representative::Boolean
                } else {
                    Representative::Int64
                };
                StartPlan::Field(context.outer(), class)
            }
            None => StartPlan::Field(Outer::Select, Representative::Int64),
        };
        document = with_start(&document, &expression, &start);
    }
}
