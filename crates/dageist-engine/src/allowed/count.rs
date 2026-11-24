//! What is known of how many values a chain gives, where the data or the arguments decide.

use std::fmt;

/// What is known of how many values a chain of the shape [`Shape::Counted`] gives.
///
/// The count is for the data and the arguments to say, and the walk states it only where the
/// arguments fix it: `head(2)` of a column gives two values or fewer, a range from its bounds
/// gives as many as they and its step fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Count {
    /// Exactly this many.
    Exactly(u64),
    /// This many or fewer.
    AtMost(u64),
    /// How many is not known: the data decide.
    NotKnown,
}

impl Count {
    /// Whether the count can be that of another: two counts that are both fixed and differ never
    /// are, and one that is not known may be any.
    #[must_use]
    pub const fn may_equal(self, other: Self) -> bool {
        match (self, other) {
            (Self::Exactly(first), Self::Exactly(second)) => first == second,
            (Self::Exactly(exactly), Self::AtMost(most))
            | (Self::AtMost(most), Self::Exactly(exactly)) => exactly <= most,
            _ => true,
        }
    }

    /// The count of what two values of these counts give where an operation meets them: the same
    /// where they are the same fixed number, and what is known of both otherwise.
    #[must_use]
    pub const fn meeting(self, other: Self) -> Self {
        match (self, other) {
            (Self::Exactly(first), Self::Exactly(second)) if first == second => self,
            (Self::Exactly(exactly), Self::AtMost(_))
            | (Self::AtMost(_), Self::Exactly(exactly)) => Self::Exactly(exactly),
            (Self::AtMost(first), Self::AtMost(second)) => {
                Self::AtMost(if first < second { first } else { second })
            }
            _ => Self::NotKnown,
        }
    }

    /// The most values it can give, where that is known.
    #[must_use]
    pub const fn most(self) -> Option<u64> {
        match self {
            Self::Exactly(count) | Self::AtMost(count) => Some(count),
            Self::NotKnown => None,
        }
    }
}

impl fmt::Display for Count {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exactly(count) => write!(formatter, "{count}"),
            Self::AtMost(count) => write!(formatter, "at most {count}"),
            Self::NotKnown => formatter.write_str("a number the data decide"),
        }
    }
}
