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
//! on request, focused on chosen methods where it is asked to be, and
//! [`replay_a_case`] replays one kept case; the how-to guide says how.

mod campaign;
mod case;
mod frames;
mod generate;
mod limits;
mod literal;
mod masks;
mod observe;
mod predict;
mod probe;
mod reduce;
mod target;
mod timeout;
mod watchdog;

use std::path::{Path, PathBuf};
use std::time::Instant;

use dageist_fuzz::engine::replay;
use dageist_fuzz::{CaseTarget, OutcomeKind};

use campaign::{InProcess, Report, Settings, detail_of};
use case::Case;
use generate::Focus;
use limits::Limit;
use target::AllowedTarget;

/// The seed and size of the run that goes with the engine's tests.
const SEED: u64 = 0x00d4_6e15_7a11_0001;
const CASES: u32 = 1000;

/// How long a case of a campaign is given, in milliseconds, unless `DAGEIST_FUZZ_DEADLINE_MS`
/// says another. A case that has not ended by then ends the campaign.
const DEADLINE_MS: u64 = 15_000;

/// `path` where it is absolute, and where it is relative the path from the root of the
/// repository, which is where the commands that run a campaign are run from, not the
/// directory of the engine that the tests run in.
fn from_the_root(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

/// Where kept evidence of a failing case is written, unless a directory is named by
/// `DAGEIST_FUZZ_KEEP`, which is taken from the root of the repository if it is relative.
fn keep_directory() -> PathBuf {
    std::env::var_os("DAGEIST_FUZZ_KEEP").map_or_else(
        || PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("allowed-fuzz"),
        |path| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            from_the_root(&root, PathBuf::from(path))
        },
    )
}

fn regressions() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/allowed_fuzz/regressions")
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

/// The methods `DAGEIST_FUZZ_FOCUS` names, which a campaign focuses on. Unset or
/// empty, it names none.
fn env_focus() -> Option<Focus> {
    let text = match std::env::var("DAGEIST_FUZZ_FOCUS") {
        Ok(text) => text,
        Err(std::env::VarError::NotPresent) => return None,
        Err(std::env::VarError::NotUnicode(_)) => panic!("DAGEIST_FUZZ_FOCUS is not text"),
    };
    Focus::parse(&text).unwrap_or_else(|refusal| panic!("{refusal}"))
}

#[test]
fn the_tables_predict_what_polars_does_for_the_fixed_seed() {
    let started = Instant::now();
    let report = on_a_large_stack(|| {
        let target = AllowedTarget::new();
        campaign::run(
            &Settings::new(SEED, CASES, None, keep_directory()),
            &InProcess::new(&target),
        )
    });
    eprintln!(
        "{CASES} cases of seed {SEED:#x} took {:.1?}\n{}",
        started.elapsed(),
        report.tally.join("\n")
    );
    assert_no_findings(&report);
    assert!(
        report.evaluated >= u64::from(CASES) / 2,
        "only {} of {CASES} cases checked something",
        report.evaluated
    );
}

/// Panics with every finding of `report`.
fn assert_no_findings(report: &Report) {
    if !report.findings.is_empty() {
        let findings: Vec<String> = report.findings.iter().map(ToString::to_string).collect();
        panic!("{}", findings.join("\n\n"));
    }
}

/// The whole number that `text` says `name` is, or why it is not one.
fn whole_number(name: &str, text: &str) -> Result<u64, String> {
    text.trim()
        .replace('_', "")
        .parse()
        .map_err(|_| format!("{name} is {text:?}, which is not a whole number"))
}

/// The whole number `name` is set to, if it is set. A setting that is no number is refused
/// rather than ignored.
fn env_whole(name: &str) -> Option<u64> {
    let text = match std::env::var(name) {
        Ok(text) => text,
        Err(std::env::VarError::NotPresent) => return None,
        Err(std::env::VarError::NotUnicode(_)) => panic!("{name} is not text"),
    };
    Some(whole_number(name, &text).unwrap_or_else(|refusal| panic!("{refusal}")))
}

/// How long a case of a campaign is given: `DAGEIST_FUZZ_DEADLINE_MS` milliseconds.
fn env_deadline() -> std::time::Duration {
    let milliseconds = env_whole("DAGEIST_FUZZ_DEADLINE_MS").unwrap_or(DEADLINE_MS);
    assert!(
        milliseconds > 0,
        "DAGEIST_FUZZ_DEADLINE_MS is 0, and no case ends in no time"
    );
    std::time::Duration::from_millis(milliseconds)
}

