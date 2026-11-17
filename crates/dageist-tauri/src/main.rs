//! Tauri 2 desktop host for dageist.
//!
//! This adapter owns the window, the webview and the IPC commands that expose
//! the dageist core to the TypeScript view in `ui/`. Application behavior
//! belongs to the core; commands translate its values into serializable views.

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod catalogue;
mod identity;
mod node;
mod report;
mod sample_table;
#[cfg(test)]
mod snapshot;
mod windows;

fn main() {
    tauri::Builder::default()
        .manage(sample_table::SampleGate::default())
        .manage(sample_table::OpenTable::default())
        .on_window_event(windows::close_with_main)
        .invoke_handler(tauri::generate_handler![
            identity::core_identity,
            catalogue::operation_catalogue,
            node::node_inputs,
            node::run_document,
            node::rows_after_step,
            sample_table::open_sample_window,
            sample_table::open_sample_result,
            sample_table::table_block,
            windows::open_view_window,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("dageist failed to run: {error}"));
}
