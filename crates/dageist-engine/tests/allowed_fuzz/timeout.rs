//! A campaign whose case does not end is stopped by its timer, and a run from the case
//! after it goes on.
//!
//! The campaign is run in a process of its own, this test binary run to
//! [`a_campaign_for_a_parent_to_watch`], since it ends the process it runs in. That test
//! runs the cases of a seed through an evaluator that writes down each case it is given,
//! and blocks for ever in one, which is how a case that Polars never returns from looks
//! to a campaign, and makes a thread panic in another. The tests here are its parent.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use dageist_fuzz::{CaseOutcome, FailureClass, FailureKey, PropertyId, Result};

use super::campaign::{Evaluator, InProcess, Observation, Settings, run};
use super::target::{ALLOWANCE, AllowedTarget};

/// The variable that names the directory the child works in, and so asks for a campaign.
const CHILD: &str = "DAGEIST_FUZZ_TIMEOUT_TEST";

/// The status of a campaign that stopped at a case that did not end.
const TIMEOUT_STATUS: i32 = 75;

const SEED: u64 = 7;
const CASES: u32 = 25;

/// How long a parent gives its child to end, which is well above the few seconds that the
/// timers of these tests need, and well below a hang.
const PARENT_WAIT: Duration = Duration::from_secs(40);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|at| u8::from_str_radix(&text[2 * at..2 * at + 2], 16).expect("hex"))
        .collect()
}

/// Whether the case written as `canonical` is one of those that check nothing, which depends
/// on the case alone.
fn is_quiet(canonical: &[u8]) -> bool {
    canonical.iter().map(|byte| u64::from(*byte)).sum::<u64>() % 4 == 0
}

/// An evaluator that writes down each case it is given, blocks for ever in the case named
/// by `block`, and makes a thread panic in the case named by `panics`; otherwise it
/// evaluates as the target does.
struct Scripted<'a> {
    evaluation: InProcess<'a>,
    log: PathBuf,
    block: Option<Vec<u8>>,
    panics: Option<Vec<u8>>,
    /// The case that ends as a disagreement, which a reduction then replays.
    disagrees: Option<Vec<u8>>,
    /// Whether the replays of a reduction block for ever, once the case that disagrees has.
    hangs_in_reduction: bool,
    disagreed: std::cell::Cell<bool>,
}

impl Evaluator for Scripted<'_> {
    fn observe(&self, canonical: &[u8]) -> Result<Observation> {
        use std::io::Write;
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .expect("the log");
        // Whether the case counts towards the campaign's number of cases is not known until
        // it has been evaluated, which the case that blocks never is.
        if self.block.as_deref() == Some(canonical) {
            writeln!(log, "{}\t?", hex(canonical)).expect("the log is written");
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
        if self.hangs_in_reduction
            && self.disagreed.get()
            && self.disagrees.as_deref() != Some(canonical)
        {
            writeln!(log, "{}\t?", hex(canonical)).expect("the log is written");
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
        if self.panics.as_deref() == Some(canonical) {
            let _ = std::thread::spawn(|| panic!("a thread of Polars panics")).join();
        }
        let mut observation = self.evaluation.observe(canonical)?;
        // About a quarter of the cases, whichever the run, check nothing, so that cases
        // before a case that hangs are not all counted.
        if is_quiet(canonical) {
            observation.outcome = CaseOutcome::unsupported("a scripted case that checks nothing")?;
        }
        if self.disagrees.as_deref() == Some(canonical) {
            self.disagreed.set(true);
            observation.outcome = CaseOutcome::failure(
                FailureKey::new(
                    PropertyId::new(ALLOWANCE)?,
                    FailureClass::PropertyViolation,
                    Some("scripted".to_owned()),
                )?,
                "a scripted disagreement",
            )?;
        }
        let counts =
            observation.outcome.is_evaluated() || observation.outcome.failure_key().is_some();
        writeln!(log, "{}\t{}", hex(canonical), u8::from(counts)).expect("the log is written");
        Ok(observation)
    }
}

fn file_of(directory: &Path, name: &str) -> Option<Vec<u8>> {
    std::fs::read(directory.join(name)).ok()
}

/// The campaign that [`Parent`] watches, which does nothing unless it is asked to.
#[test]
#[ignore = "the campaign a test watches, in a process of its own"]
fn a_campaign_for_a_parent_to_watch() {
    let Some(directory) = std::env::var_os(CHILD).map(PathBuf::from) else {
        return;
    };
    let wanted = |name: &str| {
        file_of(&directory, name).map(|text| unhex(String::from_utf8_lossy(&text).trim()))
    };
    let settings = child_settings(&directory);
    super::on_a_large_stack(|| {
        let target = AllowedTarget::new();
        let scripted = Scripted {
            evaluation: InProcess::new(&target),
            log: directory.join("log"),
            block: wanted("block"),
            panics: wanted("panics"),
            disagrees: wanted("disagrees"),
            hangs_in_reduction: directory.join("hangs_in_reduction").exists(),
            disagreed: std::cell::Cell::new(false),
        };
        // A replay of the case that blocks is timed as a campaign is.
        if let Some(case) = scripted
            .block
            .clone()
            .filter(|_| directory.join("replay").exists())
        {
            super::replayed_in_time(&case, super::env_deadline(), || scripted.observe(&case))
                .expect("a replay that does not end does not return");
            return;
        }
        let report = run(&settings, &scripted);
        println!("the campaign ended after {} cases", report.evaluated);
    });
}

/// What the child's campaign is given.
fn child_settings(directory: &Path) -> Settings {
    let number = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|text| text.parse::<u64>().ok())
    };
    let mut settings = Settings::new(SEED, CASES, None, directory.join("kept"));
    std::fs::create_dir_all(&settings.keep).expect("a directory for kept evidence");
    settings.deadline = number("DAGEIST_FUZZ_DEADLINE_MS").map(Duration::from_millis);
    settings.start = number("DAGEIST_FUZZ_START").unwrap_or(1);
    settings.counted = number("DAGEIST_FUZZ_COUNTED");
    settings
}

