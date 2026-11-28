//! Reading a text as a step document, through the constructors the step editor's edits use.

use std::str::FromStr;

use dageist_engine::allowed::Literal as Typed;
use dageist_engine::catalog::{
    self, Applies, Build, Entry, Kind, Parameter, Shape, ValueType, Variadic,
};
use dageist_engine::document::{
    Call, Document, Dtype, Expression, Refusal, Segment, Start, Step, StepId, Value,
};
use dageist_syntax::{
    Argument, ArgumentKind, Chain, Comment, Expr, ExprKind, Link, LiteralValue, Span,
};

use crate::error::{Problem, ReadError};
use crate::names::{self, DAGEIST, FRAME, Owner, POLARS};
use crate::spans::{self, CallSpans, ExpressionSpans, Parts, ValueSpans};

/// Reads `text` as a step document, strictly.
///
/// The text is a chain on `lf`, the table the steps run on: each call after it is a step, a method
/// of `LazyFrame`. Every name resolves to an entry of the catalogue, every argument to a parameter
/// of it, as Python binds them, and every value is read as its parameter's kind takes it; the
/// document is then made with the constructors an edit in the step editor uses, so that what they
/// refuse, the text is refused for. See the crate's documentation for the form of the text.
///
/// # Errors
///
/// When the text is not the notation, names what the catalogue lacks or leaves out, gives an
/// argument the catalogue refuses, or puts a call where it cannot stand. The error has the span of
/// the part of the text refused, and the reason.
pub fn read(text: &str) -> Result<Document, ReadError> {
    let source = dageist_syntax::parse(text)
        .map_err(|error| ReadError::new(error.span(), Problem::Syntax(error.problem().clone())))?;
    if let Some(comment) = source
        .comments
        .iter()
        .find(|comment| names::step_comment(&comment.text).is_some())
    {
        return Err(ReadError::new(comment.span, Problem::MisplacedStepName));
    }
    let (head, links): (&Expr, &[Link]) = match &source.expression.kind {
        ExprKind::Chain(chain) => (&chain.head, &chain.links),
        _ => (&source.expression, &[]),
    };
    if !matches!(&head.kind, ExprKind::Name(name) if name == FRAME) {
        return Err(ReadError::new(head.span, Problem::NotOnTheFrame));
    }
    let groups = groups(links)?;
    let mut made = Vec::with_capacity(groups.len());
    let mut at = 0;
    while at < groups.len() {
        let group = &groups[at];
        let first = at;
        let (arguments, _) = group.called()?;
        let given = step_name(group)?;
        let entry = resolve(Owner::Frame, group)?;
        let (call, _) = bind(entry, arguments, group.name_span())?;
        // A step is checked for where it stands as it is read; its name is known once every
        // step's entry and given name are.
        let placeholder = StepId::new(names::stem(entry))
            .map_err(|_| ReadError::new(group.name_span(), Problem::EmptyStepName))?;
        // The calls on a group by that follow a step that groups are the step's, as far as the
        // call that finishes the grouping: Python gives a table again there, and what follows is
        // a step. What follows and is no call on a group by leaves the grouping unfinished.
        let mut linked = Vec::new();
        if matches!(entry.build(), Build::Grouping(_)) {
            while let Some(next) = groups.get(at + 1) {
                let Some(called) = grouped_call(next) else {
                    break;
                };
                next.no_step_name()?;
                let (arguments, _) = next.called()?;
                let (link, _) = bind(called, arguments, next.name_span())?;
                linked.push(link);
                at += 1;
                if matches!(called.build(), Build::GroupedEnd(_)) {
                    break;
                }
            }
        }
        let step = Step::chain(placeholder, call, linked).map_err(|refusal| {
            let span = match refusal.place().path().first() {
                Some(Segment::Link(link)) => groups
                    .get(first + 1 + link)
                    .map_or_else(|| group.name_span(), Group::name_span),
                _ => group.name_span(),
            };
            refused(span, refusal)
        })?;
        made.push((step, given, first));
        at += 1;
    }
    let stems: Vec<&str> = made
        .iter()
        .map(|(step, _, _)| names::stem(step.call().entry()))
        .collect();
    let given: Vec<Option<&str>> = made
        .iter()
        .map(|(_, given, _)| given.as_ref().map(|(name, _)| name.as_str()))
        .collect();
    let chosen = names::default_names(&stems, &given);
    let mut steps = Vec::with_capacity(made.len());
    for ((step, _, _), name) in made.iter().zip(chosen) {
        let id = StepId::new(name)
            .map_err(|_| ReadError::new(Span::default(), Problem::EmptyStepName))?;
        let step = Step::chain(id, step.call().clone(), step.links().to_vec())
            .map_err(|refusal| refused(Span::default(), refusal))?;
        steps.push(step);
    }
    Document::new(steps).map_err(|refusal| {
        let span = refusal
            .place()
            .step_index()
            .and_then(|at| {
                made.get(at).map(|(_, given, first)| {
                    given
                        .as_ref()
                        .map_or_else(|| groups[*first].name_span(), |(_, span)| *span)
                })
            })
            .unwrap_or_default();
        refused(span, refusal)
    })
}

