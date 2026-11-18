//! Tauri 2 desktop host for dageist.
//!
//! This adapter owns the window, the webview and the IPC commands that expose
//! the dageist core to the TypeScript view in `ui/`. Application behavior
//! belongs to the core; commands translate its values into serializable views.
//!
//! The one binary runs a job chosen on its command line: the application, or the
//! `bridge` subcommand that serves the same commands to a browser without a window.
//! Either job runs Polars, so `main` sets Polars up before it starts the job and holds
//! the engine's [`PolarsSession`] until the job ends, which flushes Polars' spill-file
//! cleaner as the Python Polars release does when its process exits.

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod catalogue;
mod cli;
mod identity;
mod node;
mod offers;
mod report;
mod sample_table;
#[cfg(test)]
mod snapshot;
mod windows;

use std::process::ExitCode;

use dageist_engine::PolarsSession;

/// Opens the application window and runs until it is closed, and returns the code the
/// application asked to exit with.
///
/// The application is run so that it returns, and does not call `std::process::exit` as
/// `Builder::run` does: the caller's [`PolarsSession`] is dropped only when this returns.
fn open_application(context: tauri::Context) -> i32 {
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
            offers::offers_at,
            offers::candidate_fits,
            offers::read_typed,
            offers::option_offers,
            sample_table::open_sample_window,
            sample_table::open_sample_result,
            sample_table::table_block,
            windows::open_view_window,
        ])
        .build(context)
        .unwrap_or_else(|error| panic!("dageist failed to run: {error}"))
        .run_return(|_application, _event| {})
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
    // Held until the job ends, whichever way `main` returns or a panic unwinds out of it.
    let _polars = PolarsSession::start();
    match invocation {
        cli::Invocation::Application => match open_application(context) {
            0 => ExitCode::SUCCESS,
            code => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        },
        cli::Invocation::Bridge { port } => match bridge::run(port, &context.config().identifier) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("dageist: the bridge stopped: {error}");
                ExitCode::FAILURE
            }
        },
    }
}
