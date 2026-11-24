//! What the engine starts new parts of a document with: a step, a link of an
//! expression, an option, a test of a condition.
//!
//! The catalogue says what each starts with, as [`Setting`]s: Polars'
//! defaults and the editor's first choices of a value. This turns them into
//! the values a document holds. Where a setting is still to be given, an
//! expression has a hole, which the report says is incomplete; any other kind
//! of argument has no value that stands for nothing, so it is left out, and the
//! report says it must be given.

use crate::allowed::{self, Place as OfferPlace};
use crate::catalog::{
    self, BuildError, Entry, Kind, Parameter, ParameterDefault, Setting, Shape, StepFrame,
};

use super::super::dtype::Dtype;
use super::super::error::{Cause, Refusal, Segment};
use super::super::model::{Call, Expression, Start, Value, function};
use super::error::EditProblem;
use super::path::Fault;

/// What `kind` takes of an expression, for a setting that stands for one.
const ANY_EXPRESSION: Kind = Kind::Expression(&[]);

/// The value a setting is for the parameter `parameter` of kind `kind`, or
/// `None` where it stands for nothing a document holds.
fn value_of(
    setting: &Setting,
    parameter: &'static str,
    kind: Kind,
) -> Result<Option<Value>, Refusal> {
    let expression = matches!(kind, Kind::Expression(_));
    Ok(Some(match setting {
        Setting::ToGive => {
            return Ok(expression.then(|| Value::Expression(Expression::hole())));
        }
        Setting::Flag(flag) => Value::Flag(Some(*flag)),
        Setting::Integer(value) if expression => {
            Value::Expression(Expression::literal(&allowed::Literal::Integer(*value))?)
        }
        Setting::Integer(value) => Value::from_literal(&allowed::Literal::Integer(*value), kind),
        Setting::Float(value) if expression => {
            Value::Expression(Expression::literal(&allowed::Literal::Float(*value))?)
        }
        Setting::Float(value) => Value::from_literal(&allowed::Literal::Float(*value), kind),
        Setting::Text(text) if expression => Value::Expression(Expression::literal(
            &allowed::Literal::String(text.clone()),
        )?),
        Setting::Text(text) if kind == Kind::Column => Value::Columns(vec![text.clone()]),
        Setting::Text(text) => Value::from_literal(&allowed::Literal::String(text.clone()), kind),
        Setting::Word(word) => match kind {
            Kind::Dtype => Value::Dtype(
                word.parse::<Dtype>()
                    .map_err(|error| Refusal::new(Cause::Dtype(error)))?,
            ),
            Kind::Column => Value::Columns(vec![word.clone()]),
            Kind::Table => Value::Table(word.clone()),
            Kind::Choice(_) => Value::Choice(word.clone()),
            Kind::TimeZone => Value::TimeZone(None),
            _ => Value::String(word.clone()),
        },
        Setting::Field(name) => Value::Expression(Expression::field(name)?),
        Setting::Element(method) => {
            let element = Call::new(function("polars.element")?, [])?;
            let link = Call::new(function(&format!("Expr.{method}"))?, [])?;
            Value::Expression(Expression::new(Start::Call(element), vec![link])?)
        }
        Setting::Comparison {
            entry,
            field,
            other,
        } => {
            let Some(Value::Expression(field)) = value_of(field, parameter, ANY_EXPRESSION)? else {
                return Err(wrong_kind(parameter, "an expression"));
            };
            let other = value_of(other, parameter, ANY_EXPRESSION)?
                .ok_or_else(|| wrong_kind(parameter, "an expression"))?;
            let link = Call::new(function(entry)?, [("other", other)])?;
            let mut links = field.links().to_vec();
            links.push(link);
            Value::Expression(Expression::new(field.start().clone(), links)?)
        }
        Setting::List(items) => Value::Many(
            items
                .iter()
                .map(|item| value_of(item, parameter, kind))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
        ),
        Setting::Pairs(pairs) => {
            let Kind::Pairs(key_kind, value_kind) = kind else {
                return Err(wrong_kind(parameter, "pairs"));
            };
            let mut entries = Vec::with_capacity(pairs.len());
            for (key, value) in pairs {
                let key = value_of(key, parameter, *key_kind)?;
                let value = value_of(value, parameter, *value_kind)?;
                if let (Some(key), Some(value)) = (key, value) {
                    entries.push((key, value));
                }
            }
            Value::Pairs(entries)
        }
    }))
}

fn wrong_kind(parameter: &'static str, expected: &'static str) -> Refusal {
    Refusal::new(Cause::Arguments(BuildError::WrongKind {
        parameter,
        expected,
        found: "something else",
    }))
}

