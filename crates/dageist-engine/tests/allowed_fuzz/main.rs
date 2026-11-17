//! A property target that checks the tables of allowed combinations against
//! Polars.
//!
//! The tables record what Polars does with each method on each dtype, in each
//! place an expression stands, and with each pair of options, measured one
//! combination at a time. This target makes the combinations they were not
//! measured as: chains several links deep from a column or a literal, expressions
//! inside others (conditions, `over`'s keys), the places a step holds them and
//! sets of options. It predicts each case from the tables and the named rules
//! alone, runs it through the step document on rows of those dtypes, and
//! compares the two. A disagreement is reduced to its smallest case and kept as
//! the bytes of the case, which the step document's own writer makes, so it can
//! be replayed.
//!
//! The fixed seed below runs with the engine's tests. [`a_campaign`] runs more
//! on request, and [`replay_a_case`] replays one kept case; the how-to guide
//! says how.

mod case;
mod frames;
mod generate;
mod limits;
mod masks;
mod observe;
mod predict;
mod probe;
mod reduce;
mod target;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use dageist_fuzz::engine::{PropertyConfig, replay, run_property};
use dageist_fuzz::generation::property;
use dageist_fuzz::reduction::{ReductionLimits, reduce_direct};
use dageist_fuzz::{CaseOutcome, CaseTarget, OutcomeKind};

use case::Case;
use limits::Limit;
use target::{AllowedTarget, take_tally};

/// The seed and size of the run that goes with the engine's tests.
const SEED: u64 = 0x00d4_6e15_7a11_0001;
const CASES: u32 = 1000;

/// Where kept evidence of a disagreement is written, unless a directory is
/// named by `DAGEIST_FUZZ_KEEP`.
fn keep_directory() -> PathBuf {
    std::env::var_os("DAGEIST_FUZZ_KEEP").map_or_else(
        || PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("allowed-fuzz"),
        PathBuf::from,
    )
}

fn regressions() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/allowed_fuzz/regressions")
}

/// What a disagreement came to: the case, reduced, and what disagreed.
struct Finding {
    kept: PathBuf,
    reduced: Vec<u8>,
    detail: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "the tables and Polars disagree: {}\nthe smallest case found, kept as {}:\n{}",
            self.detail,
            self.kept.display(),
            String::from_utf8_lossy(&self.reduced)
        )
    }
}

fn detail_of(outcome: &CaseOutcome) -> String {
    format!(
        "{}: {}",
        outcome
            .failure_key()
            .and_then(|key| key.discriminator())
            .unwrap_or("?"),
        outcome.detail().unwrap_or_default()
    )
}

/// Runs `work` on a thread with a stack large enough for Polars to build and
/// resolve the deepest expression a case makes in a debug build, which the
/// two megabytes of a test thread are not.
fn on_a_large_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("allowed-fuzz".to_owned())
            .stack_size(256 << 20)
            .spawn_scoped(scope, work)
            .expect("a thread with a large stack")
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
    })
}

/// Runs `cases` cases made from `seed`. When one disagrees, reduces it and
/// keeps the result under the directory for kept evidence.
fn campaign(seed: u64, cases: u32) -> (Result<u64, Finding>, Vec<String>) {
    let target = AllowedTarget::new();
    let strategy = property::bytes(512, target.descriptor().limits());
    let config = PropertyConfig {
        cases,
        max_rejections: 1_000_000,
        // Entropy is not the case: the owner's reduction works on the case.
        max_shrink_iterations: 0,
        seed,
    };
    // The case is written down before it is run, so that one that does not
    // return is still found: it is the file `current.case`.
    let directory = keep_directory();
    std::fs::create_dir_all(&directory).expect("a directory for kept evidence");
    let current = directory.join("current.case");
    let mut capture = |bytes: &[u8]| {
        std::fs::write(&current, bytes).map_err(|error| {
            dageist_fuzz::FuzzError::with_source(
                dageist_fuzz::FuzzErrorKind::Io,
                "capture the case",
                "write the case about to run",
                error,
            )
        })
    };
    let report = run_property(&target, &strategy, config, &mut capture)
        .unwrap_or_else(|error| panic!("the property could not be run: {error:?}"));
    // The tally is of the run, not of the reduction of what it found.
    let tally = take_tally();
    let Some(found) = report.failure else {
        return (Ok(report.counts.evaluated()), tally);
    };
    let key = found
        .outcome
        .failure_key()
        .expect("a failure has its key")
        .clone();
    let limits = ReductionLimits::new(400, Duration::from_secs(600), 1 << 24, 2)
        .expect("limits of a reduction");
    let reduction = reduce_direct(
        &target,
        &found.canonical,
        &key,
        limits,
        &AtomicBool::new(false),
    )
    .unwrap_or_else(|error| panic!("the reduction could not be run: {error:?}"));
    let reduced = reduction
        .canonical_input()
        .unwrap_or_else(|| reduction.best_input())
        .to_vec();
    let outcome = target
        .evaluate(&target.decode(&reduced).expect("a reduced case is a case"))
        .expect("a reduced case is evaluated");
    let kept = directory.join(format!("{seed:016x}-{}.case", reduced.len()));
    std::fs::write(&kept, &reduced).expect("evidence is kept");
    let finding = Finding {
        kept,
        reduced,
        detail: detail_of(&outcome),
    };
    (Err(finding), tally)
}

