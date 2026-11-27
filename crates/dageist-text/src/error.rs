//! Why a text was not read as a document, and why a document was not written as text.

use std::fmt;

use dageist_engine::catalog::Reason;
use dageist_engine::document::{DtypeError, Place, Refusal};
use dageist_syntax::{Span, SyntaxProblem};

/// A text that was not read as a document: where, and why.
#[derive(Debug)]
pub struct ReadError {
    span: Span,
    problem: Problem,
}

impl ReadError {
    pub(crate) const fn new(span: Span, problem: Problem) -> Self {
        Self { span, problem }
    }

    /// The stretch of the text that is refused, by line and column.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Why it is refused.
    #[must_use]
    pub const fn problem(&self) -> &Problem {
        &self.problem
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.span, self.problem)
    }
}

impl std::error::Error for ReadError {}

/// Why a text was not read as a document.
#[derive(Debug)]
pub enum Problem {
    /// The text is not the notation: see [`dageist_syntax::parse`].
    Syntax(SyntaxProblem),
    /// The text does not start from `lf`, the table the steps run on.
    NotOnTheFrame,
    /// A step or a method named and not called, such as `lf.sort` or `.str` at the end.
    NotCalled,
    /// A call with no name before it, such as `abs(x)` or `f()()`.
    CallWithoutName,
    /// A name the catalogue has no entry for, as written: `lf.sorted`, `pl.colum`, `.str.lenght`.
    UnknownName(String),
    /// A Python Polars item the catalogue leaves out, by its qualified name, with why.
    LeftOut {
        /// The item's qualified name, such as `Expr.map_elements`.
        id: &'static str,
        /// Why the catalogue has no entry for it.
        reason: Reason,
    },
    /// More arguments by position than the call takes.
    TooManyArguments {
        /// The entry called.
        entry: &'static str,
        /// How many it takes by position.
        takes: usize,
    },
    /// A `*` or `**` where the call has no starred parameter for it, or before something that is
    /// not a list or a dict written out.
    Spread,
    /// Text where an expression goes. Python Polars reads it as a column in some methods and as a
    /// literal in others, so the text says which with `pl.col` or `pl.lit`.
    TextForAnExpression,
    /// `None` where an expression goes. Python Polars reads it as the parameter left out where the
    /// parameter defaults to `None`, and refuses it elsewhere, so the text leaves the argument out
    /// or writes `pl.lit(None)`.
    NoneForAnExpression,
    /// A literal followed by methods, such as `(1).abs()`, which Python calls on the number.
    MethodOfALiteral,
    /// A whole number no 64-bit integer holds, as written.
    NumberOutOfRange(String),
    /// A name that stands for something else where an input would: `lf`, `pl`, `dg` or a word of
    /// Python's own.
    ReservedName(String),
    /// What is written is not of the form the parameter holds.
    Form {
        /// What the parameter holds.
        expects: &'static str,
        /// What is written instead.
        found: &'static str,
    },
    /// A `# step:` comment where it names no step: anywhere but on the line before a step's dot.
    MisplacedStepName,
    /// Two `# step:` comments for one step.
    TwoStepNames,
    /// A `# step:` comment with no name after it.
    EmptyStepName,
    /// A dtype the document cannot hold, or one written in no form of Python Polars.
    Dtype(DtypeError),
    /// What the engine's constructors refuse, which an edit in the step editor is refused for too:
    /// an argument the catalogue does not take, a call that cannot stand where it is, a link of a
    /// condition out of its order, a step name given twice. The refusal's place is within the
    /// call, expression or document the constructor was making; the span says where in the text.
    Refused(Refusal),
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(problem) => write!(formatter, "{problem}"),
            Self::NotOnTheFrame => {
                formatter.write_str("the steps start from `lf`, the table they run on")
            }
            Self::NotCalled => formatter.write_str("a step or a method is called, with brackets"),
            Self::CallWithoutName => formatter
                .write_str("a call names what it calls: `pl.name(...)` or a method `.name(...)`"),
            Self::UnknownName(name) => write!(formatter, "the catalogue has no `{name}`"),
            Self::LeftOut { id, reason } => {
                write!(
                    formatter,
                    "the catalogue leaves out `{id}`: {}",
                    reason_words(*reason)
                )
            }
            Self::TooManyArguments { entry, takes } => write!(
                formatter,
                "{entry} takes {takes} argument{} by position",
                if *takes == 1 { "" } else { "s" }
            ),
            Self::Spread => formatter.write_str(
                "`*` stands only before a list for a starred parameter, and `**` only before a \
                 dict for a double-starred one",
            ),
            Self::TextForAnExpression => formatter.write_str(
                "text where an expression goes is a column in some Polars methods and a literal \
                 in others; write `pl.col(...)` or `pl.lit(...)`",
            ),
            Self::NoneForAnExpression => formatter.write_str(
                "`None` where an expression goes is the argument left out in some Polars methods; \
                 leave it out, or write `pl.lit(None)`",
            ),
            Self::MethodOfALiteral => formatter
                .write_str("a literal has no methods here; start the chain from `pl.lit(...)`"),
            Self::NumberOutOfRange(written) => {
                write!(
                    formatter,
                    "{written} is beyond what a whole number of 64 bits holds"
                )
            }
            Self::ReservedName(name) => write!(
                formatter,
                "`{name}` cannot name an input: it stands for something else in the text"
            ),
            Self::Form { expects, found } => {
                write!(formatter, "{expects} goes here, not {found}")
            }
            Self::MisplacedStepName => formatter
                .write_str("a `# step:` comment names the step whose dot is on the line after it"),
            Self::TwoStepNames => formatter.write_str("the step is named twice"),
            Self::EmptyStepName => formatter.write_str("a `# step:` comment names no step"),
            Self::Dtype(error) => write!(formatter, "{error}"),
            Self::Refused(refusal) => write!(formatter, "{}", refusal.cause()),
        }
    }
}

/// The catalogue's reason for leaving an item out, in words.
const fn reason_words(reason: Reason) -> &'static str {
    match reason {
        Reason::Deprecated => "Polars deprecates it",
        Reason::TakesPythonFunction => "it takes a Python function, which a step cannot hold",
        Reason::PythonObject => "it needs an object that exists in Python alone",
        Reason::NeverOffered => {
            "it is no operation a step uses, or Python's syntax for one that has its own entry"
        }
        Reason::NotYetCatalogued => "it is not catalogued yet",
    }
}

/// A document that was not written as text: a name in it that the text cannot hold bare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteError {
    place: Place,
    name: String,
}

impl WriteError {
    pub(crate) const fn new(place: Place, name: String) -> Self {
        Self { place, name }
    }

    /// Where the name stands in the document.
    #[must_use]
    pub const fn place(&self) -> &Place {
        &self.place
    }

    /// The name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: the input `{}` cannot be written bare: it is no Python name, or it is `lf`, `pl`, \
             `dg` or a word of Python's own",
            self.place, self.name
        )
    }
}

impl std::error::Error for WriteError {}
