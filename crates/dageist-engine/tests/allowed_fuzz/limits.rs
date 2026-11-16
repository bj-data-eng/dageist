//! What the tables say they do not cover, which the target does not hold them
//! to.
//!
//! The tables are measured one combination at a time, on columns of three rows
//! with a null, by running each in `select` and in the position that holds it
//! (docs/explanation.md, "Which Combinations Polars Allows", says what they
//! hold). A disagreement that comes from something they were not measured on
//! is not a defect of them. Each limit below names what it is, says which way
//! it excuses a disagreement, and is counted when it does, so that a limit that
//! excuses everything shows. A disagreement no limit explains is a finding.
//!
//! A limit excuses a disagreement only when it is the cause of it: the place
//! where the tables refused, or where Polars did, lies inside the expression
//! that holds what the limit is about, and the reason says what the limit says.
//! A feature elsewhere in the case excuses nothing. Where Polars refused when it
//! collected rows there is no place, and the reason and the feature in the case
//! are what there is. Whether a limit applies does not depend on the order the
//! limits are asked in, and a case that two apply to is excused by both.

use dageist_engine::document::{Call, Expression, Place, Segment, Start, Value};

use super::case::Case;
use super::observe::{Observed, Refusal, Stage};
use super::predict::{Prediction, Refused, Source, Verdict};

/// A limit of the tables, and what it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// An option changes what the method gives: `Expr.over`'s `mapping_strategy`
    /// `explode` gives a different number of rows, which meets another
    /// expression's with lengths that do not match, and `join` gives a `List`,
    /// and `Expr.dt.total_days`'s `fractional` gives a `Float64`. `options.txt`
    /// records the dtypes, but [`allowed::offer`](dageist_engine::allowed::offer)
    /// is asked of a method without its options, so none of it is applied.
    OptionsChangeTheResult,
    /// A literal is one value, which Polars broadcasts and casts as it does not
    /// a column: it accepts a text compared with a typed number, or a window
    /// over a constant, where a column of the dtype is refused, and joins it
    /// with a column of other length. The tables measure an argument as a
    /// column of the dtype, except the values of `shift` and a pivot's group.
    ALiteral,
    /// An expression with an aggregate in it gives one value. Polars does
    /// not evaluate what it does not need for one: a sort key that is one value
    /// is dropped, and `len` does not read what it counts. What the tables say
    /// of a method's dtype was measured in `select` on a column.
    AnAggregateGivesOneValue,
    /// A pivot's aggregate is run on the values of each group, not on a
    /// column in `select`, and Polars uses other kernels for them: it takes the
    /// `max` of a `Struct` there, which `select` refuses, and compares, casts
    /// and windows what `select` does not. A table with no rows has no groups,
    /// and the values of an aggregate over none have no dtype.
    AGroupAggregate,
    /// A single value in a key, a join's or a window's, beside a column is not
    /// elementwise, and Polars refuses it: an aggregate, or a null literal. The
    /// context table measured the entry as the whole key.
    ASingleValueInAKey,
    /// Whether values of different lengths combine, as the lists of two rows
    /// do not, depends on the rows, which the tables measured on one sample.
    TheLengthsOfTheRows,
    /// A debug build of Polars panics where an arithmetic overflow wraps in a
    /// release build, on keys the tables did not measure.
    AnOverflow,
    /// A condition whose values are one expression is simplified to it, and
    /// its predicates are dropped, so what Polars refuses in them it does not.
    TheBranchesAreOne,
    /// A panic of Polars is an internal check that failed. The tables record
    /// one for the plan Polars made of a combination in `select`, and the plan
    /// of another place may not reach it; a combination of what they measured
    /// singly, such as two keys of a window, may reach another.
    APanic,
    /// A place more than two positions deep is judged by its first two, so a
    /// predicate inside a value of a condition is judged as a value, which need
    /// not be a `Boolean`.
    ADeeperPlace,
    /// A condition with a predicate that is a literal is decided before it is
    /// run: Polars takes the branch the literal selects and drops the others, so
    /// what the predicates and values it drops would refuse, it does not.
    ADecidedCondition,
    /// A value of a condition runs on the rows its predicate selects, and a
    /// predicate after the first on the rows left to it. On the rows of the
    /// sample that may be none, and what would be refused is not raised.
    AValueThatRunsOnNoRows,
    /// The other table's key is made of the dtype its chain gives on its own,
    /// and the join may resolve the chain to another, as the precision of a
    /// decimal that a null is taken from.
    AJoinKeyDtype,
    /// The tables hold the schema of a method and what its rows give as one
    /// thing, and refuse a method whose plan says another dtype than its rows
    /// give. Inside an expression that goes on from it or compares it, the rows
    /// of the step do not show it.
    AHiddenSchema,
    /// A window over a window is run by Polars in the aggregation context of the
    /// outer one, where a key that is not elementwise is refused; the tables
    /// measured one window.
    AWindowOverAWindow,
    /// The tables are asked what a chain gives so that a place that needs a
    /// `Boolean` can be checked, and a start, or a method that gives what its
    /// arguments decide, gives nothing to check: the dtype of a column is the
    /// caller's to say, and [`offer`](dageist_engine::allowed::offer) is not told it.
    ABooleanIsNeededAndTheDtypeUnknown,
}

