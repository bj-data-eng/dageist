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
//!
//! A limit is for a disagreement a probe cannot settle. Where the tables refuse a
//! part and Polars allows the step, the target first asks Polars about the part
//! alone ([`super::probe`]): a part that Polars does not evaluate is not a limit
//! of the tables, for the editor holds every part to its types. A limit is for
//! the other direction, where the tables allow and Polars refuses, and for what
//! Polars evaluates and accepts, which no probe of the part shows.

use dageist_engine::document::{Call, Expression, Place, Segment, Start, Value};

use super::case::Case;
use dageist_engine::allowed::Stage as TableStage;

use super::observe::{Observed, Refusal, Stage};
use super::predict::{Prediction, Refused, Source, Verdict};

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
    /// A literal's length over a window is a constant to Polars, which does not
    /// evaluate what follows a comparison of a value with it. A literal is one
    /// value, which Polars broadcasts and casts as it does not a column: it accepts a text compared with a typed number, or a
    /// window over a constant, where a column of the dtype is refused, and joins
    /// it with a column of other length. The tables measure an argument as a
    /// column of the dtype, except the values of `shift` and a pivot's group.
    ALiteral,
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
    /// Polars panicking has no place, and is gated by its reason. A panic of
    /// Polars is an internal check that failed. The tables record
    /// one for the plan Polars made of a combination in `select`, and the plan
    /// of another place may not reach it; a combination of what they measured
    /// singly, such as two keys of a window, may reach another.
    APanic,
    /// The other table's key is made of the dtype its chain gives on its own,
    /// and the join may resolve the chain to another, as the precision of a
    /// decimal that a null is taken from. Polars' message names the decimal, and
    /// the refusal is of the step.
    AJoinKeyDtype,
    /// A window whose receiver holds another window is run by Polars in the
    /// aggregation context of the outer one, where a key that is not elementwise
    /// is refused; the tables measured one window. The receiver is what comes
    /// before the outer `over` in its chain, with the arguments of those calls.
    /// Polars refuses the rows, so there is no place and the reason gates it.
    AWindowOverAWindow,
}

impl Limit {
    /// Every limit, which the kept cases cover each by one.
    pub const ALL: [Self; 11] = [
        Self::AnExplodeChangesTheLengths,
        Self::ALiteral,
        Self::AGroupAggregate,
        Self::ASingleValueInAKey,
        Self::ANullComparisonInAKey,
        Self::AShiftThatLeavesNulls,
        Self::TheLengthsOfTheRows,
        Self::AnOverflow,
        Self::APanic,
        Self::AWindowOverAWindow,
        Self::AJoinKeyDtype,
    ];

