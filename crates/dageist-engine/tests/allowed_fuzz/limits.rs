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
//! A limit excuses a disagreement only when it is the cause of it. The tables
//! always say where they refused, and the place must lie inside the expression
//! that holds what the limit is about. Polars says where only when it refused
//! while it built the step and worked out its schema, and then not always: a
//! refusal of the whole step has the step for its place. A refusal of the rows
//! (the empty table's too) or a panic has none, and for those the reason and
//! the feature in the case are what there is; a limit that can only be gated so
//! says it in its documentation, and its tests gate the reason. A feature
//! elsewhere in the case excuses nothing. Whether a limit applies does not
//! depend on the order the limits are asked in, and a case that two apply to is
//! excused by both.

use dageist_engine::document::{Call, Expression, Place, Segment, Start, Value};

use super::case::Case;
use dageist_engine::allowed::Stage as TableStage;

use super::observe::{Observed, Refusal, Stage};
use super::predict::{Prediction, Refused, Source, Verdict};

/// A limit of the tables, and what it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// `Expr.over`'s `mapping_strategy` `explode` gives a different number of
    /// rows, which meets another expression's with lengths that do not match.
    /// `options.txt` records what it gives, but
    /// [`allowed::offer`](dageist_engine::allowed::offer) is asked of a method
    /// without its options. Polars refusing the rows, with a reason about
    /// lengths, cannot be placed, so the reason and an `explode` in the case are
    /// what gate it; the tables refusing is placed, inside an expression that
    /// holds an `explode`. (That `join` gives a `List` and `fractional` a
    /// `Float64` is not a limit but a stop: the prediction does not go past
    /// them, and the tally counts it.)
    OptionsChangeTheResult,
    /// A comparison or a subtraction with a null literal is null, and Polars does not evaluate
    /// the other side of it, wherever in the expression that comparison is. A
    /// literal is one value, which Polars broadcasts and casts as it does not
    /// a column: it accepts a text compared with a typed number, or a window
    /// over a constant, where a column of the dtype is refused, and joins it
    /// with a column of other length. The tables measure an argument as a
    /// column of the dtype, except the values of `shift` and a pivot's group.
    ALiteral,
    /// Polars does not evaluate what it does not need for one value: a sort key
    /// that is one value is dropped, and `len` does not read what it counts. The
    /// tables refusing at a call that follows an aggregate in its own chain, or
    /// inside a sort key that is one value, or inside an expression with a `len`
    /// in it, which Polars allows. What the tables say of a method's dtype was
    /// measured in `select` on a column.
    AnAggregateGivesOneValue,
    /// A pivot's aggregate is run on the values of each group, not on a
    /// column in `select`, and Polars uses other kernels for them: it takes the
    /// `max` of a `Struct` there, which `select` refuses, and compares, casts
    /// and windows what `select` does not. A table with no rows has no groups,
    /// and the values of an aggregate over none have no dtype. The empty table's
    /// refusal is gated by its reason, which has no place; the tables refusing
    /// is placed, in the aggregate, with a reason about a kernel.
    AGroupAggregate,
    /// A single value in a key, a join's or a window's, beside a column is not
    /// elementwise, and Polars refuses it: an aggregate, or a null literal. The
    /// context table measured the entry as the whole key. A key that is wholly a
    /// single value is the whole key, and is not this. Polars refusing the step
    /// has the step for its place, so the key itself is found by what it holds;
    /// a refusal of the rows of a window has none and is gated by its reason.
    /// The tables refusing a key that is wholly single, which Polars allows, is
    /// placed in that key.
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
    /// Polars panicking has no place, and is gated by its reason. A panic of
    /// Polars is an internal check that failed. The tables record
    /// one for the plan Polars made of a combination in `select`, and the plan
    /// of another place may not reach it; a combination of what they measured
    /// singly, such as two keys of a window, may reach another.
    APanic,
    /// A place more than two positions deep is judged by its first two, so a
    /// predicate inside a value of a condition is judged as a value, which need
    /// not be a `Boolean`: the tables allow a predicate of a known dtype that is
    /// no `Boolean`, and refuse a value inside a predicate as a predicate.
    ADeeperPlace,
    /// A condition with a predicate made of literals alone is decided before it
    /// is run: Polars takes the branch it selects and drops the others, so what
    /// the predicates and values it drops would refuse, it does not. A predicate
    /// that only starts from a literal, and goes on to a column, is not this.
    ADecidedCondition,
    /// A value of a condition runs on the rows its predicate selects, and a
    /// predicate after the first on the rows left to it. On the rows of the
    /// sample that may be none, and what would be refused is not raised. Only
    /// what the tables refused when it ran rows is excused; what they refused
    /// when it built the step is refused whatever the rows.
    AValueThatRunsOnNoRows,
    /// The other table's key is made of the dtype its chain gives on its own,
    /// and the join may resolve the chain to another, as the precision of a
    /// decimal that a null is taken from. Polars' message names the decimal, and
    /// the refusal is of the step.
    AJoinKeyDtype,
    /// The tables hold the schema of a method and what its rows give as one
    /// thing, and refuse a method whose plan says another dtype than its rows
    /// give. Inside an expression that goes on from it or compares it, the rows
    /// of the step do not show it.
    AHiddenSchema,
    /// A window whose receiver holds another window is run by Polars in the
    /// aggregation context of the outer one, where a key that is not elementwise
    /// is refused; the tables measured one window. The receiver is what comes
    /// before the outer `over` in its chain, with the arguments of those calls.
    /// Polars refuses the rows, so there is no place and the reason gates it.
    AWindowOverAWindow,
    /// The tables are asked what a chain gives so that a predicate, which needs
    /// a `Boolean`, can be checked, and a start, or a method that gives what its
    /// arguments decide, gives nothing to check: the dtype of a column is the
    /// caller's to say, and [`offer`](dageist_engine::allowed::offer) is not told
    /// it. Only an expression in a predicate position is meant. The tables
    /// refusing a start there is judged on the dtype it was measured on.
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

