//! A condition under construction: `when`, `then` and the links that follow.
//!
//! Python writes `when(...).then(...).when(...).then(...).otherwise(...)`, and
//! each call gives an object that only the next valid call can follow:
//! `When`, `Then`, `ChainedWhen` and `ChainedThen`. A saved step holds one
//! link at a time, so the links are held as one value whose state the next
//! link checks, instead of a type for each state.

use polars::prelude::{ChainedThen, ChainedWhen, Expr, NULL, Then, When, lit};

use super::BuildError;

/// One state of a `when-then-otherwise` expression.
#[derive(Clone)]
pub enum Conditional {
    /// `when(...)`: a condition that still needs its `then`.
    When(When),
    /// `when(...).then(...)`: a complete expression, null where no condition
    /// holds, which `otherwise` may give another value.
    Then(Then),
    /// `...then(...).when(...)`: a further condition that needs its `then`.
    ChainedWhen(ChainedWhen),
    /// `...when(...).then(...)`: like [`Conditional::Then`], with more
    /// conditions.
    ChainedThen(ChainedThen),
}

impl std::fmt::Debug for Conditional {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::When(_) => "Conditional::When",
            Self::Then(_) => "Conditional::Then",
            Self::ChainedWhen(_) => "Conditional::ChainedWhen",
            Self::ChainedThen(_) => "Conditional::ChainedThen",
        })
    }
}

impl Conditional {
    /// The state's name as Python's classes name it, such as `then`.
    pub(super) const fn state(&self) -> &'static str {
        match self {
            Self::When(_) => "when",
            Self::Then(_) => "then",
            Self::ChainedWhen(_) => "chained when",
            Self::ChainedThen(_) => "chained then",
        }
    }

    /// The error for a link that needs the state `expected` before it.
    pub(super) const fn out_of_order(&self, expected: &'static str) -> BuildError {
        BuildError::OutOfOrder {
            expected,
            found: self.state(),
        }
    }

    /// The expression, as Python's `Then` and `ChainedThen` are expressions:
    /// the value of the first `then` whose `when` holds, and null where none
    /// does, as if it ended in `otherwise(None)`.
    ///
    /// # Errors
    ///
    /// When it ends in a `when`, which has no value until it has its `then`.
    pub fn into_expr(self) -> Result<Expr, BuildError> {
        match self {
            Self::Then(then) => Ok(then.otherwise(lit(NULL))),
            Self::ChainedThen(then) => Ok(then.otherwise(lit(NULL))),
            other @ (Self::When(_) | Self::ChainedWhen(_)) => Err(other.out_of_order("then")),
        }
    }
}
