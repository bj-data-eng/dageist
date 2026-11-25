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

/// The committed fixture `file`, as JSON, for a test that holds what a command returns against
/// what another command's snapshot recorded.
pub fn committed(file: &str) -> Value {
    let text = std::fs::read_to_string(path(file))
        .unwrap_or_else(|error| panic!("ui/packages/host/fixtures/{file} is not read: {error}"));
    serde_json::from_str(&text).expect("a fixture is JSON")
}

/// Writes the fixture `file` from `returned` when the variable `refresh` is set, and otherwise
/// checks that the committed file holds `returned`, as [`check`] does, for a fixture that is a
/// list of documents under the key `documents`, beside other fields that are written whole on the
/// line before. Each document is written on a line of its own,
/// without the indentation that the other fixtures have, which would make the file several times
/// as long. The formatter leaves such files alone: they are in its ignore list.
pub fn check_lines(file: &str, returned: &Value, refresh: &str, how: &str) {
    let documents = returned["documents"]
        .as_array()
        .expect("a fixture of documents holds a list of them");
    if std::env::var_os(refresh).is_some() {
        let lines: Vec<String> = documents
            .iter()
            .map(|document| serde_json::to_string(document).expect("it serializes"))
            .collect();
        let inputs = serde_json::to_string(&returned["inputs"]).expect("it serializes");
        let text = format!(
            "{{\"inputs\":{inputs},\"documents\":[\n{}\n]}}\n",
            lines.join(",\n")
        );
        std::fs::create_dir_all(FIXTURES).expect("the fixtures folder exists");
        std::fs::write(path(file), text).expect("the snapshot is written");
        return;
    }
    let committed: Option<Value> = std::fs::read_to_string(path(file))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    assert!(
        committed.as_ref() == Some(returned),
        "ui/packages/host/fixtures/{file} is not what the corpus holds. Refresh it with: \
         {refresh}=1 {how}"
    );
}