/// Whether `call` is a null literal on its own.
fn is_a_null(call: &Call) -> bool {
    is(call, "polars.lit") && matches!(call.argument("value"), Some(Value::Flag(None)))
}

/// Whether a call of `expression` compares or subtracts a null literal, as what it is given
/// or as what it follows.
fn compared_with_a_null(expression: &Expression) -> bool {
    let comparisons = [
        "Expr.eq", "Expr.ne", "Expr.gt", "Expr.ge", "Expr.lt", "Expr.le", "Expr.sub",
    ];
    expression.links().iter().enumerate().any(|(at, link)| {
        comparisons.contains(&link.entry().id().as_str())
            && (operands(link).into_iter().any(|(_, operand)| {
                operand.links().is_empty()
                    && matches!(operand.start(), Start::Call(start) if is_a_null(start))
            }) || (at == 0
                && matches!(expression.start(), Start::Call(start) if is_a_null(start))))
    })
}

/// Whether `expression` is made of literals alone: nothing in it, at its start
/// or in the arguments of its calls, reads a column.
fn only_literals(expression: &Expression) -> bool {
    let start = match expression.start() {
        Start::Call(start) if is(start, "polars.lit") => true,
        Start::Call(start) if is(start, "polars.when") => operands(start)
            .into_iter()
            .all(|(_, inside)| only_literals(inside)),
        _ => false,
    };
    start
        && expression.links().iter().all(|link| {
            operands(link)
                .into_iter()
                .all(|(_, inside)| only_literals(inside))
        })
}

/// Whether a condition has a predicate that is made of literals alone.
fn is_decided(expression: &Expression) -> bool {
    let Start::Call(start) = expression.start() else {
        return false;
    };
    is(start, "polars.when")
        && std::iter::once(start)
            .chain(expression.links())
            .flat_map(|call| operands(call))
            .any(|(parameter, predicate)| parameter == "predicates" && only_literals(predicate))
}

/// Whether the first `upto` links of `expression` give one value and nothing
/// beside them does not: an aggregate among them with only single values after
/// it; with `literals`, a start made of literals too; or a condition whose parts
/// are single.
fn wholly_single(expression: &Expression, upto: usize, literals: bool) -> bool {
    let links = &expression.links()[..upto.min(expression.links().len())];
    let single = |call: &Call| {
        operands(call)
            .into_iter()
            .all(|(_, inside)| wholly_single(inside, inside.links().len(), true))
    };
    if let Some(last) = links.iter().rposition(is_one_value) {
        return links[last + 1..].iter().all(single);
    }
    match expression.start() {
        Start::Call(start) if is(start, "polars.lit") => literals && links.iter().all(single),
        Start::Call(start) if is(start, "polars.when") => single(start) && links.iter().all(single),
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

/// Whether an expression is in a predicate position, which needs a `Boolean`:
/// the last parameter on the way to it is `predicates`.
fn in_a_predicate_position(place: &Place) -> bool {
    place.path().iter().rev().find_map(|segment| match segment {
        Segment::Parameter(name) => Some(name.as_str()),
        _ => None,
    }) == Some("predicates")
}

/// The predicates the tables were asked of that gave nothing to check against a
/// `Boolean`: no link, or a last link whose dtype they did not give.
fn predicates_of_unknown_dtype(prediction: &Prediction) -> Vec<&Place> {
    prediction
        .expressions
        .iter()
        .filter(|expression| {
            in_a_predicate_position(&expression.place)
                && (!expression.told || expression.links.last().is_none_or(Option::is_none))
        })
        .map(|expression| &expression.place)
        .collect()
}

/// Whether `place` lies inside `inside`, as the way to it begins with the way
/// to `inside`.
fn lies_inside(place: &Place, inside: &Place) -> bool {
    place.path().starts_with(inside.path())
}

/// The keys of the step: a join's, if `joins`, and the keys of every window in it.
fn keys(case: &Case, joins: bool) -> Vec<&Expression> {
    let step = case.document.steps()[0].call();
    let mut found: Vec<&Expression> = Vec::new();
    if joins && is(step, "LazyFrame.join") {
        found.extend(
            operands(step)
                .into_iter()
                .filter(|(parameter, _)| ["on", "left_on", "right_on"].contains(parameter))
                .map(|(_, key)| key),
        );
    }
    fn windows<'a>(expression: &'a Expression, found: &mut Vec<&'a Expression>) {
        let calls = std::iter::once(expression.start())
            .filter_map(|start| match start {
                Start::Call(call) => Some(call),
                Start::Input(_) => None,
            })
            .chain(expression.links());
        for call in calls {
            if is(call, "Expr.over") {
                found.extend(
                    operands(call)
                        .into_iter()
                        .filter(|(parameter, _)| {
                            ["partition_by", "more_exprs", "order_by"].contains(parameter)
                        })
                        .map(|(_, key)| key),
                );
            }
            for (_, inside) in operands(call) {
                windows(inside, found);
            }
        }
    }
    for (_, held) in operands(step) {
        windows(held, &mut found);
    }
    found
}

/// Whether a key holds a single value that is no column, an aggregate or a null
/// literal, beside what is not one: a key that is wholly one is the whole key,
/// which the tables measured.
fn holds_a_single_value_beside_a_column(key: &Expression) -> bool {
    any_call(key, &is_a_single_value) && !wholly_single(key, key.links().len(), true)
}

/// Whether an `over` has, before it in its chain, another window.
fn window_over_a_window(expression: &Expression) -> bool {
    let calls: Vec<&Call> = std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) => None,
        })
        .chain(expression.links())
        .collect();
    let windowed = |call: &Call| {
        is(call, "Expr.over")
            || operands(call)
                .into_iter()
                .any(|(_, inside)| any_call(inside, &|inner| is(inner, "Expr.over")))
    };
    let own = calls.iter().enumerate().any(|(at, call)| {
        is(call, "Expr.over") && calls[..at].iter().any(|before| windowed(before))
    });
    own || calls.iter().any(|call| {
        operands(call)
            .into_iter()
            .any(|(_, inside)| window_over_a_window(inside))
    })
}

