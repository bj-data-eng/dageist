//! The probe of a part the tables refuse and Polars allows.
//!
//! The editor holds every part of an expression to its types, whether or not
//! Polars evaluates it. Polars skips parts: it folds a comparison with a null,
//! decides a condition by a constant, drops a sort key that is one value, runs a
//! window's keys on groups. Where the tables refuse a part and Polars allows the
//! step, the target does not model what Polars skips. It asks Polars about the
//! part alone, in a document where the part must be evaluated: the chain through
//! a refused link selected on its own, a condition with a refused predicate and
//! literal values selected on its own, either in the same input and, for a part
//! of a pivot's aggregate, in the same pivot. When Polars refuses that for the
//! reason the tables gave, the tables are stricter by design where Polars does not
//! evaluate the part; when it allows it, or refuses it for another reason, the
//! disagreement is a finding.

use dageist_engine::catalog;
use dageist_engine::document::{
    Call, Document, Expression, Place, Segment, Start, Step, StepId, Value,
};

use super::case::{Case, STEP};
use super::observe::observe;
use super::predict::{Refused, Source};

/// What a probe found: the document it ran, and why Polars refused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settled {
    pub probe: String,
    pub reason: String,
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
    /// Whether the place is in a pivot's aggregate, where `element` stands.
    in_the_aggregate: bool,
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
    for (at, segment) in path.iter().enumerate() {
        node = match (node, segment) {
            (Node::Call(call), Segment::Parameter(name)) => enter(call.argument(name)?),
            (Node::Value(Value::Many(items)), Segment::Item(item)) => enter(items.get(*item)?),
            (Node::Value(Value::Pairs(pairs)), Segment::Key(item)) => enter(&pairs.get(*item)?.0),
            (Node::Value(Value::Pairs(pairs)), Segment::Value(item)) => enter(&pairs.get(*item)?.1),
            (Node::Expression(expression), Segment::Start) => match expression.start() {
                Start::Call(call) => Node::Call(call),
                Start::Input(_) => return None,
            },
            (Node::Expression(expression), Segment::Link(link)) => {
                Node::Call(expression.links().get(*link)?)
            }
            _ => return None,
        };
        if let Node::Expression(expression) = &node {
            held = Some((expression, at + 1));
        }
    }
    let (expression, consumed) = held?;
    Some(Found {
        expression,
        rest: &path[consumed..],
        in_the_aggregate: matches!(
            path.first(),
            Some(Segment::Parameter(name)) if name == "aggregate_function"
        ) && step.entry().id().as_str() == "LazyFrame.pivot",
    })
}

fn call(id: &str, given: Vec<(&str, Value)>) -> Option<Call> {
    Call::new(catalog::entry(id)?, given).ok()
}

fn literal(whole: i64) -> Option<Expression> {
    let lit = call("polars.lit", vec![("value", Value::Integer(whole))])?;
    Expression::new(Start::Call(lit), Vec::new()).ok()
}

/// The chain through the refused call: the start, or the links up to the one.
fn chain_through(found: &Found) -> Option<Expression> {
    let expression = found.expression;
    match found.rest.first() {
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

/// The words of a reason that say what Polars objected to: what is quoted, in
/// parentheses or a dtype is the part's, not the objection's.
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
    let mut kept = String::new();
    let mut depth = 0_u32;
    let mut quoted = false;
    for character in reason.chars() {
        match character {
            '`' => quoted = !quoted,
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if quoted || depth > 0 => {}
            _ => kept.push(character),
        }
    }
    kept.to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphabetic())
        .filter(|word| word.len() > 1 && !DTYPES.contains(word))
        .take(8)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether `probe` is refused as the tables refused the part: for Polars' own
/// reason the same objection, and for a rule that a predicate be a `Boolean`
/// Polars' words for it.
fn same_reason(source: &Source, tables: &str, probe: &str) -> bool {
    match source {
        Source::Polars => !objection(tables).is_empty() && objection(tables) == objection(probe),
        Source::Rule(rule) if PREDICATE_RULES.contains(rule) => {
            let probe = probe.to_ascii_lowercase();
            probe.contains("boolean") && (probe.contains("expected") || probe.contains("must be"))
        }
        _ => false,
    }
}

/// Asks Polars about the part the tables refused at `refused`, alone. `None`
/// when there is nothing to ask, Polars allows the probe, or it refuses it for
/// another reason than the tables did.
pub fn settle(case: &Case, refused: &Refused) -> Option<Settled> {
    let found = find(case, &refused.place)?;
    let (part, filter) = match &refused.source {
        Source::Polars => (chain_through(&found)?, false),
        Source::Rule("condition-predicate-is-boolean") => {
            (condition_of(found_predicate(&found)?)?, false)
        }
        Source::Rule("filter-predicate-is-boolean") => (found_predicate(&found)?, true),
        _ => return None,
    };
    let document = document_of(case, &found, part, filter)?;
    let probe = Case {
        receiver: case.receiver,
        argument: case.argument,
        document,
    };
    let reason = observe(&probe).refusal?.reason;
    same_reason(&refused.source, &refused.reason, &reason).then(|| Settled {
        probe: probe.document.write(),
        reason,
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

    /// A case that selects the one chain `a.cum_sum()` on columns of `receiver`.
    fn cum_sum_of(receiver: &str) -> Case {
        let text = format!(
            "{{\"a\":\"{receiver}\",\"b\":\"Int64\",\"revision\":1}}\n{{\"version\":1,\"steps\":[{{\"id\":\"s\",\"call\":{{\"entry\":\"LazyFrame.select\",\"arguments\":{{\"exprs\":{{\"many\":[{{\"expression\":{{\"start\":{{\"entry\":\"polars.col\",\"arguments\":{{\"name\":{{\"columns\":[\"a\"]}}}}}},\"then\":[{{\"entry\":\"Expr.cum_sum\",\"arguments\":{{}}}}]}}}}]}}}}}}}}]}}"
        );
        Case::decode(text.as_bytes()).expect("a case")
    }

    fn refused_at_the_link(reason: &str) -> Refused {
        let place = Place::of_step(0, Some(StepId::new("s").expect("an identifier")))
            .then(Segment::Parameter("exprs".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Link(0));
        Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage: None,
            reason: reason.to_owned(),
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
    fn the_catalogue_has_what_a_probe_is_made_of() {
        for id in ["LazyFrame.select", "LazyFrame.pivot", "LazyFrame.filter"] {
            assert!(catalog::entry(id).is_some(), "{id}");
        }
    }
}
