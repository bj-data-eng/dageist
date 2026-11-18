//! What the host's tests write as fixtures for the view, built from the views
//! of the catalogue: the words of its closed lists.

use dageist_engine::catalog::{Applies, Setting, ValueType};

use super::{applies_view, setting_view, value_type_view};

/// One setting of every kind the engine reports. The `match` has no wildcard, so a kind the
/// engine adds fails to compile here until it is listed, and so is in the vocabulary.
fn every_setting() -> Vec<Setting> {
    let all = vec![
        Setting::ToGive,
        Setting::Flag(true),
        Setting::Integer(1),
        Setting::Float(1.5),
        Setting::Text("text".to_owned()),
        Setting::Word("word".to_owned()),
        Setting::Field("column".to_owned()),
        Setting::Element("first"),
        Setting::Comparison {
            entry: "Expr.eq",
            field: Box::new(Setting::ToGive),
            other: Box::new(Setting::ToGive),
        },
        Setting::List(Vec::new()),
        Setting::Pairs(Vec::new()),
    ];
    for setting in &all {
        match setting {
            Setting::ToGive
            | Setting::Flag(_)
            | Setting::Integer(_)
            | Setting::Float(_)
            | Setting::Text(_)
            | Setting::Word(_)
            | Setting::Field(_)
            | Setting::Element(_)
            | Setting::Comparison { .. }
            | Setting::List(_)
            | Setting::Pairs(_) => {}
        }
    }
    all
}

/// The words of the catalogue's closed vocabularies, written out from the
/// engine's own lists of variants, for the view's tests to check its decoders
/// against.
pub(crate) fn vocabulary() -> serde_json::Value {
    let settings: Vec<serde_json::Value> = every_setting()
        .iter()
        .map(|setting| {
            serde_json::to_value(setting_view(setting)).expect("a setting serializes")["kind"]
                .clone()
        })
        .collect();
    serde_json::json!({
        "applies": Applies::ALL.map(applies_view),
        "valueTypes": ValueType::ALL.map(value_type_view),
        "settings": settings,
    })
}
