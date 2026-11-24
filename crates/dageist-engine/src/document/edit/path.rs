//! Following a place down through a step, and rebuilding what lies on the way.
//!
//! Nothing here changes a document: each call, expression and step on the way
//! to the place is made again with its constructor, so that what an edit
//! builds is checked as it is built. A place is followed one segment at a
//! time. A parameter segment goes from a call to the argument it was given; an
//! item, key or value segment goes into a list or pairs; the start and link
//! segments go from an expression to a call of it.

use super::super::error::{Place, Refusal, Segment};
use super::super::model::{Call, Expression, Start, Value};
use super::error::{EditError, EditProblem, Thing, Wanted};
use super::starting::default_value;
use crate::catalog::Shape;

/// An edit that could not be made, before it is given the place it was made
/// at.
#[derive(Debug)]
pub(super) struct Fault {
    /// How many segments of the way the edit followed, counted from the node
    /// the fault was made at, before the one that failed.
    depth: usize,
    /// More of the way, from the model's own refusal.
    relative: Vec<Segment>,
    problem: EditProblem,
}

impl Fault {
    /// A fault at the node the edit has come to.
    pub(super) const fn here(problem: EditProblem) -> Self {
        Self::beyond(0, problem)
    }

    /// A fault `more` segments further than the node the edit has come to.
    pub(super) const fn beyond(more: usize, problem: EditProblem) -> Self {
        Self {
            depth: more,
            relative: Vec::new(),
            problem,
        }
    }

    /// A fault at a part inside the node the edit has come to, named by the way
    /// from it.
    pub(super) const fn inside(relative: Vec<Segment>, problem: EditProblem) -> Self {
        Self {
            depth: 0,
            relative,
            problem,
        }
    }

    /// A refusal of the model, of the node the edit has come to.
    pub(super) fn refused(refusal: Refusal) -> Self {
        let (place, cause) = refusal.into_parts();
        Self {
            depth: 0,
            relative: place.path().to_vec(),
            problem: EditProblem::Refused(cause),
        }
    }

    /// The same fault, `by` segments further down, for the node it was made at
    /// is `by` segments in.
    pub(super) const fn shifted(mut self, by: usize) -> Self {
        self.depth += by;
        self
    }

    /// The error for an edit at `place`: the place as far as the edit followed
    /// it, then the way inside that the model's refusal gives.
    pub(super) fn at(self, place: &Place) -> EditError {
        let mut here = Place::of_step(
            place.step_index().unwrap_or_default(),
            place.step_id().cloned(),
        );
        for segment in place.path().iter().take(self.depth) {
            here = here.then(segment.clone());
        }
        for segment in self.relative {
            here = here.then(segment);
        }
        EditError::new(here, self.problem)
    }
}

