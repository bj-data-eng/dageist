//! Writing a step document as text that reads back as the same document.

use dageist_engine::catalog::{Entry, Parameter, Shape, Variadic};
use dageist_engine::document::{Call, Document, Expression, Place, Segment, Start, Step, Value};
use dageist_syntax::{Argument, ArgumentKind, Comment, Expr, ExprKind, KEYWORDS, Link, print};

use crate::error::WriteError;
use crate::names::{self, FRAME};

/// The document as text: a chain on `lf` with a step to a line, laid out by
/// [`dageist_syntax::print`].
///
/// A step is written without its name where reading gives it that name, as the step editor names
/// a new step, and with a `# step:` comment on the line before its dot where it is named
/// otherwise. Each call's arguments are written in the order the document holds them: the first,
/// and those Python needs, by position as long as every one before them is, a starred parameter's
/// items by position or after `*`, and the rest by name. A list of one for a parameter of many is
/// written as its item, and an expression that is `pl.lit` of a number, `True` or `False` alone is
/// written bare.
///
/// Reading the text gives a document equal to this one, with one exception: Python reads no `*`
/// after a `**`, so where a call holds a starred parameter's items after pairs that are written as
/// a dict after `**`, because a key is no Python name, the items are written before the pairs, and
/// read back in that order.
///
/// # Errors
///
/// When an input or a table is named by what is no Python name, or by `lf`, `pl`, `dg` or a word
/// of Python's own, which the text cannot hold bare.
pub fn write(document: &Document) -> Result<String, WriteError> {
    let steps = document.steps();
    let named = named_steps(steps);
    let mut links = Vec::with_capacity(steps.len() * 2);
    for (at, (step, named)) in steps.iter().zip(named).enumerate() {
        let place = Place::of_step(at, Some(step.id().clone()));
        let (_, attributes) = names::written(step.call().entry());
        let comments = if named {
            vec![Comment::new(format!(
                " step: {}",
                step_name(step.id().as_str())
            ))]
        } else {
            Vec::new()
        };
        for (index, name) in attributes.into_iter().enumerate() {
            links.push(Link::Attribute {
                dot: dageist_syntax::Span::default(),
                name: name.to_owned(),
                name_span: dageist_syntax::Span::default(),
                comments: if index == 0 {
                    comments.clone()
                } else {
                    Vec::new()
                },
            });
        }
        links.push(Link::call(arguments(step.call(), &place)?));
    }
    Ok(print(&Expr::chain(Expr::name(FRAME), links)))
}

/// Which steps are written with their names: those reading would name otherwise. Writing a name
/// takes it before names are made, which can change what another step is made, so the steps to
/// name are found again until no more are.
fn named_steps(steps: &[Step]) -> Vec<bool> {
    let stems: Vec<&str> = steps
        .iter()
        .map(|step| names::stem(step.call().entry()))
        .collect();
    let mut named = vec![false; steps.len()];
    loop {
        let given: Vec<Option<&str>> = steps
            .iter()
            .zip(&named)
            .map(|(step, named)| named.then(|| step.id().as_str()))
            .collect();
        let made = names::default_names(&stems, &given);
        let mut more = false;
        for ((step, made), named) in steps.iter().zip(made).zip(named.iter_mut()) {
            if !*named && made != step.id().as_str() {
                *named = true;
                more = true;
            }
        }
        if !more {
            return named;
        }
    }
}

fn step_name(name: &str) -> String {
    if names::is_bare_step_name(name) {
        name.to_owned()
    } else {
        Value::String(name.to_owned()).python().unwrap_or_default()
    }
}

/// The text of a scalar, as the engine writes it, read as a tree.
fn python(value: &Value) -> Expr {
    let text = value
        .python()
        .expect("a value that is no list, pairs or expression has Python's words");
    dageist_syntax::parse(&text)
        .expect("the engine writes a value as Python reads it")
        .expression
}

fn under(place: &Place, segment: Segment) -> Place {
    place.clone().then(segment)
}

