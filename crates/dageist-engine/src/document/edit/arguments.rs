//! Edits of the arguments of a call: setting one, taking one away, adding an
//! option, and the items, entries and tests of the lists and pairs they hold.
//!
//! The place of an argument is the way from its step: the parameter, then an
//! item of a list, or the key or value of an entry of pairs, then the start or
//! a link of an expression, and the parameter of that call, as far as it goes.

use crate::catalog::{BuildError, Shape};

use super::super::error::{Cause, Place, Segment};
use super::super::model::{Call, Document, Value};
use super::error::{EditError, EditProblem, Thing, Wanted};
use super::path::{Fault, Leaf, with_given};
use super::starting::{default_value, option_start, started_test};

/// What a list, or pairs, is changed by.
enum Change {
    Insert { at: usize, item: Value },
    Remove { at: usize },
}

impl Document {
    /// The document with the argument at `place` set to `value`, in place of
    /// what was there.
    ///
    /// The place ends at a parameter, which is given `value` if it was not
    /// given, or at an item of a list, or at the key or the value of an entry
    /// of pairs, which are replaced. Setting an option, a list of columns, a
    /// literal or a condition is setting the argument of its parameter, or of
    /// an item of it.
    ///
    /// # Errors
    ///
    /// When the place names no step or nothing in it, or the model refuses the
    /// call made with `value`: it has the wrong kind for the parameter, the
    /// parameter is not one of the call's entry, a number is not finite.
    pub fn set_argument(&self, place: &Place, value: Value) -> Result<Self, EditError> {
        let (parent, last) = split(place)?;
        let walk = parent.path().len();
        match &last {
            Segment::Parameter(name) => self.edited_at(
                place,
                walk,
                &Leaf::Call(&|call| with_given(call, name, Some(value.clone()))),
            ),
            Segment::Item(at) => self.edited_at(
                place,
                walk,
                &Leaf::Value(&|list| match list {
                    Value::Many(items) => {
                        let mut items = items.clone();
                        let slot = items
                            .get_mut(*at)
                            .ok_or_else(|| Fault::beyond(1, EditProblem::Nothing))?;
                        *slot = value.clone();
                        Ok(Value::Many(items))
                    }
                    other => Err(does_not_fit(other)),
                }),
            ),
            Segment::Key(at) | Segment::Value(at) => {
                let key = matches!(last, Segment::Key(_));
                self.edited_at(
                    place,
                    walk,
                    &Leaf::Value(&|pairs| match pairs {
                        Value::Pairs(entries) => {
                            let mut entries = entries.clone();
                            let (first, second) = entries
                                .get_mut(*at)
                                .ok_or_else(|| Fault::beyond(1, EditProblem::Nothing))?;
                            *(if key { first } else { second }) = value.clone();
                            Ok(Value::Pairs(entries))
                        }
                        other => Err(does_not_fit(other)),
                    }),
                )
            }
            Segment::Start | Segment::Link(_) => Err(wrong_place(
                place,
                Wanted::Argument,
                Thing::of_segment(&last),
            )),
        }
    }

    /// The document with the argument at `place` taken away, so that the
    /// parameter holds its default again. The place ends at the parameter.
    ///
    /// # Errors
    ///
    /// When the place names no step, nothing in it, or a parameter that is not
    /// given.
    pub fn clear_argument(&self, place: &Place) -> Result<Self, EditError> {
        let (parent, last) = split(place)?;
        let Segment::Parameter(name) = last else {
            return Err(wrong_place(
                place,
                Wanted::Parameter,
                Thing::of_segment(&last),
            ));
        };
        self.edited_at(
            place,
            parent.path().len(),
            &Leaf::Call(&|call| {
                if call.argument(&name).is_none() {
                    return Err(Fault::beyond(1, EditProblem::Nothing));
                }
                with_given(call, &name, None)
            }),
        )
    }

    /// The document with the option `parameter` of the call at `call` added,
    /// set to what it starts with: the catalogue's first value for it, which
    /// for a flag or a choice is the word that differs from its default and
    /// that Polars allows beside the options already set, and for a list a
    /// list of one that is still to be given. An option that is already given
    /// stays as it is. The call is the step itself, or the start or a link of
    /// an expression.
    ///
    /// # Errors
    ///
    /// When the place names no call, `parameter` is not one of its entry's,
    /// Polars refuses every word the option could start with, or it starts with
    /// nothing a document holds, as text that is still to be typed does
    /// ([`EditProblem::NeedsValue`]): give such an option its value with
    /// [`set_argument`](Self::set_argument).
    pub fn add_option(&self, call: &Place, parameter: &str) -> Result<Self, EditError> {
        self.edited_at(
            call,
            call.path().len(),
            &Leaf::Call(&|held| {
                if held.argument(parameter).is_some() {
                    return Ok(held.clone());
                }
                let Some(found) = held
                    .entry()
                    .parameters()
                    .iter()
                    .find(|known| known.name() == parameter)
                else {
                    return Err(unknown_parameter(parameter));
                };
                with_given(held, parameter, Some(option_start(held, found)?))
            }),
        )
    }

