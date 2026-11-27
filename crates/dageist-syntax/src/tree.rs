//! The tree the text reads as: expressions, the chains of attributes and calls that follow them,
//! and the arguments of a call, each with its span.

use crate::span::Span;

/// A text read whole: its one expression, and the comments that stand before no attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// The expression the text holds.
    pub expression: Expr,
    /// The comments that stand before something other than an attribute's dot, in the order
    /// they stand. A comment before a dot is the attribute's, in [`Link::Attribute`].
    pub comments: Vec<Comment>,
}

/// A comment: what follows a `#` to the end of its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// Where the comment stands, from its `#`.
    pub span: Span,
    /// What follows the `#`, as written, without the end of the line.
    pub text: String,
}

impl Comment {
    /// A comment of `text`, made in code. Printed, it is `#` and the text, on a line of its own.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            span: Span::default(),
            text: text.into(),
        }
    }
}

/// An expression, with the stretch of the text it was read from.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// Where the expression stands.
    pub span: Span,
    /// What it is.
    pub kind: ExprKind,
}

/// What an expression is.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// A bare name, such as `lf` or `limit`.
    Name(String),
    /// A number, text, `True`, `False` or `None`.
    Literal(Literal),
    /// `...`.
    Ellipsis,
    /// `[a, b]`.
    List(Vec<Expr>),
    /// `{key: value}`, each entry in the order written.
    Dict(Vec<(Expr, Expr)>),
    /// An expression followed by attributes and calls.
    Chain(Chain),
}

/// A literal, as written and as what it stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    /// The literal as it was written, which printing writes again: `r"\d+"`, `1_000`, `-2.5`.
    pub written: String,
    /// What it stands for.
    pub value: LiteralValue,
}

/// What a literal stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralValue {
    /// A whole number in decimal digits, with a `-` before them when it is negative and without
    /// the underscores it was written with. How large it may be is for the reader of the tree.
    Integer(String),
    /// A number with a fraction or an exponent, as Rust and Python both read it: digits, a point
    /// and an exponent, with a `-` before them when it is negative and without underscores.
    Float(String),
    /// Text, with its escapes read.
    String(String),
    /// `True`.
    True,
    /// `False`.
    False,
    /// `None`.
    None,
}

/// An expression and the attributes and calls that follow it, such as `pl.col("a").abs()`: the
/// head `pl`, then `.col`, `("a")`, `.abs` and `()`.
#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    /// What the chain starts with. It is never a chain itself: a chain in brackets that is
    /// followed by more is read as one chain.
    pub head: Box<Expr>,
    /// What follows the head, in order. There is at least one.
    pub links: Vec<Link>,
}

/// One attribute or call of a chain.
#[derive(Debug, Clone, PartialEq)]
pub enum Link {
    /// `.name`.
    Attribute {
        /// Where the dot stands.
        dot: Span,
        /// The name after the dot.
        name: String,
        /// Where the name stands.
        name_span: Span,
        /// The comments that stand between what comes before and the dot, in order.
        comments: Vec<Comment>,
    },
    /// `(arguments)`.
    Call {
        /// From the opening bracket to the closing one.
        span: Span,
        /// The arguments, in the order written.
        arguments: Vec<Argument>,
    },
}

impl Link {
    /// An attribute `.name`, made in code.
    #[must_use]
    pub fn attribute(name: impl Into<String>) -> Self {
        Self::Attribute {
            dot: Span::default(),
            name: name.into(),
            name_span: Span::default(),
            comments: Vec::new(),
        }
    }

    /// A call with `arguments`, made in code.
    #[must_use]
    pub fn call(arguments: Vec<Argument>) -> Self {
        Self::Call {
            span: Span::default(),
            arguments,
        }
    }
}

/// One argument of a call.
#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    /// Where the argument stands, its name or stars included.
    pub span: Span,
    /// What it is.
    pub kind: ArgumentKind,
}

/// How an argument is given.
#[derive(Debug, Clone, PartialEq)]
pub enum ArgumentKind {
    /// By position.
    Positional(Expr),
    /// By name: `name=value`.
    Keyword {
        /// The name.
        name: String,
        /// Where the name stands.
        name_span: Span,
        /// The value.
        value: Expr,
    },
    /// `*value`: the items of a list, each by position.
    Spread(Expr),
    /// `**value`: the entries of a dict, each by name.
    SpreadPairs(Expr),
}

impl Argument {
    /// An argument made in code.
    #[must_use]
    pub fn new(kind: ArgumentKind) -> Self {
        Self {
            span: Span::default(),
            kind,
        }
    }
}

impl Expr {
    /// An expression made in code, with the default span.
    #[must_use]
    pub fn new(kind: ExprKind) -> Self {
        Self {
            span: Span::default(),
            kind,
        }
    }

    /// The bare name `name`, made in code.
    #[must_use]
    pub fn name(name: impl Into<String>) -> Self {
        Self::new(ExprKind::Name(name.into()))
    }

    /// `head` followed by `links`, made in code. A head that is a chain is continued, and no links
    /// leave the head as it is.
    #[must_use]
    pub fn chain(head: Self, links: Vec<Link>) -> Self {
        if links.is_empty() {
            return head;
        }
        match head.kind {
            ExprKind::Chain(mut chain) => {
                chain.links.extend(links);
                Self::new(ExprKind::Chain(chain))
            }
            kind => Self::new(ExprKind::Chain(Chain {
                head: Box::new(Self::new(kind)),
                links,
            })),
        }
    }
}
