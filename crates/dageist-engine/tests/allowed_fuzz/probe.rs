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
use super::observe::observe_probe;
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
    let mut named = false;
    let mut previous = ' ';
    for character in reason.chars() {
        let opens_a_name = character == '\'' && !previous.is_ascii_alphabetic();
        previous = character;
        match character {
            '\'' if opens_a_name || named => named = !named,
            '`' => quoted = !quoted,
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if quoted || named || depth > 0 => {}
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
    let reason = observe_probe(&probe, found.on_the_other_table)
        .refusal?
        .reason;
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
        let reason = observe_probe(&first, false)
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

    #[test]
    fn the_catalogue_has_what_a_probe_is_made_of() {
        for id in ["LazyFrame.select", "LazyFrame.pivot", "LazyFrame.filter"] {
            assert!(catalog::entry(id).is_some(), "{id}");
        }
    }
}
