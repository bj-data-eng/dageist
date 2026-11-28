//! What Polars does with a step that the tables refuse, asked of Polars itself.
//!
//! The step engine does not run a step whose walk is refused: it reports the walk's words and
//! neither plans the step nor collects its rows. A target that checks the tables against Polars
//! needs Polars' own answer to such a step, or it could never find a table that is stricter than
//! Polars. So the step is built by [`ask_polars`], through the catalogue's own build functions and
//! without any of the engine's refusals, and run as [`observe`](super::observe) runs a step: its
//! schema, then its rows on the table with three rows, then on the table with none.
//!
//! Polars panics on some of what the tables refuse, so every run is made under `catch_unwind`,
//! and a panic is Polars' answer, as it is for the step engine. A panic on a thread of Polars' own
//! that does not reach the caller is not recovered; the campaign's deadline ends a case it leaves
//! unable to end.
//!
//! A step is asked this way only when the walk refuses it, and not for a rule that is there
//! because Polars panics on what it refuses: a method that depends on the element dtype of a list
//! or an array is not asked of an element the tables do not measure, nor one Polars has no
//! implementation of. Every other step is asked of the step engine, whose answer is Polars' and
//! the engine's own checks together.
//!
//! [`ask_polars`] copies what the step engine builds, as the engine's builder is private. So that
//! the copy does not drift from it, a case of a campaign whose step the walk allows is asked of
//! both, and the two answers, which Polars refuses and where, and the dtype of each expression,
//! must be the same: [`drift_between`](super::observe::drift_between) names a difference, which
//! is a finding of the target.

use std::panic::{AssertUnwindSafe, catch_unwind};

use dageist_engine::allowed::{RefusalSource, Rule};
use dageist_engine::document::Document;
use polars::prelude::DataFrame;

use super::ask_polars::{self, Connected, Said};
use super::frames::Frames;
use super::observe::{
    Observed, Refusal, Seen, SeenExpression, Stage, panic_text, schema_disagrees,
};

/// How many rows the rows stage collects, as the step engine's preview of a step does.
const ROWS: usize = 100;

/// What the walk says of the step of `document` on the tables `frames` holds, as far as asking
/// Polars of it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walked {
    /// The walk refuses it, and Polars may be asked.
    AskPolars,
    /// The walk refuses it by a rule that is there because Polars panics on what it refuses: no
    /// run of Polars is asked.
    NotAsked,
    /// The walk refuses nothing and knows the step to its end.
    Allows,
    /// The walk refuses nothing and could not follow the step to its end, or could not be made.
    NotKnown,
}

/// What the walk says of the step of `document` on the tables `frames` holds.
pub fn walked(document: &Document, frames: &Frames) -> Walked {
    // A walk that panics is not asked again here: the step engine's run meets the same panic.
    catch_unwind(AssertUnwindSafe(|| {
        let walk = document.steps()[0].walk(0, frames.rows.schema(), &frames.inputs(false));
        match (walk.refusal(), walk.not_known()) {
            (Some(found), _) => {
                if matches!(
                    found.refusal().source(),
                    RefusalSource::Rule(
                        Rule::ElementDtypeIsImplemented | Rule::ElementDtypeIsMeasured
                    )
                ) {
                    Walked::NotAsked
                } else {
                    Walked::AskPolars
                }
            }
            (None, Some(_)) => Walked::NotKnown,
            (None, None) => Walked::Allows,
        }
    }))
    .unwrap_or(Walked::NotKnown)
}

fn seen(said: Said) -> Seen {
    match said {
        Said::Dtype(dtype, name, class) => Seen::Dtype(dtype, name, class),
        Said::Unsaid => Seen::Unsaid,
        Said::Refused(reason) => Seen::Refused(reason),
    }
}

/// What Polars does with the step of `document` on `frames`, or `None` for a step it cannot be
/// asked about: one that holds a hole, which the cases of a campaign never hold.
pub fn ask(document: &Document, frames: &Frames) -> Option<Observed> {
    let step = &document.steps()[0];
    let rows_tables = |name: &str| frames.table(name, false);
    let connected = Connected::tables(&rows_tables);
    let planned = catch_unwind(AssertUnwindSafe(|| {
        ask_polars::plan(step, 0, &frames.rows, connected)
    }));
    let planned = match planned {
        Err(payload) => {
            return Some(Observed {
                refusal: Some(Refusal {
                    stage: Stage::Schema,
                    at: "the step".to_owned(),
                    place: None,
                    reason: panic_text(payload.as_ref()),
                }),
                ..Observed::default()
            });
        }
        Ok(planned) => planned?,
    };
    let mut observed = Observed {
        expressions: planned
            .traces
            .into_iter()
            .map(|traced| SeenExpression {
                place: traced.place,
                start: seen(traced.start),
                links: traced.links.into_iter().map(seen).collect(),
            })
            .collect(),
        ..Observed::default()
    };
    let schema = match planned.schema {
        Ok(schema) => schema,
        Err(refused) => {
            observed.refusal = Some(Refusal {
                stage: Stage::Schema,
                at: refused.place.to_string(),
                place: Some(refused.place),
                reason: refused.reason,
            });
            return Some(observed);
        }
    };
    for (stage, empty, working) in [
        (Stage::Rows, false, &frames.rows),
        (Stage::Empty, true, &frames.empty),
    ] {
        let tables = |name: &str| frames.table(name, empty);
        let collected = catch_unwind(AssertUnwindSafe(|| {
            ask_polars::collect(step, 0, working, Connected::tables(&tables), ROWS)
        }));
        let reason = match collected {
            Err(payload) => panic_text(payload.as_ref()),
            Ok(Err(reason)) => reason,
            Ok(Ok(rows)) => match (&stage, schema_disagrees(&schema, &rows)) {
                (Stage::Rows, Some(reason)) => reason,
                _ => continue,
            },
        };
        observed.refusal = Some(Refusal {
            stage,
            at: "the rows".to_owned(),
            place: None,
            reason,
        });
        return Some(observed);
    }
    Some(observed)
}

/// The first rows of the table the step of `document` gives on the sample, as Polars gives them
/// whatever the tables say of the step, or `None` where it does not: the part of a step that a
/// limit asks about on its own.
pub fn first_rows(document: &Document, frames: &Frames) -> Option<DataFrame> {
    let rows_tables = |name: &str| frames.table(name, false);
    catch_unwind(AssertUnwindSafe(|| {
        ask_polars::collect(
            &document.steps()[0],
            0,
            &frames.rows,
            Connected::tables(&rows_tables),
            ROWS,
        )
    }))
    .ok()?
    .ok()
}