fn env_number<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|text| text.trim().replace('_', "").parse().ok())
        .unwrap_or(default)
}

#[test]
fn the_tables_predict_what_polars_does_for_the_fixed_seed() {
    let started = Instant::now();
    let (result, tally) = on_a_large_stack(|| campaign(SEED, CASES));
    eprintln!(
        "{CASES} cases of seed {SEED:#x} took {:.1?}\n{}",
        started.elapsed(),
        tally.join("\n")
    );
    match result {
        Ok(evaluated) => assert!(
            evaluated >= u64::from(CASES) / 2,
            "only {evaluated} of {CASES} cases checked something"
        ),
        Err(finding) => panic!("{finding}"),
    }
}

/// A longer run: `DAGEIST_FUZZ_CASES` cases from `DAGEIST_FUZZ_SEED`.
#[test]
#[ignore = "a campaign, run on request with DAGEIST_FUZZ_CASES and DAGEIST_FUZZ_SEED"]
fn a_campaign() {
    let seed: u64 = env_number("DAGEIST_FUZZ_SEED", SEED);
    let cases: u32 = env_number("DAGEIST_FUZZ_CASES", 5_000);
    let started = Instant::now();
    let (result, tally) = on_a_large_stack(|| campaign(seed, cases));
    eprintln!(
        "{cases} cases of seed {seed:#x} took {:.1?}\n{}",
        started.elapsed(),
        tally.join("\n")
    );
    if let Err(finding) = result {
        panic!("{finding}");
    }
}

/// Replays the case in the file `DAGEIST_FUZZ_REPLAY` names.
#[test]
#[ignore = "replays one kept case, named by DAGEIST_FUZZ_REPLAY"]
fn replay_a_case() {
    let path = std::env::var_os("DAGEIST_FUZZ_REPLAY")
        .expect("DAGEIST_FUZZ_REPLAY names the file of a kept case");
    let bytes = std::fs::read(&path).expect("the file of the case is read");
    let evaluation = replay(&AllowedTarget::new(), &bytes, &mut |_| Ok(()))
        .unwrap_or_else(|error| panic!("the case could not be replayed: {error:?}"));
    let case = Case::decode(&evaluation.canonical).expect("a replayed case is a case");
    println!("{}", String::from_utf8_lossy(&evaluation.canonical));
    println!("{}", on_a_large_stack(|| target::describe(&case)));
    println!("{:?}", evaluation.outcome);
    assert_ne!(
        evaluation.outcome.kind(),
        OutcomeKind::Failure,
        "{}",
        detail_of(&evaluation.outcome)
    );
}

/// Every kept case replays, and no longer disagrees. A file named
/// `fixed-*.case` was a disagreement the tables or a named rule now settle,
/// and replays as checked; a file named `stricter-<what it shows>.case` is a
/// case where the tables refuse a part Polars does not evaluate, and a probe of
/// that part shows the tables right; a file named `uncovered-<limit>.case` is a case the
/// tables state they do not cover, and replays as excused by that limit, and
/// every limit has one. A limit may have others, named
/// `uncovered-<limit>--<what it shows>.case`.
#[test]
fn every_kept_case_replays_as_settled() {
    on_a_large_stack(kept_cases_replay_as_settled);
}