/// A longer run: `DAGEIST_FUZZ_CASES` cases from `DAGEIST_FUZZ_SEED`, focused on the
/// methods `DAGEIST_FUZZ_FOCUS` names, if it names any, each given `DAGEIST_FUZZ_DEADLINE_MS`
/// milliseconds. It goes on from the case `DAGEIST_FUZZ_START`, with `DAGEIST_FUZZ_COUNTED`
/// of the cases before it counted, which is what a run that a case did not end says.
#[test]
#[ignore = "a campaign, run on request with DAGEIST_FUZZ_CASES, DAGEIST_FUZZ_SEED and DAGEIST_FUZZ_FOCUS"]
fn a_campaign() {
    let seed = env_whole("DAGEIST_FUZZ_SEED").unwrap_or(SEED);
    let cases = env_whole("DAGEIST_FUZZ_CASES").map_or(5_000, |cases| {
        u32::try_from(cases).unwrap_or_else(|_| {
            panic!(
                "DAGEIST_FUZZ_CASES is {cases}, more than a campaign counts, {}",
                u32::MAX
            )
        })
    });
    let focus = env_focus();
    let focused = focus
        .as_ref()
        .map(|focus| format!(" focused on {}", focus.names()))
        .unwrap_or_default();
    let started = Instant::now();
    let report = on_a_large_stack(|| {
        let target = AllowedTarget::new();
        let mut settings = Settings::new(seed, cases, focus, keep_directory());
        settings.deadline = Some(env_deadline());
        settings.start = env_whole("DAGEIST_FUZZ_START").unwrap_or(1);
        settings.counted = env_whole("DAGEIST_FUZZ_COUNTED");
        if let Err(refusal) = settings.check() {
            panic!("{refusal}");
        }
        campaign::run(&settings, &InProcess::new(&target))
    });
    eprintln!(
        "{cases} cases of seed {seed:#x}{focused} took {:.1?}\n{}",
        started.elapsed(),
        report.tally.join("\n")
    );
    assert_no_findings(&report);
}

/// Runs `work` on the case written as `bytes`, which ends the process with the status of a
/// campaign whose case did not end if it takes longer than `deadline`.
fn replayed_in_time<T>(
    replaying: &str,
    bytes: &[u8],
    deadline: std::time::Duration,
    work: impl FnOnce() -> T,
) -> T {
    let watchdog = watchdog::Watchdog::start(watchdog::Context {
        seed: 0,
        keep: None,
        deadline,
        replaying: Some(replaying.to_owned()),
    });
    watchdog.timing(1, bytes, work)
}