/// What a child that was run came to.
struct Ran {
    /// The status it ended with, if it ended by itself.
    status: Option<i32>,
    said: String,
    /// The cases it was given, in order.
    log: Vec<Vec<u8>>,
    /// Whether each counted towards the campaign's number of cases.
    counted: Vec<bool>,
}

/// Runs the child in `directory`, with `settings` as variables, and waits for it to end for
/// no longer than [`PARENT_WAIT`].
fn run_child(directory: &Path, settings: &[(&str, String)]) -> Ran {
    run_child_as(directory, settings, true)
}

/// As [`run_child`], with the output of the test shown, or left to the test harness to capture.
fn run_child_as(directory: &Path, settings: &[(&str, String)], nocapture: bool) -> Ran {
    let said = directory.join("said");
    let output = std::fs::File::create(&said).expect("the output");
    let mut command = Command::new(std::env::current_exe().expect("this binary"));
    command
        .args([
            "--exact",
            "timeout::a_campaign_for_a_parent_to_watch",
            "--ignored",
            "--test-threads=1",
        ])
        .args(nocapture.then_some("--nocapture"))
        // Nothing of the settings the tests run under reaches the campaign, but what they give.
        .env_remove("DAGEIST_FUZZ_START")
        .env_remove("DAGEIST_FUZZ_COUNTED")
        .env_remove("DAGEIST_FUZZ_DEADLINE_MS")
        .env_remove("DAGEIST_FUZZ_KEEP")
        .env_remove("DAGEIST_FUZZ_SEED")
        .env_remove("DAGEIST_FUZZ_CASES")
        .env_remove("DAGEIST_FUZZ_FOCUS")
        .env(CHILD, directory)
        .envs(settings.iter().map(|(name, value)| (*name, value)))
        .stdin(Stdio::null())
        .stdout(output.try_clone().expect("the output"))
        .stderr(output);
    let mut child = command.spawn().expect("the child starts");
    let began = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("the child is waited for") {
            break status.code();
        }
        if began.elapsed() >= PARENT_WAIT {
            child.kill().expect("the child is stopped");
            child.wait().expect("the child is reaped");
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let lines: Vec<String> = file_of(directory, "log").map_or_else(Vec::new, |text| {
        String::from_utf8_lossy(&text)
            .lines()
            .map(str::to_owned)
            .collect()
    });
    let field = |line: &String, at: usize| line.split('\t').nth(at).unwrap_or_default().to_owned();
    Ran {
        status,
        said: String::from_utf8_lossy(&std::fs::read(said).unwrap_or_default()).into_owned(),
        log: lines.iter().map(|line| unhex(&field(line, 0))).collect(),
        counted: lines.iter().map(|line| field(line, 1) == "1").collect(),
    }
}

fn fresh(name: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a directory");
    directory
}

/// The value of `name` in the lines `NAME=value` of `text`.
fn setting(text: &str, name: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix(&format!("{name}=")))
        .and_then(|value| value.trim().parse().ok())
}

