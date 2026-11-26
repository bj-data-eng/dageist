//! Tables a graph's nodes give: the timing corpus's tables scanned from their
//! files, and a document run on a scanned table giving the table its last step
//! gives.

use std::path::PathBuf;

use dageist_engine::document::{Document, Inputs};
use dageist_engine::{FileFormat, Frame};

/// The file `name` of the timing corpus.
fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
        .join("timing")
        .join(name)
}

fn names(columns: Vec<dageist_engine::document::ColumnDescription>) -> Vec<String> {
    columns
        .iter()
        .map(|column| format!("{}: {}", column.name(), column.dtype()))
        .collect()
}

#[test]
fn the_corpus_tables_scan_from_their_files_with_their_columns_and_rows() {
    let claims = Frame::scan(&corpus("claims.parquet"), FileFormat::Parquet).unwrap();
    assert_eq!(
        names(claims.columns().unwrap()),
        [
            "dcn: String",
            "timestamp: Datetime(us)",
            "step_to: String",
            "owner: String",
            "row_days: Int64",
        ]
    );
    assert_eq!(claims.working(100).unwrap().height(), 7);
    let holidays = Frame::scan(&corpus("holidays.parquet"), FileFormat::Parquet).unwrap();
    assert_eq!(names(holidays.columns().unwrap()), ["date: Date"]);
}

#[test]
fn a_document_run_on_a_frame_gives_its_last_steps_table_or_nothing_when_a_step_stops() {
    let claims = Frame::scan(&corpus("claims.parquet"), FileFormat::Parquet).unwrap();
    let none = Document::new(Vec::new()).unwrap();
    let report = none.run_frame(&Inputs::new(), &claims).unwrap();
    let output = report.output().unwrap();
    assert_eq!(
        output.columns().unwrap().len(),
        claims.columns().unwrap().len()
    );
    // The corpus's steps join tables these inputs lack, so a step is refused.
    let steps = Document::read(&std::fs::read_to_string(corpus("steps.json")).unwrap()).unwrap();
    let report = steps.run_frame(&Inputs::new(), &claims).unwrap();
    assert!(report.output().is_none());
}
