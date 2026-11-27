//! Writing a tree as text that reads back as the same tree.
//!
//! What fits on a line is written on one. What does not is broken, four spaces deeper for each
//! bracket: the items of a call, a list or a dict one to a line, each followed by a comma, and a
//! chain at each of its calls, a dot starting each line. The outermost chain is broken at each of
//! its calls whenever it has more than one, so that a chain of steps reads a step to a line, and
//! is then put in brackets, as Python needs to read it across lines.

use crate::tree::{Argument, ArgumentKind, Chain, Expr, ExprKind, Link, LiteralValue};

/// How many characters a line may hold, as the common Python formatters hold it.
pub const WIDTH: usize = 88;

const INDENT: usize = 4;

/// `expression` as text, ending with a new line.
///
/// Reading the text gives the same tree, apart from spans, brackets that only grouped, and
/// comments that stood before no attribute, which a tree read from text holds apart and printing
/// leaves out. A comment of an attribute is written on a line of its own before the attribute's
/// dot. Printing the tree read from printed text gives the same text.
#[must_use]
pub fn print(expression: &Expr) -> String {
    let mut text = match &expression.kind {
        ExprKind::Chain(chain) if top_broken(expression, chain) => {
            let pad = " ".repeat(INDENT);
            format!("(\n{pad}{}\n)", broken_chain(chain, INDENT, true))
        }
        _ => render(expression, 0, WIDTH),
    };
    text.push('\n');
    text
}

/// Whether the outermost chain is written broken at its links, which Python reads across lines
/// only in brackets: where it has more than one call or a comment, or does not fit on a line and
/// has more than one segment to break at.
fn top_broken(expression: &Expr, chain: &Chain) -> bool {
    if calls(chain) > 1 || has_comments(chain) {
        return true;
    }
    let fits = flat(expression).is_some_and(|text| width(&text) <= WIDTH);
    !fits && segments(chain).len() > 1
}

fn calls(chain: &Chain) -> usize {
    chain
        .links
        .iter()
        .filter(|link| matches!(link, Link::Call { .. }))
        .count()
}

fn has_comments(chain: &Chain) -> bool {
    chain
        .links
        .iter()
        .any(|link| matches!(link, Link::Attribute { comments, .. } if !comments.is_empty()))
}

fn width(text: &str) -> usize {
    text.chars().count()
}

/// The expression on one line, or `None` where it holds a comment and cannot be.
fn flat(expression: &Expr) -> Option<String> {
    Some(match &expression.kind {
        ExprKind::Name(name) => name.clone(),
        ExprKind::Literal(literal) => literal.written.clone(),
        ExprKind::Ellipsis => "...".to_owned(),
        ExprKind::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(flat)
                .collect::<Option<Vec<_>>>()?
                .join(", ")
        ),
        ExprKind::Dict(entries) => format!(
            "{{{}}}",
            entries
                .iter()
                .map(|(key, value)| Some(format!("{}: {}", flat(key)?, flat(value)?)))
                .collect::<Option<Vec<_>>>()?
                .join(", ")
        ),
        ExprKind::Chain(chain) => {
            let mut text = head(&chain.head)?;
            for link in &chain.links {
                text.push_str(&flat_link(link)?);
            }
            text
        }
    })
}

/// The head of a chain on one line. A number is put in brackets, as `1.abs` would read as a
/// number.
fn head(expression: &Expr) -> Option<String> {
    let text = flat(expression)?;
    Some(match &expression.kind {
        ExprKind::Literal(literal)
            if matches!(
                literal.value,
                LiteralValue::Integer(_) | LiteralValue::Float(_)
            ) =>
        {
            format!("({text})")
        }
        _ => text,
    })
}

fn flat_link(link: &Link) -> Option<String> {
    match link {
        Link::Attribute { name, comments, .. } => comments.is_empty().then(|| format!(".{name}")),
        Link::Call { arguments, .. } => Some(format!(
            "({})",
            arguments
                .iter()
                .map(flat_argument)
                .collect::<Option<Vec<_>>>()?
                .join(", ")
        )),
    }
}

fn flat_argument(argument: &Argument) -> Option<String> {
    Some(match &argument.kind {
        ArgumentKind::Positional(value) => flat(value)?,
        ArgumentKind::Keyword { name, value, .. } => format!("{name}={}", flat(value)?),
        ArgumentKind::Spread(value) => format!("*{}", flat(value)?),
        ArgumentKind::SpreadPairs(value) => format!("**{}", flat(value)?),
    })
}

/// `expression` written from a column with `room` characters left on its line, its later lines
/// starting `indent` spaces in.
fn render(expression: &Expr, indent: usize, room: usize) -> String {
    if let Some(text) = flat(expression)
        && width(&text) <= room
    {
        return text;
    }
    match &expression.kind {
        ExprKind::List(items) => bracketed(
            '[',
            items.iter().map(|item| item_of(item)).collect(),
            ']',
            indent,
        ),
        ExprKind::Dict(entries) => bracketed(
            '{',
            entries
                .iter()
                .map(|(key, value)| -> Box<dyn Fn(usize, usize) -> String + '_> {
                    Box::new(move |indent, room| {
                        let key = render(key, indent, room);
                        let left = room.saturating_sub(width(last_line(&key)) + 2);
                        format!("{key}: {}", render(value, indent, left))
                    })
                })
                .collect(),
            '}',
            indent,
        ),
        ExprKind::Chain(chain) => render_chain(chain, indent, room),
        // A name, a literal or `...` is one token, and cannot be broken.
        _ => flat(expression).unwrap_or_default(),
    }
}