#[test]
fn a_case_that_does_not_end_stops_the_campaign_with_a_status_and_the_run_from_the_next_case_goes_on()
 {
    // The cases of the seed, run to their end.
    let whole = run_child(&fresh("timeout-whole"), &[]);
    assert_eq!(whole.status, Some(0), "{}", whole.said);
    let cases = whole.log.len();
    assert!(cases >= 12, "{cases}");

    // The same cases, with a thread that panics in the fourth and a case that never ends,
    // the first of those from the ninth on that counts, since a case that does not end counts.
    let hangs = (8..cases)
        .find(|at| whole.counted[*at])
        .expect("a counted case");
    let number = hangs as u64 + 1;
    let directory = fresh("timeout-stopped");
    std::fs::write(directory.join("panics"), hex(&whole.log[3])).unwrap();
    std::fs::write(directory.join("block"), hex(&whole.log[hangs])).unwrap();
    let stopped = run_child(
        &directory,
        &[("DAGEIST_FUZZ_DEADLINE_MS", "3000".to_owned())],
    );
    assert_eq!(stopped.status, Some(TIMEOUT_STATUS), "{}", stopped.said);
    assert!(
        stopped.log == whole.log[..=hangs],
        "it stopped at the case that hung"
    );
    let kept = directory.join("kept");
    let files = std::fs::read_dir(&kept)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    let named = |what: &str| -> Vec<&PathBuf> {
        files
            .iter()
            .filter(|path| path.to_string_lossy().ends_with(what))
            .collect()
    };
    let [timeout] = named(&format!("-{number}-timeout.case"))[..] else {
        panic!("the case that hung is kept as a timeout: {files:?}");
    };
    assert_eq!(std::fs::read(timeout).unwrap(), whole.log[hangs]);
    let [panicked] = named("-4-panicked.case")[..] else {
        panic!("the fourth case is kept as the likely cause: {files:?}");
    };
    assert_eq!(std::fs::read(panicked).unwrap(), whole.log[3]);
    let env =
        String::from_utf8(file_of(&kept, "timeout.env").expect("the setting to go on")).unwrap();
    let start = number + 1;
    assert_eq!(setting(&env, "DAGEIST_FUZZ_START"), Some(start), "{env}");
    assert!(
        stopped
            .said
            .contains(&format!("DAGEIST_FUZZ_START={start}")),
        "{}",
        stopped.said
    );
    assert!(
        stopped.said.contains(&format!("case {number}")),
        "{}",
        stopped.said
    );

    // The run from the next case makes the cases the whole run made from there on, and ends
    // where it did.
    let counted = setting(&env, "DAGEIST_FUZZ_COUNTED").expect("how many were counted");
    let rest = run_child(
        &fresh("timeout-rest"),
        &[
            ("DAGEIST_FUZZ_START", start.to_string()),
            ("DAGEIST_FUZZ_COUNTED", counted.to_string()),
        ],
    );
    assert_eq!(rest.status, Some(0), "{}", rest.said);
    assert_eq!(
        rest.log.len(),
        cases - hangs - 1,
        "it ends where the whole run did"
    );
    assert!(
        rest.log == whole.log[hangs + 1..],
        "the cases from the next on are the same"
    );
}

