//! The identity of the running core, as the view receives it.

use serde::Serialize;

/// Identity of the running dageist core, as presented to the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IdentityView {
    name: &'static str,
    version: &'static str,
}

/// Who the core is: its crate's name and version.
pub fn identity_view() -> IdentityView {
    IdentityView {
        name: dageist::crate_name(),
        version: dageist::crate_version(),
    }
}

/// Reports the identity of the dageist core to the view.
#[tauri::command]
pub fn core_identity() -> IdentityView {
    identity_view()
}

#[cfg(test)]
mod tests {
    use super::{IdentityView, core_identity, identity_view};

    #[test]
    fn reports_core_identity() {
        assert_eq!(
            core_identity(),
            IdentityView {
                name: "dageist",
                version: "0.1.0",
            }
        );
    }

    #[test]
    fn the_command_answers_what_the_function_does() {
        assert_eq!(core_identity(), identity_view());
    }

    #[test]
    fn serializes_identity_for_the_view() {
        let json = serde_json::to_value(identity_view()).expect("identity serializes");
        assert_eq!(
            json,
            serde_json::json!({ "name": "dageist", "version": "0.1.0" })
        );
    }
}