const fn refused(span: Span, refusal: Refusal) -> ReadError {
    ReadError::new(span, Problem::Refused(refusal))
}

/// Attributes of a chain up to a call: a step, a function or a method, such as `.str.contains(…)`.
#[derive(Default)]
struct Group<'a> {
    names: Vec<(&'a str, Span)>,
    /// Each comment before an attribute's dot, with the attribute's place in the group.
    comments: Vec<(usize, &'a Comment)>,
    call: Option<(&'a [Argument], Span)>,
}

impl<'a> Group<'a> {
    fn path(&self) -> String {
        self.names
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(".")
    }

    fn name_span(&self) -> Span {
        match (self.names.first(), self.names.last()) {
            (Some((_, first)), Some((_, last))) => first.to(*last),
            _ => Span::default(),
        }
    }

    fn called(&self) -> Result<(&'a [Argument], Span), ReadError> {
        self.call
            .ok_or_else(|| ReadError::new(self.name_span(), Problem::NotCalled))
    }

    /// Refuses a `# step:` comment in the group, which is no step.
    fn no_step_name(&self) -> Result<(), ReadError> {
        match self
            .comments
            .iter()
            .find(|(_, comment)| names::step_comment(&comment.text).is_some())
        {
            Some((_, comment)) => Err(ReadError::new(comment.span, Problem::MisplacedStepName)),
            None => Ok(()),
        }
    }
}

/// The links of a chain as groups, each of attributes and the call after them. Attributes at the
/// end with no call are a group with none.
fn groups(links: &[Link]) -> Result<Vec<Group<'_>>, ReadError> {
    let mut groups = Vec::new();
    let mut current: Option<Group<'_>> = None;
    for link in links {
        match link {
            Link::Attribute {
                name,
                name_span,
                comments,
                ..
            } => {
                let group = current.get_or_insert_with(Group::default);
                let at = group.names.len();
                group
                    .comments
                    .extend(comments.iter().map(|comment| (at, comment)));
                group.names.push((name.as_str(), *name_span));
            }
            Link::Call { span, arguments } => match current.take() {
                Some(mut group) => {
                    group.call = Some((arguments.as_slice(), *span));
                    groups.push(group);
                }
                None => return Err(ReadError::new(*span, Problem::CallWithoutName)),
            },
        }
    }
    groups.extend(current);
    Ok(groups)
}

/// The entry the group names as a call on a group by, if it names one.
fn grouped_call(group: &Group<'_>) -> Option<&'static Entry> {
    names::resolve(Owner::Grouped, &group.path())
        .ok()
        .filter(|entry| entry.applies() == Applies::Grouped)
}

/// The name a `# step:` comment before a step's dot gives it, with the comment's span.
fn step_name(group: &Group<'_>) -> Result<Option<(String, Span)>, ReadError> {
    let mut found = None;
    for (at, comment) in &group.comments {
        let Some(written) = names::step_comment(&comment.text) else {
            continue;
        };
        if *at != 0 {
            return Err(ReadError::new(comment.span, Problem::MisplacedStepName));
        }
        if found.is_some() {
            return Err(ReadError::new(comment.span, Problem::TwoStepNames));
        }
        let name = if written.starts_with(['"', '\'']) {
            let quoted = dageist_syntax::parse(written).ok().and_then(|source| {
                match source.expression.kind {
                    ExprKind::Literal(literal) => match literal.value {
                        LiteralValue::String(text) => Some(text),
                        _ => None,
                    },
                    _ => None,
                }
            });
            quoted.ok_or_else(|| {
                ReadError::new(
                    comment.span,
                    Problem::Form {
                        expects: "a step's name, bare or as one string",
                        found: "other text",
                    },
                )
            })?
        } else {
            written.to_owned()
        };
        if name.is_empty() {
            return Err(ReadError::new(comment.span, Problem::EmptyStepName));
        }
        found = Some((name, comment.span));
    }
    Ok(found)
}

/// The entry the group names after `owner`.
fn resolve(owner: Owner<'_>, group: &Group<'_>) -> Result<&'static Entry, ReadError> {
    let path = group.path();
    names::resolve(owner, &path).map_err(|left_out| {
        let problem = match left_out {
            Some(item) => Problem::LeftOut {
                id: item.id(),
                reason: item.reason(),
            },
            None => Problem::UnknownName(match owner {
                Owner::Frame => format!("{FRAME}.{path}"),
                Owner::Function("dageist") => format!("{DAGEIST}.{path}"),
                Owner::Function(_) => format!("{POLARS}.{path}"),
                Owner::Method(_) | Owner::Grouped => format!(".{path}"),
            }),
        };
        ReadError::new(group.name_span(), problem)
    })
}

/// One argument bound to its parameter.
struct Bound {
    name: &'static str,
    value: Value,
    spans: ValueSpans,
}

/// The items given to a starred parameter, or the entries to a double-starred one.
struct Gathered<T> {
    values: Vec<T>,
    spans: Vec<ValueSpans>,
    pairs: Vec<(ValueSpans, ValueSpans)>,
    span: Span,
}

impl<T> Gathered<T> {
    fn new(span: Span) -> Self {
        Self {
            values: Vec::new(),
            spans: Vec::new(),
            pairs: Vec::new(),
            span,
        }
    }
}

/// The call of `entry` with `arguments`, bound to its parameters as Python binds them, and the
/// spans of its parts. The arguments are given to the call in the order they are written, the
/// items of a starred parameter where the first of them is and the entries of a double-starred
/// one where the first of them is, so that a document reads back with its arguments in the order
/// it holds them.
fn bind(
    entry: &'static Entry,
    arguments: &[Argument],
    name: Span,
) -> Result<(Call, CallSpans), ReadError> {
    let parameters = entry.parameters();
    let star = parameters
        .iter()
        .find(|parameter| parameter.variadic() == Some(Variadic::Args));
    let double = parameters
        .iter()
        .find(|parameter| parameter.variadic() == Some(Variadic::Kwargs));
    let mut slots = Vec::new();
    for parameter in parameters {
        if parameter.variadic() == Some(Variadic::Kwargs) || parameter.is_keyword_only() {
            continue;
        }
        slots.push(parameter);
        if parameter.variadic() == Some(Variadic::Args) {
            break;
        }
    }
    // A starred or double-starred parameter's place is kept free until all of it is read.
    let mut bound: Vec<Option<Bound>> = Vec::new();
    let mut star_at = None;
    let mut pairs_at = None;
    let mut next = 0;
    let mut items: Option<Gathered<Value>> = None;
    let mut pairs: Option<Gathered<(Value, Value)>> = None;
    for argument in arguments {
        match &argument.kind {
            ArgumentKind::Positional(value) => match slots.get(next) {
                Some(parameter) if parameter.variadic() == Some(Variadic::Args) => {
                    let gathered = items.get_or_insert_with(|| {
                        star_at = Some(bound.len());
                        bound.push(None);
                        Gathered::new(value.span)
                    });
                    let (read, spans) = read_kind(parameter.kind(), value)?;
                    gathered.values.push(read);
                    gathered.spans.push(spans);
                    gathered.span = gathered.span.to(value.span);
                }
                Some(parameter) => {
                    bound.push(Some(read_bound(parameter, value)?));
                    next += 1;
                }
                None => {
                    return Err(ReadError::new(
                        value.span,
                        Problem::TooManyArguments {
                            entry: entry.id().as_str(),
                            takes: slots.len(),
                        },
                    ));
                }
            },
            ArgumentKind::Spread(value) => match (slots.get(next), &value.kind) {
                (Some(parameter), ExprKind::List(list))
                    if parameter.variadic() == Some(Variadic::Args) =>
                {
                    let gathered = items.get_or_insert_with(|| {
                        star_at = Some(bound.len());
                        bound.push(None);
                        Gathered::new(argument.span)
                    });
                    for item in list {
                        let (read, spans) = read_kind(parameter.kind(), item)?;
                        gathered.values.push(read);
                        gathered.spans.push(spans);
                    }
                    gathered.span = gathered.span.to(argument.span);
                }
                _ => return Err(ReadError::new(argument.span, Problem::Spread)),
            },
            ArgumentKind::Keyword {
                name: keyword,
                name_span,
                value,
            } => {
                let named = parameters.iter().find(|parameter| {
                    parameter.name() == keyword && parameter.variadic().is_none()
                });
                let starred =
                    star.filter(|parameter| parameter.name() == keyword && double.is_none());
                if let Some(parameter) = named.or(starred) {
                    bound.push(Some(read_bound(parameter, value)?));
                } else if let Some(Kind::Pairs(_, kind)) = double.map(Parameter::kind) {
                    let gathered = pairs.get_or_insert_with(|| {
                        pairs_at = Some(bound.len());
                        bound.push(None);
                        Gathered::new(argument.span)
                    });
                    let (read, spans) = read_kind(*kind, value)?;
                    gathered
                        .values
                        .push((Value::Columns(vec![keyword.clone()]), read));
                    gathered.pairs.push((ValueSpans::leaf(*name_span), spans));
                    gathered.span = gathered.span.to(argument.span);
                } else {
                    // The catalogue's refusal of a parameter the entry does not have.
                    let refusal = Call::new(entry, [(keyword.as_str(), Value::Flag(None))])
                        .err()
                        .map_or(
                            Problem::UnknownName(format!("{keyword}=")),
                            Problem::Refused,
                        );
                    return Err(ReadError::new(*name_span, refusal));
                }
            }
            ArgumentKind::SpreadPairs(value) => match (double.map(Parameter::kind), &value.kind) {
                (Some(Kind::Pairs(key_kind, value_kind)), ExprKind::Dict(entries)) => {
                    let gathered = pairs.get_or_insert_with(|| {
                        pairs_at = Some(bound.len());
                        bound.push(None);
                        Gathered::new(argument.span)
                    });
                    for (key, entry_value) in entries {
                        let (key_read, key_spans) = read_kind(*key_kind, key)?;
                        let (value_read, value_spans) = read_kind(*value_kind, entry_value)?;
                        gathered.values.push((key_read, value_read));
                        gathered.pairs.push((key_spans, value_spans));
                    }
                    gathered.span = gathered.span.to(argument.span);
                }
                _ => return Err(ReadError::new(argument.span, Problem::Spread)),
            },
        }
    }
    if let (Some(parameter), Some(gathered), Some(at)) = (star, items, star_at) {
        bound[at] = Some(Bound {
            name: parameter.name(),
            value: Value::Many(gathered.values),
            spans: ValueSpans {
                span: gathered.span,
                parts: Parts::Items(gathered.spans),
            },
        });
    }
    if let (Some(parameter), Some(gathered), Some(at)) = (double, pairs, pairs_at) {
        bound[at] = Some(Bound {
            name: parameter.name(),
            value: Value::Pairs(gathered.values),
            spans: ValueSpans {
                span: gathered.span,
                parts: Parts::Pairs(gathered.pairs),
            },
        });
    }
    let bound: Vec<Bound> = bound.into_iter().flatten().collect();
    let mut given = Vec::with_capacity(bound.len());
    let mut parameter_spans = Vec::with_capacity(bound.len());
    for Bound {
        name: parameter,
        value,
        spans,
    } in bound
    {
        given.push((parameter, value));
        parameter_spans.push((parameter, spans));
    }
    let spans = CallSpans {
        name,
        parameters: parameter_spans,
    };
    match Call::new(entry, given) {
        Ok(call) => Ok((call, spans)),
        Err(refusal) => Err(refused(spans.locate(refusal.place().path()), refusal)),
    }
}

/// The argument for `parameter`, in its shape: a parameter of many takes a list, or one item that
/// is a list of one; one of an item each takes one or a list.
fn read_bound(parameter: &Parameter, written: &Expr) -> Result<Bound, ReadError> {
    let (value, spans) = match (parameter.shape(), &written.kind) {
        (Shape::Many | Shape::PerItem(_), ExprKind::List(list)) => {
            read_list(parameter.kind(), list, written.span)?
        }
        (Shape::Many, _) => {
            let (value, spans) = read_kind(parameter.kind(), written)?;
            (
                Value::Many(vec![value]),
                ValueSpans {
                    span: written.span,
                    parts: Parts::Items(vec![spans]),
                },
            )
        }
        _ => read_kind(parameter.kind(), written)?,
    };
    Ok(Bound {
        name: parameter.name(),
        value,
        spans,
    })
}

fn read_list(kind: Kind, list: &[Expr], span: Span) -> Result<(Value, ValueSpans), ReadError> {
    let mut values = Vec::with_capacity(list.len());
    let mut spans = Vec::with_capacity(list.len());
    for item in list {
        let (value, item_spans) = read_kind(kind, item)?;
        values.push(value);
        spans.push(item_spans);
    }
    Ok((
        Value::Many(values),
        ValueSpans {
            span,
            parts: Parts::Items(spans),
        },
    ))
}

/// What a parameter of `kind` holds, in words.
const fn expects(kind: Kind) -> &'static str {
    match kind {
        Kind::Expression(_) => "an expression",
        Kind::Column => "column names",
        Kind::Field => "a field's name",
        Kind::Value(ValueType::Integer) => "a whole number",
        Kind::Value(ValueType::Float) => "a number",
        Kind::Value(ValueType::String) => "text",
        Kind::Value(ValueType::Date) => "a date",
        Kind::Value(ValueType::Any) => "a number, text or a flag",
        Kind::Flag => "a flag",
        Kind::Choice(_) => "a choice",
        Kind::Dtype => "a dtype",
        Kind::Duration => "a duration",
        Kind::Pattern => "a pattern",
        Kind::TimeZone => "a time zone",
        Kind::Table => "a table, by its name",
        Kind::Path => "a path",
        Kind::Pairs(..) => "a dict",
    }
}