/// With no thread panicked before it, nothing is kept as the likely cause, and the campaign
/// says so.
#[test]
fn a_case_that_does_not_end_after_no_panic_keeps_no_likely_cause() {
    let whole = run_child(&fresh("timeout-quiet-whole"), &[]);
    let directory = fresh("timeout-quiet");
    std::fs::write(directory.join("block"), hex(&whole.log[5])).unwrap();
    let stopped = run_child(
        &directory,
        &[("DAGEIST_FUZZ_DEADLINE_MS", "3000".to_owned())],
    );
    assert_eq!(stopped.status, Some(TIMEOUT_STATUS), "{}", stopped.said);
    assert!(
        stopped.said.contains("no thread panicked"),
        "{}",
        stopped.said
    );
    let kept: Vec<String> = std::fs::read_dir(directory.join("kept"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        kept.iter().all(|name| !name.ends_with("-panicked.case")),
        "{kept:?}"
    );
}

/// A replay of a case that does not end is timed as a campaign is, and ends the process with
/// the same status.
#[test]
fn a_replay_of_a_case_that_does_not_end_ends_with_the_status_of_a_timeout() {
    let whole = run_child(&fresh("timeout-replay-whole"), &[]);
    let directory = fresh("timeout-replay");
    std::fs::write(directory.join("block"), hex(&whole.log[0])).unwrap();
    std::fs::write(directory.join("replay"), "").unwrap();
    let stopped = run_child(
        &directory,
        &[("DAGEIST_FUZZ_DEADLINE_MS", "3000".to_owned())],
    );
    assert_eq!(stopped.status, Some(TIMEOUT_STATUS), "{}", stopped.said);
    assert!(
        stopped.said.contains("did not end in 3000 ms"),
        "{}",
        stopped.said
    );
    assert!(
        stopped.said.contains("the replay of"),
        "a replay is not a case of a campaign: {}",
        stopped.said
    );
}

/// Two cases in a row that do not end, the second the first one a resumed run evaluates, are
/// gone on from one after the other, and the three runs make the cases of the whole run, in
/// order, and end where it did.
#[test]
fn a_run_that_hangs_at_its_first_case_after_a_resume_says_how_many_cases_it_counted() {
    let whole = run_child(&fresh("timeout-chain-whole"), &[]);
    assert_eq!(whole.status, Some(0), "{}", whole.said);
    let cases = whole.log.len();
    let first = (4..cases - 1)
        .find(|at| whole.counted[*at] && whole.counted[*at + 1])
        .expect("two counted cases in a row");
    let (deadline, mut logs) = (("DAGEIST_FUZZ_DEADLINE_MS", "3000".to_owned()), Vec::new());

    let one = fresh("timeout-chain-one");
    std::fs::write(one.join("block"), hex(&whole.log[first])).unwrap();
    let hung = run_child(&one, std::slice::from_ref(&deadline));
    assert_eq!(hung.status, Some(TIMEOUT_STATUS), "{}", hung.said);
    logs.extend(hung.log.clone());
    let env = String::from_utf8(file_of(&one.join("kept"), "timeout.env").unwrap()).unwrap();
    let (start, counted) = (
        setting(&env, "DAGEIST_FUZZ_START").unwrap(),
        setting(&env, "DAGEIST_FUZZ_COUNTED").unwrap(),
    );
    assert_eq!(start, first as u64 + 2, "{env}");

    // The first case it evaluates is the one that hangs.
    let two = fresh("timeout-chain-two");
    std::fs::write(two.join("block"), hex(&whole.log[first + 1])).unwrap();
    let hung_again = run_child(
        &two,
        &[
            deadline,
            ("DAGEIST_FUZZ_START", start.to_string()),
            ("DAGEIST_FUZZ_COUNTED", counted.to_string()),
        ],
    );
    assert_eq!(
        hung_again.status,
        Some(TIMEOUT_STATUS),
        "{}",
        hung_again.said
    );
    logs.extend(hung_again.log.clone());
    let env = String::from_utf8(file_of(&two.join("kept"), "timeout.env").unwrap()).unwrap();
    let (start, counted_again) = (
        setting(&env, "DAGEIST_FUZZ_START").unwrap(),
        setting(&env, "DAGEIST_FUZZ_COUNTED").unwrap(),
    );
    assert_eq!(counted_again, counted + 1, "{env}");

    let rest = run_child(
        &fresh("timeout-chain-rest"),
        &[
            ("DAGEIST_FUZZ_START", start.to_string()),
            ("DAGEIST_FUZZ_COUNTED", counted_again.to_string()),
        ],
    );
    assert_eq!(rest.status, Some(0), "{}", rest.said);
    logs.extend(rest.log.clone());
    assert_eq!(
        logs.len(),
        cases,
        "the runs together end where the whole run did"
    );
    assert!(
        logs == whole.log,
        "the runs together make the cases of the whole run"
    );
}

/// A replay of a reduction that does not end ends the campaign too, with the case that
/// disagreed in what it found, kept as it was, and the replay named for what it is.
#[test]
fn a_replay_of_a_reduction_that_does_not_end_is_kept_as_one_and_counts_the_case_once() {
    let whole = run_child(&fresh("timeout-reduction-whole"), &[]);
    let cases = whole.log.len();
    let at = (4..cases)
        .find(|at| whole.counted[*at])
        .expect("a counted case");
    let number = at as u64 + 1;
    let directory = fresh("timeout-reduction");
    std::fs::write(directory.join("disagrees"), hex(&whole.log[at])).unwrap();
    std::fs::write(directory.join("hangs_in_reduction"), "").unwrap();
    let stopped = run_child(
        &directory,
        &[("DAGEIST_FUZZ_DEADLINE_MS", "3000".to_owned())],
    );
    assert_eq!(stopped.status, Some(TIMEOUT_STATUS), "{}", stopped.said);
    let kept = directory.join("kept");
    let names: Vec<String> = std::fs::read_dir(&kept)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let has = |suffix: String| names.iter().any(|name| name.ends_with(&suffix));
    assert!(
        has(format!("-{number}-reduction-timeout.case")),
        "{names:?}"
    );
    assert!(!has(format!("-{number}-timeout.case")), "{names:?}");
    let unreduced = names
        .iter()
        .find(|name| name.ends_with(&format!("-{number}-unreduced.case")))
        .unwrap_or_else(|| panic!("the case that disagreed is kept: {names:?}"));
    assert_eq!(file_of(&kept, unreduced).unwrap(), whole.log[at]);
    assert!(
        stopped.said.contains("a scripted disagreement"),
        "{}",
        stopped.said
    );
    assert!(
        stopped
            .said
            .contains(&format!("reduction of the case {number}")),
        "{}",
        stopped.said
    );
    // The case that disagreed counts, once.
    let env = String::from_utf8(file_of(&kept, "timeout.env").unwrap()).unwrap();
    let before = whole.counted[..at]
        .iter()
        .filter(|counted| **counted)
        .count() as u64;
    assert_eq!(
        setting(&env, "DAGEIST_FUZZ_COUNTED"),
        Some(before + 1),
        "{env}"
    );
}

/// What a campaign says when a case does not end reaches whoever runs it whether or not the
/// test harness shows the output of tests.
#[test]
fn the_report_of_a_case_that_does_not_end_is_shown_without_nocapture() {
    let whole = run_child(&fresh("timeout-shown-whole"), &[]);
    let directory = fresh("timeout-shown");
    std::fs::write(directory.join("block"), hex(&whole.log[5])).unwrap();
    let stopped = run_child_as(
        &directory,
        &[("DAGEIST_FUZZ_DEADLINE_MS", "3000".to_owned())],
        false,
    );
    assert_eq!(stopped.status, Some(TIMEOUT_STATUS), "{}", stopped.said);
    assert!(
        stopped.said.contains("DAGEIST_FUZZ_START="),
        "{}",
        stopped.said
    );
}

/// A `timeout.env` that an earlier run left is not left for a timed run that ends well to be
/// found as if it were that run's, and an untimed run, which the fixed seed of the engine's
/// tests is, leaves it for the run that goes on from it.
#[test]
fn only_a_timed_campaign_removes_a_timeout_env_of_an_earlier_run() {
    let stale = |name: &str| {
        let directory = fresh(name);
        std::fs::create_dir_all(directory.join("kept")).unwrap();
        std::fs::write(directory.join("kept/timeout.env"), "DAGEIST_FUZZ_START=3\n").unwrap();
        directory
    };
    let timed = stale("timeout-stale-timed");
    let ended = run_child(&timed, &[("DAGEIST_FUZZ_DEADLINE_MS", "15000".to_owned())]);
    assert_eq!(ended.status, Some(0), "{}", ended.said);
    assert!(!timed.join("kept/timeout.env").exists());
    let untimed = stale("timeout-stale-untimed");
    let ended = run_child(&untimed, &[]);
    assert_eq!(ended.status, Some(0), "{}", ended.said);
    assert!(untimed.join("kept/timeout.env").exists());
}