impl Limit {
    /// Every limit, which the kept cases cover each by one.
    pub const ALL: [Self; 16] = [
        Self::OptionsChangeTheResult,
        Self::ALiteral,
        Self::AnAggregateGivesOneValue,
        Self::AGroupAggregate,
        Self::ASingleValueInAKey,
        Self::TheLengthsOfTheRows,
        Self::AnOverflow,
        Self::TheBranchesAreOne,
        Self::APanic,
        Self::ABooleanIsNeededAndTheDtypeUnknown,
        Self::ADeeperPlace,
        Self::AHiddenSchema,
        Self::AWindowOverAWindow,
        Self::ADecidedCondition,
        Self::AValueThatRunsOnNoRows,
        Self::AJoinKeyDtype,
    ];

    /// The name a kept case is filed under.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::OptionsChangeTheResult => "options-change-the-result",
            Self::ALiteral => "a-literal",
            Self::AnAggregateGivesOneValue => "an-aggregate-gives-one-value",
            Self::AGroupAggregate => "a-group-aggregate",
            Self::ASingleValueInAKey => "a-single-value-in-a-key",
            Self::TheLengthsOfTheRows => "the-lengths-of-the-rows",
            Self::AnOverflow => "an-overflow",
            Self::TheBranchesAreOne => "the-branches-are-one",
            Self::APanic => "a-panic",
            Self::ABooleanIsNeededAndTheDtypeUnknown => "a-boolean-is-needed",
            Self::ADeeperPlace => "a-deeper-place",
            Self::AHiddenSchema => "a-hidden-schema",
            Self::AWindowOverAWindow => "a-window-over-a-window",
            Self::ADecidedCondition => "a-decided-condition",
            Self::AJoinKeyDtype => "a-join-key-dtype",
            Self::AValueThatRunsOnNoRows => "a-value-that-runs-on-no-rows",
        }
    }

    pub const fn statement(self) -> &'static str {
        match self {
            Self::OptionsChangeTheResult => {
                "an option changes what a method gives (over's mapping_strategy, total_days' fractional), which offer is not told"
            }
            Self::ALiteral => "a literal is one value, which the tables measured as a column",
            Self::TheLengthsOfTheRows => {
                "whether values of different lengths combine depends on the rows, which the tables measured on one sample"
            }
            Self::TheBranchesAreOne => {
                "a condition whose values are one expression is that expression, without its predicates"
            }
            Self::AnOverflow => {
                "a debug build of Polars panics on an overflow of the values, which the tables measured on one sample"
            }
            Self::APanic => {
                "a panic of Polars depends on the plan it makes, which another place may not make"
            }
            Self::AnAggregateGivesOneValue => {
                "an expression with an aggregate in it gives one value, which Polars need not evaluate"
            }
            Self::AGroupAggregate => {
                "a pivot's aggregate is run on groups, with kernels of its own, not on a column in select"
            }
            Self::ASingleValueInAKey => {
                "a single value, an aggregate or a null, in a key beside a column is not elementwise"
            }
            Self::AJoinKeyDtype => {
                "the join resolves a key to a dtype other than the one its chain gives on its own"
            }
            Self::AValueThatRunsOnNoRows => {
                "a value of a condition, or a predicate after the first, runs on the rows left to it, which may be none"
            }
            Self::ADecidedCondition => {
                "a condition with a literal predicate is decided before it is run, and drops the other branches"
            }
            Self::AHiddenSchema => {
                "a method whose plan and rows differ in dtype is hidden by what goes on from it"
            }
            Self::AWindowOverAWindow => {
                "a window over a window is run in an aggregation context, where a key that is not elementwise is refused"
            }
            Self::ADeeperPlace => {
                "a place more than two positions deep is judged by its first two, as the tables hold them"
            }
            Self::ABooleanIsNeededAndTheDtypeUnknown => {
                "the dtype a column or an argument decides is not asked, where a Boolean is needed"
            }
        }
    }
}

