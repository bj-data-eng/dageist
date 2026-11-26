//! Generates the Tauri context and capability schemas for the desktop host, and embeds the
//! sample workspace the application opens on.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The sample workspace in the repository, beside this crate.
fn sample_folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("samples")
        .join("orders")
}

/// Every file under `folder`, by its path in it with `/` between folders, in the order of the
/// paths. A file or folder whose name starts with `.`, such as one a file browser leaves, is no
/// file of the sample and is left out.
fn files_under(folder: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let mut folders = vec![(folder.to_path_buf(), String::new())];
    while let Some((at, path)) = folders.pop() {
        let entries = std::fs::read_dir(&at)
            .unwrap_or_else(|error| panic!("reading {}: {error}", at.display()));
        for entry in entries {
            let entry = entry.unwrap_or_else(|error| panic!("reading {}: {error}", at.display()));
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let inside = if path.is_empty() {
                name
            } else {
                format!("{path}/{name}")
            };
            if entry.path().is_dir() {
                folders.push((entry.path(), inside));
            } else {
                found.push((inside, entry.path()));
            }
        }
    }
    found.sort();
    found
}

/// Writes `sample.rs` into the build's output folder: the sample's files, each by its path and
/// its bytes, included from the repository so that the application carries them.
fn embed_sample() {
    let folder = sample_folder();
    println!("cargo:rerun-if-changed={}", folder.display());
    let mut text = String::from(
        "/// The files of the sample workspace, by their paths in its folder.\n\
         pub(crate) const SAMPLE: &[(&str, &[u8])] = &[\n",
    );
    for (path, file) in files_under(&folder) {
        let file = file
            .canonicalize()
            .unwrap_or_else(|error| panic!("finding {}: {error}", file.display()));
        // Writing to a `String` cannot fail.
        let _ = writeln!(
            text,
            "    ({path:?}, include_bytes!({:?})),",
            file.display().to_string()
        );
    }
    text.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    std::fs::write(out.join("sample.rs"), text).expect("writing the sample's list");
}

fn main() {
    embed_sample();
    tauri_build::build();
}
