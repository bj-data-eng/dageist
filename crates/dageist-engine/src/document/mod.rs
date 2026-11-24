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
//! memory, and [`Document::preview`] gives them as text. [`Document::run_on`]
//! runs the document on such a table and also counts, for each step, the rows
//! it kept of those it received, with the first values of the columns it added
//! or replaced; each [`Outcome::Valid`] says which columns those are.
//!
//! # Editing
//!
//! Each edit takes a document and gives the edited document, or an [`EditError`]
//! that says why it could not.
//!
//! Every edit is made on the model, not on text: the calls, expressions and
//! steps on the way to what is edited are made again with their
//! constructors, so that an edit is checked as it is made, as reading checks
//! a document. The edited document is as the reader reads it: writing it with
//! [`Document::write`] and reading that gives the same document. A document
//! is edited, never changed in place; an edit that is refused leaves the
//! document as it was.
//!
//! Whether the steps run is not decided by an edit. It may leave a step
//! incomplete, with a hole where an expression is still to be picked, or
//! refused, with an argument left out that Polars needs; running the document
//! says so.
//!
//! An edit names where it is made by a [`Place`], as reports and offers name
//! places: a step by its number and, if it has one, its identifier, which an
//! edit checks against the step, so that a place made for an earlier document
//! is refused and not applied to another step; then the way down through the
//! step's arguments. The segments mean:
//!
//! - [`Segment::Parameter`]: the argument given for a parameter of the call
//!   the way has come to, which is the step's own at first;
//! - [`Segment::Item`]: an item of a list;
//! - [`Segment::Key`] and [`Segment::Value`]: the key or the value of an entry
//!   of pairs;
//! - [`Segment::Start`] and [`Segment::Link`]: the call an expression starts
//!   with, and one of the links after it.
//!
//! | Edit | Methods |
//! | --- | --- |
//! | The steps | [`add_step`](Document::add_step), [`swap_step`](Document::swap_step), [`remove_step`](Document::remove_step), [`move_step`](Document::move_step) |
//! | An argument | [`set_argument`](Document::set_argument), [`clear_argument`](Document::clear_argument), [`add_option`](Document::add_option), [`set_for_item`](Document::set_for_item) |
//! | Lists and pairs | [`insert_item`](Document::insert_item), [`remove_item`](Document::remove_item), [`insert_test`](Document::insert_test), [`insert_entry`](Document::insert_entry), [`remove_entry`](Document::remove_entry) |
//! | An expression | [`set_start`](Document::set_start), [`start_with`](Document::start_with), [`insert_link`](Document::insert_link), [`swap_link`](Document::swap_link), [`remove_link`](Document::remove_link), [`move_link`](Document::move_link) |
//!
//! What the engine decides a new part starts with, a step, a link, an option
//! or a test, comes from the catalogue's starts and the tables of allowed
//! combinations, so no host or view repeats it. [`Expression::hole`],
//! [`Expression::field`], [`Expression::literal`] and [`Expression::input`] make
//! the expressions of one word that an edit puts in a slot, and
//! [`Value::from_literal`] a typed literal as the argument of a parameter.
//!
//! The JSON is read by RFC 8259, strictly: a number keeps the value its digits
//! name, so a float reads back as the `f64` it was written from.
//!
//! The version of the format is [`VERSION`]. A document of any other version is
//! refused; there is no migration before the core API reaches 1.0.

mod changes;
mod columns;
mod counts;
mod defaults;
mod dtype;
mod edit;
mod error;
mod inputs;
mod json;
mod model;
mod preview;
mod read;
mod run;
mod summary;
#[cfg(test)]
mod tests;
mod values;
mod walk;
mod wording;
mod write;

pub use changes::{Change, ColumnChange};
pub use columns::ColumnDescription;
pub use counts::{ColumnValues, Counted, StepRows};
pub use dtype::{Dtype, DtypeError};
pub use edit::{EditError, EditProblem, Thing, ValueKind, Wanted};
pub use error::{Cause, Place, Position, ReadError, Refusal, Segment};
pub use inputs::{DuplicateInput, InputDescription, InputKind, Inputs, ListOrigin, Origin};
pub(crate) use json::{NotWritten, WrittenNumber, read_number};
pub use json::{SyntaxError, SyntaxProblem};
pub use model::{Call, Document, EmptyStepId, Expression, Given, Start, Step, StepId, Value};
pub use preview::Preview;
pub use run::{
    ExpressionDtypes, Outcome, Report, Resolution, RowsError, RunCause, StepRefusal, StepReport,
};
pub use values::{FIRST_VALUES, FirstValues};
pub use walk::{Standing, Walk, WalkRefusal, WalkedExpression};
pub(crate) use wording::float;

/// The version of the document format this engine reads and writes.
pub const VERSION: u32 = 1;

/// How many arrays and objects may be open inside each other in a document's
/// JSON text, counting the document's own object as the first.
///
/// [`Document::read`] refuses text nested deeper, and [`Document::new`]
/// refuses steps that would be written nested deeper, so whatever
/// [`Document::write`] gives, [`Document::read`] reads.
pub const MAX_NESTING: usize = 128;