/// What is written, in words.
const fn found(written: &Expr) -> &'static str {
    match &written.kind {
        ExprKind::Name(_) => "a name",
        ExprKind::Literal(literal) => match literal.value {
            LiteralValue::Integer(_) => "a whole number",
            LiteralValue::Float(_) => "a number",
            LiteralValue::String(_) => "text",
            LiteralValue::True | LiteralValue::False => "a flag",
            LiteralValue::None => "None",
        },
        ExprKind::Ellipsis => "`...`",
        ExprKind::List(_) => "a list",
        ExprKind::Dict(_) => "a dict",
        ExprKind::Chain(_) => "a call",
    }
}

fn form(kind: Kind, written: &Expr) -> ReadError {
    ReadError::new(
        written.span,
        Problem::Form {
            expects: expects(kind),
            found: found(written),
        },
    )
}

/// One value of `kind`, as it is written. A form the kind does not hold, where there is a value
/// of its own, is read as that value, for the catalogue to refuse with its reason.
fn read_kind(kind: Kind, written: &Expr) -> Result<(Value, ValueSpans), ReadError> {
    let leaf = |value: Value| Ok((value, ValueSpans::leaf(written.span)));
    match (kind, &written.kind) {
        (Kind::Expression(_), _) => {
            let (expression, spans) = read_expression(written)?;
            Ok((
                Value::Expression(expression),
                ValueSpans {
                    span: written.span,
                    parts: Parts::Expression(spans),
                },
            ))
        }
        (Kind::Column, ExprKind::List(list)) => {
            let mut names = Vec::with_capacity(list.len());
            for item in list {
                match &item.kind {
                    ExprKind::Literal(literal) => match &literal.value {
                        LiteralValue::String(text) => names.push(text.clone()),
                        _ => return Err(form(Kind::Value(ValueType::String), item)),
                    },
                    _ => return Err(form(Kind::Value(ValueType::String), item)),
                }
            }
            leaf(Value::Columns(names))
        }
        (Kind::Pairs(key_kind, value_kind), ExprKind::Dict(entries)) => {
            let mut values = Vec::with_capacity(entries.len());
            let mut spans = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                let (key_read, key_spans) = read_kind(*key_kind, key)?;
                let (value_read, value_spans) = read_kind(*value_kind, value)?;
                values.push((key_read, value_read));
                spans.push((key_spans, value_spans));
            }
            Ok((
                Value::Pairs(values),
                ValueSpans {
                    span: written.span,
                    parts: Parts::Pairs(spans),
                },
            ))
        }
        (Kind::Dtype, ExprKind::Chain(_)) => leaf(read_dtype(written)?),
        (Kind::Table, ExprKind::Name(name)) => leaf(Value::Table(free(name, written.span)?)),
        (_, ExprKind::Literal(literal)) => leaf(literal_value(kind, &literal.value, written)?),
        (_, ExprKind::List(list)) => read_list(kind, list, written.span),
        _ => Err(form(kind, written)),
    }
}