/// The argument a setting is for `parameter`, in the shape the parameter holds
/// it, or `None` where it stands for nothing a document holds.
fn parameter_value(
    setting: &Setting,
    parameter: &'static Parameter,
) -> Result<Option<Value>, Refusal> {
    let value = value_of(setting, parameter.name(), parameter.kind())?;
    Ok(match (parameter.shape(), setting, value) {
        // A parameter that holds a list starts with a list of one still to be given.
        (Shape::Many, Setting::ToGive, Some(value)) => Some(Value::Many(vec![value])),
        // Anything else that is not a list is the one item of one.
        (Shape::Many, Setting::List(_), value) => value,
        (Shape::Many, _, Some(value)) => Some(Value::Many(vec![value])),
        (_, _, value) => value,
    })
}

/// The call a new step of `entry` starts as, on `frame`.
pub(super) fn step_call(entry: &'static Entry, frame: &StepFrame<'_>) -> Result<Call, Fault> {
    let start = catalog::start_step(entry, frame).ok_or_else(|| {
        Fault::here(EditProblem::NoStart {
            entry: entry.id().as_str(),
        })
    })?;
    let mut given = Vec::new();
    for (name, setting) in start.arguments() {
        let Some(parameter) = entry
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == *name)
        else {
            continue;
        };
        if let Some(value) = parameter_value(setting, parameter).map_err(Fault::refused)? {
            given.push((*name, value));
        }
    }
    Call::new(entry, given).map_err(Fault::refused)
}

/// A new test of a condition: a comparison of a field and a value, each still
/// to be given.
pub(super) fn started_test() -> Result<Value, Fault> {
    value_of(&Setting::started_test(), "predicates", ANY_EXPRESSION)
        .map_err(Fault::refused)?
        .ok_or_else(|| Fault::refused(wrong_kind("predicates", "an expression")))
}

/// What an argument that is an expression still to be picked is for
/// `parameter`.
fn unfilled(parameter: &Parameter) -> Value {
    let hole = Value::Expression(Expression::hole());
    if parameter.shape() == Shape::Many {
        Value::Many(vec![hole])
    } else {
        hole
    }
}

/// Whether `value` holds nothing but what is still to be picked.
fn is_unpicked(value: &Value) -> bool {
    match value {
        Value::Expression(expression) => {
            matches!(expression.start(), Start::Hole) && expression.links().is_empty()
        }
        Value::Many(items) => items.iter().all(is_unpicked),
        _ => false,
    }
}

/// `value` as a parameter of `shape` holds it: a list for a parameter that
/// holds one, and the first of a list for a parameter that holds one.
fn in_shape(value: Value, shape: Shape) -> Value {
    match (shape, value) {
        (Shape::Many, list @ Value::Many(_)) => list,
        (Shape::Many, one) => Value::Many(vec![one]),
        (_, Value::Many(items)) => items
            .into_iter()
            .next()
            .unwrap_or_else(|| Value::Expression(Expression::hole())),
        (_, one) => one,
    }
}

/// The first parameter of `entry`, when it takes an expression: the argument
/// of a method, which is drawn where the method is.
fn argument_parameter(entry: &Entry) -> Option<&'static Parameter> {
    entry
        .parameters()
        .first()
        .filter(|parameter| matches!(parameter.kind(), Kind::Expression(_)))
}

/// The call of the method `entry` as a link added where `standing` says it
/// stands, or as the link `from` becomes when it is swapped for `entry`.
///
/// The first parameter, when it takes an expression, is given: what `from`
/// had for its own, when that was picked; otherwise the default the tables
/// allow there, a number or text; otherwise nothing, where Polars is left to
/// its default; otherwise a hole. The options `from` had are kept for the
/// parameters `entry` has of the same name, kind and shape. A parameter the
/// method must be given and that is no argument is a slot, which starts as a
/// hole for an expression and is left out otherwise.
pub(super) fn link_call(
    entry: &'static Entry,
    standing: Option<&OfferPlace>,
    from: Option<&Call>,
) -> Result<Call, Fault> {
    let mut given: Vec<(&'static str, Value)> = Vec::new();
    if let Some(first) = argument_parameter(entry) {
        let carried = from
            .and_then(|old| {
                argument_parameter(old.entry()).and_then(|parameter| old.argument(parameter.name()))
            })
            .filter(|value| !is_unpicked(value))
            .map(|value| in_shape(value.clone(), first.shape()));
        if let Some(value) = carried {
            given.push((first.name(), value));
        } else if let Some(default) =
            standing.and_then(|place| allowed::default_argument(entry, place))
        {
            let value =
                Value::Expression(Expression::literal(default.literal()).map_err(Fault::refused)?);
            given.push((default.parameter(), in_shape(value, first.shape())));
        } else if allowed::left_out_at_start(entry) != Some(first.name()) {
            given.push((first.name(), unfilled(first)));
        }
    }
    if let Some(old) = from {
        let old_argument = argument_parameter(old.entry()).map(Parameter::name);
        for held in old.arguments() {
            if Some(held.parameter()) == old_argument {
                continue;
            }
            let same = |parameter: &Parameter| {
                old.entry()
                    .parameters()
                    .iter()
                    .find(|old| old.name() == parameter.name())
                    .is_some_and(|old| {
                        old.kind() == parameter.kind() && old.shape() == parameter.shape()
                    })
            };
            if let Some(parameter) = entry
                .parameters()
                .iter()
                .find(|parameter| parameter.name() == held.parameter() && same(parameter))
                && !given.iter().any(|(name, _)| *name == parameter.name())
            {
                given.push((parameter.name(), held.value().clone()));
            }
        }
    }
    for name in allowed::slots_at_start(entry) {
        if given.iter().any(|(held, _)| *held == name) {
            continue;
        }
        let Some(parameter) = entry
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == name)
        else {
            continue;
        };
        if let Some(value) =
            parameter_value(&parameter.start_setting(), parameter).map_err(Fault::refused)?
        {
            given.push((name, value));
        }
    }
    given.sort_by_key(|(name, _)| {
        entry
            .parameters()
            .iter()
            .position(|parameter| parameter.name() == *name)
    });
    Call::new(entry, given).map_err(Fault::refused)
}

