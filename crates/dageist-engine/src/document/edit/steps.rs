//! Edits of the list of steps: adding a step, changing what a step does,
//! taking one away and moving one.

use crate::catalog::{Entry, StepFrame};

use super::super::error::{Cause, Place};
use super::super::model::{Document, Step, StepId};
use super::error::{EditError, EditProblem};
use super::starting::{step_call, step_links};

impl Document {
    /// The document with a new step of the entry `entry` at the position `at`,
    /// so that it is the `at`th step, counted from zero, and the steps from
    /// there on follow it. `at` is the number of steps to add it at the end.
    ///
    /// The step starts as the catalogue says a new step of `entry` does, on a
    /// table of the columns `frame` names and with the tables it names to
    /// bring in: see [`start_step`](crate::catalog::start_step). What it
    /// leaves to be given, an expression is a hole and any other argument is
    /// left out, so the step may be incomplete or refused when it is run. Its
    /// identifier is the entry's name, such as `with_columns`, and a number
    /// after it where a step of the document has it already: `with_columns_2`.
    ///
    /// # Errors
    ///
    /// When `entry` is not in the catalogue or starts no step, `at` is past the
    /// end, or the model refuses the start.
    pub fn add_step(
        &self,
        at: usize,
        entry: &str,
        frame: &StepFrame<'_>,
    ) -> Result<Self, EditError> {
        let place = Place::of_step(at, None);
        let entry = entry_named(entry, &Place::default())?;
        let len = self.steps().len();
        if at > len {
            return Err(EditError::new(
                Place::default(),
                EditProblem::OutOfRange { index: at, len },
            ));
        }
        let call = step_call(entry, frame).map_err(|fault| fault.at(&place))?;
        let links = step_links(entry, frame).map_err(|fault| fault.at(&place))?;
        let id = self.fresh_id(entry)?;
        let step = Step::chain(id.clone(), call, links)
            .map_err(|refusal| EditError::of_refusal(refusal.in_step(at, Some(id))))?;
        let mut steps = self.steps().to_vec();
        steps.insert(at, step);
        Self::new(steps).map_err(EditError::of_refusal)
    }

    /// The document with the step at `step` made a step of the entry `entry`
    /// instead, started as [`add_step`](Self::add_step) starts one, and keeping
    /// its identifier and its place in the list. What the step was given is
    /// not kept.
    ///
    /// # Errors
    ///
    /// When the place names no step, or one by an identifier it no longer has,
    /// `entry` is not in the catalogue or starts no step, or the model refuses
    /// the start.
    pub fn swap_step(
        &self,
        step: &Place,
        entry: &str,
        frame: &StepFrame<'_>,
    ) -> Result<Self, EditError> {
        let (index, found) = self.step_named(step)?;
        let entry = entry_named(entry, step)?;
        let call = step_call(entry, frame).map_err(|fault| fault.at(step))?;
        let links = step_links(entry, frame).map_err(|fault| fault.at(step))?;
        let made = Step::chain(found.id().clone(), call, links).map_err(|refusal| {
            EditError::of_refusal(refusal.in_step(index, Some(found.id().clone())))
        })?;
        self.with_step(index, made)
    }

    /// The document without the step at `step`.
    ///
    /// # Errors
    ///
    /// When the place names no step, or one by an identifier it no longer has.
    pub fn remove_step(&self, step: &Place) -> Result<Self, EditError> {
        let (index, _) = self.step_named(step)?;
        let mut steps = self.steps().to_vec();
        steps.remove(index);
        Self::new(steps).map_err(EditError::of_refusal)
    }

    /// The document with the step at `step` moved to the position `to`, so
    /// that it is the `to`th step, counted from zero, and the steps between
    /// move over by one.
    ///
    /// # Errors
    ///
    /// When the place names no step, or one by an identifier it no longer has,
    /// or `to` is not the position of a step.
    pub fn move_step(&self, step: &Place, to: usize) -> Result<Self, EditError> {
        let (index, _) = self.step_named(step)?;
        let len = self.steps().len();
        if to >= len {
            return Err(EditError::new(
                step.clone(),
                EditProblem::OutOfRange { index: to, len },
            ));
        }
        let mut steps = self.steps().to_vec();
        let moved = steps.remove(index);
        steps.insert(to, moved);
        Self::new(steps).map_err(EditError::of_refusal)
    }

    /// An identifier no step of the document has, made from the entry's name.
    fn fresh_id(&self, entry: &Entry) -> Result<StepId, EditError> {
        let id = entry.id().as_str();
        let stem = id.rsplit('.').next().unwrap_or(id);
        let taken = |text: &str| self.steps().iter().any(|step| step.id().as_str() == text);
        let text = std::iter::once(stem.to_owned())
            .chain((2..).map(|number| format!("{stem}_{number}")))
            .find(|text| !taken(text))
            .unwrap_or_else(|| stem.to_owned());
        StepId::new(text)
            .map_err(|_| EditError::new(Place::default(), EditProblem::Refused(Cause::EmptyStepId)))
    }
}

/// The entry with the identifier `id`, which the error names `place` for when
/// the catalogue has none.
pub(super) fn entry_named(id: &str, place: &Place) -> Result<&'static Entry, EditError> {
    crate::catalog::entry(id)
        .ok_or_else(|| EditError::new(place.clone(), EditProblem::UnknownEntry(id.to_owned())))
}
