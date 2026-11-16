//! The step document: the steps of a transform, saved as versioned JSON and
//! built through the catalogue.
//!
//! A [`Document`] is a list of steps. Each step is a call of a method of a lazy
//! frame, such as `LazyFrame.with_columns`, with arguments for its parameters,
//! and builds on the table the step before it gives. An argument that is an
//! expression holds a chain: where it starts, then the methods and the links
//! of a condition that follow.
//!
//! [`Document::read`] is strict and names the place of every refusal, down to
//! the link of an expression; [`Document::write`] gives text that reads back as
//! the same document. [`Document::run`] builds the steps in Polars without
//! reading data and reports for each its schema and the dtype after every link
//! of its expressions, or why Polars refused it, and for each expression the
//! context it stands in and what each of its links follows, which
//! [`Step::walk`] finds from the tables of allowed combinations alone.
//! [`Document::rows`] collects the rows after a step from a table held in
//! memory.
//!
//! The JSON is read by RFC 8259, strictly: a number keeps the value its digits
//! name, so a float reads back as the `f64` it was written from.
//!
//! The version of the format is [`VERSION`]. A document of any other version is
//! refused; there is no migration before the core API reaches 1.0.

mod dtype;
mod error;
mod inputs;
mod json;
mod model;
mod read;
mod run;
#[cfg(test)]
mod tests;
mod walk;
mod write;

pub use dtype::{Dtype, DtypeError};
pub use error::{Cause, Place, Position, ReadError, Refusal, Segment};
pub use inputs::{DuplicateInput, Inputs};
pub use json::{SyntaxError, SyntaxProblem};
pub use model::{Call, Document, EmptyStepId, Expression, Given, Start, Step, StepId, Value};
pub use run::{
    ExpressionDtypes, Outcome, Report, Resolution, RowsError, RunCause, StepRefusal, StepReport,
};
pub use walk::{Evaluation, Standing, Walk, WalkRefusal, WalkedExpression};

/// The version of the document format this engine reads and writes.
pub const VERSION: u32 = 1;

/// How many arrays and objects may be open inside each other in a document's
/// JSON text, counting the document's own object as the first.
///
/// [`Document::read`] refuses text nested deeper, and [`Document::new`]
/// refuses steps that would be written nested deeper, so whatever
/// [`Document::write`] gives, [`Document::read`] reads.
pub const MAX_NESTING: usize = 128;
