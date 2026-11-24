//! What is known of how many values a chain gives, where the data or the arguments decide.

use std::fmt;

/// What is known of how many values a chain of the shape [`Shape::Counted`](super::Shape::Counted) gives.
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
    /// What two counts that an operation meets give, where they are the same number: the one
    /// both fix, and what is known of either where the other is not. `None` where they cannot be
    /// the same, which two counts that are fixed and differ are not, nor a count and a bound that
    /// it exceeds.
    #[must_use]
    pub const fn meeting(self, other: Self) -> Option<Self> {
        match (self, other) {
            (Self::Exactly(first), Self::Exactly(second)) => {
                if first == second {
                    Some(self)
                } else {
                    None
                }
            }
            (Self::Exactly(exactly), Self::AtMost(most))
            | (Self::AtMost(most), Self::Exactly(exactly)) => {
                if exactly <= most {
                    Some(Self::Exactly(exactly))
                } else {
                    None
                }
            }
            (Self::AtMost(first), Self::AtMost(second)) => {
                Some(Self::AtMost(if first < second { first } else { second }))
            }
            (Self::NotKnown, known) | (known, Self::NotKnown) => Some(known),
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

#[cfg(test)]
mod tests {
    use super::Count;

    #[test]
    fn two_counts_that_meet_are_the_number_they_both_fix() {
        use Count::{AtMost, Exactly, NotKnown};
        let cases = [
            (Exactly(2), Exactly(2), Some(Exactly(2))),
            (Exactly(2), Exactly(3), None),
            (Exactly(2), AtMost(3), Some(Exactly(2))),
            (Exactly(3), AtMost(2), None),
            (AtMost(3), AtMost(2), Some(AtMost(2))),
            (NotKnown, Exactly(4), Some(Exactly(4))),
            (AtMost(4), NotKnown, Some(AtMost(4))),
            (NotKnown, NotKnown, Some(NotKnown)),
        ];
        for (first, second, met) in cases {
            assert_eq!(first.meeting(second), met, "{first:?} with {second:?}");
            assert_eq!(second.meeting(first), met, "{second:?} with {first:?}");
        }
    }
}