/// The call of the function `entry` as the start of an expression, and the
/// links that complete what it starts: none, except for `polars.when`, which
/// is a condition and starts with a test, and is completed by a `then` and an
/// `otherwise` that are still to be picked.
pub(super) fn start_calls(entry: &'static Entry) -> Result<(Call, Vec<Call>), Fault> {
    let mut given: Vec<(&'static str, Value)> = Vec::new();
    for parameter in entry.parameters() {
        if parameter.default() == ParameterDefault::Required
            && matches!(parameter.kind(), Kind::Expression(_))
        {
            given.push((parameter.name(), unfilled(parameter)));
        }
    }
    let mut links = Vec::new();
    if entry.id().as_str() == "polars.when" {
        given.retain(|(name, _)| *name != "predicates");
        given.push(("predicates", Value::Many(vec![started_test()?])));
        for id in ["When.then", "Then.otherwise"] {
            let statement = Value::Expression(Expression::hole());
            links.push(
                Call::new(
                    function(id).map_err(Fault::refused)?,
                    [("statement", statement)],
                )
                .map_err(Fault::refused)?,
            );
        }
    }
    let start = Call::new(entry, given).map_err(Fault::refused)?;
    Ok((start, links))
}

/// What the option `parameter` of `call` starts with when it is added.
pub(super) fn option_start(call: &Call, parameter: &'static Parameter) -> Result<Value, Fault> {
    if matches!(parameter.kind(), Kind::Flag | Kind::Choice(_)) {
        // Which word it may start with depends on the options already set.
        let chosen: Vec<(&str, String)> = call
            .arguments()
            .iter()
            .filter_map(|held| match held.value() {
                Value::Flag(Some(flag)) => Some((held.parameter(), flag.to_string())),
                Value::Choice(word) => Some((held.parameter(), word.clone())),
                _ => None,
            })
            .collect();
        let chosen: Vec<(&str, &str)> = chosen
            .iter()
            .map(|(name, word)| (*name, word.as_str()))
            .collect();
        let offers = allowed::option_offers(call.entry(), &chosen);
        if let Some(offer) = offers
            .iter()
            .find(|offer| offer.parameter() == parameter.name())
        {
            return match (offer.starter(), offer.stuck()) {
                (Some(word), _) if parameter.kind() == Kind::Flag => {
                    Ok(Value::Flag(Some(word == "true")))
                }
                (Some(word), _) => Ok(Value::Choice(word.to_owned())),
                (None, Some(refusal)) => {
                    Err(at_parameter(parameter, EditProblem::CannotAdd(refusal)))
                }
                (None, None) => Err(needs_value(parameter)),
            };
        }
    }
    parameter_value(&parameter.start_setting(), parameter)
        .map_err(Fault::refused)?
        .ok_or_else(|| needs_value(parameter))
}

fn needs_value(parameter: &Parameter) -> Fault {
    at_parameter(
        parameter,
        EditProblem::NeedsValue {
            parameter: parameter.name(),
        },
    )
}

/// A fault at the argument of `parameter` of the call the edit has come to.
fn at_parameter(parameter: &Parameter, problem: EditProblem) -> Fault {
    Fault::inside(
        vec![Segment::Parameter(parameter.name().to_owned())],
        problem,
    )
}

/// What a parameter holds when it is not given, as an argument, for the items
/// of a parameter that holds one for each item of another. `None` where the
/// default is not a value a document holds.
pub(super) fn default_value(parameter: &Parameter) -> Option<Value> {
    use catalog::DefaultValue;
    Some(match (parameter.kind(), parameter.default().value()) {
        (Kind::Flag, DefaultValue::Flag(flag)) => Value::Flag(Some(flag)),
        (Kind::Flag, DefaultValue::Unset) => Value::Flag(None),
        (Kind::Choice(_), DefaultValue::Text(word)) => Value::Choice(word),
        (Kind::Value(_), DefaultValue::Integer(value)) => Value::Integer(value),
        (Kind::Value(_), DefaultValue::Float(value)) => Value::Float(value),
        _ => return None,
    })
}
