//! Where in a document something is, and why a document is refused.

use std::fmt;

use crate::catalog::{Applies, BuildError};

use super::dtype::DtypeError;
use super::json::SyntaxError;
use super::model::StepId;

/// One step of the way from a step to the thing inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// The argument for a parameter, by Python's name for it.
    Parameter(String),
    /// An item of a list.
    Item(usize),
    /// The key of an entry of pairs.
    Key(usize),
    /// The value of an entry of pairs.
    Value(usize),
    /// The start of an expression.
    Start,
    /// A link of an expression, counted from the first after its start, or a link of a step that
    /// groups, counted from the first after the call that starts the step. A way that starts with
    /// it, in a place of a step, goes into that link, and one that starts with anything else goes
    /// into the step's own call.
    Link(usize),
}

/// A place in a document: a step, then the way down through its arguments.
///
/// The way is as long as it needs to be: `named_exprs`, the value of its
/// first pair, the third link of that expression, the argument `other` of that
/// link, and so on into the expressions arguments hold.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Place {
    index: Option<usize>,
    step: Option<StepId>,
    path: Vec<Segment>,
}

impl Place {
    /// The place of a step, given where it is in the document and, if it has
    /// been read, its identifier.
    #[must_use]
    pub const fn of_step(index: usize, step: Option<StepId>) -> Self {
        Self {
            index: Some(index),
            step,
            path: Vec::new(),
        }
    }

    /// Where the step is in the document, counted from zero. `None` for a
    /// place in the document outside its steps.
    #[must_use]
    pub const fn step_index(&self) -> Option<usize> {
        self.index
    }

    /// The identifier of the step, when it has one.
    #[must_use]
    pub const fn step_id(&self) -> Option<&StepId> {
        self.step.as_ref()
    }

    /// The way from the step to the thing, outermost first.
    #[must_use]
    pub fn path(&self) -> &[Segment] {
        &self.path
    }

    /// This place, one segment further in.
    #[must_use]
    pub fn then(mut self, segment: Segment) -> Self {
        self.path.push(segment);
        self
    }

    /// This place inside a step.
    #[must_use]
    pub(super) fn in_step(mut self, index: usize, step: Option<StepId>) -> Self {
        self.index = Some(index);
        self.step = step;
        self
    }

    /// This place under `segment`: `segment` is where the way now begins.
    #[must_use]
    pub(super) fn under(mut self, segment: Segment) -> Self {
        self.path.insert(0, segment);
        self
    }

    /// This place under the way `path`.
    #[must_use]
    pub(super) fn under_all(mut self, mut path: Vec<Segment>) -> Self {
        path.append(&mut self.path);
        self.path = path;
        self
    }
}

impl fmt::Display for Place {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.step, self.index) {
            (Some(id), _) => write!(formatter, "step {:?}", id.as_str())?,
            (None, Some(index)) => write!(formatter, "step number {}", index.saturating_add(1))?,
            (None, None) => formatter.write_str("the document")?,
        }
        for segment in &self.path {
            match segment {
                Segment::Parameter(name) => write!(formatter, ", {name}")?,
                Segment::Item(item) => write!(formatter, " item {}", item.saturating_add(1))?,
                Segment::Key(entry) => write!(formatter, " entry {} key", entry.saturating_add(1))?,
                Segment::Value(entry) => {
                    write!(formatter, " entry {} value", entry.saturating_add(1))?
                }
                Segment::Start => formatter.write_str(", the start")?,
                Segment::Link(link) => write!(formatter, ", link {}", link.saturating_add(1))?,
            }
        }
        Ok(())
    }
}

/// Where a call may stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// A step of the document: a method of a lazy frame.
    Step,
    /// The start of an expression: a function, or `polars.when`.
    Start,
    /// A link of an expression, after its start: an expression method or a
    /// link of a condition.
    Link,
    /// A link of a step that groups, after `group_by`: a call of the group by that gives.
    Grouped,
}

impl Position {
    const fn words(self) -> &'static str {
        match self {
            Self::Step => "a step",
            Self::Start => "the start of an expression",
            Self::Link => "a link after the start of an expression",
            Self::Grouped => "a call after group_by in a step that groups",
        }
    }
}

