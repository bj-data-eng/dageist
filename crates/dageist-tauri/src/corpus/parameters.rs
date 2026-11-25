//! Giving a call every parameter the engine offers, so that each parameter of each entry is
//! given in some document.

use dageist_engine::allowed::Outer;
use dageist_engine::document::{Document, Place};

use super::arguments::sample_for;
use super::documents::call_at;

/// The document with every parameter the engine offers of the call at `place` that it is not
/// given, given a value of its kind. A parameter that the call refuses beside what it is given is
/// left out.
pub(super) fn with_all_parameters(document: &Document, place: &Place, outer: Outer) -> Document {
    let mut document = document.clone();
    let entry = call_at(&document, place).entry();
    for parameter in entry.parameters().iter().filter(|held| held.offered()) {
        if call_at(&document, place)
            .argument(parameter.name())
            .is_some()
        {
            continue;
        }
        let target = place
            .clone()
            .then(dageist_engine::document::Segment::Parameter(
                parameter.name().to_owned(),
            ));
        if let Ok(edited) = document.set_argument(&target, sample_for(parameter, outer)) {
            document = edited;
        }
    }
    document
}

/// The parameters of the call at `place` that the engine offers and that it is given.
pub(super) fn given_parameters(document: &Document, place: &Place) -> Vec<&'static str> {
    let call = call_at(document, place);
    call.entry()
        .parameters()
        .iter()
        .filter(|held| held.offered() && call.argument(held.name()).is_some())
        .map(dageist_engine::catalog::Parameter::name)
        .collect()
}
