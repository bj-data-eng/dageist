//! Running the source in Python Polars and comparing what it gives with the
//! engine's own run of the document, on the same rows.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use dageist_engine::document::{Document, StepId};
use polars::prelude::*;
use serde_json::json;

use super::fixtures::Fixture;

/// The Python that runs the check, from `DAGEIST_PYTHON`, resolved from the
/// root of the workspace when it is a relative path.
pub fn python() -> PathBuf {
    let named = std::env::var_os("DAGEIST_PYTHON").unwrap_or_else(|| {
        panic!(
            "DAGEIST_PYTHON names no Python. Create one with Polars as docs/how-to.md says, \
             then run this check again with DAGEIST_PYTHON=tmp/python/bin/python"
        )
    });
    let named = PathBuf::from(named);
    if named.is_absolute() {
        named
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(named)
    }
}

fn write_arrow(path: &Path, frame: &DataFrame) {
    let file = fs::File::create(path).expect("an Arrow file is created");
    IpcWriter::new(file)
        .finish(&mut frame.clone())
        .expect("the frame is written");
}

fn one_column(series: &Series) -> DataFrame {
    DataFrame::new(series.len(), vec![series.clone().into()]).expect("a frame")
}

/// The folder a case's files are written to, empty.
fn folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("python_source")
        .join(name);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("a folder for the case");
    folder
}

/// Runs the source of `document` in `python` on the rows of `fixture`, and
/// compares what it gives with the engine's run of the same document.
///
/// # Errors
///
/// The reason the two differ, or Python's complaint.
pub fn same_in_python(
    python: &Path,
    name: &str,
    document: &Document,
    fixture: &Fixture,
) -> Result<(), String> {
    let last = document
        .steps()
        .last()
        .map(|step| StepId::new(step.id().as_str()).expect("an identifier"))
        .ok_or("the document has no steps")?;
    let expected = document
        .rows(&fixture.inputs(), &fixture.frame, &last, 10_000)
        .map_err(|error| format!("the engine does not run the document: {error}"))?;

    let folder = folder(name);
    let source = document.source(fixture.frame_name, &fixture.inputs());
    assert!(
        source.not_runnable().is_empty(),
        "{name}: the source says it does not run: {:?}",
        source.not_runnable()
    );
    fs::write(folder.join("source.py"), source.text()).expect("the source is written");
    write_arrow(&folder.join("frame.arrow"), &fixture.frame);
    write_arrow(&folder.join("expected.arrow"), &expected);
    let mut inputs = Vec::new();
    for table in &fixture.tables {
        let file = format!("{}.arrow", table.name);
        write_arrow(&folder.join(&file), &table.frame);
        inputs.push(json!({
            "name": table.name,
            "kind": "table",
            "file": file,
        }));
    }
    for input in &fixture.series {
        let file = format!("{}.arrow", input.name);
        let mut series = input.series.clone();
        series.rename("value".into());
        write_arrow(&folder.join(&file), &one_column(&series));
        inputs.push(json!({
            "name": input.name,
            "kind": if input.list { "list" } else { "value" },
            "file": file,
        }));
    }
    fs::write(
        folder.join("case.json"),
        json!({ "frame": fixture.frame_name, "inputs": inputs }).to_string(),
    )
    .expect("the case is described");

    let output = Command::new(python)
        .arg("-I")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/document/python/execute.py"))
        .arg(&folder)
        .output()
        .map_err(|error| format!("{} did not run: {error}", python.display()))?;
    if output.status.success() {
        let _ = fs::remove_dir_all(&folder);
        Ok(())
    } else {
        Err(format!(
            "{}\n{}\nthe files are in {}",
            source.text(),
            String::from_utf8_lossy(&output.stderr).trim_end(),
            folder.display()
        ))
    }
}

/// Every case of `cases`, as the ignored test runs them: the failures of all of them.
pub fn all_in_python(python: &Path, cases: &[super::cases::Case]) -> Vec<String> {
    cases
        .iter()
        .filter_map(|case| {
            same_in_python(python, &case.name, &case.document(), case.fixture)
                .err()
                .map(|why| format!("{}: {why}", case.name))
        })
        .collect()
}
