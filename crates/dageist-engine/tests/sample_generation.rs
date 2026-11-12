//! Generates sample tables and checks what is left on disk afterwards.

use std::fs;
use std::path::{Path, PathBuf};

use dageist_engine::{ParquetTable, SampleSpec, claims_generated, generate_claims};

struct SampleDirectory {
    path: PathBuf,
}

impl SampleDirectory {
    fn named(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("dageist-generation-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        Self { path }
    }
}

impl Drop for SampleDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn parquet_files(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("the directory is listed")
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn generating_a_smaller_table_leaves_no_files_of_the_larger_one() {
    let sample = SampleDirectory::named("replace");
    let two_files = SampleSpec {
        keys: 25_040,
        filler_columns: 0,
    };
    let one_file = SampleSpec {
        keys: 60,
        filler_columns: 0,
    };
    generate_claims(&sample.path, two_files).expect("the larger table is generated");
    assert_eq!(parquet_files(&sample.path).len(), 2);

    let written = generate_claims(&sample.path, one_file).expect("the smaller table is generated");

    assert_eq!(written.len(), 1);
    assert_eq!(parquet_files(&sample.path), ["claims-0000.parquet"]);
    let table = ParquetTable::open(&sample.path).expect("the table opens");
    // Sixty keys of two to eight step rows each.
    assert!(
        (120..=480).contains(&table.height()),
        "{} rows for sixty keys",
        table.height()
    );
}

#[test]
fn generated_table_is_reported_only_where_it_was_written() {
    let sample = SampleDirectory::named("reported");
    let spec = SampleSpec {
        keys: 20,
        filler_columns: 0,
    };

    assert!(!claims_generated(&sample.path));
    let written = generate_claims(&sample.path, spec).expect("the table is generated");

    assert!(claims_generated(&sample.path));
    assert!(written.iter().all(|file| file.starts_with(&sample.path)));
    assert!(written.iter().all(|file| file.is_file()));
}
