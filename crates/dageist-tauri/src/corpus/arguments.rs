//! What a call is given besides the method or function itself: the values of the parameters it
//! must be given, and the expressions that stand where an argument is still to be picked, chosen
//! so that the tables allow the call where it stands.

use dageist_engine::allowed::{self, Literal, Outer, Representative};
use dageist_engine::catalog::{Kind, Parameter, ParameterDefault, Shape, ValueType};
use dageist_engine::document::{Call, Document, Dtype, Expression, Place, Segment, Start, Value};

use super::documents::{call_at, entry, field, field_name, hole};
use super::source::{OTHER, column_of, report};

/// The value that stands for one argument of `kind`, as a document holds it.
pub(super) fn sample(kind: Kind, outer: Outer) -> Value {
    match kind {
        Kind::Expression(_) => field(&field_name(outer, Representative::Int64)),
        Kind::Column => Value::Columns(vec![column_of(Representative::Int64)]),
        Kind::Value(ValueType::Integer | ValueType::Any) => Value::Integer(1),
        Kind::Value(ValueType::Float) => Value::Float(1.5),
        Kind::Value(ValueType::String) => Value::String("a".to_owned()),
        Kind::Value(ValueType::Date) => Value::String("2026-03-02".to_owned()),
        Kind::Flag => Value::Flag(Some(true)),
        Kind::Choice(words) => {
            Value::Choice((*words.first().expect("a choice has a word")).to_owned())
        }
        Kind::Dtype => Value::Dtype("Int64".parse::<Dtype>().expect("a dtype")),
        Kind::Duration => Value::Duration("1d".to_owned()),
        Kind::Pattern => Value::Pattern("a+".to_owned()),
        Kind::TimeZone => Value::TimeZone(Some("UTC".to_owned())),
        Kind::Table => Value::Table(OTHER.to_owned()),
        Kind::Path => Value::Path("a.csv".to_owned()),
        Kind::Pairs(key, value) => Value::Pairs(vec![(sample(*key, outer), sample(*value, outer))]),
    }
}

/// The argument for `parameter`, one of its kind, in the shape it holds them.
pub(super) fn sample_for(parameter: &Parameter, outer: Outer) -> Value {
    let one = sample(parameter.kind(), outer);
    match parameter.shape() {
        Shape::One => one,
        Shape::Many | Shape::PerItem(_) => Value::Many(vec![one]),
    }
}

/// Whether `value` is an expression with nothing picked, or a list of them.
fn is_unpicked(value: &Value) -> bool {
    match value {
        Value::Expression(expression) => {
            matches!(expression.start(), Start::Hole) && expression.links().is_empty()
        }
        Value::Many(items) => !items.is_empty() && items.iter().all(is_unpicked),
        _ => false,
    }
}

/// Whether `parameter` must be given to the call.
fn must_be_given(parameter: &Parameter) -> bool {
    parameter.default() == ParameterDefault::Required || parameter.is_needed()
}

/// The document with each parameter of the call at `place` that it must be given and that is no
/// expression given a value of its kind, and a window given the key it needs. A parameter that is
/// an expression is a hole, which [`settle`] fills.
pub(super) fn with_required(document: &Document, place: &Place, outer: Outer) -> Document {
    let call = call_at(document, place);
    let mut document = document.clone();
    for parameter in call.entry().parameters() {
        if !must_be_given(parameter) || call.argument(parameter.name()).is_some() {
            continue;
        }
        let value = if matches!(parameter.kind(), Kind::Expression(_)) {
            let one = hole();
            if parameter.shape() == Shape::One {
                one
            } else {
                Value::Many(vec![one])
            }
        } else {
            sample_for(parameter, outer)
        };
        document = give(&document, place, parameter.name(), value);
    }
    // A window needs at least one key: what to group by, or what to order by.
    if call.entry().id().as_str() == "Expr.over"
        && call.argument("partition_by").is_none()
        && call.argument("order_by").is_none()
    {
        document = give(&document, place, "partition_by", Value::Many(vec![hole()]));
    }
    document
}

/// The document with `value` given for `parameter` of the call at `place`.
pub(super) fn give(document: &Document, place: &Place, parameter: &str, value: Value) -> Document {
    document
        .set_argument(
            &place.clone().then(Segment::Parameter(parameter.to_owned())),
            value,
        )
        .unwrap_or_else(|error| panic!("{parameter} is not given at {place:?}: {error}"))
}

