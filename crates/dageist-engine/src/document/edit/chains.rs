//! Edits of an expression: what it starts from, and the links that follow.
//!
//! The place of an expression is the way from its step to the argument that
//! holds it. The place of a link is that way and one more segment, the link's.
//! Links are counted from the first after the start.

use crate::allowed::Place as OfferPlace;
use crate::catalog::Build;

use super::super::error::{Place, Position, Segment};
use super::super::model::{Document, Expression, Start, Value};
use super::error::{EditError, EditProblem, Thing, Wanted};
use super::path::{Fault, Leaf};
use super::starting::{link_call, start_calls};
use super::steps::entry_named;

impl Document {
    /// The document with the expression at `expression` made again from
    /// `start`, with the links it had.
    ///
    /// Picking a field, an input or a hole to start from is this edit. To
    /// start from a function, see [`start_with`](Self::start_with), which
    /// also makes a condition's links.
    ///
    /// # Errors
    ///
    /// When the place names no expression, or the model refuses the result:
    /// the start is a call that cannot stand there, or the links that stay
    /// do not follow it, as those of a condition follow only `when`.
    pub fn set_start(&self, expression: &Place, start: Start) -> Result<Self, EditError> {
        self.with_expression(expression, expression.path().len(), &|held| {
            Expression::new(start.clone(), held.links().to_vec()).map_err(Fault::refused)
        })
    }

    /// The document with the expression at `expression` made again to start
    /// from the function `entry`, with the methods it had after its start.
    ///
    /// The function starts as the catalogue says: each argument it must be
    /// given that is an expression is a hole, and `polars.when` also starts
    /// with a test, as [`insert_test`](Self::insert_test) makes one, and is
    /// followed by a `then` and an `otherwise` that are still to be picked.
    /// The links of a condition the expression had are not kept, for they
    /// follow only the `when` they were made for.
    ///
    /// # Errors
    ///
    /// When the place names no expression, `entry` is not in the catalogue or
    /// is not a function that starts an expression, or the model refuses the
    /// result.
    pub fn start_with(&self, expression: &Place, entry: &str) -> Result<Self, EditError> {
        let entry = entry_named(entry, expression)?;
        Position::Start.check(entry).map_err(|cause| {
            EditError::new(
                expression.clone().then(Segment::Start),
                EditProblem::Refused(cause),
            )
        })?;
        let (start, links) = start_calls(entry).map_err(|fault| fault.at(expression))?;
        self.with_expression(expression, expression.path().len(), &|held| {
            let mut all = links.clone();
            all.extend(
                held.links()
                    .iter()
                    .filter(|link| matches!(link.entry().build(), Build::Expression(_)))
                    .cloned(),
            );
            Expression::new(Start::Call(start.clone()), all).map_err(Fault::refused)
        })
    }

    /// The document with a link of the method `entry` put into the expression
    /// at `expression`, so that it is the `at`th link, counted from the first
    /// after the start, and the links from there on follow it. `at` is the
    /// number of links to add it at the end.
    ///
    /// `standing` is where the offers say the link stands: what it follows and
    /// the context of the expression, as the report gives them for the
    /// place a link would be added at, or `None` where it does not know them.
    /// The link starts with the argument the tables allow there as Polars'
    /// default for its first parameter, when it has one, and a hole for every
    /// other expression it must be given. See
    /// [`allowed::default_argument`](crate::allowed::default_argument).
    ///
    /// # Errors
    ///
    /// When the place names no expression, `entry` is not in the catalogue,
    /// `at` is past the end, or the model refuses the result: the call cannot
    /// be a link, or a link of a condition would stand out of order.
    pub fn insert_link(
        &self,
        expression: &Place,
        at: usize,
        entry: &str,
        standing: Option<&OfferPlace>,
    ) -> Result<Self, EditError> {
        let entry = entry_named(entry, expression)?;
        let call = link_call(entry, standing, None).map_err(|fault| fault.at(expression))?;
        self.with_expression(expression, expression.path().len(), &|held| {
            let mut links = held.links().to_vec();
            if at > links.len() {
                return Err(Fault::here(EditProblem::OutOfRange {
                    index: at,
                    len: links.len(),
                }));
            }
            links.insert(at, call.clone());
            Expression::new(held.start().clone(), links).map_err(Fault::refused)
        })
    }

