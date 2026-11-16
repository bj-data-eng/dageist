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

use dageist_engine::document::{Call, Expression, Start, Value};

use super::case::Case;
use super::observe::Observed;
use super::predict::{Prediction, Source, Verdict};

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
    /// `max` of a `Struct` there, which `select` refuses. A table with no rows
    /// has no groups, and the values of an aggregate over none have no dtype.
    AGroupAggregate,
    /// An aggregate in a key, a join's or a window's, beside a column is not
    /// elementwise, and Polars refuses it. The context table measured the entry
    /// as the whole key.
    AnAggregateInAKey,
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
    /// The tables are asked what a chain gives so that a place that needs a
    /// `Boolean` can be checked, and a start, or a method that gives what its
    /// arguments decide, gives nothing to check: the dtype of a column is the
    /// caller's to say, and [`offer`](dageist_engine::allowed::offer) is not told it.
    ABooleanIsNeededAndTheDtypeUnknown,
}

impl Limit {
    /// Every limit, which the kept cases cover each by one.
    pub const ALL: [Self; 10] = [
        Self::OptionsChangeTheResult,
        Self::ALiteral,
        Self::AnAggregateGivesOneValue,
        Self::AGroupAggregate,
        Self::AnAggregateInAKey,
        Self::TheLengthsOfTheRows,
        Self::AnOverflow,
        Self::TheBranchesAreOne,
        Self::APanic,
        Self::ABooleanIsNeededAndTheDtypeUnknown,
    ];

    /// The name a kept case is filed under.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::OptionsChangeTheResult => "options-change-the-result",
            Self::ALiteral => "a-literal",
            Self::AnAggregateGivesOneValue => "an-aggregate-gives-one-value",
            Self::AGroupAggregate => "a-group-aggregate",
            Self::AnAggregateInAKey => "an-aggregate-in-a-key",
            Self::TheLengthsOfTheRows => "the-lengths-of-the-rows",
            Self::AnOverflow => "an-overflow",
            Self::TheBranchesAreOne => "the-branches-are-one",
            Self::APanic => "a-panic",
            Self::ABooleanIsNeededAndTheDtypeUnknown => "a-boolean-is-needed",
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
                "a pivot's aggregate is run on groups, not on a column in select"
            }
            Self::AnAggregateInAKey => "an aggregate in a key beside a column is not elementwise",
            Self::ABooleanIsNeededAndTheDtypeUnknown => {
                "the dtype a column or an argument decides is not asked, where a Boolean is needed"
            }
        }
    }
}

/// Whether the limits excuse nothing: set by `DAGEIST_FUZZ_STRICT`.
pub fn strict() -> bool {
    std::env::var_os("DAGEIST_FUZZ_STRICT").is_some()
}

