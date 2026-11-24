//! How a method of an expression decides how many values it gives, where that is neither one
//! value nor as many as the table has rows.
//!
//! The tables of allowed combinations measure a call on three rows and cannot say a count such as
//! the first two values, or the values of a column that differ, for the count is what the data and
//! the arguments make it, and three rows decide it by chance. The catalogue says which methods
//! decide it, and by what; the walk works the count out of the arguments a call is given.

use super::Entry;

/// What decides how many values a method gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counting {
    /// The first or last values, as many as the count the named parameter holds, or fewer where
    /// the value it follows has fewer: `head`, `tail`, `limit`, `top_k` and `bottom_k`.
    AtMost(&'static str),
    /// The values from a place, for a length: `slice`. Without a length, every value after the
    /// place.
    FromAPlace,
    /// Every nth value from a place: `gather_every`.
    EveryNth,
    /// The values the data decide: the different ones, the ones that occur most often, their
    /// places or their counts. At most as many as the value it follows has.
    TheData,
    /// The values where a condition holds, as many as the condition's values that are true: the
    /// predicate meets even one value with each of its own.
    TheCondition,
    /// As many as the values of an argument: the places `gather` takes, the elements that
    /// `search_sorted` looks for, one place for each.
    TheArgument(&'static str),
    /// As many as the value it follows has less the rows the first values it drops take:
    /// `diff` that is told to drop what has no value before it.
    TheRowsLess(&'static str),
}

impl Counting {
    /// What decides how many values the call of `entry` gives, with the options it is given set
    /// as `(parameter, word)` pairs, if the count is not one value or the table's rows.
    #[must_use]
    pub fn of(entry: &Entry, options: &[(&str, &str)]) -> Option<Self> {
        Some(match entry.id().as_str() {
            "Expr.head" | "Expr.limit" | "Expr.tail" => Self::AtMost("n"),
            "Expr.top_k" | "Expr.bottom_k" => Self::AtMost("k"),
            "Expr.slice" => Self::FromAPlace,
            "Expr.gather_every" => Self::EveryNth,
            "Expr.unique" | "Expr.mode" | "Expr.arg_unique" | "Expr.unique_counts" => Self::TheData,
            "Expr.filter" => Self::TheCondition,
            "Expr.gather" => Self::TheArgument("indices"),
            "Expr.search_sorted" => Self::TheArgument("element"),
            "Expr.diff"
                if options
                    .iter()
                    .any(|(name, word)| *name == "null_behavior" && *word == "drop") =>
            {
                Self::TheRowsLess("n")
            }
            _ => return None,
        })
    }

    /// Whether the method gives one value of one value, as the first value of one is: it takes
    /// some of the values of what it follows. The values where a condition holds are not, for
    /// the condition meets the one value with each of its own.
    #[must_use]
    pub const fn keeps_one_value(self) -> bool {
        !matches!(self, Self::TheCondition | Self::TheArgument(_))
    }
}
