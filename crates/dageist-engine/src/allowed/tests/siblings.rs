//! The claim that the element dtype of a list or an array does not change what an ordinary
//! method accepts, held by a run of Polars.
//!
//! The tables hold lines for the list of whole numbers, the list of text and the array of whole
//! numbers, and read any other list or array as the one that stands for it
//! ([`Representative::sibling`]). That is true of a method only where Polars judges the two alike,
//! so this runs every combination of every class that is read so, in a process of its own on one
//! thread as the tables are made, and fails where Polars judges one differently from the line that
//! is read for it: refuses where the line allows, allows where it refuses, refuses at another
//! stage, or gives another class or another number of values.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use crate::allowed::dtype::Representative;
use crate::allowed::table::{Looked, Outcome, Tables, read_as};

use super::generate::{dtype_jobs_over, dtype_line, in_parallel};

/// Set in the process that measures, to the file it writes what differs to.
const OUT: &str = "DAGEIST_SIBLINGS_OUT";

/// What differs between Polars and the tables for one combination, or nothing.
fn difference(
    polars: &Outcome,
    table: &Outcome,
    receiver: Option<Representative>,
    argument: Option<Representative>,
) -> Option<String> {
    let said = match (polars, table) {
        (Outcome::Allowed(polars), Outcome::Allowed(table)) => {
            let class = |produces: &Option<crate::allowed::table::Produces>| {
                produces
                    .as_ref()
                    .map(|produces| (produces.result.class(), produces.shape))
            };
            (class(polars) != class(table)).then(|| {
                format!(
                    "Polars gives {}, the table {}",
                    polars_result(polars),
                    polars_result(table)
                )
            })
        }
        (Outcome::Refused(polars), Outcome::Refused(table)) => {
            (polars.stage != table.stage).then(|| {
                format!(
                    "Polars refuses at {:?}, the table at {:?}",
                    polars.stage, table.stage
                )
            })
        }
        (Outcome::Allowed(_), Outcome::Refused(table)) => Some(format!(
            "Polars allows it, the table refuses it: {}",
            table.text
        )),
        (Outcome::Refused(polars), Outcome::Allowed(_)) => Some(format!(
            "Polars refuses it, the table allows it: {}",
            polars.text
        )),
    };
    said.map(|said| {
        format!(
            "on {}, given {}: {said}",
            receiver.map_or_else(|| "-".to_owned(), |class| class.to_string()),
            argument.map_or_else(|| "-".to_owned(), |class| class.to_string()),
        )
    })
}

fn polars_result(produces: &Option<crate::allowed::table::Produces>) -> String {
    produces.as_ref().map_or_else(
        || "-".to_owned(),
        |produces| format!("{} {}", produces.result, produces.shape.name()),
    )
}

/// Measures every combination read as its sibling and writes what differs to the file `OUT`
/// names, one line for each entry and parameter, with how many combinations differ and one
/// example. It does nothing without it: the test below runs it in a process of its own.
#[test]
fn measure_the_siblings() {
    let Some(path) = std::env::var_os(OUT) else {
        return;
    };
    let tables = Tables::committed();
    let jobs: Vec<_> = dtype_jobs_over(|_| Representative::ALL.to_vec())
        .into_iter()
        .filter(|job| {
            let id = job.entry.id().as_str();
            [job.receiver, job.argument]
                .into_iter()
                .flatten()
                .any(|class| read_as(id, class) != Some(class))
        })
        .collect();
    let found = in_parallel(&jobs, |job| {
        let line = dtype_line(job);
        match tables.dtype(line.entry, line.parameter, line.receiver, line.argument) {
            Looked::NotJudged => Some((
                job.entry.id().as_str(),
                job.parameter,
                "the tables do not judge it".to_owned(),
            )),
            Looked::Missing => Some((job.entry.id().as_str(), job.parameter, "no line".to_owned())),
            Looked::Line(table) => difference(&line.outcome, &table, line.receiver, line.argument)
                .map(|said| (job.entry.id().as_str(), job.parameter, said)),
        }
    });
    let mut grouped: BTreeMap<(&str, Option<&str>), (usize, String)> = BTreeMap::new();
    for (entry, parameter, said) in found.into_iter().flatten() {
        let slot = grouped.entry((entry, parameter)).or_insert((0, said));
        slot.0 += 1;
    }
    let text: String = grouped
        .iter()
        .map(|((entry, parameter), (count, example))| {
            format!(
                "{entry} {} differs on {count}: {example}\n",
                parameter.unwrap_or("-")
            )
        })
        .collect();
    std::fs::write(PathBuf::from(path), text).expect("what differs is written");
}

#[test]
fn a_list_or_an_array_is_judged_by_each_ordinary_method_as_the_class_that_stands_for_it_is() {
    let file = std::env::temp_dir().join(format!("dageist-siblings-{}", std::process::id()));
    let status = Command::new(std::env::current_exe().expect("the test program"))
        .args([
            "--exact",
            "allowed::tests::siblings::measure_the_siblings",
            "--test-threads=1",
        ])
        .env(OUT, &file)
        .env("POLARS_MAX_THREADS", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("the test program runs");
    assert!(status.success(), "measuring the siblings failed");
    let differing = std::fs::read_to_string(&file).expect("the siblings were measured");
    std::fs::remove_file(&file).expect("the file is removed");
    assert!(
        differing.is_empty(),
        "Polars judges a list or an array differently from the class that stands for it for these \
         methods, which the tables must measure on more of them:\n{differing}"
    );
}
