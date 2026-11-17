//! Tauri 2 desktop host for dageist.
//!
//! This adapter owns the window, the webview and the IPC commands that expose
//! the dageist core to the TypeScript view in `ui/`. Application behavior
//! belongs to the core; commands translate its values into serializable views.
//!
//! The one binary runs a job chosen on its command line: the application, or the
//! `bridge` subcommand that serves the same commands to a browser without a window.

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod catalogue;
mod cli;
mod identity;
mod node;
mod report;
mod sample_table;
#[cfg(test)]
mod snapshot;
mod windows;

use std::process::ExitCode;

/// Opens the application window and runs until it is closed.
fn open_application(context: tauri::Context) {
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
        .run(context)
        .unwrap_or_else(|error| panic!("dageist failed to run: {error}"));
}

fn main() -> ExitCode {
    let invocation = match cli::parse(std::env::args_os().skip(1)) {
        Ok(invocation) => invocation,
        Err(error) => {
            eprintln!("dageist: {error}\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    // The context is built here and used by either job: the application runs on it, and the
    // bridge reads the application's identifier from it to find the application's cache.
    let context = tauri::generate_context!();
    match invocation {
        cli::Invocation::Application => {
            open_application(context);
            ExitCode::SUCCESS
        }
        cli::Invocation::Bridge { port } => match bridge::run(port, &context.config().identifier) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("dageist: the bridge stopped: {error}");
                ExitCode::FAILURE
            }
        },
    }
}
