//! A campaign: the cases a seed makes, each evaluated by an [`Evaluator`], and
//! every disagreement reduced and kept.
//!
//! Where a case is evaluated is the evaluator's: [`InProcess`] runs it on the
//! thread that asks, which is what the fixed seed of the engine's tests does.

use std::cell::RefCell;
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
}

impl Settings {
    pub fn new(seed: u64, cases: u32, focus: Option<Focus>, keep: PathBuf) -> Self {
        Self {
            seed,
            cases,
            focus,
            keep,
            reduction: ReductionLimits::new(400, Duration::from_secs(600), 1 << 24, 2)
                .expect("limits of a reduction"),
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
/// target does and evaluates them through the evaluator.
struct Observing<'a, E> {
    target: &'a AllowedTarget,
    evaluator: &'a E,
    tally: RefCell<Tally>,
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
        let canonical = self.target.encode(case)?;
        let observation = self.evaluator.observe(&canonical, Duration::MAX)?;
        for (what, count) in observation.tally {
            *self.tally.borrow_mut().entry(what).or_default() += count;
        }
        Ok(observation.outcome)
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

/// Reduces the case written as `canonical`, which `evaluator` found to end as
/// `first` did, and keeps what it came to under `settings.keep`.
fn keep<E: Evaluator>(
    target: &AllowedTarget,
    evaluator: &E,
    settings: &Settings,
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
            let observation = evaluator.observe(input, budget.remaining_time)?;
            Ok(ReductionEvaluation::new(
                observation.outcome,
                observation.output_bytes,
            ))
        },
    )?;
    let (reduced, detail) = reduced_of(&report, key, first);
    let kept = settings
        .keep
        .join(format!("{:016x}-{}.case", settings.seed, reduced.len()));
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

/// Runs the cases `settings` names through `evaluator`. When one disagrees,
/// reduces it and keeps the result. Panics where the run itself cannot be made.
pub fn run<E: Evaluator>(settings: &Settings, evaluator: &E) -> Report {
    let target = AllowedTarget::new().with_focus(settings.focus.clone());
    // A draw reads more than a byte where a count needs more, and again where it rejects what it
    // read, so a case that is made from entropy of up to this many bytes runs out of it about as
    // seldom as one did of 512 when each draw was one byte.
    let strategy = property::bytes(1024, target.descriptor().limits());
    let config = PropertyConfig {
        cases: settings.cases,
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
    let observing = Observing {
        target: &target,
        evaluator,
        tally: RefCell::new(Tally::new()),
    };
    let report = run_property(&observing, &strategy, config, &mut capture)
        .unwrap_or_else(|error| panic!("the property could not be run: {error:?}"));
    // The tally is of the run, not of the reduction of what it found. What the thread tallied
    // since its last evaluation is of the cases made after it.
    let mut tally = observing.tally.into_inner();
    for (what, count) in take_counts() {
        *tally.entry(what).or_default() += count;
    }
    let findings = report
        .failure
        .map(|found| {
            keep(
                &target,
                evaluator,
                settings,
                &found.canonical,
                &found.outcome,
            )
            .unwrap_or_else(|error| panic!("the finding could not be kept: {error:?}"))
        })
        .into_iter()
        .collect();
    Report {
        evaluated: report.counts.evaluated(),
        findings,
        tally: tally_lines(&tally),
    }
}

#[cfg(test)]
mod tests {
    use dageist_fuzz::{FailureClass, PropertyId};

    use super::super::target::ALLOWANCE;
    use super::*;

    /// Evaluates as the target does, except that a case whose text holds `needle` ends as
    /// one that missed its deadline: a failure of the class that a process stopped for
    /// taking too long has, which no in-process evaluation can have.
    struct Misses<'a> {
        evaluation: InProcess<'a>,
        needle: &'a str,
    }

    impl Evaluator for Misses<'_> {
        fn observe(&self, canonical: &[u8], within: Duration) -> Result<Observation> {
            let mut observation = self.evaluation.observe(canonical, within)?;
            if String::from_utf8_lossy(canonical).contains(self.needle) {
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
}
