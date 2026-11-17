//! What the engine's start-up does to Polars, one part at a time.
//!
//! Each part is seen through an effect Polars shows: a plan written in SQL
//! resolves, an extension type nothing registered is loaded as the setting
//! says, and files spilled to disk are deleted. The set-up lives in the process,
//! and the environment variables it reads are read when it first runs, so each
//! test runs in a process of its own with the variables it needs.
//!
//! The order the release sets the three parts up in cannot be seen from outside
//! Polars: none of them reads what another sets, so no test can tell one order
//! from another. What the start does to the spill-file cleaner can be seen: the
//! cleaner deletes what a process that ended left behind as it starts, which
//! Polars would otherwise do only at the first spill or the first flush.

mod support;

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use dageist_engine::PolarsSession;
use polars::datatypes::extension::get_extension_type_or_generic;
use polars::prelude::*;
use polars::sql::SQLContext;
use polars_ooc::{LeastRecentSpillContext, SpillFrame};
use support::in_fresh_process;

const EXTENSION_SETTING: &str = "POLARS_UNKNOWN_EXTENSION_TYPE_BEHAVIOR";
const SPILL_DIRECTORY: &str = "POLARS_OOC_SPILL_DIR";
const PLAN: &str = "DAGEIST_TEST_SQL_PLAN";

/// A directory of the test's own, removed when the test ends.
struct Directory(PathBuf);