/// The place Polars refused at, inside the step, if it said.
fn placed(refusal: &Refusal) -> Option<&Place> {
    refusal
        .place
        .as_ref()
        .filter(|place| !place.path().is_empty())
}

/// Whether a refusal by Polars has no place inside the step: it refused the
/// rows or panicked, or refused the step as a whole.
fn unplaced(refusal: &Refusal) -> bool {
    refusal
        .place
        .as_ref()
        .is_none_or(|place| place.path().is_empty())
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
            unplaced(refusal)
                && is_lengths(&refusal.reason)
                && held.iter().any(|(_, held)| explodes_in(held))
        }
        (Limit::OptionsChangeTheResult, Disagreement::PolarsAllows(refused)) => {
            locate(case, &refused.place).top.is_some_and(&explodes_in)
        }
        (Limit::TheLengthsOfTheRows, Disagreement::PolarsRefuses(refusal)) => {
            unplaced(refusal)
                && is_lengths(&refusal.reason)
                && !held.iter().any(|(_, held)| explodes_in(held))
        }
        (Limit::ALiteral, Disagreement::PolarsAllows(refused)) => {
            let locus = locate(case, &refused.place);
            let chain_starts = locus.chain.is_some_and(starts_from_a_literal);
            let operand = locus.call.is_some_and(|call| {
                operands(call).into_iter().any(|(parameter, operand)| {
                    !measured_literal(call, parameter, pivot) && starts_from_a_literal(operand)
                })
            });
            // A comparison with a null literal is null, and Polars does not
            // evaluate the other side of it, wherever in the expression it is.
            let folded = locus
                .inside
                .iter()
                .any(|holder| compared_with_a_null(holder));
            chain_starts || operand || folded
        }
        (Limit::AnAggregateGivesOneValue, Disagreement::PolarsAllows(refused)) => {
            let locus = locate(case, &refused.place);
            // What the refused call follows in its chain is one value, which
            // Polars broadcasts.
            let receiver = locus.inside.last().is_some_and(|chain| {
                locus
                    .index
                    .is_some_and(|at| at > 0 && wholly_single(chain, at, false))
            });
            // A sort key that is one value is dropped, and so is a window's.
            let keys = keys(case, false);
            let dropped = (is(step, "LazyFrame.sort")
                && locus
                    .top
                    .is_some_and(|top| wholly_single(top, top.links().len(), true)))
                || locus.inside.iter().any(|holder| {
                    keys.iter().any(|key| std::ptr::eq(*key, *holder))
                        && wholly_single(holder, holder.links().len(), true)
                });
            // `len` does not read what it counts.
            let counted = locus
                .inside
                .iter()
                .any(|holder| holder.links().iter().any(|link| is(link, "Expr.len")));
            receiver || dropped || counted
        }
        (Limit::AGroupAggregate, Disagreement::PolarsRefuses(refusal)) => {
            // A table with no rows has no groups, and the values of an
            // aggregate over none have no dtype.
            pivot && refusal.stage == Stage::Empty && refusal.reason.contains("not supported")
        }
        (Limit::AGroupAggregate, Disagreement::PolarsAllows(refused)) => {
            // The kernels that take the values of a group accept what the ones of
            // `select` refuse: a dtype of an aggregate, a comparison, a cast, a
            // struct, or a window over a group.
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
            let keys = keys(case, true);
            let mixed = |key: &Expression| holds_a_single_value_beside_a_column(key);
            refusal.reason.contains("elementwise")
                && match placed(refusal) {
                    // The refusal is inside a key, and that key holds it.
                    Some(place) => locate(case, place).inside.iter().any(|inside| {
                        keys.iter().any(|key| std::ptr::eq(*key, *inside)) && mixed(inside)
                    }),
                    // Of the step, or of the rows of a window: some key does.
                    None => keys.iter().any(|key| mixed(key)),
                }
        }
        (Limit::ASingleValueInAKey, Disagreement::PolarsAllows(refused)) => {
            // The key the tables refused is the one that holds the single value.
            refused.reason.contains("elementwise")
                && locate(case, &refused.place).top.is_some_and(|top| {
                    keys(case, true).iter().any(|key| std::ptr::eq(*key, top))
                        && any_call(top, &is_a_single_value)
                })
        }
        (Limit::AnOverflow, Disagreement::PolarsRefuses(refusal)) => {
            unplaced(refusal) && refusal.reason.contains("with overflow")
        }
        (Limit::TheBranchesAreOne, Disagreement::PolarsAllows(refused)) => {
            locate(case, &refused.place)
                .in_predicates
                .iter()
                .any(|condition| {
                    let values: Vec<&Expression> = condition
                        .links()
                        .iter()
                        .filter(|link| is_a_value_link(link))
                        .filter_map(|link| match link.argument("statement") {
                            Some(Value::Expression(value)) => Some(value),
                            _ => None,
                        })
                        .collect();
                    values.len() > 1 && values.windows(2).all(|pair| pair[0] == pair[1])
                })
        }
        (Limit::APanic, Disagreement::PolarsRefuses(refusal)) => {
            unplaced(refusal)
                && is_panic(&refusal.reason)
                && !refusal.reason.contains("with overflow")
        }
        (Limit::APanic, Disagreement::PolarsAllows(refused)) => is_panic(&refused.reason),
        (Limit::ADeeperPlace, Disagreement::PolarsRefuses(refusal)) => {
            // The tables judged a predicate of two positions deep as a value,
            // which need not be a `Boolean`, and it is a dtype that is not.
            refusal.reason.contains("Boolean")
                && prediction.expressions.iter().any(|expression| {
                    let class = expression.links.last().copied().unwrap_or(expression.start);
                    expression.nested >= 2
                        && in_a_predicate_position(&expression.place)
                        && class.is_some_and(|class| {
                            class != dageist_engine::allowed::Representative::Boolean
                        })
                        && placed(refusal).is_none_or(|place| lies_inside(place, &expression.place))
                })
        }
        (Limit::ADeeperPlace, Disagreement::PolarsAllows(refused)) => {
            refused.reason.contains("Boolean") && locate(case, &refused.place).nested >= 2
        }
        (Limit::AJoinKeyDtype, Disagreement::PolarsRefuses(refusal)) => {
            is(step, "LazyFrame.join")
                && unplaced(refusal)
                && refusal.reason.contains("datatypes of join keys")
                && refusal.reason.contains("decimal")
        }
        (Limit::AValueThatRunsOnNoRows, Disagreement::PolarsAllows(refused)) => {
            // Only what the tables refused when it ran rows. A condition that is
            // decided has dropped its other branches, which is its own cause.
            refused.stage == Some(TableStage::Rows)
                && locate(case, &refused.place)
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
            unplaced(refusal)
                && refusal.reason.contains("aggregation context")
                && held.iter().any(|(_, held)| window_over_a_window(held))
        }
        (Limit::ABooleanIsNeededAndTheDtypeUnknown, Disagreement::PolarsAllows(refused)) => {
            // The tables judged a start where a `Boolean` is needed on the dtype
            // they were measured on, which is not the dtype of this case.
            let locus = locate(case, &refused.place);
            refused.reason.contains("Boolean")
                && in_a_predicate_position(&refused.place)
                && locus.chain.is_some_and(|chain| {
                    chain.links().is_empty() || (locus.call.is_some() && locus.index.is_none())
                })
        }
        (Limit::ABooleanIsNeededAndTheDtypeUnknown, Disagreement::PolarsRefuses(refusal)) => {
            refusal.reason.contains("Boolean")
                && predicates_of_unknown_dtype(prediction)
                    .into_iter()
                    .any(|predicate| {
                        placed(refusal).is_none_or(|place| lies_inside(place, predicate))
                    })
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
    //! A limit excuses a disagreement only where it is the cause: the same feature
    //! somewhere else in the case excuses nothing. Each limit is tested so, or,
    //! where Polars' refusal has no place (the rows, a panic), by the reason that
    //! gates it.

    use dageist_engine::allowed::Representative;
    use dageist_engine::catalog;
    use dageist_engine::document::{Document, Dtype, Step, StepId};

    use super::super::predict::Traced;
    use super::*;

    // ----- Building cases and disagreements

    fn call(id: &str, given: Vec<(&str, Value)>) -> Call {
        Call::new(catalog::entry(id).expect("an entry"), given).expect("a call")
    }

    fn column(name: &str) -> Call {
        call(
            "polars.col",
            vec![("name", Value::Columns(vec![name.to_owned()]))],
        )
    }

    /// The whole number 1 as a literal of `Int8`.
    fn literal() -> Call {
        let dtype = Dtype::new(Representative::Int8.dtype()).expect("a dtype");
        call(
            "polars.lit",
            vec![("value", Value::Integer(1)), ("dtype", Value::Dtype(dtype))],
        )
    }

    fn null() -> Call {
        call("polars.lit", vec![("value", Value::Flag(None))])
    }

    fn chain(start: Call, links: &[&str]) -> Expression {
        chain_of(start, links.iter().map(|id| call(id, Vec::new())).collect())
    }

    fn chain_of(start: Call, links: Vec<Call>) -> Expression {
        Expression::new(Start::Call(start), links).expect("a chain")
    }

    fn many(items: Vec<Expression>) -> Value {
        Value::Many(items.into_iter().map(Value::Expression).collect())
    }

    fn compared_with(id: &str, other: Expression) -> Call {
        call(id, vec![("other", Value::Expression(other))])
    }

    fn over(key: &str, mapping: Option<&str>) -> Call {
        let mut given = vec![("partition_by", many(vec![chain(column(key), &[])]))];
        if let Some(word) = mapping {
            given.push(("mapping_strategy", Value::Choice(word.to_owned())));
        }
        call("Expr.over", given)
    }

    /// `when(predicate).then(value).otherwise(value)`.
    fn condition(predicate: Expression, value: Expression) -> Expression {
        chain_of(
            call("polars.when", vec![("predicates", many(vec![predicate]))]),
            vec![
                call(
                    "When.then",
                    vec![("statement", Value::Expression(value.clone()))],
                ),
                call(
                    "Then.otherwise",
                    vec![("statement", Value::Expression(value))],
                ),
            ],
        )
    }

    fn case_of(step: Call) -> Case {
        let step = Step::new(StepId::new("s").expect("an identifier"), step).expect("a step");
        Case {
            receiver: Representative::Int64,
            argument: Representative::Int64,
            document: Document::new(vec![step]).expect("a document"),
        }
    }

    fn select(items: Vec<Expression>) -> Case {
        case_of(call("LazyFrame.select", vec![("exprs", many(items))]))
    }

    fn sort(items: Vec<Expression>) -> Case {
        case_of(call("LazyFrame.sort", vec![("by", many(items))]))
    }

    fn filter(items: Vec<Expression>) -> Case {
        case_of(call("LazyFrame.filter", vec![("predicates", many(items))]))
    }

    fn join(left: Expression, right: Expression) -> Case {
        case_of(call(
            "LazyFrame.join",
            vec![
                ("other", Value::Table("other".to_owned())),
                ("left_on", many(vec![left])),
                ("right_on", many(vec![right])),
            ],
        ))
    }

    fn pivot(aggregate: Expression) -> Case {
        case_of(call(
            "LazyFrame.pivot",
            vec![
                ("on", Value::Columns(vec!["h".to_owned()])),
                ("on_columns", Value::Table("on_h".to_owned())),
                ("values", Value::Columns(vec!["a".to_owned()])),
                ("aggregate_function", Value::Expression(aggregate)),
            ],
        ))
    }

    fn step_place() -> Place {
        Place::of_step(0, Some(StepId::new("s").expect("an identifier")))
    }

    fn at(path: &[Segment]) -> Place {
        path.iter().cloned().fold(step_place(), Place::then)
    }

    fn parameter(name: &str) -> Segment {
        Segment::Parameter(name.to_owned())
    }

    /// The place of link `link` of item `item` of the parameter `name`.
    fn link_of(name: &str, item: usize, link: usize) -> Place {
        at(&[parameter(name), Segment::Item(item), Segment::Link(link)])
    }

    fn prediction(verdict: Verdict, expressions: Vec<Traced>) -> Prediction {
        Prediction {
            verdict,
            expressions,
            stopped_by_options: false,
        }
    }

    /// The tables refuse the case at `place`, at `stage`, and Polars allows it.
    fn tables_refuse(
        case: &Case,
        place: Place,
        stage: Option<TableStage>,
        reason: &str,
    ) -> Vec<Limit> {
        let verdict = Verdict::Refuses(Refused {
            source: Source::Polars,
            at: place.to_string(),
            place,
            stage,
            reason: reason.to_owned(),
        });
        excuses(case, &prediction(verdict, Vec::new()), &Observed::default())
    }

    /// The tables allow the case and Polars refuses it, at `place` if it said.
    fn polars_refuses(
        case: &Case,
        place: Option<Place>,
        stage: Stage,
        reason: &str,
        traced: Vec<Traced>,
    ) -> Vec<Limit> {
        let observed = Observed {
            refusal: Some(Refusal {
                stage,
                at: String::new(),
                place,
                reason: reason.to_owned(),
            }),
            expressions: Vec::new(),
        };
        excuses(case, &prediction(Verdict::Allows, traced), &observed)
    }

    const ROWS: Option<TableStage> = Some(TableStage::Rows);

    // ----- Literal, aggregate

    #[test]
    fn a_literal_in_another_expression_excuses_nothing() {
        let refused = chain(column("a"), &["Expr.cum_sum"]);
        let elsewhere = chain(literal(), &["Expr.first"]);
        let case = select(vec![refused, elsewhere]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn a_literal_in_a_later_call_of_the_chain_excuses_nothing() {
        let refused = chain_of(
            column("a"),
            vec![
                call("Expr.cum_sum", Vec::new()),
                compared_with("Expr.eq", chain(literal(), &[])),
            ],
        );
        let case = select(vec![refused]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
        // At the call that is given it, it excuses.
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 1), ROWS, "no"),
            vec![Limit::ALiteral]
        );
    }

    #[test]
    fn a_literal_that_starts_the_chain_with_the_refused_call_excuses_it() {
        let case = select(vec![chain(literal(), &["Expr.cum_sum"])]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
            vec![Limit::ALiteral]
        );
    }

    #[test]
    fn an_aggregate_before_the_refused_call_excuses_it_and_after_it_does_not() {
        let case = select(vec![chain(column("a"), &["Expr.sum", "Expr.cum_sum"])]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 1), ROWS, "no"),
            vec![Limit::AnAggregateGivesOneValue]
        );
        let after = select(vec![chain(column("a"), &["Expr.cum_sum", "Expr.sum"])]);
        assert_eq!(
            tables_refuse(&after, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn an_aggregate_in_another_expression_excuses_nothing() {
        let case = select(vec![
            chain(column("a"), &["Expr.cum_sum"]),
            chain(column("b"), &["Expr.sum", "Expr.len"]),
        ]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn a_sort_key_that_is_one_value_is_dropped_and_a_select_item_is_not() {
        let key = chain(column("a"), &["Expr.cum_sum", "Expr.sum"]);
        assert_eq!(
            tables_refuse(&sort(vec![key.clone()]), link_of("by", 0, 0), ROWS, "no"),
            vec![Limit::AnAggregateGivesOneValue]
        );
        assert_eq!(
            tables_refuse(&select(vec![key]), link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
        // A sort key that is not one value is not dropped, whatever is in the
        // other key.
        let case = case_of(call(
            "LazyFrame.sort",
            vec![(
                "by",
                many(vec![
                    chain(column("a"), &["Expr.cum_sum"]),
                    chain(column("b"), &["Expr.sum"]),
                ]),
            )],
        ));
        assert_eq!(
            tables_refuse(&case, link_of("by", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn len_does_not_read_what_it_counts_and_the_other_aggregates_do() {
        let counted = select(vec![chain(column("a"), &["Expr.cum_sum", "Expr.len"])]);
        assert_eq!(
            tables_refuse(&counted, link_of("exprs", 0, 0), ROWS, "no"),
            vec![Limit::AnAggregateGivesOneValue]
        );
        let summed = select(vec![chain(column("a"), &["Expr.cum_sum", "Expr.sum"])]);
        assert_eq!(
            tables_refuse(&summed, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn a_case_that_two_limits_explain_is_excused_by_both() {
        let case = select(vec![chain(literal(), &["Expr.sum", "Expr.cum_sum"])]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 1), ROWS, "no"),
            vec![Limit::ALiteral, Limit::AnAggregateGivesOneValue]
        );
    }

    // ----- Options, lengths, overflow, panic: the reason gates what has no place

    fn explode() -> Call {
        over("k", Some("explode"))
    }

    #[test]
    fn an_explode_excuses_the_tables_refusing_inside_its_expression_only() {
        let with = select(vec![chain_of(
            column("a"),
            vec![call("Expr.cum_sum", Vec::new()), explode()],
        )]);
        assert_eq!(
            tables_refuse(&with, link_of("exprs", 0, 0), ROWS, "no"),
            vec![Limit::OptionsChangeTheResult]
        );
        let elsewhere = select(vec![
            chain(column("a"), &["Expr.cum_sum"]),
            chain_of(column("b"), vec![explode()]),
        ]);
        assert_eq!(
            tables_refuse(&elsewhere, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn a_refusal_about_lengths_is_the_explodes_or_the_rows_by_whether_there_is_an_explode() {
        let reason = "lengths don't match: zip node received non-equal length inputs";
        let with = select(vec![chain_of(column("a"), vec![explode()])]);
        assert_eq!(
            polars_refuses(&with, None, Stage::Rows, reason, Vec::new()),
            vec![Limit::OptionsChangeTheResult]
        );
        let without = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            polars_refuses(&without, None, Stage::Rows, reason, Vec::new()),
            vec![Limit::TheLengthsOfTheRows]
        );
        // Another reason, or a refusal placed at a link, is neither.
        assert_eq!(
            polars_refuses(&without, None, Stage::Rows, "no", Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(
                &without,
                Some(link_of("exprs", 0, 0)),
                Stage::Schema,
                reason,
                Vec::new()
            ),
            vec![]
        );
    }

    #[test]
    fn an_overflow_is_not_a_panic_and_a_panic_is_not_an_overflow() {
        let case = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            polars_refuses(
                &case,
                None,
                Stage::Rows,
                "Polars panicked: attempt to add with overflow",
                Vec::new()
            ),
            vec![Limit::AnOverflow]
        );
        assert_eq!(
            polars_refuses(
                &case,
                None,
                Stage::Rows,
                "Polars panicked: assertion failed: i < self.len()",
                Vec::new()
            ),
            vec![Limit::APanic]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, "refused", Vec::new()),
            vec![]
        );
        // The tables recording a panic, where Polars allows the case.
        assert_eq!(
            tables_refuse(
                &case,
                link_of("exprs", 0, 0),
                ROWS,
                "Polars panicked: should not fail"
            ),
            vec![Limit::APanic]
        );
    }

    // ----- Keys

    #[test]
    fn a_key_that_holds_a_single_value_beside_a_column_excuses_the_refused_step() {
        let mixed = chain_of(
            column("f"),
            vec![compared_with("Expr.le", chain(null(), &[]))],
        );
        let reason = "all join key expressions must be elementwise.";
        let case = join(mixed, chain(column("key"), &[]));
        assert_eq!(
            polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
            vec![Limit::ASingleValueInAKey]
        );
    }

    #[test]
    fn a_single_value_in_the_other_key_does_not_excuse_a_bad_key() {
        let bad = chain(column("a"), &["Expr.cum_max"]);
        let whole = chain(column("x"), &["Expr.first"]);
        let reason = "all join key expressions must be elementwise.";
        let case = join(bad, whole);
        assert_eq!(
            polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
            vec![]
        );
    }

    #[test]
    fn a_refusal_inside_a_key_is_excused_by_that_key_and_not_by_the_other() {
        let bad = chain(column("a"), &["Expr.cum_max"]);
        let mixed = chain_of(
            column("f"),
            vec![compared_with("Expr.le", chain(null(), &[]))],
        );
        let reason = "all join key expressions must be elementwise.";
        let case = join(bad, mixed);
        let in_left = link_of("left_on", 0, 0);
        let in_right = link_of("right_on", 0, 0);
        assert_eq!(
            polars_refuses(&case, Some(in_left), Stage::Schema, reason, Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, Some(in_right), Stage::Schema, reason, Vec::new()),
            vec![Limit::ASingleValueInAKey]
        );
    }

    #[test]
    fn the_tables_refusing_a_key_are_excused_when_that_key_holds_the_single_value() {
        let reason = "all join key expressions must be elementwise.";
        let single = chain_of(
            column("f"),
            vec![
                call("Expr.cum_max", Vec::new()),
                compared_with("Expr.le", chain(column("x"), &["Expr.max"])),
            ],
        );
        let plain = chain(column("a"), &["Expr.cum_max"]);
        let case = join(single.clone(), plain.clone());
        assert_eq!(
            tables_refuse(
                &case,
                link_of("left_on", 0, 0),
                Some(TableStage::Schema),
                reason
            ),
            vec![Limit::ASingleValueInAKey]
        );
        let other = join(plain, single);
        assert_eq!(
            tables_refuse(
                &other,
                link_of("left_on", 0, 0),
                Some(TableStage::Schema),
                reason
            ),
            vec![]
        );
    }

    #[test]
    fn a_join_key_dtype_is_the_refusal_of_a_join_that_names_a_decimal() {
        let reason = "datatypes of join keys don't match - `a`: decimal[38,2] on left does not match `key`: decimal[10,2] on right";
        let case = join(chain(column("a"), &[]), chain(column("key"), &[]));
        assert_eq!(
            polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
            vec![Limit::AJoinKeyDtype]
        );
        let not_decimal = "datatypes of join keys don't match - `a`: i64 on left";
        assert_eq!(
            polars_refuses(
                &case,
                Some(step_place()),
                Stage::Schema,
                not_decimal,
                Vec::new()
            ),
            vec![]
        );
        let select = select(vec![chain(column("a"), &[])]);
        assert_eq!(
            polars_refuses(&select, None, Stage::Schema, reason, Vec::new()),
            vec![]
        );
    }

    #[test]
    fn a_window_whose_receiver_holds_a_window_is_a_window_over_a_window() {
        let reason = "window expression with non-elementwise `partition_by` or `order_by` not allowed in aggregation context";
        let chained = select(vec![chain_of(
            column("a"),
            vec![
                over("k", None),
                call("Expr.cum_sum", Vec::new()),
                over("g", None),
            ],
        )]);
        assert_eq!(
            polars_refuses(&chained, None, Stage::Rows, reason, Vec::new()),
            vec![Limit::AWindowOverAWindow]
        );
        // Two windows that are not one over the other, and a window in a key.
        let apart = select(vec![
            chain_of(column("a"), vec![over("k", None)]),
            chain_of(column("b"), vec![over("g", None)]),
        ]);
        assert_eq!(
            polars_refuses(&apart, None, Stage::Rows, reason, Vec::new()),
            vec![]
        );
        let in_a_key = select(vec![chain_of(
            column("a"),
            vec![call(
                "Expr.over",
                vec![(
                    "partition_by",
                    many(vec![chain_of(column("g"), vec![over("k", None)])]),
                )],
            )],
        )]);
        assert_eq!(
            polars_refuses(&in_a_key, None, Stage::Rows, reason, Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(&chained, None, Stage::Rows, "no", Vec::new()),
            vec![]
        );
    }

    // ----- Conditions

    /// The place of link 0 of the value of link 0 of item 0 of `exprs`: inside a
    /// value of the condition at the item.
    fn in_the_value() -> Place {
        at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(0),
            parameter("statement"),
            Segment::Link(0),
        ])
    }

    /// The place of link 0 of the predicate of the condition at item 0.
    fn in_the_predicate() -> Place {
        at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Start,
            parameter("predicates"),
            Segment::Item(0),
            Segment::Link(0),
        ])
    }

    fn undecided() -> Expression {
        condition(
            chain(column("g"), &["Expr.is_null"]),
            chain(column("a"), &["Expr.cum_sum"]),
        )
    }

    /// A condition whose values are not one expression.
    fn distinct() -> Expression {
        chain_of(
            call(
                "polars.when",
                vec![(
                    "predicates",
                    many(vec![chain(column("g"), &["Expr.is_null"])]),
                )],
            ),
            vec![
                call(
                    "When.then",
                    vec![(
                        "statement",
                        Value::Expression(chain(column("a"), &["Expr.cum_sum"])),
                    )],
                ),
                call(
                    "Then.otherwise",
                    vec![(
                        "statement",
                        Value::Expression(chain(column("b"), &["Expr.cum_sum"])),
                    )],
                ),
            ],
        )
    }

    fn decided() -> Expression {
        condition(
            chain(null(), &["Expr.is_null"]),
            chain(column("a"), &["Expr.cum_sum"]),
        )
    }

    #[test]
    fn branches_that_are_one_excuse_a_refusal_in_their_predicate_and_not_elsewhere() {
        let with = select(vec![undecided()]);
        assert_eq!(
            tables_refuse(&with, in_the_predicate(), Some(TableStage::Schema), "no"),
            vec![Limit::TheBranchesAreOne]
        );
        // The same condition in another item.
        let elsewhere = select(vec![chain(column("a"), &["Expr.cum_sum"]), undecided()]);
        assert_eq!(
            tables_refuse(
                &elsewhere,
                link_of("exprs", 0, 0),
                Some(TableStage::Schema),
                "no"
            ),
            vec![]
        );
        // A refusal in the value, not the predicate, is not this.
        assert!(
            !tables_refuse(&with, in_the_value(), Some(TableStage::Schema), "no")
                .contains(&Limit::TheBranchesAreOne)
        );
    }

    #[test]
    fn a_predicate_that_only_starts_from_a_literal_does_not_decide_a_condition() {
        let predicate = chain_of(
            null(),
            vec![compared_with("Expr.lt", chain(column("a"), &[]))],
        );
        let leans = condition(predicate, chain(column("a"), &["Expr.cum_sum"]));
        let case = select(vec![leans]);
        assert_eq!(
            tables_refuse(&case, in_the_value(), Some(TableStage::Schema), "no"),
            vec![]
        );
        let case = select(vec![decided()]);
        assert_eq!(
            tables_refuse(&case, in_the_value(), Some(TableStage::Schema), "no"),
            vec![Limit::ADecidedCondition]
        );
        // A decided condition in another item decides nothing here.
        let elsewhere = select(vec![undecided(), decided()]);
        assert_eq!(
            tables_refuse(&elsewhere, in_the_value(), Some(TableStage::Schema), "no"),
            vec![]
        );
    }

    #[test]
    fn a_value_runs_on_no_rows_only_for_what_the_tables_refused_when_they_ran_rows() {
        let case = select(vec![distinct()]);
        assert_eq!(
            tables_refuse(&case, in_the_value(), ROWS, "no"),
            vec![Limit::AValueThatRunsOnNoRows]
        );
        assert_eq!(
            tables_refuse(&case, in_the_value(), Some(TableStage::Schema), "no"),
            vec![]
        );
        // A refusal outside the value, and a condition elsewhere.
        assert_eq!(tables_refuse(&case, in_the_predicate(), ROWS, "no"), vec![]);
        let elsewhere = select(vec![chain(column("a"), &["Expr.cum_sum"]), distinct()]);
        assert_eq!(
            tables_refuse(&elsewhere, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
        // A decided condition has dropped its branches, which is its own cause.
        let case = select(vec![decided()]);
        assert_eq!(
            tables_refuse(&case, in_the_value(), ROWS, "no"),
            vec![Limit::ADecidedCondition]
        );
    }

    // ----- Places, schemas, Booleans, pivots

    fn traced(place: Place, nested: usize, class: Option<Representative>) -> Traced {
        Traced {
            place,
            nested,
            start: None,
            links: vec![class],
            told: true,
        }
    }

    fn predicate_at(item: usize) -> Place {
        at(&[parameter("predicates"), Segment::Item(item)])
    }

    #[test]
    fn a_predicate_two_positions_deep_of_a_dtype_that_is_no_boolean_is_judged_as_a_value() {
        let case = filter(vec![chain(column("a"), &[]), chain(column("b"), &[])]);
        let reason = "invalid series dtype: expected `Boolean`, got `i8`";
        let deep = traced(predicate_at(0), 2, Some(Representative::Int8));
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, reason, vec![deep.clone()]),
            vec![Limit::ADeeperPlace]
        );
        // Not so deep, or a Boolean, or another reason.
        let shallow = traced(predicate_at(0), 1, Some(Representative::Int8));
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, reason, vec![shallow]),
            vec![]
        );
        let boolean = traced(predicate_at(0), 2, Some(Representative::Boolean));
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, reason, vec![boolean]),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, "no", vec![deep.clone()]),
            vec![]
        );
        // A refusal placed outside that predicate.
        let outside = Some(at(&[
            parameter("predicates"),
            Segment::Item(1),
            Segment::Link(0),
        ]));
        assert_eq!(
            polars_refuses(&case, outside, Stage::Schema, reason, vec![deep]),
            vec![]
        );
    }

    #[test]
    fn the_tables_refusing_deep_is_excused_for_a_boolean_and_only_where_deep() {
        let deep = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(0),
            parameter("statement"),
            Segment::Start,
            parameter("predicates"),
            Segment::Item(0),
        ]);
        let case = select(vec![distinct()]);
        let reason = "it cannot end here: expected `Boolean`";
        assert_eq!(
            tables_refuse(&case, deep, Some(TableStage::Schema), reason),
            vec![Limit::ADeeperPlace]
        );
        assert_eq!(
            tables_refuse(&case, in_the_predicate(), Some(TableStage::Schema), reason),
            vec![]
        );
    }

    #[test]
    fn a_hidden_schema_is_excused_inside_an_expression_and_not_at_its_end() {
        let reason = "the schema says Int64 and the rows give UInt128";
        let case = select(vec![chain(column("a"), &["Expr.cum_sum", "Expr.sum"])]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, reason),
            vec![Limit::AHiddenSchema]
        );
        // At the last link of the step's own expression it is the step's schema.
        let last = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            tables_refuse(&last, link_of("exprs", 0, 0), ROWS, reason),
            vec![]
        );
        // Another reason.
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn a_boolean_is_needed_only_of_a_predicate_whose_dtype_was_not_given() {
        let case = filter(vec![chain(column("a"), &[]), chain(column("b"), &[])]);
        let reason = "filter predicate must be of type `Boolean`, got `Int64`";
        let unknown = Traced {
            place: predicate_at(0),
            nested: 1,
            start: Some(Representative::Int64),
            links: Vec::new(),
            told: false,
        };
        assert_eq!(
            polars_refuses(
                &case,
                Some(step_place()),
                Stage::Schema,
                reason,
                vec![unknown.clone()]
            ),
            vec![Limit::ABooleanIsNeededAndTheDtypeUnknown]
        );
        // A predicate with a last link whose dtype the walk worked out, as a
        // condition's, is as unknown to `offer`; one the tables gave is not.
        let worked_out = Traced {
            links: vec![Some(Representative::String)],
            ..unknown.clone()
        };
        assert_eq!(
            polars_refuses(
                &case,
                Some(step_place()),
                Stage::Schema,
                reason,
                vec![worked_out.clone()]
            ),
            vec![Limit::ABooleanIsNeededAndTheDtypeUnknown]
        );
        let told = Traced {
            told: true,
            ..worked_out
        };
        assert_eq!(
            polars_refuses(&case, Some(step_place()), Stage::Schema, reason, vec![told]),
            vec![]
        );
        // A refusal placed in another predicate, and a value that is no predicate.
        let elsewhere = Some(at(&[
            parameter("predicates"),
            Segment::Item(1),
            Segment::Link(0),
        ]));
        assert_eq!(
            polars_refuses(
                &case,
                elsewhere,
                Stage::Schema,
                reason,
                vec![unknown.clone()]
            ),
            vec![]
        );
        let value = Traced {
            place: at(&[
                parameter("predicates"),
                Segment::Item(0),
                Segment::Link(0),
                parameter("statement"),
            ]),
            ..unknown
        };
        assert_eq!(
            polars_refuses(
                &case,
                Some(step_place()),
                Stage::Schema,
                reason,
                vec![value]
            ),
            vec![]
        );
    }

    #[test]
    fn the_tables_refusing_a_start_where_a_boolean_is_needed_is_excused_for_a_predicate() {
        let reason = "it cannot end here: invalid series dtype: expected `Boolean`, got `i64`";
        let case = pivot(chain_of(
            call(
                "polars.when",
                vec![("predicates", many(vec![chain(column("a"), &[])]))],
            ),
            vec![
                call(
                    "When.then",
                    vec![("statement", Value::Expression(chain(column("a"), &[])))],
                ),
                call(
                    "Then.otherwise",
                    vec![("statement", Value::Expression(chain(column("b"), &[])))],
                ),
            ],
        ));
        let predicate = at(&[
            parameter("aggregate_function"),
            Segment::Start,
            parameter("predicates"),
            Segment::Item(0),
        ]);
        assert_eq!(
            tables_refuse(&case, predicate, Some(TableStage::Rows), reason),
            vec![Limit::ABooleanIsNeededAndTheDtypeUnknown]
        );
        let value = at(&[
            parameter("aggregate_function"),
            Segment::Link(0),
            parameter("statement"),
        ]);
        assert_eq!(
            tables_refuse(&case, value, Some(TableStage::Rows), reason),
            vec![Limit::AValueThatRunsOnNoRows]
        );
    }

    #[test]
    fn a_pivot_aggregate_is_excused_by_the_kernel_reason_and_by_the_empty_table_with_one() {
        let case = pivot(chain(column("a"), &["Expr.cum_sum"]));
        let at_the_call = at(&[parameter("aggregate_function"), Segment::Link(0)]);
        assert_eq!(
            tables_refuse(
                &case,
                at_the_call.clone(),
                Some(TableStage::Rows),
                "cannot compare string with numeric type (f64)"
            ),
            vec![Limit::AGroupAggregate]
        );
        assert_eq!(
            tables_refuse(&case, at_the_call, Some(TableStage::Rows), "no"),
            vec![]
        );
        assert_eq!(
            polars_refuses(
                &case,
                None,
                Stage::Empty,
                "`cum_max` operation not supported for dtype `null`",
                Vec::new()
            ),
            vec![Limit::AGroupAggregate]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Empty, "no", Vec::new()),
            vec![]
        );
        // The same reasons in a step that is no pivot.
        let select = select(vec![chain(column("a"), &["Expr.cum_sum"])]);
        assert_eq!(
            polars_refuses(
                &select,
                None,
                Stage::Empty,
                "`cum_max` operation not supported for dtype `null`",
                Vec::new()
            ),
            vec![]
        );
    }
}
