//! A campaign: the cases a seed makes, each evaluated by an [`Evaluator`], and
//! every disagreement reduced and kept.
//!
//! Where a case is evaluated is the evaluator's: [`InProcess`] runs it on the
//! thread that asks, which is what the fixed seed of the engine's tests does.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use dageist_fuzz::engine::{PropertyConfig, run_property};
use dageist_fuzz::generation::property;
use dageist_fuzz::reduction::{
    ReductionEvaluation, ReductionLimits, ReductionReport, ReductionStatus, reduce_with,
};
use dageist_fuzz::{
    CaseOutcome, CaseTarget, FailureKey, FuzzError, Generation, Result, Sha256Digest,
    TargetDescriptor,
};

use super::case::Case;
use super::generate::Focus;
use super::target::{AllowedTarget, Tally, take_counts, tally_lines};
use super::watchdog::{Context, Watchdog};

/// What evaluating one case came to.
pub struct Observation {
    pub outcome: CaseOutcome,
    /// How the case ended, as [`take_counts`] says of the cases of a thread.
    pub tally: Tally,
    /// What the evaluation wrote that the reduction's budget of output counts.
    pub output_bytes: u64,
}

/// Where and how a case is evaluated.
pub trait Evaluator {
    /// Evaluates the case written as `canonical`, in no more than `within`.
    fn observe(&self, canonical: &[u8], within: Duration) -> Result<Observation>;
}

/// Evaluates a case on the thread that asks, with the target's own evaluation.
pub struct InProcess<'a> {
    target: &'a AllowedTarget,
}

impl<'a> InProcess<'a> {
    pub const fn new(target: &'a AllowedTarget) -> Self {
        Self { target }
    }
}

impl Evaluator for InProcess<'_> {
    fn observe(&self, canonical: &[u8], _within: Duration) -> Result<Observation> {
        let outcome = self.target.evaluate(&self.target.decode(canonical)?)?;
        let output_bytes = outcome.detail().map_or(0, |detail| detail.len() as u64);
        Ok(Observation {
            outcome,
            tally: take_counts(),
            output_bytes,
        })
    }
}

/// What a campaign runs.
pub struct Settings {
    pub seed: u64,
    pub cases: u32,
    pub focus: Option<Focus>,
    /// Where evidence is kept.
    pub keep: PathBuf,
    pub reduction: ReductionLimits,
    /// How long a case may take before the campaign ends, if it is timed.
    pub deadline: Option<Duration>,
    /// The number of the first case that is evaluated, from 1: the cases before it are made
    /// and passed over, so that the cases from it on are those of a run that began at 1.
    pub start: u64,
    /// How many of the cases before `start` were counted towards `cases` in that run, if
    /// it is known, and all of them if it is not.
    pub counted: Option<u64>,
}

impl Settings {
    /// Whether the settings make a campaign, and if they do not, what is wrong, in the words
    /// of the variables they come from.
    ///
    /// # Errors
    ///
    /// Where the start is no case, the number counted before it is more than the cases before
    /// it, or the cases a run makes before it has its number of them do not fit in a count.
    pub fn check(&self) -> std::result::Result<(), String> {
        if self.start == 0 {
            return Err("DAGEIST_FUZZ_START is 0, and cases are numbered from 1".to_owned());
        }
        let passed_over = self.start - 1;
        if let Some(counted) = self.counted
            && counted > passed_over
        {
            return Err(format!(
                "DAGEIST_FUZZ_COUNTED is {counted}, more than the {passed_over} cases before \
                 DAGEIST_FUZZ_START={}",
                self.start
            ));
        }
        let uncounted = passed_over - self.counted.unwrap_or(passed_over);
        let to_make = u64::from(self.cases) + uncounted;
        if u32::try_from(to_make).is_err() {
            return Err(format!(
                "DAGEIST_FUZZ_START={} and DAGEIST_FUZZ_CASES={} make {to_make} cases, \
                 more than a campaign counts, {}",
                self.start,
                self.cases,
                u32::MAX
            ));
        }
        if self.deadline.is_some_and(|deadline| deadline.is_zero()) {
            return Err("DAGEIST_FUZZ_DEADLINE_MS is 0, and no case ends in no time".to_owned());
        }
        Ok(())
    }

    pub fn new(seed: u64, cases: u32, focus: Option<Focus>, keep: PathBuf) -> Self {
        Self {
            seed,
            cases,
            focus,
            keep,
            reduction: ReductionLimits::new(400, Duration::from_secs(600), 1 << 24, 2)
                .expect("limits of a reduction"),
            deadline: None,
            start: 1,
            counted: None,
        }
    }
}

