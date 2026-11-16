//! The committed tables are what Polars gives.
//!
//! The test runs every combination, as the tables' headers say, and compares
//! the text with the committed artifacts under `api/catalogue/`. With
//! `DAGEIST_REFRESH_TABLES` set it writes them instead.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use crate::allowed::rules::Rule;
use crate::allowed::table::{self, Outcome, Stage};

use super::generate::{Generated, generate};

/// Set to rewrite the tables instead of checking them.
const REFRESH: &str = "DAGEIST_REFRESH_TABLES";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The release `vendor/polars` is pinned to.
pub(super) struct Release {
    /// The tag of the Python Polars release, such as `py-2.1.0`.
    tag: String,
    /// The commit the tag names.
    commit: String,
}

/// What `git` prints for `arguments` in `vendor/polars`.
fn polars_git(arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root().join("vendor/polars"))
        .args(arguments)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "`git {}` runs in vendor/polars, which is pinned to a release tag",
        arguments.join(" ")
    );
    String::from_utf8(output.stdout)
        .expect("git prints text")
        .trim()
        .to_owned()
}

/// The tag and the commit of `vendor/polars`. The checkout is at the tag
/// exactly, as it is pinned to a release, so the tag does not change from run
/// to run.
fn release() -> Release {
    Release {
        tag: polars_git(&["describe", "--tags", "--exact-match"]),
        commit: polars_git(&["rev-parse", "HEAD"]),
    }
}

/// How many combinations were allowed, and how many Polars refused at each
/// stage.
#[derive(Default)]
pub(super) struct Counts {
    pub allowed: usize,
    pub schema: usize,
    pub rows: usize,
    pub empty: usize,
}

impl Counts {
    pub(super) fn of<'a>(outcomes: impl IntoIterator<Item = &'a Outcome>) -> Self {
        let mut counts = Self::default();
        for outcome in outcomes {
            match outcome {
                Outcome::Allowed(_) => counts.allowed += 1,
                Outcome::Refused(reason) => match reason.stage {
                    Stage::Schema => counts.schema += 1,
                    Stage::Rows => counts.rows += 1,
                    Stage::Empty => counts.empty += 1,
                },
            }
        }
        counts
    }

    fn summary(&self) -> String {
        format!(
            "{} combinations: {} allowed; {} refused when Polars resolved the schema, {} when it \
             ran the rows, {} when it ran the empty table.",
            self.allowed + self.schema + self.rows + self.empty,
            self.allowed,
            self.schema,
            self.rows,
            self.empty
        )
    }
}

fn header(title: &str, release: &Release, counts: &Counts, format: &[String]) -> String {
    let mut lines = vec![
        format!("# {title}"),
        "#".to_owned(),
        format!(
            "# Polars: the release {} at the commit {} of vendor/polars.",
            release.tag, release.commit
        ),
        "# Generated from the catalogue and Polars by the engine test".to_owned(),
        "# `allowed::tests::artifacts::the_committed_tables_are_what_polars_gives`, with Polars on"
            .to_owned(),
        "# one thread, so that a reason that depends on which of two failures comes first is the"
            .to_owned(),
        "# same every time. Do not edit.".to_owned(),
        "# To refresh after a change of the catalogue or of Polars:".to_owned(),
        format!(
            "#   {REFRESH}=1 cargo test --offline --locked -p dageist-engine allowed::tests::artifacts"
        ),
        "# See docs/how-to.md, \"Refresh The Tables Of Allowed Combinations\".".to_owned(),
        "#".to_owned(),
    ];
    lines.extend(
        format
            .iter()
            .map(|line| format!("# {line}").trim_end().to_owned()),
    );
    lines.extend([
        "#".to_owned(),
        "# An outcome is `allowed <dtype of the result> <one|many>` (`-` for a step), or".to_owned(),
        "# `refused <stage>: <Polars' reason>`. A combination is allowed only when Polars resolved"
            .to_owned(),
        "# its schema, ran it on three rows that include a null, and ran it on a table with no rows."
            .to_owned(),
        "# The stage is where Polars refused it: `schema`; `rows`, when the schema resolved and"
            .to_owned(),
        "# the rows were refused; or `empty`, when only the table with no rows was. `one` is a"
            .to_owned(),
        "# value for the three rows together, `many` any other number. A reason is Polars' message"
            .to_owned(),
        "# up to its first blank line, on one line.".to_owned(),
        "#".to_owned(),
        format!("# {}", counts.summary()),
        "#".to_owned(),
    ]);
    lines.join("\n")
}

