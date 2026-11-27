//! Reading the tokens as a tree.

use crate::error::{MAX_DEPTH, SyntaxError, SyntaxProblem};
use crate::lex::{Token, TokenKind, tokens};
use crate::span::Span;
use crate::tree::{
    Argument, ArgumentKind, Chain, Comment, Expr, ExprKind, Link, Literal, LiteralValue, Source,
};

/// Python's keywords. `True`, `False` and `None` are literals; any other is refused where a name
/// could stand.
pub const KEYWORDS: [&str; 35] = [
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// Reads `text` as one expression.
///
/// The notation is a subset of Python's expressions: bare names, numbers in decimal, strings in
/// single or double quotes (raw ones too), `True`, `False`, `None`, `...`, lists, dicts,
/// attributes and calls, with arguments by position, by name, `*` and `**`. A `-` may stand
/// before a number. Brackets group, and a line ends the expression only outside brackets or after
/// a `\`. Comments are kept: one before an attribute's dot is the attribute's, and any other is
/// in [`Source::comments`].
///
/// # Errors
///
/// When the text is not one expression of the notation: the error says where, and why, naming
/// what Python has that the notation does not, such as an assignment, a lambda or an operator.
pub fn parse(text: &str) -> Result<Source, SyntaxError> {
    let tokens = tokens(text)?;
    let mut parser = Parser {
        tokens,
        at: 0,
        depth: 0,
        loose: Vec::new(),
    };
    parser.skip_newlines();
    if parser.peek() == &TokenKind::End {
        let span = parser.tokens[parser.at].span;
        return Err(SyntaxError::new(span, SyntaxProblem::Empty));
    }
    let expression = parser.expression()?;
    match parser.peek() {
        TokenKind::Newline => {
            parser.skip_newlines();
            if parser.peek() != &TokenKind::End {
                return Err(parser.refuse_here(SyntaxProblem::MoreThanOneStatement));
            }
        }
        TokenKind::End => {}
        TokenKind::Semicolon => {
            return Err(parser.refuse_here(SyntaxProblem::MoreThanOneStatement));
        }
        TokenKind::Equals => return Err(parser.refuse_here(SyntaxProblem::Assignment)),
        _ => return Err(parser.after_expression("the end of the line")),
    }
    let end = parser.bump();
    parser.loose.extend(end.comments);
    Ok(Source {
        expression,
        comments: parser.loose,
    })
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
    loose: Vec<Comment>,
}

/// Whether `operator` assigns, as `+=` and `:=` do.
fn assigns(operator: &str) -> bool {
    operator == ":=" || (operator.ends_with('=') && !matches!(operator, "==" | "<=" | ">=" | "!="))
}

impl Parser {
    fn peek(&self) -> &TokenKind {
        &self.tokens[self.at].kind
    }

    fn peek_second(&self) -> &TokenKind {
        let at = (self.at + 1).min(self.tokens.len() - 1);
        &self.tokens[at].kind
    }

    fn span(&self) -> Span {
        self.tokens[self.at].span
    }

    /// The next token, with its comments. The caller keeps them or puts them with the loose ones.
    fn take(&mut self) -> Token {
        let token = self.tokens[self.at].clone();
        if self.at + 1 < self.tokens.len() {
            self.at += 1;
        }
        token
    }

    /// The next token, its comments put with the loose ones.
    fn bump(&mut self) -> Token {
        let mut token = self.take();
        self.loose.append(&mut token.comments);
        token
    }

    fn skip_newlines(&mut self) {
        while self.peek() == &TokenKind::Newline {
            self.bump();
        }
    }

    fn refuse_here(&self, problem: SyntaxProblem) -> SyntaxError {
        SyntaxError::new(self.span(), problem)
    }

    /// The refusal for the token after an expression, where `expected` was needed: an operator, a
    /// keyword or an assignment is named as such.
    fn after_expression(&self, expected: &'static str) -> SyntaxError {
        let problem = match self.peek() {
            TokenKind::Operator(operator) if assigns(operator) => SyntaxProblem::Assignment,
            TokenKind::Operator(operator) => SyntaxProblem::Operator(operator.clone()),
            TokenKind::Star => SyntaxProblem::Operator("*".to_owned()),
            TokenKind::DoubleStar => SyntaxProblem::Operator("**".to_owned()),
            TokenKind::Minus => SyntaxProblem::Operator("-".to_owned()),
            TokenKind::Name(word) if word == "for" => SyntaxProblem::Comprehension,
            TokenKind::Name(word) if KEYWORDS.contains(&word.as_str()) => {
                SyntaxProblem::Keyword(word.clone())
            }
            other => SyntaxProblem::Unexpected {
                found: other.described(),
                expected,
            },
        };
        self.refuse_here(problem)
    }

    fn enter(&mut self, opener: Span) -> Result<(), SyntaxError> {
        if self.depth >= MAX_DEPTH {
            return Err(SyntaxError::new(opener, SyntaxProblem::TooDeep));
        }
        self.depth += 1;
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn expression(&mut self) -> Result<Expr, SyntaxError> {
        let head = self.primary()?;
        let mut links = Vec::new();
        let mut end = head.span;
        loop {
            match self.peek() {
                TokenKind::Dot => {
                    let dot = self.take();
                    let TokenKind::Name(name) = self.peek().clone() else {
                        return Err(self.refuse_here(SyntaxProblem::Unexpected {
                            found: self.peek().described(),
                            expected: "a name after `.`",
                        }));
                    };
                    if KEYWORDS.contains(&name.as_str()) {
                        return Err(self.refuse_here(SyntaxProblem::Keyword(name)));
                    }
                    let name_token = self.bump();
                    end = name_token.span;
                    links.push(Link::Attribute {
                        dot: dot.span,
                        name,
                        name_span: name_token.span,
                        comments: dot.comments,
                    });
                }
                TokenKind::Open('(') => {
                    let (span, arguments) = self.arguments()?;
                    end = span;
                    links.push(Link::Call { span, arguments });
                }
                TokenKind::Open('[') => return Err(self.refuse_here(SyntaxProblem::Subscript)),
                _ => break,
            }
        }
        if links.is_empty() {
            return Ok(head);
        }
        let span = head.span.to(end);
        let (head, links) = match head.kind {
            // A chain in brackets that is followed by more is one chain.
            ExprKind::Chain(mut chain) => {
                chain.links.extend(links);
                (chain.head, chain.links)
            }
            kind => (
                Box::new(Expr {
                    span: head.span,
                    kind,
                }),
                links,
            ),
        };
        Ok(Expr {
            span,
            kind: ExprKind::Chain(Chain { head, links }),
        })
    }

    fn primary(&mut self) -> Result<Expr, SyntaxError> {
        let span = self.span();
        match self.peek().clone() {
            TokenKind::Name(name) => {
                let value = match name.as_str() {
                    "True" => Some(LiteralValue::True),
                    "False" => Some(LiteralValue::False),
                    "None" => Some(LiteralValue::None),
                    word if KEYWORDS.contains(&word) => {
                        return Err(self.refuse_here(SyntaxProblem::Keyword(name.clone())));
                    }
                    _ => None,
                };
                self.bump();
                let kind = match value {
                    Some(value) => ExprKind::Literal(Literal {
                        written: name,
                        value,
                    }),
                    None => ExprKind::Name(name),
                };
                Ok(Expr { span, kind })
            }
            TokenKind::Literal(literal) => {
                self.bump();
                Ok(Expr {
                    span,
                    kind: ExprKind::Literal(literal),
                })
            }
            TokenKind::Ellipsis => {
                self.bump();
                Ok(Expr {
                    span,
                    kind: ExprKind::Ellipsis,
                })
            }
            TokenKind::Minus => {
                let TokenKind::Literal(Literal {
                    written,
                    value: value @ (LiteralValue::Integer(_) | LiteralValue::Float(_)),
                }) = self.peek_second().clone()
                else {
                    return Err(self.refuse_here(SyntaxProblem::Operator("-".to_owned())));
                };
                self.bump();
                let number = self.bump();
                let value = match value {
                    LiteralValue::Integer(digits) => LiteralValue::Integer(negated(&digits)),
                    LiteralValue::Float(digits) => LiteralValue::Float(negated(&digits)),
                    other => other,
                };
                Ok(Expr {
                    span: span.to(number.span),
                    kind: ExprKind::Literal(Literal {
                        written: format!("-{written}"),
                        value,
                    }),
                })
            }
            TokenKind::Open('(') => self.group(),
            TokenKind::Open('[') => self.list(),
            TokenKind::Open('{') => self.dict(),
            TokenKind::Operator(operator) if assigns(&operator) => {
                Err(self.refuse_here(SyntaxProblem::Assignment))
            }
            TokenKind::Operator(operator) => {
                Err(self.refuse_here(SyntaxProblem::Operator(operator)))
            }
            other => Err(self.refuse_here(SyntaxProblem::Unexpected {
                found: other.described(),
                expected: "an expression",
            })),
        }
    }

    /// The closing `bracket` of what opened at `opener`, after an item, where a `,` or the
    /// bracket was needed.
    fn closing(&mut self, bracket: char, opener: Span) -> Result<Option<Span>, SyntaxError> {
        match self.peek() {
            TokenKind::Close(found) if *found == bracket => Ok(Some(self.bump().span)),
            TokenKind::Comma => {
                self.bump();
                Ok(None)
            }
            TokenKind::End => Err(SyntaxError::new(
                opener,
                SyntaxProblem::Unclosed(opening(bracket)),
            )),
            _ => Err(self.after_expression(match bracket {
                ')' => "`,` or `)`",
                ']' => "`,` or `]`",
                _ => "`,` or `}`",
            })),
        }
    }

    fn group(&mut self) -> Result<Expr, SyntaxError> {
        let opener = self.bump().span;
        self.enter(opener)?;
        if let TokenKind::Close(')') = self.peek() {
            return Err(self.refuse_here(SyntaxProblem::Unexpected {
                found: "`)`".to_owned(),
                expected: "an expression",
            }));
        }
        let mut inner = self.expression()?;
        match self.peek() {
            TokenKind::Close(')') => {
                let close = self.bump().span;
                self.leave();
                // The brackets are kept in the span, so that a refusal of what they group
                // names them too.
                inner.span = opener.to(close);
                Ok(inner)
            }
            TokenKind::End => Err(SyntaxError::new(opener, SyntaxProblem::Unclosed('('))),
            TokenKind::Comma => Err(self.refuse_here(SyntaxProblem::Unexpected {
                found: "`,`".to_owned(),
                expected: "`)`, as a tuple is not read,",
            })),
            _ => Err(self.after_expression("`)`")),
        }
    }

    fn list(&mut self) -> Result<Expr, SyntaxError> {
        let opener = self.bump().span;
        self.enter(opener)?;
        let mut items = Vec::new();
        let close = loop {
            if let TokenKind::Close(']') = self.peek() {
                break self.bump().span;
            }
            if self.peek() == &TokenKind::End {
                return Err(SyntaxError::new(opener, SyntaxProblem::Unclosed('[')));
            }
            items.push(self.expression()?);
            if let Some(close) = self.closing(']', opener)? {
                break close;
            }
        };
        self.leave();
        Ok(Expr {
            span: opener.to(close),
            kind: ExprKind::List(items),
        })
    }

    fn dict(&mut self) -> Result<Expr, SyntaxError> {
        let opener = self.bump().span;
        self.enter(opener)?;
        let mut entries = Vec::new();
        let close = loop {
            if let TokenKind::Close('}') = self.peek() {
                break self.bump().span;
            }
            if self.peek() == &TokenKind::End {
                return Err(SyntaxError::new(opener, SyntaxProblem::Unclosed('{')));
            }
            let key = self.expression()?;
            if self.peek() != &TokenKind::Colon {
                return Err(self.after_expression("`:` after a key"));
            }
            self.bump();
            let value = self.expression()?;
            entries.push((key, value));
            if let Some(close) = self.closing('}', opener)? {
                break close;
            }
        };
        self.leave();
        Ok(Expr {
            span: opener.to(close),
            kind: ExprKind::Dict(entries),
        })
    }

    fn arguments(&mut self) -> Result<(Span, Vec<Argument>), SyntaxError> {
        let opener = self.bump().span;
        self.enter(opener)?;
        let mut arguments = Vec::new();
        // As Python reads arguments: none by position after one by name or a `**`, and no `*`
        // after a `**`.
        let mut by_name = false;
        let mut spread_pairs = false;
        let close = loop {
            if let TokenKind::Close(')') = self.peek() {
                break self.bump().span;
            }
            if self.peek() == &TokenKind::End {
                return Err(SyntaxError::new(opener, SyntaxProblem::Unclosed('(')));
            }
            let start = self.span();
            let positional = !matches!(self.peek(), TokenKind::DoubleStar)
                && !(matches!(self.peek(), TokenKind::Name(_))
                    && self.peek_second() == &TokenKind::Equals);
            let spread = self.peek() == &TokenKind::Star;
            if (positional && !spread && by_name) || (spread && spread_pairs) {
                return Err(self.refuse_here(SyntaxProblem::PositionalAfterKeyword));
            }
            let argument = match self.peek().clone() {
                TokenKind::Star => {
                    self.bump();
                    let value = self.expression()?;
                    Argument {
                        span: start.to(value.span),
                        kind: ArgumentKind::Spread(value),
                    }
                }
                TokenKind::DoubleStar => {
                    self.bump();
                    by_name = true;
                    spread_pairs = true;
                    let value = self.expression()?;
                    Argument {
                        span: start.to(value.span),
                        kind: ArgumentKind::SpreadPairs(value),
                    }
                }
                TokenKind::Name(name) if !positional => {
                    if KEYWORDS.contains(&name.as_str()) {
                        return Err(self.refuse_here(SyntaxProblem::Keyword(name)));
                    }
                    let repeated = arguments.iter().any(|argument: &Argument| {
                        matches!(&argument.kind, ArgumentKind::Keyword { name: given, .. } if *given == name)
                    });
                    if repeated {
                        return Err(self.refuse_here(SyntaxProblem::RepeatedKeyword(name)));
                    }
                    let name_span = self.bump().span;
                    self.bump();
                    by_name = true;
                    let value = self.expression()?;
                    Argument {
                        span: start.to(value.span),
                        kind: ArgumentKind::Keyword {
                            name,
                            name_span,
                            value,
                        },
                    }
                }
                _ => {
                    let value = self.expression()?;
                    Argument {
                        span: value.span,
                        kind: ArgumentKind::Positional(value),
                    }
                }
            };
            arguments.push(argument);
            if let Some(close) = self.closing(')', opener)? {
                break close;
            }
        };
        self.leave();
        Ok((opener.to(close), arguments))
    }
}

const fn opening(bracket: char) -> char {
    match bracket {
        ')' => '(',
        ']' => '[',
        _ => '{',
    }
}

/// The digits of a number with a `-` before them, or without it where they had one.
fn negated(digits: &str) -> String {
    digits
        .strip_prefix('-')
        .map_or_else(|| format!("-{digits}"), str::to_owned)
}
