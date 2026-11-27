//! The text as tokens, each with its span and the comments that stand before it.

use crate::error::{SyntaxError, SyntaxProblem};
use crate::span::{Position, Span};
use crate::tree::{Comment, Literal, LiteralValue};

/// What a token is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Name(String),
    Literal(Literal),
    Ellipsis,
    Dot,
    Comma,
    Colon,
    Equals,
    Star,
    DoubleStar,
    Minus,
    Open(char),
    Close(char),
    Operator(String),
    Semicolon,
    Newline,
    End,
}

impl TokenKind {
    /// The token in words, as a refusal names what it found.
    pub(crate) fn described(&self) -> String {
        match self {
            Self::Name(name) => format!("the name `{name}`"),
            Self::Literal(literal) => format!("the literal `{}`", literal.written),
            Self::Ellipsis => "`...`".to_owned(),
            Self::Dot => "`.`".to_owned(),
            Self::Comma => "`,`".to_owned(),
            Self::Colon => "`:`".to_owned(),
            Self::Equals => "`=`".to_owned(),
            Self::Star => "`*`".to_owned(),
            Self::DoubleStar => "`**`".to_owned(),
            Self::Minus => "`-`".to_owned(),
            Self::Open(bracket) | Self::Close(bracket) => format!("`{bracket}`"),
            Self::Operator(operator) => format!("`{operator}`"),
            Self::Semicolon => "`;`".to_owned(),
            Self::Newline => "the end of the line".to_owned(),
            Self::End => "the end of the text".to_owned(),
        }
    }
}

/// One token.
#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub span: Span,
    /// The comments between the token before and this one.
    pub comments: Vec<Comment>,
}

/// The operators of Python that are no token of their own here, longest first.
const OPERATORS: [&str; 37] = [
    "**=", "//=", ">>=", "<<=", "**", "//", ">>", "<<", "<=", ">=", "==", "!=", "->", ":=", "+=",
    "-=", "*=", "/=", "%=", "@=", "&=", "|=", "^=", "+", "-", "*", "/", "%", "@", "<", ">", "=",
    "~", "&", "|", "^", ":",
];

struct Lexer<'a> {
    text: &'a str,
    offset: usize,
    line: usize,
    column: usize,
    depth: usize,
    tokens: Vec<Token>,
    comments: Vec<Comment>,
}

/// The tokens of `text`, ending with [`TokenKind::End`]. A line ends a statement only outside
/// brackets, and lines in a row give one [`TokenKind::Newline`].
pub(crate) fn tokens(text: &str) -> Result<Vec<Token>, SyntaxError> {
    let mut lexer = Lexer {
        text,
        offset: 0,
        line: 1,
        column: 1,
        depth: 0,
        tokens: Vec::new(),
        comments: Vec::new(),
    };
    lexer.run()?;
    Ok(lexer.tokens)
}

const fn is_name_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

