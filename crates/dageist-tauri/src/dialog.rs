//! The system's file dialog, which picks the file a source reads or an output writes.
//!
//! The dialog runs on the process's main thread, as the platform asks. In the application
//! window the command runs on a worker thread, and the dialog reaches the main thread through
//! the event loop Tauri runs there. The bridge has no such loop, so its main thread takes the
//! requests the bridge's threads send it and runs each dialog itself: see [`Dialogs`].

use std::path::Path;
use std::sync::mpsc;

use serde::Deserialize;

use crate::graph::FileView;

/// What a file is chosen for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Purpose {
    /// A file to read, which exists.
    Open,
    /// A file to write, which may not exist yet.
    Save,
}

/// Opens the system's dialog and gives the file chosen, with its format worked out from its
/// extension: `.csv` is CSV and anything else Parquet. Nothing when the dialog was cancelled.
pub fn choose(purpose: Purpose) -> Option<FileView> {
    let dialog = rfd::FileDialog::new()
        .add_filter("Parquet", &["parquet"])
        .add_filter("CSV", &["csv"]);
    let path = match purpose {
        Purpose::Open => dialog.pick_file(),
        Purpose::Save => dialog.save_file(),
    }?;
    Some(FileView::chosen(&path))
}

/// Whether `path` names a CSV file.
pub(crate) fn is_csv(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
}

/// A request for a dialog, sent to the thread that runs them, with where to send the answer.
pub struct Request {
    purpose: Purpose,
    answer: mpsc::Sender<Option<FileView>>,
}

/// Where the bridge's threads send their requests for a dialog: to the main thread, which runs
/// each with [`serve_dialogs`]. With no main thread taking them, as in a test, every request is
/// answered with nothing chosen.
#[derive(Debug, Clone)]
pub struct Dialogs {
    requests: Option<mpsc::Sender<Request>>,
}

impl Dialogs {
    /// Dialogs that send their requests to `requests`.
    pub const fn to(requests: mpsc::Sender<Request>) -> Self {
        Self {
            requests: Some(requests),
        }
    }

    /// Dialogs no thread runs, which choose nothing.
    #[cfg(test)]
    pub const fn none() -> Self {
        Self { requests: None }
    }

    /// Asks the main thread for a dialog and waits for what was chosen.
    pub fn choose(&self, purpose: Purpose) -> Option<FileView> {
        let requests = self.requests.as_ref()?;
        let (answer, answered) = mpsc::channel();
        requests.send(Request { purpose, answer }).ok()?;
        answered.recv().ok().flatten()
    }
}

/// Runs every dialog asked for on `requests` on this thread, until every sender is gone.
pub fn serve_dialogs(requests: &mpsc::Receiver<Request>) {
    for request in requests {
        // A request whose sender stopped waiting is answered to no one.
        let _ = request.answer.send(choose(request.purpose));
    }
}

/// Opens the system's file dialog and reports the file chosen, or nothing.
#[tauri::command(async)]
pub fn choose_file(purpose: Purpose) -> Option<FileView> {
    choose(purpose)
}