    /// The document with the link at `link` made a link of the method `entry`
    /// instead, in the same place of the expression.
    ///
    /// What the link had is kept where the new method takes it: the argument
    /// it had for its first parameter, when that was picked, goes to the new
    /// method's first parameter; the options it had go to the parameters of
    /// the new method of the same name, kind and shape; and what the new method
    /// must be given besides starts as a link added there does. Whether the
    /// new method accepts what it is given is for the report to say.
    /// `standing` is as for [`insert_link`](Self::insert_link).
    ///
    /// # Errors
    ///
    /// When the place names no link, `entry` is not in the catalogue, or the
    /// model refuses the result.
    pub fn swap_link(
        &self,
        link: &Place,
        entry: &str,
        standing: Option<&OfferPlace>,
    ) -> Result<Self, EditError> {
        let (expression, at) = link_of(link)?;
        let entry = entry_named(entry, link)?;
        self.with_expression(link, expression.path().len(), &|held| {
            let old = held
                .links()
                .get(at)
                .ok_or_else(|| Fault::beyond(1, EditProblem::Nothing))?;
            let made = link_call(entry, standing, Some(old)).map_err(|fault| fault.shifted(1))?;
            let mut links = held.links().to_vec();
            if let Some(slot) = links.get_mut(at) {
                *slot = made;
            }
            Expression::new(held.start().clone(), links).map_err(Fault::refused)
        })
    }

    /// The document without the link at `link`.
    ///
    /// # Errors
    ///
    /// When the place names no link, or the model refuses what is left: a
    /// condition that is left without a link it needs.
    pub fn remove_link(&self, link: &Place) -> Result<Self, EditError> {
        let (expression, at) = link_of(link)?;
        self.with_expression(link, expression.path().len(), &|held| {
            if at >= held.links().len() {
                return Err(Fault::beyond(1, EditProblem::Nothing));
            }
            let mut links = held.links().to_vec();
            links.remove(at);
            Expression::new(held.start().clone(), links).map_err(Fault::refused)
        })
    }

    /// The document with the link at `link` moved to the position `to` among
    /// the links of its expression, and the links between moved over by one.
    ///
    /// # Errors
    ///
    /// When the place names no link, `to` is not the position of a link, or
    /// the model refuses the result: a link of a condition would stand out of
    /// order.
    pub fn move_link(&self, link: &Place, to: usize) -> Result<Self, EditError> {
        let (expression, at) = link_of(link)?;
        self.with_expression(link, expression.path().len(), &|held| {
            let len = held.links().len();
            if at >= len {
                return Err(Fault::beyond(1, EditProblem::Nothing));
            }
            if to >= len {
                return Err(Fault::here(EditProblem::OutOfRange { index: to, len }));
            }
            let mut links = held.links().to_vec();
            let moved = links.remove(at);
            links.insert(to, moved);
            Expression::new(held.start().clone(), links).map_err(Fault::refused)
        })
    }

    /// The document with the expression made again by `change`, which is at
    /// the first `walk` segments of `place`; an error is for `place`.
    fn with_expression(
        &self,
        place: &Place,
        walk: usize,
        change: &dyn Fn(&Expression) -> Result<Expression, Fault>,
    ) -> Result<Self, EditError> {
        self.edited_at(
            place,
            walk,
            &Leaf::Value(&|held| match held {
                Value::Expression(expression) => change(expression).map(Value::Expression),
                other => Err(Fault::here(EditProblem::WrongPlace {
                    needs: Wanted::Expression,
                    found: Thing::of_value(other),
                })),
            }),
        )
    }
}

/// The place of the expression a link is of, and where the link is.
fn link_of(link: &Place) -> Result<(Place, usize), EditError> {
    let wrong = |found| {
        EditError::new(
            link.clone(),
            EditProblem::WrongPlace {
                needs: Wanted::Link,
                found,
            },
        )
    };
    let Some((last, before)) = link.path().split_last() else {
        return Err(wrong(Thing::Step));
    };
    let Segment::Link(at) = last else {
        return Err(wrong(Thing::of_segment(last)));
    };
    let mut expression = Place::of_step(
        link.step_index().unwrap_or_default(),
        link.step_id().cloned(),
    );
    for segment in before {
        expression = expression.then(segment.clone());
    }
    Ok((expression, *at))
}
