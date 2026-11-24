//! Why an edit of a document is refused, and where.

use std::fmt;

use crate::allowed;

use super::super::error::{Cause, Place, Refusal, Segment};
use super::super::model::{StepId, Value};

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
        /// What is there.
        found: Thing,
    },
    /// The place names another kind of thing than the edit works on, such as a
    /// parameter where the edit needs a link.
    WrongPlace {
        /// What the edit needs.
        needs: Wanted,
        /// What the place names.
        found: Thing,
    },
    /// A parameter that does not hold a value for each item of another, where
    /// the edit sets the value for an item.
    NotPerItem,
    /// A parameter that holds a value for each item of another, and has no
    /// default to give the items that have none of their own.
    NoDefault,
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
            Self::NotPerItem => {
                formatter.write_str("this parameter holds no value for each item of another")
            }
            Self::NoDefault => {
                formatter.write_str("this parameter has no default for the items it is not set for")
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

/// What a value is the argument of a parameter holds, for an error that names
/// what was there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// An expression: [`Value::Expression`].
    Expression,
    /// Names of input columns.
    Columns,
    /// A whole number.
    Integer,
    /// A number.
    Float,
    /// Text.
    String,
    /// A flag.
    Flag,
    /// One word of a closed set.
    Choice,
    /// A dtype.
    Dtype,
    /// A duration.
    Duration,
    /// A pattern.
    Pattern,
    /// A time zone.
    TimeZone,
    /// A table.
    Table,
    /// A path.
    Path,
    /// Key and value entries.
    Pairs,
    /// An ordered list.
    List,
}

impl ValueKind {
    /// The kind of `value`.
    #[must_use]
    pub const fn of(value: &Value) -> Self {
        match value {
            Value::Expression(_) => Self::Expression,
            Value::Columns(_) => Self::Columns,
            Value::Integer(_) => Self::Integer,
            Value::Float(_) => Self::Float,
            Value::String(_) => Self::String,
            Value::Flag(_) => Self::Flag,
            Value::Choice(_) => Self::Choice,
            Value::Dtype(_) => Self::Dtype,
            Value::Duration(_) => Self::Duration,
            Value::Pattern(_) => Self::Pattern,
            Value::TimeZone(_) => Self::TimeZone,
            Value::Table(_) => Self::Table,
            Value::Path(_) => Self::Path,
            Value::Pairs(_) => Self::Pairs,
            Value::Many(_) => Self::List,
        }
    }

    const fn words(self) -> &'static str {
        match self {
            Self::Expression => "an expression",
            Self::Columns => "column names",
            Self::Integer => "a whole number",
            Self::Float => "a number",
            Self::String => "text",
            Self::Flag => "a flag",
            Self::Choice => "a choice",
            Self::Dtype => "a dtype",
            Self::Duration => "a duration",
            Self::Pattern => "a pattern",
            Self::TimeZone => "a time zone",
            Self::Table => "a table",
            Self::Path => "a path",
            Self::Pairs => "pairs",
            Self::List => "a list",
        }
    }
}

/// What a place names, for an error that says what was there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thing {
    /// A step: the place has no way into it.
    Step,
    /// A call: the step's own, or the start or a link of an expression.
    Call,
    /// The argument of a parameter.
    Parameter,
    /// An item of a list.
    Item,
    /// The key of an entry of pairs.
    Key,
    /// The value of an entry of pairs.
    EntryValue,
    /// The start of an expression.
    Start,
    /// A link of an expression.
    Link,
    /// An expression that starts with no call: a hole, or a connected input.
    ExpressionWithoutCall,
    /// An argument, or a part of one, of this kind.
    Value(ValueKind),
}

impl Thing {
    /// What the last segment of a way names.
    #[must_use]
    pub const fn of_segment(segment: &Segment) -> Self {
        match segment {
            Segment::Parameter(_) => Self::Parameter,
            Segment::Item(_) => Self::Item,
            Segment::Key(_) => Self::Key,
            Segment::Value(_) => Self::EntryValue,
            Segment::Start => Self::Start,
            Segment::Link(_) => Self::Link,
        }
    }

    /// What is there where an edit that needs `found` finds `value`.
    pub(super) const fn of_value(value: &Value) -> Self {
        Self::Value(ValueKind::of(value))
    }
}

impl fmt::Display for Thing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Step => "a step",
            Self::Call => "a call",
            Self::Parameter => "a parameter",
            Self::Item => "an item of a list",
            Self::Key => "the key of an entry",
            Self::EntryValue => "the value of an entry",
            Self::Start => "the start of an expression",
            Self::Link => "a link of an expression",
            Self::ExpressionWithoutCall => "an expression that starts with no call",
            Self::Value(kind) => kind.words(),
        })
    }
}

/// What an edit needs the place it is made at to name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wanted {
    /// A call: the step's own, or the start or a link of an expression.
    Call,
    /// The argument of a parameter, or a part of one: an item, or the key or
    /// value of an entry.
    Argument,
    /// A parameter, whose argument is given or taken away.
    Parameter,
    /// An item of a list.
    Item,
    /// The key or the value of an entry of pairs.
    EntryPart,
    /// A link of an expression.
    Link,
    /// An expression.
    Expression,
    /// A list.
    List,
    /// Pairs.
    Pairs,
    /// A part inside a step, which a step has no way to.
    PartOfStep,
}

impl fmt::Display for Wanted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Call => "a call",
            Self::Argument => "an argument, or an item, key or value of one",
            Self::Parameter => "a parameter",
            Self::Item => "an item of a list",
            Self::EntryPart => "the key or value of an entry",
            Self::Link => "a link of an expression",
            Self::Expression => "an expression",
            Self::List => "a list",
            Self::Pairs => "pairs",
            Self::PartOfStep => "a part inside a step",
        })
    }
}