/// Every expression the document holds, with the call and the parameter that
/// hold it, and whether that is the step itself.
fn visit(call: &Call, held_by_step: bool, f: &mut dyn FnMut(&Call, &str, &Expression, bool)) {
    fn value(
        call: &Call,
        parameter: &str,
        held_by_step: bool,
        v: &Value,
        f: &mut dyn FnMut(&Call, &str, &Expression, bool),
    ) {
        match v {
            Value::Expression(expression) => {
                f(call, parameter, expression, held_by_step);
                if let Start::Call(start) = expression.start() {
                    visit(start, false, f);
                }
                for link in expression.links() {
                    visit(link, false, f);
                }
            }
            Value::Many(items) => {
                for item in items {
                    value(call, parameter, held_by_step, item, f);
                }
            }
            Value::Pairs(pairs) => {
                for (key, entry) in pairs {
                    value(call, parameter, held_by_step, key, f);
                    value(call, parameter, held_by_step, entry, f);
                }
            }
            _ => {}
        }
    }
    for given in call.arguments() {
        value(call, given.parameter(), held_by_step, given.value(), f);
    }
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

struct Facts {
    explodes: bool,
    literal: bool,
    aggregate_ends: bool,
    pivot: bool,
    same_branches: bool,
}

fn facts(case: &Case) -> Facts {
    let step = case.document.steps()[0].call();
    let pivot = is(step, "LazyFrame.pivot");
    let mut facts = Facts {
        explodes: false,
        literal: false,
        aggregate_ends: false,
        pivot,
        same_branches: false,
    };
    visit(step, true, &mut |owner, parameter, expression, by_step| {
        // The values `shift` takes, and the arguments of a pivot's group, are
        // literals as the tables measured them.
        let measured =
            (is(owner, "Expr.shift") && matches!(parameter, "n" | "fill_value")) || pivot;
        if let Start::Call(start) = expression.start()
            && is(start, "polars.lit")
            && !measured
        {
            facts.literal = true;
        }
        // One value stays one value through the elementwise methods after it,
        // and is broadcast against the columns it meets, in a predicate of a
        // condition as in a key.
        let _ = by_step;
        if expression
            .links()
            .iter()
            .any(|link| ONE_VALUE.contains(&link.entry().id().as_str()))
        {
            facts.aggregate_ends = true;
        }
        if let Start::Call(start) = expression.start()
            && is(start, "polars.when")
        {
            let values: Vec<&Expression> = expression
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
            if values.len() > 1 && values.windows(2).all(|pair| pair[0] == pair[1]) {
                facts.same_branches = true;
            }
        }
        for link in expression.links() {
            if is(link, "Expr.over")
                && matches!(link.argument("mapping_strategy"), Some(Value::Choice(word)) if word == "explode")
            {
                facts.explodes = true;
            }
        }
    });
    facts
}

/// Whether some chain the tables were asked about has no dtype to check where
/// a `Boolean` is needed.
fn dtype_unknown(prediction: &Prediction) -> bool {
    prediction
        .expressions
        .iter()
        .any(|expression| expression.links.last().is_none_or(Option::is_none))
}

/// The limit that excuses the disagreement of `prediction` and `observed` for
/// `case`, if one does.
pub fn excuse(case: &Case, prediction: &Prediction, observed: &Observed) -> Option<Limit> {
    // `DAGEIST_FUZZ_STRICT` holds the tables to everything, to see what a limit
    // excuses.
    if strict() {
        return None;
    }
    let facts = facts(case);
    match (&prediction.verdict, &observed.refusal) {
        (Verdict::Allows, Some(refusal)) => {
            let reason = refusal.reason.as_str();
            if facts.explodes {
                Some(Limit::OptionsChangeTheResult)
            } else if reason.starts_with("Polars panicked") && !reason.contains("with overflow") {
                Some(Limit::APanic)
            } else if reason.contains("elementwise") {
                Some(Limit::AnAggregateInAKey)
            } else if facts.pivot {
                Some(Limit::AGroupAggregate)
            } else if reason.contains("Boolean") && dtype_unknown(prediction) {
                Some(Limit::ABooleanIsNeededAndTheDtypeUnknown)
            } else if reason.contains("with overflow") {
                Some(Limit::AnOverflow)
            } else if reason.contains("lengths don't match") || reason.contains("lengths differed")
            {
                Some(Limit::TheLengthsOfTheRows)
            } else {
                None
            }
        }
        (Verdict::Refuses(refused), None) if refused.source == Source::Polars => {
            if facts.explodes {
                Some(Limit::OptionsChangeTheResult)
            } else if facts.pivot {
                Some(Limit::AGroupAggregate)
            } else if refused.reason.starts_with("Polars panicked") {
                Some(Limit::APanic)
            } else if facts.same_branches {
                Some(Limit::TheBranchesAreOne)
            } else if facts.literal {
                Some(Limit::ALiteral)
            } else if facts.aggregate_ends {
                Some(Limit::AnAggregateGivesOneValue)
            } else {
                None
            }
        }
        _ => None,
    }
}