/// What a disagreement came to: the case, reduced, and what disagreed.
pub struct Finding {
    pub kept: PathBuf,
    pub reduced: Vec<u8>,
    /// How many bytes the case was before it was reduced.
    pub original_bytes: usize,
    pub detail: String,
    /// How the reduction ended.
    pub reduction: ReductionStatus,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "the tables and Polars disagree: {}\nthe smallest case found ({} bytes of {}, reduction {:?}), kept as {}:\n{}",
            self.detail,
            self.reduced.len(),
            self.original_bytes,
            self.reduction,
            self.kept.display(),
            String::from_utf8_lossy(&self.reduced)
        )
    }
}

/// What a campaign came to.
pub struct Report {
    /// How many cases checked something.
    pub evaluated: u64,
    pub findings: Vec<Finding>,
    /// How the cases ended, as lines.
    pub tally: Vec<String>,
}

pub fn detail_of(outcome: &CaseOutcome) -> String {
    format!(
        "{}: {}",
        outcome
            .failure_key()
            .and_then(|key| key.discriminator())
            .unwrap_or("?"),
        outcome.detail().unwrap_or_default()
    )
}

/// The target a campaign hands to the property runner: it makes cases as the
/// target does and evaluates them through the evaluator. A case that fails is
/// reduced and kept here, and the runner is told it was checked, since the runner
/// ends the run at a failure and the next case is to be run whatever this one did.
struct Observing<'a, E> {
    target: &'a AllowedTarget,
    evaluator: &'a E,
    settings: &'a Settings,
    /// How many cases were made.
    cases: Cell<u64>,
    /// How many of them were counted towards the campaign's number of cases.
    counted: Cell<u64>,
    watchdog: Option<&'a Watchdog>,
    tally: RefCell<Tally>,
    findings: RefCell<Vec<Finding>>,
}

impl<E: Evaluator> CaseTarget for Observing<'_, E> {
    type Case = Case;

    fn descriptor(&self) -> &TargetDescriptor {
        self.target.descriptor()
    }

    fn decode(&self, input: &[u8]) -> Result<Case> {
        self.target.decode(input)
    }

    fn encode(&self, case: &Case) -> Result<Vec<u8>> {
        self.target.encode(case)
    }

    fn generate(&self, input: &[u8]) -> Result<Generation<Case>> {
        self.target.generate(input)
    }

    fn evaluate(&self, case: &Case) -> Result<CaseOutcome> {
        let number = self.cases.get() + 1;
        self.cases.set(number);
        if number < self.settings.start {
            // Passed over: the runner is told the case was checked, as it was in the run that
            // is gone on from, so that it makes the cases after it.
            let property = self.target.descriptor().properties()[0].clone();
            return CaseOutcome::evaluated(vec![property]);
        }
        let canonical = self.target.encode(case)?;
        let observation = timed(self.watchdog, number, &canonical, || {
            self.evaluator.observe(&canonical, Duration::MAX)
        })?;
        for (what, count) in observation.tally {
            *self.tally.borrow_mut().entry(what).or_default() += count;
        }
        let failed = observation.outcome.failure_key().cloned();
        let counts = failed.is_some() || observation.outcome.is_evaluated();
        let result = match failed {
            None => Ok(observation.outcome),
            Some(key) => {
                // The disagreement is told of before it is reduced, since a replay in its
                // reduction may be the case that does not end.
                if let Some(watchdog) = self.watchdog {
                    watchdog.reducing(number, &canonical, detail_of(&observation.outcome));
                }
                let finding = keep(
                    self.target,
                    self.evaluator,
                    self.settings,
                    self.watchdog,
                    number,
                    &canonical,
                    &observation.outcome,
                );
                if let Some(watchdog) = self.watchdog {
                    watchdog.reduced();
                }
                finding.and_then(|finding| {
                    if let Some(watchdog) = self.watchdog {
                        watchdog.found(finding.to_string());
                    }
                    self.findings.borrow_mut().push(finding);
                    CaseOutcome::evaluated(vec![key.property().clone()])
                })
            }
        };
        // Counted only when the case is done with, so that a case that does not end while it
        // is reduced is counted once, by the timer, as a case that did not end is.
        if counts {
            self.counted.set(self.counted.get() + 1);
        }
        if let Some(watchdog) = self.watchdog {
            watchdog.counted(self.counted.get());
        }
        result
    }
}

/// Runs `work` as the case `number` written as `canonical`, timed if there is a timer.
fn timed<T>(
    watchdog: Option<&Watchdog>,
    number: u64,
    canonical: &[u8],
    work: impl FnOnce() -> T,
) -> T {
    match watchdog {
        Some(watchdog) => watchdog.timing(number, canonical, work),
        None => work(),
    }
}

