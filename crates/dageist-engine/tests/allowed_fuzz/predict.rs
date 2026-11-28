//! What the tables and the named rules say of a case, without running Polars.
//!
//! The engine's own walk ([`Step::walk`]) does it: it follows the document as
//! running it does, asks [`allowed::offer`] of every call at the place the
//! chain has come to, asks whether every expression a position holds may end
//! there, and asks [`allowed::options`] of the options a step is given. This
//! reads its answer as a verdict on the case: allowed when nothing is refused.
//!
//! The tables are measured, so what they cannot say is not guessed: a chain
//! whose dtype is not known where a method follows it is *undetermined* from
//! there on, and a case that is only undetermined is compared for what was
//! predicted before that.

use dageist_engine::allowed::{self, Inner, Outer, RefusalSource, Representative, Shape};
use dageist_engine::document::{Place, Walk, WalkRefusal, WalkedExpression};

use super::case::{Case, STEP};
use super::frames::Frames;

/// Who refused a case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Polars, as the tables recorded it.
    Polars,
    /// A named rule, which Polars does not hold.
    Rule(&'static str),
    /// The tables hold nothing for what was asked: a gap in them, or a case
    /// they cannot be asked about.
    Silent,
}

/// A refusal the tables and the rules give, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub source: Source,
    /// Where, in words.
    pub at: String,
    /// Where, in the document.
    pub place: Place,
    /// The stage at which Polars refused it, as the tables recorded it, when
    /// Polars did.
    pub stage: Option<allowed::Stage>,
    pub reason: String,
    /// Whether the call was refused without the dtype of an argument being known:
    /// it is refused for every dtype that argument might have, and the reason is
    /// Polars' for one of them.
    pub argument_unknown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Every call may stand where it does, and every chain may end where it
    /// ends.
    Allows,
    /// Something is refused.
    Refuses(Refused),
    /// Nothing is refused, but the tables could not be asked past a point.
    Undetermined(String),
}

/// What the tables say an expression has at its start and after each link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Traced {
    pub place: Place,
    pub start: Option<Representative>,
    pub links: Vec<Option<Representative>>,
}

#[derive(Debug, Clone)]
pub struct Prediction {
    pub verdict: Verdict,
    pub expressions: Vec<Traced>,
}

/// The class of what a place follows, when it follows a value of one. Where the values are
/// evaluated on the rows of each group, which an aggregate and a predicate of a group by are, what
/// is many values is a list of them for each group: a list of the scalar dtype the tables measured,
/// and no class for a list of a dtype that is no scalar.
fn class(place: Option<allowed::Place>, by_group: bool) -> Option<Representative> {
    match place?.after() {
        allowed::After::Value { dtype, shape } if by_group && shape != Shape::One => {
            Inner::of_class(dtype).map(Representative::List)
        }
        allowed::After::Value { dtype, .. } => Some(dtype),
        _ => None,
    }
}

/// What the walk found the expression to have after its start and each link:
/// what the next call follows, which is not what a call offered without an
/// argument's dtype says it gives.
fn traced(expression: &WalkedExpression) -> Traced {
    let places = expression.places();
    let by_group = expression
        .context()
        .is_some_and(|context| matches!(context.outer(), Outer::Aggregate | Outer::Having));
    Traced {
        place: expression.place().clone(),
        start: places
            .first()
            .copied()
            .and_then(|place| class(place, by_group)),
        links: places
            .iter()
            .skip(1)
            .copied()
            .map(|place| class(place, by_group))
            .collect(),
    }
}

fn refused(found: &WalkRefusal) -> Refused {
    let refusal = found.refusal();
    let (source, stage) = match refusal.source() {
        RefusalSource::Polars(stage) => (Source::Polars, Some(stage)),
        RefusalSource::Rule(rule) => (Source::Rule(rule.name()), None),
        RefusalSource::Structure => (Source::Silent, None),
    };
    Refused {
        source,
        at: found.place().to_string(),
        place: found.place().clone(),
        stage,
        reason: if found.at_end() {
            format!("it cannot end here: {}", refusal.reason())
        } else {
            refusal.reason().to_owned()
        },
        argument_unknown: found.argument_unknown(),
    }
}

/// The engine's walk of the case's step: what the tables and the rules say of
/// it, and what Polars evaluates of it.
fn walk_of(case: &Case) -> Walk {
    let step = &case.document.steps()[0];
    debug_assert_eq!(step.id().as_str(), STEP);
    let frames = Frames::with_key(case.receiver, case.argument, &case.structs, None);
    step.walk(0, frames.rows.schema(), &frames.inputs(false))
}

pub fn predict(case: &Case) -> Prediction {
    let walk = walk_of(case);
    let verdict = match (walk.refusal(), walk.not_known()) {
        (Some(found), _) => Verdict::Refuses(refused(found)),
        (None, Some(reason)) => Verdict::Undetermined(reason.to_owned()),
        (None, None) => Verdict::Allows,
    };
    Prediction {
        verdict,
        expressions: walk.expressions().iter().map(traced).collect(),
    }
}