type Item<'a> = Box<dyn Fn(usize, usize) -> String + 'a>;

fn item_of(expression: &Expr) -> Item<'_> {
    Box::new(move |indent, room| render(expression, indent, room))
}

fn argument_item(argument: &Argument) -> Item<'_> {
    Box::new(move |indent, room| match &argument.kind {
        ArgumentKind::Positional(value) => render(value, indent, room),
        ArgumentKind::Keyword { name, value, .. } => format!(
            "{name}={}",
            render(value, indent, room.saturating_sub(width(name) + 1))
        ),
        ArgumentKind::Spread(value) => {
            format!("*{}", render(value, indent, room.saturating_sub(1)))
        }
        ArgumentKind::SpreadPairs(value) => {
            format!("**{}", render(value, indent, room.saturating_sub(2)))
        }
    })
}

/// The items between `open` and `close`, one to a line, each `INDENT` deeper than `indent` and
/// followed by a comma.
fn bracketed(open: char, items: Vec<Item<'_>>, close: char, indent: usize) -> String {
    if items.is_empty() {
        return format!("{open}{close}");
    }
    let inner = indent + INDENT;
    let pad = " ".repeat(inner);
    let mut text = format!("{open}\n");
    for item in items {
        // The comma after the item takes one character of the line.
        let room = WIDTH.saturating_sub(inner + 1);
        text.push_str(&pad);
        text.push_str(&item(inner, room));
        text.push_str(",\n");
    }
    text.push_str(&" ".repeat(indent));
    text.push(close);
    text
}

fn last_line(text: &str) -> &str {
    text.rsplit('\n').next().unwrap_or(text)
}

/// The links of a chain cut where a line may start: at an attribute that is the first link,
/// follows a call or has a comment, and at a call that follows a call or the head.
fn segments(chain: &Chain) -> Vec<&[Link]> {
    let mut cuts = Vec::new();
    for (at, link) in chain.links.iter().enumerate() {
        let before = at.checked_sub(1).map(|before| &chain.links[before]);
        let starts = match link {
            Link::Attribute { comments, .. } => {
                !comments.is_empty() || !matches!(before, Some(Link::Attribute { .. }))
            }
            Link::Call { .. } => !matches!(before, Some(Link::Attribute { .. })),
        };
        if starts {
            cuts.push(at);
        }
    }
    cuts.push(chain.links.len());
    cuts.windows(2)
        .map(|pair| &chain.links[pair[0]..pair[1]])
        .collect()
}

fn render_chain(chain: &Chain, indent: usize, room: usize) -> String {
    let parts = segments(chain);
    let first_commented = matches!(
        parts.first().and_then(|part| part.first()),
        Some(Link::Attribute { comments, .. }) if !comments.is_empty()
    );
    if parts.len() == 1 && !first_commented {
        let head_text = render_head(&chain.head, indent, room);
        let left = room.saturating_sub(width(last_line(&head_text)));
        return format!("{head_text}{}", segment(parts[0], indent, left));
    }
    broken_chain(chain, indent, false)
}

fn render_head(expression: &Expr, indent: usize, room: usize) -> String {
    match head(expression) {
        Some(text) if width(&text) <= room => text,
        _ => render(expression, indent, room),
    }
}

/// The chain with each segment on a line of its own, starting `indent` spaces in. The first
/// segment stays on the head's line unless `head_alone` or it has a comment.
fn broken_chain(chain: &Chain, indent: usize, head_alone: bool) -> String {
    let pad = " ".repeat(indent);
    let room = WIDTH.saturating_sub(indent);
    let mut text = render_head(&chain.head, indent, room);
    for (at, part) in segments(chain).into_iter().enumerate() {
        let comments = match part.first() {
            Some(Link::Attribute { comments, .. }) => comments.as_slice(),
            _ => &[],
        };
        if at == 0 && !head_alone && comments.is_empty() {
            let left = room.saturating_sub(width(last_line(&text)));
            text.push_str(&segment(part, indent, left));
            continue;
        }
        for comment in comments {
            text.push('\n');
            text.push_str(&pad);
            text.push('#');
            text.push_str(&comment.text);
        }
        text.push('\n');
        text.push_str(&pad);
        text.push_str(&segment(part, indent, room));
    }
    text
}

/// A segment of a chain: its attributes, then its call, whose arguments are broken where they do
/// not fit in `room`.
fn segment(links: &[Link], indent: usize, room: usize) -> String {
    let mut text = String::new();
    for link in links {
        match link {
            Link::Attribute { name, .. } => {
                text.push('.');
                text.push_str(name);
            }
            Link::Call { arguments, .. } => {
                let flat_call = flat_link(link);
                let left = room.saturating_sub(width(&text));
                match flat_call {
                    Some(call) if width(&call) <= left => text.push_str(&call),
                    _ => {
                        text.push_str(&bracketed(
                            '(',
                            arguments.iter().map(argument_item).collect(),
                            ')',
                            indent,
                        ));
                    }
                }
            }
        }
    }
    text
}