/// The arguments of `call`, in the order the document holds them.
///
/// An argument is written by position while every one before it is, it is for the next parameter
/// Python binds by position, and it is the first such parameter or one Python needs. A starred
/// parameter's items are written by position where every parameter before it is, after `*` where
/// an argument by name comes first, and by name where a parameter before it is not given, which
/// Python could not bind. Any other argument is written by name.
fn arguments(call: &Call, place: &Place) -> Result<Vec<Argument>, WriteError> {
    let entry = call.entry();
    let parameters = entry.parameters();
    let mut slots = Vec::new();
    for parameter in parameters {
        if parameter.variadic() == Some(Variadic::Kwargs) || parameter.is_keyword_only() {
            continue;
        }
        slots.push(parameter.name());
        if parameter.variadic() == Some(Variadic::Args) {
            break;
        }
    }
    let mut written = Vec::with_capacity(call.arguments().len());
    let mut next = 0;
    let mut by_position = true;
    for given in call.arguments() {
        let Some(parameter) = parameters
            .iter()
            .find(|parameter| parameter.name() == given.parameter())
        else {
            continue;
        };
        let at = under(place, Segment::Parameter(parameter.name().to_owned()));
        let is_next = slots.get(next) == Some(&parameter.name());
        match (parameter.variadic(), given.value()) {
            (Some(Variadic::Kwargs), Value::Pairs(pairs)) => {
                written.extend(keyword_pairs(entry, pairs, &at)?);
                by_position = false;
            }
            (Some(Variadic::Args), Value::Many(items)) if is_next => {
                let values = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| value(item, &under(&at, Segment::Item(index))))
                    .collect::<Result<Vec<_>, _>>()?;
                if by_position && !values.is_empty() {
                    written.extend(
                        values
                            .into_iter()
                            .map(|value| Argument::new(ArgumentKind::Positional(value))),
                    );
                } else {
                    written.push(Argument::new(ArgumentKind::Spread(Expr::new(
                        ExprKind::List(values),
                    ))));
                }
                next += 1;
                by_position = false;
            }
            (None, held) if by_position && is_next && (next == 0 || parameter.is_needed()) => {
                written.push(Argument::new(ArgumentKind::Positional(value_of(
                    parameter, held, &at,
                )?)));
                next += 1;
            }
            (_, held) => {
                written.push(keyword(parameter, value_of(parameter, held, &at)?));
                by_position = false;
            }
        }
    }
    // Python reads no `*` after a `**`: where a document holds a starred parameter after pairs
    // that are written as a dict, its items are written before them.
    if let Some(pairs) = written
        .iter()
        .position(|argument| matches!(argument.kind, ArgumentKind::SpreadPairs(_)))
        && let Some(spread) = written
            .iter()
            .rposition(|argument| matches!(argument.kind, ArgumentKind::Spread(_)))
        && spread > pairs
    {
        let moved = written.remove(spread);
        written.insert(pairs, moved);
    }
    Ok(written)
}

fn keyword(parameter: &Parameter, value: Expr) -> Argument {
    Argument::new(ArgumentKind::Keyword {
        name: parameter.name().to_owned(),
        name_span: dageist_syntax::Span::default(),
        value,
    })
}

