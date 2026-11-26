//! The sample workspace the application opens on: the files of `samples/orders` in the
//! repository, which the build embeds, unpacked once into the application's data folder and made
//! a workspace there, so that edits to it and its history are kept from one start to the next.

use std::path::{Path, PathBuf};

use crate::sample_table::ErrorView;

include!(concat!(env!("OUT_DIR"), "/sample.rs"));

/// Where the sample workspace is kept in the data folder `data`.
pub(crate) fn sample_root(data: &Path) -> PathBuf {
    data.join("samples").join("orders")
}

/// Makes the sample workspace in `root` from the embedded files, unless it is there already: a
/// folder with a repository in it is the sample as it was left. The files are written into a
/// folder of their own beside it first, which is removed once the workspace is made.
///
/// # Errors
///
/// When `root` holds files but no repository, or the files cannot be written or made a workspace.
pub(crate) fn unpack(root: &Path) -> Result<(), ErrorView> {
    if root.join(".git").exists() {
        return Ok(());
    }
    let parent = root
        .parent()
        .ok_or_else(|| ErrorView::new("the sample's folder has no parent"))?;
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let staging = parent.join(format!(".unpacking-{}-{started}", std::process::id()));
    let unpacked = write_files(&staging).and_then(|()| {
        dageist::workspace::Workspace::create_from_folder(
            &staging,
            root,
            dageist::unsigned_author(),
            dageist::workspace::Options::default(),
        )
        .map(drop)
        .map_err(|error| {
            ErrorView::new(format!(
                "making the sample workspace in {}: {error}",
                root.display()
            ))
        })
    });
    let _ = std::fs::remove_dir_all(&staging);
    unpacked
}

/// Writes the embedded files of the sample under `folder`.
fn write_files(folder: &Path) -> Result<(), ErrorView> {
    for (path, bytes) in SAMPLE {
        let at = path
            .split('/')
            .fold(folder.to_path_buf(), |at, part| at.join(part));
        let written = at
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&at, bytes));
        written.map_err(|error| ErrorView::new(format!("writing the sample's {path}: {error}")))?;
    }
    Ok(())
}
