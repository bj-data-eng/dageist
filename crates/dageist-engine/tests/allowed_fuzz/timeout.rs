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

use dageist_fuzz::Result;

use super::campaign::{Evaluator, InProcess, Observation, Settings, run};
use super::target::AllowedTarget;

/// The variable that names the directory the child works in, and so asks for a campaign.
const CHILD: &str = "DAGEIST_FUZZ_TIMEOUT_TEST";

/// The status of a campaign that stopped at a case that did not end.
const TIMEOUT_STATUS: i32 = 75;

const SEED: u64 = 7;
const CASES: u32 = 25;

/// How long a parent gives its child to end, which is well above what a timer of
/// half a second needs and well below a hang.
const PARENT_WAIT: Duration = Duration::from_secs(40);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|at| u8::from_str_radix(&text[2 * at..2 * at + 2], 16).expect("hex"))
        .collect()
}

/// An evaluator that writes down each case it is given, blocks for ever in the case named
/// by `block`, and makes a thread panic in the case named by `panics`; otherwise it
/// evaluates as the target does.
struct Scripted<'a> {
    evaluation: InProcess<'a>,
    log: PathBuf,
    block: Option<Vec<u8>>,
    panics: Option<Vec<u8>>,
}

impl Evaluator for Scripted<'_> {
    fn observe(&self, canonical: &[u8], within: Duration) -> Result<Observation> {
        use std::io::Write;
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .expect("the log");
        writeln!(log, "{}", hex(canonical)).expect("the log is written");
        if self.block.as_deref() == Some(canonical) {
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
        if self.panics.as_deref() == Some(canonical) {
            let _ = std::thread::spawn(|| panic!("a thread of Polars panics")).join();
        }
        self.evaluation.observe(canonical, within)
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
        };
        let report = run(&settings, &scripted);
        println!("the campaign ended after {} cases", report.evaluated);
    });
}

/// What the child's campaign is given.
fn child_settings(directory: &Path) -> Settings {
    Settings::new(SEED, CASES, None, directory.join("kept"))
}

/// What a child that was run came to.
struct Ran {
    /// The status it ended with, if it ended by itself.
    status: Option<i32>,
    said: String,
    /// The cases it was given, in order.
    log: Vec<Vec<u8>>,
}

/// Runs the child in `directory`, with `settings` as variables, and waits for it to end for
/// no longer than [`PARENT_WAIT`].
fn run_child(directory: &Path, settings: &[(&str, String)]) -> Ran {
    let said = directory.join("said");
    let output = std::fs::File::create(&said).expect("the output");
    let mut command = Command::new(std::env::current_exe().expect("this binary"));
    command
        .args([
            "--exact",
            "timeout::a_campaign_for_a_parent_to_watch",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
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
    let log = file_of(directory, "log").map_or_else(Vec::new, |text| {
        String::from_utf8_lossy(&text).lines().map(unhex).collect()
    });
    Ran {
        status,
        said: String::from_utf8_lossy(&std::fs::read(said).unwrap_or_default()).into_owned(),
        log,
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
    assert!(cases >= 10, "{cases}");

    // The same cases, with a thread that panics in the fourth and a case that never ends in
    // the ninth.
    let directory = fresh("timeout-stopped");
    std::fs::write(directory.join("panics"), hex(&whole.log[3])).unwrap();
    std::fs::write(directory.join("block"), hex(&whole.log[8])).unwrap();
    let stopped = run_child(
        &directory,
        &[("DAGEIST_FUZZ_DEADLINE_MS", "500".to_owned())],
    );
    assert_eq!(stopped.status, Some(TIMEOUT_STATUS), "{}", stopped.said);
    assert_eq!(stopped.log, whole.log[..9], "it stopped at the ninth case");
    let kept = directory.join("kept");
    let timed_out = std::fs::read_dir(&kept)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    let named = |what: &str| -> Vec<&PathBuf> {
        timed_out
            .iter()
            .filter(|path| path.to_string_lossy().ends_with(what))
            .collect()
    };
    let [timeout] = named("-9-timeout.case")[..] else {
        panic!("the ninth case is kept as a timeout: {timed_out:?}");
    };
    assert_eq!(std::fs::read(timeout).unwrap(), whole.log[8]);
    let [panicked] = named("-4-panicked.case")[..] else {
        panic!("the fourth case is kept as the likely cause: {timed_out:?}");
    };
    assert_eq!(std::fs::read(panicked).unwrap(), whole.log[3]);
    let env =
        String::from_utf8(file_of(&kept, "timeout.env").expect("the setting to go on")).unwrap();
    assert_eq!(setting(&env, "DAGEIST_FUZZ_START"), Some(10), "{env}");
    assert!(
        stopped.said.contains("DAGEIST_FUZZ_START=10"),
        "{}",
        stopped.said
    );
    assert!(stopped.said.contains("case 9"), "{}", stopped.said);

    // The run from the next case makes the cases the whole run made from there on, and ends
    // where it did.
    let counted = setting(&env, "DAGEIST_FUZZ_COUNTED").expect("how many were counted");
    let rest = run_child(
        &fresh("timeout-rest"),
        &[
            ("DAGEIST_FUZZ_START", "10".to_owned()),
            ("DAGEIST_FUZZ_COUNTED", counted.to_string()),
        ],
    );
    assert_eq!(rest.status, Some(0), "{}", rest.said);
    assert_eq!(rest.log, whole.log[9..]);
}
