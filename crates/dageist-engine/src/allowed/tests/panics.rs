//! The combinations Polars panics on, and the rule that refuses each before Polars is asked.
//!
//! `arr.min` and `arr.max` of an array of anything but whole numbers and floats, `reverse` of
//! such an array, and `list.mean` of a list of `Float16` panic where the rows run, on a thread of
//! Polars' own, which can leave its streaming executor broken for the queries after it (#102). A
//! named rule refuses each before the tables or a document ask Polars, so that none is run.
//!
//! The panics were found by a probe that is not a table: each combination is run here in a
//! process of its own, so that a panic that stalls the next query ends with that process.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::Command;

use polars::prelude::{DataType, Expr, IntoLazy, col};

use crate::allowed::dtype::Representative;
use crate::allowed::offers::{After, Chosen, Place, RefusalSource, Verdict, offer};
use crate::allowed::table::{DtypeLine, Outcome, lines};
use crate::allowed::{Context, Outer, Rule, Shape};
use crate::catalog;

use super::sample::Frames;

/// Set in the process that probes, to the method and the class it is to run, as `method|class`.
const PROBE: &str = "DAGEIST_PANIC_PROBE";

/// The method of Polars that an entry stands for, built by its own API and not the catalogue's.
fn polars_call(entry: &str) -> Expr {
    let a = col("a");
    match entry {
        "Expr.arr.min" => a.arr().min(),
        "Expr.arr.max" => a.arr().max(),
        "Expr.list.mean" => a.list().mean(),
        "Expr.reverse" => a.reverse(),
        other => panic!("{other} is no method that the probe knows"),
    }
}

/// The combinations a rule is to refuse, found by the probe below: every one that the rule names.
fn panicking() -> Vec<(&'static str, Representative)> {
    let mut found = Vec::new();
    for class in Representative::ALL {
        let numeric = |inner: crate::allowed::dtype::Inner| {
            let dtype = inner.class().dtype();
            dtype.is_integer() || dtype.is_float()
        };
        match class {
            Representative::Array(inner) if !numeric(inner) => {
                found.extend([
                    ("Expr.arr.min", class),
                    ("Expr.arr.max", class),
                    ("Expr.reverse", class),
                ]);
            }
            Representative::List(crate::allowed::dtype::Inner::Float16) => {
                found.push(("Expr.list.mean", class));
            }
            _ => {}
        }
    }
    found
}

/// Runs the probe named by `PROBE`: it exits 0 if Polars panicked, 1 if it ran, 2 if it refused.
/// It does nothing without it.
#[test]
fn probe_a_combination_that_polars_panics_on() {
    let Ok(wanted) = std::env::var(PROBE) else {
        return;
    };
    let (entry, class) = wanted.split_once('|').expect("a method and a class");
    let class = Representative::from_name(class).expect("a class");
    crate::startup::ensure();
    let frames = Frames::new(class, class, &DataType::Int64);
    let call = polars_call(entry);
    let ran = catch_unwind(AssertUnwindSafe(|| {
        frames
            .rows
            .clone()
            .lazy()
            .select([call.alias("out")])
            .collect()
    }));
    std::process::exit(match ran {
        Err(_) => 0,
        Ok(Ok(_)) => 1,
        Ok(Err(_)) => 2,
    });
}

#[test]
fn polars_panics_where_the_rows_run_on_each_combination_the_rule_names() {
    let mut not_panicking = Vec::new();
    for (entry, class) in panicking() {
        let status = Command::new(std::env::current_exe().expect("the test program"))
            .args([
                "--exact",
                "allowed::tests::panics::probe_a_combination_that_polars_panics_on",
                "--test-threads=1",
            ])
            .env(PROBE, format!("{entry}|{}", class.name()))
            .env("POLARS_MAX_THREADS", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("the test program runs");
        if status.code() != Some(0) {
            not_panicking.push(format!("{entry} on {class}: exit {:?}", status.code()));
        }
    }
    assert!(
        not_panicking.is_empty(),
        "Polars does not panic on {} combinations the rule names: {not_panicking:?}",
        not_panicking.len()
    );
}

fn verdict(entry: &str, class: Representative) -> Verdict {
    let entry = catalog::entry(entry).unwrap_or_else(|| panic!("no entry {entry}"));
    offer(
        entry,
        &Place::new(
            Context::step(Outer::Select),
            After::Value {
                dtype: class,
                shape: Shape::Many,
            },
        ),
        &Chosen::default(),
    )
}

#[test]
fn a_named_rule_refuses_each_combination_polars_panics_on_and_says_what_the_element_lacks() {
    for (entry, class) in panicking() {
        let verdict = verdict(entry, class);
        let refusal = verdict
            .refusal()
            .unwrap_or_else(|| panic!("{entry} on {class} is refused: {verdict:?}"));
        assert!(
            matches!(
                refusal.source(),
                RefusalSource::Rule(Rule::ElementDtypeIsImplemented)
            ),
            "{entry} on {class} is refused by a rule: {refusal}"
        );
        let element = class.element().expect("a list or an array").class().name();
        assert!(
            refusal.reason().contains(element),
            "{entry} on {class} says which element it lacks: {}",
            refusal.reason()
        );
    }
}

#[test]
fn no_combination_the_rule_does_not_name_is_refused_by_it_or_panics_in_the_tables() {
    let named = panicking();
    for class in Representative::ALL
        .into_iter()
        .filter(|class| class.is_nested())
    {
        for entry in [
            "Expr.arr.min",
            "Expr.arr.max",
            "Expr.list.mean",
            "Expr.reverse",
        ] {
            let by_a_rule = matches!(
                verdict(entry, class),
                Verdict::Refused(refusal) if matches!(
                    refusal.source(),
                    RefusalSource::Rule(Rule::ElementDtypeIsImplemented)
                )
            );
            assert_eq!(
                by_a_rule,
                named.contains(&(entry, class)),
                "{entry} on {class}"
            );
        }
    }
    // No line of the tables for a method of the `list` or `arr` namespace is a panic: the tables
    // never run a combination the rule refuses, and Polars panics on no other.
    let committed =
        lines(crate::allowed::table::DTYPES, DtypeLine::parse).expect("the table reads");
    for line in committed
        .iter()
        .filter(|line| line.entry.starts_with("Expr.list.") || line.entry.starts_with("Expr.arr."))
    {
        assert!(
            !matches!(&line.outcome, Outcome::Refused(reason) if reason.text.starts_with("Polars panicked")),
            "{} on {:?} is a panic of Polars'",
            line.entry,
            line.receiver
        );
    }
}
