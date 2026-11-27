//! The notation dageist's steps are written in as text, without Polars: a subset of Python's
//! expressions, read into a tree whose every node knows its line and column, and printed back.
//!
//! The subset is what a chain of Polars calls needs, such as
//!
//! ```text
//! lf.with_columns(
//!     pl.col("ordered_at").shift(1).over(pl.col("customer_id")).alias("previous_order"),
//! )
//! ```
//!
//! bare names, numbers in decimal, strings in single or double quotes, raw strings, `True`,
//! `False`, `None`, `...`, lists, dicts, attributes and calls with arguments by position, by
//! name, `*` and `**`, and comments. What Python has beyond it is refused by name: an assignment,
//! a lambda, an import, a loop, a comprehension, an operator, a subscript, a condition. This crate
//! knows nothing of what the names mean; `dageist-text` reads a tree as a step document, against
//! the catalogue.
//!
//! [`parse`] reads a text as a [`Source`], or refuses it with a [`SyntaxError`] that says where
//! and why. [`print()`] writes an [`Expr`] as text that reads back as the same tree, laid out
//! within [`WIDTH`] characters a line where it can be.

#![forbid(unsafe_code)]

mod error;
mod lex;
mod parse;
mod print;
mod span;
mod tree;

pub use error::{MAX_DEPTH, SyntaxError, SyntaxProblem};
pub use parse::{KEYWORDS, parse};
pub use print::{WIDTH, print};
pub use span::{Position, Span};
pub use tree::{
    Argument, ArgumentKind, Chain, Comment, Expr, ExprKind, Link, Literal, LiteralValue, Source,
};

#[cfg(test)]
mod tests;