/// Whether the limits excuse nothing: set by `DAGEIST_FUZZ_STRICT=1`.
pub fn strict() -> bool {
    std::env::var_os("DAGEIST_FUZZ_STRICT").is_some_and(|value| value == "1")
}

fn is(call: &Call, id: &str) -> bool {
    call.entry().id().as_str() == id
}

const ONE_VALUE: [&str; 6] = [
    "Expr.sum",
    "Expr.min",
    "Expr.max",
    "Expr.len",
    "Expr.first",
    "Expr.last",
];

fn is_one_value(call: &Call) -> bool {
    ONE_VALUE.contains(&call.entry().id().as_str())
}

/// The expressions a call is given, by parameter.
fn operands(call: &Call) -> Vec<(&str, &Expression)> {
    fn collect<'a>(
        parameter: &'a str,
        value: &'a Value,
        found: &mut Vec<(&'a str, &'a Expression)>,
    ) {
        match value {
            Value::Expression(expression) => found.push((parameter, expression)),
            Value::Many(items) => items
                .iter()
                .for_each(|item| collect(parameter, item, found)),
            Value::Pairs(pairs) => pairs.iter().for_each(|(key, entry)| {
                collect(parameter, key, found);
                collect(parameter, entry, found);
            }),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for given in call.arguments() {
        collect(given.parameter(), given.value(), &mut found);
    }
    found
}

/// The calls of an expression, and of the expressions inside it, as far as
/// `found` says, which is asked of each.
fn any_call(expression: &Expression, found: &dyn Fn(&Call) -> bool) -> bool {
    let calls = std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) => None,
        })
        .chain(expression.links());
    calls.into_iter().any(|call| {
        found(call)
            || operands(call)
                .into_iter()
                .any(|(_, inside)| any_call(inside, found))
    })
}

fn is_a_single_value(call: &Call) -> bool {
    is_one_value(call)
        || (is(call, "polars.lit") && matches!(call.argument("value"), Some(Value::Flag(None))))
}

fn has_explode(call: &Call) -> bool {
    is(call, "Expr.over")
        && matches!(call.argument("mapping_strategy"), Some(Value::Choice(word)) if word == "explode")
}

/// How many calls of an expression, and of the expressions inside it, `found`
/// says yes to.
fn count_calls(expression: &Expression, found: &dyn Fn(&Call) -> bool) -> usize {
    let calls = std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) => None,
        })
        .chain(expression.links());
    calls
        .map(|call| {
            usize::from(found(call))
                + operands(call)
                    .into_iter()
                    .map(|(_, inside)| count_calls(inside, found))
                    .sum::<usize>()
        })
        .sum()
}

/// Whether `expression` starts from a literal, or from a condition whose
/// values are literals, which the tables did not measure as a start.
fn starts_from_a_literal(expression: &Expression) -> bool {
    match expression.start() {
        Start::Call(start) if is(start, "polars.lit") => true,
        Start::Call(start) if is(start, "polars.when") => {
            let values: Vec<&Expression> = expression
                .links()
                .iter()
                .filter(|link| is_a_value_link(link))
                .filter_map(|link| match link.argument("statement") {
                    Some(Value::Expression(value)) => Some(value),
                    _ => None,
                })
                .collect();
            !values.is_empty() && values.iter().all(|value| starts_from_a_literal(value))
        }
        _ => false,
    }
}