/// The reduced case of `report`, and the words for what it showed.
fn reduced_of(
    report: &ReductionReport,
    key: &FailureKey,
    first: &CaseOutcome,
) -> (Vec<u8>, String) {
    let reduced = report
        .canonical_input()
        .unwrap_or_else(|| report.best_input())
        .to_vec();
    let digest = Sha256Digest::from_bytes(&reduced);
    let outcome = report
        .observations()
        .iter()
        .rev()
        .find(|seen| seen.input_digest == digest && seen.outcome.failure_key() == Some(key))
        .map_or(first, |seen| &seen.outcome);
    (reduced, detail_of(outcome))
}

/// Reduces the case written as `canonical`, the `ordinal`th of the campaign, which
/// `evaluator` found to end as `first` did, and keeps what it came to under
/// `settings.keep`.
fn keep<E: Evaluator>(
    target: &AllowedTarget,
    evaluator: &E,
    settings: &Settings,
    watchdog: Option<&Watchdog>,
    ordinal: u64,
    canonical: &[u8],
    first: &CaseOutcome,
) -> Result<Finding> {
    let key = first.failure_key().expect("a failure has its key");
    let report = reduce_with(
        target,
        canonical,
        key,
        settings.reduction,
        &AtomicBool::new(false),
        |input, budget| {
            let observation = timed(watchdog, ordinal, input, || {
                evaluator.observe(input, budget.remaining_time)
            })?;
            Ok(ReductionEvaluation::new(
                observation.outcome,
                observation.output_bytes,
            ))
        },
    )?;
    let (reduced, detail) = reduced_of(&report, key, first);
    let kept = settings.keep.join(format!(
        "{:016x}-{ordinal}-{}.case",
        settings.seed,
        reduced.len()
    ));
    std::fs::write(&kept, &reduced).map_err(|error| {
        FuzzError::with_source(
            dageist_fuzz::FuzzErrorKind::Io,
            "keep a finding",
            "write the reduced case",
            error,
        )
    })?;
    Ok(Finding {
        kept,
        reduced,
        original_bytes: canonical.len(),
        detail,
        reduction: report.status(),
    })
}

/// Runs the cases `settings` names through `evaluator`, every one whatever the
/// ones before it did. A case that fails is reduced and kept, and the next is run.
/// Panics where the run itself cannot be made.
pub fn run<E: Evaluator>(settings: &Settings, evaluator: &E) -> Report {
    let target = AllowedTarget::new().with_focus(settings.focus.clone());
    // A draw reads more than a byte where a count needs more, and again where it rejects what it
    // read, so a case that is made from entropy of up to this many bytes runs out of it about as
    // seldom as one did of 512 when each draw was one byte.
    let strategy = property::bytes(1024, target.descriptor().limits());
    // The cases before `start` are made and counted again as they were, so the runner has that
    // many more to make before it has the number of cases that are left.
    if let Err(refusal) = settings.check() {
        panic!("{refusal}");
    }
    let passed_over = settings.start - 1;
    let counted_before = settings.counted.unwrap_or(passed_over);
    let config = PropertyConfig {
        cases: u32::try_from(u64::from(settings.cases) + (passed_over - counted_before))
            .expect("the settings were checked"),
        max_rejections: 1_000_000,
        // Entropy is not the case: the owner's reduction works on the case.
        max_shrink_iterations: 0,
        seed: settings.seed,
    };
    // The case is written down before it is run, so that one that does not
    // return is still found: it is the file `current.case`.
    std::fs::create_dir_all(&settings.keep).expect("a directory for kept evidence");
    let current = settings.keep.join("current.case");
    let mut capture = |bytes: &[u8]| {
        std::fs::write(&current, bytes).map_err(|error| {
            FuzzError::with_source(
                dageist_fuzz::FuzzErrorKind::Io,
                "capture the case",
                "write the case about to run",
                error,
            )
        })
    };
    let watchdog = settings.deadline.map(|deadline| {
        Watchdog::start(Context {
            seed: settings.seed,
            keep: Some(settings.keep.clone()),
            deadline,
        })
    });
    if let Some(watchdog) = &watchdog {
        watchdog.counted(counted_before);
    }
    let observing = Observing {
        target: &target,
        evaluator,
        settings,
        cases: Cell::new(0),
        counted: Cell::new(counted_before),
        watchdog: watchdog.as_ref(),
        tally: RefCell::new(Tally::new()),
        findings: RefCell::new(Vec::new()),
    };
    let report = run_property(&observing, &strategy, config, &mut capture)
        .unwrap_or_else(|error| panic!("the property could not be run: {error:?}"));
    // The tally is of the run, not of the reduction of what it found. What the thread tallied
    // since its last evaluation is of the cases made after it.
    let mut tally = observing.tally.into_inner();
    for (what, count) in take_counts() {
        *tally.entry(what).or_default() += count;
    }
    Report {
        evaluated: report.counts.evaluated(),
        findings: observing.findings.into_inner(),
        tally: tally_lines(&tally),
    }
}

