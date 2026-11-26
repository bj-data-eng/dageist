//! The sample workspace the application opens on: the files of `samples/orders` in the
//! repository, which the build embeds, unpacked once into the application's data folder and made
//! a workspace there, so that edits to it and its history are kept from one start to the next.
//!
//! The workspace is made whole in a folder of its own beside its place and then moved into place
//! by one rename, so its place holds either nothing or the whole sample: two starts that unpack
//! at once both make one, and the one that moves second keeps the first's. A start that stops
//! part way leaves its folder beside the place, which a later start removes once it is old.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::sample_table::ErrorView;

include!(concat!(env!("OUT_DIR"), "/sample.rs"));

/// What the folders a start unpacks into are called, before what keeps them apart.
const UNPACKING: &str = ".unpacking-";

/// How old a folder a start unpacked into must be before a later start takes it to be left by a
/// start that stopped, and removes it.
const LEFT_AFTER: Duration = Duration::from_secs(60 * 60);

/// Where the sample workspace is kept in the data folder `data`.
pub(crate) fn sample_root(data: &Path) -> PathBuf {
    data.join("samples").join("orders")
}

/// Makes the sample workspace in `root` from the embedded files, unless something is there
/// already, which is then the sample as it was left. An empty folder in its place is no sample.
///
/// # Errors
///
/// When the files cannot be written, made a workspace or moved into place.
pub(crate) fn unpack(root: &Path) -> Result<(), ErrorView> {
    let empty = std::fs::read_dir(root).map(|mut entries| entries.next().is_none());
    if matches!(empty, Ok(false)) {
        return Ok(());
    }
    let parent = root
        .parent()
        .ok_or_else(|| ErrorView::new("the sample's folder has no parent"))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| ErrorView::new(format!("making {}: {error}", parent.display())))?;
    remove_left(parent);
    // The process, the time and a count kept apart two starts in two processes and in one.
    static STARTS: AtomicU64 = AtomicU64::new(0);
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let unique = format!(
        "{}-{started}-{}",
        std::process::id(),
        STARTS.fetch_add(1, Ordering::Relaxed)
    );
    let files = parent.join(format!("{UNPACKING}files-{unique}"));
    let made = parent.join(format!("{UNPACKING}workspace-{unique}"));
    let unpacked = write_files(&files).and_then(|()| {
        // The workspace is let go as soon as it is made, so that it can be moved and opened.
        dageist::workspace::Workspace::create_from_folder(
            &files,
            &made,
            dageist::unsigned_author(),
            dageist::workspace::Options::default(),
        )
        .map(drop)
        .map_err(|error| ErrorView::new(format!("making the sample workspace: {error}")))
    });
    let _ = std::fs::remove_dir_all(&files);
    // An empty folder in the place is removed first, since not every system renames onto one; a
    // folder another start has filled stays, and the rename then keeps it.
    let _ = std::fs::remove_dir(root);
    let moved = unpacked.and_then(|()| match std::fs::rename(&made, root) {
        Ok(()) => Ok(()),
        // Another start moved its sample into place first; that one is kept.
        Err(_) if std::fs::read_dir(root).is_ok_and(|mut entries| entries.next().is_some()) => {
            Ok(())
        }
        Err(error) => Err(ErrorView::new(format!(
            "moving the sample workspace to {}: {error}",
            root.display()
        ))),
    });
    let _ = std::fs::remove_dir_all(&made);
    moved
}

/// Removes the folders under `parent` that starts unpacked into and left, once they are old
/// enough that no start can still be using them.
fn remove_left(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let left = entry.file_name().to_string_lossy().starts_with(UNPACKING)
            && entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > LEFT_AFTER);
        if left {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
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