impl Directory {
    fn named(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("dageist-start-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the directory is made");
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A plan written in SQL, as JSON, planned here so the process that reads it
/// never constructs the SQL context that would register the resolver itself.
fn sql_plan_json() -> String {
    let mut context = SQLContext::new();
    context.register(
        "numbers",
        df!("a" => [1_i64, 2, 3]).expect("a table").lazy(),
    );
    let plan = context
        .execute("SELECT a FROM numbers WHERE a > 1")
        .expect("the query is planned")
        .logical_plan;
    let mut json = Vec::new();
    plan.serialize_json_into(&mut json)
        .expect("the plan is written");
    String::from_utf8(json).expect("the plan is text")
}

fn collect_sql_plan() -> PolarsResult<DataFrame> {
    let json = std::env::var(PLAN).expect("the process is given a plan");
    let plan = DslPlan::deserialize_json_from_str(&json).expect("the plan is read");
    LazyFrame::from(plan).collect()
}

#[test]
fn a_sql_plan_resolves_after_the_start_and_not_before() {
    in_fresh_process(
        "a_sql_plan_resolves_after_the_start_and_not_before",
        || vec![(PLAN, sql_plan_json())],
        || {
            let before = collect_sql_plan().expect_err("no resolver is registered yet");
            assert!(
                before.to_string().contains("no SQL resolver registered"),
                "refused for another reason: {before}"
            );

            let _session = PolarsSession::start();

            let rows = collect_sql_plan().expect("the query resolves once Polars is set up");
            assert_eq!(rows.height(), 2);
        },
    );
}

#[test]
fn starting_more_than_once_and_from_many_threads_sets_up_once_and_harms_nothing() {
    in_fresh_process(
        "starting_more_than_once_and_from_many_threads_sets_up_once_and_harms_nothing",
        || vec![(PLAN, sql_plan_json())],
        || {
            let threads: Vec<_> = (0..8)
                .map(|_| std::thread::spawn(|| drop(PolarsSession::start())))
                .collect();
            for thread in threads {
                thread.join().expect("a start does not panic");
            }
            let _first = PolarsSession::start();
            let _second = PolarsSession::start();

            let rows = collect_sql_plan().expect("the query resolves");
            assert_eq!(rows.height(), 2);
        },
    );
}

/// The dtype Polars reads for a column whose Arrow field says it is of an
/// extension type that nothing registered.
fn dtype_read_for_an_unknown_extension_type() -> DataType {
    let name = "dageist.unregistered";
    let storage = DataType::Int32;
    // The name is unregistered, so Polars makes the type of it generically.
    let extension = DataType::Extension(
        get_extension_type_or_generic(name, &storage, None),
        Box::new(storage.clone()),
    );
    let column = Series::new("value".into(), [1_i32, 2])
        .cast(&extension)
        .expect("the values are of the extension type");
    let mut frame = DataFrame::new(2, vec![column.into()]).expect("a table");
    let mut bytes = Vec::new();
    IpcStreamWriter::new(&mut bytes)
        .finish(&mut frame)
        .expect("the table is written");
    let read = IpcStreamReader::new(Cursor::new(bytes))
        .finish()
        .expect("the bytes are read");
    read.column("value").expect("the column").dtype().clone()
}

fn reads_unknown_extension_type_as_storage(setting: Option<&str>) {
    let test = match setting {
        Some("load_as_storage") => "an_unknown_extension_type_loads_as_storage_when_asked",
        Some("load_as_extension") => "an_unknown_extension_type_loads_as_an_extension_when_asked",
        Some(_) => "an_unknown_extension_type_loads_as_an_extension_for_a_setting_not_known",
        None => "an_unknown_extension_type_loads_as_an_extension_by_default",
    };
    let expect_storage = setting == Some("load_as_storage");
    in_fresh_process(
        test,
        || {
            setting
                .map(|setting| vec![(EXTENSION_SETTING, setting.to_owned())])
                .unwrap_or_default()
        },
        || {
            let _session = PolarsSession::start();
            let dtype = dtype_read_for_an_unknown_extension_type();
            if expect_storage {
                assert_eq!(dtype, DataType::Int32);
            } else {
                assert!(
                    matches!(dtype, DataType::Extension(..)),
                    "read as {dtype:?}"
                );
            }
        },
    );
}

#[test]
fn an_unknown_extension_type_loads_as_storage_when_asked() {
    reads_unknown_extension_type_as_storage(Some("load_as_storage"));
}

#[test]
fn an_unknown_extension_type_loads_as_an_extension_when_asked() {
    reads_unknown_extension_type_as_storage(Some("load_as_extension"));
}

#[test]
fn an_unknown_extension_type_loads_as_an_extension_for_a_setting_not_known() {
    reads_unknown_extension_type_as_storage(Some("anything else"));
}

#[test]
fn an_unknown_extension_type_loads_as_an_extension_by_default() {
    reads_unknown_extension_type_as_storage(None);
}

/// The files of `directory` and of the directories in it, by path.
fn files_under(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[test]
fn files_spilled_to_disk_are_gone_when_the_session_ends() {
    let spill = Directory::named("spill");
    in_fresh_process(
        "files_spilled_to_disk_are_gone_when_the_session_ends",
        || {
            vec![
                (SPILL_DIRECTORY, spill.0.display().to_string()),
                // A budget far below the frames, so that the older ones spill at once.
                ("POLARS_OOC_MEMORY_BUDGET_MB", "1".to_owned()),
                ("POLARS_OOC_SPILL_MIN_BYTES", "0".to_owned()),
            ]
        },
        || {
            let directory = PathBuf::from(
                std::env::var(SPILL_DIRECTORY).expect("the process is given a spill directory"),
            );
            let session = PolarsSession::start();

            let context = LeastRecentSpillContext::new("startup-test".into(), None);
            let frames: Vec<_> = (0..4)
                .map(|number| {
                    let values: Vec<i64> = (0..2_000_000).map(|value| value + number).collect();
                    let frame = df!("value" => values).expect("a table");
                    SpillFrame::new_blocking(frame, &context)
                })
                .collect();
            let deadline = Instant::now() + Duration::from_secs(30);
            while files_under(&directory).is_empty() {
                assert!(
                    Instant::now() < deadline,
                    "nothing was spilled to {}",
                    directory.display()
                );
                std::thread::sleep(Duration::from_millis(20));
            }

            drop(frames);
            drop(session);

            assert_eq!(files_under(&directory), Vec::<PathBuf>::new());
        },
    );
}

#[test]
fn starting_deletes_the_spill_directory_of_a_process_that_no_longer_exists() {
    let spill = Directory::named("stale");
    in_fresh_process(
        "starting_deletes_the_spill_directory_of_a_process_that_no_longer_exists",
        || vec![(SPILL_DIRECTORY, spill.0.display().to_string())],
        || {
            let directory = PathBuf::from(
                std::env::var(SPILL_DIRECTORY).expect("the process is given a spill directory"),
            );
            // A process that has ended, and the files it left.
            let mut ended = Command::new(std::env::current_exe().expect("the program's path"))
                .arg("--list")
                .spawn()
                .expect("a process starts");
            ended.wait().expect("the process ends");
            let left = directory.join(ended.id().to_string());
            fs::create_dir_all(&left).expect("the directory is made");
            fs::write(left.join("spill-old.ipc"), b"left behind").expect("the file is written");
            // A directory that is not a process's is left alone.
            let other = directory.join("not-a-process");
            fs::create_dir_all(&other).expect("the directory is made");

            // The session is held: the cleaner has not been asked to flush, so it is
            // the start that put it to work.
            let _session = PolarsSession::start();
            let deadline = Instant::now() + Duration::from_secs(30);
            while left.exists() {
                assert!(
                    Instant::now() < deadline,
                    "{} was not deleted",
                    left.display()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(other.exists());
        },
    );
}