/// `name` as an input's or a table's name, which it can only be where it stands for nothing else.
fn free(name: &str, span: Span) -> Result<String, ReadError> {
    if names::is_free(name) {
        Ok(name.to_owned())
    } else {
        Err(ReadError::new(span, Problem::ReservedName(name.to_owned())))
    }
}

/// The value a literal is, as a parameter of `kind` takes it: through [`Value::from_literal`],
/// as the editor makes a typed value.
fn literal_value(kind: Kind, value: &LiteralValue, written: &Expr) -> Result<Value, ReadError> {
    Ok(match value {
        LiteralValue::Integer(digits) => {
            let number = i64::from_str(digits).map_err(|_| {
                ReadError::new(written.span, Problem::NumberOutOfRange(digits.clone()))
            })?;
            Value::from_literal(&Typed::Integer(number), kind)
        }
        LiteralValue::Float(digits) => {
            // The syntax gives a float's digits as Rust reads them; one too large is infinite,
            // which the catalogue refuses.
            let number = f64::from_str(digits).unwrap_or(f64::NAN);
            Value::from_literal(&Typed::Float(number), kind)
        }
        LiteralValue::String(text) => match kind {
            Kind::Column => Value::Columns(vec![text.clone()]),
            Kind::Choice(_) => Value::Choice(text.clone()),
            _ => Value::from_literal(&Typed::String(text.clone()), kind),
        },
        LiteralValue::True => Value::Flag(Some(true)),
        LiteralValue::False => Value::Flag(Some(false)),
        LiteralValue::None => match kind {
            Kind::TimeZone => Value::TimeZone(None),
            _ => Value::Flag(None),
        },
    })
}

