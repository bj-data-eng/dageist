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

use dageist_engine::document::{Call, Evaluation, Expression, Place, Segment, Start, Value, Walk};

use super::case::Case;
use dageist_engine::allowed::Stage as TableStage;

use super::observe::{Observed, Refusal, Stage};
use super::predict::{Prediction, Refused, Source, Verdict, walk_of};

/// A limit of the tables, and what it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// `Expr.over`'s `mapping_strategy` `explode` gives a different number of
    /// rows, which meet another expression's with lengths that do not match. The
    /// tables do not measure what meets them. Polars refusing the rows, with a
    /// reason about lengths, cannot be placed, so the reason and an `explode` in
    /// the case are what gate it; the tables refusing is placed, inside an
    /// expression that holds an `explode`.
    AnExplodeChangesTheLengths,
    /// A comparison or a subtraction with a null literal is null, and Polars does not evaluate
    /// the other side of it, wherever in the expression that comparison is; the
    /// engine's walk says what it folds and what it never evaluates
    /// ([`Walk::evaluation_at`]), and this limit reads that and has no model of
    /// its own. A named rule's refusal of a predicate that a fold drops is this
    /// too. A literal is one value, which Polars broadcasts and casts as it
    /// does not a column: it accepts a text compared with a typed number, or a
    /// window over a constant, where a column of the dtype is refused, and joins
    /// it with a column of other length. The tables measure an argument as a
    /// column of the dtype, except the values of `shift` and a pivot's group.
    ALiteral,
    /// Polars does not evaluate what it does not need for one value: a sort key
    /// that is one value is dropped, and `len` does not read what it counts. The
    /// tables refusing at a call that follows an aggregate in its own chain, or
    /// inside a sort key that is one value, or inside an expression with a `len`
    /// in it, which Polars allows. What the tables say of a method's dtype was
    /// measured in `select` on a column. Of the named rules it excuses only the
    /// refusal of a predicate inside a sort key or a window's `order_by` that is
    /// one value, and inside what a `len` counts in a `select` of one values.
    AnAggregateGivesOneValue,
    /// A pivot's aggregate is run on the values of each group, not on a
    /// column in `select`, and Polars uses other kernels for them: it takes the
    /// `max` of a `Struct` there, which `select` refuses, and compares, casts
    /// and windows what `select` does not. A table with no rows has no groups,
    /// and the values of an aggregate over none have no dtype. The empty table's
    /// refusal is gated by its reason, which has no place; the tables refusing
    /// is placed, in the aggregate, with a reason about a kernel.
    AGroupAggregate,
    /// A key that is wholly a single value, an aggregate or a null, is not one
    /// that has to be elementwise: Polars allows in it what a key may not hold,
    /// such as a running count under a `last`. The tables measured the entry as
    /// the whole key. The tables refusing it, with a reason about elementwise,
    /// placed inside that key. A key that holds an aggregate beside a column, or
    /// a column compared with a null, is refused by a named rule, and is not
    /// this.
    ASingleValueInAKey,
    /// Polars folds a comparison with a null literal into a null, before it asks
    /// whether a join's key is elementwise, and what it leaves depends on the
    /// dtypes and on what the comparison follows, which the tables did not
    /// measure and the named rule, which holds what the evidence settles, does
    /// not model: a column compared with a null is refused by the rule, and an
    /// aggregate compared with a null, or a comparison that follows one, is
    /// found by running. Polars refusing the join as not elementwise, with no
    /// place, in a case whose join key holds a comparison with a null literal at
    /// any depth.
    ANullComparisonInAKey,
    /// A shift that fills nothing leaves nulls, and Polars does not meet a call
    /// with a dtype that the tables refuse when they run rows where what it
    /// follows or is given is only nulls: shifts that total the sample's three
    /// rows or more, an aggregate that leaves one value which a shift moves away,
    /// the first of what a shift moved. A comparison of a list with a `Boolean`
    /// is refused after none, one or two shifts and allowed after three. The
    /// tables refusing, when they run rows, at a call that follows such a chain
    /// or is given one.
    AShiftThatLeavesNulls,
    /// Whether values of different lengths combine, as the lists of two rows
    /// do not, depends on the rows, which the tables measured on one sample.
    TheLengthsOfTheRows,
    /// A debug build of Polars panics where an arithmetic overflow wraps in a
    /// release build, on keys the tables did not measure.
    AnOverflow,
    /// A condition whose values are one expression is simplified to it, and
    /// its predicates are dropped, so what Polars refuses in them it does not. Of
    /// the named rules it excuses only the refusal of what is inside a predicate,
    /// not of the predicate itself, which Polars simplifies away only when it is a
    /// comparison and refuses when it is a column.
    TheBranchesAreOne,
    /// Polars panicking has no place, and is gated by its reason. A panic of
    /// Polars is an internal check that failed. The tables record
    /// one for the plan Polars made of a combination in `select`, and the plan
    /// of another place may not reach it; a combination of what they measured
    /// singly, such as two keys of a window, may reach another.
    APanic,
    /// A condition with a predicate made of literals alone is decided before it
    /// is run: Polars takes the branch it selects and drops the others, so what
    /// the predicates and values it drops would refuse, it does not. A predicate
    /// that only starts from a literal, and goes on to a column, is not this. Of
    /// the named rules it excuses only the refusal of what follows the first value
    /// of a condition that a literal true decides, when that value reads a column
    /// or the whole condition is literals.
    ADecidedCondition,
    /// A value of a condition runs on the rows its predicate selects, and a
    /// predicate after the first on the rows left to it. On the rows of the
    /// sample that may be none, and what would be refused is not raised. Only
    /// what the tables refused when it ran rows is excused; what they refused
    /// when it built the step is refused whatever the rows. Of the named rules it
    /// excuses only the refusal of a later predicate in the `order_by` of a
    /// window, which Polars evaluates on groups and skips where no row reaches it.
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
}

impl Limit {
    /// Every limit, which the kept cases cover each by one.
    pub const ALL: [Self; 16] = [
        Self::AnExplodeChangesTheLengths,
        Self::ALiteral,
        Self::AnAggregateGivesOneValue,
        Self::AGroupAggregate,
        Self::ASingleValueInAKey,
        Self::ANullComparisonInAKey,
        Self::AShiftThatLeavesNulls,
        Self::TheLengthsOfTheRows,
        Self::AnOverflow,
        Self::TheBranchesAreOne,
        Self::APanic,
        Self::AHiddenSchema,
        Self::AWindowOverAWindow,
        Self::ADecidedCondition,
        Self::AValueThatRunsOnNoRows,
        Self::AJoinKeyDtype,
    ];

