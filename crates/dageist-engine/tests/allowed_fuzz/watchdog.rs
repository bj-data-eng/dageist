//! The timer of a campaign: it times the case in progress, and when one has had longer
//! than its deadline it keeps what it knows and ends the process.
//!
//! A case that Polars never returns from cannot be ended from inside: nothing unwinds a
//! thread that waits for ever, and a panic of a Polars task leaves the streaming executor
//! broken for every query after it, so the process is not worth going on in either. The
//! timer is a thread of its own that looks at the case in progress every few hundred
//! milliseconds. It keeps the case that did not end, keeps the last case in which any
//! thread panicked, which a panic hook notes since a thread of Polars' own is not one that
//! `catch_unwind` in the target sees, says what the campaign found so far and how to go on
//! from the next case, and exits with [`TIMEOUT_STATUS`].

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The status of a campaign that ended because a case did not.
pub const TIMEOUT_STATUS: i32 = 75;

/// The file in the directory of kept evidence that holds the settings that go on from the
/// case after the one that did not end, one `NAME=value` to a line.
pub const GO_ON: &str = "timeout.env";

/// What the timer needs to know of the campaign it times.
pub struct Context {
    pub seed: u64,
    /// Where it keeps what it knows, if it keeps anything.
    pub keep: Option<PathBuf>,
    pub deadline: Duration,
}

struct Current {
    number: u64,
    canonical: Vec<u8>,
    since: Instant,
}

/// A disagreement that is being reduced, as it was found.
struct Reducing {
    number: u64,
    canonical: Vec<u8>,
    detail: String,
}

/// The case in progress, and the last that ended, which a panic is put to when no case is.
struct Timing {
    current: Option<Current>,
    last: Option<(u64, Vec<u8>)>,
    /// How many cases before the one in progress were counted, which is read with the case in
    /// progress, under the same lock, so that a case that ends as it is read is not counted twice.
    counted: u64,
}

struct Shared {
    context: Context,
    timing: Mutex<Timing>,
    /// The last case in which a thread panicked: its number and its bytes.
    panicked: Mutex<Option<(u64, Vec<u8>)>>,
    reducing: Mutex<Option<Reducing>>,
    /// What the campaign found so far, as it prints it.
    findings: Mutex<Vec<String>>,
    stop: AtomicBool,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Watchdog {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Watchdog {
    /// Starts timing: installs the panic hook, which stays for the life of the process, and
    /// the thread that looks at the case in progress.
    pub fn start(context: Context) -> Self {
        let poll =
            (context.deadline / 4).clamp(Duration::from_millis(10), Duration::from_millis(200));
        let shared = Arc::new(Shared {
            context,
            timing: Mutex::new(Timing {
                current: None,
                last: None,
                counted: 0,
            }),
            reducing: Mutex::new(None),
            panicked: Mutex::new(None),
            findings: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
        });
        let noted = Arc::clone(&shared);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |information| {
            // A blocking lock could wait for a thread that is itself waiting on this one, so
            // take it without blocking, and try again for a moment if the timer has it.
            for _ in 0..100 {
                if let Ok(timing) = noted.timing.try_lock() {
                    let ran = timing
                        .current
                        .as_ref()
                        .map(|current| (current.number, current.canonical.clone()))
                        .or_else(|| timing.last.clone());
                    drop(timing);
                    if let Some(ran) = ran {
                        for _ in 0..100 {
                            if let Ok(mut panicked) = noted.panicked.try_lock() {
                                *panicked = Some(ran);
                                break;
                            }
                            std::thread::sleep(Duration::from_micros(100));
                        }
                    }
                    break;
                }
                std::thread::sleep(Duration::from_micros(100));
            }
            previous(information);
        }));
        let looking = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("allowed-fuzz-timer".to_owned())
            .spawn(move || {
                while !looking.stop.load(Ordering::Acquire) {
                    std::thread::sleep(poll);
                    let timing = locked(&looking.timing);
                    let late = timing
                        .current
                        .as_ref()
                        .filter(|current| current.since.elapsed() >= looking.context.deadline)
                        .map(|current| (current.number, current.canonical.clone()));
                    if let Some((number, canonical)) = late {
                        // The lock is kept to the end, so that the case cannot end, and be
                        // counted, between its being found late and the process ending.
                        timed_out(&looking, &timing, number, &canonical);
                    }
                }
            })
            .expect("the thread of the timer");
        Self {
            shared,
            thread: Some(thread),
        }
    }