fn is_a_value_link(call: &Call) -> bool {
    matches!(
        call.entry().id().as_str(),
        "When.then" | "Then.otherwise" | "ChainedWhen.then" | "ChainedThen.otherwise"
    )
}

/// The literals the tables measured are the values `shift` takes, and, in a
/// pivot's aggregate, every argument.
fn measured_literal(owner: &Call, parameter: &str, pivot: bool) -> bool {
    pivot || (is(owner, "Expr.shift") && matches!(parameter, "n" | "fill_value"))
}

/// Whether a condition has a predicate that is made from a literal alone.
fn is_decided(expression: &Expression) -> bool {
    let Start::Call(start) = expression.start() else {
        return false;
    };
    is(start, "polars.when")
        && std::iter::once(start)
            .chain(expression.links())
            .flat_map(|call| operands(call))
            .any(|(parameter, predicate)| {
                parameter == "predicates"
                    && matches!(predicate.start(), Start::Call(literal) if is(literal, "polars.lit"))
            })
}

/// Whether the first `upto` links of `expression` give one value: an aggregate
/// among them, or a condition whose values do. A literal is its own limit.
fn one_valued(expression: &Expression, upto: usize) -> bool {
    let links = &expression.links()[..upto.min(expression.links().len())];
    if links.iter().any(is_one_value) {
        return true;
    }
    match expression.start() {
        Start::Call(start) if is(start, "polars.when") => {
            let values: Vec<&Expression> = links
                .iter()
                .filter(|link| {
                    matches!(
                        link.entry().id().as_str(),
                        "When.then"
                            | "Then.otherwise"
                            | "ChainedWhen.then"
                            | "ChainedThen.otherwise"
                    )
                })
                .filter_map(|link| match link.argument("statement") {
                    Some(Value::Expression(value)) => Some(value),
                    _ => None,
                })
                .collect();
            !values.is_empty()
                && values
                    .iter()
                    .all(|value| one_valued(value, value.links().len()))
        }
        _ => false,
    }
}

/// Where a place is in the document: the expression a position of the step
/// holds that it is inside, the chain and the call it is at, and the
/// conditions whose predicates it is inside.
struct Locus<'a> {
    /// The expression the step holds, outermost.
    top: Option<&'a Expression>,
    /// The expression that holds the call, innermost.
    chain: Option<&'a Expression>,
    /// The link the place is at, `None` for the start.
    index: Option<usize>,
    call: Option<&'a Call>,
    /// The conditions the place is inside a predicate of.
    in_predicates: Vec<&'a Expression>,
    /// The conditions the place is inside a value or a later predicate of.
    in_values: Vec<&'a Expression>,
    /// Every expression the place is inside, outermost first.
    inside: Vec<&'a Expression>,
    /// How many positions inside an expression hold the place.
    nested: usize,
}

