//! A step document as Polars-style text: [`write()`] gives the text of a document, and [`read`]
//! reads a text back, strictly, as the same document.
//!
//! The text is a chain of Polars calls on `lf`, the table the steps run on, one step to a line:
//!
//! ```text
//! (
//!     lf
//!     .with_columns(
//!         pl.col("ordered_at").shift(1).over(pl.col("customer_id")).alias("previous_order"),
//!     )
//!     # step: latest
//!     .sort(pl.col("ordered_at"), descending=True)
//! )
//! ```
//!
//! It is the notation of [`dageist_syntax`], read against the catalogue:
//!
//! - **Names.** A step is a method of `LazyFrame` after `lf`; `pl.<name>(…)` is a function of
//!   `polars`, and `dg.<name>(…)` one of dageist's own, `dageist.<name>`; a method is one of the
//!   class its chain has come to, `Expr` or a stage of a condition, its namespace included:
//!   `.str.contains(…)` is `Expr.str.contains`, and `.then(…)` after `pl.when(…)` is
//!   `When.then`. A name the catalogue leaves out is refused with the reason it records.
//! - **Arguments** bind to the entry's parameters as Python binds them: by position in order, by
//!   name, the rest by position to a starred parameter, and names no parameter has to a
//!   double-starred one. `*[…]` and `**{…}` give a starred parameter its items and a
//!   double-starred one its entries written out. Where an entry has no double-starred parameter,
//!   a starred one may also be given by name, as `more_by=[…]`, which Python does not read: the
//!   text needs it for a document that gives a starred parameter but not a parameter before it,
//!   whose items Python would bind to that parameter. Arguments keep the order written, which is
//!   the order the document holds them in.
//! - **Values** are read as the parameter's kind takes them: a string is a column's name, a
//!   choice, a pattern, a duration, a time zone, a path or text, as the kind says; a dtype is
//!   written as Python Polars writes it, `pl.Int64` or `pl.List(pl.String)`; a parameter of many
//!   takes a list, or one item for a list of one.
//! - **Expressions** start from `pl.<function>(…)`, from a connected input by its bare name, or
//!   from `...`, a hole the editor has not filled. A number, `True` or `False` alone is `pl.lit`
//!   of it. Text alone is refused, as Python Polars reads it as a column in some methods and a
//!   literal in others; the text writes `pl.col(…)` or `pl.lit(…)`. `None` alone is refused too,
//!   as Python Polars reads it as the argument left out where its parameter defaults to `None`.
//! - **Step names.** A step read without a name is named as the step editor names a step it
//!   adds: the entry's name, then `_2`, `_3` and on. A comment `# step: <name>` on the line before
//!   a step's dot names it otherwise, so that a step keeps its name, and its history, when the
//!   text is read back.
//!
//! Every call, expression, step and the document are made with the constructors the step editor's
//! edits use, so that the text is refused for what an edit is refused for, at the span of the part
//! of the text refused. Nothing outside the notation is read: no assignment, loop, import, lambda
//! or comprehension, and no operator: an operator's method is written instead.

#![forbid(unsafe_code)]

mod error;
mod names;
mod read;
mod spans;
mod write;

pub use error::{Problem, ReadError, WriteError};
pub use read::read;
pub use write::write;