/// What an edit does where the way ends: to the call there, or to the value.
pub(super) enum Leaf<'a> {
    /// The place ends at a call: the step's own, the start of an expression or
    /// one of its links. The edit makes the call again.
    Call(&'a dyn Fn(&Call) -> Result<Call, Fault>),
    /// The place ends at an argument, or a part of one. The edit makes it
    /// again.
    Value(&'a dyn Fn(&Value) -> Result<Value, Fault>),
}

/// `call` with the argument for `parameter` set to `value`, or taken away when
/// that is `None`, as [`with_given_alone`] does, and the values that a parameter holds
/// for each key kept as many as the keys when this changes how many there are: a
/// value for each new key is the parameter's default, those for keys that are gone
/// go, and one value for every key stays as it is.
pub(super) fn with_given(
    call: &Call,
    parameter: &str,
    value: Option<Value>,
) -> Result<Call, Fault> {
    let mut made = with_given_alone(call, parameter, value)?;
    for follower in call.entry().parameters() {
        let Shape::PerItem(of) = follower.shape() else {
            continue;
        };
        let keys = made.key_count(of);
        if call.key_count(of) == keys {
            continue;
        }
        let Some(Value::Many(values)) = made.argument(follower.name()) else {
            continue;
        };
        let mut values = values.clone();
        values.truncate(keys);
        if values.len() < keys {
            let Some(default) = default_value(follower) else {
                continue;
            };
            values.resize(keys, default);
        }
        made = with_given_alone(&made, follower.name(), Some(Value::Many(values)))?;
    }
    Ok(made)
}

/// `call` with the argument for `parameter` set to `value`, or taken away when
/// that is `None`. A new argument goes where the entry's parameters put it,
/// among the ones already given. Nothing else changes.
pub(super) fn with_given_alone(
    call: &Call,
    parameter: &str,
    value: Option<Value>,
) -> Result<Call, Fault> {
    let rank = |name: &str| {
        call.entry()
            .parameters()
            .iter()
            .position(|known| known.name() == name)
    };
    let present = call
        .arguments()
        .iter()
        .any(|given| given.parameter() == parameter);
    let mut arguments: Vec<(&str, Value)> = Vec::with_capacity(call.arguments().len() + 1);
    let mut pending = if present { None } else { value.clone() };
    for given in call.arguments() {
        if given.parameter() == parameter {
            if let Some(value) = &value {
                arguments.push((parameter, value.clone()));
            }
            continue;
        }
        let comes_later = rank(given.parameter())
            .zip(rank(parameter))
            .is_some_and(|(there, mine)| there > mine);
        if comes_later && let Some(value) = pending.take() {
            arguments.push((parameter, value));
        }
        arguments.push((given.parameter(), given.value().clone()));
    }
    if let Some(value) = pending {
        arguments.push((parameter, value));
    }
    Call::new(call.entry(), arguments).map_err(Fault::refused)
}

/// `call` made again with `leaf` applied where `path` ends. `at` is how many
/// segments of the place were followed to come to `call`.
pub(super) fn rebuild_call(
    call: &Call,
    path: &[Segment],
    at: usize,
    leaf: &Leaf<'_>,
) -> Result<Call, Fault> {
    let Some((segment, rest)) = path.split_first() else {
        return match leaf {
            Leaf::Call(edit) => edit(call).map_err(|fault| fault.shifted(at)),
            Leaf::Value(_) => Err(Fault::here(EditProblem::WrongPlace {
                needs: Wanted::Argument,
                found: Thing::Call,
            })
            .shifted(at)),
        };
    };
    let Segment::Parameter(name) = segment else {
        return Err(Fault::beyond(1, EditProblem::DoesNotFit { found: Thing::Call }).shifted(at));
    };
    let Some(given) = call.argument(name) else {
        return Err(Fault::beyond(1, EditProblem::Nothing).shifted(at));
    };
    let value = rebuild_value(given, rest, at + 1, leaf)?;
    with_given(call, name, Some(value)).map_err(|fault| fault.shifted(at))
}

/// `value` made again with `leaf` applied where `path` ends. `at` is how many
/// segments of the place were followed to come to `value`.
pub(super) fn rebuild_value(
    value: &Value,
    path: &[Segment],
    at: usize,
    leaf: &Leaf<'_>,
) -> Result<Value, Fault> {
    let Some((segment, rest)) = path.split_first() else {
        return match leaf {
            Leaf::Value(edit) => edit(value).map_err(|fault| fault.shifted(at)),
            Leaf::Call(_) => Err(Fault::here(EditProblem::WrongPlace {
                needs: Wanted::Call,
                found: Thing::of_value(value),
            })
            .shifted(at)),
        };
    };
    let misfit = || {
        Fault::beyond(
            1,
            EditProblem::DoesNotFit {
                found: Thing::of_value(value),
            },
        )
        .shifted(at)
    };
    let nothing = || Fault::beyond(1, EditProblem::Nothing).shifted(at);
    match (segment, value) {
        (Segment::Item(index), Value::Many(items)) => {
            let item = items.get(*index).ok_or_else(nothing)?;
            let item = rebuild_value(item, rest, at + 1, leaf)?;
            let mut items = items.clone();
            if let Some(slot) = items.get_mut(*index) {
                *slot = item;
            }
            Ok(Value::Many(items))
        }
        (Segment::Key(index) | Segment::Value(index), Value::Pairs(pairs)) => {
            let (key, entry) = pairs.get(*index).ok_or_else(nothing)?;
            let (key, entry) = if matches!(segment, Segment::Key(_)) {
                (rebuild_value(key, rest, at + 1, leaf)?, entry.clone())
            } else {
                (key.clone(), rebuild_value(entry, rest, at + 1, leaf)?)
            };
            let mut pairs = pairs.clone();
            if let Some(slot) = pairs.get_mut(*index) {
                *slot = (key, entry);
            }
            Ok(Value::Pairs(pairs))
        }
        (Segment::Start, Value::Expression(expression)) => {
            let Start::Call(start) = expression.start() else {
                return Err(Fault::beyond(
                    1,
                    EditProblem::DoesNotFit {
                        found: Thing::ExpressionWithoutCall,
                    },
                )
                .shifted(at));
            };
            let start = rebuild_call(start, rest, at + 1, leaf)?;
            Expression::new(Start::Call(start), expression.links().to_vec())
                .map(Value::Expression)
                .map_err(|refusal| Fault::refused(refusal).shifted(at))
        }
        (Segment::Link(index), Value::Expression(expression)) => {
            let link = expression.links().get(*index).ok_or_else(nothing)?;
            let link = rebuild_call(link, rest, at + 1, leaf)?;
            let mut links = expression.links().to_vec();
            if let Some(slot) = links.get_mut(*index) {
                *slot = link;
            }
            Expression::new(expression.start().clone(), links)
                .map(Value::Expression)
                .map_err(|refusal| Fault::refused(refusal).shifted(at))
        }
        _ => Err(misfit()),
    }
}
