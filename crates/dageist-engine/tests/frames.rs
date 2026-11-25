//! Tables a graph's nodes give: the sample's tables written as files and
//! scanned back, and a document run on a scanned table giving the table its
//! last step gives.

use std::fs;

use dageist_engine::claim_timing::ClaimTiming;
use dageist_engine::document::{Document, Inputs};
use dageist_engine::{FileFormat, Frame};

#[test]
fn the_sample_tables_written_as_files_scan_back_with_their_columns() {
    let directory = std::env::temp_dir().join(format!("dageist-frames-{}", std::process::id()));
    let files = ClaimTiming::write_tables(&directory).unwrap();
    let claims = Frame::scan(&files.claims, FileFormat::Parquet).unwrap();
    let node = ClaimTiming::new().unwrap();
    let names = |columns: Vec<dageist_engine::document::ColumnDescription>| {
        columns
            .iter()
            .map(|column| format!("{}: {}", column.name(), column.dtype()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(claims.columns().unwrap()),
        names(node.source_columns())
    );
    assert_eq!(
        claims.working(100).unwrap().height(),
        node.source().height()
    );
    let holidays = Frame::scan(&files.holidays, FileFormat::Parquet).unwrap();
    assert_eq!(names(holidays.columns().unwrap()), ["date: Date"]);
    fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn a_document_run_on_a_frame_gives_its_last_steps_table_or_nothing_when_a_step_stops() {
    let directory = std::env::temp_dir().join(format!("dageist-frames-run-{}", std::process::id()));
    let files = ClaimTiming::write_tables(&directory).unwrap();
    let claims = Frame::scan(&files.claims, FileFormat::Parquet).unwrap();
    let none = Document::new(Vec::new()).unwrap();
    let report = none.run_frame(&Inputs::new(), &claims).unwrap();
    let output = report.output().unwrap();
    assert_eq!(
        output.columns().unwrap().len(),
        claims.columns().unwrap().len()
    );
    // The sample's steps join tables these inputs lack, so a step is refused.
    let steps = Document::read(ClaimTiming::STEPS_JSON).unwrap();
    let report = steps.run_frame(&Inputs::new(), &claims).unwrap();
    assert!(report.output().is_none());
    fs::remove_dir_all(&directory).unwrap();
}
