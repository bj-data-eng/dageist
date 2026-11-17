//! The probe of a part the tables refuse and Polars allows.
//!
//! The editor holds every part of an expression to its types, whether or not
//! Polars evaluates it. Polars skips parts: it folds a comparison with a null,
//! decides a condition by a constant, drops a sort key that is one value, runs a
//! window's keys on groups. Where the tables refuse a part and Polars allows the
//! step, the target does not model what Polars skips. It asks Polars about the
//! part in a document where the part must be evaluated, in the order [`settle`],
//! the limits, [`settle_apart`]. What a probe shows is that the tables are right
//! about the part or the call when it is evaluated, which the target takes for a
//! part Polars skips; it is said in [`Settled::said`] as what it was.
//!
//! [`settle`] asks about the part as written, alone: the chain through a refused
//! link selected on its own, a condition with a refused predicate and literal
//! values selected on its own, either in the same input and, for a part of a pivot's
//! aggregate, in the same pivot. It settles the case when Polars refuses it for
//! the reason the tables gave (the objection and the call it names, not the dtypes
//! and the column names), at the refused call, and the part before the refused
//! call alone is not refused for that reason ([`Showing::TheSameReason`]).
//!
//! Polars folds some chains as they are written, so what follows is not evaluated
//! in that probe either. [`settle_apart`] asks about the refused part apart from
//! what surrounds it, in four ways, after the limits (which say what Polars
//! evaluates and accepts, as a literal it broadcasts or the nulls the sample's rows
//! leave, which these cannot tell from what it skips). The first three are settled
//! for the tables' reason as well ([`Showing::TheSameReason`]):
//!
//! - [`settle_on_a_value`]: the refused call, with its arguments, selected on a
//!   column of the dtype that Polars gave what reaches it, which Polars cannot fold.
//! - [`settle_as_a_key`]: a refusal that a join's key is not elementwise is of the
//!   position, so the part that holds the refused call is the whole key of the
//!   same join, and what comes before the call as a key is not refused for it:
//!   the chain that holds the key before the call that holds the refused call, and,
//!   for a refused call in an argument, the key with that argument cut before it.
//! - [`settle_on_a_value_as_a_key`]: a part that is one value is allowed as a key
//!   whatever it holds, so the refused call is asked on a column as the key.
//!
//! The fourth, [`settle_given_no_known_dtype`], is for a call the tables refused
//! without knowing the dtype of an argument: they refuse it for every dtype that
//! argument might have, with the reason Polars gave for one of them, so the reason
//! of a probe that has the argument's own dtype is not the tables' in words
//! ([`Showing::ForAnyDtypeOfTheArgument`]). Polars refusing the part as written, for
//! itself, at the refused call, with the part before the call allowed, shows the
//! refusal right. It shows that Polars refuses the part at that call, not why: a
//! refusal there for another argument or option would count too, and the reason the
//! editor shows for such a call is the tables' for a stand-in dtype, not Polars' for
//! the argument's own.
//!
//! The last has no reason to match. [`settle_in_the_position`] is for a refusal
//! that is the target's own comparison of a call's plan with its rows, which is no
//! refusal of Polars and so is neither the tables' reason nor one a probe is refused
//! for. It settles the case only when Polars' own plan and rows for the call on a
//! column differ as the tables say, Polars refuses the call there in the position
//! the case holds it, and a column whose plan and rows agree and the column alone
//! are allowed there ([`Showing::TheSchemaMismatch`]). A probe settles nothing when
//! Polars allows it, refuses it for another reason, or refuses it at another call;
//! one that a control meets too is not the mismatch's.

use dageist_engine::allowed::Representative;
use dageist_engine::catalog;
use dageist_engine::document::{
    Call, Document, Expression, Place, Segment, Start, Step, StepId, Value,
};

use super::case::{Case, STEP};
use super::frames::STAND_IN;
use super::observe::{
    Observed, Refusal, Seen, Tables, is_bare_key, is_the_targets_own, observe_probe,
};
use super::predict::{Prediction, Refused, Source};

/// What a probe showed of the refused part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Showing {
    /// Polars refused the probe for the reason the tables gave.
    TheSameReason,
    /// The tables' reason is the target's own comparison of a call's plan with its
    /// rows. Polars' plan and rows for the call on a column differ as it says, the
    /// call on that column is refused by Polars in the position, and a column
    /// whose plan and rows agree is not.
    TheSchemaMismatch,
    /// The tables refused the call for every dtype that an argument they did not
    /// know might have, and Polars refuses the part as written at the call, for
    /// the dtype the argument has.
    ForAnyDtypeOfTheArgument,
}

/// What a probe found: the document it ran, why Polars refused it, and what that
/// showed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settled {
    pub probe: String,
    pub reason: String,
    pub showing: Showing,
}

impl Settled {
    /// What the probe showed, in words, as the outcome and the replay say it.
    pub fn said(&self) -> String {
        match self.showing {
            Showing::TheSameReason => format!(
                "the probe {} is refused for the tables' reason: {}",
                self.probe, self.reason
            ),
            Showing::TheSchemaMismatch => format!(
                "the probe {} is refused by Polars ({}), where the call on a column of the dtype that reaches it has a plan and rows that differ as the tables say and one whose plan and rows agree is allowed",
                self.probe, self.reason
            ),
            Showing::ForAnyDtypeOfTheArgument => format!(
                "the probe {} is refused by Polars at the call the tables refused for every dtype of an argument they did not know: {}",
                self.probe, self.reason
            ),
        }
    }
}

/// The rules that a predicate be a `Boolean`, which Polars holds as it
/// evaluates the predicate.
const PREDICATE_RULES: [&str; 2] = [
    "condition-predicate-is-boolean",
    "filter-predicate-is-boolean",
];

/// The innermost expression that holds the place, and what of the place is
/// left under it.
struct Found<'a> {
    expression: &'a Expression,
    rest: &'a [Segment],
    /// Every expression the place is inside, outermost first, and how much of
    /// the place is consumed on entering it.
    levels: Vec<(&'a Expression, usize)>,
    /// Whether the place is in a pivot's aggregate, where `element` stands.
    in_the_aggregate: bool,
    /// Whether the place is in a join's `right_on`, read against the other table.
    on_the_other_table: bool,
}