    /// The document with the value of `parameter` for the `item`th item of
    /// the parameter it follows set to `value`, for a parameter that holds one
    /// value for each item of another, as a sort's `descending` does for each
    /// key. The call is the step itself, or the start or a link of an
    /// expression.
    ///
    /// The values of the other items stay as they were: those the parameter
    /// holds, or the one value it holds for every item, or its default. Where
    /// the parameter held another number of values than there are items, the
    /// others are made as many.
    ///
    /// # Errors
    ///
    /// When the place names no call, `parameter` is not one of its entry's or
    /// does not hold a value for each item, `item` is not one of the items,
    /// the parameter has no default to fill the others with, or the model
    /// refuses `value`.
    pub fn set_for_item(
        &self,
        call: &Place,
        parameter: &str,
        item: usize,
        value: Value,
    ) -> Result<Self, EditError> {
        self.edited_at(
            call,
            call.path().len(),
            &Leaf::Call(&|held| set_for_item(held, parameter, item, &value)),
        )
    }

    /// The document with `item` put into the list at `list`, so that it is the
    /// `at`th item, counted from zero, and the items from there on follow it.
    /// `at` is the number of items to add it at the end.
    ///
    /// A parameter that holds one value for each item of the list, as a sort's
    /// `descending` does for each key, is given its default for the new item,
    /// where it holds a list.
    ///
    /// # Errors
    ///
    /// When the place names no list, `at` is past the end, or the model
    /// refuses the item: it has the wrong kind.
    pub fn insert_item(&self, list: &Place, at: usize, item: Value) -> Result<Self, EditError> {
        self.change_list(list, list, &Change::Insert { at, item })
    }

    /// The document without the item at `item`, whose place ends at the item.
    /// A parameter that holds one value for each item of the list, as a sort's
    /// `descending` does for each key, loses the value for it, where it holds a
    /// list.
    ///
    /// # Errors
    ///
    /// When the place names no item.
    pub fn remove_item(&self, item: &Place) -> Result<Self, EditError> {
        let (list, last) = split(item)?;
        let Segment::Item(at) = last else {
            return Err(wrong_place(item, Wanted::Item, Thing::of_segment(&last)));
        };
        self.change_list(item, &list, &Change::Remove { at })
    }

    /// The document with a new test put into the list of conditions at `list`,
    /// as the `at`th item: a comparison of a field and a value, each still to be
    /// given, as a new filter starts with. Which comparison it starts as is the
    /// catalogue's to say.
    ///
    /// # Errors
    ///
    /// As [`insert_item`](Self::insert_item).
    pub fn insert_test(&self, list: &Place, at: usize) -> Result<Self, EditError> {
        let test = started_test().map_err(|fault| fault.at(list))?;
        self.insert_item(list, at, test)
    }

    /// The document with an entry put into the pairs at `pairs`, as the `at`th
    /// entry, counted from zero, with the key and value given.
    ///
    /// # Errors
    ///
    /// When the place names no pairs, `at` is past the end, or the model
    /// refuses the entry: a side has the wrong kind.
    pub fn insert_entry(
        &self,
        pairs: &Place,
        at: usize,
        key: Value,
        value: Value,
    ) -> Result<Self, EditError> {
        self.edited_at(
            pairs,
            pairs.path().len(),
            &Leaf::Value(&|held| match held {
                Value::Pairs(entries) => {
                    if at > entries.len() {
                        return Err(Fault::here(EditProblem::OutOfRange {
                            index: at,
                            len: entries.len(),
                        }));
                    }
                    let mut entries = entries.clone();
                    entries.insert(at, (key.clone(), value.clone()));
                    Ok(Value::Pairs(entries))
                }
                other => Err(wrong_found(Wanted::Pairs, other)),
            }),
        )
    }

    /// The document without the entry of pairs whose key or value is at
    /// `entry`: the place ends at either.
    ///
    /// # Errors
    ///
    /// When the place names no entry.
    pub fn remove_entry(&self, entry: &Place) -> Result<Self, EditError> {
        let (pairs, last) = split(entry)?;
        let (Segment::Key(at) | Segment::Value(at)) = last else {
            return Err(wrong_place(
                entry,
                Wanted::EntryPart,
                Thing::of_segment(&last),
            ));
        };
        self.edited_at(
            entry,
            pairs.path().len(),
            &Leaf::Value(&|held| match held {
                Value::Pairs(entries) => {
                    if at >= entries.len() {
                        return Err(Fault::beyond(1, EditProblem::Nothing));
                    }
                    let mut entries = entries.clone();
                    entries.remove(at);
                    Ok(Value::Pairs(entries))
                }
                other => Err(does_not_fit(other)),
            }),
        )
    }