    /// The case `number`, written as `canonical`, begins.
    pub fn begin(&self, number: u64, canonical: &[u8]) {
        locked(&self.shared.timing).current = Some(Current {
            number,
            canonical: canonical.to_vec(),
            since: Instant::now(),
        });
    }

    /// The case in progress ends.
    pub fn end(&self) {
        let mut timing = locked(&self.shared.timing);
        if let Some(current) = timing.current.take() {
            timing.last = Some((current.number, current.canonical));
        }
    }

    /// The case `number`, written as `canonical`, disagreed in `detail`, and is being reduced.
    pub fn reducing(&self, number: u64, canonical: &[u8], detail: String) {
        *locked(&self.shared.reducing) = Some(Reducing {
            number,
            canonical: canonical.to_vec(),
            detail,
        });
    }

    /// The reduction ended, and what it came to is found.
    pub fn reduced(&self) {
        *locked(&self.shared.reducing) = None;
    }

    /// How many cases so far were counted towards the campaign's number of cases.
    pub fn counted(&self, counted: u64) {
        locked(&self.shared.timing).counted = counted;
    }

    /// The campaign found something, which the timer prints if it ends the campaign.
    pub fn found(&self, finding: String) {
        locked(&self.shared.findings).push(finding);
    }

    /// Runs `work`, timed as the case `number` written as `canonical`.
    pub fn timing<T>(&self, number: u64, canonical: &[u8], work: impl FnOnce() -> T) -> T {
        self.begin(number, canonical);
        let done = work();
        self.end();
        done
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Keeps what is known, says it, and ends the process.
fn timed_out(shared: &Shared, timing: &Timing, number: u64, canonical: &[u8]) -> ! {
    let context = &shared.context;
    let reducing = locked(&shared.reducing);
    let reducing = reducing
        .as_ref()
        .filter(|reducing| reducing.number == number);
    let (name, what) = if reducing.is_some() {
        (
            format!("{:016x}-{number}-reduction-timeout.case", context.seed),
            format!("a replay in the reduction of the case {number}"),
        )
    } else {
        (
            format!("{:016x}-{number}-timeout.case", context.seed),
            format!("the case {number}"),
        )
    };
    let mut said = format!(
        "{what} did not end in {} ms\n",
        context.deadline.as_millis()
    );
    let keep = |name: String, bytes: &[u8]| -> String {
        let Some(directory) = &context.keep else {
            return "not kept".to_owned();
        };
        let path = directory.join(name);
        match std::fs::write(&path, bytes) {
            Ok(()) => format!("kept as {}", path.display()),
            Err(error) => format!("not kept: {error}"),
        }
    };
    let kept = keep(name, canonical);
    said.push_str(&format!("{what} is {kept}\n"));
    match locked(&shared.panicked).as_ref() {
        Some((earlier, bytes)) => {
            let kept = keep(
                format!("{:016x}-{earlier}-panicked.case", context.seed),
                bytes,
            );
            let when = if *earlier == number {
                "this case"
            } else {
                "an earlier case"
            };
            said.push_str(&format!(
                "likely cause: a thread panicked in the case {earlier}, {when}, {kept}\n"
            ));
        }
        None => said.push_str("no thread panicked in any case before it\n"),
    }
    let findings = locked(&shared.findings);
    said.push_str(&format!(
        "findings so far: {}\n",
        findings.len() + usize::from(reducing.is_some())
    ));
    for finding in findings.iter() {
        said.push_str(finding);
        said.push('\n');
    }
    if let Some(reducing) = reducing {
        let kept = keep(
            format!("{:016x}-{number}-unreduced.case", context.seed),
            &reducing.canonical,
        );
        said.push_str(&format!(
            "the case {number} disagrees, not reduced, {kept}: {}\n",
            reducing.detail
        ));
    }
    if let Some(directory) = &context.keep {
        let settings = format!(
            "DAGEIST_FUZZ_START={}\nDAGEIST_FUZZ_COUNTED={}\n",
            number + 1,
            // A case that did not end counts, as a case that is kept as a failure does.
            timing.counted + 1
        );
        let path = directory.join(GO_ON);
        let written = std::fs::write(&path, &settings);
        said.push_str(&format!(
            "to go on from the next case, run again with\n{}\n(also {} {})\n",
            settings.replace('\n', " ").trim_end(),
            if written.is_ok() {
                "written to"
            } else {
                "not written to"
            },
            path.display()
        ));
    }
    // Written to the process's own error output, which the test harness does not capture,
    // since a captured report is lost when the process ends.
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(said.as_bytes());
    let _ = stderr.flush();
    let _ = std::io::stdout().flush();
    std::process::exit(TIMEOUT_STATUS)
}
