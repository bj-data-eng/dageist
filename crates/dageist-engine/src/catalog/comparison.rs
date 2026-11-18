//! The comparisons a test makes of a field and a value.
//!
//! Each is an entry of the catalogue, a method of an expression that takes what
//! the field is compared with as `other`, and each has a sign that the language
//! file gives, as summaries write it and as the editor shows it.

use super::words::SummaryPhrase;

/// A comparison of a field with a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Comparison {
    /// Equal to: `Expr.eq`.
    Equal,
    /// Not equal to: `Expr.ne`.
    NotEqual,
    /// Greater than: `Expr.gt`.
    Greater,
    /// At least: `Expr.ge`.
    AtLeast,
    /// Less than: `Expr.lt`.
    Less,
    /// At most: `Expr.le`.
    AtMost,
}

impl Comparison {
    /// Every comparison, in the order they are offered.
    pub const ALL: [Self; 6] = [
        Self::Equal,
        Self::NotEqual,
        Self::Greater,
        Self::AtLeast,
        Self::Less,
        Self::AtMost,
    ];

    /// The identifier of the entry that makes the comparison.
    #[must_use]
    pub const fn entry(self) -> &'static str {
        match self {
            Self::Equal => "Expr.eq",
            Self::NotEqual => "Expr.ne",
            Self::Greater => "Expr.gt",
            Self::AtLeast => "Expr.ge",
            Self::Less => "Expr.lt",
            Self::AtMost => "Expr.le",
        }
    }

    /// The comparison the entry with identifier `id` makes, or `None` where it
    /// makes none.
    #[must_use]
    pub fn of_entry(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|comparison| comparison.entry() == id)
    }

    /// The phrase of the language file that is the comparison's sign.
    #[must_use]
    pub const fn phrase(self) -> SummaryPhrase<'static> {
        match self {
            Self::Equal => SummaryPhrase::Equal,
            Self::NotEqual => SummaryPhrase::NotEqual,
            Self::Greater => SummaryPhrase::Greater,
            Self::AtLeast => SummaryPhrase::AtLeast,
            Self::Less => SummaryPhrase::Less,
            Self::AtMost => SummaryPhrase::AtMost,
        }
    }
}
