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
//!   column of the dtype that Polars gave what reaches it, which Polars cannot fold;
//!   and again with each literal it is given a column of its dtype too, for a
//!   literal hides what Polars skips as it hides what it evaluates.
//!   And once more with the receiver and every argument a column of the dtype that reaches it,
//!   so that nothing an argument holds is refused before the call is. In a pivot's aggregate the
//!   call stands on `element`, which holds the values of the column `a`, where that is the dtype
//!   the tables say reaches the call.
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
//! A refusal by a rule that a predicate be a `Boolean` is asked the same way
//! ([`settle_a_predicate_on_a_value`]): the predicate as written may be refused first for a
//! call it holds, or not evaluated at all, so a column of the dtype that reaches its end stands
//! as the predicate, and Polars refusing it as the rule says shows the rule right about it.
//!
//! A refusal in a predicate of a condition whose values are one expression, which Polars drops
//! and never evaluates, is asked of the case itself ([`settle_by_values_that_differ`]): the last
//! value of each condition is made a fill forward of what it was, so that the values differ, and
//! Polars refusing that case for the tables' reason, or, for a call the tables judge by its parts,
//! for any reason where the same case with the predicate that holds the refused call made `false`
//! is allowed, shows the predicate is one it skips ([`Showing::WhereTheValuesDiffer`]). A probe
//! of the part alone is no better, for an argument of it may be refused first.
//!
//! A predicate that the rule that it be a `Boolean` refused where no column of its dtype can be
//! asked (Polars refuses it on its own for another call it holds, or it is in a pivot's aggregate)
//! and Polars never evaluates, because a call after its condition makes it one value that Polars
//! drops or folds, is asked in [`settle_by_evaluating_the_condition`]: the condition alone, in the
//! place of the expression that holds it, with what follows it removed and its last value made to
//! differ. Polars refusing that in the predicate, and allowing it with that predicate made `false`,
//! shows that it is the predicate Polars refuses once nothing hides it
//! ([`Showing::WhereTheConditionIsEvaluated`]).
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
use super::frames::{STAND_IN, key_column, stand_in_column};
use super::limits::{RUNS, case_changed, changed, judges_its_values_apart};
use super::literal::{call_with_columns, class_of, operand_causes, placed_operands};
use super::observe::{
    Observed, Refusal, SCHEMA_SAYS, Seen, Tables, is_bare_key, is_the_targets_own, observe,
    observe_probe,
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
    /// The tables refused a part of a predicate of a condition that Polars allows, because the
    /// values of the condition are one expression and Polars drops its predicates. The case with
    /// the values made to differ is refused by Polars for the tables' reason, or, for a call the
    /// tables judge by its parts, for the part it meets first where the case with the predicate
    /// that holds the call made `false` is allowed, so the predicate is evaluated there, and it is
    /// the dropping that Polars allows the case for.
    WhereTheValuesDiffer,
    /// The rule that a condition's predicate be a `Boolean` refused a predicate that Polars allows
    /// because it never evaluates it: a call that follows the condition makes it one value that
    /// Polars drops or folds. The condition standing alone, with what follows it removed and its
    /// values made to differ, is refused by Polars, and the same condition with the predicate
    /// made `false` is allowed, so it is the predicate that Polars refuses once it evaluates it.
    WhereTheConditionIsEvaluated,
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
            Showing::WhereTheValuesDiffer => format!(
                "Polars drops the predicate, for the values of the condition are one expression, and refuses the case {} where they differ, for the tables' reason or, for a call judged by its parts, for the part it meets first: {}",
                self.probe, self.reason
            ),
            Showing::WhereTheConditionIsEvaluated => format!(
                "Polars never evaluates the predicate where what follows its condition makes it one value, and refuses the condition alone {}, {}, which is allowed with the predicate made false: {}",
                self.probe,
                if is_refused_as_a_predicate_that_is_no_boolean(&self.reason) {
                    "for the rule's reason"
                } else {
                    "for another reason than the rule's, in the predicate"
                },
                self.reason
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
    /// The path of the place.
    path: &'a [Segment],
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
        path,
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

/// How Polars on one thread, which the tables are measured on, refuses a bitwise call between a
/// whole number and text, and others that read a series as a dtype it is not.
const UNPACKING: &str = "cannot unpack series of type";

/// How the streaming engine of Polars on many threads fails where the tables' reason is a
/// failure to unpack a list: it casts the list to the dtype of the other side.
const CASTING_A_LIST: &str = "cannot cast List type";

/// The same where the tables' reason is a failure to unpack an array.
const CASTING_AN_ARRAY: &str = "cannot cast Array type";

/// Whether `probe` is how Polars on many threads refuses what the tables' reason, a failure to
/// unpack a series, refuses. Its streaming engine does not unpack the text as a whole number:
/// it gives the column of the whole number where its schema says text, which is the same call
/// refused, for it is not what the schema says. And it fails to cast a list, or an array, to the
/// dtype of the other side where Polars on one thread fails to unpack it as that dtype.
fn is_the_same_unpacking(tables: &str, probe: &str) -> bool {
    tables.starts_with(UNPACKING)
        && (probe.starts_with(SCHEMA_SAYS)
            || (tables.contains("`list[") && probe.starts_with(CASTING_A_LIST))
            || (tables.contains("`array[") && probe.starts_with(CASTING_AN_ARRAY)))
}

/// Whether both reasons are the refusal of one comparison between two series that Polars cannot
/// compare, in the words of its two engines: one says it could not evaluate the comparison, the
/// other that it cannot perform it, and each names the comparison's symbol.
pub fn is_the_same_comparison(tables: &str, probe: &str) -> bool {
    let symbol = |reason: &str| reason.split('\'').nth(1).map(str::to_owned);
    let comparing = |reason: &str| {
        (reason.starts_with("could not evaluate '") || reason.starts_with("cannot perform '"))
            && reason.contains("' comparison between series")
    };
    comparing(tables) && comparing(probe) && symbol(tables) == symbol(probe)
}

/// Whether `probe` is refused as the tables refused the part: for Polars' own
/// reason the same objection, and for a rule that a predicate be a `Boolean`
/// Polars' words for it.
pub fn same_reason(source: &Source, tables: &str, probe: &str) -> bool {
    match source {
        // What the target says of a schema is no objection of Polars, so it is
        // neither the tables' reason nor one that a probe can be refused for.
        Source::Polars => {
            (!is_the_targets_own(tables)
                && !is_the_targets_own(probe)
                && !objection(tables).is_empty()
                && objection(tables) == objection(probe))
                || is_the_same_unpacking(tables, probe)
                || (is_a_count_that_may_not_end(tables) && is_a_refusal_of_lengths(probe))
        }
        Source::Rule(rule) if PREDICATE_RULES.contains(rule) => {
            let probe = probe.to_ascii_lowercase();
            probe.contains("boolean") && (probe.contains("expected") || probe.contains("must be"))
        }
        Source::Rule(rule) if *rule == COUNT_RULE => probe.contains("must have the same length as"),
        // A count that values meet, or a window is given, is refused as lengths that do not match.
        Source::Rule(rule) if COUNT_MEETING_RULES.contains(rule) => is_a_refusal_of_lengths(probe),
        // Polars finds no dtype that two zones have in common, and says it of a comparison as it
        // does of a difference or a fill.
        Source::Rule(rule) if *rule == ZONE_RULE => {
            ZONE_WORDS.iter().any(|said| probe.contains(said))
        }
        _ => false,
    }
}

/// The rules about a count of values that the data or the arguments decide, which Polars meets
/// where a call is given values of other lengths than it takes.
const COUNT_MEETING_RULES: [&str; 2] = ["values-meet-in-count", "window-input-is-not-counted"];

/// How the tables word the reason of a chain whose count of values may not end an expression
/// where the context takes only one value or the table's rows.
const A_COUNT_THAT_MAY_NOT_END: &str = "a count of values that the data or the arguments decide";

/// Whether the tables' reason is that a count of values may not end an expression, where Polars
/// refused it for the lengths that did not match.
fn is_a_count_that_may_not_end(tables: &str) -> bool {
    tables.contains(A_COUNT_THAT_MAY_NOT_END) && tables.contains("may not end an expression")
}

/// Whether Polars refused for values of lengths that do not meet, in the words of its two engines.
fn is_a_refusal_of_lengths(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    [
        "lengths don't match",
        "non-equal length",
        "different lengths",
    ]
    .iter()
    .any(|words| reason.contains(words))
}

/// The tables a probe of the part runs on.
fn tables_of(found: &Found, stand_in: Option<Representative>) -> Tables {
    Tables::standing(found.on_the_other_table, stand_in.as_slice()).unwrap_or_default()
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

/// The probe of a predicate that a rule refused: `predicate` as the predicate of a condition, or as
/// a filter's, as the rule is about.
fn predicate_probe(found: &Found, rule: &str, predicate: Expression) -> Option<Probe> {
    if rule == "condition-predicate-is-boolean" {
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
    } else {
        Some(Probe {
            part: predicate,
            filter: true,
            before: None,
            at: vec![place_of_the_part(found, true)],
        })
    }
}

/// The probe of the chain that takes the refused place as an argument: the call that is given the
/// count is the one that meets it beside its other operands, which the place alone does not hold.
fn enclosing_probe(found: &Found) -> Option<Probe> {
    if !found.rest.is_empty() || found.levels.len() < 2 {
        return None;
    }
    let (outer, consumed) = found.levels[found.levels.len() - 2];
    let at = found.path.get(consumed)?;
    let held = place_of_the_part(found, false);
    // A predicate of a condition is met by the condition, which its values complete: a condition
    // of literal values with the predicate stands where the chain of the condition's start would.
    if matches!(at, Segment::Start)
        && matches!(
            found.path.get(consumed + 1),
            Some(Segment::Parameter(name)) if name == "predicates"
        )
    {
        let started = held.then(Segment::Start);
        let inside = started
            .clone()
            .then(Segment::Parameter("predicates".to_owned()))
            .then(Segment::Item(0));
        return Some(Probe {
            part: condition_of(found.expression.clone())?,
            filter: false,
            before: None,
            at: vec![started, inside],
        });
    }
    Some(Probe {
        part: chain_up_to(outer, Some(at))?,
        filter: false,
        before: None,
        at: vec![held.then(at.clone())],
    })
}

/// The probe of the part the tables refused, by who refused it.
fn probe_of_the_part(found: &Found, source: &Source) -> Option<Probe> {
    match source {
        Source::Polars => chain_probe(found),
        Source::Rule(rule)
            if *rule == COUNT_RULE || *rule == ZONE_RULE || COUNT_MEETING_RULES.contains(rule) =>
        {
            chain_probe(found)
        }
        Source::Rule(rule) if PREDICATE_RULES.contains(rule) => {
            predicate_probe(found, rule, found_predicate(found)?)
        }
        _ => None,
    }
}

/// The rule that moments compared, subtracted, filled or bound by one another be in one time zone,
/// whose refusal Polars shows by the dtype that two zones have in common, which is none.
const ZONE_RULE: &str = "moments-share-a-zone";

/// What Polars says of two zones that have no dtype in common.
const ZONE_WORDS: [&str; 3] = [
    "could not evaluate",
    "failed to determine supertype",
    "is incompatible with expected type",
];

/// The rule that a replacement take as many values as it is given the number of, whose refusal
/// Polars shows by the lengths it refuses the call for.
const COUNT_RULE: &str = "replacement-values-match-in-number";

/// The probe of the chain through the refused call, selected alone.
fn chain_probe(found: &Found) -> Option<Probe> {
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
    // A call that the tables judge by its values apart, as a replacement and an interval, is
    // refused for the first value they refuse, and Polars names whichever it meets, so a refusal
    // of the call itself is the tables' right whatever words it has, where it is refused at the
    // call and not before it.
    let by_parts = refused.place.path().is_empty()
        || at
            .iter()
            .any(|place| judges_its_values_apart(probed, place));
    (0..ATTEMPTS).find_map(|_| {
        // `None` ends the search with the probe allowed, which is no refusal.
        let refusal = observe_probe(probed, tables).refusal;
        match refusal {
            Some(refusal)
                if (same_reason(&refused.source, &refused.reason, &refusal.reason)
                    && lies_at(&refusal, at))
                    || (by_parts
                        && refusal.place.as_ref().is_none_or(|place| {
                            place.path().is_empty()
                                || at
                                    .iter()
                                    .any(|probed| place.path().starts_with(probed.path()))
                        })) =>
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
    let mut probes = vec![probe_of_the_part(&found, &refused.source)];
    // A count that may not end an expression where a call is given it is met by the call that
    // takes it, beside its other operands, which a probe of the count alone does not hold.
    if matches!(refused.source, Source::Polars) && is_a_count_that_may_not_end(&refused.reason) {
        probes.push(enclosing_probe(&found));
    }
    let run = |part: Expression, filter: bool| {
        let document = document_of(case, &found, part, filter)?;
        Some(Case {
            receiver: case.receiver,
            argument: case.argument,
            document,
        })
    };
    probes.into_iter().flatten().find_map(|probe| {
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
    dtype_given(held, link, predicted, observed, false)
}

/// As [`dtype_reaching`], where the tables made no claim of the dtype too: Polars gives a condition
/// with no `otherwise` the dtype of its values, and the tables say nothing of one that is not
/// finished, so there is no dtype of theirs for it to differ from.
fn dtype_polars_gave(
    held: &Place,
    link: usize,
    predicted: &Prediction,
    observed: &Observed,
) -> Option<Representative> {
    dtype_given(held, link, predicted, observed, true)
}

fn dtype_given(
    held: &Place,
    link: usize,
    predicted: &Prediction,
    observed: &Observed,
    unsaid_is_free: bool,
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
        Seen::Dtype(_, _, Some(class))
            if Some(*class) == said || (unsaid_is_free && said.is_none()) =>
        {
            Some(*class)
        }
        _ => None,
    }
}

/// The class of the dtype that the tables say reaches the call at `link` of the expression at
/// `held`, where Polars was not asked.
fn predicted_dtype_reaching(
    held: &Place,
    link: usize,
    predicted: &Prediction,
) -> Option<Representative> {
    let traced = predicted
        .expressions
        .iter()
        .find(|traced| traced.place == *held)?;
    match link.checked_sub(1) {
        None => traced.start,
        Some(before) => *traced.links.get(before)?,
    }
}

/// Whether the case is a pivot that takes the values of the column `a`, which the target's pivots
/// all do: the `element` of its aggregate holds them.
fn takes_the_values_of_a(case: &Case) -> bool {
    matches!(
        case.document.steps()[0].call().argument("values"),
        Some(Value::Columns(names)) if names.as_slice() == ["a"]
    )
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
/// asked first, and the limit for a literal is asked only of the literals that
/// are why. A literal the call is given that is not, because Polars allows the
/// call with a column of its dtype as well, is a column in the probe too: it is
/// asked as written first, and then so. And with each argument whose dtype Polars did not give,
/// which no column can stand for, left at its default. `None` when there is nothing to ask,
/// Polars allows the probe, or it refuses it for another reason or not at the call.
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
    let consumed = refused.place.path().len() - found.rest.len();
    let held = refused.place.path()[..consumed]
        .iter()
        .cloned()
        .fold(Place::of_step(0, StepId::new(STEP).ok()), Place::then);
    // Polars is not asked of the dtype that reaches a call in a pivot's aggregate, for what it says
    // of a prefix is said in a select, where `element` stands nowhere: the tables' is taken.
    let class = if found.in_the_aggregate {
        predicted_dtype_reaching(&held, *link, predicted)?
    } else {
        dtype_reaching(&held, *link, predicted, observed)?
    };
    // A pivot's aggregate reads no column to stand in, but its `element` holds the values of the
    // column the pivot takes the values of, `a`, which is of the dtype that reaches the call where
    // that is the dtype of `a`.
    let name = if found.in_the_aggregate {
        if class != case.receiver || !takes_the_values_of_a(case) {
            return None;
        }
        call("polars.element", Vec::new())?
    } else {
        call(
            "polars.col",
            vec![("name", Value::Columns(vec![STAND_IN.to_owned()]))],
        )?
    };
    let refused_call = found.expression.links().get(*link)?;
    // The call as written, and then with each literal it is given a column of its
    // dtype, which Polars cannot fold or broadcast as it can a literal.
    let literals = if found.in_the_aggregate {
        // No column stands in for a literal there either.
        Vec::new()
    } else {
        operand_causes(refused_call, &refused.place, false)
    };
    let literal_classes = literals
        .iter()
        .map(|cause| class_of(cause, observed))
        .collect::<Option<Vec<_>>>();
    let mut attempts = vec![(refused_call.clone(), vec![class])];
    if let Some(literal_classes) = literal_classes.filter(|classes| !classes.is_empty()) {
        let standing: Vec<&Expression> = literals.iter().map(|cause| cause.expression).collect();
        if let Some(columns) = call_with_columns(refused_call, &standing, 1) {
            attempts.push((columns, [vec![class], literal_classes].concat()));
        }
    }
    // And with every argument a column of the dtype that reaches its end, so that nothing an
    // argument holds, as a call the tables allow without knowing a dtype, is refused before the
    // call is.
    if !found.in_the_aggregate
        && let Some((columns, argument_classes)) =
            call_on_columns(refused_call, &refused.place, predicted, observed)
    {
        attempts.push((columns, [vec![class], argument_classes].concat()));
    }
    // And with every argument whose dtype is not known, which no column can stand for, left out so
    // that the call takes its default: where the call has one, nothing in the argument is refused
    // before the call is.
    if !found.in_the_aggregate
        && let Some(without) =
            call_without_unknowns(refused_call, &refused.place, predicted, observed)
    {
        attempts.push((without, vec![class]));
    }
    attempts.into_iter().find_map(|(link_call, classes)| {
        let part = Expression::new(Start::Call(name.clone()), vec![link_call]).ok()?;
        let probed = Case {
            receiver: case.receiver,
            argument: case.argument,
            document: document_of(case, &found, part, false)?,
        };
        let at = place_of_the_part(&found, false).then(Segment::Link(0));
        let classes = if found.in_the_aggregate {
            Vec::new()
        } else {
            classes
        };
        let tables = Tables::standing(found.on_the_other_table, &classes)?;
        let refusal = refused_as(&probed, tables, refused, &[at])?;
        Some(Settled {
            probe: probed.document.write(),
            reason: refusal.reason,
            showing: Showing::TheSameReason,
        })
    })
}

/// Asks Polars about a function that starts a chain, which the tables refused for what it is
/// given, with a column of the dtype of each argument that Polars cannot fold, skip or leave null as
/// it can the argument: first with each literal it is given a column of its dtype, which Polars
/// folds when the call is asked alone, and then with every argument a column of the dtype that
/// reaches it, which a condition that no row takes and a part that a length hides are not. The part
/// as written may be allowed because of what an argument is, and a column of the dtype is refused as
/// the tables say. `None` for a refusal of a rule or of a link, for a pivot's aggregate, which reads
/// no column to stand in, when no argument has a dtype Polars gave, and when Polars allows the
/// probe, refuses it for another reason or not at the call.
pub fn settle_a_start_given_columns(
    case: &Case,
    refused: &Refused,
    predicted: &Prediction,
    observed: &Observed,
) -> Option<Settled> {
    if !matches!(refused.source, Source::Polars) {
        return None;
    }
    let found = find(case, &refused.place)?;
    if found.in_the_aggregate || !matches!(found.rest, [Segment::Start]) {
        return None;
    }
    let Start::Call(start) = found.expression.start() else {
        return None;
    };
    let literals = operand_causes(start, &refused.place, false);
    let literal_classes = literals
        .iter()
        .map(|cause| class_of(cause, observed))
        .collect::<Option<Vec<_>>>()
        .filter(|classes| !classes.is_empty());
    let mut attempts: Vec<(Call, Vec<Representative>)> = Vec::new();
    if let Some(classes) = literal_classes {
        let standing: Vec<&Expression> = literals.iter().map(|cause| cause.expression).collect();
        if let Some(columns) = call_with_columns(start, &standing, 0) {
            attempts.push((columns, classes));
        }
    }
    // The columns of the arguments are the second and after, as a call that follows a value has
    // that value for the first.
    if let Some((columns, classes)) = call_on_columns(start, &refused.place, predicted, observed) {
        attempts.push((columns, [vec![Representative::Int64], classes].concat()));
    }
    attempts.into_iter().find_map(|(columns, classes)| {
        let part = Expression::new(Start::Call(columns), Vec::new()).ok()?;
        let probed = Case {
            receiver: case.receiver,
            argument: case.argument,
            document: document_of(case, &found, part, false)?,
        };
        let at = place_of_the_part(&found, false).then(Segment::Start);
        let tables = Tables::standing(found.on_the_other_table, &classes)?;
        let refusal = refused_as(&probed, tables, refused, &[at])?;
        Some(Settled {
            probe: probed.document.write(),
            reason: refusal.reason,
            showing: Showing::TheSameReason,
        })
    })
}

/// Asks Polars about a predicate that a rule refused, apart from what the predicate holds: a
/// column of the dtype that reaches its end, as the predicate of a condition or of a filter. The
/// predicate as written may be refused first for a call inside it that the tables allow, as a
/// comparison of text with a number they were not told the dtype of, and then Polars refuses it
/// for another reason than the rule; and it may not be evaluated at all, as under the length of a
/// condition. A column that has the dtype is not folded, and Polars refuses it as the rule says
/// where it evaluates it, which shows the rule right about that dtype. The dtype is Polars' as
/// well as the tables': a wrong prediction of it is a disagreement of the dtypes, found before
/// anything is probed. `None` for a refusal of Polars, for a predicate in a pivot's aggregate,
/// which reads no column, when Polars gave the predicate no dtype, and when Polars allows the
/// column or refuses it for another reason.
pub fn settle_a_predicate_on_a_value(
    case: &Case,
    refused: &Refused,
    predicted: &Prediction,
    observed: &Observed,
) -> Option<Settled> {
    let Source::Rule(rule) = refused.source else {
        return None;
    };
    if !PREDICATE_RULES.contains(&rule) {
        return None;
    }
    let found = find(case, &refused.place)?;
    if !found.rest.is_empty() || found.in_the_aggregate {
        return None;
    }
    let held = refused.place.clone();
    let class = dtype_reaching(&held, found.expression.links().len(), predicted, observed)?;
    let name = call(
        "polars.col",
        vec![("name", Value::Columns(vec![STAND_IN.to_owned()]))],
    )?;
    let column = Expression::new(Start::Call(name), Vec::new()).ok()?;
    let probe = predicate_probe(&found, rule, column)?;
    let probed = Case {
        receiver: case.receiver,
        argument: case.argument,
        document: document_of(case, &found, probe.part, probe.filter)?,
    };
    let tables = Tables::standing(found.on_the_other_table, &[class])?;
    let refusal = refused_as(&probed, tables, refused, &probe.at)?;
    Some(Settled {
        probe: probed.document.write(),
        reason: refusal.reason,
        showing: Showing::TheSameReason,
    })
}

/// `call`, which is at `at`, with every expression it is given a column of the class that reaches
/// its end, as Polars and the tables both gave it, and those classes in the order of the columns,
/// which are [`stand_in_column`]s from the second: the first stands for what the call follows.
/// `None` for a call given nothing, or an argument whose dtype is not known to both.
fn call_on_columns(
    call: &Call,
    at: &Place,
    predicted: &Prediction,
    observed: &Observed,
) -> Option<(Call, Vec<Representative>)> {
    fn columns_in(value: &Value, next: &mut usize) -> Option<Value> {
        Some(match value {
            Value::Expression(_) => {
                *next += 1;
                let name = self::call(
                    "polars.col",
                    vec![("name", Value::Columns(vec![stand_in_column(*next)]))],
                )?;
                Value::Expression(Expression::new(Start::Call(name), Vec::new()).ok()?)
            }
            Value::Many(items) => Value::Many(
                items
                    .iter()
                    .map(|item| columns_in(item, next))
                    .collect::<Option<Vec<_>>>()?,
            ),
            Value::Pairs(pairs) => Value::Pairs(
                pairs
                    .iter()
                    .map(|(key, entry)| Some((columns_in(key, next)?, columns_in(entry, next)?)))
                    .collect::<Option<Vec<_>>>()?,
            ),
            other => other.clone(),
        })
    }
    let classes = placed_operands(call, at)
        .into_iter()
        .map(|(_, place, operand)| {
            dtype_polars_gave(&place, operand.links().len(), predicted, observed)
        })
        .collect::<Option<Vec<_>>>()?;
    if classes.is_empty() {
        return None;
    }
    let mut next = 0;
    let given = call
        .arguments()
        .iter()
        .map(|given| Some((given.parameter(), columns_in(given.value(), &mut next)?)))
        .collect::<Option<Vec<_>>>()?;
    Some((self::call(call.entry().id().as_str(), given)?, classes))
}

/// `call`, which is at `at`, without the parameters that hold an expression whose dtype Polars did
/// not give, so that it takes the default of each, and nothing else changed. `None` where no
/// parameter is left out, or the call cannot go without one.
fn call_without_unknowns(
    call: &Call,
    at: &Place,
    predicted: &Prediction,
    observed: &Observed,
) -> Option<Call> {
    let unknown: Vec<&str> = placed_operands(call, at)
        .into_iter()
        .filter(|(_, place, operand)| {
            dtype_polars_gave(place, operand.links().len(), predicted, observed).is_none()
        })
        .map(|(parameter, _, _)| parameter)
        .collect();
    if unknown.is_empty() {
        return None;
    }
    let given = call
        .arguments()
        .iter()
        .filter(|given| !unknown.contains(&given.parameter()))
        .map(|given| (given.parameter(), given.value().clone()))
        .collect();
    self::call(call.entry().id().as_str(), given)
}

/// The parameters of a join that hold its keys.
const KEYS: [&str; 3] = ["on", "left_on", "right_on"];

/// The join of the case with the key that `parameter` holds replaced by `part`, which is then the
/// only key of the join. The other side's key is the bare `key` column, so that the refusal can
/// only be the part's: it is kept where it is that, and where the case has several keys it is
/// put in the place of them all.
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
                _ if name == parameter => Value::Many(vec![Value::Expression(part.clone())]),
                [Value::Expression(key)] if is_bare_key(key) => argument.value().clone(),
                [_, _, ..] => {
                    let bare = call(
                        "polars.col",
                        vec![("name", Value::Columns(vec![key_column(0)]))],
                    )?;
                    Value::Many(vec![Value::Expression(
                        Expression::new(Start::Call(bare), Vec::new()).ok()?,
                    )])
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
    let tables = Tables::standing(false, &[class])?;
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
    let on = |class: Representative| Tables::standing(false, &[class]).unwrap_or_default();
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

/// The case with the last value of every condition that has an `otherwise` made a fill forward of
/// what it was, which gives as many values of the dtype it had, so that the values of the
/// condition are no longer one expression and Polars does not drop its predicates.
fn case_with_values_that_differ(case: &Case) -> Option<Case> {
    case_changed(case, &|call| {
        if !matches!(
            call.entry().id().as_str(),
            "Then.otherwise" | "ChainedThen.otherwise"
        ) {
            return None;
        }
        let Some(Value::Expression(statement)) = call.argument("statement") else {
            return None;
        };
        let fill = Call::new(catalog::entry("Expr.forward_fill")?, Vec::new()).ok()?;
        let links: Vec<Call> = statement
            .links()
            .iter()
            .cloned()
            .chain(std::iter::once(fill))
            .collect();
        let statement = Expression::new(statement.start().clone(), links).ok()?;
        Call::new(
            catalog::entry(call.entry().id().as_str())?,
            vec![("statement", Value::Expression(statement))],
        )
        .ok()
    })
}

/// Asks Polars about a refusal of Polars' own words, at a call inside a predicate of a condition
/// that has an `otherwise`, which Polars allows: it takes a condition whose values are one
/// expression for that expression and drops the predicates, so it never evaluates the part. The
/// case is run again with the last value of each condition made a fill forward of what it was,
/// so that the values differ and the predicates are evaluated, and Polars refusing it for the
/// tables' reason shows that the part is one it skips. A probe of the part alone cannot ask it,
/// for what the part holds may be refused first for another reason. `None` where the refusal is
/// a rule's, is not in a predicate, or the case has no `otherwise` to make differ, and where
/// Polars allows the control or refuses it for another reason.
pub fn settle_by_values_that_differ(case: &Case, refused: &Refused) -> Option<Settled> {
    if !matches!(refused.source, Source::Polars)
        || !refused
            .place
            .path()
            .iter()
            .any(|segment| matches!(segment, Segment::Parameter(name) if name == "predicates"))
    {
        return None;
    }
    let control = changed(case_with_values_that_differ(case), case)?;
    // A call that the tables judge by its parts is refused by Polars for the first it meets, which
    // is not always the one the tables refused first, so a refusal of the case where the values
    // differ is the part's where it is the predicate that holds it that Polars refuses: with that
    // predicate made `false`, and no other, the same case is allowed.
    let by_parts = judges_its_values_apart(case, &refused.place);
    let refusal = (0..RUNS).find_map(|_| {
        observe(&control).refusal.filter(|refusal| {
            refusal.reason == refused.reason
                || (by_parts && is_refused_for_its_predicate(&control, &refused.place))
        })
    })?;
    Some(Settled {
        probe: control.document.write(),
        reason: refusal.reason,
        showing: Showing::WhereTheValuesDiffer,
    })
}

/// Whether `case`, which Polars refuses, is allowed every time it is run with the predicate that
/// holds `place` made `false`, and no other: that predicate is why it is refused.
fn is_refused_for_its_predicate(case: &Case, place: &Place) -> bool {
    let path = place.path();
    // The predicate is the item after the last `predicates` the place goes through, and that is a
    // condition's where a call of an expression holds it, the start or a link, and not the
    // step's own, which a filter holds.
    let Some(at) = path
        .iter()
        .rposition(|segment| matches!(segment, Segment::Parameter(name) if name == "predicates"))
        .filter(|at| matches!(path.get(at + 1), Some(Segment::Item(_))))
        .filter(|at| *at > 0 && matches!(path[at - 1], Segment::Start | Segment::Link(_)))
    else {
        return false;
    };
    // Made `false`, the branch it guards is dropped and the others are still evaluated, which they
    // would not be after a `true`, whose branch Polars takes whatever follows it.
    let Some(falsity) = call("polars.lit", vec![("value", Value::Flag(Some(false)))])
        .and_then(|falsity| Expression::new(Start::Call(falsity), Vec::new()).ok())
    else {
        return false;
    };
    let step = case.document.steps()[0].call();
    let Some((Segment::Parameter(parameter), inside)) = path[..=at + 1].split_first() else {
        return false;
    };
    let given: Option<Vec<(&str, Value)>> = step
        .arguments()
        .iter()
        .map(|argument| {
            let value = if argument.parameter() == parameter {
                replaced_deep_in_value(argument.value(), inside, &falsity)?
            } else {
                argument.value().clone()
            };
            Some((argument.parameter(), value))
        })
        .collect();
    let made_false = given
        .and_then(|given| call(step.entry().id().as_str(), given))
        .and_then(|rebuilt| Step::new(case.document.steps()[0].id().clone(), rebuilt).ok())
        .and_then(|rebuilt| Document::new(vec![rebuilt]).ok())
        .map(|document| Case {
            receiver: case.receiver,
            argument: case.argument,
            document,
        });
    made_false.is_some_and(|made_false| (0..RUNS).all(|_| observe(&made_false).refusal.is_none()))
}

/// `value` with the expression that `path` leads to, through the arguments of the calls of the
/// expressions on the way, replaced by `with`.
fn replaced_deep_in_value(value: &Value, path: &[Segment], with: &Expression) -> Option<Value> {
    match (value, path) {
        (Value::Expression(_), []) => Some(Value::Expression(with.clone())),
        (Value::Expression(expression), _) => {
            Some(Value::Expression(replaced_deep(expression, path, with)?))
        }
        (Value::Many(items), [Segment::Item(at), rest @ ..]) => {
            let mut items = items.clone();
            *items.get_mut(*at)? = replaced_deep_in_value(items.get(*at)?, rest, with)?;
            Some(Value::Many(items))
        }
        (Value::Pairs(pairs), [Segment::Key(at), rest @ ..]) => {
            let mut pairs = pairs.clone();
            pairs.get_mut(*at)?.0 = replaced_deep_in_value(&pairs.get(*at)?.0, rest, with)?;
            Some(Value::Pairs(pairs))
        }
        (Value::Pairs(pairs), [Segment::Value(at), rest @ ..]) => {
            let mut pairs = pairs.clone();
            pairs.get_mut(*at)?.1 = replaced_deep_in_value(&pairs.get(*at)?.1, rest, with)?;
            Some(Value::Pairs(pairs))
        }
        _ => None,
    }
}

/// `expression` with the expression that `path` leads to, from its start or one of its links and
/// through the arguments of that call, however deep, replaced by `with`.
fn replaced_deep(
    expression: &Expression,
    path: &[Segment],
    with: &Expression,
) -> Option<Expression> {
    let rebuilt = |held: &Call, name: &str, rest: &[Segment]| {
        let mut given: Vec<(&str, Value)> = Vec::new();
        for argument in held.arguments() {
            let value = if argument.parameter() == name {
                replaced_deep_in_value(argument.value(), rest, with)?
            } else {
                argument.value().clone()
            };
            given.push((argument.parameter(), value));
        }
        call(held.entry().id().as_str(), given)
    };
    match path {
        [Segment::Start, Segment::Parameter(name), rest @ ..] => {
            let Start::Call(start) = expression.start() else {
                return None;
            };
            Expression::new(
                Start::Call(rebuilt(start, name, rest)?),
                expression.links().to_vec(),
            )
            .ok()
        }
        [Segment::Link(link), Segment::Parameter(name), rest @ ..] => {
            let mut links = expression.links().to_vec();
            let held = links.get(*link)?;
            *links.get_mut(*link)? = rebuilt(held, name, rest)?;
            Expression::new(expression.start().clone(), links).ok()
        }
        _ => None,
    }
}

/// The entries that end a condition, or give a value: `then` and `otherwise` of each state.
const CONDITION_VALUES: [&str; 4] = [
    "When.then",
    "ChainedWhen.then",
    "Then.otherwise",
    "ChainedThen.otherwise",
];

/// Whether Polars' words are those of the rule that a predicate be a `Boolean`.
fn is_refused_as_a_predicate_that_is_no_boolean(reason: &str) -> bool {
    same_reason(&Source::Rule("condition-predicate-is-boolean"), "", reason)
}

/// `expression` cut after its last link that gives a condition a value, which is the condition
/// without what follows it, with the last value of an `otherwise` made a fill forward of what it
/// was, so that the values of the condition are not one expression, which Polars would take for
/// that expression and drop the predicates of. `None` where no link gives a value.
fn condition_alone(expression: &Expression) -> Option<Expression> {
    let last = expression
        .links()
        .iter()
        .rposition(|link| CONDITION_VALUES.contains(&link.entry().id().as_str()))?;
    let mut links = expression.links()[..=last].to_vec();
    let end = links.get(last)?;
    if matches!(
        end.entry().id().as_str(),
        "Then.otherwise" | "ChainedThen.otherwise"
    ) && let Some(Value::Expression(statement)) = end.argument("statement")
    {
        let fill = Call::new(catalog::entry("Expr.forward_fill")?, Vec::new()).ok()?;
        let filled = Expression::new(
            statement.start().clone(),
            statement
                .links()
                .iter()
                .cloned()
                .chain(std::iter::once(fill))
                .collect(),
        )
        .ok()?;
        links[last] = call(
            end.entry().id().as_str(),
            vec![("statement", Value::Expression(filled))],
        )?;
    }
    Expression::new(expression.start().clone(), links).ok()
}

/// `expression` with the expression that `path` leads to, from its start or one of its links and
/// through the arguments of that call, replaced by `with`.
fn replaced_through(
    expression: &Expression,
    path: &[Segment],
    with: &Expression,
) -> Option<Expression> {
    match path {
        [Segment::Link(_), Segment::Parameter(_), ..] => replaced_inside(expression, path, with),
        [Segment::Start, Segment::Parameter(name), rest @ ..] => {
            let Start::Call(start) = expression.start() else {
                return None;
            };
            let mut given: Vec<(&str, Value)> = Vec::new();
            for argument in start.arguments() {
                let value = if argument.parameter() == name {
                    replaced_in_value(argument.value(), rest, with)?
                } else {
                    argument.value().clone()
                };
                given.push((argument.parameter(), value));
            }
            Expression::new(
                Start::Call(call(start.entry().id().as_str(), given)?),
                expression.links().to_vec(),
            )
            .ok()
        }
        _ => None,
    }
}

/// The conditions the tables hold to the rule that their predicate be a `Boolean`, which Polars
/// does not hold where it never evaluates the predicate.
const CONDITION_RULE: &str = "condition-predicate-is-boolean";

/// Asks Polars about a predicate that the rule that it be a `Boolean` refused and Polars allows
/// because it never evaluates it, as a sort key of one value is dropped and `or_(true)` folds what
/// it follows. A probe of the predicate on a column of its dtype cannot ask it where the dtype is
/// not known, as for a predicate that Polars refuses on its own for another reason, and a pivot's
/// aggregate has no column to stand in. The condition that holds the predicate is asked alone, in
/// the place of the expression that holds it, with what follows it removed and its last value made
/// to differ from the others, which removes what Polars folds or drops it for. Polars refusing it
/// shows that Polars evaluates the predicate and refuses it once nothing hides it. That it is the
/// predicate is shown by a control: the same condition with that predicate made `false` is allowed,
/// which drops its branch and leaves the other predicates evaluated, as a `true` would not; and by
/// a second, with it made `true`, which keeps the value the branch guards and shows it is no cause.
///
/// The refusal may be in the rule's words, or in other words for what the predicate holds, which
/// Polars meets before it judges the dtype. Both are a refusal of the predicate once it is
/// evaluated, and the outcome says which it was. `None` for another rule, where the refusal is not
/// of a predicate of a condition, where there is no condition to ask alone, and where Polars
/// allows the condition alone, refuses it elsewhere than in the predicate, or refuses the control
/// too.
pub fn settle_by_evaluating_the_condition(case: &Case, refused: &Refused) -> Option<Settled> {
    if refused.source != Source::Rule(CONDITION_RULE) {
        return None;
    }
    let found = find(case, &refused.place)?;
    if !found.rest.is_empty() {
        return None;
    }
    let [.., (holder, holder_at), (_, predicate_at)] = found.levels.as_slice() else {
        return None;
    };
    let inner = refused.place.path().get(*holder_at..*predicate_at)?;
    let alone = condition_alone(holder)?;
    let held = place_of_the_part(&found, false);
    // Where the predicate stands in the probe: in the condition that stands alone.
    let predicate_place = inner.iter().cloned().fold(held, Place::then);
    // Made `false` the branch the predicate guards is dropped, and the predicates after it are
    // still evaluated, which a `true` would leave unevaluated where Polars takes its branch.
    let falsity = Expression::new(
        Start::Call(call(
            "polars.lit",
            vec![("value", Value::Flag(Some(false)))],
        )?),
        Vec::new(),
    )
    .ok()?;
    let with_falsity = replaced_through(&alone, inner, &falsity)?;
    // Made `false` the predicate drops the value its branch guards with it, so a value that Polars
    // refuses only when the rows run, with a refusal that has no place, would be shown as the
    // predicate's fault. Made `true` the branch is taken and its value is evaluated, so that
    // case, which is allowed only where the value is no cause, shows the value is fine.
    let truth = Expression::new(
        Start::Call(call(
            "polars.lit",
            vec![("value", Value::Flag(Some(true)))],
        )?),
        Vec::new(),
    )
    .ok()?;
    let with_truth = replaced_through(&alone, inner, &truth)?;
    let run = |part: Expression| {
        Some(Case {
            receiver: case.receiver,
            argument: case.argument,
            document: document_of(case, &found, part, false)?,
        })
    };
    let probed = run(alone)?;
    let control = run(with_falsity)?;
    let value_control = run(with_truth)?;
    let tables = tables_of(&found, None);
    let refusal = (0..RUNS).find_map(|_| {
        observe_probe(&probed, tables)
            .refusal
            .filter(|refusal| lies_in(refusal, &predicate_place))
    })?;
    // The controls are allowed every time they run.
    if (0..RUNS).any(|_| {
        observe_probe(&control, tables).refusal.is_some()
            || observe_probe(&value_control, tables).refusal.is_some()
    }) {
        return None;
    }
    Some(Settled {
        probe: probed.document.write(),
        reason: refusal.reason,
        showing: Showing::WhereTheConditionIsEvaluated,
    })
}

/// Whether Polars' refusal is in the predicate at `place`: it names no place, as when the rows
/// are refused, or a place within the predicate.
fn lies_in(refusal: &Refusal, place: &Place) -> bool {
    refusal
        .place
        .as_ref()
        .is_none_or(|at| at.path().is_empty() || at.path().starts_with(place.path()))
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
        .or_else(|| settle_a_start_given_columns(case, refused, predicted, observed))
        .or_else(|| settle_as_a_key(case, refused))
        .or_else(|| settle_on_a_value_as_a_key(case, refused, predicted, observed))
        .or_else(|| settle_in_the_position(case, refused, predicted, observed))
        .or_else(|| settle_given_no_known_dtype(case, refused))
        .or_else(|| settle_a_predicate_on_a_value(case, refused, predicted, observed))
        .or_else(|| settle_by_values_that_differ(case, refused))
        .or_else(|| settle_by_evaluating_the_condition(case, refused))
}

/// Whether `rule` is one that a predicate be a `Boolean`, which a probe of the predicate apart
/// from what it holds can show right.
pub fn is_a_predicate_rule(rule: &str) -> bool {
    PREDICATE_RULES.contains(&rule)
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
    // A call that the tables judge by its values apart, as a replacement, is refused by them for
    // the first value they refuse, beside a companion, and Polars names whichever it meets: the
    // reason is not compared either.
    if !(refused.argument_unknown || judges_its_values_apart(case, &refused.place))
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

    #[test]
    fn a_comparison_of_two_series_is_the_same_refusal_in_the_words_of_either_engine() {
        let could_not = "could not evaluate '>=' comparison between series 'a' of dtype: List(String) and series 'b' of dtype: Int8";
        let cannot = "cannot perform '>=' comparison between series 'p' of dtype: list[str] and series 'b' of dtype: list[str]";
        assert!(is_the_same_comparison(could_not, cannot));
        assert!(is_the_same_comparison(cannot, could_not));
        // Another comparison is another refusal, and so is another objection of the same words.
        let other = "cannot perform '<' comparison between series 'p' of dtype: list[str] and series 'b' of dtype: list[str]";
        assert!(!is_the_same_comparison(could_not, other));
        assert!(!is_the_same_comparison(
            could_not,
            "cannot cast List type (inner: 'String', to: 'String')"
        ));
    }

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
    fn a_bitwise_call_on_text_is_refused_alone_where_polars_gives_a_column_its_schema_does_not_say()
    {
        // `when(k > k).then(k).otherwise(k).or_(h)` of a whole number and text, in the value of
        // a condition that a constant decides, so that Polars never evaluates it. The tables
        // refuse it as Polars on one thread does, by failing to unpack the text as a whole
        // number. On many threads Polars' streaming engine does not fail: it gives the column
        // of the whole number where its schema says text, which the target counts as a refusal.
        let found =
            settled("stricter-a-bitwise-or-of-text-in-a-condition-decided-by-a-constant.case");
        // On few threads Polars' reason is the tables' own, the failure to unpack the text.
        assert!(
            found.reason.starts_with("the schema says")
                || found.reason.starts_with("cannot unpack series of type"),
            "{}",
            found.reason
        );
        assert!(found.probe.contains("Expr.or_"), "{}", found.probe);
    }

    /// What the probe of a refused part apart from the case settles: the tables' refusal of the
    /// kept case, and what Polars gave it.
    fn settled_apart(name: &str) -> Option<Settled> {
        let case = kept(name);
        settle_apart(&case, &refusal(&case), &predict(&case), &observe(&case))
    }

    #[test]
    fn a_whole_number_as_the_predicate_of_a_condition_under_a_len_is_refused_on_a_column_of_its_dtype()
     {
        // `when((h < 1.5).cum_count()).then(f).when(f).then(g).len()`: the length of a condition
        // never evaluates its predicate, and the predicate alone is refused for the comparison of
        // text with a number, which the tables do not refuse, before it is for what the rule is
        // about. A column of the dtype that reaches the end of the predicate, the count, is refused
        // as the rule says, in the place the predicate stands.
        let found =
            settled_apart("stricter-a-count-as-the-predicate-of-a-condition-under-a-len.case")
                .expect("a column of the dtype that Polars refuses as a predicate");
        assert!(found.reason.contains("Boolean"), "{}", found.reason);
        assert!(found.probe.contains("polars.when"), "{}", found.probe);
        assert!(!found.probe.contains("Expr.cum_count"), "{}", found.probe);
    }

    #[test]
    fn a_call_whose_argument_polars_refuses_first_is_refused_on_columns_of_the_dtypes_that_reach_it()
     {
        // `sort(by=lit(null).pow((h < 1.5).first()))`: Polars drops a sort key that is a literal
        // and never meets the power, and the probe of the power on a column is refused first for
        // the comparison of text with a number in its exponent, which the tables allow without
        // knowing the dtype of the number. The power of a column of the dtype Null by a column of
        // the dtype that the exponent has is refused as the tables say.
        let found = settled_apart("stricter-a-power-of-a-null-literal-in-a-sort-key.case")
            .expect("the call on columns of the dtypes that reach it");
        assert!(found.reason.contains("not supported"), "{}", found.reason);
        assert!(found.probe.contains("Expr.pow"), "{}", found.probe);
        // The receiver and the exponent are each a column; the comparison is not in the probe.
        assert!(found.probe.contains("\"p2\""), "{}", found.probe);
        assert!(!found.probe.contains("Expr.lt"), "{}", found.probe);
    }

    #[test]
    fn a_condition_no_row_takes_is_asked_on_a_column_of_the_dtype_polars_gave_it() {
        // `h > when(b.arccosh() == k).then(a)` with a list `a`: no row of the sample takes the
        // branch, so the condition is all nulls, which Polars compares with text where it refuses
        // the list. The tables say nothing of the dtype of a condition with no `otherwise`, and
        // Polars gives it the dtype of its values, so a column of that dtype beside a column of
        // text is refused as the tables say.
        let found = settled_apart("stricter-a-condition-with-no-otherwise-that-no-row-takes.case")
            .expect("the call on columns of the dtypes that Polars gave it");
        assert!(
            found.reason.contains("cannot cast List"),
            "{}",
            found.reason
        );
        assert!(!found.probe.contains("polars.when"), "{}", found.probe);
    }

    #[test]
    fn a_condition_is_not_asked_on_a_column_where_the_tables_gave_it_another_dtype() {
        // The tables made no claim of the condition above, so Polars' dtype stands; where they
        // claim another, the case is a disagreement about the dtype and nothing is asked.
        let case = kept("stricter-a-condition-with-no-otherwise-that-no-row-takes.case");
        let mut wrong = predict(&case);
        for traced in &mut wrong.expressions {
            for held in &mut traced.links {
                if held.is_none() {
                    *held = Some(Representative::String);
                }
            }
        }
        assert_eq!(
            settle_apart(&case, &refusal(&case), &wrong, &observe(&case)),
            None
        );
    }

    #[test]
    fn a_call_is_not_asked_on_columns_where_polars_gave_an_argument_no_dtype() {
        let case = kept("stricter-a-power-of-a-null-literal-in-a-sort-key.case");
        assert_eq!(
            settle_apart(
                &case,
                &refusal(&case),
                &predict(&case),
                &Observed::default()
            ),
            None
        );
    }

    #[test]
    fn a_key_beside_another_key_is_asked_as_the_only_key_of_the_join() {
        // `join(left_on=[lit(1, Float32).cum_max(), f | 1], right_on=[key, key2])`: a literal is
        // folded into one value, which Polars allows as a key, where the running maximum of a
        // column is not elementwise. The other key is refused first for what it holds when the
        // join is asked as it is written, so the key is asked as the only one of the join, as a
        // column of the dtype that reaches the call is.
        let found = settled_apart(
            "stricter-a-running-value-of-a-literal-in-a-join-key-beside-another-key.case",
        )
        .expect("the call on a column as the only key of the join");
        assert!(found.reason.contains("elementwise"), "{}", found.reason);
        assert!(found.probe.contains("LazyFrame.join"), "{}", found.probe);
        assert!(!found.probe.contains("Expr.or_"), "{}", found.probe);
    }

    #[test]
    fn a_predicate_is_not_asked_on_a_column_where_polars_gave_no_dtype_or_the_tables_gave_no_rule()
    {
        let case = kept("stricter-a-count-as-the-predicate-of-a-condition-under-a-len.case");
        let rule = refusal(&case);
        // What Polars gave the predicate is not known, so the column is of no dtype it has.
        assert_eq!(
            settle_a_predicate_on_a_value(&case, &rule, &predict(&case), &Observed::default()),
            None
        );
        // A refusal of Polars' own, the same place, is no rule's.
        let polars = Refused {
            source: Source::Polars,
            ..rule
        };
        assert_eq!(
            settle_apart(&case, &polars, &predict(&case), &observe(&case)),
            None
        );
    }

    #[test]
    fn a_failure_to_unpack_a_series_is_matched_by_a_schema_the_rows_do_not_give_and_by_nothing_else()
     {
        let tables = "cannot unpack series of type `str` into `i64`";
        let mismatch = "the schema says String and the rows give Int64";
        assert!(same_reason(&Source::Polars, tables, mismatch));
        // Another reason of the tables is not that mismatch, nor a mismatch the tables' own reason.
        assert!(!same_reason(
            &Source::Polars,
            "cannot compare string with numeric type (i8)",
            mismatch
        ));
        assert!(!same_reason(&Source::Polars, mismatch, mismatch));
    }

    #[test]
    fn a_bitwise_call_on_a_list_in_a_key_of_one_value_is_refused_alone_where_polars_cannot_cast_it()
    {
        // `sort(by=when(lit(1, UInt8) | b).then(k).len())` with a list `b`: the length of the
        // value is one number, so Polars never evaluates the call. The tables refuse it as
        // Polars on one thread does, by failing to unpack the list as a whole number; the
        // streaming engine on many threads fails to cast the list to that number.
        let found = settled("stricter-a-bitwise-or-of-a-list-in-a-sort-key-of-one-value.case");
        // On few threads Polars' reason is the tables' own, the failure to unpack the list.
        assert!(
            found.reason.starts_with("cannot cast List type")
                || found.reason.starts_with("cannot unpack series of type"),
            "{}",
            found.reason
        );
        assert!(found.probe.contains("Expr.or_"), "{}", found.probe);
    }

    #[test]
    fn a_failure_to_unpack_a_list_is_matched_by_a_failure_to_cast_the_list_and_by_nothing_else() {
        let tables = "cannot unpack series of type `list[i64]` into `u8`";
        let cast = "cannot cast List type (inner: 'Int64', to: 'UInt8')";
        assert!(same_reason(&Source::Polars, tables, cast));
        // A cast of another dtype is not it, nor is a cast the tables' reason names.
        assert!(!same_reason(
            &Source::Polars,
            "cannot unpack series of type `str` into `i64`",
            cast
        ));
        assert!(!same_reason(
            &Source::Polars,
            tables,
            "cannot cast String type to Int64"
        ));
    }

    #[test]
    fn a_bitwise_call_on_an_array_in_a_condition_of_equal_branches_is_refused_alone_where_polars_cannot_cast_it()
     {
        // `sort(by=when(g <= lit(null, Int8) & b).then(x).otherwise(x))` with an array `b`: Polars
        // folds a condition whose branches are the same expression into that expression, so it
        // never evaluates the predicate. The tables refuse the call as Polars on one thread does,
        // by failing to unpack the array as a whole number; the streaming engine on many threads
        // fails to cast the array to that number.
        let found =
            settled("stricter-a-bitwise-call-on-an-array-in-a-condition-of-equal-branches.case");
        assert!(
            found.reason.starts_with("cannot cast Array type")
                || found.reason.starts_with("cannot unpack series of type"),
            "{}",
            found.reason
        );
        assert!(found.probe.contains("Expr.and_"), "{}", found.probe);
    }

    #[test]
    fn a_failure_to_unpack_an_array_is_matched_by_a_failure_to_cast_the_array_and_by_nothing_else()
    {
        let tables = "cannot unpack series of type `array[i64, 2]` into `i8`";
        let cast = "cannot cast Array type (inner: 'Int64', to: 'Int8')";
        assert!(same_reason(&Source::Polars, tables, cast));
        // The cast of a list is another dtype's, and neither is a cast the tables' reason does not
        // name, nor the unpacking of text.
        assert!(!same_reason(
            &Source::Polars,
            tables,
            "cannot cast List type (inner: 'Int64', to: 'Int8')"
        ));
        assert!(!same_reason(
            &Source::Polars,
            "cannot unpack series of type `list[i64]` into `i8`",
            cast
        ));
        assert!(!same_reason(
            &Source::Polars,
            "cannot unpack series of type `str` into `i64`",
            cast
        ));
        assert!(!same_reason(
            &Source::Polars,
            tables,
            "cannot cast String type to Int64"
        ));
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
    fn a_call_is_refused_on_a_value_with_an_argument_of_no_known_dtype_left_at_its_default() {
        // `lit("1d").log(base=1 // null)`: Polars drops the sort key, which is one value. The
        // base is refused first as written, and its dtype is not known, so no column stands in
        // for it. The log of a column of text, given no base, is refused as the tables say.
        let name = "stricter-a-call-given-an-argument-of-no-known-dtype.case";
        let case = kept(name);
        let refused = refusal(&case);
        assert_eq!(settle(&case, &refused), None);
        let found =
            on_a_value(&case, &refused).expect("a probe that Polars refuses as the tables do");
        assert!(found.reason.contains("log"), "{}", found.reason);
        assert!(!found.probe.contains("base"), "{}", found.probe);
        assert!(!found.probe.contains("floordiv"), "{}", found.probe);
        // A receiver that a log takes is allowed with the default base: nothing shows the tables
        // right.
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/allowed_fuzz/regressions")
                .join(name),
        )
        .expect("a kept case");
        let number = Case::decode(
            text.replace("{\"string\":\"1d\"}", "{\"integer\":4}")
                .as_bytes(),
        )
        .expect("a case");
        assert_eq!(on_a_value(&number, &refusal(&case)), None);
        // An argument that the call cannot go without is not left out.
        let required = Case::decode(
            text.replace("Expr.log", "Expr.add")
                .replace("\"base\"", "\"other\"")
                .as_bytes(),
        )
        .expect("a case");
        assert_eq!(on_a_value(&required, &refusal(&case)), None);
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
    fn a_literal_that_a_call_is_given_is_a_column_of_its_dtype_where_the_call_is_asked_on_a_value()
    {
        // `f < null` of `List(String)` in a `then` that no row takes: as written, Polars
        // folds the comparison with the null literal and allows it. With the null as a
        // column of its dtype, Polars refuses the comparison as the tables do.
        let case = kept("stricter-a-literal-compared-in-a-then-no-row-takes.case");
        let refused = refusal(&case);
        assert_eq!(settle(&case, &refused), None);
        let found = on_a_value(&case, &refused).expect("a call that Polars refuses on columns");
        assert!(
            found.reason.contains("could not evaluate"),
            "{}",
            found.reason
        );
        assert!(!found.probe.contains("polars.lit"), "{}", found.probe);
        assert!(
            found.probe.contains("\"columns\":[\"p2\"]"),
            "{}",
            found.probe
        );
    }

    /// `k.eq(1)` of `Int64`, which Polars allows and the tables are said to refuse.
    const A_COMPARISON_GIVEN_A_LITERAL_OF_ITS_DTYPE: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[{"entry":"Expr.eq","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":1},"dtype":{"dtype":"Int64"}}},"then":[]}}}}]}}]}}}}]}"#;

    #[test]
    fn a_call_that_polars_allows_given_a_column_of_the_literals_dtype_settles_nothing() {
        let case =
            Case::decode(A_COMPARISON_GIVEN_A_LITERAL_OF_ITS_DTYPE.as_bytes()).expect("a case");
        let refused = refused_at_the_link("cannot compare string with numeric type (i64)");
        assert_eq!(on_a_value(&case, &refused), None);
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
    fn a_start_given_a_literal_is_settled_by_polars_refusing_it_given_a_column() {
        // `join(right_on=[datetime(1, from_epoch(lit(null, List(Int64))), rk).len()])`: the tables
        // refuse the count of a null list for the dtype of the list, Polars never evaluates what
        // the length of a call is the length of, and allows the literal where the call is asked
        // alone, which it folds. Given a column of the dtype of the literal, it refuses the count
        // for the tables' reason.
        let case = kept("stricter-a-start-given-a-null-list-polars-folds-under-a-length.case");
        let refused = refusal(&case);
        assert_eq!(settle(&case, &refused), None);
        let found = settle_apart(&case, &refused, &predict(&case), &observe(&case))
            .expect("a count that Polars refuses of a column");
        assert!(found.probe.contains("polars.from_epoch"), "{}", found.probe);
        assert!(
            found.reason.contains("cannot cast List"),
            "{}",
            found.reason
        );
        assert_eq!(found.showing, Showing::TheSameReason);
    }

    #[test]
    fn a_start_given_a_condition_that_no_row_takes_is_settled_by_polars_refusing_a_column() {
        // `with_columns(from_epoch(when(h <= a).then(time()), "ns"))` of a column `a` that is all
        // null: no row takes the condition, which has no `otherwise`, so the count meets only nulls
        // and Polars allows what the tables refuse for the dtype of a time of day. Given a column of
        // that dtype, Polars refuses the count as the tables do.
        let case = kept("stricter-a-start-given-a-condition-that-no-row-takes.case");
        let refused = refusal(&case);
        assert_eq!(settle(&case, &refused), None);
        let found = settle_apart(&case, &refused, &predict(&case), &observe(&case))
            .expect("a count that Polars refuses of a column");
        assert!(found.probe.contains("polars.from_epoch"), "{}", found.probe);
        assert!(
            found.reason.contains("Time to Datetime"),
            "{}",
            found.reason
        );
        assert_eq!(found.showing, Showing::TheSameReason);
    }

    #[test]
    fn a_start_is_not_settled_where_polars_allows_it_given_a_column_too() {
        // The tables refused a count of a whole number that Polars allows for a column of whole
        // numbers as well, so the literal is no reason, and the part is evaluated and accepted.
        let case = Case::decode(
            br#"{"a":"Int64","b":"Int64","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.from_epoch","arguments":{"column":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":1}}},"then":[]}}}},"then":[]}}]}}}}]}"#,
        )
        .expect("a case");
        let place = Place::of_step(0, StepId::new("s").ok())
            .then(Segment::Parameter("exprs".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Start);
        let refused = Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage: Some(dageist_engine::allowed::Stage::Rows),
            reason: "cannot cast List type (inner: 'Int64', to: 'Datetime(\'μs\')')".to_owned(),
            argument_unknown: false,
        };
        assert_eq!(
            settle_apart(&case, &refused, &predict(&case), &observe(&case)),
            None
        );
    }

    #[test]
    fn moments_in_two_zones_are_settled_by_polars_refusing_the_part_where_a_sort_key_skips_it() {
        // `sort(by=[datetime(.., time_zone=Europe/Berlin).first() > lit(null, Datetime(UTC))])`:
        // the rule refuses a comparison of moments in two zones, and Polars drops a sort key that
        // is one value; asked as written, it refuses the comparison for the two zones.
        let case = kept("stricter-moments-in-two-zones-in-a-sort-key-of-one-value.case");
        let refused = refusal(&case);
        assert!(matches!(
            refused.source,
            Source::Rule("moments-share-a-zone")
        ));
        let found = settle(&case, &refused).expect("a comparison that Polars refuses alone");
        assert!(found.probe.contains("Expr.gt"), "{}", found.probe);
        assert_eq!(found.showing, Showing::TheSameReason);
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
    fn a_replacement_is_settled_by_polars_refusing_it_whatever_value_it_names() {
        // `sort(by=f // a.replace(1, 1).len())` of `List(Int64)`: the tables refuse the
        // replacement for the values it lists apart, beside a companion that is a list, and Polars
        // drops a sort key that is one value; asked as written, it refuses the replacement for
        // the dtypes the values really have, and for another reason than the companion's.
        let case = kept("stricter-a-replacement-of-a-list-in-a-sort-key-of-one-value.case");
        let refused = refusal(&case);
        assert!(!refused.argument_unknown);
        let found = settle(&case, &refused).expect("a replacement that Polars refuses");
        assert!(found.probe.contains("Expr.replace"), "{}", found.probe);
        assert_ne!(found.reason, refused.reason);
        assert_eq!(found.showing, Showing::TheSameReason);
        // A call that is no replacement or interval is settled for the tables' reason only.
        let other =
            kept("stricter-a-comparison-given-no-known-dtype-in-a-sort-key-of-one-value.case");
        assert_eq!(settle(&other, &refusal(&other)), None);
    }

    #[test]
    fn a_test_of_closeness_is_settled_by_polars_refusing_it_whichever_value_it_names() {
        // `when(k is null).then(f).when(h is close to b).then(f).otherwise(f)`: the values of the
        // condition are one expression, so Polars drops its predicates, and the tables refuse
        // the test for text, as Polars does where it evaluates it, which names the text or the
        // list it is given first by how many threads it has.
        let case = kept("stricter-a-test-of-closeness-in-a-condition-of-identical-values.case");
        let refused = refusal(&case);
        let found = settle_apart(&case, &refused, &predict(&case), &observe(&case))
            .expect("a test of closeness that Polars refuses");
        assert!(found.probe.contains("Expr.is_close"), "{}", found.probe);
    }

    #[test]
    fn a_replacement_the_rule_refuses_is_settled_by_polars_refusing_the_part_for_its_lengths() {
        // `sort(by=lit(1).replace_strict(1.0, "1d", default=g).max())`: the rule refuses a default
        // of many values beside a receiver of one, and Polars drops a sort key that is one value;
        // asked as written, it refuses the replacement for the lengths.
        let case = kept("stricter-a-replacement-of-values-in-a-sort-key-of-one-value.case");
        let refused = refusal(&case);
        assert!(matches!(refused.source, Source::Rule(_)));
        let found = settle(&case, &refused).expect("a replacement that Polars refuses alone");
        assert!(
            found.probe.contains("Expr.replace_strict"),
            "{}",
            found.probe
        );
        assert!(
            found.reason.contains("must have the same length as"),
            "{}",
            found.reason
        );
        assert_eq!(found.showing, Showing::TheSameReason);
    }

    #[test]
    fn an_arithmetic_given_no_known_dtype_is_settled_by_polars_refusing_it() {
        // `sort(by=g.over(order_by=[g - h.first().over(order_by=[k], mapping_strategy=join)]).sum())`:
        // the tables do not know the dtype of the window that joins after one value,
        // which is subtracted from a text column, and refuse the subtraction for every
        // dtype it might give.
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

    /// A pivot whose aggregate is `element()` followed by `links`, each a call without
    /// arguments, on `a` of the dtype `receiver`.
    fn pivot_of_the_element(receiver: &str, links: &[(&str, &str)]) -> Case {
        let calls: Vec<String> = links
            .iter()
            .map(|(id, arguments)| format!(r#"{{"entry":"{id}","arguments":{arguments}}}"#))
            .collect();
        let text = format!(
            r#"{{"a":"{receiver}","b":"Date","revision":1}}
{{"version":1,"steps":[{{"id":"s","call":{{"entry":"LazyFrame.pivot","arguments":{{"on":{{"columns":["h"]}},"on_columns":{{"table":"on_h"}},"values":{{"columns":["a"]}},"aggregate_function":{{"expression":{{"start":{{"entry":"polars.element","arguments":{{}}}},"then":[{}]}}}}}}}}}}]}}"#,
            calls.join(",")
        );
        Case::decode(text.as_bytes()).expect("a case")
    }

    #[test]
    fn a_call_in_a_pivots_aggregate_is_refused_on_the_element_where_it_holds_the_values_it_gets() {
        // `element().str.pad_start(null).cast(Int8)`: a pad by a null gives nulls, which every dtype
        // holds, so Polars allows the cast, which the tables refuse for the text of the sample's
        // rows. The group's values are those of the column `a`, which is text, so `element()` is
        // the text that reaches the cast and Polars refuses the cast of it.
        let found = settled_apart("stricter-a-null-count-before-a-cast-in-a-pivot-aggregate.case")
            .expect("the cast of the element, in the same pivot");
        assert!(
            found.reason.contains("conversion from `str`"),
            "{}",
            found.reason
        );
        assert!(found.probe.contains("LazyFrame.pivot"), "{}", found.probe);
        assert!(found.probe.contains("polars.element"), "{}", found.probe);
        assert!(
            !found.probe.contains("Expr.str.pad_start"),
            "{}",
            found.probe
        );
    }

    #[test]
    fn a_call_in_a_pivots_aggregate_is_not_asked_on_the_element_where_another_dtype_reaches_it() {
        // The element is whole numbers, and a cast to text gives the text that reaches the cast to
        // a whole number, which is no dtype of the element: no value of the aggregate stands for it.
        let case = pivot_of_the_element(
            "Int64",
            &[
                ("Expr.cast", r#"{"dtype":{"dtype":"String"}}"#),
                ("Expr.cast", r#"{"dtype":{"dtype":"Int8"}}"#),
            ],
        );
        let place = Place::of_step(0, StepId::new(STEP).ok())
            .then(Segment::Parameter("aggregate_function".to_owned()))
            .then(Segment::Link(1));
        let refused = Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage: Some(dageist_engine::allowed::Stage::Rows),
            reason: "conversion from `str` to `i8` failed in column 'a' for 2 out of 3 values: [\"1d\", \"2d\"]".to_owned(),
            argument_unknown: false,
        };
        assert_eq!(
            settle_on_a_value(&case, &refused, &predict(&case), &observe(&case)),
            None
        );
    }

    #[test]
    fn the_catalogue_has_what_a_probe_is_made_of() {
        for id in ["LazyFrame.select", "LazyFrame.pivot", "LazyFrame.filter"] {
            assert!(catalog::entry(id).is_some(), "{id}");
        }
    }
    #[test]
    fn a_predicate_that_polars_drops_is_settled_by_the_case_with_the_values_made_to_differ() {
        // `when(f.clip(..)).then(a).when(..).then(a).otherwise(a)`: Polars drops the predicates,
        // and refuses the clip of a `Boolean` once the last value differs.
        let case = kept("stricter-a-predicate-that-is-dropped.case");
        let refused = refusal(&case);
        let found = settle_by_values_that_differ(&case, &refused)
            .expect("a predicate that Polars refuses where the values differ");
        assert_eq!(found.showing, Showing::WhereTheValuesDiffer);
        assert_eq!(found.reason, refused.reason);
        assert!(found.probe.contains("Expr.forward_fill"), "{}", found.probe);
        // Asked of the whole case, as the target asks it.
        assert!(
            settle_apart(&case, &refused, &predict(&case), &observe(&case))
                .is_some_and(|found| found.showing == Showing::WhereTheValuesDiffer)
        );
    }

    #[test]
    fn a_predicate_of_a_call_judged_by_its_parts_is_settled_by_any_refusal_of_the_call() {
        // `when(k.is_nan().dt.replace(year=b, day=g)).then(a)..otherwise(a)`: Polars drops the
        // predicates, and where the last value differs refuses the day it converts, a text that
        // is no whole number, before it meets the `Boolean` the tables refused the replacement of.
        let name = "stricter-a-predicate-that-is-dropped--a-part-of-a-date-refused-first.case";
        let case = kept(name);
        let refused = refusal(&case);
        let found = settle_by_values_that_differ(&case, &refused)
            .expect("a predicate that Polars refuses where the values differ");
        assert_eq!(found.showing, Showing::WhereTheValuesDiffer);
        assert_ne!(found.reason, refused.reason);
        assert!(
            found.reason.contains("conversion from `str`"),
            "{}",
            found.reason
        );
        assert!(found.probe.contains("Expr.forward_fill"), "{}", found.probe);
        // A call that the tables judge as a whole, the test for a NaN before it, is one that Polars
        // names a reason of its own for: the tables' words must be the same.
        let path = refused.place.path();
        let before = path[..path.len() - 1]
            .iter()
            .cloned()
            .fold(Place::of_step(0, StepId::new(STEP).ok()), Place::then)
            .then(Segment::Link(0));
        let of_the_test = Refused {
            place: before,
            ..refused.clone()
        };
        assert_eq!(settle_by_values_that_differ(&case, &of_the_test), None);
    }

    #[test]
    fn a_predicate_is_not_settled_by_the_refusal_of_another_predicate_of_the_condition() {
        // `when(a.dt.replace(day=1).is_null()).then(a).when(a.dt.replace(day=g).is_null()).then(a)
        // .otherwise(a)`: Polars drops the predicates. Where the last value differs it refuses the
        // second predicate, whose day is text, and the first it allows. The refusal the tables
        // are asked about is the first predicate's replacement, which that refusal does not show.
        // With the first made `true` Polars folds the condition to its first value and never
        // evaluates the second, which is no showing that the first is why it was refused.
        let col = |name: &str| {
            format!(
                r#"{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["{name}"]}}}}}},"then":[]}}}}"#
            )
        };
        let predicate = |start: &str, day: String| {
            format!(
                r#"{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["{start}"]}}}}}},"then":[{{"entry":"Expr.dt.replace","arguments":{{"day":{day}}}}},{{"entry":"Expr.is_null","arguments":{{}}}}]}}}}"#
            )
        };
        let one = r#"{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":1}}},"then":[]}}"#
            .to_owned();
        let value = |entry: &str| {
            format!(
                r#"{{"entry":"{entry}","arguments":{{"statement":{}}}}}"#,
                col("a")
            )
        };
        let text = format!(
            r#"{{"a":"Date","b":"Date","revision":1}}
{{"version":1,"steps":[{{"id":"s","call":{{"entry":"LazyFrame.select","arguments":{{"exprs":{{"many":[{{"expression":{{"start":{{"entry":"polars.when","arguments":{{"predicates":{{"many":[{}]}}}}}},"then":[{},{{"entry":"Then.when","arguments":{{"predicates":{{"many":[{}]}}}}}},{},{}]}}}}]}}}}}}}}]}}"#,
            predicate("a", one),
            value("When.then"),
            predicate("a", col("g")),
            value("ChainedWhen.then"),
            value("ChainedThen.otherwise"),
        );
        let case = Case::decode(text.as_bytes()).expect("a case");
        let place = Place::of_step(0, StepId::new(STEP).ok())
            .then(Segment::Parameter("exprs".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Start)
            .then(Segment::Parameter("predicates".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Link(0));
        let refused = Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage: Some(dageist_engine::allowed::Stage::Rows),
            reason: "`round` operation not supported for dtype `bool` (expected: date/datetime)"
                .to_owned(),
            argument_unknown: false,
        };
        assert!(
            observe(&case).refusal.is_none(),
            "Polars drops the predicates"
        );
        assert_eq!(settle_by_values_that_differ(&case, &refused), None);
    }

    #[test]
    fn a_predicate_of_a_filter_is_not_settled_as_the_predicate_of_a_condition_is() {
        // `filter(a.dt.replace(day=g).is_null(), when(f).then(f).otherwise(f))`: the refusal asked
        // about is the first predicate of the filter, which is no predicate of a condition and
        // which Polars does not drop. The case where the values of the condition differ is refused
        // for it, and allowed with it made `false`, which is not that Polars skips it.
        let col = |name: &str| {
            format!(
                r#"{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["{name}"]}}}}}},"then":[]}}}}"#
            )
        };
        let text = format!(
            r#"{{"a":"Date","b":"Date","revision":1}}
{{"version":1,"steps":[{{"id":"s","call":{{"entry":"LazyFrame.filter","arguments":{{"predicates":{{"many":[{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["a"]}}}}}},"then":[{{"entry":"Expr.dt.replace","arguments":{{"day":{}}}}},{{"entry":"Expr.is_null","arguments":{{}}}}]}}}},{{"expression":{{"start":{{"entry":"polars.when","arguments":{{"predicates":{{"many":[{}]}}}}}},"then":[{{"entry":"When.then","arguments":{{"statement":{}}}}},{{"entry":"Then.otherwise","arguments":{{"statement":{}}}}}]}}}}]}}}}}}}}]}}"#,
            col("g"),
            col("f"),
            col("f"),
            col("f"),
        );
        let case = Case::decode(text.as_bytes()).expect("a case");
        let place = Place::of_step(0, StepId::new(STEP).ok())
            .then(Segment::Parameter("predicates".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Link(0));
        let refused = Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage: Some(dageist_engine::allowed::Stage::Rows),
            reason: "`round` operation not supported for dtype `bool` (expected: date/datetime)"
                .to_owned(),
            argument_unknown: false,
        };
        assert_eq!(settle_by_values_that_differ(&case, &refused), None);
    }

    #[test]
    fn a_predicate_is_not_settled_where_the_values_made_to_differ_are_not_refused_for_its_reason() {
        let case = kept("stricter-a-predicate-that-is-dropped.case");
        let refused = refusal(&case);
        // Another reason than the one Polars refuses the control for.
        let other = Refused {
            reason: "`clip` operation not supported for dtype `date`".to_owned(),
            ..refused.clone()
        };
        assert_eq!(settle_by_values_that_differ(&case, &other), None);
        // A refusal that is a rule's, which is no refusal of Polars' own words.
        let rule = Refused {
            source: Source::Rule("condition-predicate-is-boolean"),
            ..refused.clone()
        };
        assert_eq!(settle_by_values_that_differ(&case, &rule), None);
        // A refusal that is not inside a predicate.
        let outside = Refused {
            place: Place::of_step(0, StepId::new(STEP).ok()),
            ..refused.clone()
        };
        assert_eq!(settle_by_values_that_differ(&case, &outside), None);
        // A condition with no `otherwise` has no value to make differ.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/allowed_fuzz/regressions/stricter-a-predicate-that-is-dropped.case");
        let text = std::fs::read_to_string(path).expect("a kept case");
        let start = text
            .find(r#",{"entry":"ChainedThen.otherwise""#)
            .expect("an otherwise");
        let end = text[start..].find("}}}}").expect("its end") + start + "}}}}".len();
        let without = format!("{}{}", &text[..start], &text[end..]);
        let case = Case::decode(without.as_bytes()).expect("a case without an otherwise");
        assert_eq!(settle_by_values_that_differ(&case, &refused), None);
    }

    fn col_json(name: &str) -> String {
        format!(
            r#"{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["{name}"]}}}}}},"then":[]}}}}"#
        )
    }

    /// `sort(by=[when(predicate).then(value) links])` on a `String` and a `List(Int64)`, the
    /// condition being the start of the key. `links` follow it, each written as a call without
    /// arguments, and `otherwise` ends the condition when there is one.
    fn sorted_by_a_condition(
        predicate: &str,
        value: &str,
        otherwise: Option<&str>,
        links: &[&str],
    ) -> Case {
        let mut calls = vec![format!(
            r#"{{"entry":"When.then","arguments":{{"statement":{value}}}}}"#
        )];
        if let Some(otherwise) = otherwise {
            calls.push(format!(
                r#"{{"entry":"Then.otherwise","arguments":{{"statement":{otherwise}}}}}"#
            ));
        }
        calls.extend(
            links
                .iter()
                .map(|id| format!(r#"{{"entry":"{id}","arguments":{{}}}}"#)),
        );
        let text = format!(
            r#"{{"a":"String","b":"List(Int64)","revision":1}}
{{"version":1,"steps":[{{"id":"s","call":{{"entry":"LazyFrame.sort","arguments":{{"by":{{"many":[{{"expression":{{"start":{{"entry":"polars.when","arguments":{{"predicates":{{"many":[{predicate}]}}}}}},"then":[{}]}}}}]}}}}}}}}]}}"#,
            calls.join(",")
        );
        Case::decode(text.as_bytes()).expect("a case")
    }

    /// The tables' refusal of the predicate of the condition that `sorted_by_a_condition` makes.
    fn refused_as_a_predicate_that_is_no_boolean() -> Refused {
        let place = Place::of_step(0, StepId::new(STEP).ok())
            .then(Segment::Parameter("by".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Start)
            .then(Segment::Parameter("predicates".to_owned()))
            .then(Segment::Item(0));
        Refused {
            source: Source::Rule(CONDITION_RULE),
            at: place.to_string(),
            place,
            stage: None,
            reason: "it cannot end here: a condition's predicate must be Boolean".to_owned(),
            argument_unknown: false,
        }
    }

    /// A predicate that is no `Boolean` on its own: the text `a` compared with a number, which
    /// Polars refuses when it evaluates the comparison.
    fn a_comparison_of_text_with_a_number() -> String {
        r#"{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[{"entry":"Expr.ne","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"float":1.5}}},"then":[]}}}}]}}"#
            .to_owned()
    }

    #[test]
    fn a_non_boolean_predicate_is_refused_where_its_condition_stands_alone_and_not_under_a_fold() {
        // The sort key is one value, which Polars drops. The predicate, a comparison of text with
        // a number, is refused where it is evaluated, and the dtype of the predicate is not one
        // Polars gives, so no column of it can be asked.
        for name in [
            "stricter-a-non-boolean-predicate-in-a-sort-key-of-one-value.case",
            "stricter-a-non-boolean-predicate-in-a-sort-key-of-a-max.case",
            "stricter-a-non-boolean-predicate-under-a-fold-by-or-true.case",
        ] {
            let case = kept(name);
            let found = settle_by_evaluating_the_condition(&case, &refusal(&case))
                .unwrap_or_else(|| panic!("{name} is refused where the condition stands alone"));
            assert_eq!(
                found.showing,
                Showing::WhereTheConditionIsEvaluated,
                "{name}"
            );
            assert!(found.probe.contains("LazyFrame.select"), "{}", found.probe);
            for folded in ["Expr.has_nulls", "Expr.max", "Expr.or_"] {
                assert!(!found.probe.contains(folded), "{name}: {}", found.probe);
            }
        }
    }

    #[test]
    fn a_predicate_is_not_settled_where_the_condition_stands_alone_by_another_predicates_refusal() {
        // `select(when(a.dt.replace(day=1).is_null()).then(a).when(a.dt.replace(day=g).is_null())
        // .then(a).otherwise(a))`: the rule is asked about the first predicate, and Polars refuses
        // the second, whose day is text, for the rows, which has no place. With the first made
        // `true` Polars takes its branch and never evaluates the second, so the control is allowed
        // for the fold and not for the first predicate.
        let predicate = |day: String| {
            format!(
                r#"{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["a"]}}}}}},"then":[{{"entry":"Expr.dt.replace","arguments":{{"day":{day}}}}},{{"entry":"Expr.is_null","arguments":{{}}}}]}}}}"#
            )
        };
        let one = r#"{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":1}}},"then":[]}}"#;
        let value = |entry: &str| {
            format!(
                r#"{{"entry":"{entry}","arguments":{{"statement":{}}}}}"#,
                col_json("a")
            )
        };
        let text = format!(
            r#"{{"a":"Date","b":"Date","revision":1}}
{{"version":1,"steps":[{{"id":"s","call":{{"entry":"LazyFrame.select","arguments":{{"exprs":{{"many":[{{"expression":{{"start":{{"entry":"polars.when","arguments":{{"predicates":{{"many":[{}]}}}}}},"then":[{},{{"entry":"Then.when","arguments":{{"predicates":{{"many":[{}]}}}}}},{},{}]}}}}]}}}}}}}}]}}"#,
            predicate(one.to_owned()),
            value("When.then"),
            predicate(col_json("g")),
            value("ChainedWhen.then"),
            value("ChainedThen.otherwise"),
        );
        let case = Case::decode(text.as_bytes()).expect("a case");
        let place = Place::of_step(0, StepId::new(STEP).ok())
            .then(Segment::Parameter("exprs".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Start)
            .then(Segment::Parameter("predicates".to_owned()))
            .then(Segment::Item(0));
        let refused = Refused {
            place: place.clone(),
            at: place.to_string(),
            ..refused_as_a_predicate_that_is_no_boolean()
        };
        assert_eq!(settle_by_evaluating_the_condition(&case, &refused), None);
    }

    #[test]
    fn a_predicate_is_not_settled_where_the_value_it_guards_is_refused_only_when_the_rows_run() {
        // `sort(by=when(f).then(g.cast(Int8)).otherwise(k.cast(Int8)))`: the rule is asked about
        // the predicate, and Polars refuses the condition alone for the value the predicate
        // guards, a text that is no number, when the rows run, with a refusal that has no place.
        // Made `false` the predicate drops that value with its branch, so the control is allowed,
        // and the case would be settled as the predicate's fault if the dropping were not seen.
        let to_a_whole_number = |name: &str| {
            format!(
                r#"{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["{name}"]}}}}}},"then":[{{"entry":"Expr.cast","arguments":{{"dtype":{{"dtype":"Int8"}}}}}}]}}}}"#
            )
        };
        let case = sorted_by_a_condition(
            &col_json("f"),
            &to_a_whole_number("g"),
            Some(&to_a_whole_number("k")),
            &[],
        );
        let settled =
            settle_by_evaluating_the_condition(&case, &refused_as_a_predicate_that_is_no_boolean());
        assert_eq!(settled, None);
    }

    #[test]
    fn a_non_boolean_predicate_in_a_pivot_aggregate_is_asked_in_the_pivot() {
        // There is no column to stand in for the predicate, so the condition stands alone as the
        // aggregate of the same pivot.
        let case = kept("stricter-a-non-boolean-predicate-folded-in-a-pivot-aggregate.case");
        let found = settle_by_evaluating_the_condition(&case, &refusal(&case))
            .expect("refused where the condition stands alone");
        assert_eq!(found.showing, Showing::WhereTheConditionIsEvaluated);
        assert!(found.probe.contains("LazyFrame.pivot"), "{}", found.probe);
        assert!(!found.probe.contains("Expr.or_"), "{}", found.probe);
        assert!(!found.probe.contains("Expr.has_nulls"), "{}", found.probe);
        // Asked of the whole case, as the target asks it.
        assert!(
            settle_apart(&case, &refusal(&case), &predict(&case), &observe(&case))
                .is_some_and(|found| found.showing == Showing::WhereTheConditionIsEvaluated)
        );
    }

    #[test]
    fn the_values_of_the_condition_that_stands_alone_are_made_to_differ() {
        // Both values are the column `b`, which Polars takes for one expression and drops the
        // predicate of, so the last value is a fill forward of it in the probe.
        let case = sorted_by_a_condition(
            &a_comparison_of_text_with_a_number(),
            &col_json("b"),
            Some(&col_json("b")),
            &["Expr.has_nulls"],
        );
        let found =
            settle_by_evaluating_the_condition(&case, &refused_as_a_predicate_that_is_no_boolean())
                .expect("refused where the condition stands alone");
        assert!(found.probe.contains("Expr.forward_fill"), "{}", found.probe);
    }

    #[test]
    fn a_predicate_that_polars_accepts_where_the_condition_stands_alone_settles_nothing() {
        // `f` is a `Boolean`, so the condition alone is allowed: whatever refused the case, it was
        // not a predicate that Polars refuses once it evaluates it.
        let case = sorted_by_a_condition(&col_json("f"), &col_json("b"), None, &["Expr.has_nulls"]);
        assert_eq!(
            settle_by_evaluating_the_condition(&case, &refused_as_a_predicate_that_is_no_boolean()),
            None
        );
    }

    #[test]
    fn a_condition_that_is_refused_for_what_is_not_its_predicate_settles_nothing() {
        // The value is refused whatever the predicate is: the same condition with the predicate
        // made `false` is refused too, so the refusal is no evidence about the predicate.
        let dt_date_of_text = r#"{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[{"entry":"Expr.cum_sum","arguments":{}}]}}"#;
        let case = sorted_by_a_condition(
            &a_comparison_of_text_with_a_number(),
            dt_date_of_text,
            None,
            &["Expr.has_nulls"],
        );
        assert!(
            (0..RUNS).any(|_| observe(&case).refusal.is_none()),
            "Polars drops the sort key, so allows the case"
        );
        let evaluated = sorted_by_a_condition(
            &a_comparison_of_text_with_a_number(),
            dt_date_of_text,
            None,
            &[],
        );
        assert!(
            (0..RUNS).any(|_| observe(&evaluated).refusal.is_some()),
            "Polars refuses the condition where it evaluates it"
        );
        assert_eq!(
            settle_by_evaluating_the_condition(&case, &refused_as_a_predicate_that_is_no_boolean()),
            None
        );
    }

    #[test]
    fn only_the_rule_about_a_predicate_of_a_condition_is_asked_where_the_condition_stands_alone() {
        let case = kept("stricter-a-non-boolean-predicate-in-a-sort-key-of-one-value.case");
        let refused = refusal(&case);
        for other in [
            Source::Polars,
            Source::Rule("filter-predicate-is-boolean"),
            Source::Rule("key-is-elementwise"),
        ] {
            let other = Refused {
                source: other,
                ..refused.clone()
            };
            assert_eq!(settle_by_evaluating_the_condition(&case, &other), None);
        }
        // A refusal that is not of the predicate itself, but of a call inside it.
        let inside = Refused {
            place: refused.place.clone().then(Segment::Link(0)),
            ..refused.clone()
        };
        assert_eq!(settle_by_evaluating_the_condition(&case, &inside), None);
        // And one that is of the step as a whole.
        let whole = Refused {
            place: Place::of_step(0, StepId::new(STEP).ok()),
            ..refused
        };
        assert_eq!(settle_by_evaluating_the_condition(&case, &whole), None);
    }

    #[test]
    fn a_condition_with_no_value_cannot_stand_alone() {
        // `when(p)` of a sort key alone is no expression Polars can run.
        let text = format!(
            r#"{{"a":"String","b":"List(Int64)","revision":1}}
{{"version":1,"steps":[{{"id":"s","call":{{"entry":"LazyFrame.sort","arguments":{{"by":{{"many":[{{"expression":{{"start":{{"entry":"polars.when","arguments":{{"predicates":{{"many":[{}]}}}}}},"then":[]}}}}]}}}}}}}}]}}"#,
            a_comparison_of_text_with_a_number()
        );
        if let Ok(case) = Case::decode(text.as_bytes()) {
            assert_eq!(
                settle_by_evaluating_the_condition(
                    &case,
                    &refused_as_a_predicate_that_is_no_boolean()
                ),
                None
            );
        }
    }

    /// `filter(b.drop_nulls().diff(null_behavior="drop") < 1)`: the count is one value on the
    /// sample's three rows, which a filter's predicate takes, and Polars refuses nothing of it
    /// alone.
    const A_COUNT_OF_ONE_BY_CHANCE: &str = r#"{"a":"Date","b":"Int8","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.filter","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[{"entry":"Expr.drop_nulls","arguments":{}},{"entry":"Expr.diff","arguments":{"null_behavior":{"choice":"drop"}}},{"entry":"Expr.lt","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":1}}},"then":[]}}}}]}}]}}}}]}"#;

    #[test]
    fn a_count_the_rule_refuses_is_settled_by_polars_refusing_the_part_for_its_lengths() {
        // `sort(by=k.truediv(b.diff(drop)).entropy())`: the rule refuses a division of the rows by
        // a count of two, and Polars drops a sort key that is one value, as the entropy is; asked
        // as written, it refuses the division for the lengths.
        let case = kept("stricter-a-count-met-by-an-operand-in-a-sort-key-of-one-value.case");
        let refused = refusal(&case);
        assert!(matches!(
            refused.source,
            Source::Rule("values-meet-in-count")
        ));
        let found = settle(&case, &refused).expect("a division that Polars refuses alone");
        assert!(found.probe.contains("Expr.truediv"), "{}", found.probe);
        assert_eq!(found.showing, Showing::TheSameReason);
    }

    #[test]
    fn a_count_meeting_a_folded_null_is_settled_by_polars_refusing_the_comparison_alone() {
        // `sort(by=when(null.diff(drop) < null.fill_nan(b)).then(h).nan_max())`: Polars drops the
        // key, for the maximum is one value, and refuses the comparison on its own for the lengths.
        let case = kept("stricter-a-count-met-by-a-folded-null-in-a-sort-key-of-one-value.case");
        let refused = refusal(&case);
        assert!(matches!(
            refused.source,
            Source::Rule("values-meet-in-count")
        ));
        let found = settle(&case, &refused).expect("a comparison that Polars refuses alone");
        assert!(found.probe.contains("Expr.lt"), "{}", found.probe);
    }

    #[test]
    fn a_count_given_to_a_condition_is_settled_by_polars_refusing_the_condition_alone() {
        // `sort(by=when(f).then(k.diff(drop)).arg_max())`: the tables refuse the count where a
        // condition's value ends a sort key, in Polars' words for the lengths, and Polars drops the
        // key. The value is refused beside its predicate, in the condition that holds it.
        let case = kept("stricter-a-count-given-to-a-condition-in-a-sort-key-of-one-value.case");
        let refused = refusal(&case);
        assert!(matches!(refused.source, Source::Polars));
        let found = settle(&case, &refused).expect("a condition that Polars refuses alone");
        assert!(found.probe.contains("polars.when"), "{}", found.probe);
    }

    #[test]
    fn a_window_over_a_count_is_settled_by_polars_refusing_the_window_alone() {
        let case = kept("stricter-a-window-over-a-count-in-a-sort-key-of-one-value.case");
        let refused = refusal(&case);
        assert!(matches!(
            refused.source,
            Source::Rule("window-input-is-not-counted")
        ));
        let found = settle(&case, &refused).expect("a window that Polars refuses alone");
        assert!(found.probe.contains("Expr.over"), "{}", found.probe);
    }

    #[test]
    fn a_count_that_polars_allows_alone_is_not_settled_by_the_part() {
        // The count is one value where the rows are three, and Polars allows the part alone: it is
        // the position, a filter's predicate, that the tables refuse a count in, and which takes
        // a count that is one by chance. The probe shows nothing.
        let case = Case::decode(A_COUNT_OF_ONE_BY_CHANCE.as_bytes()).expect("a case");
        let refused = refusal(&case);
        assert!(matches!(refused.source, Source::Polars));
        assert_eq!(settle(&case, &refused), None);
    }

    #[test]
    fn a_refusal_that_is_not_about_lengths_settles_no_count() {
        let counted = "a count of values that the data or the arguments decide, which is neither \
            one value nor the table's rows, may not end an expression in sort: lengths don't match: \
            zip node received non-equal length inputs";
        let rule = Source::Rule("values-meet-in-count");
        let window = Source::Rule("window-input-is-not-counted");
        for (source, tables) in [
            (&rule, "the values a call meets must be one value"),
            (&window, "a window takes a value as long as its group"),
            (&Source::Polars, counted),
        ] {
            assert!(same_reason(
                source,
                tables,
                "lengths don't match: zip node received non-equal length inputs"
            ));
            assert!(same_reason(
                source,
                tables,
                "cannot evaluate two Series of different lengths (0 and 3)"
            ));
            assert!(!same_reason(
                source,
                tables,
                "cannot cast List type (inner: 'String', to: 'Int8')"
            ));
            assert!(!same_reason(
                source,
                tables,
                "dtype `null` has no time unit"
            ));
        }
    }

    #[test]
    fn a_count_as_the_predicate_of_a_condition_is_settled_where_the_condition_is_evaluated() {
        // `sort(by=when(k.diff(drop) > 1).then(u).otherwise(u).n_unique())`: the predicate is two
        // values beside values of three, which Polars never evaluates as the key is one value and
        // the values of the condition are one expression. Made to differ, the condition alone is
        // refused for the lengths in its predicate, and allowed with the predicate made false.
        let case = kept(
            "stricter-a-count-as-the-predicate-of-a-condition-in-a-sort-key-of-one-value.case",
        );
        let refused = refusal(&case);
        assert!(matches!(refused.source, Source::Polars));
        let found = settle_by_evaluating_the_condition(&case, &refused)
            .expect("a condition that Polars refuses once it is evaluated");
        assert_eq!(found.showing, Showing::WhereTheConditionIsEvaluated);
        assert!(is_a_refusal_of_lengths(&found.reason), "{}", found.reason);
    }

    #[test]
    fn a_count_given_to_a_value_of_a_condition_is_no_predicate_to_evaluate() {
        let case = kept("stricter-a-count-given-to-a-condition-in-a-sort-key-of-one-value.case");
        let refused = refusal(&case);
        assert_eq!(settle_by_evaluating_the_condition(&case, &refused), None);
    }
}