enum Node<'a> {
    Call(&'a Call),
    Value(&'a Value),
    Expression(&'a Expression),
}

/// Follows `place` through the document.
fn locate<'a>(case: &'a Case, place: &Place) -> Locus<'a> {
    let mut locus = Locus {
        top: None,
        chain: None,
        index: None,
        call: None,
        in_predicates: Vec::new(),
        in_values: Vec::new(),
        inside: Vec::new(),
        nested: 0,
    };
    let mut node = Node::Call(case.document.steps()[0].call());
    for segment in place.path() {
        let value = match (&node, segment) {
            (Node::Call(call), Segment::Parameter(name)) => {
                if matches!(
                    call.entry().id().as_str(),
                    "polars.when" | "Then.when" | "ChainedThen.when"
                ) && name == "predicates"
                    && let Some(chain) = locus.chain
                {
                    locus.in_predicates.push(chain);
                }
                // A value, and a predicate after the first, run on the rows that
                // are left to them.
                if ((is_a_value_link(call) && name == "statement")
                    || (matches!(call.entry().id().as_str(), "Then.when" | "ChainedThen.when")
                        && name == "predicates"))
                    && let Some(chain) = locus.chain
                {
                    locus.in_values.push(chain);
                }
                if matches!(
                    name.as_str(),
                    "predicates" | "statement" | "partition_by" | "more_exprs" | "order_by"
                ) && !matches!(call.entry().id().as_str(), "LazyFrame.filter")
                    && locus.top.is_some()
                {
                    locus.nested += 1;
                }
                call.argument(name)
            }
            (Node::Value(Value::Many(items)), Segment::Item(at)) => items.get(*at),
            (Node::Value(Value::Pairs(pairs)), Segment::Key(at)) => {
                pairs.get(*at).map(|(key, _)| key)
            }
            (Node::Value(Value::Pairs(pairs)), Segment::Value(at)) => {
                pairs.get(*at).map(|(_, entry)| entry)
            }
            (Node::Expression(expression), Segment::Start) => {
                locus.chain = Some(expression);
                locus.index = None;
                node = match expression.start() {
                    Start::Call(call) => Node::Call(call),
                    Start::Input(_) => return locus,
                };
                continue;
            }
            (Node::Expression(expression), Segment::Link(at)) => {
                locus.chain = Some(expression);
                locus.index = Some(*at);
                node = match expression.links().get(*at) {
                    Some(call) => Node::Call(call),
                    None => return locus,
                };
                continue;
            }
            _ => return locus,
        };
        node = match value {
            Some(Value::Expression(expression)) => {
                locus.top.get_or_insert(expression);
                locus.inside.push(expression);
                Node::Expression(expression)
            }
            Some(other) => Node::Value(other),
            None => return locus,
        };
    }
    match node {
        Node::Call(call) => locus.call = Some(call),
        Node::Expression(expression) => locus.chain = Some(expression),
        Node::Value(_) => {}
    }
    locus
}

