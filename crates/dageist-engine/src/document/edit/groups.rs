//! Edits of the calls that follow `group_by` in a step that groups: filtering the groups with
//! `having`, and which call finishes the grouping.
//!
//! The place of a call is the place of its step and one more segment, the call's: a
//! [`Segment::Link`], counted from the first call after `group_by`. The calls of a step are held
//! as the links of an expression are, so the arguments of a call are edited at the place of the
//! call and the way down into them, by the edits of arguments.

use super::super::error::{Place, Segment};
use super::super::model::{Document, Step};
use super::error::{EditError, EditProblem, Thing, Wanted};
use super::path::Fault;
use super::starting::group_call;
use super::steps::entry_named;

impl Document {
    /// The document with a call of the group by, `entry`, put into the step at `step`, so that it
    /// is the `at`th call after `group_by`, counted from zero, and the calls from there on follow
    /// it. `at` is the number of calls to add it at the end.
    ///
    /// The call starts as the catalogue says a new call of a group by does: see
    /// [`start_link`](crate::catalog::start_link). `having` is added before the call that
    /// finishes the grouping, and no other call is added to a grouping that is finished.
    ///
    /// # Errors
    ///
    /// When the place names no step, `entry` is not in the catalogue or is no call of a group by,
    /// `at` is past the end, or the model refuses the result: the step does not group, has its
    /// `having` already, or would hold two calls that finish it.
    pub fn insert_group_call(
        &self,
        step: &Place,
        at: usize,
        entry: &str,
    ) -> Result<Self, EditError> {
        let (index, held) = self.step_named(step)?;
        let entry = entry_named(entry, step)?;
        let call = group_call(entry, None).map_err(|fault| fault.at(step))?;
        let mut links = held.links().to_vec();
        if at > links.len() {
            return Err(EditError::new(
                step.clone(),
                EditProblem::OutOfRange {
                    index: at,
                    len: links.len(),
                },
            ));
        }
        links.insert(at, call);
        self.with_chain(index, held, links)
    }

    /// The document with the call at `call` made a call of the entry `entry` instead, in the same
    /// place of the step.
    ///
    /// What the call had is kept where the new call takes it: the arguments of the parameters of
    /// the same name, kind and shape, such as `ignore_nulls` of `first` and `last`. Every other
    /// parameter of the new call starts as a new call's does.
    ///
    /// # Errors
    ///
    /// When the place names no call of a step, `entry` is not in the catalogue or is no call of a
    /// group by, or the model refuses the result: `having` for the call that finishes the
    /// grouping, or a call that finishes it for `having`, which leaves the grouping unfinished.
    pub fn swap_group_call(&self, call: &Place, entry: &str) -> Result<Self, EditError> {
        let (index, held, at) = self.group_call_named(call)?;
        let entry = entry_named(entry, call)?;
        let old = held
            .links()
            .get(at)
            .ok_or_else(|| Fault::beyond(1, EditProblem::Nothing).at(call))?;
        let made = group_call(entry, Some(old)).map_err(|fault| fault.shifted(1).at(call))?;
        let mut links = held.links().to_vec();
        if let Some(slot) = links.get_mut(at) {
            *slot = made;
        }
        self.with_chain(index, held, links)
    }

    /// The document without the call at `call`.
    ///
    /// # Errors
    ///
    /// When the place names no call of a step, or the model refuses what is left: a grouping that
    /// no call finishes.
    pub fn remove_group_call(&self, call: &Place) -> Result<Self, EditError> {
        let (index, held, at) = self.group_call_named(call)?;
        let mut links = held.links().to_vec();
        if at >= links.len() {
            return Err(Fault::beyond(1, EditProblem::Nothing).at(call));
        }
        links.remove(at);
        self.with_chain(index, held, links)
    }

    /// The step `call` is a call of, where it is, and the call's place among the calls after
    /// `group_by`.
    fn group_call_named(&self, call: &Place) -> Result<(usize, &Step, usize), EditError> {
        let wrong = |found| {
            EditError::new(
                call.clone(),
                EditProblem::WrongPlace {
                    needs: Wanted::GroupCall,
                    found,
                },
            )
        };
        let (index, held) = self.step_named(call)?;
        match call.path() {
            [] => Err(wrong(Thing::Step)),
            [Segment::Link(at)] => Ok((index, held, *at)),
            [.., last] => Err(wrong(Thing::of_segment(last))),
        }
    }

    /// The document with the step at `index` made again with `links` after its own call.
    fn with_chain(
        &self,
        index: usize,
        step: &Step,
        links: Vec<super::super::model::Call>,
    ) -> Result<Self, EditError> {
        let made =
            Step::chain(step.id().clone(), step.call().clone(), links).map_err(|refusal| {
                EditError::of_refusal(refusal.in_step(index, Some(step.id().clone())))
            })?;
        self.with_step(index, made)
    }
}