enum Node<'a> {
    Call(&'a Call),
    Value(&'a Value),
    Expression(&'a Expression),
}

fn enter(value: &Value) -> Node<'_> {
    match value {
        Value::Expression(expression) => Node::Expression(expression),
        other => Node::Value(other),
    }
}

fn find<'a>(case: &'a Case, place: &'a Place) -> Option<Found<'a>> {
    let path = place.path();
    let step = case.document.steps()[0].call();
    let mut node = Node::Call(step);
    let mut held: Option<(&Expression, usize)> = None;
    let mut levels: Vec<(&Expression, usize)> = Vec::new();
    for (at, segment) in path.iter().enumerate() {
        node = match (node, segment) {
            (Node::Call(call), Segment::Parameter(name)) => enter(call.argument(name)?),
            (Node::Value(Value::Many(items)), Segment::Item(item)) => enter(items.get(*item)?),
            (Node::Value(Value::Pairs(pairs)), Segment::Key(item)) => enter(&pairs.get(*item)?.0),
            (Node::Value(Value::Pairs(pairs)), Segment::Value(item)) => enter(&pairs.get(*item)?.1),
            (Node::Expression(expression), Segment::Start) => match expression.start() {
                Start::Call(call) => Node::Call(call),
                Start::Input(_) | Start::Hole => return None,
            },
            (Node::Expression(expression), Segment::Link(link)) => {
                Node::Call(expression.links().get(*link)?)
            }
            _ => return None,
        };
        if let Node::Expression(expression) = &node {
            held = Some((expression, at + 1));
            levels.push((expression, at + 1));
        }
    }
    let (expression, consumed) = held?;
    Some(Found {
        expression,
        rest: &path[consumed..],
        levels,
        in_the_aggregate: matches!(
            path.first(),
            Some(Segment::Parameter(name)) if name == "aggregate_function"
        ) && step.entry().id().as_str() == "LazyFrame.pivot",
        on_the_other_table: matches!(
            path.first(),
            Some(Segment::Parameter(name)) if name == "right_on"
        ),
    })
}

fn call(id: &str, given: Vec<(&str, Value)>) -> Option<Call> {
    Call::new(catalog::entry(id)?, given).ok()
}

fn literal(whole: i64) -> Option<Expression> {
    let lit = call("polars.lit", vec![("value", Value::Integer(whole))])?;
    Expression::new(Start::Call(lit), Vec::new()).ok()
}

/// The chain through the call that `at` names: the start, or the links up to the
/// one; the whole expression for none.
fn chain_up_to(expression: &Expression, at: Option<&Segment>) -> Option<Expression> {
    match at {
        None => Some(expression.clone()),
        Some(Segment::Start) => Expression::new(expression.start().clone(), Vec::new()).ok(),
        Some(Segment::Link(link)) => Expression::new(
            expression.start().clone(),
            expression.links().get(..=*link)?.to_vec(),
        )
        .ok(),
        Some(_) => None,
    }
}

/// The chain through the refused call.
fn chain_through(found: &Found) -> Option<Expression> {
    chain_up_to(found.expression, found.rest.first())
}

/// A condition with `predicate` and literal values: nothing in it but the
/// predicate can be refused.
fn condition_of(predicate: Expression) -> Option<Expression> {
    let when = call(
        "polars.when",
        vec![(
            "predicates",
            Value::Many(vec![Value::Expression(predicate)]),
        )],
    )?;
    let then = call(
        "When.then",
        vec![("statement", Value::Expression(literal(1)?))],
    )?;
    let otherwise = call(
        "Then.otherwise",
        vec![("statement", Value::Expression(literal(2)?))],
    )?;
    Expression::new(Start::Call(when), vec![then, otherwise]).ok()
}

/// The document that evaluates `part` where Polars must: in a pivot's own
/// aggregate when the part is one, a filter's predicate for the rule about it,
/// and a plain `select` otherwise.
fn document_of(case: &Case, found: &Found, part: Expression, filter: bool) -> Option<Document> {
    let original = case.document.steps()[0].call();
    let step = if found.in_the_aggregate {
        let given: Vec<(&str, Value)> = original
            .arguments()
            .iter()
            .map(|given| {
                let value = if given.parameter() == "aggregate_function" {
                    Value::Expression(part.clone())
                } else {
                    given.value().clone()
                };
                (given.parameter(), value)
            })
            .collect();
        call("LazyFrame.pivot", given)?
    } else if filter {
        call(
            "LazyFrame.filter",
            vec![("predicates", Value::Many(vec![Value::Expression(part)]))],
        )?
    } else {
        call(
            "LazyFrame.select",
            vec![("exprs", Value::Many(vec![Value::Expression(part)]))],
        )?
    };
    Document::new(vec![Step::new(StepId::new(STEP).ok()?, step).ok()?]).ok()
}

/// The names of the sample's columns, which the reason of a probe may quote
/// where the tables' does not: the part is the same, the column is not.
const COLUMN_NAMES: [&str; 8] = ["ra", "rb", "rf", "rk", "rg", "rh", "key", "literal"];

/// What a symbol quoted in a reason says, as letters: the comparison or the
/// operation that Polars names is the call it is about.
fn symbol_word(symbol: &str) -> String {
    symbol
        .chars()
        .map(|character| match character {
            '<' => "lt",
            '>' => "gt",
            '=' => "eq",
            '!' => "not",
            '+' => "add",
            '-' => "sub",
            '*' => "mul",
            '/' => "div",
            '%' => "rem",
            '&' => "and",
            '|' => "or",
            '^' => "xor",
            _ => "",
        })
        .collect()
}

/// The words of a reason that say what Polars objected to, and to which call:
/// what is quoted, in parentheses or a dtype is the part's, not the objection's,
/// but what is quoted in backticks that is no dtype or column, and the symbol of
/// a comparison or an operation, name the call.
fn objection(reason: &str) -> String {
    const DTYPES: [&str; 20] = [
        "str",
        "string",
        "bool",
        "boolean",
        "date",
        "datetime",
        "time",
        "duration",
        "list",
        "struct",
        "array",
        "decimal",
        "null",
        "binary",
        "categorical",
        "enum",
        "object",
        "unknown",
        "int",
        "float",
    ];
    let words_of = |text: &str| -> Vec<String> {
        text.to_ascii_lowercase()
            .split(|character: char| !character.is_ascii_alphabetic())
            .filter(|word| word.len() > 1)
            .map(str::to_owned)
            .collect()
    };
    let mut kept = String::new();
    let mut depth = 0_u32;
    // The closing mark of what is quoted, and what is quoted so far.
    let mut quoted: Option<(char, String)> = None;
    let mut previous = ' ';
    for character in reason.chars() {
        let opens_a_name = character == '\'' && !previous.is_ascii_alphabetic();
        previous = character;
        if let Some((closer, content)) = quoted.as_mut() {
            if character != *closer {
                content.push(character);
                continue;
            }
            let content = std::mem::take(content);
            let closer = *closer;
            quoted = None;
            if depth > 0 {
                continue;
            }
            kept.push(' ');
            if closer == '`' {
                for word in words_of(&content) {
                    if !DTYPES.contains(&word.as_str()) && !COLUMN_NAMES.contains(&word.as_str()) {
                        kept.push_str(&word);
                        kept.push(' ');
                    }
                }
            } else if !content.chars().any(|character| character.is_alphanumeric()) {
                kept.push_str(&symbol_word(&content));
                kept.push(' ');
            }
            continue;
        }
        match character {
            '`' => quoted = Some(('`', String::new())),
            '\'' if opens_a_name => quoted = Some(('\'', String::new())),
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            _ => kept.push(character),
        }
    }
    words_of(&kept)
        .into_iter()
        .filter(|word| !DTYPES.contains(&word.as_str()))
        .take(8)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether `probe` is refused as the tables refused the part: for Polars' own
/// reason the same objection, and for a rule that a predicate be a `Boolean`
/// Polars' words for it.
fn same_reason(source: &Source, tables: &str, probe: &str) -> bool {
    match source {
        // What the target says of a schema is no objection of Polars, so it is
        // neither the tables' reason nor one that a probe can be refused for.
        Source::Polars => {
            !is_the_targets_own(tables)
                && !is_the_targets_own(probe)
                && !objection(tables).is_empty()
                && objection(tables) == objection(probe)
        }
        Source::Rule(rule) if PREDICATE_RULES.contains(rule) => {
            let probe = probe.to_ascii_lowercase();
            probe.contains("boolean") && (probe.contains("expected") || probe.contains("must be"))
        }
        _ => false,
    }
}

/// The tables a probe of the part runs on.
fn tables_of(found: &Found, stand_in: Option<Representative>) -> Tables {
    Tables {
        other_as_left: found.on_the_other_table,
        stand_in,
    }
}

/// A probe: the part to evaluate, and where in it Polars' refusal may lie.
struct Probe {
    part: Expression,
    /// Whether the part is a filter's predicate.
    filter: bool,
    /// The part before the refused call, which must not be refused for the same
    /// reason, for the call is then not why the probe is.
    before: Option<Expression>,
    /// The places of the part, in the probe's document, that are the refused
    /// call or predicate.
    at: Vec<Place>,
}

/// The place in the probe's document that holds the part.
fn place_of_the_part(found: &Found, filter: bool) -> Place {
    let origin = Place::of_step(0, StepId::new(STEP).ok());
    if found.in_the_aggregate {
        origin.then(Segment::Parameter("aggregate_function".to_owned()))
    } else if filter {
        origin
            .then(Segment::Parameter("predicates".to_owned()))
            .then(Segment::Item(0))
    } else {
        origin
            .then(Segment::Parameter("exprs".to_owned()))
            .then(Segment::Item(0))
    }
}

/// The probe of the part the tables refused, by who refused it.
fn probe_of_the_part(found: &Found, source: &Source) -> Option<Probe> {
    match source {
        Source::Polars => {
            let part = chain_through(found)?;
            let held = place_of_the_part(found, false);
            let (at, before) = match found.rest.first() {
                None => (held, None),
                Some(Segment::Start) => (held.then(Segment::Start), None),
                Some(Segment::Link(link)) => {
                    let links = found.expression.links();
                    let before = Expression::new(
                        found.expression.start().clone(),
                        links.get(..*link)?.to_vec(),
                    )
                    .ok()?;
                    (held.then(Segment::Link(*link)), Some(before))
                }
                Some(_) => return None,
            };
            Some(Probe {
                part,
                filter: false,
                before,
                at: vec![at],
            })
        }
        Source::Rule("condition-predicate-is-boolean") => {
            let predicate = found_predicate(found)?;
            let held = place_of_the_part(found, false).then(Segment::Start);
            let inside = held
                .clone()
                .then(Segment::Parameter("predicates".to_owned()))
                .then(Segment::Item(0));
            Some(Probe {
                part: condition_of(predicate)?,
                filter: false,
                before: None,
                at: vec![held, inside],
            })
        }
        Source::Rule("filter-predicate-is-boolean") => Some(Probe {
            part: found_predicate(found)?,
            filter: true,
            before: None,
            at: vec![place_of_the_part(found, true)],
        }),
        _ => None,
    }
}

/// Whether a refusal lies where the probe's part is refused. Polars says where
/// only when it refused while it built the step, and then not always: no place,
/// or the step as a whole, says nothing against it.
fn lies_at(refusal: &Refusal, at: &[Place]) -> bool {
    refusal
        .place
        .as_ref()
        .is_none_or(|place| place.path().is_empty() || at.contains(place))
}

/// How often a probe is run again while Polars names another failure of it than
/// the tables' reason.
const ATTEMPTS: usize = 16;

/// Polars refusing `probed` as the tables refused the part, if it does. A probe
/// may fail in two ways at once, and which Polars names depends on which of its
/// threads gets there first (the tables are measured on one thread for this), so
/// a probe that Polars refuses for another reason is run again, up to
/// [`ATTEMPTS`] times, before it is taken for one that is not refused as the
/// tables' reason says. A probe that Polars allows is allowed every time.
fn refused_as(probed: &Case, tables: Tables, refused: &Refused, at: &[Place]) -> Option<Refusal> {
    (0..ATTEMPTS).find_map(|_| {
        // `None` ends the search with the probe allowed, which is no refusal.
        let refusal = observe_probe(probed, tables).refusal;
        match refusal {
            Some(refusal)
                if same_reason(&refused.source, &refused.reason, &refusal.reason)
                    && lies_at(&refusal, at) =>
            {
                Some(Some(refusal))
            }
            Some(_) => None,
            None => Some(None),
        }
    })?
}

/// Whether Polars refuses `probed` for the tables' reason, at any place, in one of
/// [`ATTEMPTS`] runs.
fn refused_anywhere_as(probed: &Case, tables: Tables, refused: &Refused) -> bool {
    (0..ATTEMPTS).any(|_| {
        observe_probe(probed, tables)
            .refusal
            .is_some_and(|refusal| same_reason(&refused.source, &refused.reason, &refusal.reason))
    })
}

/// Asks Polars about the part the tables refused at `refused`, alone. `None`
/// when there is nothing to ask, Polars allows the probe, it refuses it for
/// another reason than the tables did, or not at the refused call: a refusal at
/// an earlier call, or one that is not at any place while the part before the
/// call is refused for the same reason, is not the refused call's.
pub fn settle(case: &Case, refused: &Refused) -> Option<Settled> {
    let found = find(case, &refused.place)?;
    let probe = probe_of_the_part(&found, &refused.source)?;
    let run = |part: Expression, filter: bool| {
        let document = document_of(case, &found, part, filter)?;
        Some(Case {
            receiver: case.receiver,
            argument: case.argument,
            document,
        })
    };
    let probed = run(probe.part, probe.filter)?;
    let refusal = refused_as(&probed, tables_of(&found, None), refused, &probe.at)?;
    if let Some(before) = probe.before {
        let alone = run(before, false)?;
        if refused_anywhere_as(&alone, tables_of(&found, None), refused) {
            return None;
        }
    }
    Some(Settled {
        probe: probed.document.write(),
        reason: refusal.reason,
        showing: Showing::TheSameReason,
    })
}

/// The class of the dtype that reaches the call at `link` of the expression at
/// `held`, which Polars gave it when it ran the case and the tables predicted:
/// the tables' dtype is not trusted where Polars' is not the same, for then the
/// case is a disagreement about the dtype, not a part Polars skips.
fn dtype_reaching(
    held: &Place,
    link: usize,
    predicted: &Prediction,
    observed: &Observed,
) -> Option<Representative> {
    let seen = observed
        .expressions
        .iter()
        .find(|seen| seen.place == *held)?;
    let traced = predicted
        .expressions
        .iter()
        .find(|traced| traced.place == *held)?;
    let (found, said) = match link.checked_sub(1) {
        None => (&seen.start, traced.start),
        Some(before) => (seen.links.get(before)?, *traced.links.get(before)?),
    };
    match found {
        Seen::Dtype(_, _, Some(class)) if Some(*class) == said => Some(*class),
        _ => None,
    }
}

/// Asks Polars about the call the tables refused where it cannot be asked
/// as it is written: the chain before it may give a constant, or a column of
/// nulls, however the rows are, and Polars then does not evaluate the call. The
/// probe selects the refused call, with its arguments as written, on a column
/// of the dtype that Polars gave what reaches it, which is a value Polars cannot
/// fold. The dtype is Polars' own as well as the tables': a wrong prediction of
/// it is a disagreement of the dtypes, found before anything is probed.
///
/// What this shows is that the tables are right about the call, not that Polars
/// skipped it: a call that Polars evaluates and accepts, as one that a literal
/// broadcasts or a null that the sample's rows leave, is a limit's, which is
/// asked first. `None` when there is nothing to ask, Polars allows the probe,
/// or it refuses it for another reason or not at the call.
pub fn settle_on_a_value(
    case: &Case,
    refused: &Refused,
    predicted: &Prediction,
    observed: &Observed,
) -> Option<Settled> {
    if !matches!(refused.source, Source::Polars) {
        return None;
    }
    let found = find(case, &refused.place)?;
    let [Segment::Link(link)] = found.rest else {
        return None;
    };
    if found.in_the_aggregate {
        return None;
    }
    let consumed = refused.place.path().len() - found.rest.len();
    let held = refused.place.path()[..consumed]
        .iter()
        .cloned()
        .fold(Place::of_step(0, StepId::new(STEP).ok()), Place::then);
    let class = dtype_reaching(&held, *link, predicted, observed)?;
    let name = call(
        "polars.col",
        vec![("name", Value::Columns(vec![STAND_IN.to_owned()]))],
    )?;
    let part = Expression::new(
        Start::Call(name),
        vec![found.expression.links().get(*link)?.clone()],
    )
    .ok()?;
    let probed = Case {
        receiver: case.receiver,
        argument: case.argument,
        document: document_of(case, &found, part, false)?,
    };
    let at = place_of_the_part(&found, false).then(Segment::Link(0));
    let refusal = refused_as(&probed, tables_of(&found, Some(class)), refused, &[at])?;
    Some(Settled {
        probe: probed.document.write(),
        reason: refusal.reason,
        showing: Showing::TheSameReason,
    })
}

/// The parameters of a join that hold its keys.
const KEYS: [&str; 3] = ["on", "left_on", "right_on"];

/// The join of the case with the key that `parameter` holds replaced by `part`.
/// Another key must be the bare `key` column, so that the refusal can only be
/// the part's.
fn join_on(case: &Case, parameter: &str, part: &Expression) -> Option<Document> {
    let original = case.document.steps()[0].call();
    if original.entry().id().as_str() != "LazyFrame.join" {
        return None;
    }
    let mut given: Vec<(&str, Value)> = Vec::new();
    for argument in original.arguments() {
        let name = argument.parameter();
        let value = if !KEYS.contains(&name) {
            argument.value().clone()
        } else {
            let Value::Many(items) = argument.value() else {
                return None;
            };
            match items.as_slice() {
                [_] if name == parameter => Value::Many(vec![Value::Expression(part.clone())]),
                [Value::Expression(key)] if name != parameter && is_bare_key(key) => {
                    argument.value().clone()
                }
                _ => return None,
            }
        };
        given.push((name, value));
    }
    Document::new(vec![
        Step::new(StepId::new(STEP).ok()?, call("LazyFrame.join", given)?).ok()?,
    ])
    .ok()
}

/// `value` with the expression that `path` leads to replaced by `with`.
fn replaced_in_value(value: &Value, path: &[Segment], with: &Expression) -> Option<Value> {
    match (value, path) {
        (Value::Expression(_), []) => Some(Value::Expression(with.clone())),
        (Value::Many(items), [Segment::Item(at), rest @ ..]) => {
            let mut items = items.clone();
            *items.get_mut(*at)? = replaced_in_value(items.get(*at)?, rest, with)?;
            Some(Value::Many(items))
        }
        (Value::Pairs(pairs), [Segment::Key(at), rest @ ..]) => {
            let mut pairs = pairs.clone();
            pairs.get_mut(*at)?.0 = replaced_in_value(&pairs.get(*at)?.0, rest, with)?;
            Some(Value::Pairs(pairs))
        }
        (Value::Pairs(pairs), [Segment::Value(at), rest @ ..]) => {
            let mut pairs = pairs.clone();
            pairs.get_mut(*at)?.1 = replaced_in_value(&pairs.get(*at)?.1, rest, with)?;
            Some(Value::Pairs(pairs))
        }
        _ => None,
    }
}

/// `expression` with the expression that `path` leads to, from one of its links and
/// through the arguments of that link, replaced by `with`.
fn replaced_inside(
    expression: &Expression,
    path: &[Segment],
    with: &Expression,
) -> Option<Expression> {
    let [Segment::Link(link), Segment::Parameter(name), rest @ ..] = path else {
        return None;
    };
    let held = expression.links().get(*link)?;
    let mut given: Vec<(&str, Value)> = Vec::new();
    for argument in held.arguments() {
        let value = if argument.parameter() == name {
            replaced_in_value(argument.value(), rest, with)?
        } else {
            argument.value().clone()
        };
        given.push((argument.parameter(), value));
    }
    let mut links = expression.links().to_vec();
    links[*link] = call(held.entry().id().as_str(), given)?;
    Expression::new(expression.start().clone(), links).ok()
}

/// Asks Polars about the part of a join's key that the tables refused as not
/// elementwise, standing alone as the key. The refusal is of the position, which
/// no column or value shows, and Polars does not meet the part as a key where it
/// folds what it is in away, as a comparison with a null: the part that holds
/// the refused call, with the arguments as written, is the whole key of the same
/// join. As with [`settle_on_a_value`], what this shows is that the tables are
/// right about the part as a key. A refusal is the refused call's only when what
/// comes before it is not refused alone for the same reason: for a call in an
/// argument of another call, the key chain before that call, and the part with the
/// argument cut before the refused call, whose own links are not the key's.
pub fn settle_as_a_key(case: &Case, refused: &Refused) -> Option<Settled> {
    if !matches!(refused.source, Source::Polars) || !refused.reason.contains("elementwise") {
        return None;
    }
    let found = find(case, &refused.place)?;
    let Some(Segment::Parameter(parameter)) = refused.place.path().first() else {
        return None;
    };
    if !KEYS.contains(&parameter.as_str()) {
        return None;
    }
    // What holds the refused call: the expression that the place is directly in,
    // through its call that holds the rest of the place, which is the part.
    let (holder, owner) = match found.levels.as_slice() {
        [.., (parent, entered), _] => (*parent, refused.place.path().get(*entered)),
        _ => (found.expression, found.rest.first()),
    };
    let part = chain_up_to(holder, owner)?;
    // What comes before the refused call, as a key, must not be refused for the
    // same reason: the refused call is then not why.
    let before = match owner {
        Some(Segment::Link(link)) => Some(
            Expression::new(
                holder.start().clone(),
                holder.links().get(..*link)?.to_vec(),
            )
            .ok()?,
        ),
        _ => None,
    };
    // Where the refused call is in an argument of the call that holds it, what comes
    // before it in that argument is a part of its own, which must not be refused for
    // the same reason either: the part that holds the argument has it whole, and the
    // chain that holds the key does not show it. The part with that argument cut
    // before the refused call is that part.
    let inner_before = match (found.levels.as_slice(), found.rest) {
        ([.., (_, entered), (_, inner)], [Segment::Link(link)]) => {
            let held = found.expression;
            let cut =
                Expression::new(held.start().clone(), held.links().get(..*link)?.to_vec()).ok()?;
            let way = refused.place.path().get(*entered..*inner)?;
            Some(replaced_inside(&part, way, &cut)?)
        }
        _ => None,
    };
    let probed = Case {
        receiver: case.receiver,
        argument: case.argument,
        document: join_on(case, parameter, &part)?,
    };
    let at = Place::of_step(0, StepId::new(STEP).ok())
        .then(Segment::Parameter(parameter.clone()))
        .then(Segment::Item(0));
    let refusal = refused_as(&probed, Tables::default(), refused, &[at])?;
    for before in before.into_iter().chain(inner_before) {
        let alone = Case {
            receiver: case.receiver,
            argument: case.argument,
            document: join_on(case, parameter, &before)?,
        };
        if refused_anywhere_as(&alone, Tables::default(), refused) {
            return None;
        }
    }
    Some(Settled {
        probe: probed.document.write(),
        reason: refusal.reason,
        showing: Showing::TheSameReason,
    })
}

/// Asks Polars about the call the tables refused as not elementwise in a join's
/// key, on a column, as the whole key. The part that holds the call may be one
/// value, as an aggregate then a running value is, which Polars allows as a key
/// whatever it holds; where the key is folded, Polars never meets the call as
/// anything else. The refused call, with its arguments, on a column of the dtype
/// that reaches it, as the key of the same join, is the call as a key that is not
/// one value. The dtype is Polars' as well as the tables', as for
/// [`settle_on_a_value`].
pub fn settle_on_a_value_as_a_key(
    case: &Case,
    refused: &Refused,
    predicted: &Prediction,
    observed: &Observed,
) -> Option<Settled> {
    if !matches!(refused.source, Source::Polars) || !refused.reason.contains("elementwise") {
        return None;
    }
    let found = find(case, &refused.place)?;
    let Some(Segment::Parameter(parameter)) = refused.place.path().first() else {
        return None;
    };
    let [Segment::Link(link)] = found.rest else {
        return None;
    };
    if !KEYS.contains(&parameter.as_str()) {
        return None;
    }
    let consumed = refused.place.path().len() - found.rest.len();
    let held = refused.place.path()[..consumed]
        .iter()
        .cloned()
        .fold(Place::of_step(0, StepId::new(STEP).ok()), Place::then);
    let class = dtype_reaching(&held, *link, predicted, observed)?;
    let name = call(
        "polars.col",
        vec![("name", Value::Columns(vec![STAND_IN.to_owned()]))],
    )?;
    let part = Expression::new(
        Start::Call(name),
        vec![found.expression.links().get(*link)?.clone()],
    )
    .ok()?;
    let probed = Case {
        receiver: case.receiver,
        argument: case.argument,
        document: join_on(case, parameter, &part)?,
    };
    let at = Place::of_step(0, StepId::new(STEP).ok())
        .then(Segment::Parameter(parameter.clone()))
        .then(Segment::Item(0));
    let tables = Tables {
        other_as_left: false,
        stand_in: Some(class),
    };
    let refusal = refused_as(&probed, tables, refused, &[at])?;
    Some(Settled {
        probe: probed.document.write(),
        reason: refusal.reason,
        showing: Showing::TheSameReason,
    })
}

/// Polars refusing `probed`, for itself and not by what the target says of a
/// schema, at `at` if it says where; asked again while it is allowed or refused
/// elsewhere, as for [`refused_as`].
fn refused_by_polars(probed: &Case, tables: Tables, at: &[Place]) -> Option<Refusal> {
    (0..ATTEMPTS).find_map(|_| match observe_probe(probed, tables).refusal {
        Some(refusal) if !is_the_targets_own(&refusal.reason) && lies_at(&refusal, at) => {
            Some(Some(refusal))
        }
        Some(_) => None,
        None => Some(None),
    })?
}

/// The step of the case with the one item of the parameter `parameter` replaced
/// by `part`, which is the step with the expression the position holds changed.
fn step_with(case: &Case, parameter: &str, part: &Expression) -> Option<Document> {
    let original = case.document.steps()[0].call();
    let mut given: Vec<(&str, Value)> = Vec::new();
    for argument in original.arguments() {
        let value = if argument.parameter() == parameter {
            match argument.value() {
                Value::Many(items) if items.len() == 1 => {
                    Value::Many(vec![Value::Expression(part.clone())])
                }
                _ => return None,
            }
        } else {
            argument.value().clone()
        };
        given.push((argument.parameter(), value));
    }
    Document::new(vec![
        Step::new(
            StepId::new(STEP).ok()?,
            call(original.entry().id().as_str(), given)?,
        )
        .ok()?,
    ])
    .ok()
}

/// The classes a call is tried on as a control: whole numbers and floats, of which
/// plan and rows agree for the calls that give them.
const CONTROLS: [Representative; 2] = [Representative::Int64, Representative::Float64];

/// Whether what was seen ties a refusal in a position to a mismatch of plan and
/// rows: Polars' own plan and rows for the call differ as the tables say, the call
/// is refused in the position, and neither a call whose plan and rows agree nor
/// the column alone is. A refusal that a control meets too is not the mismatch's.
fn ties_the_refusal_to_the_mismatch(
    differs_as_the_tables_say: bool,
    refused_in_the_position: bool,
    control_is_allowed: bool,
    column_alone_is_allowed: bool,
) -> bool {
    differs_as_the_tables_say
        && refused_in_the_position
        && control_is_allowed
        && column_alone_is_allowed
}

/// Asks Polars about a call that the tables refuse by what the target says of a
/// schema, a plan that says one dtype and rows that give another, in the position
/// the case holds it, on a column. No probe of the call alone shows that
/// comparison, which is the target's own, and Polars skips a part that is one
/// value, as a sort key. What shows the refusal is the mismatch, together:
///
/// - Polars' plan and rows for the call on a column of the dtype that reaches it,
///   selected on its own, differ exactly as the tables' reason says (the same dtypes);
/// - Polars refuses the call on that column in the position, which a refusal of the
///   target's own comparison alone does not;
/// - the call on a column of a dtype whose plan and rows agree is allowed in the
///   position, and so is the column alone.
///
/// A refusal that the control meets too, as a filter's predicate that is not a
/// `Boolean`, or a panic that has no part in the mismatch, settles nothing, as does
/// Polars allowing the position.
pub fn settle_in_the_position(
    case: &Case,
    refused: &Refused,
    predicted: &Prediction,
    observed: &Observed,
) -> Option<Settled> {
    if !matches!(refused.source, Source::Polars) || !is_the_targets_own(&refused.reason) {
        return None;
    }
    let found = find(case, &refused.place)?;
    let [Segment::Link(link)] = found.rest else {
        return None;
    };
    let [Segment::Parameter(parameter), Segment::Item(_), ..] = refused.place.path() else {
        return None;
    };
    // The expression the position holds is the one the call is in.
    if found.in_the_aggregate || found.levels.len() != 1 {
        return None;
    }
    let consumed = refused.place.path().len() - found.rest.len();
    let held = refused.place.path()[..consumed]
        .iter()
        .cloned()
        .fold(Place::of_step(0, StepId::new(STEP).ok()), Place::then);
    let class = dtype_reaching(&held, *link, predicted, observed)?;
    let name = call(
        "polars.col",
        vec![("name", Value::Columns(vec![STAND_IN.to_owned()]))],
    )?;
    let alone = Expression::new(Start::Call(name.clone()), Vec::new()).ok()?;
    let part = Expression::new(
        Start::Call(name),
        vec![found.expression.links().get(*link)?.clone()],
    )
    .ok()?;
    let probe_of = |part: &Expression| {
        Some(Case {
            receiver: case.receiver,
            argument: case.argument,
            document: step_with(case, parameter, part)?,
        })
    };
    let on = |class: Representative| Tables {
        other_as_left: false,
        stand_in: Some(class),
    };
    // Polars' own plan and rows for the call on a column, selected on its own.
    let selected = Case {
        receiver: case.receiver,
        argument: case.argument,
        document: document_of(case, &found, part.clone(), false)?,
    };
    let differs_as_the_tables_say = observe_probe(&selected, on(class))
        .refusal
        .is_some_and(|refusal| refusal.reason == refused.reason);
    if !differs_as_the_tables_say {
        return None;
    }
    // A control: a column whose plan and rows agree for the call.
    let control = CONTROLS
        .into_iter()
        .filter(|control| *control != class)
        .find(|control| observe_probe(&selected, on(*control)).refusal.is_none())?;
    let item = Place::of_step(0, StepId::new(STEP).ok())
        .then(Segment::Parameter(parameter.clone()))
        .then(Segment::Item(0));
    let probed = probe_of(&part)?;
    let refusal = refused_by_polars(
        &probed,
        on(class),
        &[item.clone(), item.then(Segment::Link(0))],
    );
    let control_is_allowed = observe_probe(&probed, on(control)).refusal.is_none();
    let column_alone_is_allowed = observe_probe(&probe_of(&alone)?, on(class))
        .refusal
        .is_none();
    ties_the_refusal_to_the_mismatch(
        differs_as_the_tables_say,
        refusal.is_some(),
        control_is_allowed,
        column_alone_is_allowed,
    )
    .then(|| Settled {
        probe: probed.document.write(),
        reason: refusal.map_or_else(String::new, |refusal| refusal.reason),
        showing: Showing::TheSchemaMismatch,
    })
}

/// Asks Polars about the part the tables refused apart from what surrounds it
/// in the case, where the part as written is not asked: the refused call on a
/// value of the dtype that reaches it, or a join's key by itself as the key, or
/// the call on a value as the key.
pub fn settle_apart(
    case: &Case,
    refused: &Refused,
    predicted: &Prediction,
    observed: &Observed,
) -> Option<Settled> {
    settle_on_a_value(case, refused, predicted, observed)
        .or_else(|| settle_as_a_key(case, refused))
        .or_else(|| settle_on_a_value_as_a_key(case, refused, predicted, observed))
        .or_else(|| settle_in_the_position(case, refused, predicted, observed))
        .or_else(|| settle_given_no_known_dtype(case, refused))
}

/// Asks Polars about a call the tables refused without knowing the dtype of an
/// argument, as it is written and alone. The tables refuse such a call only when
/// no dtype the argument might have lets it stand, and say the reason Polars gave
/// for one of them, so Polars refusing the probe for the argument's own dtype is
/// in other words: the reason is not compared. What shows the tables right is that
/// Polars refuses the part for itself at the refused call, and that the part before
/// the call alone is allowed, for a refusal of that is another call's. `None` when
/// the tables knew every dtype, the reason is the target's own comparison, there is
/// nothing to ask, or Polars allows the probe, refuses it elsewhere or refuses the
/// part before the call.
pub fn settle_given_no_known_dtype(case: &Case, refused: &Refused) -> Option<Settled> {
    if !refused.argument_unknown
        || !matches!(refused.source, Source::Polars)
        || is_the_targets_own(&refused.reason)
    {
        return None;
    }
    let found = find(case, &refused.place)?;
    let probe = probe_of_the_part(&found, &refused.source)?;
    let run = |part: Expression| {
        Some(Case {
            receiver: case.receiver,
            argument: case.argument,
            document: document_of(case, &found, part, false)?,
        })
    };
    let probed = run(probe.part)?;
    let tables = tables_of(&found, None);
    let refusal = refused_by_polars(&probed, tables, &probe.at)?;
    if let Some(before) = probe.before {
        let alone = run(before)?;
        if observe_probe(&alone, tables).refusal.is_some() {
            return None;
        }
    }
    Some(Settled {
        probe: probed.document.write(),
        reason: refusal.reason,
        showing: Showing::ForAnyDtypeOfTheArgument,
    })
}

/// The predicate a rule refused: the whole expression at its place.
fn found_predicate(found: &Found) -> Option<Expression> {
    found.rest.is_empty().then(|| found.expression.clone())
}

#[cfg(test)]
mod tests {
    //! The probe settles a case only where Polars refuses the part alone for the
    //! tables' reason: a probe that Polars allows, or refuses for another reason,
    //! settles nothing.

    use dageist_engine::catalog;
    use dageist_engine::document::{Place, Segment, StepId};

    use super::super::observe::observe;
    use super::super::predict::{Source, Verdict, predict};
    use super::*;

    fn kept(name: &str) -> Case {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/allowed_fuzz/regressions")
            .join(name);
        Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case")
    }

    fn refusal(case: &Case) -> Refused {
        match predict(case).verdict {
            Verdict::Refuses(refused) => refused,
            other => panic!("the tables refuse the case: {other:?}"),
        }
    }

    fn settled(name: &str) -> Settled {
        let case = kept(name);
        settle(&case, &refusal(&case)).expect("a probe that Polars refuses as the tables do")
    }

    #[test]
    fn a_later_predicate_after_a_null_fold_is_refused_alone_for_the_rule() {
        let found = settled("stricter-a-null-folded-before-a-later-predicate.case");
        assert!(found.reason.contains("Boolean"), "{}", found.reason);
        assert!(found.probe.contains("polars.when"), "{}", found.probe);
    }

    #[test]
    fn a_condition_decided_by_a_constant_is_refused_alone_for_its_predicate() {
        let found = settled("stricter-a-condition-decided-by-a-constant.case");
        assert!(found.reason.contains("Boolean"), "{}", found.reason);
        // The part is a pivot's, so the probe is the pivot.
        assert!(found.probe.contains("LazyFrame.pivot"), "{}", found.probe);
    }

    #[test]
    fn a_list_max_that_only_an_outer_window_allows_is_refused_alone_for_the_same_reason() {
        let found = settled("stricter-a-list-max-under-an-outer-window.case");
        assert!(found.reason.contains("not supported"), "{}", found.reason);
        assert!(found.probe.contains("Expr.max"), "{}", found.probe);
    }

    #[test]
    fn a_reason_is_matched_by_what_polars_objected_to_and_not_by_the_names_it_quotes() {
        let tables = "could not evaluate '<' comparison between series 'a' of dtype: Boolean and series 'b' of dtype: Date";
        let probe = "could not evaluate '<' comparison between series '_POLARS_TMP_PHYS_2' of dtype: Boolean and series '_POLARS_TMP_PHYS_3' of dtype: Date";
        assert!(same_reason(&Source::Polars, tables, probe));
        assert!(!same_reason(
            &Source::Polars,
            tables,
            "cannot compare string with numeric type (i8)"
        ));
    }

    #[test]
    fn a_comparison_in_a_sort_key_of_one_value_is_refused_alone_for_the_same_reason() {
        let found = settled("stricter-a-comparison-in-a-sort-key-of-one-value.case");
        assert!(found.reason.contains("comparison"), "{}", found.reason);
    }

    #[test]
    fn a_part_of_a_right_key_is_probed_against_the_other_table() {
        // The part reads `rh` and `rb`, which only the other table has: selected
        // from the left table the probe would be refused for no column.
        let found = settled("stricter-a-comparison-in-a-right-key-of-branches-that-are-one.case");
        assert!(found.reason.contains("compare"), "{}", found.reason);
    }

    /// A case that selects the chain `a` followed by the `links` (calls with no
    /// arguments) on columns of `receiver`.
    fn chain_of(receiver: &str, start: &str, links: &[&str]) -> Case {
        let links: Vec<String> = links
            .iter()
            .map(|id| format!("{{\"entry\":\"{id}\",\"arguments\":{{}}}}"))
            .collect();
        let text = format!(
            "{{\"a\":\"{receiver}\",\"b\":\"Int64\",\"revision\":1}}\n{{\"version\":1,\"steps\":[{{\"id\":\"s\",\"call\":{{\"entry\":\"LazyFrame.select\",\"arguments\":{{\"exprs\":{{\"many\":[{{\"expression\":{{\"start\":{{\"entry\":\"polars.col\",\"arguments\":{{\"name\":{{\"columns\":[\"{start}\"]}}}}}},\"then\":[{}]}}}}]}}}}}}}}]}}",
            links.join(",")
        );
        Case::decode(text.as_bytes()).expect("a case")
    }

    /// A case that selects the one chain `a.cum_sum()` on columns of `receiver`.
    fn cum_sum_of(receiver: &str) -> Case {
        chain_of(receiver, "a", &["Expr.cum_sum"])
    }

    fn refused_at_the_link(reason: &str) -> Refused {
        refused_at_link(0, reason)
    }

    fn refused_at_link(link: usize, reason: &str) -> Refused {
        let place = Place::of_step(0, Some(StepId::new("s").expect("an identifier")))
            .then(Segment::Parameter("exprs".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Link(link));
        Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage: None,
            reason: reason.to_owned(),
            argument_unknown: false,
        }
    }

    #[test]
    fn a_probe_that_polars_allows_settles_nothing() {
        // `a.cum_sum()` of whole numbers is allowed, though the tables are said
        // to refuse it: nothing shows the tables right.
        let case = cum_sum_of("Int64");
        let refused = refused_at_the_link("`cum_sum` operation not supported for dtype `i64`");
        assert_eq!(settle(&case, &refused), None);
    }

    #[test]
    fn a_probe_that_polars_refuses_for_another_reason_settles_nothing() {
        // Polars refuses `cum_sum` of dates as not supported, which is not a
        // comparison of text with a number.
        let case = cum_sum_of("Date");
        let wrong = refused_at_the_link("cannot compare string with numeric type (u32)");
        assert_eq!(settle(&case, &wrong), None);
        // For the reason the tables give, it is refused alone.
        let right = refused_at_the_link("`cum_sum` operation not supported for dtype `date`");
        assert!(settle(&case, &right).is_some());
    }

    #[test]
    fn a_refusal_of_an_earlier_call_than_the_refused_one_settles_nothing() {
        // `a.cum_sum().cum_sum()` of dates: Polars refuses the first running sum
        // for the reason the tables give for the second. The probe through the
        // second is refused, but not at it, so the second is not shown refused.
        let case = chain_of("Date", "a", &["Expr.cum_sum", "Expr.cum_sum"]);
        let reason = "`cum_sum` operation not supported for dtype `date`";
        assert_eq!(settle(&case, &refused_at_link(1, reason)), None);
        // The first is refused at itself.
        assert!(settle(&case, &refused_at_link(0, reason)).is_some());
    }

    #[test]
    fn an_earlier_call_that_polars_refuses_when_it_runs_rows_settles_nothing() {
        // Taking the days of a `Boolean` is refused when the rows run, which has no
        // place, so the probe through a second call that takes them is refused
        // and nothing says at which call. The part before it is refused alone for
        // the same reason, so the second call is not the cause.
        let case = chain_of(
            "Boolean",
            "a",
            &["Expr.dt.total_days", "Expr.dt.total_days"],
        );
        let first = chain_of("Boolean", "a", &["Expr.dt.total_days"]);
        let reason = observe_probe(&first, Tables::default())
            .refusal
            .expect("Polars refuses the days of a Boolean")
            .reason;
        assert_eq!(settle(&case, &refused_at_link(1, &reason)), None);
        assert!(settle(&first, &refused_at_link(0, &reason)).is_some());
    }

    #[test]
    fn the_same_words_about_another_call_are_not_the_same_objection() {
        assert!(!same_reason(
            &Source::Polars,
            "`cum_sum` operation not supported for dtype `date`",
            "`cum_max` operation not supported for dtype `date`"
        ));
        assert!(!same_reason(
            &Source::Polars,
            "could not evaluate '<' comparison between series 'a' of dtype: Boolean and series 'b' of dtype: Date",
            "could not evaluate '>' comparison between series 'a' of dtype: Boolean and series 'b' of dtype: Date"
        ));
    }

    /// What the tables predict and Polars gives of the case, to settle it on a
    /// value.
    fn on_a_value(case: &Case, refused: &Refused) -> Option<Settled> {
        settle_on_a_value(case, refused, &predict(case), &observe(case))
    }

    #[test]
    fn a_call_after_a_part_that_polars_folds_is_refused_on_a_value_of_its_dtype() {
        // `a.min() <= 1.len().over(order_by=[true])` then `.total_days()`: Polars
        // does not evaluate the call as written, and the probe as written is
        // allowed. On a `Boolean` column, which is what reaches the call, it is
        // refused for the tables' reason.
        let name = "stricter-a-window-over-the-length-of-a-literal.case";
        let case = kept(name);
        let refused = refusal(&case);
        assert_eq!(settle(&case, &refused), None);
        let found =
            on_a_value(&case, &refused).expect("a probe that Polars refuses as the tables do");
        assert!(
            found.probe.contains("\"columns\":[\"p\"]"),
            "{}",
            found.probe
        );
        assert!(!found.probe.contains("Expr.min"), "{}", found.probe);
        assert!(found.reason.contains("Duration"), "{}", found.reason);
    }

    #[test]
    fn a_running_sum_of_a_comparison_with_a_null_is_refused_on_a_value_of_its_dtype() {
        let case = kept("stricter-a-running-sum-of-a-null-comparison.case");
        let refused = refusal(&case);
        assert_eq!(settle(&case, &refused), None);
        let found =
            on_a_value(&case, &refused).expect("a probe that Polars refuses as the tables do");
        assert!(found.reason.contains("struct"), "{}", found.reason);
    }

    #[test]
    fn a_value_of_another_dtype_than_the_tables_predict_settles_nothing() {
        // The probe takes the dtype that reaches the call from Polars and the
        // tables together: where they differ it asks nothing, and the case is a
        // disagreement about the dtype, which the target finds first.
        let case = kept("stricter-a-window-over-the-length-of-a-literal.case");
        let refused = refusal(&case);
        let mut wrong = predict(&case);
        for traced in &mut wrong.expressions {
            for held in &mut traced.links {
                *held = held.map(|_| Representative::Int64);
            }
        }
        assert_eq!(
            settle_on_a_value(&case, &refused, &wrong, &observe(&case)),
            None
        );
        let mut unknown = predict(&case);
        for traced in &mut unknown.expressions {
            traced.links.iter_mut().for_each(|held| *held = None);
        }
        assert_eq!(
            settle_on_a_value(&case, &refused, &unknown, &observe(&case)),
            None
        );
    }

    #[test]
    fn a_value_that_polars_allows_the_call_for_settles_nothing() {
        // `a.cum_sum()` of whole numbers is allowed, though the tables are said to
        // refuse it, and so is the running sum of a column of whole numbers.
        let case = cum_sum_of("Int64");
        let refused = refused_at_the_link("`cum_sum` operation not supported for dtype `i64`");
        assert_eq!(on_a_value(&case, &refused), None);
    }

    #[test]
    fn a_value_that_polars_refuses_the_call_for_another_reason_settles_nothing() {
        let case = cum_sum_of("Date");
        let wrong = refused_at_the_link("cannot compare string with numeric type (u32)");
        assert_eq!(on_a_value(&case, &wrong), None);
        let right = refused_at_the_link("`cum_sum` operation not supported for dtype `date`");
        assert!(on_a_value(&case, &right).is_some());
    }

    #[test]
    fn a_rule_is_not_asked_of_a_value() {
        let case = cum_sum_of("Date");
        let mut refused = refused_at_the_link("`cum_sum` operation not supported for dtype `date`");
        refused.source = Source::Rule("condition-predicate-is-boolean");
        assert_eq!(on_a_value(&case, &refused), None);
    }

    #[test]
    fn a_window_in_a_key_that_polars_folds_away_is_refused_alone_as_a_key() {
        // The right key holds `ra.over(rk)` in a branch behind `.last().lt(null)`,
        // which Polars folds into a null, so the window is never met as a key. The
        // window alone as a join's key is not elementwise.
        let case = kept("stricter-a-window-in-a-right-key-under-a-comparison-with-a-null.case");
        let refused = refusal(&case);
        // The part of the key that is refused, as written, is a column: allowed.
        assert_eq!(settle(&case, &refused), None);
        let found = settle_as_a_key(&case, &refused).expect("a key that Polars refuses");
        assert!(found.reason.contains("elementwise"), "{}", found.reason);
        assert!(found.probe.contains("LazyFrame.join"), "{}", found.probe);
        assert!(found.probe.contains("Expr.over"), "{}", found.probe);
        assert!(!found.probe.contains("Expr.last"), "{}", found.probe);
    }

    /// A join of `key` with the right key `rk` followed by `links`.
    fn join_with_right_key(links: &[&str]) -> Case {
        let link = links.join("\",\"arguments\":{}},{\"entry\":\"");
        let text = format!(
            "{{\"a\":\"Int64\",\"b\":\"Int64\",\"revision\":1}}\n{{\"version\":1,\"steps\":[{{\"id\":\"s\",\"call\":{{\"entry\":\"LazyFrame.join\",\"arguments\":{{\"other\":{{\"table\":\"other\"}},\"left_on\":{{\"many\":[{{\"expression\":{{\"start\":{{\"entry\":\"polars.col\",\"arguments\":{{\"name\":{{\"columns\":[\"key\"]}}}}}},\"then\":[]}}}}]}},\"right_on\":{{\"many\":[{{\"expression\":{{\"start\":{{\"entry\":\"polars.col\",\"arguments\":{{\"name\":{{\"columns\":[\"rk\"]}}}}}},\"then\":[{{\"entry\":\"{link}\",\"arguments\":{{}}}}]}}}}]}}}}}}}}]}}"
        );
        Case::decode(text.as_bytes()).expect("a case")
    }

    fn refused_in_the_right_key(reason: &str) -> Refused {
        refused_in_the_right_key_at(0, reason)
    }

    fn refused_in_the_right_key_at(link: usize, reason: &str) -> Refused {
        let place = Place::of_step(0, Some(StepId::new("s").expect("an identifier")))
            .then(Segment::Parameter("right_on".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Link(link));
        Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage: None,
            reason: reason.to_owned(),
            argument_unknown: false,
        }
    }

    #[test]
    fn a_running_value_in_an_operand_of_a_key_that_polars_folds_is_asked_on_a_column() {
        // `null >= rk.sum().cum_max()` then `- rk`: Polars folds the comparison with a
        // null into a null, so the running maximum in what it is compared with is
        // never met as a key. The part that holds the refused call is folded the same
        // way, and the running maximum of one value is allowed as a key, so the call
        // is asked on a column.
        let case = kept("stricter-a-running-value-in-a-right-key-after-a-null-literal.case");
        let refused = refusal(&case);
        assert_eq!(settle(&case, &refused), None);
        assert_eq!(settle_as_a_key(&case, &refused), None);
        let found = settle_apart(&case, &refused, &predict(&case), &observe(&case))
            .expect("a key that Polars refuses");
        assert!(found.reason.contains("elementwise"), "{}", found.reason);
        assert!(found.probe.contains("Expr.cum_max"), "{}", found.probe);
        assert!(
            found.probe.contains("\"columns\":[\"p\"]"),
            "{}",
            found.probe
        );
        assert!(!found.probe.contains("polars.lit"), "{}", found.probe);
    }

    #[test]
    fn a_key_that_polars_allows_alone_as_a_key_settles_nothing() {
        // A right key that tests for a null is elementwise and allowed: the
        // refusal the tables are said to give is not shown. A running sum is not
        // elementwise, and is refused as the tables say.
        let reason = "all join key expressions must be elementwise.";
        let elementwise = join_with_right_key(&["Expr.is_null"]);
        assert_eq!(
            settle_as_a_key(&elementwise, &refused_in_the_right_key(reason)),
            None
        );
        let running = join_with_right_key(&["Expr.cum_sum"]);
        assert!(settle_as_a_key(&running, &refused_in_the_right_key(reason)).is_some());
    }

    #[test]
    fn a_call_that_polars_allows_on_a_column_as_a_key_settles_nothing() {
        // A right key that tests for a null is elementwise on a column too, so the
        // refusal the tables are said to give is not shown by it.
        let reason = "all join key expressions must be elementwise.";
        let case = join_with_right_key(&["Expr.is_null"]);
        let refused = refused_in_the_right_key(reason);
        assert_eq!(
            settle_on_a_value_as_a_key(&case, &refused, &predict(&case), &observe(&case)),
            None
        );
        let running = join_with_right_key(&["Expr.cum_sum"]);
        assert!(
            settle_on_a_value_as_a_key(
                &running,
                &refused_in_the_right_key(reason),
                &predict(&running),
                &observe(&running)
            )
            .is_some()
        );
    }

    #[test]
    fn a_key_refused_before_the_refused_call_settles_nothing() {
        // `rk.cum_sum().is_null()`: the running sum is not elementwise, so the key
        // is refused as the tables say, but not at `is_null`: what comes before it is
        // refused alone for the same reason, so the refused call is not why.
        let reason = "all join key expressions must be elementwise.";
        let case = join_with_right_key(&["Expr.cum_sum", "Expr.is_null"]);
        assert_eq!(
            settle_as_a_key(&case, &refused_in_the_right_key_at(1, reason)),
            None
        );
        // At the running sum itself it is the cause.
        assert!(settle_as_a_key(&case, &refused_in_the_right_key_at(0, reason)).is_some());
    }

    /// A join of `key` with the right key `rk > rk.cum_sum().<inner>`: the second
    /// chain is an argument of the comparison.
    fn join_with_a_chain_in_the_right_key(inner: &[&str]) -> Case {
        let column = "{\"entry\":\"polars.col\",\"arguments\":{\"name\":{\"columns\":[\"rk\"]}}}";
        let links: Vec<String> = std::iter::once("Expr.cum_sum")
            .chain(inner.iter().copied())
            .map(|id| format!("{{\"entry\":\"{id}\",\"arguments\":{{}}}}"))
            .collect();
        let key = format!(
            "{{\"expression\":{{\"start\":{column},\"then\":[{{\"entry\":\"Expr.gt\",\"arguments\":{{\"other\":{{\"expression\":{{\"start\":{column},\"then\":[{}]}}}}}}}}]}}}}",
            links.join(",")
        );
        let text = format!(
            "{{\"a\":\"Int64\",\"b\":\"Int64\",\"revision\":1}}\n{{\"version\":1,\"steps\":[{{\"id\":\"s\",\"call\":{{\"entry\":\"LazyFrame.join\",\"arguments\":{{\"other\":{{\"table\":\"other\"}},\"left_on\":{{\"many\":[{{\"expression\":{{\"start\":{{\"entry\":\"polars.col\",\"arguments\":{{\"name\":{{\"columns\":[\"key\"]}}}}}},\"then\":[]}}}}]}},\"right_on\":{{\"many\":[{key}]}}}}}}}}]}}"
        );
        Case::decode(text.as_bytes()).expect("a case")
    }

    /// The tables refuse link `link` of the chain that the comparison of the
    /// right key is given.
    fn refused_in_the_argument_of_the_right_key(link: usize, reason: &str) -> Refused {
        let place = Place::of_step(0, Some(StepId::new("s").expect("an identifier")))
            .then(Segment::Parameter("right_on".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Link(0))
            .then(Segment::Parameter("other".to_owned()))
            .then(Segment::Link(link));
        Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage: None,
            reason: reason.to_owned(),
            argument_unknown: false,
        }
    }

    #[test]
    fn a_key_refused_before_the_refused_call_in_an_argument_settles_nothing() {
        // `rk > rk.cum_sum().cum_max()`: the running sum is not elementwise, so the
        // key is refused as the tables say, but the refused call is the running
        // maximum, which follows it in the argument. The part of the argument before
        // the refused call, `rk > rk.cum_sum()`, is refused alone for the same
        // reason, so the refused call is not why, though the key before the
        // comparison, `rk`, is allowed.
        let reason = "all join key expressions must be elementwise.";
        let case = join_with_a_chain_in_the_right_key(&["Expr.cum_max"]);
        assert_eq!(
            settle_as_a_key(&case, &refused_in_the_argument_of_the_right_key(1, reason)),
            None
        );
        // At the running sum itself, nothing in the argument comes before it that is
        // refused, and it is the cause.
        assert!(
            settle_as_a_key(&case, &refused_in_the_argument_of_the_right_key(0, reason)).is_some()
        );
    }

    #[test]
    fn a_reason_that_is_not_about_a_key_is_not_asked_of_a_key() {
        let case = kept("stricter-a-window-in-a-right-key-under-a-comparison-with-a-null.case");
        let mut refused = refusal(&case);
        refused.reason = "cannot compare string with numeric type (i8)".to_owned();
        assert_eq!(settle_as_a_key(&case, &refused), None);
    }

    #[test]
    fn what_the_target_says_of_a_schema_is_no_refusal_of_polars() {
        // `a > a.cum_sum()` of `UInt128`: Polars evaluates the running sum and
        // allows the step. The tables' reason, that the schema says `Int64` and the
        // rows give `UInt128`, is the target's own comparison of a column's plan with
        // its rows, which only shows where the running sum is the output, so the
        // probe that selects it alone matches it though Polars refused nothing.
        let case = kept("uncovered-a-hidden-schema.case");
        let refused = refusal(&case);
        assert!(
            refused.reason.starts_with("the schema says"),
            "{}",
            refused.reason
        );
        assert_eq!(settle(&case, &refused), None);
        assert_eq!(
            settle_apart(&case, &refused, &predict(&case), &observe(&case)),
            None
        );
        let said = "the schema says Int64 and the rows give UInt128";
        assert!(!same_reason(&Source::Polars, said, said));
    }

    #[test]
    fn a_sort_key_of_one_value_that_polars_drops_is_asked_where_it_is_evaluated() {
        // `sort(by=a.last().cum_sum())` of `UInt128`: the tables refuse the running
        // sum because its plan says `Int64` and its rows give `UInt128`, the target's
        // own comparison, which no probe of the call alone can show. Polars drops a
        // sort key that is one value, so it never meets the mismatch; as the key
        // on a column it panics, and that is where the call is asked.
        let case = kept("stricter-a-sort-key-of-one-value-whose-schema-differs.case");
        let refused = refusal(&case);
        assert!(
            refused.reason.starts_with("the schema says"),
            "{}",
            refused.reason
        );
        assert_eq!(settle(&case, &refused), None);
        let found = settle_apart(&case, &refused, &predict(&case), &observe(&case))
            .expect("a key that Polars refuses");
        assert!(found.probe.contains("LazyFrame.sort"), "{}", found.probe);
        assert!(
            found.probe.contains("\"columns\":[\"p\"]"),
            "{}",
            found.probe
        );
        assert!(found.reason.contains("panicked"), "{}", found.reason);
        assert_eq!(found.showing, Showing::TheSchemaMismatch);
    }

    #[test]
    fn a_comparison_given_no_known_dtype_is_settled_by_polars_refusing_it() {
        // `sort(by=when(b <= b - 1).then(b).last())` of `List(Int64)`: the tables
        // do not know what `b - 1` gives, and refuse the comparison for every
        // dtype it might give, with the reason Polars gave for one of them. Polars
        // drops a sort key that is one value; asked as written, it refuses the
        // comparison, for the dtype the argument really has.
        let case =
            kept("stricter-a-comparison-given-no-known-dtype-in-a-sort-key-of-one-value.case");
        let refused = refusal(&case);
        assert_eq!(settle(&case, &refused), None);
        let found = settle_apart(&case, &refused, &predict(&case), &observe(&case))
            .expect("a comparison that Polars refuses");
        assert!(found.probe.contains("LazyFrame.select"), "{}", found.probe);
        assert!(found.reason.contains("comparison"), "{}", found.reason);
        assert_eq!(found.showing, Showing::ForAnyDtypeOfTheArgument);
    }

    #[test]
    fn an_arithmetic_given_no_known_dtype_is_settled_by_polars_refusing_it() {
        // `sort(by=g.over(order_by=[g - h.over(order_by=[k], nulls_last=true)]).sum())`:
        // the tables do not know the dtype of the window that is subtracted from a
        // text column, and refuse the subtraction for every dtype it might give.
        let case =
            kept("stricter-an-arithmetic-given-no-known-dtype-in-a-sort-key-of-one-value.case");
        let refused = refusal(&case);
        assert_eq!(settle(&case, &refused), None);
        let found = settle_apart(&case, &refused, &predict(&case), &observe(&case))
            .expect("a subtraction that Polars refuses");
        assert!(found.probe.contains("LazyFrame.select"), "{}", found.probe);
        assert!(found.reason.contains("sub operation"), "{}", found.reason);
        assert_eq!(found.showing, Showing::ForAnyDtypeOfTheArgument);
    }

    #[test]
    fn a_call_refused_with_every_dtype_known_is_asked_for_its_reason() {
        // The probe of a call given no known dtype does not compare reasons, so it is
        // asked only of a refusal the walk says was made that way.
        let case =
            kept("stricter-a-comparison-given-no-known-dtype-in-a-sort-key-of-one-value.case");
        let refused = Refused {
            argument_unknown: false,
            ..refusal(&case)
        };
        assert_eq!(settle_given_no_known_dtype(&case, &refused), None);
    }

    #[test]
    fn a_call_given_no_known_dtype_that_polars_allows_alone_settles_nothing() {
        // The running sum of whole numbers is allowed, whatever the tables say of it.
        let case = in_a_position("LazyFrame.select", "exprs", "Int64", &["Expr.cum_sum"]);
        let refused = Refused {
            argument_unknown: true,
            ..refused_at_the_link("cannot perform `cum_sum`")
        };
        assert_eq!(settle_given_no_known_dtype(&case, &refused), None);
    }

    #[test]
    fn a_call_given_no_known_dtype_after_a_call_polars_refuses_settles_nothing() {
        // The sum of text is refused by Polars at the first call, so its refusal is
        // not the second call's.
        let case = in_a_position(
            "LazyFrame.select",
            "exprs",
            "String",
            &["Expr.sum", "Expr.cum_sum"],
        );
        let refused = Refused {
            argument_unknown: true,
            ..refused_at_link(1, "cannot perform `cum_sum`")
        };
        assert_eq!(settle_given_no_known_dtype(&case, &refused), None);
    }

    #[test]
    fn a_refusal_that_is_the_targets_own_is_not_settled_for_a_call_given_no_known_dtype() {
        let case =
            kept("stricter-a-comparison-given-no-known-dtype-in-a-sort-key-of-one-value.case");
        let refused = Refused {
            argument_unknown: true,
            reason: "the schema says Int64 and the rows give UInt128".to_owned(),
            ..refusal(&case)
        };
        assert_eq!(settle_given_no_known_dtype(&case, &refused), None);
    }

    #[test]
    fn a_call_that_polars_allows_as_a_sort_key_on_a_column_settles_nothing() {
        // The running sum of whole numbers is allowed as a sort key, so a mismatch
        // that the tables are said to find in it is not shown there.
        let text = "{\"a\":\"Int64\",\"b\":\"Int64\",\"revision\":1}\n{\"version\":1,\"steps\":[{\"id\":\"s\",\"call\":{\"entry\":\"LazyFrame.sort\",\"arguments\":{\"by\":{\"many\":[{\"expression\":{\"start\":{\"entry\":\"polars.col\",\"arguments\":{\"name\":{\"columns\":[\"a\"]}}},\"then\":[{\"entry\":\"Expr.cum_sum\",\"arguments\":{}}]}}]}}}}]}";
        let case = Case::decode(text.as_bytes()).expect("a case");
        let place = Place::of_step(0, Some(StepId::new("s").expect("an identifier")))
            .then(Segment::Parameter("by".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Link(0));
        let refused = Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage: None,
            reason: "the schema says Int64 and the rows give UInt128".to_owned(),
            argument_unknown: false,
        };
        assert_eq!(
            settle_in_the_position(&case, &refused, &predict(&case), &observe(&case)),
            None
        );
    }

    /// A case that selects the chain `a` then `links` (no arguments) in the
    /// position `entry` and `parameter` name, on columns of `receiver`.
    fn in_a_position(entry: &str, parameter: &str, receiver: &str, links: &[&str]) -> Case {
        let links: Vec<String> = links
            .iter()
            .map(|id| format!("{{\"entry\":\"{id}\",\"arguments\":{{}}}}"))
            .collect();
        let text = format!(
            "{{\"a\":\"{receiver}\",\"b\":\"Int64\",\"revision\":1}}\n{{\"version\":1,\"steps\":[{{\"id\":\"s\",\"call\":{{\"entry\":\"{entry}\",\"arguments\":{{\"{parameter}\":{{\"many\":[{{\"expression\":{{\"start\":{{\"entry\":\"polars.col\",\"arguments\":{{\"name\":{{\"columns\":[\"a\"]}}}}}},\"then\":[{}]}}}}]}}}}}}}}]}}",
            links.join(",")
        );
        Case::decode(text.as_bytes()).expect("a case")
    }

    fn refused_by_the_schema(parameter: &str, link: usize) -> Refused {
        let place = Place::of_step(0, Some(StepId::new("s").expect("an identifier")))
            .then(Segment::Parameter(parameter.to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Link(link));
        Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage: None,
            reason: "the schema says Int64 and the rows give UInt128".to_owned(),
            argument_unknown: false,
        }
    }

    #[test]
    fn a_refusal_in_the_position_that_is_not_about_the_mismatch_settles_nothing() {
        // A filter's predicate must be a `Boolean`, so the running sum of an
        // unsigned 128-bit column is refused as one, as is the running sum of whole
        // numbers: Polars refuses the position whatever the plan and the rows of the
        // call are, and that does not show the mismatch the tables name.
        let case = in_a_position(
            "LazyFrame.filter",
            "predicates",
            "UInt128",
            &["Expr.cum_sum"],
        );
        let refused = refused_by_the_schema("predicates", 0);
        assert_eq!(
            settle_in_the_position(&case, &refused, &predict(&case), &observe(&case)),
            None
        );
    }

    #[test]
    fn a_refusal_is_tied_to_the_mismatch_only_with_every_piece_of_evidence() {
        // A panic or a refusal in the position that has no part in the mismatch is
        // met by the control too, or by the column alone, or the call does not differ
        // as the tables say: each missing piece settles nothing.
        assert!(ties_the_refusal_to_the_mismatch(true, true, true, true));
        assert!(!ties_the_refusal_to_the_mismatch(false, true, true, true));
        assert!(!ties_the_refusal_to_the_mismatch(true, false, true, true));
        assert!(!ties_the_refusal_to_the_mismatch(true, true, false, true));
        assert!(!ties_the_refusal_to_the_mismatch(true, true, true, false));
    }

    #[test]
    fn the_catalogue_has_what_a_probe_is_made_of() {
        for id in ["LazyFrame.select", "LazyFrame.pivot", "LazyFrame.filter"] {
            assert!(catalog::entry(id).is_some(), "{id}");
        }
    }
}