/// What an argument still to be picked is given, in the order the choices are tried.
#[derive(Debug, Clone, Copy)]
enum Fill {
    /// A field of a class the tables allow for the parameter after the value the call follows.
    Argument,
    /// The same number or text on every row.
    Literal(&'static Literal),
    /// A field of a class.
    Field(Representative),
    /// The first value of a field of a class, which is one value.
    First(Representative),
}

const WHOLE: Literal = Literal::Integer(1);
const FRACTION: Literal = Literal::Float(0.5);

fn fills() -> Vec<Fill> {
    // `Literal::String` owns its text, so it is written where it is used.
    vec![
        Fill::Argument,
        Fill::Literal(&WHOLE),
        Fill::Literal(&FRACTION),
        Fill::Field(Representative::Int64),
        Fill::Field(Representative::Float64),
        Fill::Field(Representative::String),
        Fill::Field(Representative::Boolean),
        Fill::Field(Representative::Date),
        Fill::Field(Representative::Datetime),
        Fill::Field(Representative::Duration),
        Fill::Field(Representative::Time),
        Fill::First(Representative::Int64),
        Fill::First(Representative::Float64),
        Fill::First(Representative::String),
        Fill::First(Representative::Boolean),
        Fill::First(Representative::Date),
        Fill::First(Representative::Datetime),
        Fill::First(Representative::Duration),
        Fill::First(Representative::Time),
    ]
}

/// The first class the tables allow for `parameter` of `call` after a value of `receiver`. For
/// `is_in` it is a list, none where the tables allow none, for a collection of the receiver's own
/// dtype is a form that Polars has deprecated.
fn allowed_class(
    call: &Call,
    parameter: &Parameter,
    receiver: Representative,
) -> Option<Representative> {
    let classes: Vec<Representative> =
        allowed::argument_dtypes(call.entry(), parameter.name(), receiver)
            .into_iter()
            .filter(|(_, outcome)| matches!(outcome, allowed::Outcome::Allowed(_)))
            .map(|(class, _)| class)
            .collect();
    if call.entry().id().as_str() == "Expr.is_in" {
        let own = format!("List({})", receiver.name());
        return classes
            .iter()
            .copied()
            .find(|class| class.name() == own)
            .or_else(|| {
                classes
                    .iter()
                    .copied()
                    .find(|class| class.name().starts_with("List("))
            });
    }
    classes.first().copied()
}

/// The value `fill` makes for the argument `parameter` of `call`, which follows `receiver`.
fn value_of(
    fill: Fill,
    call: &Call,
    parameter: &Parameter,
    receiver: Option<Representative>,
    outer: Outer,
) -> Value {
    let one = match fill {
        Fill::Argument => {
            let class = receiver
                .and_then(|receiver| allowed_class(call, parameter, receiver))
                .unwrap_or(Representative::Int64);
            field(&field_name(outer, class))
        }
        Fill::Literal(literal) => {
            Value::Expression(Expression::literal(literal).expect("a literal"))
        }
        Fill::Field(class) => field(&field_name(outer, class)),
        Fill::First(class) => {
            let start = Start::field(field_name(outer, class)).expect("a field");
            let first = Call::new(entry("Expr.first"), []).expect("a call of first");
            Value::Expression(Expression::new(start, vec![first]).expect("a chain"))
        }
    };
    if parameter.shape() == Shape::One {
        one
    } else {
        Value::Many(vec![one])
    }
}

/// The expression parameters of the call at `place` that nothing is picked for.
fn unpicked(document: &Document, place: &Place) -> Vec<&'static Parameter> {
    let call = call_at(document, place);
    call.entry()
        .parameters()
        .iter()
        .filter(|parameter| matches!(parameter.kind(), Kind::Expression(_)))
        .filter(|parameter| call.argument(parameter.name()).is_some_and(is_unpicked))
        .collect()
}

/// Whether the engine's walk says the call numbered `index` of the expression at `expression`
/// may stand where it does, counting the start as the first.
fn is_allowed(document: &Document, expression: &Place, index: usize) -> bool {
    let reported = report(document);
    reported
        .steps()
        .first()
        .into_iter()
        .flat_map(|step| step.walk().expressions())
        .filter(|walked| {
            walked.place().step_index() == expression.step_index()
                && walked.place().path() == expression.path()
        })
        .find_map(|walked| {
            walked
                .calls()
                .get(index)
                .and_then(dageist_engine::document::Standing::verdict)
                .map(allowed::Verdict::is_allowed)
        })
        .unwrap_or(false)
}

/// The document with the arguments of the call at `place` that are still to be picked given
/// expressions that the walk allows the call with, where some are. The call is the `index`th of
/// the expression at `expression`, counting its start as the first. `receiver` is the class of
/// the value it follows.
pub(super) fn settle(
    document: &Document,
    expression: &Place,
    index: usize,
    place: &Place,
    receiver: Option<Representative>,
    outer: Outer,
) -> Document {
    let parameters = unpicked(document, place);
    if parameters.is_empty() {
        return document.clone();
    }
    let mut first: Option<Document> = None;
    // A collection of the receiver's own dtype is the form of `is_in` that Polars has deprecated,
    // so the only argument tried is a list.
    let is_in = call_at(document, place).entry().id().as_str() == "Expr.is_in";
    let tried = if is_in { vec![Fill::Argument] } else { fills() };
    if is_in
        && receiver
            .and_then(|receiver| {
                let call = call_at(document, place);
                allowed_class(call, parameters.first()?, receiver)
            })
            .is_none()
    {
        return document.clone();
    }
    for fill in tried {
        let mut filled = document.clone();
        for parameter in &parameters {
            let call = call_at(&filled, place);
            let value = value_of(fill, call, parameter, receiver, outer);
            filled = give(&filled, place, parameter.name(), value);
        }
        if is_allowed(&filled, expression, index) {
            return filled;
        }
        first.get_or_insert(filled);
    }
    first.expect("a fill was tried")
}
