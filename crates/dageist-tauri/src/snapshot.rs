//! Snapshots of what commands return, kept in the view's host package as the
//! fixtures its tests decode.
//!
//! A host test writes a snapshot when an environment variable is set and
//! otherwise checks that the committed file holds exactly what the command
//! returns now, so a change of shape in the host shows up in the view's tests.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The folder of the view's fixtures.
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../ui/packages/host/fixtures"
);

/// The path of the fixture `file`.
fn path(file: &str) -> PathBuf {
    Path::new(FIXTURES).join(file)
}

/// Writes the fixture `file` from `returned` when the variable `refresh` is
/// set, and otherwise checks that the committed file holds `returned`.
///
/// `how` is the command that refreshes it, which a failure shows.
pub fn check(file: &str, returned: &Value, refresh: &str, how: &str) {
    let path = path(file);
    if std::env::var_os(refresh).is_some() {
        let mut text = serde_json::to_string_pretty(returned).expect("it serializes");
        text.push('\n');
        std::fs::create_dir_all(FIXTURES).expect("the fixtures folder exists");
        std::fs::write(&path, text).expect("the snapshot is written");
        return;
    }
    // The values are compared, not the text: the view's formatter lays short lists out
    // on one line, so the committed file is formatted after it is written.
    let committed: Option<Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    assert!(
        committed.as_ref() == Some(returned),
        "ui/packages/host/fixtures/{file} is not what its command returns. Refresh it with: \
         {refresh}=1 {how} && npm --prefix ui run format"
    );
}