fn kept_cases_replay_as_settled() {
    let target = AllowedTarget::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(regressions())
        .expect("the directory of kept cases")
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "case")
        })
        .collect();
    files.sort();
    let mut fixed = 0;
    let mut stricter = 0;
    let mut excused = std::collections::BTreeSet::new();
    for path in files {
        let bytes = std::fs::read(&path).expect("a kept case is read");
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("a file name");
        let evaluation = replay(&target, &bytes, &mut |_| Ok(()))
            .unwrap_or_else(|error| panic!("{name} could not be replayed: {error:?}"));
        let outcome = &evaluation.outcome;
        if name.starts_with("fixed-") {
            assert_eq!(
                outcome.kind(),
                OutcomeKind::Evaluated,
                "{name} is settled and checked: {}",
                detail_of(outcome)
            );
            fixed += 1;
        } else if name.starts_with("stricter-") {
            assert_eq!(outcome.kind(), OutcomeKind::ExpectedRejection, "{name}");
            assert!(
                outcome
                    .detail()
                    .is_some_and(|detail| detail.starts_with(target::STRICTER_WHERE_UNEVALUATED)),
                "{name} is stricter where Polars does not evaluate the part: {}",
                detail_of(outcome)
            );
            stricter += 1;
        } else if let Some(slug) = name
            .strip_prefix("uncovered-")
            .and_then(|rest| rest.strip_suffix(".case"))
            // `uncovered-<limit>--<what it shows>.case` is another case of a limit.
            .map(|rest| rest.split("--").next().unwrap_or(rest))
        {
            let limit = Limit::ALL
                .into_iter()
                .find(|limit| limit.slug() == slug)
                .unwrap_or_else(|| panic!("{name} names no limit"));
            assert_eq!(
                (outcome.kind(), outcome.detail()),
                (OutcomeKind::ExpectedRejection, Some(limit.statement())),
                "{name} is excused by {slug}"
            );
            excused.insert(slug.to_owned());
        } else {
            panic!("{name} is neither fixed-* nor uncovered-*");
        }
    }
    assert!(fixed > 0, "a disagreement that was settled is kept");
    assert!(stricter > 0, "a part Polars does not evaluate is kept");
    for limit in Limit::ALL {
        assert!(
            excused.contains(limit.slug()),
            "no kept case shows the limit {}",
            limit.slug()
        );
    }
}

#[test]
fn a_case_reads_back_as_the_bytes_it_was_written_as() {
    let target = AllowedTarget::new();
    let strategy_seed = 7_u64;
    let mut checked = 0;
    for offset in 0..24_u64 {
        let entropy: Vec<u8> = (0..400_u64)
            .map(|at| {
                let mixed = (at + offset * 977 + strategy_seed).wrapping_mul(0x9e37_79b9_7f4a_7c15);
                (mixed >> 29) as u8
            })
            .collect();
        let dageist_fuzz::Generation::Generated(case) = target.generate(&entropy).unwrap() else {
            continue;
        };
        let bytes = target.encode(&case).unwrap();
        assert_eq!(target.decode(&bytes).unwrap(), case);
        assert_eq!(
            target.encode(&target.decode(&bytes).unwrap()).unwrap(),
            bytes
        );
        checked += 1;
    }
    assert!(checked > 0);
}

#[test]
fn a_case_of_another_revision_or_not_written_canonically_is_refused() {
    let target = AllowedTarget::new();
    let kept = br#"{"a":"Int64","b":"Int64","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[]}}]}}}}]}"#;
    assert!(target.decode(kept).is_ok());
    let other_revision = String::from_utf8_lossy(kept).replace("\"revision\":1", "\"revision\":2");
    assert!(target.decode(other_revision.as_bytes()).is_err());
    let spaced = String::from_utf8_lossy(kept).replace("\"a\":\"Int64\",", "\"a\": \"Int64\",");
    assert!(target.decode(spaced.as_bytes()).is_err());
    let unknown = String::from_utf8_lossy(kept).replace("\"Int64\",\"b\"", "\"Int65\",\"b\"");
    assert!(target.decode(unknown.as_bytes()).is_err());
    let _: Case = target.decode(kept).unwrap();
}
