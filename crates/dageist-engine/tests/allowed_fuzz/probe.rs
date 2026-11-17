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

use dageist_engine::allowed::Representative;
use dageist_engine::catalog;
use dageist_engine::document::{
    Call, Document, Expression, Place, Segment, Start, Step, StepId, Value,
};

use super::case::{Case, STEP};
use super::frames::STAND_IN;
use super::observe::{Observed, Refusal, Seen, Tables, observe_probe};
use super::predict::{Prediction, Refused, Source};

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
        Source::Polars => !objection(tables).is_empty() && objection(tables) == objection(probe),
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
    let refusal = observe_probe(&probed, tables_of(&found, None)).refusal?;
    if !same_reason(&refused.source, &refused.reason, &refusal.reason)
        || !lies_at(&refusal, &probe.at)
    {
        return None;
    }
    if let Some(before) = probe.before {
        let alone = run(before, false)?;
        let earlier = observe_probe(&alone, tables_of(&found, None)).refusal;
        if earlier
            .is_some_and(|earlier| same_reason(&refused.source, &refused.reason, &earlier.reason))
        {
            return None;
        }
    }
    Some(Settled {
        probe: probed.document.write(),
        reason: refusal.reason,
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
    let refusal = observe_probe(&probed, tables_of(&found, Some(class))).refusal?;
    let at = place_of_the_part(&found, false).then(Segment::Link(0));
    (same_reason(&refused.source, &refused.reason, &refusal.reason) && lies_at(&refusal, &[at]))
        .then(|| Settled {
            probe: probed.document.write(),
            reason: refusal.reason,
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
    fn the_catalogue_has_what_a_probe_is_made_of() {
        for id in ["LazyFrame.select", "LazyFrame.pivot", "LazyFrame.filter"] {
            assert!(catalog::entry(id).is_some(), "{id}");
        }
    }
}
