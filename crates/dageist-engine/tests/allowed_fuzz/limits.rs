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
//! that holds what the limit is about. Where the feature may be there beside
//! another cause, Polars is asked: the case is run again without the feature, with
//! no `explode` or with each literal a column of its dtype, and the limit excuses
//! it only where that shows the feature is why. Polars says where only when it
//! refused while it built the step and worked out its schema, and then not always: a
//! refusal of the whole step has the step for its place. A refusal of the rows
//! (the empty table's too) or a panic has none, and for those the reason and
//! the feature in the case are what there is; a limit that can only be gated so
//! says it in its documentation, and its tests gate the reason. A feature
//! elsewhere in the case excuses nothing. Whether a limit applies does not
//! depend on the order the limits are asked in, and a case that two apply to is
//! excused by both.
//!
//! A limit is never for a part Polars skips. Where the tables refuse a part and
//! Polars allows the step, the target first asks Polars about the part as it is
//! written ([`super::probe`]): a part that Polars does not evaluate is not a limit
//! of the tables, for the editor holds every part to its types. A limit is for what
//! Polars evaluates and accepts, which the part alone does not show, such as a
//! literal it broadcasts or the nulls that the sample's rows leave, and for the
//! other direction, where the tables allow and Polars refuses. The limits are
//! asked after the part as written and before the probes apart from the case,
//! which cannot tell what Polars evaluates from what it skips.

use dageist_engine::allowed::Stage as TableStage;
use dageist_engine::catalog;
use dageist_engine::document::{Call, Document, Expression, Place, Segment, Start, Step, Value};

use super::case::Case;
use super::frames::Frames;
use super::literal::{
    LiteralCause, call_with_columns, class_of, expression_with_columns, operand_causes,
    placed_operands, starts_from_a_literal,
};
use super::observe::{Observed, Refusal, SCHEMA_SAYS, Stage, Tables, observe, observe_probe};
use super::predict::{Prediction, Refused, Source, Verdict};
use super::probe::{same_reason, settle_in_the_position};

/// A limit of the tables, and what it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// `Expr.over`'s `mapping_strategy` `explode` gives a different number of
    /// rows, which meet another expression's with lengths that do not match. The
    /// tables do not measure what meets them. Polars refusing the rows, with a
    /// reason about lengths, cannot be placed, so the reason gates it, and an
    /// `explode` in the case is its cause only where Polars stops refusing for
    /// lengths when the case is run again with each `explode` undone; the tables
    /// refusing is placed, inside an expression that holds an `explode`.
    AnExplodeChangesTheLengths,
    /// A literal is one value, which Polars broadcasts and casts as it does not a
    /// column: it accepts a text compared with a typed number, or a window over a
    /// constant, where a column of the dtype is refused, and joins it with a
    /// column of other length. The tables measure an argument as a column of the
    /// dtype, except the values of `shift` and a pivot's group. The tables refusing
    /// at a call that follows a literal or is given one, or, when they run rows,
    /// inside what a call after a literal is given, which Polars allows. The
    /// literal is why only where Polars refuses the case for the tables' reason
    /// once each literal at that place is a column of the dtype Polars gave it, and
    /// nothing else is changed: Polars allowing the column too is a part it skips
    /// for another cause, which a probe or another limit settles. A pivot's
    /// aggregate reads no column to stand in, so it is not this.
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
    /// The tables ran every call on columns that hold values, so what Polars does
    /// with a call that has only nulls to meet was not measured: it does not meet
    /// the dtype that the tables refuse when they run rows. A shift fills with a
    /// null unless it is given a value, and the sample's three rows decide what
    /// is left: the target follows the rows of the sample through the chain. A
    /// start column holds a value in each row, and a null in the middle one for
    /// `a` and `b`. A shift moves the rows down for a positive number and up for a
    /// negative one, and the rows it empties hold its `fill_value` (a null where
    /// there is none). A running sum or maximum, and the parts of a date, keep
    /// each row where it is and null where it was; `is_null`, `is_not_null` and
    /// `cum_count` give a value in every row; a comparison, a subtraction, an
    /// `offset_by`, a condition and a window that is not an `explode` keep the
    /// rows in place and leave what they hold unknown, and give the one value that
    /// an aggregate leaves to each of the sample's rows; any other call leaves
    /// nothing known. The first and the last take a row (the first or last that
    /// is not null, where they skip nulls), the minimum and the maximum are null
    /// of rows that are all null and a value of any that holds one, and a sum is
    /// not known of rows that are all null. The tables refusing, when they run
    /// rows, at a call that follows a chain whose rows are all null, or is given
    /// one. A comparison of a list with a `Boolean` is refused after none, one or
    /// two shifts of the sample and allowed after three. Nulls that Polars folds
    /// whatever the rows are, as those of a comparison with a null, are no matter
    /// of the sample: a probe settles them.
    AShiftThatLeavesNulls,
    /// Whether values of different lengths combine, as the lists of two rows
    /// do not, depends on the rows, which the tables measured on one sample.
    /// Polars refusing the rows with a reason about lengths, in a case with no
    /// `explode`, or one that Polars refuses for lengths still when each `explode`
    /// is undone.
    TheLengthsOfTheRows,
    /// A debug build of Polars panics where an arithmetic overflow wraps in a
    /// release build, on keys the tables did not measure.
    AnOverflow,
    /// Polars raises an integer to a power by taking the exponent as a `UInt32`, so
    /// it refuses a negative whole number as the exponent of an integer when the rows
    /// run: the number is a value, which the tables measured as the dtype and the
    /// number 1. Polars refusing the rows with a failed conversion to `u32`, which
    /// has no place, in a case that holds a `pow` whose exponent is a whole number
    /// typed below zero or holds a subtraction, a negation, a not, a sum or a product,
    /// which can take the values of a column below zero.
    ANegativeExponent,
    /// A column or a literal of the dtype Null is an operand of `and`, `or` and `xor`
    /// that the tables allow, giving a `Boolean`, as `select` shows. Polars treats it
    /// in ways `select` hides. It negates what a bitwise call combines operand by
    /// operand, as the negation of a conjunction is the disjunction of the negations,
    /// so a `not` meets each operand, which it refuses for the dtype Null. Polars
    /// refusing the rows with the reason that the dtype Null is not supported in `not`,
    /// which has no place, in a case that holds a `not` straight after an `and` or an
    /// `or`. It gives a filter's predicate the dtype Null, which Polars refuses with
    /// the reason that a predicate must be a `Boolean`, in a filter that holds one.
    ABitwiseCallOnANull,
    /// A `UInt128` is compared with a `Decimal` by casting it to `decimal[38,0]`,
    /// which holds up to 38 digits, and a `UInt128` holds more. The sample's values
    /// are within the decimal, so the tables did not measure what a value beyond
    /// it meets, which a subtraction that wraps below zero makes. Polars refusing
    /// the rows with a failed conversion of `u128` to a decimal, which has no
    /// place, in a case that holds a subtraction.
    AValueBeyondADecimal,
    /// Polars refuses to divide a `Decimal` by zero, where a whole number divided by
    /// zero is null and a float is infinite: the divisor is a value, which the tables
    /// measured as the dtype and the sample's values, none of them zero. A remainder
    /// or a difference of the sample's values can be. Polars refusing the rows with
    /// the reason that it divides by zero, which has no place, in a case that holds a
    /// division, a floor division or a remainder.
    ADivisionByZero,
    /// Polars panicking has no place, and is gated by its reason. A panic of
    /// Polars is an internal check that failed. The tables record
    /// one for the plan Polars made of a combination in `select`, and the plan
    /// of another place may not reach it; a combination of what they measured
    /// singly, such as two keys of a window, may reach another.
    APanic,
    /// The other table's key is made of the dtype its chain gives on its own,
    /// and the join may resolve the chain to another, as the precision of a
    /// decimal that a null is taken from, or leave it unresolved: an integer raised
    /// to a number with a fraction typed without a dtype is a `Float64` where it is
    /// selected and a `dyn float`, which no dtype matches, where it is a key. Polars'
    /// message names the decimal or the `dyn float`, the latter in a case that holds
    /// such a power, and the refusal is of the step.
    AJoinKeyDtype,
    /// A window whose receiver holds another window is run by Polars in the
    /// aggregation context of the outer one, where a key that is not elementwise
    /// is refused; the tables measured one window. The receiver is what comes
    /// before the outer `over` in its chain, with the arguments of those calls.
    /// Polars refuses the rows, so there is no place and the reason gates it.
    AWindowOverAWindow,
    /// The tables hold the schema of a method and what its rows give as one thing,
    /// and refuse a method whose plan says another dtype than its rows give: the
    /// target's own comparison, not a refusal of Polars, which shows only where the
    /// method is the output of the step. Inside an expression that goes on from it
    /// or compares it, Polars evaluates the method and the step's rows do not show
    /// the difference; and a sort key or a join's key is never an output of the step,
    /// which a probe of the call on a column in the key may show refused as well, and
    /// then settles it. The tables refusing, with the reason that the schema says one
    /// dtype and the rows give another, at a call that is not the last of the step's
    /// own expression or in a key that no probe settles, which Polars allows.
    AHiddenSchema,
}

