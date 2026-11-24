//! Why an edit of a document is refused, and where.

use std::fmt;

use crate::allowed;

use super::super::error::{Cause, Place, Refusal};
use super::super::model::StepId;

/// An edit that was refused, the place it was made at and why. The document the
/// edit was made on is unchanged.
#[derive(Debug)]
pub struct EditError(Box<Failed>);

#[derive(Debug)]
struct Failed {
    place: Place,
    problem: EditProblem,
}

impl EditError {
    pub(super) fn new(place: Place, problem: EditProblem) -> Self {
        Self(Box::new(Failed { place, problem }))
    }

    /// The error for a part of a document the model refused, at the place the
    /// refusal gives.
    pub(super) fn of_refusal(refusal: Refusal) -> Self {
        let (place, cause) = refusal.into_parts();
        Self::new(place, EditProblem::Refused(cause))
    }

    /// Where the edit was refused: the place it names, or the part of it where
    /// the way down into the document failed, or the argument the model
    /// refused.
    #[must_use]
    pub fn place(&self) -> &Place {
        &self.0.place
    }

    /// Why it was refused.
    #[must_use]
    pub fn problem(&self) -> &EditProblem {
        &self.0.problem
    }
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.0.place, self.0.problem)
    }
}

impl std::error::Error for EditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0.problem)
    }
}

/// Why an edit was refused.
#[derive(Debug)]
pub enum EditProblem {
    /// The place names a step the document does not have: its number is past
    /// the last step, or the place is in no step.
    NoStep {
        /// How many steps the document has.
        steps: usize,
    },
    /// The place names a step by an identifier the step at its number does not
    /// have, so the place is out of date: the document changed since it was
    /// made.
    StepChanged {
        /// The identifier of the step that is there now.
        found: StepId,
    },
    /// Nothing is at the place: a parameter that is not given, or an item,
    /// entry or link past the end.
    Nothing,
    /// The last segment of the place names a part that what is there does not
    /// have, such as an item of an argument that is not a list.
    DoesNotFit {
        /// What is there, such as `a flag`.
        found: &'static str,
    },
    /// The place names another kind of thing than the edit works on, such as a
    /// parameter where the edit needs a link.
    WrongPlace {
        /// What the edit needs, such as `a link of an expression`.
        needs: &'static str,
        /// What the place names.
        found: &'static str,
    },
    /// A position past the end of what the edit puts something into, or moves
    /// something within.
    OutOfRange {
        /// The position that was asked for.
        index: usize,
        /// How many there are.
        len: usize,
    },
    /// An identifier that is no entry of the catalogue.
    UnknownEntry(String),
    /// An entry that is a method of a lazy frame but starts no step: the
    /// engine says nothing of what a new step of it holds.
    NoStart {
        /// The entry's identifier.
        entry: &'static str,
    },
    /// A parameter that cannot be added as an option, because Polars refuses
    /// every value it could start with beside the options already set.
    CannotAdd(allowed::Refusal),
    /// A parameter that starts with nothing the document can hold, such as
    /// text still to be typed: give it its value with `set_argument`.
    NeedsValue {
        /// The parameter, by Python's name.
        parameter: &'static str,
    },
    /// The model refuses what the edit would make, as it refuses it when it
    /// reads it: arguments of the wrong kind, an unknown parameter, links out
    /// of order, a repeated step identifier, a step nested too deep. The
    /// error's place is where.
    Refused(Cause),
}

impl fmt::Display for EditProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoStep { steps } => {
                write!(formatter, "there is no such step: the document has {steps}")
            }
            Self::StepChanged { found } => write!(
                formatter,
                "the step there is {:?} now, so the place is out of date",
                found.as_str()
            ),
            Self::Nothing => formatter.write_str("there is nothing here"),
            Self::DoesNotFit { found } => write!(formatter, "{found} has no such part"),
            Self::WrongPlace { needs, found } => {
                write!(formatter, "this needs {needs}, not {found}")
            }
            Self::OutOfRange { index, len } => {
                write!(formatter, "position {index} is past the {len} there are")
            }
            Self::UnknownEntry(entry) => write!(formatter, "the catalogue has no entry {entry:?}"),
            Self::NoStart { entry } => write!(formatter, "{entry} has no start as a step"),
            Self::CannotAdd(refusal) => write!(formatter, "{}", refusal.reason()),
            Self::NeedsValue { parameter } => {
                write!(
                    formatter,
                    "{parameter} starts with nothing: it needs its value"
                )
            }
            Self::Refused(cause) => cause.fmt(formatter),
        }
    }
}

impl std::error::Error for EditProblem {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Refused(cause) => Some(cause),
            _ => None,
        }
    }
}