/// Which way a case disagrees.
enum Disagreement<'a> {
    /// The tables allow it and Polars refuses it, here.
    PolarsRefuses(&'a Refusal),
    /// The tables refuse it, here, and Polars allows it.
    PolarsAllows(&'a Refused),
}

fn is_lengths(reason: &str) -> bool {
    reason.contains("lengths don't match") || reason.contains("lengths differed")
}

fn is_panic(reason: &str) -> bool {
    reason.starts_with("Polars panicked")
}

/// Whether the tables, asked for what a place that needs a `Boolean` holds,
/// had no dtype to check.
fn boolean_dtype_unknown(case: &Case, prediction: &Prediction) -> bool {
    let filter = is(case.document.steps()[0].call(), "LazyFrame.filter");
    prediction.expressions.iter().any(|expression| {
        let needs_boolean =
            expression
                .place
                .path()
                .iter()
                .rev()
                .find_map(|segment| match segment {
                    Segment::Parameter(name) => Some(name.as_str()),
                    _ => None,
                });
        let needs_boolean = match needs_boolean {
            Some("predicates") => true,
            Some("statement") => filter,
            _ => false,
        };
        needs_boolean && expression.links.last().is_none_or(Option::is_none)
    })
}

/// Whether a key, a join's or a window's, holds a single value that is no
/// column: an aggregate, or a null literal.
fn key_holds_a_single_value(case: &Case) -> bool {
    let step = case.document.steps()[0].call();
    let in_keys = |call: &Call, parameters: &[&str]| {
        operands(call)
            .into_iter()
            .filter(|(parameter, _)| parameters.contains(parameter))
            .any(|(_, key)| any_call(key, &is_a_single_value))
    };
    if is(step, "LazyFrame.join") && in_keys(step, &["on", "left_on", "right_on"]) {
        return true;
    }
    operands(step).into_iter().any(|(_, held)| {
        any_call(held, &|call| {
            is(call, "Expr.over") && in_keys(call, &["partition_by", "more_exprs", "order_by"])
        })
    })
}

/// Whether the limit is why the tables and Polars disagree.
fn applies(
    limit: Limit,
    case: &Case,
    prediction: &Prediction,
    disagreement: &Disagreement,
) -> bool {
    let step = case.document.steps()[0].call();
    let pivot = is(step, "LazyFrame.pivot");
    let held = operands(step);
    let explodes_in = |expression: &Expression| any_call(expression, &has_explode);
    match (limit, disagreement) {
        (Limit::OptionsChangeTheResult, Disagreement::PolarsRefuses(refusal)) => {
            is_lengths(&refusal.reason) && held.iter().any(|(_, held)| explodes_in(held))
        }
        (Limit::OptionsChangeTheResult, Disagreement::PolarsAllows(refused)) => {
            locate(case, &refused.place).top.is_some_and(&explodes_in)
        }
        (Limit::TheLengthsOfTheRows, Disagreement::PolarsRefuses(refusal)) => {
            is_lengths(&refusal.reason) && !held.iter().any(|(_, held)| explodes_in(held))
        }
        (Limit::ALiteral, Disagreement::PolarsAllows(refused)) => {
            let locus = locate(case, &refused.place);
            let chain_starts = locus.chain.is_some_and(starts_from_a_literal);
            let operand = locus.call.is_some_and(|call| {
                operands(call).into_iter().any(|(parameter, operand)| {
                    !measured_literal(call, parameter, pivot) && starts_from_a_literal(operand)
                })
            });
            chain_starts || operand
        }
        (Limit::AnAggregateGivesOneValue, Disagreement::PolarsAllows(refused)) => {
            let locus = locate(case, &refused.place);
            // The chain the call is in gives one value up to it, or an
            // expression that holds it does, as a sort key that is dropped or a
            // `len` that does not read what it counts.
            let upto = locus.index.map_or(0, |at| at + 1);
            let innermost = locus
                .inside
                .last()
                .is_some_and(|chain| one_valued(chain, upto));
            let holders = locus
                .inside
                .iter()
                .enumerate()
                .filter(|(at, _)| *at == 0 || *at + 1 < locus.inside.len())
                .any(|(_, holder)| one_valued(holder, holder.links().len()));
            innermost || holders
        }
        (Limit::AGroupAggregate, Disagreement::PolarsRefuses(refusal)) => {
            pivot && refusal.stage == Stage::Empty
        }
        (Limit::AGroupAggregate, Disagreement::PolarsAllows(refused)) => {
            // The kernels that take the values of a group accept what the ones of
            // `select` refuse: a dtype of an aggregate, a comparison, a cast, or
            // a window over a group.
            pivot
                && [
                    "not supported for dtype",
                    "cannot compare",
                    "cannot perform",
                    "could not evaluate",
                    "casting from",
                    "lengths don't match",
                    "must specify one field",
                    "cannot cast",
                ]
                .iter()
                .any(|words| refused.reason.contains(words))
                && locate(case, &refused.place).call.is_some()
        }
        (Limit::ASingleValueInAKey, Disagreement::PolarsRefuses(refusal)) => {
            refusal.reason.contains("elementwise") && key_holds_a_single_value(case)
        }
        (Limit::ASingleValueInAKey, Disagreement::PolarsAllows(refused)) => {
            refused.reason.contains("elementwise") && key_holds_a_single_value(case)
        }
        (Limit::AnOverflow, Disagreement::PolarsRefuses(refusal)) => {
            refusal.reason.contains("with overflow")
        }
        (Limit::TheBranchesAreOne, Disagreement::PolarsAllows(refused)) => {
            locate(case, &refused.place)
                .in_predicates
                .iter()
                .any(|condition| {
                    let values: Vec<&Expression> = condition
                        .links()
                        .iter()
                        .filter(|link| {
                            matches!(
                                link.entry().id().as_str(),
                                "When.then"
                                    | "Then.otherwise"
                                    | "ChainedWhen.then"
                                    | "ChainedThen.otherwise"
                            )
                        })
                        .filter_map(|link| match link.argument("statement") {
                            Some(Value::Expression(value)) => Some(value),
                            _ => None,
                        })
                        .collect();
                    values.len() > 1 && values.windows(2).all(|pair| pair[0] == pair[1])
                })
        }
        (Limit::APanic, Disagreement::PolarsRefuses(refusal)) => {
            is_panic(&refusal.reason) && !refusal.reason.contains("with overflow")
        }
        (Limit::APanic, Disagreement::PolarsAllows(refused)) => is_panic(&refused.reason),
        (Limit::ADeeperPlace, Disagreement::PolarsRefuses(refusal)) => {
            refusal.reason.contains("Boolean")
                && prediction.expressions.iter().any(|expression| {
                    expression.nested >= 2
                        && matches!(
                            expression.place.path().iter().rev().find_map(
                                |segment| match segment {
                                    Segment::Parameter(name) => Some(name.as_str()),
                                    _ => None,
                                }
                            ),
                            Some("predicates")
                        )
                })
        }
        (Limit::ADeeperPlace, Disagreement::PolarsAllows(refused)) => {
            refused.reason.contains("Boolean") && locate(case, &refused.place).nested >= 2
        }
        (Limit::AJoinKeyDtype, Disagreement::PolarsRefuses(refusal)) => {
            is(step, "LazyFrame.join") && refusal.reason.contains("datatypes of join keys")
        }
        (Limit::AValueThatRunsOnNoRows, Disagreement::PolarsAllows(refused)) => {
            // A condition that is decided has dropped its other branches, which
            // is its own cause.
            locate(case, &refused.place)
                .in_values
                .iter()
                .any(|condition| !is_decided(condition))
        }
        (Limit::ADecidedCondition, Disagreement::PolarsAllows(refused)) => {
            locate(case, &refused.place)
                .inside
                .iter()
                .any(|holder| is_decided(holder))
        }
        (Limit::AHiddenSchema, Disagreement::PolarsAllows(refused)) => {
            let locus = locate(case, &refused.place);
            let last = locus.inside.last().map(|chain| chain.links().len());
            refused.reason.starts_with("the schema says")
                && (locus.inside.len() > 1 || locus.index.is_some_and(|at| Some(at + 1) != last))
        }
        (Limit::AWindowOverAWindow, Disagreement::PolarsRefuses(refusal)) => {
            refusal.reason.contains("aggregation context")
                && held
                    .iter()
                    .any(|(_, held)| count_calls(held, &|call| is(call, "Expr.over")) > 1)
        }
        (Limit::ABooleanIsNeededAndTheDtypeUnknown, Disagreement::PolarsAllows(refused)) => {
            // The tables judged a start where a `Boolean` is needed on the dtype
            // they were measured on, which is not the dtype of this case.
            let locus = locate(case, &refused.place);
            refused.reason.contains("Boolean")
                && locus
                    .chain
                    .is_some_and(|chain| chain.links().is_empty() || locus.index.is_none())
        }
        (Limit::ABooleanIsNeededAndTheDtypeUnknown, Disagreement::PolarsRefuses(refusal)) => {
            refusal.reason.contains("Boolean") && boolean_dtype_unknown(case, prediction)
        }
        _ => false,
    }
}

/// The limits that excuse the disagreement of `prediction` and `observed` for
/// `case`, in the order of [`Limit::ALL`]. None when the case does not
/// disagree, or no limit is why it does.
pub fn excuses(case: &Case, prediction: &Prediction, observed: &Observed) -> Vec<Limit> {
    // `DAGEIST_FUZZ_STRICT=1` holds the tables to everything, to see what the
    // limits excuse.
    if strict() {
        return Vec::new();
    }
    let disagreement = match (&prediction.verdict, &observed.refusal) {
        (Verdict::Allows, Some(refusal)) => Disagreement::PolarsRefuses(refusal),
        (Verdict::Refuses(refused), None) if refused.source == Source::Polars => {
            Disagreement::PolarsAllows(refused)
        }
        _ => return Vec::new(),
    };
    Limit::ALL
        .into_iter()
        .filter(|limit| applies(*limit, case, prediction, &disagreement))
        .collect()
}

#[cfg(test)]
mod tests {
    //! A limit excuses a disagreement only at the expression that holds what it
    //! is about: the same feature somewhere else in the case excuses nothing.

    use dageist_engine::allowed::Representative;
    use dageist_engine::catalog;
    use dageist_engine::document::{Document, Dtype, Step, StepId};

    use super::super::predict::Traced;
    use super::*;

    fn call(id: &str, given: Vec<(&str, Value)>) -> Call {
        Call::new(catalog::entry(id).expect("an entry"), given).expect("a call")
    }

    fn column(name: &str) -> Call {
        call(
            "polars.col",
            vec![("name", Value::Columns(vec![name.to_owned()]))],
        )
    }

    /// The whole number 1 as a literal of `Int8`, which the tables did not
    /// measure as an argument.
    fn literal() -> Call {
        let dtype = Dtype::new(Representative::Int8.dtype()).expect("a dtype");
        call(
            "polars.lit",
            vec![("value", Value::Integer(1)), ("dtype", Value::Dtype(dtype))],
        )
    }

    fn chain(start: Call, links: &[&str]) -> Expression {
        let links = links.iter().map(|id| call(id, Vec::new())).collect();
        Expression::new(Start::Call(start), links).expect("a chain")
    }

    /// `select` of the chains, the tables refusing the call at `link` of the
    /// first, and Polars allowing the case.
    fn refused_at_the_first(items: Vec<Expression>, link: usize, reason: &str) -> Vec<Limit> {
        let held = Value::Many(items.into_iter().map(Value::Expression).collect());
        let step = Step::new(
            StepId::new("s").expect("an identifier"),
            call("LazyFrame.select", vec![("exprs", held)]),
        )
        .expect("a step");
        let case = Case {
            receiver: Representative::Int64,
            argument: Representative::Int64,
            document: Document::new(vec![step]).expect("a document"),
        };
        let place = Place::of_step(0, Some(StepId::new("s").expect("an identifier")))
            .then(Segment::Parameter("exprs".to_owned()))
            .then(Segment::Item(0))
            .then(Segment::Link(link));
        let prediction = Prediction {
            verdict: Verdict::Refuses(Refused {
                source: Source::Polars,
                at: place.to_string(),
                place,
                reason: reason.to_owned(),
            }),
            expressions: Vec::<Traced>::new(),
        };
        excuses(&case, &prediction, &Observed::default())
    }

    #[test]
    fn a_literal_in_another_expression_excuses_nothing() {
        let refused = chain(column("a"), &["Expr.cum_sum"]);
        let elsewhere = chain(literal(), &["Expr.first"]);
        assert_eq!(
            refused_at_the_first(vec![refused, elsewhere], 0, "no"),
            vec![]
        );
    }

    #[test]
    fn a_literal_that_starts_the_chain_with_the_refused_call_excuses_it() {
        let refused = chain(literal(), &["Expr.cum_sum"]);
        assert_eq!(
            refused_at_the_first(vec![refused], 0, "no"),
            vec![Limit::ALiteral]
        );
    }

    #[test]
    fn an_aggregate_in_the_expression_that_holds_the_refusal_excuses_it_and_one_in_another_does_not()
     {
        let after = chain(column("a"), &["Expr.cum_sum", "Expr.sum"]);
        let elsewhere = chain(column("b"), &["Expr.sum"]);
        // After the refused link of its own chain it is one value only in the
        // expression the position holds, which is what a sort drops.
        assert_eq!(
            refused_at_the_first(vec![after, elsewhere.clone()], 0, "no"),
            vec![Limit::AnAggregateGivesOneValue]
        );
        let alone = chain(column("a"), &["Expr.cum_sum"]);
        assert_eq!(
            refused_at_the_first(vec![alone, elsewhere], 0, "no"),
            vec![]
        );
    }

    #[test]
    fn an_aggregate_before_the_refused_call_excuses_it() {
        let refused = chain(column("a"), &["Expr.sum", "Expr.cum_sum"]);
        assert_eq!(
            refused_at_the_first(vec![refused], 1, "no"),
            vec![Limit::AnAggregateGivesOneValue]
        );
    }

    #[test]
    fn a_case_that_two_limits_explain_is_excused_by_both() {
        let refused = chain(literal(), &["Expr.sum", "Expr.cum_sum"]);
        let held = vec![refused];
        let both = refused_at_the_first(held, 1, "no");
        assert_eq!(both, vec![Limit::ALiteral, Limit::AnAggregateGivesOneValue]);
    }
}