/// The pairs of a double-starred parameter: each by name where every key is a Python name that
/// no parameter of the entry has and no other pair repeats, and a dict after `**` where one is
/// not, since Python reads no name twice.
fn keyword_pairs(
    entry: &Entry,
    pairs: &[(Value, Value)],
    place: &Place,
) -> Result<Vec<Argument>, WriteError> {
    let names: Option<Vec<&str>> = pairs
        .iter()
        .map(|(key, _)| match key {
            Value::Columns(names) => match names.as_slice() {
                [name]
                    if is_name(name)
                        && !entry
                            .parameters()
                            .iter()
                            .any(|parameter| parameter.name() == name) =>
                {
                    Some(name.as_str())
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    match names {
        Some(names)
            if !names.is_empty()
                && names
                    .iter()
                    .enumerate()
                    .all(|(at, name)| !names[..at].contains(name)) =>
        {
            names
                .into_iter()
                .zip(pairs)
                .enumerate()
                .map(|(index, (name, (_, entry_value)))| {
                    Ok(Argument::new(ArgumentKind::Keyword {
                        name: name.to_owned(),
                        name_span: dageist_syntax::Span::default(),
                        value: value(entry_value, &under(place, Segment::Value(index)))?,
                    }))
                })
                .collect()
        }
        _ => Ok(vec![Argument::new(ArgumentKind::SpreadPairs(dict(
            pairs, place,
        )?))]),
    }
}

/// Whether `name` is a Python name a keyword argument can have.
fn is_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
        && !KEYWORDS.contains(&name)
}

/// The argument for `parameter`: a list of one for a parameter of many is its item, where the
/// item is not written as a list itself.
fn value_of(parameter: &Parameter, given: &Value, place: &Place) -> Result<Expr, WriteError> {
    if parameter.shape() == Shape::Many
        && parameter.variadic().is_none()
        && let Value::Many(items) = given
        && let [item] = items.as_slice()
        && !is_list(item)
    {
        return value(item, &under(place, Segment::Item(0)));
    }
    value(given, place)
}

/// Whether `value` is written as a list.
fn is_list(value: &Value) -> bool {
    match value {
        Value::Many(_) => true,
        Value::Columns(names) => names.len() != 1,
        _ => false,
    }
}

fn value(value: &Value, place: &Place) -> Result<Expr, WriteError> {
    Ok(match value {
        Value::Expression(inner) => expression(inner, place)?,
        Value::Many(items) => Expr::new(ExprKind::List(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| self::value(item, &under(place, Segment::Item(index))))
                .collect::<Result<_, _>>()?,
        )),
        Value::Pairs(pairs) => dict(pairs, place)?,
        Value::Table(name) => free(name, place)?,
        scalar => python(scalar),
    })
}

fn dict(pairs: &[(Value, Value)], place: &Place) -> Result<Expr, WriteError> {
    Ok(Expr::new(ExprKind::Dict(
        pairs
            .iter()
            .enumerate()
            .map(|(index, (key, entry_value))| {
                Ok((
                    value(key, &under(place, Segment::Key(index)))?,
                    value(entry_value, &under(place, Segment::Value(index)))?,
                ))
            })
            .collect::<Result<_, WriteError>>()?,
    )))
}

fn free(name: &str, place: &Place) -> Result<Expr, WriteError> {
    if names::is_free(name) {
        Ok(Expr::name(name))
    } else {
        Err(WriteError::new(place.clone(), name.to_owned()))
    }
}

/// An expression: a number, `True` or `False` alone where it is `pl.lit` of one and nothing more,
/// and otherwise its start and its links.
fn expression(expression: &Expression, place: &Place) -> Result<Expr, WriteError> {
    if let Some(bare) = bare_literal(expression) {
        return Ok(python(bare));
    }
    let head = match expression.start() {
        Start::Hole => Expr::new(ExprKind::Ellipsis),
        Start::Input(name) => free(name, &under(place, Segment::Start))?,
        Start::Call(call) => {
            let (before, attributes) = names::written(call.entry());
            let mut links: Vec<Link> = attributes.into_iter().map(Link::attribute).collect();
            links.push(Link::call(arguments(call, &under(place, Segment::Start))?));
            Expr::chain(Expr::name(before.unwrap_or(names::POLARS)), links)
        }
    };
    let mut links = Vec::new();
    for (index, link) in expression.links().iter().enumerate() {
        let (_, attributes) = names::written(link.entry());
        links.extend(attributes.into_iter().map(Link::attribute));
        links.push(Link::call(arguments(
            link,
            &under(place, Segment::Link(index)),
        )?));
    }
    Ok(Expr::chain(head, links))
}

/// The number, `True` or `False` an expression is, where it is `pl.lit` of it alone, which Python
/// Polars reads the same when it is written bare.
fn bare_literal(expression: &Expression) -> Option<&Value> {
    let Start::Call(call) = expression.start() else {
        return None;
    };
    if !expression.links().is_empty() || call.entry().id().as_str() != "polars.lit" {
        return None;
    }
    match call.arguments() {
        [given] if given.parameter() == "value" => match given.value() {
            // `None` alone is no value to Python Polars where a parameter defaults to it, but the
            // parameter left out, so `pl.lit(None)` is written out.
            value @ (Value::Integer(_) | Value::Float(_) | Value::Flag(Some(_))) => Some(value),
            _ => None,
        },
        _ => None,
    }
}