fn file(header: &str, mut lines: Vec<String>) -> String {
    lines.sort();
    lines.dedup();
    format!("{header}\n{}\n", lines.join("\n"))
}

/// The three files, by name.
pub(super) fn files(generated: &Generated, release: &Release) -> [(&'static str, String); 3] {
    let text =
        |lines: &[&str]| -> Vec<String> { lines.iter().map(|line| (*line).to_owned()).collect() };
    let dtypes = header(
        "Allowed combinations by dtype",
        release,
        &Counts::of(generated.dtypes.iter().map(|line| &line.outcome)),
        &text(&[
            "Each line is: entry | parameter | receiver | argument | outcome.",
            "",
            "An expression method is run in `select` on a column `a` of the receiver dtype. With",
            "`-` for the parameter and the argument, every parameter is at its default. Otherwise",
            "the parameter is given a column `b` of the argument dtype, or, where Polars takes one",
            "value only (`shift`'s `n` and `fill_value`), the number 1 of that dtype, and the others are at",
            "their defaults. `over` is given `partition_by` unless that is the parameter, which",
            "`*more_exprs` extends. The columns hold three rows: a value, a null and a value. Text",
            "is a duration in days, such as `1d`.",
        ]),
    );
    let mut contexts_format = text(&[
        "Each line is: entry | context | chain | receiver | argument | rules | outcome.",
        "",
        "An entry that is not a step stands in a context: a position of a step that holds an",
        "expression, such as `select`, or such a position inside another expression, such as",
        "`select > over.order_by`. Each entry in each context has two lines, one for each chain:",
        "",
        "  continues: the chain goes on from the entry, with the neutral continuation the context",
        "    needs, and the line says whether the entry may stand there at all. The continuation",
        "    is `.is_not_null()` where a `Boolean` is required (a filter's predicate, what a",
        "    condition gives in a filter, a condition's predicate), a cast to the dtype of the",
        "    other table's key (`Int64`) for the whole expression in a join key, and `.first()`",
        "    for the whole expression in a pivot's aggregate. Where the context needs nothing,",
        "    nothing follows. Polars refusing the entry however it goes on, as it refuses a window",
        "    in a join key, makes the entry unable to stand.",
        "  ends: the chain ends at the entry as it is, and the line says whether the entry may end",
        "    an expression there. A `Boolean` is required of a chain that ends in a filter, so an",
        "    entry that gives something else may stand there, followed by what makes a `Boolean`,",
        "    though it may not end the chain.",
        "",
        "A method is measured on the receiver and argument dtypes that fit it best, the first",
        "that runs in the dtype table (`Int64` first), and, where the chain ends in a context that",
        "needs a `Boolean`, one that gives a `Boolean` if there is one. The result of an allowed",
        "method is what it gives on its own. `col` and `lit` are measured, where the chain ends and",
        "a `Boolean` is needed, on a `Boolean` column and literal, and on a column of whole numbers",
        "and the literal `1` otherwise. Where the chain ends in a join key, the other table's key",
        "has the dtype the key has. `join_right` is evaluated against the other table, whose",
        "columns are named differently from the left table's; `pivot_aggregate` starts from",
        "`element()`, and a method's arguments there are nulls of their dtype.",
        "",
        "The rules column names the rules that govern the line, which dageist holds and Polars",
        "does not. The outcome is Polars' alone, whatever the rules say. The rules are:",
    ]);
    contexts_format.extend(
        Rule::ALL
            .iter()
            .map(|rule| format!("  {}: {}.", rule.name(), rule.statement())),
    );
    let contexts = header(
        "Allowed entries by context",
        release,
        &Counts::of(generated.contexts.iter().map(|line| &line.outcome)),
        &contexts_format,
    );
    let options = header(
        "Allowed combinations of options",
        release,
        &Counts::of(generated.options.iter().map(|line| &line.outcome)),
        &text(&[
            "Each line is: entry | receiver | options | outcome.",
            "",
            "The options are the flags and the choices of an entry. An entry with one option has a",
            "line for each value of it; an entry with more has a line for each pair of options and",
            "each pair of their values, so options that interact are all covered. A flag is `true`,",
            "`false`, or `unset` where Polars decides. A method is run in `select` on a column of",
            "the receiver, with the options it needs to mean anything: `over` is given",
            "`partition_by` and `order_by`. A step is run on the sample table, with what it needs:",
            "a join on `u`, a pivot of `a` by `g`, a rename of `a`, a sort by `a`.",
        ]),
    );
    [
        (
            "dtypes.txt",
            file(
                &dtypes,
                generated
                    .dtypes
                    .iter()
                    .map(table::DtypeLine::render)
                    .collect(),
            ),
        ),
        (
            "contexts.txt",
            file(
                &contexts,
                generated
                    .contexts
                    .iter()
                    .map(table::ContextLine::render)
                    .collect(),
            ),
        ),
        (
            "options.txt",
            file(
                &options,
                generated
                    .options
                    .iter()
                    .map(table::OptionLine::render)
                    .collect(),
            ),
        ),
    ]
}

/// The lines that differ, for a failure message.
fn difference(committed: &str, made: &str) -> String {
    let committed_lines: std::collections::BTreeSet<&str> = committed.lines().collect();
    let made_lines: std::collections::BTreeSet<&str> = made.lines().collect();
    let only = |left: &std::collections::BTreeSet<&str>,
                right: &std::collections::BTreeSet<&str>| {
        left.difference(right)
            .take(8)
            .map(|line| format!("    {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "committed only:\n{}\nPolars now:\n{}",
        only(&committed_lines, &made_lines),
        only(&made_lines, &committed_lines)
    )
}

/// Set in the process that measures, to the directory it writes the tables to.
const OUT: &str = "DAGEIST_TABLES_OUT";

/// Makes the tables in a process of its own that runs Polars on one thread.
///
/// Polars runs the two sides of some operations in parallel and reports the
/// failure that comes first, so a comparison of a `Struct` with another dtype
/// gives one of two reasons from run to run. On one thread it gives the same
/// every time. The pool of threads is fixed when Polars first uses it, which
/// the tests of other modules may have done in this process already, so the
/// measuring is done in a new one.
fn made_by_one_thread() -> [(&'static str, String); 3] {
    let dir = std::env::temp_dir().join(format!("dageist-tables-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a directory for the tables");
    let status = Command::new(std::env::current_exe().expect("the test program"))
        .args([
            "--exact",
            "allowed::tests::artifacts::measure_for_the_tables",
            "--test-threads=1",
        ])
        .env(OUT, &dir)
        .env("POLARS_MAX_THREADS", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("the test program runs");
    assert!(status.success(), "measuring the tables failed");
    let read = |name: &str| {
        std::fs::read_to_string(dir.join(name))
            .unwrap_or_else(|error| panic!("measuring wrote no {name}: {error}"))
    };
    let made = [
        ("dtypes.txt", read("dtypes.txt")),
        ("contexts.txt", read("contexts.txt")),
        ("options.txt", read("options.txt")),
    ];
    std::fs::remove_dir_all(&dir).expect("the directory of the tables is removed");
    made
}

/// Measures the tables and writes them to the directory `OUT` names. It does
/// nothing without it: the test above runs it in a process of its own.
#[test]
fn measure_for_the_tables() {
    let Some(dir) = std::env::var_os(OUT) else {
        return;
    };
    let generated = generate();
    for (name, text) in files(&generated, &release()) {
        std::fs::write(PathBuf::from(&dir).join(name), text).expect("a table is written");
    }
}

#[test]
fn the_committed_tables_are_what_polars_gives() {
    let started = Instant::now();
    let files = made_by_one_thread();
    eprintln!("the tables took {:.1?} to make", started.elapsed());
    let committed = [table::DTYPES, table::CONTEXTS, table::OPTIONS];
    if std::env::var_os(REFRESH).is_some() {
        for (name, text) in &files {
            let path = root().join("api/catalogue").join(name);
            std::fs::create_dir_all(path.parent().expect("a directory")).expect("a directory");
            std::fs::write(&path, text).expect("the table is written");
        }
        return;
    }
    for ((name, made), committed) in files.iter().zip(committed) {
        assert!(
            committed == made,
            "api/catalogue/{name} is not what Polars gives for the catalogue now. Refresh it with: \
             {REFRESH}=1 cargo test --offline --locked -p dageist-engine \
             allowed::tests::artifacts\n{}",
            difference(committed, made)
        );
    }
}
