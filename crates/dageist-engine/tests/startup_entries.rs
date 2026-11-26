//! Whatever runs Polars through the engine finds Polars set up, as the Python
//! Polars release sets it up when it loads, without being asked to start it.
//!
//! The set-up is observed through the SQL resolver. A query written in SQL is
//! planned without being resolved, and Polars resolves it when the plan is
//! collected, which it can do only once the resolver is registered. Nothing in
//! Polars registers it but the set-up. So a plan collected after an engine entry
//! has run shows that the entry started Polars. Each entry runs in a process of
//! its own, since once any entry has started Polars it stays started.

mod support;

use std::fs;
use std::path::PathBuf;

use dageist_engine::allowed::Literal;
use dageist_engine::catalog::{self, Argument, Build};
use dageist_engine::document::{Document, Inputs, StepId};
use dageist_engine::{ParquetTable, SampleSpec, Table, generate_claims};
use polars::prelude::*;
use polars::sql::SQLContext;
use serde_json::json;
use support::in_fresh_process;

/// The variable that gives a process the directory its test made for it.
const DIRECTORY: &str = "DAGEIST_TEST_DIRECTORY";
const KEYS: u32 = 20;
const SPEC: SampleSpec = SampleSpec {
    keys: KEYS,
    filler_columns: 0,
};

/// A directory of the test's own, removed when the test ends.
struct Directory(PathBuf);