/// An expression: a chain from `pl.<function>(…)`, an input or `...`, or a number, `True` or
/// `False` alone, which is `pl.lit` of it.
fn read_expression(written: &Expr) -> Result<(Expression, ExpressionSpans), ReadError> {
    let word = |expression: Expression| {
        Ok((
            expression,
            ExpressionSpans {
                start: spans::Start::Word(written.span),
                links: Vec::new(),
            },
        ))
    };
    match &written.kind {
        ExprKind::Literal(literal) => match literal.value {
            LiteralValue::String(_) => {
                Err(ReadError::new(written.span, Problem::TextForAnExpression))
            }
            LiteralValue::None => Err(ReadError::new(written.span, Problem::NoneForAnExpression)),
            _ => literal_expression(&literal.value, written),
        },
        ExprKind::Ellipsis => word(Expression::hole()),
        ExprKind::Name(name) => word(Expression::input(free(name, written.span)?)),
        ExprKind::List(_) | ExprKind::Dict(_) => Err(form(Kind::Expression(&[]), written)),
        ExprKind::Chain(chain) => chain_expression(chain),
    }
}

/// `pl.lit` of a number, `True` or `False` written alone.
fn literal_expression(
    value: &LiteralValue,
    written: &Expr,
) -> Result<(Expression, ExpressionSpans), ReadError> {
    let entry = catalog::entry("polars.lit").ok_or_else(|| {
        ReadError::new(written.span, Problem::UnknownName(format!("{POLARS}.lit")))
    })?;
    let value = literal_value(Kind::Value(ValueType::Any), value, written)?;
    let call =
        Call::new(entry, [("value", value)]).map_err(|refusal| refused(written.span, refusal))?;
    let expression = Expression::new(Start::Call(call), Vec::new())
        .map_err(|refusal| refused(written.span, refusal))?;
    Ok((
        expression,
        ExpressionSpans {
            start: spans::Start::Call(CallSpans {
                name: written.span,
                parameters: vec![("value", ValueSpans::leaf(written.span))],
            }),
            links: Vec::new(),
        },
    ))
}