const fn is_name_part(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

impl Lexer<'_> {
    const fn here(&self) -> Position {
        Position::new(self.line, self.column, self.offset)
    }

    fn peek(&self) -> Option<char> {
        self.text[self.offset..].chars().next()
    }

    fn peek_at(&self, ahead: usize) -> Option<char> {
        self.text[self.offset..].chars().nth(ahead)
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.offset += character.len_utf8();
        // A `\r` alone ends a line, as `\n` does; before a `\n` it is part of that line's end.
        let line_ends = character == '\n' || (character == '\r' && self.peek() != Some('\n'));
        if line_ends {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(character)
    }

    fn push(&mut self, kind: TokenKind, start: Position) {
        let comments = std::mem::take(&mut self.comments);
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.here()),
            comments,
        });
    }

    fn error(&self, start: Position, problem: SyntaxProblem) -> SyntaxError {
        SyntaxError::new(Span::new(start, self.here()), problem)
    }

    fn run(&mut self) -> Result<(), SyntaxError> {
        while let Some(character) = self.peek() {
            let start = self.here();
            match character {
                ' ' | '\t' | '\x0c' => {
                    self.bump();
                }
                '\n' | '\r' => {
                    self.bump();
                    if character == '\r' && self.peek() == Some('\n') {
                        self.bump();
                    }
                    let after_line = matches!(
                        self.tokens.last().map(|token| &token.kind),
                        None | Some(TokenKind::Newline)
                    );
                    if self.depth == 0 && !after_line {
                        self.push(TokenKind::Newline, start);
                    }
                }
                '\\' => {
                    self.bump();
                    match self.peek() {
                        Some('\n') => {
                            self.bump();
                        }
                        Some('\r') => {
                            self.bump();
                            if self.peek() == Some('\n') {
                                self.bump();
                            }
                        }
                        _ => {
                            return Err(self.error(start, SyntaxProblem::UnexpectedCharacter('\\')));
                        }
                    }
                }
                '#' => self.comment(start),
                '"' | '\'' => self.string(start, "")?,
                '0'..='9' => self.number(start)?,
                '.' if self.peek_at(1).is_some_and(|next| next.is_ascii_digit()) => {
                    self.number(start)?;
                }
                '.' => {
                    if self.peek_at(1) == Some('.') && self.peek_at(2) == Some('.') {
                        self.bump();
                        self.bump();
                        self.bump();
                        self.push(TokenKind::Ellipsis, start);
                    } else {
                        self.bump();
                        self.push(TokenKind::Dot, start);
                    }
                }
                character if is_name_start(character) => self.name(start)?,
                '(' | '[' | '{' => {
                    self.bump();
                    self.depth += 1;
                    self.push(TokenKind::Open(character), start);
                }
                ')' | ']' | '}' => {
                    self.bump();
                    self.depth = self.depth.saturating_sub(1);
                    self.push(TokenKind::Close(character), start);
                }
                ',' => {
                    self.bump();
                    self.push(TokenKind::Comma, start);
                }
                ';' => {
                    self.bump();
                    self.push(TokenKind::Semicolon, start);
                }
                _ => self.operator(start, character)?,
            }
        }
        let end = self.here();
        self.push(TokenKind::End, end);
        Ok(())
    }

    fn comment(&mut self, start: Position) {
        self.bump();
        let from = self.offset;
        while let Some(character) = self.peek() {
            if character == '\n' || character == '\r' {
                break;
            }
            self.bump();
        }
        let text = self.text[from..self.offset].to_owned();
        self.comments.push(Comment {
            span: Span::new(start, self.here()),
            text,
        });
    }

    fn operator(&mut self, start: Position, character: char) -> Result<(), SyntaxError> {
        let rest = &self.text[self.offset..];
        let Some(operator) = OPERATORS
            .iter()
            .find(|operator| rest.starts_with(**operator))
        else {
            self.bump();
            return Err(self.error(start, SyntaxProblem::UnexpectedCharacter(character)));
        };
        for _ in 0..operator.len() {
            self.bump();
        }
        let kind = match *operator {
            "**" => TokenKind::DoubleStar,
            "*" => TokenKind::Star,
            "=" => TokenKind::Equals,
            ":" => TokenKind::Colon,
            "-" => TokenKind::Minus,
            other => TokenKind::Operator(other.to_owned()),
        };
        self.push(kind, start);
        Ok(())
    }

    fn name(&mut self, start: Position) -> Result<(), SyntaxError> {
        let from = self.offset;
        while self.peek().is_some_and(is_name_part) {
            self.bump();
        }
        let name = &self.text[from..self.offset];
        if matches!(self.peek(), Some('"' | '\''))
            && name.len() <= 2
            && name.chars().all(|letter| "rRuUbBfF".contains(letter))
        {
            let prefix = name.to_owned();
            return self.string(start, &prefix);
        }
        if let Some(next) = self.peek()
            && !next.is_ascii()
            && next.is_alphanumeric()
        {
            self.bump();
            return Err(self.error(start, SyntaxProblem::UnexpectedCharacter(next)));
        }
        self.push(TokenKind::Name(name.to_owned()), start);
        Ok(())
    }

    fn number(&mut self, start: Position) -> Result<(), SyntaxError> {
        let from = self.offset;
        // Letters are taken in too, so that `0x1f`, `1j` and `1abc` are refused whole; a sign is
        // taken only after an exponent's `e`.
        while self
            .peek()
            .is_some_and(|character| is_name_part(character) || character == '.')
            || (matches!(self.peek(), Some('+' | '-'))
                && matches!(self.text[from..self.offset].chars().last(), Some('e' | 'E')))
        {
            self.bump();
        }
        let written = &self.text[from..self.offset];
        let value = decimal(written)
            .ok_or_else(|| self.error(start, SyntaxProblem::BadNumber(written.to_owned())))?;
        self.push(
            TokenKind::Literal(Literal {
                written: written.to_owned(),
                value,
            }),
            start,
        );
        Ok(())
    }

    fn string(&mut self, start: Position, prefix: &str) -> Result<(), SyntaxError> {
        let raw = match prefix {
            "" | "u" | "U" => false,
            "r" | "R" => true,
            other => {
                return Err(self.error(start, SyntaxProblem::StringPrefix(other.to_owned())));
            }
        };
        let Some(quote) = self.bump() else {
            return Err(self.error(start, SyntaxProblem::UnterminatedString));
        };
        if self.peek() == Some(quote) && self.peek_at(1) == Some(quote) {
            self.bump();
            self.bump();
            return Err(self.error(start, SyntaxProblem::TripleQuoted));
        }
        let mut value = String::new();
        loop {
            let Some(character) = self.peek() else {
                return Err(self.error(start, SyntaxProblem::UnterminatedString));
            };
            if character == '\n' || character == '\r' {
                return Err(self.error(start, SyntaxProblem::UnterminatedString));
            }
            self.bump();
            if character == quote {
                break;
            }
            if character != '\\' {
                value.push(character);
                continue;
            }
            let escape_start = Position::new(self.line, self.column - 1, self.offset - 1);
            let Some(next) = self.peek() else {
                return Err(self.error(start, SyntaxProblem::UnterminatedString));
            };
            if raw {
                if next == '\n' || next == '\r' {
                    return Err(self.error(start, SyntaxProblem::UnterminatedString));
                }
                self.bump();
                value.push('\\');
                value.push(next);
                continue;
            }
            self.escape(escape_start, next, &mut value)?;
        }
        let written = self.text[start.offset()..self.offset].to_owned();
        self.push(
            TokenKind::Literal(Literal {
                written,
                value: LiteralValue::String(value),
            }),
            start,
        );
        Ok(())
    }

    /// Reads the escape whose backslash is behind and whose next character is `next`.
    fn escape(
        &mut self,
        start: Position,
        next: char,
        value: &mut String,
    ) -> Result<(), SyntaxError> {
        self.bump();
        let simple = match next {
            '\\' => Some('\\'),
            '\'' => Some('\''),
            '"' => Some('"'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            'a' => Some('\x07'),
            'b' => Some('\x08'),
            'f' => Some('\x0c'),
            'v' => Some('\x0b'),
            _ => None,
        };
        if let Some(character) = simple {
            value.push(character);
            return Ok(());
        }
        let bad = |lexer: &Self| {
            let written = lexer.text[start.offset()..lexer.offset].to_owned();
            lexer.error(start, SyntaxProblem::BadEscape(written))
        };
        let digits = |lexer: &mut Self, count: usize, radix: u32| -> Option<u32> {
            let mut number = 0_u32;
            for _ in 0..count {
                let digit = lexer.peek()?.to_digit(radix)?;
                lexer.bump();
                number = number * radix + digit;
            }
            Some(number)
        };
        let code = match next {
            'x' => digits(self, 2, 16),
            'u' => digits(self, 4, 16),
            'U' => digits(self, 8, 16),
            '0'..='7' => {
                let mut number = next.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    match self.peek().and_then(|digit| digit.to_digit(8)) {
                        Some(digit) => {
                            self.bump();
                            number = number * 8 + digit;
                        }
                        None => break,
                    }
                }
                Some(number)
            }
            _ => None,
        };
        let character = code.and_then(char::from_u32).ok_or_else(|| bad(self))?;
        value.push(character);
        Ok(())
    }
}

