//! Where each part of a call that was read stands in the text, so that a refusal whose place the
//! engine names, down to an item, an entry or a link, is given the span of that part.

use dageist_engine::document::Segment;
use dageist_syntax::Span;

/// The spans of a call: its name, and the arguments by parameter.
#[derive(Debug, Clone)]
pub(crate) struct CallSpans {
    pub name: Span,
    pub parameters: Vec<(&'static str, ValueSpans)>,
}

/// The span of an argument, and of its parts.
#[derive(Debug, Clone)]
pub(crate) struct ValueSpans {
    pub span: Span,
    pub parts: Parts,
}

/// The parts of an argument that a place can name.
#[derive(Debug, Clone)]
pub(crate) enum Parts {
    None,
    Items(Vec<ValueSpans>),
    Pairs(Vec<(ValueSpans, ValueSpans)>),
    Expression(ExpressionSpans),
}

/// The spans of an expression: its start, a call or one word, and its links.
#[derive(Debug, Clone)]
pub(crate) struct ExpressionSpans {
    pub start: Start,
    pub links: Vec<CallSpans>,
}

/// The span of an expression's start.
#[derive(Debug, Clone)]
pub(crate) enum Start {
    Call(CallSpans),
    Word(Span),
}

impl ValueSpans {
    pub(crate) const fn leaf(span: Span) -> Self {
        Self {
            span,
            parts: Parts::None,
        }
    }
}

impl CallSpans {
    /// The span of what `path` names inside the call: as deep as the path can be followed, and
    /// the call's name where it names nothing in it.
    pub(crate) fn locate(&self, path: &[Segment]) -> Span {
        let Some((Segment::Parameter(name), rest)) = path.split_first() else {
            return self.name;
        };
        // A parameter given twice is refused at its later argument.
        self.parameters
            .iter()
            .rev()
            .find(|(parameter, _)| parameter == name)
            .map_or(self.name, |(_, value)| value.locate(rest))
    }
}

impl ValueSpans {
    fn locate(&self, path: &[Segment]) -> Span {
        let Some((segment, rest)) = path.split_first() else {
            return self.span;
        };
        let found = match (segment, &self.parts) {
            (Segment::Item(at), Parts::Items(items)) => {
                items.get(*at).map(|item| item.locate(rest))
            }
            (Segment::Key(at), Parts::Pairs(pairs)) => {
                pairs.get(*at).map(|(key, _)| key.locate(rest))
            }
            (Segment::Value(at), Parts::Pairs(pairs)) => {
                pairs.get(*at).map(|(_, value)| value.locate(rest))
            }
            (_, Parts::Expression(expression)) => Some(expression.locate(path)),
            _ => None,
        };
        found.unwrap_or(self.span)
    }
}

impl ExpressionSpans {
    /// The span of what `path` names inside the expression.
    pub(crate) fn locate(&self, path: &[Segment]) -> Span {
        match path.split_first() {
            Some((Segment::Start, rest)) => match &self.start {
                Start::Call(call) => call.locate(rest),
                Start::Word(span) => *span,
            },
            Some((Segment::Link(at), rest)) => self
                .links
                .get(*at)
                .map_or_else(|| self.first(), |link| link.locate(rest)),
            _ => self.first(),
        }
    }

    fn first(&self) -> Span {
        match &self.start {
            Start::Call(call) => call.name,
            Start::Word(span) => *span,
        }
    }
}