impl Limit {
    /// Every limit, which the kept cases cover each by one.
    pub const ALL: [Self; 16] = [
        Self::AnExplodeChangesTheLengths,
        Self::ALiteral,
        Self::AGroupAggregate,
        Self::ASingleValueInAKey,
        Self::ANullComparisonInAKey,
        Self::AShiftThatLeavesNulls,
        Self::TheLengthsOfTheRows,
        Self::AnOverflow,
        Self::ANegativeExponent,
        Self::ABitwiseCallOnANull,
        Self::ADivisionByZero,
        Self::AValueBeyondADecimal,
        Self::APanic,
        Self::AWindowOverAWindow,
        Self::AJoinKeyDtype,
        Self::AHiddenSchema,
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
            Self::ANegativeExponent => "a-negative-exponent",
            Self::ABitwiseCallOnANull => "a-bitwise-call-on-a-null",
            Self::ADivisionByZero => "a-division-by-zero",
            Self::AValueBeyondADecimal => "a-value-beyond-a-decimal",
            Self::APanic => "a-panic",
            Self::AWindowOverAWindow => "a-window-over-a-window",
            Self::AJoinKeyDtype => "a-join-key-dtype",
            Self::AHiddenSchema => "a-hidden-schema",
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
            Self::ANegativeExponent => {
                "Polars takes the exponent of an integer as a UInt32, so a negative one fails when the rows run, which the tables measured with the number 1"
            }
            Self::ABitwiseCallOnANull => {
                "Polars meets an operand of the dtype Null of a bitwise call where select does not, and refuses it, though the tables allowed the call as a Boolean"
            }
            Self::ADivisionByZero => {
                "Polars refuses to divide a Decimal by zero, which the sample's values never give the tables"
            }
            Self::AValueBeyondADecimal => {
                "a UInt128 beyond the digits of a decimal cannot be cast to it, which the tables measured on values within it"
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
            Self::AHiddenSchema => {
                "a method whose plan and rows differ in dtype is hidden by what goes on from it"
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
            Start::Input(_) | Start::Hole => None,
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

/// Whether `call` is `and`, `or` or `xor`.
fn is_a_bitwise_call(call: &Call) -> bool {
    matches!(
        call.entry().id().as_str(),
        "Expr.and_" | "Expr.or_" | "Expr.xor"
    )
}

/// Whether an expression has a `not` straight after an `and` or an `or`, at any depth.
fn negates_a_bitwise_call(expression: &Expression) -> bool {
    expression.links().windows(2).any(|pair| {
        is(&pair[1], "Expr.not_") && (is(&pair[0], "Expr.and_") || is(&pair[0], "Expr.or_"))
    }) || std::iter::once(expression.start())
        .filter_map(|start| match start {
            Start::Call(call) => Some(call),
            Start::Input(_) | Start::Hole => None,
        })
        .chain(expression.links())
        .any(|call| {
            operands(call)
                .into_iter()
                .any(|(_, inside)| negates_a_bitwise_call(inside))
        })
}

/// Whether `call` is a literal that is a whole number below zero.
fn is_a_whole_number_below_zero(call: &Call) -> bool {
    is(call, "polars.lit")
        && matches!(call.argument("value"), Some(Value::Integer(number)) if *number < 0)
}

/// Whether `call` is a `pow` whose exponent holds a value that may be below zero: a whole number
/// typed below zero, or a call that can take the values of a column there, as the sample's own
/// values are not.
fn is_a_power_that_may_be_given_a_value_below_zero(call: &Call) -> bool {
    is(call, "Expr.pow")
        && matches!(call.argument("exponent"), Some(Value::Expression(exponent))
        if any_call(exponent, &|inner| {
            is_a_whole_number_below_zero(inner)
                || matches!(
                    inner.entry().id().as_str(),
                    "Expr.sub" | "Expr.neg" | "Expr.not_" | "Expr.add" | "Expr.mul"
                )
        }))
}

/// Whether `call` is a `pow` whose exponent is a number with a fraction typed without a dtype.
fn is_a_power_of_a_fraction(call: &Call) -> bool {
    is(call, "Expr.pow")
        && matches!(call.argument("exponent"), Some(Value::Expression(exponent))
            if exponent.links().is_empty()
                && matches!(exponent.start(), Start::Call(start)
                    if is(start, "polars.lit")
                        && start.argument("dtype").is_none()
                        && matches!(start.argument("value"), Some(Value::Float(_)))))
}

fn has_explode(call: &Call) -> bool {
    is(call, "Expr.over")
        && matches!(call.argument("mapping_strategy"), Some(Value::Choice(word)) if word == "explode")
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
    /// Where each of them is, in the same order.
    places: Vec<Place>,
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
        places: Vec::new(),
    };
    let mut node = Node::Call(case.document.steps()[0].call());
    let mut here = Place::of_step(0, Some(case.document.steps()[0].id().clone()));
    for segment in place.path() {
        here = here.then(segment.clone());
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
                    Start::Input(_) | Start::Hole => return locus,
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
                locus.places.push(here.clone());
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
            Start::Input(_) | Start::Hole => None,
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
                Start::Input(_) | Start::Hole => None,
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
            Start::Input(_) | Start::Hole => None,
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
const SAMPLE_ROWS: usize = 3;

/// What a row of the values a chain gives holds, as far as the sample says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    Null,
    Value,
    /// Not known to be either.
    Unknown,
}

/// What a call of a chain does to the rows it is given: the number of rows, where
/// each goes and which hold a null.
enum Effect {
    /// Each row stays where it is, null where it was null and a value where it was one:
    /// a running sum or maximum, and a part of a date.
    Keeps,
    /// Each row is a value, whatever it was.
    Values,
    /// The rows stay in place, and what they hold is not known: a comparison, a
    /// subtraction or a window. The number stays too, but where there is one row,
    /// which an aggregate leaves, the call gives it to each of the sample's rows, as
    /// what it meets may be a column of them.
    Blurs,
    /// What it gives is not known of the rows at all: another number, or another order.
    Lost,
}

fn effect(call: &Call) -> Effect {
    match call.entry().id().as_str() {
        "Expr.cum_sum"
        | "Expr.cum_max"
        | "Expr.abs"
        | "Expr.neg"
        | "Expr.not_"
        | "Expr.dt.date"
        | "Expr.dt.month_start"
        | "Expr.dt.time"
        | "Expr.dt.total_days" => Effect::Keeps,
        "Expr.is_null" | "Expr.is_not_null" | "Expr.cum_count" => Effect::Values,
        "Expr.eq"
        | "Expr.ne"
        | "Expr.gt"
        | "Expr.ge"
        | "Expr.lt"
        | "Expr.le"
        | "Expr.add"
        | "Expr.and_"
        | "Expr.floordiv"
        | "Expr.mod"
        | "Expr.mul"
        | "Expr.or_"
        | "Expr.pow"
        | "Expr.sub"
        | "Expr.truediv"
        | "Expr.xor"
        | "Expr.dt.offset_by"
        | "When.then"
        | "Then.otherwise"
        | "Then.when"
        | "ChainedWhen.then"
        | "ChainedThen.otherwise" => Effect::Blurs,
        "Expr.over" if !has_explode(call) => Effect::Blurs,
        _ => Effect::Lost,
    }
}

/// The rows a shift moves, `n` of them: positive down, negative up. The number is
/// 1 when it is not given, and not known when it is not written as a whole number.
fn shift_amount(link: &Call) -> Option<i64> {
    match link.argument("n") {
        None => Some(1),
        Some(Value::Expression(n)) if n.links().is_empty() => match n.start() {
            Start::Call(start) if is(start, "polars.lit") => match start.argument("value") {
                Some(Value::Integer(rows)) => Some(*rows),
                _ => None,
            },
            _ => None,
        },
        Some(_) => None,
    }
}

/// What the rows a shift moves in hold: the `fill_value`, a value where it is a
/// literal of one, a null where it is the null literal, and nothing known where it
/// is another expression; null when it is not given.
fn filled_with(link: &Call) -> Cell {
    match link.argument("fill_value") {
        None => Cell::Null,
        Some(Value::Expression(fill)) if fill.links().is_empty() => match fill.start() {
            Start::Call(start) if is(start, "polars.lit") => match start.argument("value") {
                Some(Value::Flag(None)) => Cell::Null,
                Some(_) => Cell::Value,
                None => Cell::Unknown,
            },
            _ => Cell::Unknown,
        },
        Some(_) => Cell::Unknown,
    }
}

/// `rows` after the shift `link`: each moves by `n`, and the rows left empty hold
/// what it fills them with.
fn shifted(rows: &[Cell], link: &Call) -> Option<Vec<Cell>> {
    let n = shift_amount(link)?;
    let (fill, count) = (filled_with(link), rows.len());
    let moved = usize::try_from(n.unsigned_abs()).unwrap_or(usize::MAX);
    Some(
        (0..count)
            .map(|row| {
                let from = if n >= 0 {
                    row.checked_sub(moved)
                } else {
                    row.checked_add(moved).filter(|from| *from < count)
                };
                from.map_or(fill, |from| rows[from])
            })
            .collect(),
    )
}

/// The one row that the first or last of `rows` is: the row itself, or with
/// `skipping`, the first or last that is not null.
fn picked(rows: &[Cell], skipping: bool, from_the_end: bool) -> Cell {
    let mut rows: Vec<Cell> = rows.to_vec();
    if from_the_end {
        rows.reverse();
    }
    if !skipping {
        return rows.first().copied().unwrap_or(Cell::Unknown);
    }
    rows.iter()
        .copied()
        .find(|cell| *cell != Cell::Null)
        .unwrap_or(Cell::Null)
}

/// The rows after the first `upto` links of `expression`, taken from the sample:
/// what each holds, by what the calls do to it. `None` when what the calls did to
/// the rows is not known.
fn rows_after(frames: &Frames, expression: &Expression, upto: usize) -> Option<Vec<Cell>> {
    // A chain that starts from one column of the sample has the rows it holds, a
    // value or a null in each; any other start is the sample's number of rows, of
    // nothing known.
    let mut rows: Vec<Cell> = (0..SAMPLE_ROWS)
        .map(|row| {
            starting_column(expression)
                .and_then(|name| frames.holds_a_null_at(name, row))
                .map_or(
                    Cell::Unknown,
                    |null| {
                        if null { Cell::Null } else { Cell::Value }
                    },
                )
        })
        .collect();
    for link in &expression.links()[..upto.min(expression.links().len())] {
        let skipping = matches!(link.argument("ignore_nulls"), Some(Value::Flag(Some(true))));
        rows = match link.entry().id().as_str() {
            "Expr.shift" => shifted(&rows, link)?,
            "Expr.first" => vec![picked(&rows, skipping, false)],
            "Expr.last" => vec![picked(&rows, skipping, true)],
            // The smallest and the largest of rows that are all null is null, and
            // of any that holds a value is a value. The sum of rows that are all
            // null is zero for numbers and null for a column of `Null`, so it is
            // known only where a row holds a value.
            "Expr.min" | "Expr.max" | "Expr.sum" => {
                let one = if rows.contains(&Cell::Value) {
                    Cell::Value
                } else if rows.contains(&Cell::Unknown) || link.entry().id().as_str() == "Expr.sum"
                {
                    Cell::Unknown
                } else {
                    Cell::Null
                };
                vec![one]
            }
            "Expr.len" => vec![Cell::Value],
            _ => match effect(link) {
                Effect::Keeps => rows,
                Effect::Values => vec![Cell::Value; rows.len()],
                Effect::Blurs => {
                    let count = if rows.len() == 1 {
                        SAMPLE_ROWS
                    } else {
                        rows.len()
                    };
                    vec![Cell::Unknown; count]
                }
                Effect::Lost => return None,
            },
        };
    }
    Some(rows)
}

/// The name of the one column `expression` starts from, when it starts from one.
fn starting_column(expression: &Expression) -> Option<&str> {
    match expression.start() {
        Start::Call(start) if is(start, "polars.col") => match start.argument("name") {
            Some(Value::Columns(names)) => match names.as_slice() {
                [name] => Some(name),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// Whether the first `upto` links of `expression`, taken from the sample, leave only
/// nulls: shifts that move all its rows, in either direction, or the row that a
/// shift left null and a first, last, minimum or maximum then takes, or the one
/// value that an aggregate leaves, shifted away (`rows_after` follows each call:
/// the sample's columns hold their values and a null in the middle, or none, and a
/// call that is not known to keep the rows where they are, or the nulls in them,
/// leaves nothing known).
fn leaves_only_nulls(frames: &Frames, expression: &Expression, upto: usize) -> bool {
    rows_after(frames, expression, upto)
        .is_some_and(|rows| !rows.is_empty() && rows.iter().all(|cell| *cell == Cell::Null))
}

/// `value` with every window in it that explodes given the mapping it has when none
/// is named.
fn value_without_explodes(value: &Value) -> Value {
    match value {
        Value::Expression(expression) => {
            Value::Expression(without_explodes(expression).unwrap_or_else(|| expression.clone()))
        }
        Value::Many(items) => Value::Many(items.iter().map(value_without_explodes).collect()),
        Value::Pairs(pairs) => Value::Pairs(
            pairs
                .iter()
                .map(|(key, entry)| (value_without_explodes(key), value_without_explodes(entry)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `call` with the arguments that explode, and the windows in them, undone.
fn call_without_explodes(call: &Call) -> Option<Call> {
    let explodes = has_explode(call);
    let given: Vec<(&str, Value)> = call
        .arguments()
        .iter()
        .filter(|given| !(explodes && given.parameter() == "mapping_strategy"))
        .map(|given| (given.parameter(), value_without_explodes(given.value())))
        .collect();
    Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()
}

/// `expression` with no window that explodes: each is the window the same call
/// makes when it is not told how to map.
fn without_explodes(expression: &Expression) -> Option<Expression> {
    let start = match expression.start() {
        Start::Call(call) => Start::Call(call_without_explodes(call)?),
        other => other.clone(),
    };
    let links = expression
        .links()
        .iter()
        .map(call_without_explodes)
        .collect::<Option<Vec<_>>>()?;
    Expression::new(start, links).ok()
}

/// The case with no window that explodes, the same in all else.
fn case_without_explodes(case: &Case) -> Option<Case> {
    let step = &case.document.steps()[0];
    let rebuilt = Step::new(step.id().clone(), call_without_explodes(step.call())?).ok()?;
    Some(Case {
        receiver: case.receiver,
        argument: case.argument,
        document: Document::new(vec![rebuilt]).ok()?,
    })
}

/// How often the case without its explodes is run again: which of two failures of
/// a plan Polars names depends on which thread gets there first.
const RUNS: usize = 8;

/// Whether Polars refuses the case for lengths still when no window explodes: the
/// explode is then no cause of the refusal, which the rows' lengths are.
fn refused_for_lengths_without_explodes(case: &Case) -> bool {
    case_without_explodes(case).is_some_and(|control| {
        (0..RUNS).any(|_| {
            observe(&control)
                .refusal
                .is_some_and(|refusal| unplaced(&refusal) && is_lengths(&refusal.reason))
        })
    })
}

/// The expressions that start from a literal and that the limit is about at the
/// place the tables refused: the chain the call is at, an argument the call is
/// given, and, where the tables refused when they ran rows, a chain that holds the
/// place in an argument of a call that follows the literal (what a call after a
/// literal is given, at any depth, is met by one value, which Polars broadcasts).
/// None in a pivot's aggregate, which reads no column to stand in for a literal.
fn literal_causes<'a>(case: &'a Case, refused: &Refused, pivot: bool) -> Vec<LiteralCause<'a>> {
    if pivot {
        return Vec::new();
    }
    let locus = locate(case, &refused.place);
    let mut found: Vec<LiteralCause<'a>> = Vec::new();
    let mut add = |cause: LiteralCause<'a>| {
        if !found.iter().any(|known| known.place == cause.place) {
            found.push(cause);
        }
    };
    if let (Some(chain), Some(place)) = (locus.chain, locus.places.last())
        && starts_from_a_literal(chain)
    {
        add(LiteralCause {
            place: place.clone(),
            expression: chain,
        });
    }
    if let Some(call) = locus.call {
        operand_causes(call, &refused.place, pivot)
            .into_iter()
            .for_each(&mut add);
    }
    if refused.stage == Some(TableStage::Rows) {
        for ((holder, reach), place) in locus.inside.iter().zip(&locus.reach).zip(&locus.places) {
            if matches!(reach, Reach::Operand(_)) && starts_from_a_literal(holder) {
                add(LiteralCause {
                    place: place.clone(),
                    expression: holder,
                });
            }
        }
    }
    found
}

/// Whether Polars refuses the case for the tables' reason when each of `causes` is
/// a column of its dtype and nothing else is changed. Polars allows the case as it
/// is, so a refusal is of what the literals were; the tables are then right about a
/// column, and the literal is why Polars differs. A case run again that Polars
/// allows still, or refuses for another reason, shows no such thing: the part is
/// skipped or the tables are wrong about the column as well, which no literal
/// explains. `false` where a literal has no class to stand in with.
fn refused_with_columns_for_the_literals(
    causes: &[LiteralCause<'_>],
    case: &Case,
    refused: &Refused,
    observed: &Observed,
) -> bool {
    let Some(classes) = causes
        .iter()
        .map(|cause| class_of(cause, observed))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let standing: Vec<&Expression> = causes.iter().map(|cause| cause.expression).collect();
    let Some((control, other_as_left)) = control_with_columns(case, refused, &standing) else {
        return false;
    };
    let Some(tables) = Tables::standing(other_as_left, &classes) else {
        return false;
    };
    (0..RUNS).any(|_| {
        observe_probe(&control, tables)
            .refusal
            .is_some_and(|refusal| same_reason(&refused.source, &refused.reason, &refusal.reason))
    })
}

/// The case run again with the literals in `standing` as columns, and whether it
/// reads the other table. A join's key must be elementwise, which a column beside
/// an aggregate is not and a literal is, so Polars refusing the key for that is of
/// the position and not of the call the tables refused. Unless the tables refused
/// the position, the key that holds the literals is then the only item of a
/// `select`, which Polars evaluates as the column it is.
fn control_with_columns(
    case: &Case,
    refused: &Refused,
    standing: &[&Expression],
) -> Option<(Case, bool)> {
    let step = &case.document.steps()[0];
    let key_of_a_join = match (
        is(step.call(), "LazyFrame.join") && !refused.reason.contains("elementwise"),
        refused.place.path().first(),
    ) {
        (true, Some(Segment::Parameter(name)))
            if ["on", "left_on", "right_on"].contains(&name.as_str()) =>
        {
            locate(case, &refused.place)
                .top
                .map(|key| (key, name == "right_on"))
        }
        _ => None,
    };
    let (call, other_as_left) = match key_of_a_join {
        Some((key, other_as_left)) => (
            Call::new(
                catalog::entry("LazyFrame.select")?,
                vec![(
                    "exprs",
                    Value::Many(vec![Value::Expression(expression_with_columns(
                        key, standing, 0,
                    )?)]),
                )],
            )
            .ok()?,
            other_as_left,
        ),
        None => (call_with_columns(step.call(), standing, 0)?, false),
    };
    let rebuilt = Step::new(step.id().clone(), call).ok()?;
    Some((
        Case {
            receiver: case.receiver,
            argument: case.argument,
            document: Document::new(vec![rebuilt]).ok()?,
        },
        other_as_left,
    ))
}

/// Whether the limit is why the tables and Polars disagree.
fn applies(
    limit: Limit,
    case: &Case,
    disagreement: &Disagreement,
    prediction: &Prediction,
    observed: &Observed,
) -> bool {
    let step = case.document.steps()[0].call();
    let pivot = is(step, "LazyFrame.pivot");
    let held = operands(step);
    let explodes_in = |expression: &Expression| any_call(expression, &has_explode);
    match (limit, disagreement) {
        (Limit::AnExplodeChangesTheLengths, Disagreement::PolarsRefuses(refusal)) => {
            // The explode is the cause where Polars stops refusing for lengths when
            // no window explodes.
            unplaced(refusal)
                && is_lengths(&refusal.reason)
                && held.iter().any(|(_, held)| explodes_in(held))
                && !refused_for_lengths_without_explodes(case)
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
            // The rows' lengths are the cause where no window explodes, or Polars
            // refuses for lengths still when none does.
            unplaced(refusal)
                && is_lengths(&refusal.reason)
                && (!held.iter().any(|(_, held)| explodes_in(held))
                    || refused_for_lengths_without_explodes(case))
        }
        (Limit::ALiteral, Disagreement::PolarsAllows(refused)) => {
            // The literal is the cause where Polars refuses the case for the tables'
            // reason once each literal that could be is a column of its dtype.
            let causes = literal_causes(case, refused, pivot);
            !causes.is_empty()
                && refused_with_columns_for_the_literals(&causes, case, refused, observed)
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
                    // holds a running value, an aggregate or a window beside what
                    // is not one value. A key that is wholly one value is one
                    // Polars allows beside any other key that is elementwise.
                    let keys = keys(case, true);
                    let other_cause = |key: &Expression| {
                        !wholly_single(key, key.links().len(), true)
                            && any_call(key, &|call| {
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
            let frames = Frames::with_key(case.receiver, case.argument, None);
            // What the call follows, or is given, is only nulls.
            refused.stage == Some(TableStage::Rows)
                && (locus
                    .chain
                    .zip(locus.index)
                    .is_some_and(|(chain, at)| leaves_only_nulls(&frames, chain, at))
                    || locus.call.is_some_and(|call| {
                        operands(call).into_iter().any(|(_, operand)| {
                            leaves_only_nulls(&frames, operand, operand.links().len())
                        })
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
        (Limit::ANegativeExponent, Disagreement::PolarsRefuses(refusal)) => {
            // The rows hold the number; a table with no rows holds none to convert.
            unplaced(refusal)
                && refusal.stage == Stage::Rows
                && refusal.reason.starts_with("conversion from `")
                && refusal.reason.contains("` to `u32` failed")
                && held.iter().any(|(_, held)| {
                    any_call(held, &is_a_power_that_may_be_given_a_value_below_zero)
                })
        }
        (Limit::ABitwiseCallOnANull, Disagreement::PolarsRefuses(refusal)) => {
            let reason = refusal.reason.as_str();
            let negated = reason.contains("dtype Null not supported in 'not' operation")
                && held.iter().any(|(_, held)| negates_a_bitwise_call(held));
            let filtered = reason
                .contains("filter predicate must be of type `Boolean`, got `Null`")
                && is(step, "LazyFrame.filter")
                && held
                    .iter()
                    .any(|(_, held)| any_call(held, &is_a_bitwise_call));
            unplaced(refusal) && refusal.stage == Stage::Rows && (negated || filtered)
        }
        (Limit::ADivisionByZero, Disagreement::PolarsRefuses(refusal)) => {
            // The rows hold the divisor; a table with no rows holds none.
            unplaced(refusal)
                && refusal.stage == Stage::Rows
                && refusal.reason.contains("division by zero")
                && held.iter().any(|(_, held)| {
                    any_call(held, &|call| {
                        matches!(
                            call.entry().id().as_str(),
                            "Expr.truediv" | "Expr.floordiv" | "Expr.mod"
                        )
                    })
                })
        }
        (Limit::AValueBeyondADecimal, Disagreement::PolarsRefuses(refusal)) => {
            // The rows hold the values; a table with no rows holds none to fail.
            unplaced(refusal)
                && refusal.stage == Stage::Rows
                && refusal
                    .reason
                    .starts_with("conversion from `u128` to `decimal[")
                && held
                    .iter()
                    .any(|(_, held)| any_call(held, &|call| is(call, "Expr.sub")))
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
                && (refusal.reason.contains("decimal")
                    || (refusal.reason.contains("dyn float")
                        && held
                            .iter()
                            .any(|(_, held)| any_call(held, &is_a_power_of_a_fraction))))
        }
        (Limit::AHiddenSchema, Disagreement::PolarsAllows(refused)) => {
            let locus = locate(case, &refused.place);
            let last = locus.inside.last().map(|chain| chain.links().len());
            refused.reason.starts_with(SCHEMA_SAYS)
                && (locus.inside.len() > 1
                    || locus.index.is_some_and(|at| Some(at + 1) != last)
                    // A sort key is no output of the step, unless a probe of the call on a
                    // column in the key settles it, which is asked of a case the limits do
                    // not excuse.
                    || (matches!(
                        step.entry().id().as_str(),
                        "LazyFrame.sort" | "LazyFrame.join"
                    ) && settle_in_the_position(case, refused, prediction, observed).is_none()))
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
        .filter(|limit| applies(*limit, case, &disagreement, prediction, observed))
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
            argument_unknown: false,
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
            argument_unknown: false,
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
    fn a_literal_given_to_a_later_call_of_the_chain_excuses_nothing_before_it() {
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
    }

    /// The limits that excuse the case a kept file holds, as the target finds them: with the tables'
    /// verdict and what Polars does, which no limit's own tests make up.
    fn excused_by_kept(name: &str) -> Vec<Limit> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/allowed_fuzz/regressions")
            .join(name);
        let case = Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case");
        excused_by(&case)
    }

    fn excused_by(case: &Case) -> Vec<Limit> {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(scope, || {
                    excuses(case, &super::super::predict::predict(case), &observe(case))
                })
                .expect("a thread")
                .join()
                .expect("the limits are asked")
        })
    }

    #[test]
    fn a_literal_is_excused_where_polars_allows_it_and_refuses_a_column_of_its_dtype() {
        // A literal that is given to a comparison, one that starts a chain, one that
        // starts the other table's key, and a call that follows a literal and is given
        // what the tables refuse when they run rows.
        for name in [
            "uncovered-a-literal--given-to-a-comparison.case",
            "uncovered-a-literal--starting-a-chain.case",
            "uncovered-a-literal--given-to-a-call-that-follows-a-literal.case",
            "uncovered-a-literal.case",
        ] {
            assert_eq!(excused_by_kept(name), vec![Limit::ALiteral], "{name}");
        }
    }

    #[test]
    fn a_literal_is_no_cause_where_polars_allows_the_column_of_its_dtype_too() {
        // Each holds a literal at the place the tables refused, as a start or an
        // argument, and Polars allows the part for another cause: a branch no row
        // takes, a sort key of one value, a key that is wholly one value, a panic.
        // The literal replaced by a column of its dtype is allowed all the same, so
        // it is not why, and a case of the other causes is excused by theirs alone.
        for name in [
            "stricter-a-literal-compared-in-a-then-no-row-takes.case",
            "stricter-a-literal-compared-in-a-then-no-row-takes-in-a-join-key.case",
            "stricter-a-literal-compared-in-a-sort-key-of-one-value.case",
        ] {
            assert_eq!(excused_by_kept(name), vec![], "{name}");
        }
        assert_eq!(
            excused_by_kept("uncovered-a-single-value-in-a-key--a-literal-in-the-key.case"),
            vec![Limit::ASingleValueInAKey]
        );
        assert_eq!(
            excused_by_kept("uncovered-a-panic--a-literal-in-a-sort-key.case"),
            vec![Limit::APanic]
        );
    }

    #[test]
    fn what_follows_an_aggregate_compared_with_the_length_of_a_literal_over_a_window_is_no_limit() {
        // `a.min() <= 1.len().over(order_by=[true])` then `.total_days()`: Polars
        // takes the length of a literal over a window for a constant, and does not
        // evaluate what follows the comparison of a value with it, where the tables
        // refuse a `Boolean`. That is a part Polars skips, which a probe settles
        // and no limit excuses, however the chain is made.
        let window = |length: &str, order_by: Expression| {
            chain_of(
                literal(),
                vec![
                    call(length, Vec::new()),
                    call("Expr.over", vec![("order_by", many(vec![order_by]))]),
                ],
            )
        };
        let item = |operand: Expression| {
            select(vec![chain_of(
                column("a"),
                vec![
                    call("Expr.min", Vec::new()),
                    compared_with("Expr.le", operand),
                    call("Expr.dt.total_days", Vec::new()),
                ],
            )])
        };
        let constant = || chain(literal(), &[]);
        for (name, operand) in [
            ("the length of a literal", window("Expr.len", constant())),
            ("a sum", window("Expr.sum", constant())),
            (
                "a window by a column",
                window("Expr.len", chain(column("k"), &[])),
            ),
            ("no window", chain(literal(), &["Expr.len"])),
        ] {
            assert_eq!(
                tables_refuse(&item(operand), link_of("exprs", 0, 2), ROWS, "no"),
                vec![],
                "{name}"
            );
        }
    }

    #[test]
    fn what_a_call_after_a_literal_is_given_is_excused_only_where_rows_refuse_it_and_a_literal_starts_the_chain()
     {
        // `1.cum_sum().le(b.max())`: the tables refuse `max` of a list when they run
        // rows, and Polars meets it with one value.
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
    /// A join key that is a null literal of `Null` with a running count: Polars allows the key because it is
    /// wholly one value, and a column of `Null` as the key is not elementwise.
    const A_KEY_OF_ONE_LITERAL: &str = r#"{"a":"Int16","b":"Datetime(us)","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.join","arguments":{"other":{"table":"other"},"on":{"many":[{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":null},"dtype":{"dtype":"Null"}}},"then":[{"entry":"Expr.cum_count","arguments":{"reverse":{"flag":true}}}]}}]},"nulls_equal":{"flag":false},"build_side":{"choice":"prefer_left"}}}}]}"#;

    #[test]
    fn a_case_that_two_limits_explain_is_excused_by_both() {
        // The literal is why Polars allows the key: a column of its dtype is refused as
        // the tables say. So is the key being wholly one value, which has a limit of
        // its own, and nothing is gated on the other not applying.
        let case = Case::decode(A_KEY_OF_ONE_LITERAL.as_bytes()).expect("a case");
        assert_eq!(
            excused_by(&case),
            vec![Limit::ALiteral, Limit::ASingleValueInAKey]
        );
    }

    /// Two predicates of a filter: a literal over a window, which Polars refuses for
    /// the lengths of the rows, and, after it, a window that explodes, which has
    /// nothing to do with the refusal.
    const LENGTHS_OF_THE_ROWS_BESIDE_AN_EXPLODE: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.filter","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":1},"dtype":{"dtype":"Datetime(time_unit='us', time_zone=None)"}}},"then":[{"entry":"Expr.over","arguments":{"order_by":{"many":[{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":null}}},"then":[]}}]}}},{"entry":"Expr.lt","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":1},"dtype":{"dtype":"Int32"}}},"then":[]}}}}]}},{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[{"entry":"Expr.first","arguments":{}},{"entry":"Expr.over","arguments":{"partition_by":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[]}}]},"mapping_strategy":{"choice":"explode"}}}]}}]}}}}]}"#;

    /// The window that explodes, alone: its groups are fewer than the rows.
    const AN_EXPLODE_ALONE: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.filter","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[{"entry":"Expr.first","arguments":{}},{"entry":"Expr.over","arguments":{"partition_by":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[]}}]},"mapping_strategy":{"choice":"explode"}}}]}}]}}}}]}"#;

    #[test]
    fn a_lengths_refusal_that_remains_without_the_explode_is_the_rows_and_not_the_explode() {
        // Polars refuses `lit(1).over(order_by=null) < 1` as a filter for lengths with
        // no explode in sight; the explode in the second predicate is no cause of it,
        // and the same filter with that window not exploding is refused the same
        // way. Where the refusal goes when the explode does, it is the explode's.
        let case = |text: &str| Case::decode(text.as_bytes()).expect("a case");
        let refused = |case: &Case| {
            std::thread::scope(|scope| {
                std::thread::Builder::new()
                    .stack_size(256 << 20)
                    .spawn_scoped(scope, || {
                        polars_refuses(case, None, Stage::Rows, LENGTHS, Vec::new())
                    })
                    .expect("a thread")
                    .join()
                    .expect("the limits are asked")
            })
        };
        assert_eq!(
            refused(&case(LENGTHS_OF_THE_ROWS_BESIDE_AN_EXPLODE)),
            vec![Limit::TheLengthsOfTheRows]
        );
        assert_eq!(
            refused(&case(AN_EXPLODE_ALONE)),
            vec![Limit::AnExplodeChangesTheLengths]
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

    // ----- A negative exponent

    const NEGATIVE_EXPONENT: &str =
        "conversion from `i32` to `u32` failed in column 'literal' for 1 out of 1 values: [-1]";

    /// A literal that is the whole number `number`, typed without a dtype.
    fn whole(number: i64) -> Expression {
        chain(
            call("polars.lit", vec![("value", Value::Integer(number))]),
            &[],
        )
    }

    fn pow_of(exponent: Expression) -> Call {
        call("Expr.pow", vec![("exponent", Value::Expression(exponent))])
    }

    #[test]
    fn a_failed_conversion_to_a_u32_is_excused_where_a_power_is_given_a_negative_number() {
        let case = select(vec![chain_of(column("a"), vec![pow_of(whole(-1))])]);
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, NEGATIVE_EXPONENT, Vec::new()),
            vec![Limit::ANegativeExponent]
        );
        // A power in an expression the chain is given reaches it too.
        let nested = select(vec![chain_of(
            column("a"),
            vec![compared_with(
                "Expr.eq",
                chain_of(column("k"), vec![pow_of(whole(-1))]),
            )],
        )]);
        assert_eq!(
            polars_refuses(&nested, None, Stage::Rows, NEGATIVE_EXPONENT, Vec::new()),
            vec![Limit::ANegativeExponent]
        );
    }

    #[test]
    fn a_failed_conversion_to_a_u32_is_excused_where_a_power_is_given_a_value_below_zero() {
        // A subtraction, a negation, the bits of a number flipped, a sum and a product can take
        // the values of a column below zero, as a number typed below zero does.
        for exponent in [
            chain_of(
                column("b"),
                vec![compared_with("Expr.sub", chain(column("k"), &[]))],
            ),
            chain(column("k"), &["Expr.neg"]),
            chain(column("k"), &["Expr.not_"]),
            chain_of(column("k"), vec![compared_with("Expr.add", whole(-3))]),
            chain_of(
                column("k"),
                vec![compared_with("Expr.mul", chain(column("a"), &[]))],
            ),
        ] {
            let case = select(vec![chain_of(column("a"), vec![pow_of(exponent)])]);
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, NEGATIVE_EXPONENT, Vec::new()),
                vec![Limit::ANegativeExponent]
            );
        }
    }

    #[test]
    fn a_failed_conversion_to_a_u32_is_not_excused_without_a_power_of_a_negative_number() {
        // A power of a positive number or of a column, and a negative number given to another
        // call, are not why a number fails to become a `u32`.
        for case in [
            select(vec![chain_of(column("a"), vec![pow_of(whole(2))])]),
            select(vec![chain_of(
                column("a"),
                vec![pow_of(chain(column("b"), &[]))],
            )]),
            select(vec![chain_of(
                column("a"),
                vec![compared_with("Expr.sub", whole(-1))],
            )]),
        ] {
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, NEGATIVE_EXPONENT, Vec::new()),
                vec![]
            );
        }
    }

    #[test]
    fn only_a_failed_conversion_to_a_u32_of_the_rows_without_a_place_is_excused_for_a_power() {
        let case = select(vec![chain_of(column("a"), vec![pow_of(whole(-1))])]);
        for reason in [
            "conversion from `i32` to `u8` failed in column 'literal' for 1 out of 1 values: [-1]",
            "`pow` operation not supported for dtype `str` as base",
        ] {
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, reason, Vec::new()),
                vec![],
                "{reason}"
            );
        }
        // A refusal that Polars places at a call is the call's.
        assert_eq!(
            polars_refuses(
                &case,
                Some(link_of("exprs", 0, 0)),
                Stage::Rows,
                NEGATIVE_EXPONENT,
                Vec::new()
            ),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Schema, NEGATIVE_EXPONENT, Vec::new()),
            vec![]
        );
    }

    // ----- A negated bitwise call

    const NOT_OF_A_NULL: &str = "dtype Null not supported in 'not' operation";

    fn and_of(operand: Expression) -> Call {
        call("Expr.and_", vec![("others", many(vec![operand]))])
    }

    #[test]
    fn a_not_of_the_dtype_null_is_excused_where_a_not_comes_straight_after_an_and_or_an_or() {
        for combined in [
            and_of(chain(column("f"), &[])),
            call(
                "Expr.or_",
                vec![("others", many(vec![chain(column("f"), &[])]))],
            ),
        ] {
            let case = select(vec![chain_of(
                null(),
                vec![
                    combined,
                    call("Expr.not_", Vec::new()),
                    call("Expr.min", Vec::new()),
                ],
            )]);
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, NOT_OF_A_NULL, Vec::new()),
                vec![Limit::ABitwiseCallOnANull]
            );
        }
        // In an expression the chain is given too.
        let nested = select(vec![chain_of(
            column("a"),
            vec![compared_with(
                "Expr.eq",
                chain_of(
                    null(),
                    vec![
                        and_of(chain(column("f"), &[])),
                        call("Expr.not_", Vec::new()),
                    ],
                ),
            )],
        )]);
        assert_eq!(
            polars_refuses(&nested, None, Stage::Rows, NOT_OF_A_NULL, Vec::new()),
            vec![Limit::ABitwiseCallOnANull]
        );
    }

    #[test]
    fn a_not_of_the_dtype_null_is_not_excused_without_a_not_straight_after_a_bitwise_call() {
        for case in [
            // A not of a null, and no bitwise call.
            select(vec![chain(null(), &["Expr.not_"])]),
            // A bitwise call that no not follows.
            select(vec![chain_of(
                null(),
                vec![
                    and_of(chain(column("f"), &[])),
                    call("Expr.min", Vec::new()),
                ],
            )]),
            // A call between the two.
            select(vec![chain_of(
                null(),
                vec![
                    and_of(chain(column("f"), &[])),
                    call("Expr.cum_max", vec![]),
                    call("Expr.not_", Vec::new()),
                ],
            )]),
        ] {
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, NOT_OF_A_NULL, Vec::new()),
                vec![]
            );
        }
    }

    #[test]
    fn only_a_refusal_of_a_not_of_the_dtype_null_at_the_rows_without_a_place_is_excused_for_a_not()
    {
        let case = select(vec![chain_of(
            null(),
            vec![
                and_of(chain(column("f"), &[])),
                call("Expr.not_", Vec::new()),
            ],
        )]);
        assert_eq!(
            polars_refuses(
                &case,
                None,
                Stage::Rows,
                "dtype Int8 not supported in 'not' operation",
                Vec::new()
            ),
            vec![]
        );
        assert_eq!(
            polars_refuses(
                &case,
                Some(link_of("exprs", 0, 1)),
                Stage::Rows,
                NOT_OF_A_NULL,
                Vec::new()
            ),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Schema, NOT_OF_A_NULL, Vec::new()),
            vec![]
        );
    }

    #[test]
    fn a_filter_predicate_of_the_dtype_null_is_excused_where_a_bitwise_call_is_in_the_case() {
        // Polars gives `null & f` as a filter's predicate the dtype Null, where select shows a
        // `Boolean`.
        let reason = "filter predicate must be of type `Boolean`, got `Null`";
        let filtered = |predicate: Expression| {
            case_of(call(
                "LazyFrame.filter",
                vec![("predicates", many(vec![predicate]))],
            ))
        };
        let case = filtered(chain_of(null(), vec![and_of(chain(column("f"), &[]))]));
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, reason, Vec::new()),
            vec![Limit::ABitwiseCallOnANull]
        );
        let other_side = filtered(chain_of(column("f"), vec![and_of(chain(null(), &[]))]));
        assert_eq!(
            polars_refuses(&other_side, None, Stage::Rows, reason, Vec::new()),
            vec![Limit::ABitwiseCallOnANull]
        );
        // A predicate that holds no bitwise call, and another step, are not why.
        let plain = filtered(chain(null(), &["Expr.is_null"]));
        assert_eq!(
            polars_refuses(&plain, None, Stage::Rows, reason, Vec::new()),
            vec![]
        );
        let selected = select(vec![chain_of(
            null(),
            vec![and_of(chain(column("f"), &[]))],
        )]);
        assert_eq!(
            polars_refuses(&selected, None, Stage::Rows, reason, Vec::new()),
            vec![]
        );
        // Only the refusal of a predicate of the dtype Null, at the rows, without a place.
        assert_eq!(
            polars_refuses(
                &case,
                None,
                Stage::Rows,
                "filter predicate must be of type `Boolean`, got `Int64`",
                Vec::new()
            ),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Schema, reason, Vec::new()),
            vec![]
        );
    }

    // ----- A division by zero

    const DIVISION_BY_ZERO: &str = "division by zero Decimal";

    #[test]
    fn a_division_by_zero_is_excused_where_a_division_is_in_the_case() {
        for division in ["Expr.floordiv", "Expr.truediv", "Expr.mod"] {
            let case = select(vec![chain_of(
                column("b"),
                vec![compared_with(division, chain(column("a"), &[]))],
            )]);
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, DIVISION_BY_ZERO, Vec::new()),
                vec![Limit::ADivisionByZero],
                "{division}"
            );
        }
        // A division in an expression the chain is given reaches it too.
        let nested = select(vec![chain_of(
            column("a"),
            vec![compared_with(
                "Expr.eq",
                chain_of(
                    column("b"),
                    vec![compared_with("Expr.floordiv", chain(column("k"), &[]))],
                ),
            )],
        )]);
        assert_eq!(
            polars_refuses(&nested, None, Stage::Rows, DIVISION_BY_ZERO, Vec::new()),
            vec![Limit::ADivisionByZero]
        );
    }

    #[test]
    fn a_division_by_zero_is_not_excused_without_a_division_and_only_at_the_rows_without_a_place() {
        let none = select(vec![chain_of(
            column("b"),
            vec![compared_with("Expr.mul", chain(column("a"), &[]))],
        )]);
        assert_eq!(
            polars_refuses(&none, None, Stage::Rows, DIVISION_BY_ZERO, Vec::new()),
            vec![]
        );
        let case = select(vec![chain_of(
            column("b"),
            vec![compared_with("Expr.floordiv", chain(column("a"), &[]))],
        )]);
        assert_eq!(
            polars_refuses(
                &case,
                Some(link_of("exprs", 0, 0)),
                Stage::Rows,
                DIVISION_BY_ZERO,
                Vec::new()
            ),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Schema, DIVISION_BY_ZERO, Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, "no", Vec::new()),
            vec![]
        );
    }

    // ----- A value beyond a decimal

    const BEYOND_A_DECIMAL: &str = "conversion from `u128` to `decimal[38,0]` failed in column \
        '_POLARS_TMP_PHYS_0' for 1 out of 1 values: [340282366920938463463374607431768211454]";

    /// `a - a == b`: a subtraction, whose values wrap below zero in an unsigned
    /// dtype, compared with a column.
    fn a_subtraction_compared() -> Case {
        select(vec![chain_of(
            column("a"),
            vec![
                compared_with("Expr.sub", chain(column("a"), &[])),
                compared_with("Expr.eq", chain(column("b"), &[])),
            ],
        )])
    }

    #[test]
    fn a_failed_conversion_of_a_value_beyond_a_decimal_is_excused_where_a_subtraction_made_it() {
        assert_eq!(
            polars_refuses(
                &a_subtraction_compared(),
                None,
                Stage::Rows,
                BEYOND_A_DECIMAL,
                Vec::new()
            ),
            vec![Limit::AValueBeyondADecimal]
        );
        // A subtraction in an expression the chain is given reaches it too.
        let nested = select(vec![chain_of(
            column("a"),
            vec![compared_with(
                "Expr.eq",
                chain_of(
                    column("k"),
                    vec![compared_with("Expr.sub", chain(column("a"), &[]))],
                ),
            )],
        )]);
        assert_eq!(
            polars_refuses(&nested, None, Stage::Rows, BEYOND_A_DECIMAL, Vec::new()),
            vec![Limit::AValueBeyondADecimal]
        );
    }

    #[test]
    fn a_failed_conversion_of_a_value_beyond_a_decimal_without_a_subtraction_is_not_excused() {
        // The sample's own values are within a decimal: only a call that makes a
        // value beyond it explains the conversion failing.
        let case = select(vec![chain_of(
            column("a"),
            vec![compared_with("Expr.eq", chain(column("b"), &[]))],
        )]);
        assert_eq!(
            polars_refuses(&case, None, Stage::Rows, BEYOND_A_DECIMAL, Vec::new()),
            vec![]
        );
    }

    #[test]
    fn only_a_failed_conversion_of_a_u128_to_a_decimal_of_the_rows_is_excused() {
        let case = a_subtraction_compared();
        for reason in [
            "conversion from `i64` to `decimal[38,0]` failed in column 'a' for 1 out of 1 values: [1]",
            "conversion from `u128` to `i64` failed in column 'a' for 1 out of 1 values: [1]",
            "cannot compare u128 with decimal[10,2]",
        ] {
            assert_eq!(
                polars_refuses(&case, None, Stage::Rows, reason, Vec::new()),
                vec![],
                "{reason}"
            );
        }
        // A table with no rows holds no value to fail, and Polars saying where it
        // refused the step is another refusal.
        assert_eq!(
            polars_refuses(&case, None, Stage::Empty, BEYOND_A_DECIMAL, Vec::new()),
            vec![]
        );
        assert_eq!(
            polars_refuses(
                &case,
                Some(link_of("exprs", 0, 1)),
                Stage::Schema,
                BEYOND_A_DECIMAL,
                Vec::new()
            ),
            vec![]
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

    const DYN_FLOAT: &str = "datatypes of join keys don't match - `key`: f64 on left does not match `rk`: dyn float on right (and no other type was available to cast to)";

    /// A literal that is the number `number`, typed without a dtype.
    fn fraction(number: f64) -> Expression {
        chain(
            call("polars.lit", vec![("value", Value::Float(number))]),
            &[],
        )
    }

    #[test]
    fn a_join_key_that_raises_an_integer_to_a_fraction_is_a_dyn_float_that_no_key_matches() {
        // Polars resolves `rk ** 1.5` to a `Float64` where it is selected, and leaves it the
        // unresolved `dyn float` where it is a join's key, which no key of a dtype matches.
        let case = join(
            chain(column("key"), &[]),
            chain_of(column("rk"), vec![pow_of(fraction(1.5))]),
        );
        assert_eq!(
            polars_refuses(
                &case,
                Some(step_place()),
                Stage::Schema,
                DYN_FLOAT,
                Vec::new()
            ),
            vec![Limit::AJoinKeyDtype]
        );
    }

    #[test]
    fn a_dyn_float_join_key_is_not_excused_without_a_power_of_a_fraction() {
        // A power of a whole number, a subtraction of a fraction and no power at all resolve
        // to the dtype the chain gives, so Polars refusing the keys for a `dyn float` is no
        // limit's for them.
        for key in [
            chain_of(column("rk"), vec![pow_of(whole(2))]),
            chain_of(column("rk"), vec![compared_with("Expr.sub", fraction(1.5))]),
            chain(column("rk"), &[]),
        ] {
            let case = join(chain(column("key"), &[]), key);
            assert_eq!(
                polars_refuses(
                    &case,
                    Some(step_place()),
                    Stage::Schema,
                    DYN_FLOAT,
                    Vec::new()
                ),
                vec![]
            );
        }
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
    fn the_last_of_a_shift_is_null_where_the_sample_has_a_null_there() {
        // The sample's columns are a value, a null and a value. One shift puts the
        // null last, so `b.shift().last()` is null, and three shifts leave only
        // nulls; two shifts put a value last, and no shift leaves the value.
        let shift = || call("Expr.shift", Vec::new());
        let last = || call("Expr.last", Vec::new());
        let item = |links: Vec<Call>| {
            let mut all = links;
            all.push(compared_with("Expr.lt", chain(column("g"), &[])));
            select(vec![chain_of(column("b"), all)])
        };
        let excused = |links: Vec<Call>| {
            let at = links.len();
            tables_refuse(&item(links), link_of("exprs", 0, at), ROWS, "no")
        };
        assert_eq!(
            excused(vec![shift(), last()]),
            vec![Limit::AShiftThatLeavesNulls]
        );
        assert_eq!(
            excused(vec![shift(), shift(), shift(), last()]),
            vec![Limit::AShiftThatLeavesNulls]
        );
        assert_eq!(excused(vec![shift(), shift(), last()]), vec![]);
        assert_eq!(excused(vec![last()]), vec![]);
        assert_eq!(excused(vec![shift(), call("Expr.max", Vec::new())]), vec![]);
    }

    #[test]
    fn the_last_of_a_shift_is_null_only_in_a_column_that_has_the_sample_null() {
        // `a` and `b` hold a null in the middle row, which one shift puts last. The
        // columns `f`, `k`, `g`, `h` and `u` hold no null, so what is last after one
        // shift is a value, and the limit excuses nothing after it. Three shifts
        // leave only nulls whatever the column holds.
        let shift = || call("Expr.shift", Vec::new());
        let last = || call("Expr.last", Vec::new());
        let excused = |start: &str, links: Vec<Call>| {
            let at = links.len();
            let mut all = links;
            all.push(compared_with("Expr.lt", chain(column("g"), &[])));
            tables_refuse(
                &select(vec![chain_of(column(start), all)]),
                link_of("exprs", 0, at),
                ROWS,
                "no",
            )
        };
        for start in ["a", "b"] {
            assert_eq!(
                excused(start, vec![shift(), last()]),
                vec![Limit::AShiftThatLeavesNulls],
                "{start}"
            );
        }
        for start in ["f", "k", "g", "h", "u"] {
            assert_eq!(excused(start, vec![shift(), last()]), vec![], "{start}");
            assert_eq!(
                excused(start, vec![shift(), shift(), shift(), last()]),
                vec![Limit::AShiftThatLeavesNulls],
                "{start}"
            );
        }
        // After an aggregate there is one value, which a shift moves away.
        assert_eq!(
            excused("k", vec![call("Expr.max", Vec::new()), shift(), last()]),
            vec![Limit::AShiftThatLeavesNulls]
        );
    }

    #[test]
    fn a_running_sum_of_a_comparison_with_a_null_is_no_limit_of_the_sample() {
        // `f > null` then `cum_sum` then `== b`: Polars folds the comparison with
        // a null into a null whatever the rows are, so what follows the running
        // sum meets a column of nulls however many rows there are. That is a part
        // Polars skips, which a probe settles and no limit excuses.
        let item = |links: Vec<Call>| {
            let mut all = links;
            all.push(compared_with("Expr.eq", chain(column("b"), &[])));
            select(vec![chain_of(column("f"), all)])
        };
        let null_comparison = || compared_with("Expr.gt", chain(null(), &[]));
        let sum = || call("Expr.cum_sum", Vec::new());
        let case = item(vec![null_comparison(), sum()]);
        assert_eq!(
            tables_refuse(&case, link_of("exprs", 0, 2), ROWS, "no"),
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
        // The last after one shift is the sample's null; after two it is a value.
        let last = given(chain_of(column("b"), shift(&["Expr.shift", "Expr.last"])));
        assert_eq!(
            tables_refuse(&last, at_the_call.clone(), ROWS, "no"),
            vec![Limit::AShiftThatLeavesNulls]
        );
        let without_the_null = given(chain_of(column("k"), shift(&["Expr.shift", "Expr.last"])));
        assert_eq!(
            tables_refuse(&without_the_null, at_the_call.clone(), ROWS, "no"),
            vec![]
        );
        let later = given(chain_of(
            column("k"),
            shift(&["Expr.shift", "Expr.shift", "Expr.last"]),
        ));
        assert_eq!(tables_refuse(&later, at_the_call, ROWS, "no"), vec![]);
    }

    #[test]
    fn a_first_that_skips_nulls_is_null_where_what_it_takes_is_only_nulls() {
        // The sample's columns are a value, a null and a value. A first that
        // skips nulls takes the first value, which is null only when every row
        // is: after three shifts, or after a first of what a shift moved, which
        // is one null. After one or two shifts a value is first.
        let shift = || call("Expr.shift", Vec::new());
        let first = || call("Expr.first", Vec::new());
        let skipping = || {
            call(
                "Expr.first",
                vec![("ignore_nulls", Value::Flag(Some(true)))],
            )
        };
        let excused = |links: Vec<Call>| {
            let at = links.len();
            let mut all = links;
            all.push(compared_with("Expr.gt", chain(column("h"), &[])));
            tables_refuse(
                &select(vec![chain_of(column("b"), all)]),
                link_of("exprs", 0, at),
                ROWS,
                "no",
            )
        };
        assert_eq!(
            excused(vec![shift(), first(), skipping()]),
            vec![Limit::AShiftThatLeavesNulls]
        );
        assert_eq!(
            excused(vec![shift(), shift(), shift(), skipping()]),
            vec![Limit::AShiftThatLeavesNulls]
        );
        assert_eq!(excused(vec![shift(), skipping()]), vec![]);
        assert_eq!(excused(vec![shift(), shift(), skipping()]), vec![]);
        assert_eq!(excused(vec![first(), skipping()]), vec![]);
    }

    /// The tables refuse `lt` against the text column `g` after `links` on the
    /// column `start`, and Polars allows the step: which limits excuse it.
    fn excused_after(start: &str, links: Vec<Call>) -> Vec<Limit> {
        let at = links.len();
        let mut all = links;
        all.push(compared_with("Expr.lt", chain(column("g"), &[])));
        tables_refuse(
            &select(vec![chain_of(column(start), all)]),
            link_of("exprs", 0, at),
            ROWS,
            "no",
        )
    }

    fn shift_by(rows: i64) -> Call {
        call(
            "Expr.shift",
            vec![("n", Value::Expression(chain_with_integer(rows)))],
        )
    }

    fn plain(id: &str) -> Call {
        call(id, Vec::new())
    }

    #[test]
    fn a_shift_back_leaves_its_nulls_at_the_end() {
        // Polars: `a.shift(-1)` of a value, a null and a value is `null, 3, null`
        // for the column `a` and `1, 1, 2` -> `1, 2, null` for `k`, and `shift(-2)`
        // of `a` is `3, null, null`. The nulls a negative shift leaves are the
        // last rows, not the first.
        let shifts = [Limit::AShiftThatLeavesNulls];
        // The last row is null after a shift back, whatever the column holds there.
        assert_eq!(
            excused_after("k", vec![shift_by(-1), plain("Expr.last")]),
            shifts
        );
        assert_eq!(
            excused_after("a", vec![shift_by(-2), plain("Expr.last")]),
            shifts
        );
        // The first row is a value: `k.shift(-1)` starts with `1`, and `a.shift(-2)` with `3`.
        assert_eq!(
            excused_after("k", vec![shift_by(-1), plain("Expr.first")]),
            vec![]
        );
        assert_eq!(
            excused_after("a", vec![shift_by(-2), plain("Expr.first")]),
            vec![]
        );
        // A shift back that moves every row leaves only nulls.
        assert_eq!(
            excused_after("k", vec![shift_by(-3), plain("Expr.first")]),
            shifts
        );
        // A shift back, then forward: `a` is `3, null, null`, then `null, 3, null`.
        assert_eq!(
            excused_after("a", vec![shift_by(-2), shift_by(1), plain("Expr.first")]),
            shifts
        );
        assert_eq!(
            excused_after("a", vec![shift_by(-2), shift_by(1), plain("Expr.last")]),
            shifts
        );
        // Forward, then back: `1, null, 3` is `null, 1, null`, then `1, null, null`.
        assert_eq!(
            excused_after("a", vec![shift_by(1), shift_by(-1), plain("Expr.first")]),
            vec![]
        );
        assert_eq!(
            excused_after("a", vec![shift_by(1), shift_by(-1), plain("Expr.last")]),
            shifts
        );
    }

    #[test]
    fn what_a_call_before_a_shift_does_to_the_nulls_decides_what_the_shift_leaves() {
        // Polars: `b.cum_sum()` keeps the null of the sample's middle row, so the
        // last row after a shift is null as it is for the column. `b.cum_count()` is
        // `1, 1, 2` and `b.is_null()` is `false, true, false`: neither holds a null,
        // so the last row after a shift is a value, and the column's null does not
        // reach it.
        let shifts = [Limit::AShiftThatLeavesNulls];
        let last = || plain("Expr.last");
        assert_eq!(
            excused_after("b", vec![plain("Expr.cum_sum"), shift_by(1), last()]),
            shifts
        );
        assert_eq!(
            excused_after("b", vec![plain("Expr.cum_max"), shift_by(1), last()]),
            shifts
        );
        for gives_values in ["Expr.cum_count", "Expr.is_null", "Expr.is_not_null"] {
            assert_eq!(
                excused_after("b", vec![plain(gives_values), shift_by(1), last()]),
                vec![],
                "{gives_values}"
            );
        }
        // A call whose effect on the rows is not known leaves nothing known of them.
        assert_eq!(
            excused_after("b", vec![over("k", None), shift_by(1), last()]),
            vec![]
        );
        // Shifts that empty the rows leave only nulls whatever came before them, as
        // long as the rows are the sample's.
        assert_eq!(
            excused_after(
                "b",
                vec![plain("Expr.cum_count"), shift_by(3), plain("Expr.first")]
            ),
            shifts
        );
    }

    #[test]
    fn a_fill_value_moves_the_rows_and_gives_the_rows_it_fills_a_value() {
        // Polars: `b.shift(1, fill_value=7)` is `7, 1, null`: the row that was
        // moved to the end is the sample's null, and the rows filled are values.
        let shifts = [Limit::AShiftThatLeavesNulls];
        let fills = |with: Expression| {
            call(
                "Expr.shift",
                vec![
                    ("n", Value::Expression(chain_with_integer(1))),
                    ("fill_value", Value::Expression(with)),
                ],
            )
        };
        let value = || chain(literal(), &[]);
        assert_eq!(
            excused_after("b", vec![fills(value()), plain("Expr.last")]),
            shifts
        );
        assert_eq!(
            excused_after("k", vec![fills(value()), plain("Expr.last")]),
            vec![]
        );
        assert_eq!(
            excused_after("k", vec![fills(value()), plain("Expr.first")]),
            vec![]
        );
        // Filled with a null, the first row is null.
        assert_eq!(
            excused_after("k", vec![fills(chain(null(), &[])), plain("Expr.first")]),
            shifts
        );
    }

    #[test]
    fn a_call_that_broadcasts_an_aggregate_gives_the_sample_rows_again() {
        // Polars: `k.min().sub(k).shift(1)` is `null, 0, 0` and
        // `k.min().over(k).shift(1)` is `null, 1, 1`: the one value of the aggregate
        // is given to each of the sample's three rows, so a shift leaves a value
        // last, where one row would be shifted away.
        let shifts = [Limit::AShiftThatLeavesNulls];
        let last = || plain("Expr.last");
        for aggregate in [
            "Expr.min",
            "Expr.max",
            "Expr.sum",
            "Expr.first",
            "Expr.last",
        ] {
            let broadcast =
                |call: Call| excused_after("k", vec![plain(aggregate), call, shift_by(1), last()]);
            assert_eq!(
                broadcast(compared_with("Expr.sub", chain(column("k"), &[]))),
                vec![],
                "{aggregate} then sub"
            );
            assert_eq!(broadcast(over("k", None)), vec![], "{aggregate} then over");
        }
        // The one value alone, shifted, leaves one null.
        assert_eq!(
            excused_after("k", vec![plain("Expr.min"), shift_by(1), last()]),
            shifts
        );
    }

    #[test]
    fn the_minimum_and_maximum_of_only_nulls_are_null_and_the_sum_is_not_known() {
        // Polars: `k.shift(3).min()` and `.max()` are null; `k.shift(3).sum()` is `0`
        // of whole numbers, and `null` of a column of the `Null` dtype.
        let shifts = [Limit::AShiftThatLeavesNulls];
        for id in ["Expr.min", "Expr.max"] {
            assert_eq!(
                excused_after("k", vec![shift_by(3), plain(id)]),
                shifts,
                "{id}"
            );
        }
        for id in ["Expr.sum", "Expr.len"] {
            assert_eq!(
                excused_after("k", vec![shift_by(3), plain(id)]),
                vec![],
                "{id}"
            );
        }
        // One shift leaves a value for them to meet.
        assert_eq!(
            excused_after("k", vec![shift_by(1), plain("Expr.min")]),
            vec![]
        );
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
        // The other key holds a running value, or an aggregate beside a column,
        // either of which may be why Polars refuses the join.
        for other in [
            chain(column("rb"), &["Expr.cum_max"]),
            chain_of(
                column("rb"),
                vec![
                    call("Expr.sum", Vec::new()),
                    compared_with("Expr.gt", chain(column("rk"), &[])),
                ],
            ),
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
        // With the other key plain it is excused, and so it is beside a key that is
        // wholly one value, which Polars allows beside an elementwise key.
        let case = join(compared.clone(), chain(column("key"), &[]));
        assert_eq!(
            polars_refuses(&case, Some(step_place()), Stage::Schema, reason, Vec::new()),
            vec![Limit::ANullComparisonInAKey]
        );
        let case = case_of(call(
            "LazyFrame.join",
            vec![
                ("other", Value::Table("other".to_owned())),
                (
                    "left_on",
                    many(vec![compared, chain(column("rb"), &["Expr.sum"])]),
                ),
                ("right_on", many(vec![chain(column("rk"), &[]); 2])),
            ],
        ));
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
    fn a_hidden_schema_is_excused_at_the_end_of_a_sort_key_which_is_no_output_of_the_step() {
        // Polars sorts by a key without making it a column of the step's table, so a plan that
        // says `Null` where its rows give `Float64`, as a null divided by a whole number does,
        // is not met where the key is the last call too.
        let reason = "the schema says Null and the rows give Float64";
        let sorted =
            |key: Expression| case_of(call("LazyFrame.sort", vec![("by", many(vec![key]))]));
        let divided = chain_of(column("a"), vec![compared_with("Expr.truediv", whole(1))]);
        assert_eq!(
            tables_refuse(&sorted(divided.clone()), link_of("by", 0, 0), ROWS, reason),
            vec![Limit::AHiddenSchema]
        );
        // The step's own schema is shown by what it selects, however the key is.
        let selected = select(vec![divided]);
        assert_eq!(
            tables_refuse(&selected, link_of("exprs", 0, 0), ROWS, reason),
            vec![]
        );
        // Another reason.
        let divided = chain_of(column("a"), vec![compared_with("Expr.truediv", whole(1))]);
        assert_eq!(
            tables_refuse(&sorted(divided), link_of("by", 0, 0), ROWS, "no"),
            vec![]
        );
    }

    #[test]
    fn a_hidden_schema_is_excused_at_the_end_of_a_join_key_which_is_no_output_of_the_step() {
        let reason = "the schema says Null and the rows give Float64";
        let divided = chain_of(column("a"), vec![compared_with("Expr.truediv", whole(1))]);
        let joined = join(chain(column("key"), &[]), divided.clone());
        assert_eq!(
            tables_refuse(&joined, link_of("right_on", 0, 0), ROWS, reason),
            vec![Limit::AHiddenSchema]
        );
        let selected = select(vec![divided]);
        assert_eq!(
            tables_refuse(&selected, link_of("exprs", 0, 0), ROWS, reason),
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
