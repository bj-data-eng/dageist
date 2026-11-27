//! Where a piece of the text stands: a place in it, and the stretch between two places.

use std::fmt;

/// A place in the text, before one of its characters or after the last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Position {
    line: usize,
    column: usize,
    offset: usize,
}

impl Position {
    /// The place at `line` and `column`, both counted from 1, and `offset` bytes into the text.
    #[must_use]
    pub const fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }

    /// The line, counted from 1. A tree made in code, not read, has 0.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// The column, counted from 1 in characters, not bytes.
    #[must_use]
    pub const fn column(&self) -> usize {
        self.column
    }

    /// How many bytes of the text come before the place.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for Position {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}, column {}", self.line, self.column)
    }
}

/// A stretch of the text: from `start` up to `end`, which is the place after its last character.
/// A node made in code, not read, has the default span, at line 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    start: Position,
    end: Position,
}

impl Span {
    /// The stretch from `start` up to `end`.
    #[must_use]
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// Where the stretch starts.
    #[must_use]
    pub const fn start(&self) -> Position {
        self.start
    }

    /// The place after the stretch's last character.
    #[must_use]
    pub const fn end(&self) -> Position {
        self.end
    }

    /// The stretch from the start of `self` to the end of `other`.
    #[must_use]
    pub const fn to(self, other: Self) -> Self {
        Self {
            start: self.start,
            end: other.end,
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start.line == self.end.line {
            write!(
                formatter,
                "line {}, columns {} to {}",
                self.start.line, self.start.column, self.end.column
            )
        } else {
            write!(formatter, "{} to {}", self.start, self.end)
        }
    }
}
