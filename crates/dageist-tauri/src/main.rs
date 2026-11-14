//! Tauri 2 desktop host for dageist.
//!
//! This adapter owns the window, the webview and the IPC commands that expose
//! the dageist core to the TypeScript view in `ui/`. Application behavior
//! belongs to the core; commands translate its values into serializable views.

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod sample_table;
mod windows;

use serde::Serialize;

use sample_table::{OpenTable, SampleGate};

/// Identity of the running dageist core, as presented to the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct IdentityView {
    name: &'static str,
    version: &'static str,
}

/// Reports the identity of the dageist core to the view.
#[tauri::command]
fn core_identity() -> IdentityView {
    IdentityView {
        name: dageist::crate_name(),
        version: dageist::crate_version(),
    }
}

fn main() {
    tauri::Builder::default()
        .manage(SampleGate::default())
        .manage(OpenTable::default())
        .invoke_handler(tauri::generate_handler![
            core_identity,
            sample_table::open_sample_window,
            sample_table::open_sample_result,
            sample_table::table_block,
            windows::open_view_window,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("dageist failed to run: {error}"));
}

#[cfg(test)]
mod tests {
    use super::{IdentityView, core_identity};

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
    fn serializes_identity_for_the_view() {
        let json = serde_json::to_value(core_identity()).expect("identity serializes");
        assert_eq!(
            json,
            serde_json::json!({ "name": "dageist", "version": "0.1.0" })
        );
    }
}