fn chain_expression(chain: &Chain) -> Result<(Expression, ExpressionSpans), ReadError> {
    let groups = groups(&chain.links)?;
    let head = &chain.head;
    let (start, start_spans, rest, mut class) = match &head.kind {
        ExprKind::Name(name) if name == POLARS || name == DAGEIST => {
            let first = groups
                .first()
                .ok_or_else(|| ReadError::new(head.span, Problem::NotCalled))?;
            first.no_step_name()?;
            let (arguments, _) = first.called()?;
            let module = if name == POLARS { "polars" } else { "dageist" };
            let entry = resolve(Owner::Function(module), first)?;
            let (call, spans) = bind(entry, arguments, first.name_span())?;
            (
                Start::Call(call),
                spans::Start::Call(spans),
                &groups[1..],
                names::class_given(entry),
            )
        }
        ExprKind::Name(name) => (
            Start::Input(free(name, head.span)?),
            spans::Start::Word(head.span),
            &groups[..],
            "Expr",
        ),
        ExprKind::Ellipsis => (
            Start::Hole,
            spans::Start::Word(head.span),
            &groups[..],
            "Expr",
        ),
        ExprKind::Literal(_) => {
            return Err(ReadError::new(head.span, Problem::MethodOfALiteral));
        }
        _ => return Err(form(Kind::Expression(&[]), head)),
    };
    let mut links = Vec::with_capacity(rest.len());
    let mut link_spans = Vec::with_capacity(rest.len());
    for group in rest {
        group.no_step_name()?;
        let (arguments, _) = group.called()?;
        let entry = resolve(Owner::Method(class), group)?;
        let (call, spans) = bind(entry, arguments, group.name_span())?;
        class = names::class_given(entry);
        links.push(call);
        link_spans.push(spans);
    }
    let spans = ExpressionSpans {
        start: start_spans,
        links: link_spans,
    };
    match Expression::new(start, links) {
        Ok(expression) => Ok((expression, spans)),
        Err(refusal) => Err(refused(spans.locate(refusal.place().path()), refusal)),
    }
}

