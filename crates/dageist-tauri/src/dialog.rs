//! The system's file dialog, which picks the file a source reads or an output writes, or the
//! folder of a workspace to open.
//!
//! In the application, Tauri's dialog plugin opens it as a sheet of the window that asked. The
//! bridge has no Tauri, so it opens the same system dialog with rfd, the library the plugin is
//! built on. A dialog runs on the process's main thread, as the platform asks, and the bridge has
//! no event loop there to hand it to, so the bridge's main thread takes the requests its
//! connections send it and runs each dialog itself: see [`Dialogs`].

use std::path::{Path, PathBuf};
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

/// The kinds of file a dialog offers, by name and extension.
const FILTERS: [(&str, &[&str]); 2] = [("Parquet", &["parquet"]), ("CSV", &["csv"])];

/// What a dialog picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// A data file, to read or to write.
    File(Purpose),
    /// A folder.
    Folder,
}

/// Opens the system's dialog with rfd and gives the path chosen, or nothing when the dialog was
/// cancelled. A file is picked among the kinds the dialog offers.
fn pick(pick: Pick) -> Option<PathBuf> {
    let files = || {
        FILTERS
            .iter()
            .fold(rfd::FileDialog::new(), |dialog, (name, extensions)| {
                dialog.add_filter(*name, extensions)
            })
    };
    match pick {
        Pick::File(Purpose::Open) => files().pick_file(),
        Pick::File(Purpose::Save) => files().save_file(),
        Pick::Folder => rfd::FileDialog::new().pick_folder(),
    }
}

/// Whether `path` names a CSV file.
pub(crate) fn is_csv(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
}

/// A request for a dialog, sent to the thread that runs them, with where to send the answer.
pub struct Request {
    pick: Pick,
    answer: mpsc::Sender<Option<PathBuf>>,
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

    /// Asks the main thread for a dialog and waits for the path chosen.
    fn pick(&self, pick: Pick) -> Option<PathBuf> {
        let requests = self.requests.as_ref()?;
        let (answer, answered) = mpsc::channel();
        requests.send(Request { pick, answer }).ok()?;
        answered.recv().ok().flatten()
    }

    /// Asks the main thread for a file dialog and gives the file chosen, with its format worked
    /// out from its extension: `.csv` is CSV and anything else Parquet.
    pub fn choose(&self, purpose: Purpose) -> Option<FileView> {
        self.pick(Pick::File(purpose))
            .map(|path| FileView::chosen(&path))
    }

    /// Asks the main thread for a folder dialog and gives the folder chosen.
    pub fn choose_folder(&self) -> Option<String> {
        self.pick(Pick::Folder)
            .map(|path| path.to_string_lossy().into_owned())
    }
}

/// Runs every dialog asked for on `requests` on this thread, until every sender is gone.
pub fn serve_dialogs(requests: &mpsc::Receiver<Request>) {
    for request in requests {
        // A request whose sender stopped waiting is answered to no one.
        let _ = request.answer.send(pick(request.pick));
    }
}

/// Opens the system's file dialog over `window` and reports the file chosen, or nothing.
#[tauri::command(async)]
pub fn choose_file(window: tauri::WebviewWindow, purpose: Purpose) -> Option<FileView> {
    use tauri_plugin_dialog::DialogExt;
    let dialog = FILTERS.iter().fold(
        window.dialog().file().set_parent(&window),
        |dialog, (name, extensions)| dialog.add_filter(*name, extensions),
    );
    let chosen = match purpose {
        Purpose::Open => dialog.blocking_pick_file(),
        Purpose::Save => dialog.blocking_save_file(),
    }?;
    chosen.into_path().ok().map(|path| FileView::chosen(&path))
}

/// Opens the system's folder dialog over `window` and reports the folder chosen, or nothing.
#[tauri::command(async)]
pub fn choose_folder(window: tauri::WebviewWindow) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    window
        .dialog()
        .file()
        .set_parent(&window)
        .blocking_pick_folder()?
        .into_path()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::mpsc;

    use super::{Dialogs, Pick, Purpose, Request};
    use crate::graph::FileView;

    #[test]
    fn a_request_reaches_the_thread_that_runs_dialogs_and_its_answer_comes_back() {
        let (requests, asked) = mpsc::channel::<Request>();
        let runner = std::thread::spawn(move || {
            // Stands in for the main thread: it answers by what is picked, as a dialog would.
            for request in asked {
                let chosen = match request.pick {
                    Pick::File(Purpose::Save) => Some("/data/out.csv".into()),
                    Pick::File(Purpose::Open) => None,
                    Pick::Folder => Some("/work/claims".into()),
                };
                request.answer.send(chosen).expect("the asker waits");
            }
        });
        let dialogs = Dialogs::to(requests);
        assert_eq!(
            dialogs.choose(Purpose::Save),
            Some(FileView::chosen(Path::new("/data/out.csv")))
        );
        assert_eq!(dialogs.choose(Purpose::Open), None);
        assert_eq!(dialogs.choose_folder().as_deref(), Some("/work/claims"));
        drop(dialogs);
        runner.join().expect("the runner ends once no one can ask");
    }

    #[test]
    fn dialogs_no_thread_runs_choose_nothing() {
        assert_eq!(Dialogs::none().choose(Purpose::Open), None);
        assert_eq!(Dialogs::none().choose_folder(), None);
    }
}