    /// The name a kept case is filed under.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::AnExplodeChangesTheLengths => "an-explode-changes-the-lengths",
            Self::ALiteral => "a-literal",
            Self::AnAggregateGivesOneValue => "an-aggregate-gives-one-value",
            Self::AGroupAggregate => "a-group-aggregate",
            Self::ASingleValueInAKey => "a-single-value-in-a-key",
            Self::ANullComparisonInAKey => "a-null-comparison-in-a-key",
            Self::AShiftThatLeavesNulls => "a-shift-that-leaves-nulls",
            Self::TheLengthsOfTheRows => "the-lengths-of-the-rows",
            Self::AnOverflow => "an-overflow",
            Self::TheBranchesAreOne => "the-branches-are-one",
            Self::APanic => "a-panic",
            Self::AHiddenSchema => "a-hidden-schema",
            Self::AWindowOverAWindow => "a-window-over-a-window",
            Self::ADecidedCondition => "a-decided-condition",
            Self::AValueThatRunsOnNoRows => "a-value-that-runs-on-no-rows",
            Self::AJoinKeyDtype => "a-join-key-dtype",
        }
    }

    pub const fn statement(self) -> &'static str {
        match self {
            Self::AnExplodeChangesTheLengths => {
                "an explode gives other lengths, which meet those of what follows it, as the tables did not measure"
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
            Self::AShiftThatLeavesNulls => {
                "a shift leaves nulls, which Polars does not meet with the dtype the tables refuse"
            }
            Self::ANullComparisonInAKey => {
                "Polars folds a comparison with a null in a join key into a null, which the rule does not wholly model"
            }
            Self::ASingleValueInAKey => {
                "a key that is wholly a single value, an aggregate or a null, need not be elementwise in what it holds"
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

/// The indexes of the links of `expression` that `found` says yes to.
fn links_where(expression: &Expression, found: &dyn Fn(&Call) -> bool) -> Vec<usize> {
    expression
        .links()
        .iter()
        .enumerate()
        .filter(|(_, link)| found(link))
        .map(|(at, _)| at)
        .collect()
}

/// Whether a place reached `holder` where a `len` does not read what it counts:
/// what the `len` follows, and the arguments of the links before it.
fn reached_by_a_len(holder: &Expression, reach: Reach) -> bool {
    links_where(holder, &|call| is(call, "Expr.len"))
        .into_iter()
        .any(|at| match reach {
            Reach::Whole => at + 1 == holder.links().len(),
            Reach::Start => true,
            Reach::Operand(j) => j < at,
            Reach::At(k) => k <= at,
        })
}

/// Whether a place reached `holder` where an `explode` changes the rows it
/// meets: at the window or after it, and in the arguments of the links after.
fn reached_by_an_explode(holder: &Expression, reach: Reach) -> bool {
    links_where(holder, &has_explode)
        .into_iter()
        .any(|at| match reach {
            Reach::Whole => true,
            Reach::Start => false,
            Reach::Operand(j) => j > at,
            Reach::At(k) => k >= at,
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

/// Where a place is in an expression that holds it: in what the chain starts
/// from, in the arguments of one of its links, at one of its calls, or at the
/// expression as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    Whole,
    Start,
    /// Inside the arguments of the link.
    Operand(usize),
    /// At the call of the link itself.
    At(usize),
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
    /// How the place reaches each of them, in the same order.
    reach: Vec<Reach>,
    /// How many segments of the place it took to enter each of them.
    depths: Vec<usize>,
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
        reach: Vec::new(),
        depths: Vec::new(),
    };
    let mut node = Node::Call(case.document.steps()[0].call());
    for (taken, segment) in place.path().iter().enumerate() {
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
                if let Some(last) = locus.reach.last_mut() {
                    *last = Reach::Start;
                }
                node = match expression.start() {
                    Start::Call(call) => Node::Call(call),
                    Start::Input(_) => return locus,
                };
                continue;
            }
            (Node::Expression(expression), Segment::Link(at)) => {
                locus.chain = Some(expression);
                locus.index = Some(*at);
                if let Some(last) = locus.reach.last_mut() {
                    *last = Reach::Operand(*at);
                }
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
                locus.reach.push(Reach::Whole);
                locus.depths.push(taken + 1);
                // Which call of this expression the place is at is not known
                // until the way says; it is not the link of the chain that holds
                // this one.
                locus.index = None;
                Node::Expression(expression)
            }
            Some(other) => Node::Value(other),
            None => return locus,
        };
    }
    // The path ends at a call: the innermost expression is reached at it.
    if let (Node::Call(_), Some(Reach::Operand(at)), Some(last)) =
        (&node, locus.reach.last().copied(), locus.reach.last_mut())
    {
        *last = Reach::At(at);
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

/// The expressions a call is given, by parameter, and the place of each: the
/// call is at `call`.
fn placed_operands<'a>(call: &'a Call, at: &Place) -> Vec<(&'a str, Place, &'a Expression)> {
    fn collect<'a>(
        parameter: &'a str,
        value: &'a Value,
        place: Place,
        found: &mut Vec<(&'a str, Place, &'a Expression)>,
    ) {
        match value {
            Value::Expression(expression) => found.push((parameter, place, expression)),
            Value::Many(items) => items.iter().enumerate().for_each(|(item, inside)| {
                collect(
                    parameter,
                    inside,
                    place.clone().then(Segment::Item(item)),
                    found,
                );
            }),
            Value::Pairs(pairs) => pairs.iter().enumerate().for_each(|(at, (key, entry))| {
                collect(parameter, key, place.clone().then(Segment::Key(at)), found);
                collect(
                    parameter,
                    entry,
                    place.clone().then(Segment::Value(at)),
                    found,
                );
            }),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for given in call.arguments() {
        let place = at
            .clone()
            .then(Segment::Parameter(given.parameter().to_owned()));
        collect(given.parameter(), given.value(), place, &mut found);
    }
    found
}

/// The keys of the step, each with its place: a join's, if `joins`, and the keys
/// of every window in it.
fn keys(case: &Case, joins: bool) -> Vec<(Place, &Expression)> {
    let step = &case.document.steps()[0];
    let origin = Place::of_step(0, Some(step.id().clone()));
    let mut found: Vec<(Place, &Expression)> = Vec::new();
    if joins && is(step.call(), "LazyFrame.join") {
        found.extend(
            placed_operands(step.call(), &origin)
                .into_iter()
                .filter(|(parameter, _, _)| ["on", "left_on", "right_on"].contains(parameter))
                .map(|(_, place, key)| (place, key)),
        );
    }
    fn windows<'a>(
        expression: &'a Expression,
        place: &Place,
        found: &mut Vec<(Place, &'a Expression)>,
    ) {
        let starts = match expression.start() {
            Start::Call(call) => Some((call, place.clone().then(Segment::Start))),
            Start::Input(_) => None,
        };
        let links = expression
            .links()
            .iter()
            .enumerate()
            .map(|(at, link)| (link, place.clone().then(Segment::Link(at))));
        for (call, at) in starts.into_iter().chain(links) {
            let held = placed_operands(call, &at);
            if is(call, "Expr.over") {
                found.extend(
                    held.iter()
                        .filter(|(parameter, _, _)| {
                            ["partition_by", "more_exprs", "order_by"].contains(parameter)
                        })
                        .map(|(_, key_at, key)| (key_at.clone(), *key)),
                );
            }
            for (_, inside_at, inside) in held {
                windows(inside, &inside_at, found);
            }
        }
    }
    for (_, held_at, held) in placed_operands(step.call(), &origin) {
        windows(held, &held_at, &mut found);
    }
    found
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

/// Whether a refusal by Polars has no place inside the step: it refused the
/// rows or panicked, or refused the step as a whole.
fn unplaced(refusal: &Refusal) -> bool {
    refusal
        .place
        .as_ref()
        .is_none_or(|place| place.path().is_empty())
}

/// Whether the values of a condition are one expression, so that Polars takes it
/// for that expression and drops its predicates.
fn branches_are_one(condition: &Expression) -> bool {
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
}

/// Whether a sort key, or the key of a window, that is wholly one value, holds
/// the place: Polars drops it, and what the tables refuse in it with it.
fn dropped_as_one_value(case: &Case, locus: &Locus) -> bool {
    let step = case.document.steps()[0].call();
    let keys = keys(case, false);
    (is(step, "LazyFrame.sort")
        && locus
            .top
            .is_some_and(|top| wholly_single(top, top.links().len(), true)))
        || locus.inside.iter().any(|holder| {
            keys.iter().any(|(_, key)| std::ptr::eq(*key, *holder))
                && wholly_single(holder, holder.links().len(), true)
        })
}

/// The rows of the sample tables the tables were measured on and the target
/// runs cases on.
const SAMPLE_ROWS: i64 = 3;

/// How many rows the `shift` `link` moves, when it fills them with nothing: a
/// shift of a number that is not written as a whole number, or one given a value
/// to fill with, counts for none.
fn shift_rows(link: &Call) -> i64 {
    if !is(link, "Expr.shift") || link.argument("fill_value").is_some() {
        return 0;
    }
    match link.argument("n") {
        None => 1,
        Some(Value::Expression(n)) if n.links().is_empty() => match n.start() {
            Start::Call(start) if is(start, "polars.lit") => match start.argument("value") {
                Some(Value::Integer(rows)) => rows.abs(),
                _ => 0,
            },
            _ => 0,
        },
        Some(_) => 0,
    }
}

/// Whether `links`, taken from the sample, leave only nulls: shifts that move all
/// its rows, or one value, which an aggregate leaves, shifted away, or the first
/// row that a shift left null.
fn leaves_only_nulls(links: &[Call]) -> bool {
    let (mut rows, mut shifted) = (SAMPLE_ROWS, 0);
    for link in links {
        shifted += shift_rows(link);
        if is(link, "Expr.first")
            && !matches!(link.argument("ignore_nulls"), Some(Value::Flag(Some(true))))
        {
            (rows, shifted) = (1, i64::from(shifted >= 1));
        } else if is_one_value(link) {
            (rows, shifted) = (1, 0);
        }
    }
    shifted >= rows
}

/// Whether a place is inside a later predicate of a condition, in the `order_by`
/// of a window. A window's `order_by` is evaluated on groups, where Polars does
/// not evaluate a predicate that no row reaches, as the rows of the sample
/// decide; `select`, `sort`, `partition_by` and `more_exprs` evaluate it.
fn in_a_later_predicate_of_an_order_by(case: &Case, place: &Place) -> bool {
    keys(case, false).iter().any(|(key, _)| {
        place.path().starts_with(key.path())
            && held_by(key, "order_by")
            && place.path()[key.path().len()..].windows(2).any(|pair| {
                matches!(pair, [Segment::Link(_), Segment::Parameter(name)] if name == "predicates")
            })
    })
}

/// Whether a condition is decided true by a literal that is its only first
/// predicate, and its first value reads a column, or the whole condition is
/// literals: Polars evaluates that value and not what follows it. A first value
/// that is a literal leaves the later predicates to be evaluated.
fn is_true_before_its_value(condition: &Expression) -> bool {
    let Start::Call(start) = condition.start() else {
        return false;
    };
    if !is(start, "polars.when") {
        return false;
    }
    let predicates: Vec<&Expression> = operands(start)
        .into_iter()
        .filter(|(parameter, _)| *parameter == "predicates")
        .map(|(_, predicate)| predicate)
        .collect();
    let decided_true = matches!(predicates.as_slice(), [only]
        if only.links().is_empty()
            && matches!(only.start(), Start::Call(lit)
                if is(lit, "polars.lit")
                    && matches!(lit.argument("value"), Some(Value::Flag(Some(true))))));
    let first_value = condition
        .links()
        .first()
        .filter(|link| is_a_value_link(link))
        .and_then(|link| match link.argument("statement") {
            Some(Value::Expression(value)) => Some(value),
            _ => None,
        });
    decided_true
        && (only_literals(condition)
            || first_value.is_some_and(|value| any_call(value, &|call| is(call, "polars.col"))))
}

/// Whether a `len` is why a place is not evaluated: in a `select` whose every
/// item is one value, Polars does not read what a `len` counts. `with_columns`,
/// `filter` and a `select` with an item that is a column evaluate it.
fn len_is_not_read(case: &Case, locus: &Locus) -> bool {
    let step = case.document.steps()[0].call();
    is(step, "LazyFrame.select")
        && operands(step)
            .into_iter()
            .all(|(_, item)| wholly_single(item, item.links().len(), true))
        && locus
            .inside
            .iter()
            .zip(&locus.reach)
            .any(|(holder, reach)| reached_by_a_len(holder, *reach))
}

/// Whether the expression at `place` is what the parameter `name` of a call is
/// given.
fn held_by(place: &Place, name: &str) -> bool {
    place
        .path()
        .iter()
        .rev()
        .find_map(|segment| match segment {
            Segment::Parameter(held) => Some(held == name),
            _ => None,
        })
        .unwrap_or(false)
}

/// Whether the limit is why Polars allows what the named rule refuses. A limit
/// excuses a rule only for a cause it names, and the evidence for each is a
/// campaign case and a replay of it without the cause: a fold of a null drops
/// the predicate, which the walk says ([`Walk::evaluation_at`]); a sort key that
/// is one value, or a window's `order_by` that is, is dropped, and Polars
/// refuses the same key elsewhere or when it is not one value; a `len` in a
/// `select` of one values is not given what it counts; and a condition that a
/// literal true decides drops what follows its first value; a later predicate
/// in a window's `order_by` is not evaluated where no row reaches it; and what
/// is inside the predicate of branches that are one is not evaluated.
fn applies_to_a_rule(
    limit: Limit,
    rule: &str,
    case: &Case,
    walk: &Walk,
    refused: &Refused,
) -> bool {
    let predicate_rule = matches!(
        rule,
        "condition-predicate-is-boolean" | "filter-predicate-is-boolean"
    );
    match limit {
        Limit::ALiteral => {
            predicate_rule && walk.evaluation_at(&refused.place) == Evaluation::Dropped
        }
        Limit::AValueThatRunsOnNoRows => {
            predicate_rule && in_a_later_predicate_of_an_order_by(case, &refused.place)
        }
        Limit::TheBranchesAreOne => {
            let locus = locate(case, &refused.place);
            // Polars never evaluates the predicate of such a condition, so what is
            // inside it is not refused; the predicate itself is.
            predicate_rule
                && locus.inside.iter().enumerate().any(|(at, holder)| {
                    locus
                        .in_predicates
                        .iter()
                        .any(|condition| std::ptr::eq(*condition, *holder))
                        && branches_are_one(holder)
                        && locus
                            .depths
                            .get(at + 1)
                            .is_some_and(|entered| refused.place.path().len() > *entered)
                })
        }
        Limit::ADecidedCondition => {
            let locus = locate(case, &refused.place);
            predicate_rule
                && locus
                    .inside
                    .iter()
                    .zip(&locus.reach)
                    .any(|(holder, reach)| {
                        matches!(reach, Reach::Operand(at) | Reach::At(at) if *at >= 1)
                            && is_true_before_its_value(holder)
                    })
        }
        Limit::AnAggregateGivesOneValue => {
            let locus = locate(case, &refused.place);
            // A sort key that is one value is dropped, and so is a window's
            // `order_by` that is one value, so no predicate in it is evaluated;
            // the `partition_by` and `more_exprs` of a window are evaluated.
            let sort_key = is(case.document.steps()[0].call(), "LazyFrame.sort")
                && locus
                    .top
                    .is_some_and(|top| wholly_single(top, top.links().len(), true));
            let order_by = keys(case, false).iter().any(|(place, key)| {
                refused.place.path().starts_with(place.path())
                    && held_by(place, "order_by")
                    && wholly_single(key, key.links().len(), true)
            });
            let dropped = sort_key || order_by;
            predicate_rule && (dropped || len_is_not_read(case, &locus))
        }
        _ => false,
    }
}

/// Whether the limit is why the tables and Polars disagree.
fn applies(limit: Limit, case: &Case, walk: &Walk, disagreement: &Disagreement) -> bool {
    // A rule is refused for a cause of its own, which only the limit that names
    // it can explain.
    if let Disagreement::PolarsAllows(refused) = disagreement
        && let Source::Rule(rule) = refused.source
    {
        return applies_to_a_rule(limit, rule, case, walk, refused);
    }
    let step = case.document.steps()[0].call();
    let pivot = is(step, "LazyFrame.pivot");
    let held = operands(step);
    let explodes_in = |expression: &Expression| any_call(expression, &has_explode);
    match (limit, disagreement) {
        (Limit::AnExplodeChangesTheLengths, Disagreement::PolarsRefuses(refusal)) => {
            unplaced(refusal)
                && is_lengths(&refusal.reason)
                && held.iter().any(|(_, held)| explodes_in(held))
        }
        (Limit::AnExplodeChangesTheLengths, Disagreement::PolarsAllows(refused)) => {
            // An `explode` changes the rows that what follows it meets, which
            // Polars refuses with a reason about lengths.
            is_lengths(&refused.reason) && {
                let locus = locate(case, &refused.place);
                locus
                    .inside
                    .iter()
                    .zip(&locus.reach)
                    .any(|(holder, reach)| reached_by_an_explode(holder, *reach))
            }
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
            // evaluate what it follows or what it is given: the walk says where.
            let folded = walk.evaluation_at(&refused.place) != Evaluation::Evaluated;
            // What a call that follows a literal is given, at any depth, is met by
            // one value, which Polars broadcasts: a comparison of a literal with
            // what the tables refuse when they run rows is not refused.
            let given_to_a_literal = refused.stage == Some(TableStage::Rows)
                && locus
                    .inside
                    .iter()
                    .zip(&locus.reach)
                    .any(|(holder, reach)| {
                        matches!(reach, Reach::Operand(_)) && starts_from_a_literal(holder)
                    });
            chain_starts || operand || folded || given_to_a_literal
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
            let dropped = dropped_as_one_value(case, &locus);
            // `len` does not read what it counts.
            let counted = locus
                .inside
                .iter()
                .zip(&locus.reach)
                .any(|(holder, reach)| reached_by_a_len(holder, *reach));
            receiver || dropped || counted
        }
        (Limit::AGroupAggregate, Disagreement::PolarsRefuses(refusal)) => {
            // A table with no rows has no groups, and the values of an
            // aggregate over none have no dtype.
            pivot
                && refusal.stage == Stage::Empty
                && (refusal.reason.contains("not supported")
                    || refusal.reason.contains("got `null`")
                    || refusal.reason.contains("got null"))
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
        (Limit::ANullComparisonInAKey, Disagreement::PolarsRefuses(refusal)) => {
            is(step, "LazyFrame.join")
                && unplaced(refusal)
                && refusal.reason.contains("elementwise")
                && {
                    // Another key may be why Polars refuses the join: one that
                    // holds a running value, an aggregate or a window.
                    let keys = keys(case, true);
                    let other_cause = |key: &Expression| {
                        any_call(key, &|call| {
                            is_one_value(call)
                                || matches!(
                                    call.entry().id().as_str(),
                                    "Expr.over"
                                        | "Expr.cum_sum"
                                        | "Expr.cum_max"
                                        | "Expr.cum_count"
                                )
                        })
                    };
                    // Whether the walk says a comparison with a null folds in it.
                    let folds = |place: &Place| {
                        walk.expressions()
                            .iter()
                            .any(|walked| walked.place() == place && walked.folds_a_null())
                    };
                    keys.iter().any(|(place, _)| folds(place))
                        && keys
                            .iter()
                            .filter(|(place, _)| !folds(place))
                            .all(|(_, key)| !other_cause(key))
                }
        }
        (Limit::AShiftThatLeavesNulls, Disagreement::PolarsAllows(refused)) => {
            let locus = locate(case, &refused.place);
            // What the call follows, or is given, is only nulls.
            refused.stage == Some(TableStage::Rows)
                && (locus.chain.zip(locus.index).is_some_and(|(chain, at)| {
                    leaves_only_nulls(&chain.links()[..at.min(chain.links().len())])
                }) || locus.call.is_some_and(|call| {
                    operands(call)
                        .into_iter()
                        .any(|(_, operand)| leaves_only_nulls(operand.links()))
                }))
        }
        (Limit::ASingleValueInAKey, Disagreement::PolarsAllows(refused)) => {
            // The key the tables refused is wholly one value.
            refused.reason.contains("elementwise")
                && locate(case, &refused.place).top.is_some_and(|top| {
                    keys(case, true)
                        .iter()
                        .any(|(_, key)| std::ptr::eq(*key, top))
                        && any_call(top, &is_a_single_value)
                        && wholly_single(top, top.links().len(), true)
                })
        }
        (Limit::AnOverflow, Disagreement::PolarsRefuses(refusal)) => {
            unplaced(refusal) && refusal.reason.contains("with overflow")
        }
        (Limit::TheBranchesAreOne, Disagreement::PolarsAllows(refused)) => {
            locate(case, &refused.place)
                .in_predicates
                .iter()
                .any(|condition| branches_are_one(condition))
        }
        (Limit::APanic, Disagreement::PolarsRefuses(refusal)) => {
            unplaced(refusal)
                && is_panic(&refusal.reason)
                && !refusal.reason.contains("with overflow")
        }
        (Limit::APanic, Disagreement::PolarsAllows(refused)) => is_panic(&refused.reason),
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
        (Verdict::Refuses(refused), None)
            if matches!(refused.source, Source::Polars | Source::Rule(_)) =>
        {
            Disagreement::PolarsAllows(refused)
        }
        _ => return Vec::new(),
    };
    let walk = walk_of(case);
    Limit::ALL
        .into_iter()
        .filter(|limit| applies(*limit, case, &walk, &disagreement))
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

    /// A named rule refuses the case at `place`, and Polars allows it.
    fn rule_refuses(case: &Case, place: Place, rule: &'static str, reason: &str) -> Vec<Limit> {
        let verdict = Verdict::Refuses(Refused {
            source: Source::Rule(rule),
            at: place.to_string(),
            place,
            stage: None,
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
    fn what_a_call_after_a_literal_is_given_is_excused_at_any_depth_when_rows_refuse_it() {
        // `1.cum_sum().le(b.max())`: the tables refuse `max` of a list when they
        // run rows, and Polars meets it with one value.
        let item = chain_of(
            literal(),
            vec![
                call("Expr.cum_sum", Vec::new()),
                compared_with("Expr.le", chain(column("b"), &["Expr.max"])),
            ],
        );
        let case = select(vec![item]);
        let inside = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(1),
            parameter("other"),
            Segment::Link(0),
        ]);
        assert_eq!(
            tables_refuse(&case, inside.clone(), ROWS, "no"),
            vec![Limit::ALiteral]
        );
        // What the tables refuse when they built the step is refused.
        assert_eq!(
            tables_refuse(&case, inside, Some(TableStage::Schema), "no"),
            vec![]
        );
        // Nothing starts from a literal.
        let plain = chain_of(
            column("a"),
            vec![
                call("Expr.cum_sum", Vec::new()),
                compared_with("Expr.le", chain(column("b"), &["Expr.max"])),
            ],
        );
        let inside = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(1),
            parameter("other"),
            Segment::Link(0),
        ]);
        assert_eq!(
            tables_refuse(&select(vec![plain]), inside, ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn a_comparison_that_follows_a_comparison_with_a_null_compares_a_null() {
        // `null == 1 <= b.max()`: the first comparison is a null, so the second
        // is one, and what it is given is not evaluated.
        let item = chain_of(
            null(),
            vec![
                compared_with("Expr.eq", chain(literal(), &[])),
                compared_with("Expr.le", chain(column("b"), &["Expr.max"])),
            ],
        );
        let case = select(vec![item]);
        let inside = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(1),
            parameter("other"),
            Segment::Link(0),
        ]);
        assert_eq!(
            tables_refuse(&case, inside, ROWS, "no"),
            vec![Limit::ALiteral]
        );
        // With no null before it, the second comparison evaluates its argument.
        let plain = chain_of(
            column("a"),
            vec![
                compared_with("Expr.eq", chain(literal(), &[])),
                compared_with("Expr.le", chain(column("b"), &["Expr.max"])),
            ],
        );
        let inside = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(1),
            parameter("other"),
            Segment::Link(0),
        ]);
        assert_eq!(
            tables_refuse(&select(vec![plain]), inside, ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn the_empty_table_of_a_pivot_is_excused_for_a_null_dtype_however_it_is_quoted() {
        let case = pivot(chain(column("a"), &["Expr.cum_sum"]));
        for reason in [
            "invalid series dtype: expected `Duration`, got `null` for series with name `a`",
            "expected Date or Datetime, got null",
        ] {
            assert_eq!(
                polars_refuses(&case, None, Stage::Empty, reason, Vec::new()),
                vec![Limit::AGroupAggregate],
                "{reason}"
            );
        }
    }

    // ----- Rules that refuse what Polars allows

    /// A bare literal true, false or null, as an expression.
    fn boolean(value: Option<bool>) -> Expression {
        chain(call("polars.lit", vec![("value", Value::Flag(value))]), &[])
    }

    fn when_of(predicates: Vec<Expression>) -> Call {
        call("polars.when", vec![("predicates", many(predicates))])
    }

    fn then_of(value: Expression) -> Call {
        call("When.then", vec![("statement", Value::Expression(value))])
    }

    fn otherwise_of(value: Expression) -> Call {
        call(
            "Then.otherwise",
            vec![("statement", Value::Expression(value))],
        )
    }

    fn also_when(predicates: Vec<Expression>) -> Call {
        call("Then.when", vec![("predicates", many(predicates))])
    }

    /// The place of predicate `at` of the start of item 0.
    fn start_predicate(at: usize) -> Place {
        self::at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Start,
            parameter("predicates"),
            Segment::Item(at),
        ])
    }

    /// What `f` is compared with: `f`, or `f.min() > null`, which folds into a
    /// null.
    fn operand(folds: bool) -> Expression {
        if folds {
            chain_of(
                column("f"),
                vec![
                    call("Expr.min", Vec::new()),
                    compared_with("Expr.gt", chain(null(), &[])),
                ],
            )
        } else {
            chain(column("f"), &[])
        }
    }

    const BOOLEAN_RULE: &str = "condition-predicate-is-boolean";

    #[test]
    fn a_fold_that_drops_the_predicate_is_the_cause_and_its_absence_is_not() {
        // `when(g).then(g).first() == operand`: Polars refuses the text `g`
        // unless the operand folds into a null, and then never evaluates what the
        // comparison follows. The same holds with a decided predicate beside `g`,
        // one-valued branches, and a `len`, none of which is the cause.
        let g = || chain(column("g"), &[]);
        let cases: Vec<(Vec<Call>, Vec<Expression>)> = vec![
            // The condition of the reviewed case: `g` is its only predicate.
            (
                vec![then_of(g()), call("Expr.first", Vec::new())],
                vec![g()],
            ),
            // A literal true decides it, and `g` is the second predicate.
            (
                vec![then_of(g()), call("Expr.first", Vec::new())],
                vec![g(), boolean(Some(true))],
            ),
            // Branches that are one expression.
            (
                vec![
                    then_of(chain(column("f"), &[])),
                    otherwise_of(chain(column("f"), &[])),
                    call("Expr.first", Vec::new()),
                ],
                vec![g()],
            ),
            // A `len` follows the predicate. A `select` of one values does not
            // read what it counts, so `with_columns` holds it here.
            (
                vec![
                    then_of(chain(column("f"), &[])),
                    call("Expr.len", Vec::new()),
                ],
                vec![g()],
            ),
        ];
        for (at, (links, predicates)) in cases.into_iter().enumerate() {
            let item = |folds: bool| {
                let mut all = links.clone();
                all.push(compared_with("Expr.eq", operand(folds)));
                let items = vec![chain_of(when_of(predicates.clone()), all)];
                if at == 3 {
                    case_of(call("LazyFrame.with_columns", vec![("exprs", many(items))]))
                } else {
                    select(items)
                }
            };
            assert_eq!(
                rule_refuses(&item(true), start_predicate(0), BOOLEAN_RULE, "a rule"),
                vec![Limit::ALiteral],
                "{at}: the fold drops it"
            );
            assert_eq!(
                rule_refuses(&item(false), start_predicate(0), BOOLEAN_RULE, "a rule"),
                vec![],
                "{at}: nothing drops it"
            );
        }
    }

    #[test]
    fn a_len_in_a_select_of_one_values_excuses_a_predicate_rule_and_nowhere_else() {
        // `select([when(p).then(1).when(g).then(2).otherwise(3).len()])`: Polars
        // does not evaluate what a `len` counts where every item of a `select` is
        // one value, whether or not a literal predicate decides the condition. It
        // evaluates it in `with_columns`, beside an item that is a column, and
        // without the `len`.
        let value = |at: i64| chain(literal_of(at), &[]);
        let item = |first: Expression, len: bool| {
            let mut links = vec![
                then_of(value(1)),
                also_when(vec![chain(column("g"), &[])]),
                call(
                    "ChainedWhen.then",
                    vec![("statement", Value::Expression(value(2)))],
                ),
                call(
                    "ChainedThen.otherwise",
                    vec![("statement", Value::Expression(value(3)))],
                ),
            ];
            if len {
                links.push(call("Expr.len", Vec::new()));
            }
            chain_of(when_of(vec![first]), links)
        };
        let places = [
            start_predicate(0),
            at(&[
                parameter("exprs"),
                Segment::Item(0),
                Segment::Link(1),
                parameter("predicates"),
                Segment::Item(0),
            ]),
        ];
        let predicates = [
            ("a column", chain(column("k"), &[])),
            ("a decided condition", boolean(Some(true))),
        ];
        for (name, first) in predicates {
            for place in &places {
                let case = select(vec![item(first.clone(), true)]);
                assert_eq!(
                    rule_refuses(&case, place.clone(), BOOLEAN_RULE, "a rule"),
                    vec![Limit::AnAggregateGivesOneValue],
                    "{name}"
                );
                // No `len`.
                let case = select(vec![item(first.clone(), false)]);
                assert_eq!(
                    rule_refuses(&case, place.clone(), BOOLEAN_RULE, "a rule"),
                    vec![],
                    "{name}: no len"
                );
                // `with_columns` evaluates it.
                let case = case_of(call(
                    "LazyFrame.with_columns",
                    vec![("exprs", many(vec![item(first.clone(), true)]))],
                ));
                assert_eq!(
                    rule_refuses(&case, place.clone(), BOOLEAN_RULE, "a rule"),
                    vec![],
                    "{name}: with_columns"
                );
                // An item that is a column beside it.
                let case = select(vec![item(first.clone(), true), chain(column("h"), &[])]);
                assert_eq!(
                    rule_refuses(&case, place.clone(), BOOLEAN_RULE, "a rule"),
                    vec![],
                    "{name}: beside a column"
                );
            }
        }
    }

    #[test]
    fn a_condition_decided_true_drops_what_follows_a_first_value_that_reads_a_column() {
        // `when(true).then(a).when(g).then(2).otherwise(3)`: Polars evaluates only
        // the first value, and not the later predicate `g`, where that value
        // reads a column, or where the whole condition is literals. A first value
        // that is a literal leaves the later predicate to be evaluated, and so
        // does a condition decided false or one that `g` starts.
        let value = |at: i64| chain(literal_of(at), &[]);
        let condition = |first: Expression, truthy: Expression, later: Expression| {
            chain_of(
                when_of(vec![first]),
                vec![
                    then_of(truthy),
                    also_when(vec![later]),
                    call(
                        "ChainedWhen.then",
                        vec![("statement", Value::Expression(value(2)))],
                    ),
                    call(
                        "ChainedThen.otherwise",
                        vec![("statement", Value::Expression(value(3)))],
                    ),
                ],
            )
        };
        let g = || chain(column("g"), &[]);
        let later_predicate = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(1),
            parameter("predicates"),
            Segment::Item(0),
        ]);
        let literals_only = || chain(literal_of(5), &[]);
        let excused = [
            (
                "a column first value",
                condition(boolean(Some(true)), chain(column("a"), &[]), g()),
            ),
            (
                "literals alone",
                condition(boolean(Some(true)), value(1), literals_only()),
            ),
        ];
        for (name, item) in excused {
            assert_eq!(
                rule_refuses(
                    &select(vec![item]),
                    later_predicate.clone(),
                    BOOLEAN_RULE,
                    "a rule"
                ),
                vec![Limit::ADecidedCondition],
                "{name}"
            );
        }
        let refused = [
            (
                "a literal first value",
                condition(boolean(Some(true)), value(1), g()),
            ),
            (
                "decided false",
                condition(boolean(Some(false)), chain(column("a"), &[]), g()),
            ),
            (
                "decided null",
                condition(boolean(None), chain(column("a"), &[]), g()),
            ),
        ];
        for (name, item) in refused {
            assert_eq!(
                rule_refuses(
                    &select(vec![item]),
                    later_predicate.clone(),
                    BOOLEAN_RULE,
                    "a rule"
                ),
                vec![],
                "{name}"
            );
        }
        // The first predicate itself, and the first value of the condition.
        let item = condition(boolean(Some(true)), chain(column("a"), &[]), g());
        assert_eq!(
            rule_refuses(
                &select(vec![item]),
                start_predicate(0),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![]
        );
        // `when(true).then(when(true).then(a).when(g).then(2).otherwise(3))`: the
        // inner condition is the first value of the outer one and is decided
        // true itself; one that starts from `g` is not.
        let nested = |first: Expression| {
            select(vec![chain_of(
                when_of(vec![boolean(Some(true))]),
                vec![
                    then_of(condition(first, chain(column("a"), &[]), g())),
                    otherwise_of(value(1)),
                ],
            )])
        };
        let inner_later = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(0),
            parameter("statement"),
            Segment::Link(1),
            parameter("predicates"),
            Segment::Item(0),
        ]);
        assert_eq!(
            rule_refuses(
                &nested(boolean(Some(true))),
                inner_later.clone(),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![Limit::ADecidedCondition]
        );
        assert_eq!(
            rule_refuses(&nested(g()), inner_later, BOOLEAN_RULE, "a rule"),
            vec![]
        );
    }

    #[test]
    fn a_later_predicate_in_a_window_order_by_is_excused_for_rows_it_may_not_reach() {
        // `a.over(order_by=[when(h.is_null()).then(k).when(g.is_not_null()).then(k)
        // .when(h).then(1)])`: a window's `order_by` is evaluated on groups, where
        // Polars does not evaluate a predicate that no row reaches, as the rows
        // of the sample decide. It evaluates the same condition in `select`, in
        // `sort`, and in a window's `partition_by` and `more_exprs`.
        let condition = || {
            chain_of(
                when_of(vec![chain(column("h"), &["Expr.is_null"])]),
                vec![
                    then_of(chain(column("k"), &[])),
                    also_when(vec![chain(column("g"), &["Expr.is_not_null"])]),
                    call(
                        "ChainedWhen.then",
                        vec![("statement", Value::Expression(chain(column("k"), &[])))],
                    ),
                    call(
                        "ChainedThen.when",
                        vec![("predicates", many(vec![chain(column("h"), &[])]))],
                    ),
                    call(
                        "ChainedWhen.then",
                        vec![("statement", Value::Expression(chain(literal_of(1), &[])))],
                    ),
                ],
            )
        };
        let window = |parameter_name: &str| {
            let keys = vec![(parameter_name, many(vec![condition()]))];
            let mut given = vec![("partition_by", many(vec![chain(column("k"), &[])]))];
            given.retain(|(name, _)| *name != parameter_name);
            given.extend(keys);
            select(vec![chain_of(column("a"), vec![call("Expr.over", given)])])
        };
        let in_the_window = |parameter_name: &str, link: usize| {
            at(&[
                parameter("exprs"),
                Segment::Item(0),
                Segment::Link(0),
                parameter(parameter_name),
                Segment::Item(0),
                Segment::Link(link),
                parameter("predicates"),
                Segment::Item(0),
            ])
        };
        // The third predicate, and the second: the rows left to them may be none.
        for link in [1, 3] {
            assert_eq!(
                rule_refuses(
                    &window("order_by"),
                    in_the_window("order_by", link),
                    BOOLEAN_RULE,
                    "a rule"
                ),
                vec![Limit::AValueThatRunsOnNoRows],
                "link {link}"
            );
        }
        // The first predicate is always evaluated.
        let first = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(0),
            parameter("order_by"),
            Segment::Item(0),
            Segment::Start,
            parameter("predicates"),
            Segment::Item(0),
        ]);
        assert_eq!(
            rule_refuses(&window("order_by"), first, BOOLEAN_RULE, "a rule"),
            vec![]
        );
        for other in ["partition_by", "more_exprs"] {
            assert_eq!(
                rule_refuses(
                    &window(other),
                    in_the_window(other, 3),
                    BOOLEAN_RULE,
                    "a rule"
                ),
                vec![],
                "{other}"
            );
        }
        let plain = select(vec![condition()]);
        let later = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(3),
            parameter("predicates"),
            Segment::Item(0),
        ]);
        assert_eq!(rule_refuses(&plain, later, BOOLEAN_RULE, "a rule"), vec![]);
        let sorted = sort(vec![condition()]);
        let later = at(&[
            parameter("by"),
            Segment::Item(0),
            Segment::Link(3),
            parameter("predicates"),
            Segment::Item(0),
        ]);
        assert_eq!(rule_refuses(&sorted, later, BOOLEAN_RULE, "a rule"), vec![]);
    }

    #[test]
    fn a_predicate_inside_the_predicate_of_branches_that_are_one_is_excused_for_a_rule() {
        // `when(when(b).then(null) <= 1).then(h).otherwise(h)`: Polars simplifies
        // a condition whose values are one expression to it, when its predicate
        // is a comparison, and never evaluates the predicate, so what is inside it
        // is not refused. The predicate itself, a column, is refused still, and
        // so is the same condition with values that differ or one value only.
        let inner = || {
            chain_of(
                when_of(vec![chain(column("b"), &[])]),
                vec![
                    then_of(chain(null(), &[])),
                    compared_with("Expr.le", chain(literal_of(1), &[])),
                ],
            )
        };
        let outer = |first: Expression, last: Option<Expression>| {
            let mut links = vec![then_of(chain(column("h"), &[]))];
            links.extend(last.map(otherwise_of));
            select(vec![chain_of(when_of(vec![first]), links)])
        };
        let h = || chain(column("h"), &[]);
        let in_the_inner_predicate = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Start,
            parameter("predicates"),
            Segment::Item(0),
            Segment::Start,
            parameter("predicates"),
            Segment::Item(0),
        ]);
        assert_eq!(
            rule_refuses(
                &outer(inner(), Some(h())),
                in_the_inner_predicate.clone(),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![Limit::TheBranchesAreOne]
        );
        // Values that differ, and a condition with one value.
        for last in [Some(chain(column("f"), &[])), None] {
            assert_eq!(
                rule_refuses(
                    &outer(inner(), last),
                    in_the_inner_predicate.clone(),
                    BOOLEAN_RULE,
                    "a rule"
                ),
                vec![]
            );
        }
        // The predicate itself.
        assert_eq!(
            rule_refuses(
                &outer(chain(column("b"), &[]), Some(h())),
                start_predicate(0),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![]
        );
    }

    /// `when(h.is_null()).then(k).when(g.is_not_null()).then(k).when(h).then(1)`.
    fn three_predicates() -> Expression {
        chain_of(
            when_of(vec![chain(column("h"), &["Expr.is_null"])]),
            vec![
                then_of(chain(column("k"), &[])),
                also_when(vec![chain(column("g"), &["Expr.is_not_null"])]),
                call(
                    "ChainedWhen.then",
                    vec![("statement", Value::Expression(chain(column("k"), &[])))],
                ),
                call(
                    "ChainedThen.when",
                    vec![("predicates", many(vec![chain(column("h"), &[])]))],
                ),
                call(
                    "ChainedWhen.then",
                    vec![("statement", Value::Expression(chain(literal_of(1), &[])))],
                ),
            ],
        )
    }

    #[test]
    fn a_sort_runs_the_partition_by_of_a_window_on_groups_and_other_steps_do_not() {
        // `sort(by=[h.over(partition_by=[when(h.is_null()).then(k)
        // .when(g.is_not_null()).then(k).when(h).then(1)])])`: Polars evaluates
        // the keys of a window in a sort on groups, where a predicate that no row
        // reaches is not evaluated, as the rows of the sample decide. `select`,
        // `with_columns` and `filter` evaluate them, and so does a sort for the
        // `more_exprs` of the window.
        let window = |entry: &str, parameter_name: &str| {
            let keys = many(vec![three_predicates()]);
            let given = vec![(parameter_name, keys)];
            let item = chain_of(column("h"), vec![call("Expr.over", given)]);
            match entry {
                "LazyFrame.sort" => case_of(call(entry, vec![("by", many(vec![item]))])),
                "LazyFrame.filter" => case_of(call(entry, vec![("predicates", many(vec![item]))])),
                _ => case_of(call(entry, vec![("exprs", many(vec![item]))])),
            }
        };
        let in_the_key = |outer: &str, parameter_name: &str| {
            at(&[
                parameter(outer),
                Segment::Item(0),
                Segment::Link(0),
                parameter(parameter_name),
                Segment::Item(0),
                Segment::Link(3),
                parameter("predicates"),
                Segment::Item(0),
            ])
        };
        assert_eq!(
            rule_refuses(
                &window("LazyFrame.sort", "partition_by"),
                in_the_key("by", "partition_by"),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![Limit::AValueThatRunsOnNoRows]
        );
        for (entry, outer) in [
            ("LazyFrame.select", "exprs"),
            ("LazyFrame.with_columns", "exprs"),
            ("LazyFrame.filter", "predicates"),
        ] {
            assert_eq!(
                rule_refuses(
                    &window(entry, "partition_by"),
                    in_the_key(outer, "partition_by"),
                    BOOLEAN_RULE,
                    "a rule"
                ),
                vec![],
                "{entry}"
            );
        }
        assert_eq!(
            rule_refuses(
                &window("LazyFrame.sort", "more_exprs"),
                in_the_key("by", "more_exprs"),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![]
        );
    }

    /// A whole number of the dtype of `literal`, as a literal call.
    fn literal_of(value: i64) -> Call {
        call("polars.lit", vec![("value", Value::Integer(value))])
    }

    #[test]
    fn a_predicate_rule_is_excused_by_neither_a_decided_condition_nor_branches_that_are_one() {
        // Polars refuses the text `g` as a predicate of a condition that a literal
        // decides, and of one whose values are one expression, and in a value or a
        // later predicate that no row reaches.
        let g = || chain(column("g"), &[]);
        let one = || chain(literal_of(1), &[]);
        let two = || chain(literal_of(2), &[]);
        let decided = select(vec![chain_of(
            when_of(vec![boolean(Some(true))]),
            vec![
                then_of(chain_of(
                    when_of(vec![g()]),
                    vec![then_of(one()), otherwise_of(two())],
                )),
                otherwise_of(one()),
            ],
        )]);
        let inner_predicate = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(0),
            parameter("statement"),
            Segment::Start,
            parameter("predicates"),
            Segment::Item(0),
        ]);
        assert_eq!(
            rule_refuses(&decided, inner_predicate.clone(), BOOLEAN_RULE, "a rule"),
            vec![]
        );
        // Values that are one expression.
        let branches = select(vec![chain_of(
            when_of(vec![g()]),
            vec![then_of(one()), otherwise_of(one())],
        )]);
        assert_eq!(
            rule_refuses(&branches, start_predicate(0), BOOLEAN_RULE, "a rule"),
            vec![]
        );
        // A value that a row selected by another predicate reaches, and a later
        // predicate.
        let later = select(vec![chain_of(
            when_of(vec![chain_of(
                column("k"),
                vec![compared_with("Expr.gt", chain(literal_of(5), &[]))],
            )]),
            vec![
                then_of(chain_of(
                    when_of(vec![g()]),
                    vec![then_of(one()), otherwise_of(two())],
                )),
                otherwise_of(one()),
            ],
        )]);
        assert_eq!(
            rule_refuses(&later, inner_predicate, BOOLEAN_RULE, "a rule"),
            vec![]
        );
    }

    #[test]
    fn a_rule_refusing_a_predicate_in_a_sort_key_that_is_one_value_is_excused_and_elsewhere_not() {
        // `sort(by=[when(b).then(g).min()])`: Polars drops a sort key that is one
        // value, so the text predicate is not evaluated, and refuses the same key
        // in `select`, in a window's key, and when the key is not one value.
        let key = || {
            chain_of(
                when_of(vec![chain(column("b"), &[])]),
                vec![
                    then_of(chain(column("g"), &[])),
                    call("Expr.min", Vec::new()),
                ],
            )
        };
        let predicate = |parameter_name: &str, item: usize| {
            at(&[
                parameter(parameter_name),
                Segment::Item(item),
                Segment::Start,
                parameter("predicates"),
                Segment::Item(0),
            ])
        };
        assert_eq!(
            rule_refuses(
                &sort(vec![key()]),
                predicate("by", 0),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![Limit::AnAggregateGivesOneValue]
        );
        // Each key is dropped by itself.
        let beside = sort(vec![chain(column("f"), &[]), key()]);
        assert_eq!(
            rule_refuses(&beside, predicate("by", 1), BOOLEAN_RULE, "a rule"),
            vec![Limit::AnAggregateGivesOneValue]
        );
        assert_eq!(
            rule_refuses(
                &select(vec![key()]),
                predicate("exprs", 0),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![]
        );
        let not_one_value = sort(vec![chain_of(
            when_of(vec![chain(column("b"), &[])]),
            vec![then_of(chain(column("g"), &[]))],
        )]);
        assert_eq!(
            rule_refuses(&not_one_value, predicate("by", 0), BOOLEAN_RULE, "a rule"),
            vec![]
        );
        let window = select(vec![chain_of(
            column("f"),
            vec![call("Expr.over", vec![("partition_by", many(vec![key()]))])],
        )]);
        let in_the_window = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(0),
            parameter("partition_by"),
            Segment::Item(0),
            Segment::Start,
            parameter("predicates"),
            Segment::Item(0),
        ]);
        assert_eq!(
            rule_refuses(&window, in_the_window, BOOLEAN_RULE, "a rule"),
            vec![]
        );
    }

    #[test]
    fn a_rule_refusing_a_predicate_in_a_window_order_by_that_is_one_value_is_excused_and_elsewhere_not()
     {
        // `f.over(order_by=[when(g).then(k).len()])`: Polars drops an `order_by`
        // that is one value, so the text predicate is not evaluated; it evaluates
        // the same key in `partition_by` and in `more_exprs`, and an `order_by`
        // that is not one value.
        let single = || {
            chain_of(
                when_of(vec![chain(column("g"), &[])]),
                vec![
                    then_of(chain(column("k"), &[])),
                    call("Expr.len", Vec::new()),
                ],
            )
        };
        let window = |parameter_name: &str, keys: Vec<Expression>| {
            select(vec![chain_of(
                column("f"),
                vec![call("Expr.over", vec![(parameter_name, many(keys))])],
            )])
        };
        let predicate = |parameter_name: &str, item: usize| {
            at(&[
                parameter("exprs"),
                Segment::Item(0),
                Segment::Link(0),
                parameter(parameter_name),
                Segment::Item(item),
                Segment::Start,
                parameter("predicates"),
                Segment::Item(0),
            ])
        };
        assert_eq!(
            rule_refuses(
                &window("order_by", vec![single()]),
                predicate("order_by", 0),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![Limit::AnAggregateGivesOneValue]
        );
        // Each key is dropped by itself.
        assert_eq!(
            rule_refuses(
                &window("order_by", vec![chain(column("k"), &[]), single()]),
                predicate("order_by", 1),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![Limit::AnAggregateGivesOneValue]
        );
        for other in ["partition_by", "more_exprs"] {
            assert_eq!(
                rule_refuses(
                    &window(other, vec![single()]),
                    predicate(other, 0),
                    BOOLEAN_RULE,
                    "a rule"
                ),
                vec![],
                "{other}"
            );
        }
        let not_one_value = chain_of(
            when_of(vec![chain(column("g"), &[])]),
            vec![then_of(chain(column("k"), &[]))],
        );
        assert_eq!(
            rule_refuses(
                &window("order_by", vec![not_one_value]),
                predicate("order_by", 0),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![]
        );
    }

    #[test]
    fn a_rule_refusing_a_literal_start_or_a_fold_is_excused_by_nothing_else() {
        let from_a_literal = select(vec![chain(literal(), &["Expr.cum_sum"])]);
        assert_eq!(
            rule_refuses(
                &from_a_literal,
                at(&[parameter("exprs"), Segment::Item(0)]),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![]
        );
        // The item that holds a comparison with a null is not what it drops.
        let folded = select(vec![chain_of(
            column("a"),
            vec![compared_with("Expr.eq", chain(null(), &[]))],
        )]);
        assert_eq!(
            rule_refuses(
                &folded,
                at(&[parameter("exprs"), Segment::Item(0)]),
                BOOLEAN_RULE,
                "a rule"
            ),
            vec![]
        );
    }

    #[test]
    fn a_rule_refusing_a_held_expression_is_not_excused_by_the_link_that_holds_it() {
        // The predicate `k.sum()` is one value, but it is held by a condition
        // that is the operand of link 1 of the item, and the place is the
        // predicate itself, not a call that follows one value.
        let condition = chain_of(
            call(
                "polars.when",
                vec![("predicates", many(vec![chain(column("k"), &["Expr.sum"])]))],
            ),
            vec![call(
                "When.then",
                vec![("statement", Value::Expression(chain(column("a"), &[])))],
            )],
        );
        let item = chain_of(
            column("a"),
            vec![
                call("Expr.cum_sum", Vec::new()),
                compared_with("Expr.eq", condition),
            ],
        );
        let case = select(vec![item]);
        let predicate = at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(1),
            parameter("other"),
            Segment::Start,
            parameter("predicates"),
            Segment::Item(0),
        ]);
        assert_eq!(
            rule_refuses(&case, predicate, "condition-predicate-is-boolean", "a rule"),
            vec![]
        );
    }

    #[test]
    fn a_rule_refusing_a_whole_key_is_excused_by_no_limit_that_does_not_name_it() {
        // `true.first() <= (null != true) > b` and the same from a column: the
        // rule that a join's key is elementwise is refused for what the key holds,
        // which no literal start, null comparison or aggregate in it explains.
        let key = |start: Call| {
            chain_of(
                start,
                vec![
                    call("Expr.first", Vec::new()),
                    compared_with(
                        "Expr.le",
                        chain_of(
                            null(),
                            vec![compared_with("Expr.ne", chain(literal(), &[]))],
                        ),
                    ),
                    compared_with("Expr.gt", chain(column("b"), &[])),
                ],
            )
        };
        let rule = "key-is-elementwise";
        for start in [literal(), column("f")] {
            let case = join(key(start), chain(column("rf"), &[]));
            let place = at(&[parameter("left_on"), Segment::Item(0)]);
            assert_eq!(rule_refuses(&case, place, rule, "a rule"), vec![]);
        }
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
        // What the links before the `len` are given, it does not count either.
        let given = select(vec![chain_of(
            column("a"),
            vec![
                compared_with("Expr.eq", chain(column("b"), &["Expr.cum_sum"])),
                call("Expr.len", Vec::new()),
            ],
        )]);
        assert_eq!(
            tables_refuse(&given, in_operand(0, "other", 0), ROWS, "no"),
            vec![Limit::AnAggregateGivesOneValue]
        );
    }

    #[test]
    fn a_len_before_an_unrelated_later_operand_excuses_nothing() {
        let case = select(vec![chain_of(
            column("a"),
            vec![
                call("Expr.len", Vec::new()),
                compared_with("Expr.sub", chain(column("b"), &["Expr.cum_sum"])),
            ],
        )]);
        assert_eq!(
            tables_refuse(&case, in_operand(1, "other", 0), ROWS, "no"),
            vec![]
        );
        // The `len` itself, and what it follows, are excused.
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
            vec![Limit::AnAggregateGivesOneValue]
        );
    }

    // ----- A null literal folds a comparison

    #[test]
    fn a_comparison_with_a_null_excuses_what_it_follows_and_is_given_and_not_later_operands() {
        // `a.cum_sum().eq(null).sub(b.cum_sum())`
        let case = select(vec![chain_of(
            column("a"),
            vec![
                call("Expr.cum_sum", Vec::new()),
                compared_with("Expr.eq", chain(null(), &[])),
                compared_with("Expr.sub", chain(column("b"), &["Expr.cum_sum"])),
            ],
        )]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
            vec![Limit::ALiteral]
        );
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 2), ROWS, "no"),
            vec![Limit::ALiteral]
        );
        // The argument of the subtraction after it is evaluated.
        assert_eq!(
            tables_refuse(&case, in_operand(2, "other", 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn a_comparison_given_a_column_that_a_null_follows_excuses_that_column() {
        // `null.eq(b.cum_sum())`: the null is what the comparison follows.
        let case = select(vec![chain_of(
            null(),
            vec![compared_with(
                "Expr.eq",
                chain(column("b"), &["Expr.cum_sum"]),
            )],
        )]);
        assert_eq!(
            tables_refuse(&case, in_operand(0, "other", 0), ROWS, "no"),
            vec![Limit::ALiteral]
        );
    }

    #[test]
    fn a_null_comparison_in_another_item_excuses_nothing() {
        let case = select(vec![
            chain(column("a"), &["Expr.cum_sum"]),
            chain_of(
                column("b"),
                vec![compared_with("Expr.eq", chain(null(), &[]))],
            ),
        ]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, "no"),
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

    /// The place of link `inner` of the argument `name` of link `link` of item 0
    /// of `exprs`.
    fn in_operand(link: usize, name: &str, inner: usize) -> Place {
        at(&[
            parameter("exprs"),
            Segment::Item(0),
            Segment::Link(link),
            parameter(name),
            Segment::Link(inner),
        ])
    }

    const LENGTHS: &str = "lengths don't match: zip node received non-equal length inputs";

    #[test]
    fn an_explode_excuses_what_the_window_and_what_follows_it_meet_and_not_what_is_before() {
        // Window at link 1: the call after it, and the arguments of the links
        // after it, meet its rows.
        let case = select(vec![chain_of(
            column("a"),
            vec![
                call("Expr.cum_sum", Vec::new()),
                explode(),
                compared_with("Expr.eq", chain(column("b"), &["Expr.cum_sum"])),
            ],
        )]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 2), ROWS, LENGTHS),
            vec![Limit::AnExplodeChangesTheLengths]
        );
        assert_eq!(
            tables_refuse(&case, in_operand(2, "other", 0), ROWS, LENGTHS),
            vec![Limit::AnExplodeChangesTheLengths]
        );
        // Before the window, or in its own arguments, they are not.
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 0), ROWS, LENGTHS),
            vec![]
        );
        assert_eq!(
            tables_refuse(&case, in_operand(1, "partition_by", 0), ROWS, LENGTHS),
            vec![]
        );
        // Another reason is not about the rows an explode changes.
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 2), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn an_explode_in_an_unrelated_later_operand_or_item_excuses_nothing() {
        let later = select(vec![chain_of(
            column("a"),
            vec![
                call("Expr.cum_sum", Vec::new()),
                compared_with("Expr.eq", chain_of(column("b"), vec![explode()])),
            ],
        )]);
        assert_eq!(
            tables_refuse(&later, link_of("exprs", 0, 0), ROWS, LENGTHS),
            vec![]
        );
        let elsewhere = select(vec![
            chain(column("a"), &["Expr.cum_sum"]),
            chain_of(column("b"), vec![explode()]),
        ]);
        assert_eq!(
            tables_refuse(&elsewhere, link_of("exprs", 0, 0), ROWS, LENGTHS),
            vec![]
        );
    }

    #[test]
    fn a_refusal_about_lengths_is_the_explodes_or_the_rows_by_whether_there_is_an_explode() {
        let reason = "lengths don't match: zip node received non-equal length inputs";
        let with = select(vec![chain_of(column("a"), vec![explode()])]);
        assert_eq!(
            polars_refuses(&with, None, Stage::Rows, reason, Vec::new()),
            vec![Limit::AnExplodeChangesTheLengths]
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
    fn a_join_key_with_a_null_comparison_at_any_depth_excuses_the_refused_step() {
        let reason = "all join key expressions must be elementwise.";
        // The comparison is one level down, in an operand, and before another.
        let nested = chain_of(
            column("b"),
            vec![compared_with(
                "Expr.le",
                chain_of(
                    column("a"),
                    vec![compared_with("Expr.gt", chain(null(), &[]))],
                ),
            )],
        );
        let after_one = chain_of(
            column("rb"),
            vec![
                call("Expr.max", Vec::new()),
                compared_with("Expr.le", chain(null(), &[])),
                compared_with("Expr.eq", chain(column("rg"), &[])),
            ],
        );
        for key in [nested, after_one] {
            let case = join(key, chain(column("key"), &[]));
            assert_eq!(
                polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
                vec![Limit::ANullComparisonInAKey]
            );
        }
        // A key that compares nothing with a null, and another reason.
        let plain = join(
            chain(column("a"), &["Expr.cum_max"]),
            chain(column("key"), &[]),
        );
        assert_eq!(
            polars_refuses(
                &plain,
                Some(step_place()),
                Stage::Schema,
                reason,
                Vec::new()
            ),
            vec![]
        );
        let case = join(
            chain_of(
                column("a"),
                vec![compared_with("Expr.gt", chain(null(), &[]))],
            ),
            chain(column("key"), &[]),
        );
        assert_eq!(
            polars_refuses(&case, Some(step_place()), Stage::Schema, "no", Vec::new()),
            vec![]
        );
    }

    #[test]
    fn shifts_that_total_the_sample_excuse_the_tables_refusing_what_follows_them() {
        let shift = |rows: Option<i64>| {
            call(
                "Expr.shift",
                rows.map(|rows| vec![("n", Value::Expression(chain_with_integer(rows)))])
                    .unwrap_or_default(),
            )
        };
        let compare = compared_with("Expr.gt", chain(column("b"), &[]));
        let case = |shifts: Vec<Call>| {
            let mut links = shifts;
            links.push(compare.clone());
            select(vec![chain_of(column("a"), links)])
        };
        let at_the_comparison = |shifts: usize| link_of("exprs", 0, shifts);
        // Three shifts of one row, or one of three, empty the sample.
        let three = case(vec![shift(None), shift(None), shift(None)]);
        assert_eq!(
            tables_refuse(&three, at_the_comparison(3), ROWS, "no"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        let one_of_three = case(vec![shift(Some(3))]);
        assert_eq!(
            tables_refuse(&one_of_three, at_the_comparison(1), ROWS, "no"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        // Fewer leave rows, and the tables' refusal stands.
        let two = case(vec![shift(None), shift(None)]);
        assert_eq!(
            tables_refuse(&two, at_the_comparison(2), ROWS, "no"),
            vec![]
        );
        // The rows are filled, or the refusal is of the schema, or is before them.
        let filled = case(vec![
            shift(None),
            shift(None),
            call(
                "Expr.shift",
                vec![("fill_value", Value::Expression(chain(literal(), &[])))],
            ),
        ]);
        assert_eq!(
            tables_refuse(&filled, at_the_comparison(3), ROWS, "no"),
            vec![]
        );
        assert_eq!(
            tables_refuse(&three, at_the_comparison(3), Some(TableStage::Schema), "no"),
            vec![]
        );
        assert_eq!(
            tables_refuse(&three, at_the_comparison(1), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn what_a_shift_leaves_null_excuses_the_tables_refusing_a_call_it_is_given_to() {
        // `b.first().shift()` is a null, and `k.shift().first()` is the null
        // that the shift left first: a comparison with either is met with
        // nothing, which Polars allows where the tables refuse the dtype.
        let given = |operand: Expression| {
            select(vec![chain_of(
                column("g"),
                vec![compared_with("Expr.lt", operand)],
            )])
        };
        let shift = |links: &[&str]| {
            let mut all: Vec<Call> = Vec::new();
            for id in links {
                all.push(call(id, Vec::new()));
            }
            all
        };
        let at_the_call = link_of("exprs", 0, 0);
        let first_then_shift = given(chain_of(column("b"), shift(&["Expr.first", "Expr.shift"])));
        assert_eq!(
            tables_refuse(&first_then_shift, at_the_call.clone(), ROWS, "no"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        let shift_then_first = given(chain_of(column("k"), shift(&["Expr.shift", "Expr.first"])));
        assert_eq!(
            tables_refuse(&shift_then_first, at_the_call.clone(), ROWS, "no"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        // The first that skips nulls, a shift after a first that is not one, and
        // a first with no shift are not null.
        let skipping = given(chain_of(
            column("k"),
            vec![
                call("Expr.shift", Vec::new()),
                call(
                    "Expr.first",
                    vec![("ignore_nulls", Value::Flag(Some(true)))],
                ),
            ],
        ));
        assert_eq!(
            tables_refuse(&skipping, at_the_call.clone(), ROWS, "no"),
            vec![]
        );
        let plain = given(chain_of(column("k"), shift(&["Expr.first"])));
        assert_eq!(
            tables_refuse(&plain, at_the_call.clone(), ROWS, "no"),
            vec![]
        );
        let last = given(chain_of(column("k"), shift(&["Expr.shift", "Expr.last"])));
        assert_eq!(tables_refuse(&last, at_the_call, ROWS, "no"), vec![]);
    }

    /// A whole number as an expression: a literal of it.
    fn chain_with_integer(rows: i64) -> Expression {
        chain(
            call("polars.lit", vec![("value", Value::Integer(rows))]),
            &[],
        )
    }

    #[test]
    fn a_null_comparison_in_one_key_does_not_excuse_what_another_key_may_have_caused() {
        let reason = "all join key expressions must be elementwise.";
        let compared = chain_of(
            column("f"),
            vec![compared_with("Expr.gt", chain(null(), &[]))],
        );
        // The other key holds a running value, an aggregate or a window, any of
        // which may be why Polars refuses the join.
        for other in [
            chain(column("rb"), &["Expr.cum_max"]),
            chain(column("rb"), &["Expr.sum"]),
        ] {
            let case = case_of(call(
                "LazyFrame.join",
                vec![
                    ("other", Value::Table("other".to_owned())),
                    ("left_on", many(vec![compared.clone(), other])),
                    ("right_on", many(vec![chain(column("rk"), &[]); 2])),
                ],
            ));
            assert_eq!(
                polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
                vec![]
            );
        }
        // With the other key plain it is excused.
        let case = join(compared, chain(column("key"), &[]));
        assert_eq!(
            polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
            vec![Limit::ANullComparisonInAKey]
        );
    }

    #[test]
    fn the_tables_refusing_inside_a_key_that_is_wholly_one_value_are_excused() {
        let reason = "all join key expressions must be elementwise.";
        let whole = chain(column("a"), &["Expr.cum_max", "Expr.max"]);
        let plain = chain(column("a"), &["Expr.cum_max"]);
        let case = join(whole.clone(), plain.clone());
        let refuse = |case: &Case, parameter: &str, reason: &str| {
            tables_refuse(
                case,
                link_of(parameter, 0, 0),
                Some(TableStage::Schema),
                reason,
            )
        };
        assert_eq!(
            refuse(&case, "left_on", reason),
            vec![Limit::ASingleValueInAKey]
        );
        // The other key is not one value, so what is refused in it is refused.
        assert_eq!(refuse(&join(plain, whole), "left_on", reason), vec![]);
        // Another reason.
        assert_eq!(refuse(&case, "left_on", "no"), vec![]);
    }

    #[test]
    fn a_key_that_holds_a_single_value_beside_a_column_is_not_excused() {
        // That key is refused by a rule, and Polars refuses it too.
        let reason = "all join key expressions must be elementwise.";
        let mixed = chain_of(
            column("f"),
            vec![
                call("Expr.cum_max", Vec::new()),
                compared_with("Expr.le", chain(column("x"), &["Expr.max"])),
            ],
        );
        let case = join(mixed, chain(column("key"), &[]));
        assert_eq!(
            tables_refuse(
                &case,
                link_of("left_on", 0, 0),
                Some(TableStage::Schema),
                reason
            ),
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
            polars_refuses(
                &case,
                None,
                Stage::Empty,
                "invalid series dtype: expected `Duration`, got `null` for series with name `a`",
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