#[cfg(test)]
mod tests {
    use dageist_fuzz::{FailureClass, PropertyId};

    use super::super::target::ALLOWANCE;
    use super::*;

    /// Evaluates as the target does, except that a case that checked something and whose
    /// text holds `needle` ends as one that missed its deadline: a failure of the class
    /// that a process stopped for taking too long has, which no in-process evaluation can
    /// have.
    struct Misses<'a> {
        evaluation: InProcess<'a>,
        needle: &'a str,
    }

    impl Evaluator for Misses<'_> {
        fn observe(&self, canonical: &[u8], within: Duration) -> Result<Observation> {
            let mut observation = self.evaluation.observe(canonical, within)?;
            // Only a case that checked something can end as a failure, as a property that is
            // violated is, so that the cases a run draws are the same with and without it.
            if observation.outcome.is_evaluated()
                && String::from_utf8_lossy(canonical).contains(self.needle)
            {
                observation.outcome = CaseOutcome::failure(
                    FailureKey::new(
                        PropertyId::new(ALLOWANCE)?,
                        FailureClass::Timeout,
                        Some("case-deadline".to_owned()),
                    )?,
                    "the case did not end in time",
                )?;
            }
            Ok(observation)
        }
    }

    fn settings(name: &str) -> Settings {
        let keep = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&keep);
        let mut settings = Settings::new(
            2_900_100,
            30,
            Some(Focus::parse("abs, sqrt").unwrap().unwrap()),
            keep,
        );
        settings.reduction = ReductionLimits::new(120, Duration::from_secs(120), 1 << 24, 2)
            .expect("limits of a reduction");
        settings
    }

    #[test]
    fn a_campaign_keeps_every_case_that_fails_and_goes_on_to_the_next() {
        let target = AllowedTarget::new();
        let needle = "\"entry\":\"Expr.abs\"";
        let clean = super::super::on_a_large_stack(|| {
            let never = Misses {
                evaluation: InProcess::new(&target),
                needle: "no case holds this",
            };
            run(&settings("campaign-clean"), &never)
        });
        let failing = super::super::on_a_large_stack(|| {
            let misses = Misses {
                evaluation: InProcess::new(&target),
                needle,
            };
            run(&settings("campaign-failing"), &misses)
        });
        assert!(clean.findings.is_empty());
        assert!(
            failing.findings.len() >= 2,
            "at least two of the cases of a campaign focused on abs and sqrt hold abs, and \
             each is kept: {} were",
            failing.findings.len()
        );
        for finding in &failing.findings {
            let reduced = String::from_utf8_lossy(&finding.reduced);
            assert!(reduced.contains(needle), "{reduced}");
            assert!(
                finding.reduced.len() < finding.original_bytes,
                "{} bytes are no reduction of {}",
                finding.reduced.len(),
                finding.original_bytes
            );
            assert_eq!(std::fs::read(&finding.kept).unwrap(), finding.reduced);
            assert!(
                finding.detail.starts_with("case-deadline"),
                "{}",
                finding.detail
            );
        }
        // The cases are the ones the seed makes, whatever failed before them.
        assert_eq!(failing.evaluated, clean.evaluated);
        assert_eq!(failing.tally, clean.tally);
    }

    #[test]
    fn settings_that_make_no_campaign_are_refused_in_the_words_of_their_variables() {
        let settings = |start: u64, counted: Option<u64>, cases: u32| {
            let mut settings = settings("campaign-settings");
            settings.cases = cases;
            settings.start = start;
            settings.counted = counted;
            settings
        };
        assert!(settings(1, None, 5).check().is_ok());
        assert!(settings(10, Some(9), 5).check().is_ok());
        assert!(settings(10, None, 5).check().is_ok());
        let refused = |settings: Settings| settings.check().expect_err("refused");
        assert!(refused(settings(0, None, 5)).contains("DAGEIST_FUZZ_START"));
        assert!(refused(settings(10, Some(10), 5)).contains("DAGEIST_FUZZ_COUNTED"));
        // A start beyond what a count holds, and one that the cases make overflow.
        assert!(
            refused(settings(u64::from(u32::MAX) + 2, Some(0), 5)).contains("DAGEIST_FUZZ_START")
        );
        assert!(refused(settings(u64::from(u32::MAX), Some(0), 5)).contains("DAGEIST_FUZZ_CASES"));
        let mut untimed = settings(1, None, 5);
        untimed.deadline = Some(Duration::ZERO);
        assert!(refused(untimed).contains("DAGEIST_FUZZ_DEADLINE_MS"));
    }
}
