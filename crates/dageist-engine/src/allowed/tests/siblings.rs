//! The claim that the element dtype of a list or an array does not change what an ordinary
//! method accepts, held by a run of Polars.
//!
//! The tables hold lines for the list of whole numbers, the list of text and the array of whole
//! numbers, and read any other list or array as the one that stands for it
//! ([`Representative::sibling`]). That is true of a method only where Polars judges the two alike,
//! so this runs every combination of every class that is read so, in a process of its own on one
//! thread as the tables are made, and finds the methods for which Polars judges one differently
//! from the line that is read for it: it refuses where the line allows, allows where it refuses,
//! refuses at another stage, or gives another class or another number of values. Those are the
//! methods that [`SENSITIVE`] names, and no others, and the tables measure them on more classes.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use crate::allowed::dtype::Representative;
use crate::allowed::table::{
    Looked, Outcome, Produces, SENSITIVE, Tables, measured_classes, restored,
};

use super::generate::{dtype_jobs_over, dtype_line, in_parallel};

/// Set in the process that measures, to the file it writes what differs to.
const OUT: &str = "DAGEIST_SIBLINGS_OUT";

fn polars_result(produces: Option<&Produces>) -> String {
    produces.map_or_else(
        || "-".to_owned(),
        |produces| format!("{} {}", produces.result, produces.shape.name()),
    )
}

/// What differs between Polars and the line read for the class that stands for each, or nothing.
fn difference(polars: &Outcome, table: &Outcome) -> Option<String> {
    match (polars, table) {
        (Outcome::Allowed(polars), Outcome::Allowed(table)) => {
            let class = |produces: &Option<Produces>| {
                produces
                    .as_ref()
                    .map(|produces| (produces.result.class(), produces.shape))
            };
            (class(polars) != class(table)).then(|| {
                format!(
                    "Polars gives {}, the table {}",
                    polars_result(polars.as_ref()),
                    polars_result(table.as_ref())
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
    }
}

/// Measures every combination read as its sibling and writes the entries for which Polars judges
/// one differently to the file `OUT` names, one line for each, with one example. It does nothing
/// without it: the test below runs it in a process of its own.
#[test]
fn measure_the_siblings() {
    let Some(path) = std::env::var_os(OUT) else {
        return;
    };
    let tables = Tables::committed();
    let jobs: Vec<_> = dtype_jobs_over(|_| Representative::ALL.to_vec())
        .into_iter()
        .filter(|job| {
            let measured = measured_classes(job.entry.id().as_str());
            [job.receiver, job.argument]
                .into_iter()
                .flatten()
                .any(|class| !measured.contains(&class))
        })
        .collect();
    let found = in_parallel(&jobs, |job| {
        let line = dtype_line(job);
        // What a panic of Polars' says is no judgment of the element, and what refuses it is a rule.
        let panicked = matches!(
            &line.outcome,
            Outcome::Refused(reason) if reason.text.starts_with("Polars panicked")
        );
        let sibling = |class: Option<Representative>| class.map(Representative::sibling);
        let table = match tables.dtype(
            line.entry,
            line.parameter,
            sibling(line.receiver),
            sibling(line.argument),
        ) {
            Looked::Line(outcome) => restored(
                &outcome,
                &[line.receiver, line.argument],
                &[sibling(line.receiver), sibling(line.argument)],
            ),
            Looked::NotJudged | Looked::Missing => return None,
        };
        difference(&line.outcome, &table).map(|said| {
            (
                job.entry.id().as_str(),
                format!(
                    "{}{}: on {}, given {}: {said}",
                    if panicked { "PANIC " } else { "" },
                    job.parameter.unwrap_or("-"),
                    line.receiver
                        .map_or_else(|| "-".to_owned(), |class| class.to_string()),
                    line.argument
                        .map_or_else(|| "-".to_owned(), |class| class.to_string()),
                ),
            )
        })
    });
    let mut text = String::new();
    let mut entries = BTreeSet::new();
    for (entry, said) in found.into_iter().flatten() {
        if entries.insert(entry) {
            text.push_str(&format!("{entry} | {said}\n"));
        }
    }
    std::fs::write(PathBuf::from(path), text).expect("what differs is written");
}

#[test]
fn only_the_methods_that_depend_on_the_element_dtype_judge_a_list_or_an_array_unlike_its_sibling() {
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
    let found: BTreeSet<&str> = differing
        .lines()
        .filter_map(|line| line.split_once(" | ").map(|(entry, _)| entry))
        .collect();
    let named: BTreeSet<&str> = SENSITIVE.into_iter().collect();
    let unnamed: Vec<&str> = found.difference(&named).copied().collect();
    let unproved: Vec<&str> = named.difference(&found).copied().collect();
    assert!(
        unnamed.is_empty() && unproved.is_empty(),
        "Polars judges a list or an array unlike the class that stands for it for the methods \
         {unnamed:?}, which the tables must measure on more of them, and the methods {unproved:?} \
         are named though it does not:\n{differing}"
    );
}

#[test]
fn a_method_that_does_not_depend_on_the_element_dtype_is_judged_for_every_list_and_array() {
    let tables = Tables::committed();
    for entry in crate::catalog::entries()
        .iter()
        .filter(|entry| entry.applies() == crate::catalog::Applies::Expression)
        .filter(|entry| !SENSITIVE.contains(&entry.id().as_str()))
    {
        let id = entry.id().as_str();
        for job in dtype_jobs_over(|_| Representative::ALL.to_vec())
            .into_iter()
            .filter(|job| job.entry.id().as_str() == id)
        {
            let looked = tables.dtype(id, job.parameter, job.receiver, job.argument);
            assert!(
                matches!(looked, Looked::Line(_)),
                "{id} {:?} on {:?} given {:?} is {looked:?}",
                job.parameter,
                job.receiver,
                job.argument
            );
        }
    }
}

#[test]
fn a_method_that_depends_on_the_element_dtype_is_judged_for_the_classes_it_is_measured_on() {
    let tables = Tables::committed();
    for id in SENSITIVE {
        let measured = measured_classes(id);
        assert_eq!(measured.len(), 35, "{id}");
        for job in dtype_jobs_over(|_| Representative::ALL.to_vec())
            .into_iter()
            .filter(|job| job.entry.id().as_str() == id)
        {
            let all_measured = [job.receiver, job.argument]
                .into_iter()
                .flatten()
                .all(|class| measured.contains(&class));
            let looked = tables.dtype(id, job.parameter, job.receiver, job.argument);
            assert_eq!(
                matches!(looked, Looked::Line(_)),
                all_measured,
                "{id} {:?} on {:?} given {:?} is {looked:?}",
                job.parameter,
                job.receiver,
                job.argument
            );
        }
    }
}