    /// The name a kept case is filed under.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::AnExplodeChangesTheLengths => "an-explode-changes-the-lengths",
            Self::ALiteral => "a-literal",
            Self::AGroupAggregate => "a-group-aggregate",
            Self::ASingleValueInAKey => "a-single-value-in-a-key",
            Self::ANullComparisonInAKey => "a-null-comparison-in-a-key",
            Self::AShiftThatLeavesNulls => "a-shift-that-leaves-nulls",
            Self::TheLengthsOfTheRows => "the-lengths-of-the-rows",
            Self::AnOverflow => "an-overflow",
            Self::APanic => "a-panic",
            Self::AWindowOverAWindow => "a-window-over-a-window",
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
            Self::AnOverflow => {
                "a debug build of Polars panics on an overflow of the values, which the tables measured on one sample"
            }
            Self::APanic => {
                "a panic of Polars depends on the plan it makes, which another place may not make"
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
/// holds that it is inside, and the chain and the call it is at.
struct Locus<'a> {
    /// The expression the step holds, outermost.
    top: Option<&'a Expression>,
    /// The expression that holds the call, innermost.
    chain: Option<&'a Expression>,
    /// The link the place is at, `None` for the start.
    index: Option<usize>,
    call: Option<&'a Call>,
    /// Every expression the place is inside, outermost first.
    inside: Vec<&'a Expression>,
    /// How the place reaches each of them, in the same order.
    reach: Vec<Reach>,
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
        inside: Vec::new(),
        reach: Vec::new(),
    };
    let mut node = Node::Call(case.document.steps()[0].call());
    for segment in place.path() {
        let value = match (&node, segment) {
            (Node::Call(call), Segment::Parameter(name)) => call.argument(name),
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

/// Whether a key holds, at any depth, a comparison with a null literal: one that
/// is given it, or one that follows it.
fn holds_a_null_comparison(key: &Expression) -> bool {
    let comparisons = [
        "Expr.eq", "Expr.ne", "Expr.gt", "Expr.ge", "Expr.lt", "Expr.le",
    ];
    let is_null = |expression: &Expression| {
        expression.links().is_empty()
            && matches!(expression.start(), Start::Call(start)
                if is(start, "polars.lit") && matches!(start.argument("value"), Some(Value::Flag(None))))
    };
    let compares = |expression: &Expression| {
        let null_start = matches!(expression.start(), Start::Call(start)
            if is(start, "polars.lit") && matches!(start.argument("value"), Some(Value::Flag(None))));
        expression.links().iter().enumerate().any(|(at, link)| {
            comparisons.contains(&link.entry().id().as_str())
                && ((null_start && at == 0)
                    || operands(link)
                        .into_iter()
                        .any(|(_, operand)| is_null(operand)))
        })
    };
    fn walk(expression: &Expression, compares: &dyn Fn(&Expression) -> bool) -> bool {
        let calls = std::iter::once(expression.start())
            .filter_map(|start| match start {
                Start::Call(call) => Some(call),
                Start::Input(_) => None,
            })
            .chain(expression.links());
        compares(expression)
            || calls.into_iter().any(|call| {
                operands(call)
                    .into_iter()
                    .any(|(_, inside)| walk(inside, compares))
            })
    }
    walk(key, &compares)
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

/// Whether the refused call follows a comparison of a value with the length of a
/// literal over a window, which Polars takes for a constant: it does not evaluate
/// what follows, where the tables refuse it when they run rows. The value is one
/// value, as an aggregate is; a sum, a window by a column and no window are
/// evaluated.
fn after_a_constant_window(locus: &Locus, refused: &Refused) -> bool {
    let length_over_a_window = |operand: &Expression| {
        starts_from_a_literal(operand)
            && links_where(operand, &|link| is(link, "Expr.len"))
                .first()
                .is_some_and(|len| {
                    links_where(operand, &|link| is(link, "Expr.over"))
                        .first()
                        .is_some_and(|over| over > len)
                })
            && wholly_single(operand, operand.links().len(), true)
    };
    refused.stage == Some(TableStage::Rows)
        && locus.chain.zip(locus.index).is_some_and(|(chain, at)| {
            (0..at.min(chain.links().len())).any(|compared| {
                operands(&chain.links()[compared])
                    .into_iter()
                    .any(|(_, operand)| length_over_a_window(operand))
                    && wholly_single(chain, compared, true)
            })
        })
}

/// Whether the limit is why the tables and Polars disagree.
fn applies(limit: Limit, case: &Case, disagreement: &Disagreement) -> bool {
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
            chain_starts
                || operand
                || given_to_a_literal
                || after_a_constant_window(&locus, refused)
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
                    keys.iter().any(|(_, key)| holds_a_null_comparison(key))
                        && keys
                            .iter()
                            .filter(|(_, key)| !holds_a_null_comparison(key))
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
        (Verdict::Refuses(refused), None) if matches!(refused.source, Source::Polars) => {
            Disagreement::PolarsAllows(refused)
        }
        _ => return Vec::new(),
    };
    Limit::ALL
        .into_iter()
        .filter(|limit| applies(*limit, case, &disagreement))
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
    fn what_follows_an_aggregate_compared_with_the_length_of_a_literal_over_a_window_is_excused() {
        // `a.min() <= 1.len().over(order_by=[true])` then `.total_days()`: Polars
        // takes the length of a literal over a window for a constant, and does not
        // evaluate what follows the comparison of a value with it, where the tables
        // refuse a `Boolean`. It evaluates it for a sum, for a window by a column,
        // for a column that is not one value, and for no window.
        let window = |length: &str, order_by: Expression| {
            chain_of(
                literal(),
                vec![
                    call(length, Vec::new()),
                    call("Expr.over", vec![("order_by", many(vec![order_by]))]),
                ],
            )
        };
        let item = |start: Vec<Call>, operand: Expression| {
            let mut links = start;
            links.push(compared_with("Expr.le", operand));
            links.push(call("Expr.dt.total_days", Vec::new()));
            select(vec![chain_of(column("a"), links)])
        };
        let min = || vec![call("Expr.min", Vec::new())];
        let constant = || chain(literal(), &[]);
        let at_total_days = |links: usize| link_of("exprs", 0, links);
        let case = item(min(), window("Expr.len", constant()));
        assert_eq!(
            tables_refuse(&case, at_total_days(2), ROWS, "no"),
            vec![Limit::ALiteral]
        );
        // What the tables refused when they built the step, and the comparison.
        assert_eq!(
            tables_refuse(&case, at_total_days(2), Some(TableStage::Schema), "no"),
            vec![]
        );
        for (name, other) in [
            ("a sum", item(min(), window("Expr.sum", constant()))),
            (
                "a window by a column",
                item(min(), window("Expr.len", chain(column("k"), &[]))),
            ),
            ("no window", item(min(), chain(literal(), &["Expr.len"]))),
            (
                "a column that is not one value",
                item(Vec::new(), window("Expr.len", constant())),
            ),
        ] {
            let links = if name == "a column that is not one value" {
                1
            } else {
                2
            };
            assert_eq!(
                tables_refuse(&other, at_total_days(links), ROWS, "no"),
                vec![],
                "{name}"
            );
        }
        // The calls before the comparison are not what follows it.
        assert_eq!(tables_refuse(&case, at_total_days(0), ROWS, "no"), vec![]);
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
    fn a_case_that_two_limits_explain_is_excused_by_both() {
        // A literal starts the chain, and shifts leave only nulls.
        let shift = call(
            "Expr.shift",
            vec![("n", Value::Expression(chain_with_integer(3)))],
        );
        let case = select(vec![chain_of(
            literal(),
            vec![shift, call("Expr.cum_sum", Vec::new())],
        )]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 1), ROWS, "no"),
            vec![Limit::ALiteral, Limit::AShiftThatLeavesNulls]
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

    // ----- Places, schemas, Booleans, pivots

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
