//! What the host's tests write as fixtures for the view, built from the views
//! of the catalogue: the words of its closed lists.

use dageist_engine::catalog::{Applies, ValueType};

use super::{applies_view, value_type_view};

/// The words of the catalogue's closed vocabularies, written out from the
/// engine's own lists of variants, for the view's tests to check its decoders
/// against.
pub(crate) fn vocabulary() -> serde_json::Value {
    serde_json::json!({
        "applies": Applies::ALL.map(applies_view),
        "valueTypes": ValueType::ALL.map(value_type_view),
    })
}