/// Replays the case in the file `DAGEIST_FUZZ_REPLAY` names, as long as
/// `DAGEIST_FUZZ_DEADLINE_MS` allows a case.
#[test]
#[ignore = "replays one kept case, named by DAGEIST_FUZZ_REPLAY"]
fn replay_a_case() {
    let path = std::env::var_os("DAGEIST_FUZZ_REPLAY")
        .expect("DAGEIST_FUZZ_REPLAY names the file of a kept case");
    let bytes = std::fs::read(&path).expect("the file of the case is read");
    let (evaluation, described) =
        replayed_in_time(&path.to_string_lossy(), &bytes, env_deadline(), || {
            let evaluation = replay(&AllowedTarget::new(), &bytes, &mut |_| Ok(()))
                .unwrap_or_else(|error| panic!("the case could not be replayed: {error:?}"));
            let case = Case::decode(&evaluation.canonical).expect("a replayed case is a case");
            let described = on_a_large_stack(|| target::describe(&case));
            (evaluation, described)
        });
    println!("{}", String::from_utf8_lossy(&evaluation.canonical));
    println!("{described}");
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
/// The kept cases that settle as checked on more than one thread and, on one, as a part Polars does
/// not evaluate (#103): the probe of the part is refused for the tables' own reason there.
const SETTLED_AS_STRICTER_ON_ONE_THREAD: [&str; 1] = ["fixed-a-deeper-place.case"];

/// The kept cases that Polars refuses on more than one thread and allows on one (#103), where the
/// tables and Polars then agree: a null quantile in a join key.
const AGREE_ON_ONE_THREAD: [&str; 1] =
    ["uncovered-an-approximate-quantile-of-a-literal-in-a-condition--in-a-join-key.case"];

/// How many threads Polars runs on: the size of its own pool, which `POLARS_MAX_THREADS` sets and
/// which is the number of cores where it is not set.
fn polars_threads() -> usize {
    polars_core::runtime::THREAD_POOL.current_num_threads()
}

/// Whether Polars runs on one thread.
fn one_thread() -> bool {
    polars_threads() == 1
}

#[test]
fn the_threads_polars_runs_on_are_the_ones_it_is_told_to() {
    // Where the thread count is set the pool has that size, which is why the tests of what
    // depends on it read the pool and not the setting: a machine of one core has one too.
    if let Some(told) = env_whole("POLARS_MAX_THREADS") {
        assert_eq!(polars_threads() as u64, told);
    }
    assert!(polars_threads() >= 1);
}

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
        if one_thread() && AGREE_ON_ONE_THREAD.contains(&name) {
            assert_eq!(
                outcome.kind(),
                OutcomeKind::Evaluated,
                "{name} is allowed by both on one thread: {}",
                detail_of(outcome)
            );
            if let Some(slug) = name
                .strip_prefix("uncovered-")
                .and_then(|rest| rest.strip_suffix(".case"))
                .and_then(|rest| rest.split("--").next())
            {
                excused.insert(slug.to_owned());
            }
            continue;
        }
        if name.starts_with("fixed-") {
            // On one thread Polars refuses the part the probe asks about for the tables' own
            // reason, where it does not on more, so the case settles as a part it does not evaluate.
            let on_one_thread = one_thread() && SETTLED_AS_STRICTER_ON_ONE_THREAD.contains(&name);
            if on_one_thread && outcome.kind() == OutcomeKind::ExpectedRejection {
                assert!(
                    outcome.detail().is_some_and(
                        |detail| detail.starts_with(target::STRICTER_WHERE_UNEVALUATED)
                    ),
                    "{name} is settled as stricter on one thread: {}",
                    detail_of(outcome)
                );
            } else {
                assert_eq!(
                    outcome.kind(),
                    OutcomeKind::Evaluated,
                    "{name} is settled and checked: {}",
                    detail_of(outcome)
                );
            }
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
        } else if let Some(slugs) = name
            .strip_prefix("uncovered-")
            .and_then(|rest| rest.strip_suffix(".case"))
            // `uncovered-<limit>--<what it shows>.case` is another case of a limit, and
            // `--also-<limit>` after it names another limit that explains the case too.
            .map(|rest| {
                let mut parts = rest.split("--");
                let first = parts.next().unwrap_or(rest);
                let also = parts.filter_map(|part| part.strip_prefix("also-"));
                std::iter::once(first).chain(also).collect::<Vec<_>>()
            })
        {
            let named: Vec<Limit> = slugs
                .iter()
                .map(|slug| {
                    Limit::ALL
                        .into_iter()
                        .find(|limit| limit.slug() == *slug)
                        .unwrap_or_else(|| panic!("{name} names no limit {slug}"))
                })
                .collect();
            // The detail is the statement of every limit that excuses the case, in order.
            let expected: Vec<&str> = Limit::ALL
                .into_iter()
                .filter(|limit| named.contains(limit))
                .map(Limit::statement)
                .collect();
            assert_eq!(
                (outcome.kind(), outcome.detail()),
                (
                    OutcomeKind::ExpectedRejection,
                    Some(expected.join("; ").as_str())
                ),
                "{name} is excused by {}",
                slugs.join(" and ")
            );
            excused.insert(slugs[0].to_owned());
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
fn every_case_a_focused_campaign_makes_holds_a_method_in_focus() {
    // What each focus names, from the catalogue: a method, a namespace and two methods.
    let methods = |chosen: &dyn Fn(&str) -> bool| -> Vec<String> {
        dageist_engine::catalog::entries()
            .iter()
            .filter(|entry| entry.applies() == dageist_engine::catalog::Applies::Expression)
            .map(|entry| entry.id().as_str().to_owned())
            .filter(|id| chosen(id))
            .collect()
    };
    let focuses = [
        ("dt.replace", methods(&|id| id == "Expr.dt.replace")),
        ("dt.", methods(&|id| id.starts_with("Expr.dt."))),
        (
            "replace_strict, over",
            methods(&|id| id == "Expr.replace_strict" || id == "Expr.over"),
        ),
    ];
    for (text, ids) in focuses {
        let target = AllowedTarget::new().with_focus(Focus::parse(text).unwrap());
        let mut made = 0;
        for offset in 0..600_u64 {
            let entropy: Vec<u8> = (0..512_u64)
                .map(|at| {
                    let mut mixed = (offset << 16 | at).wrapping_add(0x9e37_79b9_7f4a_7c15);
                    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                    (mixed ^ (mixed >> 31)) as u8
                })
                .collect();
            let dageist_fuzz::Generation::Generated(case) = target.generate(&entropy).unwrap()
            else {
                continue;
            };
            let written = case.document.write();
            assert!(
                ids.iter()
                    .any(|id| written.contains(&format!("\"entry\":\"{id}\""))),
                "a case focused on {text} holds none of {ids:?}:\n{written}"
            );
            made += 1;
        }
        assert!(made >= 100, "only {made} cases focused on {text} were made");
    }
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

#[test]
fn a_setting_that_is_no_number_is_refused_and_says_which() {
    assert_eq!(whole_number("DAGEIST_FUZZ_START", " 1_235 "), Ok(1235));
    let refusal = whole_number("DAGEIST_FUZZ_COUNTED", "12 cases").unwrap_err();
    assert!(
        refusal.starts_with("DAGEIST_FUZZ_COUNTED is \"12 cases\""),
        "{refusal}"
    );
    assert!(whole_number("DAGEIST_FUZZ_START", "-1").is_err());
}

#[test]
fn a_relative_directory_for_kept_evidence_is_from_the_root_of_the_repository() {
    let root = Path::new("/the/repository");
    assert_eq!(
        from_the_root(root, PathBuf::from("target/tmp/kept")),
        Path::new("/the/repository/target/tmp/kept")
    );
    assert_eq!(
        from_the_root(root, PathBuf::from("/elsewhere/kept")),
        Path::new("/elsewhere/kept")
    );
}