/// A dtype, written as Python Polars writes one: `pl.Int64`, `pl.List(pl.String)`,
/// `pl.Datetime("ms", "UTC")`, `pl.Duration(time_unit="us")`. The engine reads it from its own
/// spelling, which Python Polars prints.
fn read_dtype(written: &Expr) -> Result<Value, ReadError> {
    let spelling = dtype_spelling(written)?;
    Dtype::from_str(&spelling)
        .map(Value::Dtype)
        .map_err(|error| ReadError::new(written.span, Problem::Dtype(error)))
}

fn dtype_spelling(written: &Expr) -> Result<String, ReadError> {
    let not_one = || form(Kind::Dtype, written);
    let ExprKind::Chain(chain) = &written.kind else {
        return Err(not_one());
    };
    if !matches!(&chain.head.kind, ExprKind::Name(name) if name == POLARS) {
        return Err(not_one());
    }
    match chain.links.as_slice() {
        [Link::Attribute { name, .. }] => Ok(match name.as_str() {
            "Datetime" => "Datetime(time_unit='us', time_zone=None)".to_owned(),
            "Duration" => "Duration(time_unit='us')".to_owned(),
            other => other.to_owned(),
        }),
        [Link::Attribute { name, .. }, Link::Call { arguments, .. }] => {
            let parameters: &[(&str, Option<&str>)] = match name.as_str() {
                "List" => &[("inner", None)],
                "Datetime" => &[("time_unit", Some("'us'")), ("time_zone", Some("None"))],
                "Duration" => &[("time_unit", Some("'us'"))],
                _ => return Err(not_one()),
            };
            let mut given: Vec<Option<&Expr>> = vec![None; parameters.len()];
            for (at, argument) in arguments.iter().enumerate() {
                let slot = match &argument.kind {
                    ArgumentKind::Positional(value) => Some((at, value)),
                    ArgumentKind::Keyword { name, value, .. } => parameters
                        .iter()
                        .position(|(known, _)| known == name)
                        .map(|slot| (slot, value)),
                    _ => None,
                };
                match slot {
                    Some((slot, value)) if slot < given.len() && given[slot].is_none() => {
                        given[slot] = Some(value);
                    }
                    _ => return Err(not_one()),
                }
            }
            let mut parts = Vec::with_capacity(parameters.len());
            for ((parameter, default), value) in parameters.iter().zip(given) {
                let part = match (value, *parameter) {
                    (Some(inner), "inner") => dtype_spelling(inner)?,
                    (Some(value), _) => match &value.kind {
                        ExprKind::Literal(literal) => match &literal.value {
                            LiteralValue::String(text) => format!("'{text}'"),
                            LiteralValue::None if *parameter == "time_zone" => "None".to_owned(),
                            _ => return Err(not_one()),
                        },
                        _ => return Err(not_one()),
                    },
                    (None, _) => match default {
                        Some(default) => (*default).to_owned(),
                        None => return Err(not_one()),
                    },
                };
                parts.push(if *parameter == "inner" {
                    part
                } else {
                    format!("{parameter}={part}")
                });
            }
            Ok(format!("{name}({})", parts.join(", ")))
        }
        _ => Err(not_one()),
    }
}
