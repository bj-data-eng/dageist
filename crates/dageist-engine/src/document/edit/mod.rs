//! The editing of a document: see the documentation of [`Document`](super::Document) for what
//! each edit is and how a place names where it is made.

mod arguments;
mod chains;
mod error;
mod path;
mod starting;
mod steps;

pub use error::{EditError, EditProblem};

use super::error::Place;
use super::model::{Document, Step};
use path::{Leaf, rebuild_call};

impl Document {
    /// The step `place` names, and where it is. The place names a step by its
    /// number, and by its identifier too when it has one, which must be the
    /// identifier of the step there.
    fn step_named(&self, place: &Place) -> Result<(usize, &Step), EditError> {
        let missing = || {
            EditError::new(
                place.clone(),
                EditProblem::NoStep {
                    steps: self.steps().len(),
                },
            )
        };
        let index = place.step_index().ok_or_else(missing)?;
        let step = self.steps().get(index).ok_or_else(missing)?;
        match place.step_id() {
            Some(id) if id != step.id() => Err(EditError::new(
                place.clone(),
                EditProblem::StepChanged {
                    found: step.id().clone(),
                },
            )),
            _ => Ok((index, step)),
        }
    }

    /// The document with `step` in place of the step at `index`.
    fn with_step(&self, index: usize, step: Step) -> Result<Self, EditError> {
        let mut steps = self.steps().to_vec();
        if let Some(slot) = steps.get_mut(index) {
            *slot = step;
        }
        Self::new(steps).map_err(EditError::of_refusal)
    }

    /// The document with the step `place` names made again with `leaf`
    /// applied where the first `walk` segments of its way end. An error is for
    /// `place`.
    fn edited_at(&self, place: &Place, walk: usize, leaf: &Leaf<'_>) -> Result<Self, EditError> {
        let (index, step) = self.step_named(place)?;
        let way = place.path().get(..walk).unwrap_or_default();
        let call = rebuild_call(step.call(), way, 0, leaf).map_err(|fault| fault.at(place))?;
        let made = Step::new(step.id().clone(), call).map_err(|refusal| {
            EditError::of_refusal(refusal.in_step(index, Some(step.id().clone())))
        })?;
        self.with_step(index, made)
    }
}