    /// Makes `change` to the list at `list`, as an edit at `place`. A list
    /// that is the argument of a parameter is changed with the call that holds
    /// it, so that what it keeps in step goes with it.
    fn change_list(&self, place: &Place, list: &Place, change: &Change) -> Result<Self, EditError> {
        let (parent, last) = split(list)?;
        let Segment::Parameter(name) = last else {
            return self.edited_at(
                place,
                list.path().len(),
                &Leaf::Value(&|held| changed(held, change)),
            );
        };
        // The way to the call that holds the parameter is one segment shorter than the list's,
        // so what goes wrong in the list is one segment further than the call.
        self.edited_at(
            place,
            parent.path().len(),
            &Leaf::Call(&|call| {
                let held = call
                    .argument(&name)
                    .ok_or_else(|| Fault::beyond(1, EditProblem::Nothing))?;
                let items = changed(held, change).map_err(|fault| fault.shifted(1))?;
                let mut made = with_given(call, &name, Some(items))?;
                for follower in call.entry().parameters() {
                    // The values go with the keys: those of the list, and, for the list that
                    // holds the first keys, the ones that follow.
                    let Shape::PerItem(of) = follower.shape() else {
                        continue;
                    };
                    let Some(offset) = call.key_offset(of, &name) else {
                        continue;
                    };
                    let Some(Value::Many(values)) = made.argument(follower.name()) else {
                        continue;
                    };
                    let mut values = values.clone();
                    match change {
                        Change::Insert { at, .. } => {
                            if let Some(default) = default_value(follower)
                                && offset + *at <= values.len()
                            {
                                values.insert(offset + *at, default);
                            }
                        }
                        Change::Remove { at } => {
                            if offset + *at < values.len() {
                                values.remove(offset + *at);
                            }
                        }
                    }
                    made = with_given(&made, follower.name(), Some(Value::Many(values)))?;
                }
                Ok(made)
            }),
        )
    }
}

/// The list `held` with `change` made to it.
fn changed(held: &Value, change: &Change) -> Result<Value, Fault> {
    let Value::Many(items) = held else {
        return Err(wrong_found(Wanted::List, held));
    };
    let mut items = items.clone();
    match change {
        Change::Insert { at, item } => {
            if *at > items.len() {
                return Err(Fault::here(EditProblem::OutOfRange {
                    index: *at,
                    len: items.len(),
                }));
            }
            items.insert(*at, item.clone());
        }
        Change::Remove { at } => {
            if *at >= items.len() {
                return Err(Fault::beyond(1, EditProblem::Nothing));
            }
            items.remove(*at);
        }
    }
    Ok(Value::Many(items))
}

/// `call` with the value of `parameter` for its `item`th item set.
fn set_for_item(call: &Call, parameter: &str, item: usize, value: &Value) -> Result<Call, Fault> {
    let Some(found) = call
        .entry()
        .parameters()
        .iter()
        .find(|known| known.name() == parameter)
    else {
        return Err(unknown_parameter(parameter));
    };
    let Shape::PerItem(of) = found.shape() else {
        return Err(at_parameter(parameter, EditProblem::NotPerItem));
    };
    let items = call.key_count(of);
    if item >= items {
        return Err(at_parameter(
            parameter,
            EditProblem::OutOfRange {
                index: item,
                len: items,
            },
        ));
    }
    let default = default_value(found);
    let no_default = || at_parameter(parameter, EditProblem::NoDefault);
    let mut values = match call.argument(parameter) {
        Some(Value::Many(held)) => held.clone(),
        Some(one) => vec![one.clone(); items],
        None => vec![default.clone().ok_or_else(no_default)?; items],
    };
    if let Some(filler) = values.last().cloned().or(default) {
        values.resize(items, filler);
    }
    if let Some(slot) = values.get_mut(item) {
        *slot = value.clone();
    }
    with_given(call, parameter, Some(Value::Many(values)))
}

/// The place without its last segment, and the last segment.
fn split(place: &Place) -> Result<(Place, Segment), EditError> {
    let Some((last, before)) = place.path().split_last() else {
        return Err(wrong_place(place, Wanted::PartOfStep, Thing::Step));
    };
    let mut parent = Place::of_step(
        place.step_index().unwrap_or_default(),
        place.step_id().cloned(),
    );
    for segment in before {
        parent = parent.then(segment.clone());
    }
    Ok((parent, last.clone()))
}

fn wrong_place(place: &Place, needs: Wanted, found: Thing) -> EditError {
    EditError::new(place.clone(), EditProblem::WrongPlace { needs, found })
}

fn wrong_found(needs: Wanted, found: &Value) -> Fault {
    Fault::here(EditProblem::WrongPlace {
        needs,
        found: Thing::of_value(found),
    })
}

fn does_not_fit(found: &Value) -> Fault {
    Fault::beyond(
        1,
        EditProblem::DoesNotFit {
            found: Thing::of_value(found),
        },
    )
}

/// A fault at the argument of `parameter` of the call the edit has come to.
fn at_parameter(parameter: &str, problem: EditProblem) -> Fault {
    Fault::inside(vec![Segment::Parameter(parameter.to_owned())], problem)
}

pub(super) fn unknown_parameter(parameter: &str) -> Fault {
    Fault::inside(
        vec![Segment::Parameter(parameter.to_owned())],
        EditProblem::Refused(Cause::Arguments(BuildError::UnknownParameter {
            parameter: parameter.to_owned(),
        })),
    )
}