/// What the decimal number `written` stands for, or `None` when it is no decimal number Python
/// reads: digits with single underscores between them, no leading zero before other digits, and
/// a point and an exponent that make it a float.
fn decimal(written: &str) -> Option<LiteralValue> {
    let (mantissa, exponent) = match written.find(['e', 'E']) {
        Some(at) => (&written[..at], Some(&written[at + 1..])),
        None => (written, None),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (mantissa, None),
    };
    let digits = |part: &str, empty_ok: bool| -> Option<String> {
        if part.is_empty() {
            return empty_ok.then(String::new);
        }
        if part.starts_with('_') || part.ends_with('_') || part.contains("__") {
            return None;
        }
        part.chars()
            .all(|character| character.is_ascii_digit() || character == '_')
            .then(|| part.replace('_', ""))
    };
    let whole_digits = digits(whole, fraction.is_some())?;
    let fraction_digits = match fraction {
        Some(fraction) => Some(digits(fraction, !whole_digits.is_empty())?),
        None => None,
    };
    if whole_digits.is_empty() && fraction_digits.as_deref().is_none_or(str::is_empty) {
        return None;
    }
    let exponent_digits = match exponent {
        Some(exponent) => {
            let (sign, rest) = match exponent.as_bytes().first() {
                Some(b'+') => ("+", &exponent[1..]),
                Some(b'-') => ("-", &exponent[1..]),
                _ => ("", exponent),
            };
            let rest = digits(rest, false)?;
            Some(format!("{sign}{rest}"))
        }
        None => None,
    };
    if fraction_digits.is_none() && exponent_digits.is_none() {
        // A whole number: Python refuses a leading zero before other digits.
        if whole_digits.starts_with('0') && whole_digits.bytes().any(|digit| digit != b'0') {
            return None;
        }
        return Some(LiteralValue::Integer(whole_digits));
    }
    let mut value = if whole_digits.is_empty() {
        "0".to_owned()
    } else {
        whole_digits
    };
    if let Some(fraction) = fraction_digits {
        value.push('.');
        value.push_str(if fraction.is_empty() { "0" } else { &fraction });
    }
    if let Some(exponent) = exponent_digits {
        value.push('e');
        value.push_str(&exponent);
    }
    Some(LiteralValue::Float(value))
}
