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

mod block;
mod bridge;
mod catalogue;
mod cli;
#[cfg(test)]
mod corpus;
mod dialog;
mod document;
mod figures;
mod graph;
mod identity;
mod node;
mod offers;
mod report;
mod sample_table;
#[cfg(test)]
mod snapshot;
#[cfg(test)]
mod test_support;
mod windows;

use std::process::ExitCode;
use std::time::SystemTime;

use dageist_engine::PolarsSession;

/// Opens the application window and runs until it is closed, and returns the code the
/// application asked to exit with.
///
/// The application is run so that it returns, and does not call `std::process::exit` as
/// `Builder::run` does: the caller's [`PolarsSession`] is dropped only when this returns.
fn open_application(context: tauri::Context, workspace: graph::Workspace) -> i32 {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(sample_table::SampleGate::default())
        .manage(sample_table::OpenTable::default())
        .manage(workspace)
        .on_window_event(windows::close_with_main)
        .invoke_handler(tauri::generate_handler![
            identity::core_identity,
            catalogue::operation_catalogue,
            graph::graph,
            graph::edit_graph,
            graph::place_node,
            graph::open_workspace,
            dialog::choose_file,
            dialog::choose_folder,
            node::node_inputs,
            node::node_document,
            node::edit_document,
            node::rows_after_step,
            offers::offers_at,
            offers::candidate_fits,
            offers::read_typed,
            offers::option_offers,
            offers::step_starts,
            sample_table::open_sample_window,
            sample_table::open_sample_result,
            sample_table::table_block,
            windows::open_view_window,
        ])
        .build(context)
        .unwrap_or_else(|error| panic!("dageist failed to run: {error}"))
        .run_return(|application, event| {
            // The workspace on disk is packed whole as the application exits.
            if let tauri::RunEvent::Exit = event
                && let Err(error) = tauri::Manager::state::<graph::Workspace>(application).close()
            {
                eprintln!("dageist: the workspace was not packed: {error:?}");
            }
        })
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
    // The starting graph reads the sample's tables from files written for this run, in a folder
    // made for it that no other run has, which is removed when the run ends.
    let sample = match sample_folder() {
        Ok(folder) => folder,
        Err(error) => {
            eprintln!("dageist: a folder for the sample's tables cannot be made: {error}");
            return ExitCode::FAILURE;
        }
    };
    let workspace =
        match graph::Workspace::starting(document::first_revision(SystemTime::now()), &sample) {
            Ok(workspace) => workspace,
            Err(error) => {
                eprintln!("dageist: the starting graph cannot be made: {error:?}");
                remove_sample_folder(&sample);
                return ExitCode::FAILURE;
            }
        };
    let code = match invocation {
        cli::Invocation::Application => match open_application(context, workspace) {
            0 => ExitCode::SUCCESS,
            code => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        },
        cli::Invocation::Bridge { port } => {
            match bridge::run(port, &context.config().identifier, workspace) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("dageist: the bridge stopped: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    };
    remove_sample_folder(&sample);
    code
}

/// Makes a new folder for the sample's tables in the system's temporary folder. It is made, not
/// found: a folder of that name that is already there is refused, so no file is written through
/// a folder someone else put there.
fn sample_folder() -> std::io::Result<std::path::PathBuf> {
    let started = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let folder =
        std::env::temp_dir().join(format!("dageist-sample-{}-{started}", std::process::id()));
    std::fs::create_dir(&folder)?;
    Ok(folder)
}

/// Removes the sample's folder. A folder that cannot be removed is left for the system, which
/// clears its temporary folder in time; so is the folder of a bridge stopped by a signal.
fn remove_sample_folder(folder: &std::path::Path) {
    let _ = std::fs::remove_dir_all(folder);
}