/// Why a document, or a part of one, is refused.
#[derive(Debug)]
pub enum Cause {
    /// A value of another JSON type than the document has there.
    Shape {
        /// What the value is for, such as `steps` or `an argument`.
        what: &'static str,
        /// What is wanted, such as `an object`.
        expected: &'static str,
        /// What is there, such as `a string`.
        found: &'static str,
    },
    /// A field that must be there is not.
    MissingField(&'static str),
    /// A field the document has no place for.
    UnknownField(String),
    /// A version of the document this engine does not read, as written.
    UnsupportedVersion(String),
    /// A name that is no entry of the catalogue.
    UnknownEntry(String),
    /// An entry in a place it cannot stand.
    WrongPlace {
        /// The entry's identifier.
        entry: &'static str,
        /// What the entry applies to.
        applies: Applies,
        /// Where it was put.
        position: Position,
    },
    /// An argument of a kind the document does not have.
    UnknownArgument(String),
    /// An argument that is not an object of one key.
    ArgumentKeys(usize),
    /// An entry of pairs that is not a key and a value.
    PairSize(usize),
    /// A dtype that cannot be read.
    Dtype(DtypeError),
    /// The catalogue refuses the arguments of the call.
    Arguments(BuildError),
    /// A number that is not finite.
    NotFinite,
    /// A link of a condition that cannot follow what comes before it.
    OutOfOrder {
        /// The link's entry.
        entry: &'static str,
        /// What the link needs before it, such as `then`.
        needs: &'static str,
        /// What is before it, such as `an expression`.
        found: &'static str,
    },
    /// A step that groups and stops before a call finishes the grouping.
    UnfinishedGrouping {
        /// How far the chain came, such as `a group by`.
        found: &'static str,
    },
    /// An expression that ends where a condition still lacks its `then`.
    UnfinishedCondition {
        /// What the condition is waiting for, such as `then`.
        needs: &'static str,
    },
    /// A step with an empty identifier.
    EmptyStepId,
    /// A step identifier used twice.
    DuplicateStep(StepId),
    /// A step whose JSON would nest arrays and objects deeper than
    /// [`MAX_NESTING`](super::MAX_NESTING) allows.
    TooDeep,
}

impl fmt::Display for Cause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shape {
                what,
                expected,
                found,
            } => write!(formatter, "{what} must be {expected}, not {found}"),
            Self::MissingField(field) => write!(formatter, "the field {field:?} is missing"),
            Self::UnknownField(field) => write!(formatter, "there is no field {field:?}"),
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "version {version} is not one this engine reads; it reads version {}",
                super::VERSION
            ),
            Self::UnknownEntry(entry) => write!(formatter, "the catalogue has no entry {entry:?}"),
            Self::WrongPlace {
                entry,
                applies,
                position,
            } => write!(
                formatter,
                "{entry} ({applies:?}) cannot be {}",
                position.words()
            ),
            Self::UnknownArgument(kind) => write!(formatter, "there is no argument kind {kind:?}"),
            Self::ArgumentKeys(count) => write!(
                formatter,
                "an argument is an object of one key, not of {count}"
            ),
            Self::PairSize(size) => {
                write!(formatter, "a pair is a key and a value, not {size} items")
            }
            Self::Dtype(error) => error.fmt(formatter),
            Self::Arguments(error) => error.fmt(formatter),
            Self::NotFinite => formatter.write_str("a number must be finite"),
            Self::OutOfOrder {
                entry,
                needs,
                found,
            } => write!(formatter, "{entry} needs a {needs} before it, not {found}"),
            Self::UnfinishedCondition { needs } => {
                write!(formatter, "the condition still needs its {needs}")
            }
            Self::UnfinishedGrouping { found } => write!(
                formatter,
                "the grouping stops at {found}: it needs a call that finishes it, which is agg, a \
                 shortcut such as len or sum, head or tail"
            ),
            Self::EmptyStepId => formatter.write_str("a step needs an identifier"),
            Self::TooDeep => write!(
                formatter,
                "the step nests arrays and objects deeper than {} levels",
                super::MAX_NESTING
            ),
            Self::DuplicateStep(id) => {
                write!(
                    formatter,
                    "the step identifier {:?} is used twice",
                    id.as_str()
                )
            }
        }
    }
}

impl std::error::Error for Cause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Dtype(error) => Some(error),
            Self::Arguments(error) => Some(error),
            _ => None,
        }
    }
}

/// A part of a document that was refused, and where it is.
#[derive(Debug)]
pub struct Refusal(Box<Refused>);

#[derive(Debug)]
struct Refused {
    place: Place,
    cause: Cause,
}

impl Refusal {
    pub(super) fn new(cause: Cause) -> Self {
        Self::at(Place::default(), cause)
    }

    pub(super) fn at(place: Place, cause: Cause) -> Self {
        Self(Box::new(Refused { place, cause }))
    }

    /// Where it is.
    #[must_use]
    pub fn place(&self) -> &Place {
        &self.0.place
    }

    /// Why it was refused.
    #[must_use]
    pub fn cause(&self) -> &Cause {
        &self.0.cause
    }

    pub(super) fn under(mut self, segment: Segment) -> Self {
        self.0.place = std::mem::take(&mut self.0.place).under(segment);
        self
    }

    /// The place and the cause, by value.
    pub(super) fn into_parts(self) -> (Place, Cause) {
        let Refused { place, cause } = *self.0;
        (place, cause)
    }

    pub(super) fn under_all(mut self, path: Vec<Segment>) -> Self {
        self.0.place = std::mem::take(&mut self.0.place).under_all(path);
        self
    }

    pub(super) fn in_step(mut self, index: usize, step: Option<StepId>) -> Self {
        self.0.place = std::mem::take(&mut self.0.place).in_step(index, step);
        self
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.0.place, self.0.cause)
    }
}

impl std::error::Error for Refusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0.cause)
    }
}

/// Why text could not be read as a document.
#[derive(Debug)]
pub enum ReadError {
    /// The text is not JSON by RFC 8259, repeats a key, or nests arrays and
    /// objects deeper than [`MAX_NESTING`](super::MAX_NESTING).
    Syntax(SyntaxError),
    /// The JSON is not a document the engine reads.
    Refused(Refusal),
}

impl fmt::Display for ReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(error) => write!(formatter, "the text is not a JSON document: {error}"),
            Self::Refused(refusal) => refusal.fmt(formatter),
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Syntax(error) => Some(error),
            Self::Refused(refusal) => Some(refusal),
        }
    }
}

impl From<Refusal> for ReadError {
    fn from(refusal: Refusal) -> Self {
        Self::Refused(refusal)
    }
}
