//! Why a text was not read, and where.

use std::fmt;

use crate::span::Span;

/// How deep brackets may be nested inside each other: the brackets of calls, lists, dicts and
/// grouping. A text nested deeper is refused, so that reading, and whatever walks the tree, never
/// recurses further.
pub const MAX_DEPTH: usize = 128;

/// A text that was not read: where, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    span: Span,
    problem: SyntaxProblem,
}

impl SyntaxError {
    pub(crate) const fn new(span: Span, problem: SyntaxProblem) -> Self {
        Self { span, problem }
    }

    /// The stretch of the text that is refused.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Why it is refused.
    #[must_use]
    pub const fn problem(&self) -> &SyntaxProblem {
        &self.problem
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.span, self.problem)
    }
}

impl std::error::Error for SyntaxError {}

/// Why a text was not read. The notation is a subset of Python's expressions: names, literals,
/// lists, dicts, attributes and calls. What Python has beyond them is refused by what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxProblem {
    /// The text holds no expression.
    Empty,
    /// A character that no token starts with.
    UnexpectedCharacter(char),
    /// A string not closed on its line.
    UnterminatedString,
    /// A string in triple quotes.
    TripleQuoted,
    /// A string with a prefix other than `r` or `u`, such as an f-string or bytes.
    StringPrefix(String),
    /// An escape no string holds, as written.
    BadEscape(String),
    /// A number not written in decimal, as written: `0x1f`, `1j`, `01`.
    BadNumber(String),
    /// A word of Python's own that the notation does not have, such as `lambda`, `import`, `for`
    /// or `if`.
    Keyword(String),
    /// An assignment, `=`, `:=` or one such as `+=`.
    Assignment,
    /// A comprehension or generator: `for` inside brackets.
    Comprehension,
    /// An operator, such as `+`, `==` or `&`.
    Operator(String),
    /// A subscript: `[` after an expression.
    Subscript,
    /// More than one expression, on lines of their own or after `;`.
    MoreThanOneStatement,
    /// A token where another was needed.
    Unexpected {
        /// What was found, in words.
        found: String,
        /// What was needed, in words.
        expected: &'static str,
    },
    /// A bracket that is not closed.
    Unclosed(char),
    /// An argument by position after an argument by name or a `**`, or a `*` after a `**`, which
    /// Python refuses.
    PositionalAfterKeyword,
    /// An argument by name given twice in one call, which Python refuses.
    RepeatedKeyword(String),
    /// Brackets nested more than [`MAX_DEPTH`] deep.
    TooDeep,
}

impl fmt::Display for SyntaxProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("the text holds no expression"),
            Self::UnexpectedCharacter(character) => {
                write!(formatter, "the character {character:?} cannot stand here")
            }
            Self::UnterminatedString => formatter.write_str("the string is not closed on its line"),
            Self::TripleQuoted => formatter.write_str(
                "a string in triple quotes is not read; write it in single or double quotes",
            ),
            Self::StringPrefix(prefix) => write!(
                formatter,
                "a string with the prefix `{prefix}` is not read; only `r` and `u` are"
            ),
            Self::BadEscape(escape) => {
                write!(formatter, "`{escape}` is not an escape a string can hold")
            }
            Self::BadNumber(written) => write!(
                formatter,
                "`{written}` is not a number the text reads; write it in decimal digits"
            ),
            Self::Keyword(word) => match word.as_str() {
                "lambda" => formatter.write_str("a lambda is not read; a step takes no function"),
                "import" | "from" => formatter.write_str("an import is not read"),
                "for" | "while" | "async" => formatter.write_str("a loop is not read"),
                "def" | "class" => formatter.write_str("a definition is not read"),
                "if" | "else" | "elif" => formatter.write_str(
                    "a Python condition is not read; write `pl.when(...)` for a condition",
                ),
                "and" | "or" | "not" | "in" | "is" => write!(
                    formatter,
                    "the operator `{word}` is not read; write the method it stands for"
                ),
                _ => write!(formatter, "Python's `{word}` is not read"),
            },
            Self::Assignment => formatter.write_str("an assignment is not read"),
            Self::Comprehension => {
                formatter.write_str("a comprehension is not read; write the items out")
            }
            Self::Operator(operator) => write!(
                formatter,
                "the operator `{operator}` is not read; write the method it stands for, \
                 such as `.add(...)`"
            ),
            Self::Subscript => {
                formatter.write_str("a subscript is not read; write the method it stands for")
            }
            Self::MoreThanOneStatement => {
                formatter.write_str("the text holds more than one expression")
            }
            Self::Unexpected { found, expected } => {
                write!(formatter, "{expected} was expected, not {found}")
            }
            Self::Unclosed(bracket) => write!(formatter, "the `{bracket}` here is not closed"),
            Self::PositionalAfterKeyword => formatter
                .write_str("an argument by position cannot follow an argument by name or a `**`"),
            Self::RepeatedKeyword(name) => {
                write!(formatter, "the argument `{name}` is given twice by name")
            }
            Self::TooDeep => write!(
                formatter,
                "brackets are nested more than {MAX_DEPTH} deep inside each other"
            ),
        }
    }
}