impl Directory {
    fn named(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("dageist-startup-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        Self(path)
    }

    /// The variables for a process, after a sample is generated in the directory.
    ///
    /// Only the process that starts the test calls it. Generating a sample runs
    /// Polars through the engine, so a process that did would find Polars set up
    /// before the entry it is meant to try.
    fn variables_with_sample(&self) -> Vec<(&'static str, String)> {
        generate_claims(&self.0, SPEC).expect("the sample is generated");
        self.variables()
    }

    fn variables(&self) -> Vec<(&'static str, String)> {
        let mut variables = plan_variables();
        variables.push((DIRECTORY, self.0.display().to_string()));
        variables
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn directory_given() -> PathBuf {
    PathBuf::from(std::env::var(DIRECTORY).expect("the process is given a directory"))
}

/// The variable that gives a process a SQL plan, as JSON.
const PLAN: &str = "DAGEIST_TEST_SQL_PLAN";

/// The variables a process needs to see whether Polars is set up: a plan written
/// in SQL, built here, in the process that starts it. Building one constructs an
/// SQL context, which registers the resolver in this process only; the process
/// that is started reads the plan as text and never constructs a context.
fn plan_variables() -> Vec<(&'static str, String)> {
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
    vec![(PLAN, String::from_utf8(json).expect("the plan is text"))]
}

/// Collects the plan the process was given, which resolves its SQL.
fn sql_resolves() {
    let json = std::env::var(PLAN).expect("the process is given a plan");
    let plan = DslPlan::deserialize_json_from_str(&json).expect("the plan is read");
    let rows = LazyFrame::from(plan)
        .collect()
        .expect("the query is resolved: Polars was set up");
    assert_eq!(rows.height(), 2);
}

fn document(steps: serde_json::Value) -> Document {
    Document::read(&json!({ "version": 1, "steps": steps }).to_string()).expect("a document")
}

fn sorted_by_a() -> Document {
    let by = json!({ "many": [{ "expression": { "start": {
        "entry": "polars.col",
        "arguments": { "name": { "columns": ["a"] } },
    }, "then": [] } }] });
    document(
        json!([{ "id": "sort", "call": { "entry": "LazyFrame.sort", "arguments": { "by": by } } }]),
    )
}

fn small_table() -> DataFrame {
    df!("a" => [3_i64, 1, 2]).expect("a table")
}

#[test]
fn generating_a_sample_starts_polars() {
    let directory = Directory::named("generate");
    in_fresh_process(
        "generating_a_sample_starts_polars",
        || directory.variables(),
        || {
            generate_claims(&directory_given(), SPEC).expect("the sample is generated");
            sql_resolves();
        },
    );
}

#[test]
fn loading_a_working_window_starts_polars() {
    let directory = Directory::named("window");
    in_fresh_process(
        "loading_a_working_window_starts_polars",
        || directory.variables_with_sample(),
        || {
            Table::load_keys(&directory_given(), "dcn", KEYS).expect("the window loads");
            sql_resolves();
        },
    );
}

#[test]
fn opening_a_stored_table_starts_polars() {
    let directory = Directory::named("stored");
    in_fresh_process(
        "opening_a_stored_table_starts_polars",
        || directory.variables_with_sample(),
        || {
            ParquetTable::open(&directory_given()).expect("the table opens");
            sql_resolves();
        },
    );
}

#[test]
fn running_a_document_starts_polars() {
    in_fresh_process("running_a_document_starts_polars", plan_variables, || {
        sorted_by_a()
            .run(&Inputs::new(), small_table().lazy())
            .expect("the document runs");
        sql_resolves();
    });
}

#[test]
fn collecting_the_rows_of_a_step_starts_polars() {
    in_fresh_process(
        "collecting_the_rows_of_a_step_starts_polars",
        plan_variables,
        || {
            let through = StepId::new("sort").expect("an id");
            let rows = sorted_by_a()
                .rows(&Inputs::new(), &small_table(), &through, 10)
                .expect("the rows");
            assert_eq!(rows.height(), 3);
            sql_resolves();
        },
    );
}

#[test]
fn describing_an_input_table_starts_polars() {
    in_fresh_process(
        "describing_an_input_table_starts_polars",
        plan_variables,
        || {
            let mut inputs = Inputs::new();
            inputs
                .add_table("other", "Other", small_table().lazy())
                .expect("an input");
            inputs.describe().expect("the input is described");
            sql_resolves();
        },
    );
}

#[test]
fn walking_a_step_that_names_another_table_starts_polars() {
    in_fresh_process(
        "walking_a_step_that_names_another_table_starts_polars",
        plan_variables,
        || {
            let key = |name: &str| {
                json!({ "many": [{ "expression": { "start": {
                    "entry": "polars.col",
                    "arguments": { "name": { "columns": [name] } },
                }, "then": [] } }] })
            };
            let join = document(json!([{ "id": "join", "call": {
                "entry": "LazyFrame.join",
                "arguments": {
                    "other": { "table": "other" },
                    "left_on": key("a"),
                    "right_on": key("a"),
                },
            } }]));
            let mut inputs = Inputs::new();
            inputs
                .add_table("other", "Other", small_table().lazy())
                .expect("an input");
            let step = &join.steps()[0];
            let walk = step.walk(0, small_table().schema(), &inputs);
            assert_eq!(walk.refusal(), None);
            sql_resolves();
        },
    );
}

#[test]
fn working_out_the_class_of_a_literal_starts_polars() {
    in_fresh_process(
        "working_out_the_class_of_a_literal_starts_polars",
        plan_variables,
        || {
            assert!(Literal::Integer(1).class().is_some());
            sql_resolves();
        },
    );
}

#[test]
fn building_a_pivot_directly_starts_polars() {
    in_fresh_process(
        "building_a_pivot_directly_starts_polars",
        plan_variables,
        || {
            let table =
                df!("i" => [1_i64, 2], "s" => ["a", "b"], "f" => [0.5_f64, 1.5]).expect("a table");
            let pivot = catalog::entry("LazyFrame.pivot").expect("the pivot entry");
            let Build::Frame(build) = pivot.build() else {
                panic!("a pivot builds a frame");
            };
            let given = pivot
                .arguments([
                    ("on", Argument::Columns(vec!["s".to_owned()])),
                    (
                        "on_columns",
                        Argument::Table(df!("s" => ["a", "b"]).expect("the values").lazy()),
                    ),
                    ("index", Argument::Columns(vec!["i".to_owned()])),
                    ("values", Argument::Columns(vec!["f".to_owned()])),
                ])
                .expect("the arguments fit");
            let pivoted = build(table.lazy(), &given).expect("the pivot builds");
            drop(pivoted);
            sql_resolves();
        },
    );
}
