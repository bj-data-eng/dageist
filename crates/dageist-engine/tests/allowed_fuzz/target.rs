//! The case target: generating, the codec, the comparison of what the tables
//! predict with what Polars does, and the smaller cases reduction tries.

use std::cell::RefCell;
use std::collections::BTreeMap;

use dageist_fuzz::registry::FuzzManifest;
use dageist_fuzz::{
    CaseOutcome, CaseTarget, FailureClass, FailureKey, FuzzError, FuzzErrorKind, Generation,
    GenerationRejection, PropertyId, Result, TargetDescriptor,
};

use super::case::Case;
use super::generate::generate;
use super::limits::{excuses, strict};
use super::observe::{Observed, Seen, observe};
use super::predict::{Prediction, Refused, Source, Verdict, predict};
use super::probe::{Settled, settle, settle_apart};
use super::reduce::candidates;

/// The named rules that are stricter than Polars by design, each with why. A
/// rule that refuses what Polars allows and is not here is a disagreement.
const STRICTER_BY_DESIGN: [(&str, &str); 1] = [(
    "pivot-aggregate-gives-one-value",
    "a pivot's aggregate gives one value for each group, though Polars lets a running value stand",
)];

/// What the outcome of a case says where the tables refuse a part that Polars
/// does not evaluate, and a probe of the part shows the tables right.
pub const STRICTER_WHERE_UNEVALUATED: &str = "stricter where Polars does not evaluate the part";

pub const TARGET: &str = "dageist-engine::allowed-combinations";
pub const ALLOWANCE: &str = "dageist-engine::tables-predict-allowance";
pub const DTYPES: &str = "dageist-engine::tables-predict-dtypes";

pub struct AllowedTarget {
    descriptor: TargetDescriptor,
}

impl Default for AllowedTarget {
    fn default() -> Self {
        Self::new()
    }
}

impl AllowedTarget {
    pub fn new() -> Self {
        let manifest: FuzzManifest = toml::from_str(include_str!("descriptor.toml"))
            .expect("the descriptor of the target is valid");
        let descriptor = manifest
            .targets
            .into_iter()
            .find(|target| target.id().as_str() == TARGET)
            .expect("the descriptor declares the target");
        Self { descriptor }
    }

    fn property(name: &str) -> PropertyId {
        PropertyId::new(name).expect("a declared property")
    }
}

thread_local! {
    /// How many cases ended each way, and why not when they did not check one,
    /// on the thread that evaluates them, so that tests that run at once do not
    /// count each other's.
    static TALLY: RefCell<BTreeMap<String, u64>> = const { RefCell::new(BTreeMap::new()) };
}

fn tally(what: impl Into<String>) {
    TALLY.with(|tally| *tally.borrow_mut().entry(what.into()).or_default() += 1);
}

/// What was tallied on this thread since the last call, one line each, and the
/// totals of cases that agreed, that a limit excused, that a named rule was
/// stricter in, and that were disagreements.
pub fn take_tally() -> Vec<String> {
    let tallied = TALLY.with(|tally| std::mem::take(&mut *tally.borrow_mut()));
    let total = |prefix: &str| -> u64 {
        tallied
            .iter()
            .filter(|(what, _)| what.starts_with(prefix))
            .map(|(_, count)| count)
            .sum()
    };
    let totals = [
        ("agreeing", total("agree:")),
        ("excused by a limit", total("not covered")),
        (
            "refused where Polars does not evaluate the part",
            total("stricter where"),
        ),
        (
            "refused by a rule that is stricter than Polars",
            total("rule stricter"),
        ),
        ("disagreeing", total("disagreement:")),
    ];
    let mut lines: Vec<String> = tallied
        .iter()
        .map(|(what, count)| format!("{count:>6}  {what}"))
        .collect();
    lines.push("total:".to_owned());
    lines.extend(
        totals
            .iter()
            .map(|(what, count)| format!("{count:>6}  cases {what}")),
    );
    lines
}

/// What the tables predicted of the case, what Polars did, and which limits
/// excuse the difference, in words.
pub fn describe(case: &Case) -> String {
    let prediction = predict(case);
    let observed = observe(case);
    let predicted = match &prediction.verdict {
        Verdict::Allows => "the tables allow it".to_owned(),
        Verdict::Refuses(refused) => format!(
            "the tables refuse it at {} ({}): {}",
            refused.at,
            match (&refused.source, refused.stage) {
                (Source::Polars, Some(stage)) => format!("Polars, {stage:?}"),
                (Source::Polars, None) => "Polars".to_owned(),
                (Source::Rule(rule), _) => format!("the rule {rule}"),
                (Source::Silent, _) => "the tables are silent".to_owned(),
            },
            refused.reason
        ),
        Verdict::Undetermined(why) => format!("the tables cannot say: {why}"),
    };
    let done = observed.refusal.as_ref().map_or_else(
        || "Polars allows it".to_owned(),
        |refusal| {
            format!(
                "Polars refuses it at {} ({}): {}",
                refusal.at,
                refusal.stage.name(),
                refusal.reason
            )
        },
    );
    let excused = excuses(case, &prediction, &observed);
    // The order the case is judged in: the part as written, then the limits, then
    // the refused call apart from what surrounds it.
    let probed = match (&prediction.verdict, &observed.refusal) {
        (Verdict::Refuses(refused), None) => probe_of(case, refused).or_else(|| {
            excused
                .is_empty()
                .then(|| probe_apart(case, refused, &prediction, &observed))
                .flatten()
        }),
        _ => None,
    }
    .map_or_else(String::new, |settled| {
        format!(
            "\nprobe: Polars refuses {} for the tables' reason: {}",
            settled.probe, settled.reason
        )
    });
    let limits: Vec<&str> = excused.iter().map(|limit| limit.slug()).collect();
    format!(
        "{predicted}\n{done}{probed}\nexcused by: {}",
        if limits.is_empty() {
            "no limit".to_owned()
        } else {
            limits.join(", ")
        }
    )
}

/// What a probe of the part the tables refuse, as it is written, shows, unless
/// the limits are held to everything.
fn probe_of(case: &Case, refused: &Refused) -> Option<Settled> {
    if strict() {
        return None;
    }
    settle(case, refused)
}

/// What a probe of the refused call apart from what surrounds it shows, unless
/// the limits are held to everything.
fn probe_apart(
    case: &Case,
    refused: &Refused,
    prediction: &Prediction,
    observed: &Observed,
) -> Option<Settled> {
    if strict() {
        return None;
    }
    settle_apart(case, refused, prediction, observed)
}

/// The outcome of a case where the tables refuse a part Polars does not
/// evaluate, and the probe `settled` of the part shows the tables right.
fn stricter(settled: Option<Settled>, what: &str) -> Option<Result<CaseOutcome>> {
    let settled = settled?;
    tally(format!("{STRICTER_WHERE_UNEVALUATED}: {what}"));
    Some(CaseOutcome::expected_rejection(
        Vec::new(),
        format!(
            "{STRICTER_WHERE_UNEVALUATED}: the probe {} is refused for the tables' reason: {}",
            settled.probe, settled.reason
        ),
    ))
}

/// The first words of a reason, as a name: the same disagreement is not
/// another one because a dtype or a number in its reason is.
fn reason_slug(reason: &str) -> String {
    let words: Vec<String> = reason
        .split(|character: char| !character.is_ascii_alphabetic())
        .filter(|word| word.len() > 1)
        .take(4)
        .map(str::to_ascii_lowercase)
        .collect();
    words.join("-")
}

fn failure(property: &str, discriminator: &str, detail: String) -> Result<CaseOutcome> {
    tally(format!("disagreement: {discriminator}"));
    CaseOutcome::failure(
        FailureKey::new(
            AllowedTarget::property(property),
            FailureClass::PropertyViolation,
            Some(discriminator.to_owned()),
        )?,
        one_line(&detail),
    )
}

fn one_line(text: &str) -> String {
    let line: String = text
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    line.chars().take(3000).collect()
}

/// What the tables predict of the dtypes, compared with what Polars says: the
/// first difference, if there is one.
fn dtype_difference(case: &Case, prediction: &Prediction, observed: &Observed) -> Option<String> {
    // The runner works out the dtype of a prefix of an expression in a plain
    // `select`, where `element` stands nowhere: what it says of a pivot's
    // aggregate that holds `element` in a nested expression is not Polars'
    // verdict on the pivot.
    // A refusal predicted anywhere explains a prefix that Polars refuses: the
    // case is refused, and the prefix is one place where it is.
    let refused_anyway = matches!(prediction.verdict, Verdict::Refuses(_));
    let pivot = case.document.steps()[0].call().entry().id().as_str() == "LazyFrame.pivot";
    for expected in &prediction.expressions {
        let Some(seen) = observed
            .expressions
            .iter()
            .find(|seen| seen.place == expected.place)
        else {
            continue;
        };
        let compare = |what: String,
                       predicted: Option<dageist_engine::allowed::Representative>,
                       found: &Seen| {
            let predicted = predicted?;
            match found {
                Seen::Dtype(_, name, Some(class)) if *class != predicted => Some(format!(
                    "{} {what}: the tables give {}, Polars gives {name}",
                    expected.place,
                    predicted.name()
                )),
                Seen::Refused(reason) if !pivot && !refused_anyway => Some(format!(
                    "{} {what}: the tables allow it, Polars refuses it: {reason}",
                    expected.place
                )),
                _ => None,
            }
        };
        if let Some(difference) = compare("at the start".to_owned(), expected.start, &seen.start) {
            return Some(difference);
        }
        for (at, (predicted, found)) in expected.links.iter().zip(&seen.links).enumerate() {
            if let Some(difference) = compare(format!("after link {}", at + 1), *predicted, found) {
                return Some(difference);
            }
        }
    }
    None
}

impl CaseTarget for AllowedTarget {
    type Case = Case;

    fn descriptor(&self) -> &TargetDescriptor {
        &self.descriptor
    }

    fn decode(&self, input: &[u8]) -> Result<Case> {
        if input.len() as u64 > self.descriptor.limits().bytes() {
            return Err(FuzzError::new(
                FuzzErrorKind::InvalidCase,
                "allowed combinations case",
                "the case is larger than the target admits",
            ));
        }
        Case::decode(input)
    }

    fn encode(&self, case: &Case) -> Result<Vec<u8>> {
        Ok(case.encode())
    }

    fn generate(&self, input: &[u8]) -> Result<Generation<Case>> {
        match generate(input) {
            Some(case) => Ok(Generation::Generated(case)),
            None => Ok(Generation::Rejected(GenerationRejection::new(
                "too little entropy to make a case from",
            )?)),
        }
    }

    fn evaluate(&self, case: &Case) -> Result<CaseOutcome> {
        let prediction = predict(case);
        let observed = observe(case);
        let allowed_by_polars = observed.refusal.is_none();
        if case.document.steps()[0].call().entry().id().as_str() == "LazyFrame.pivot" {
            tally("pivot case");
        }
        let allowance = Self::property(ALLOWANCE);
        let dtypes = Self::property(DTYPES);
        let seen = || {
            observed.refusal.as_ref().map_or_else(
                || "Polars allows it".to_owned(),
                |refusal| {
                    format!(
                        "Polars refuses it at {} ({}): {}",
                        refusal.at,
                        refusal.stage.name(),
                        refusal.reason
                    )
                },
            )
        };
        let excused = |what: &str| -> Option<Result<CaseOutcome>> {
            let limits = excuses(case, &prediction, &observed);
            if limits.is_empty() {
                return None;
            }
            let statements: Vec<&str> = limits.iter().map(|limit| limit.statement()).collect();
            let slugs: Vec<&str> = limits.iter().map(|limit| limit.slug()).collect();
            tally(format!("not covered ({what}): {}", slugs.join(" and ")));
            Some(CaseOutcome::expected_rejection(
                Vec::new(),
                statements.join("; "),
            ))
        };
        // What the tables say of the dtypes is checked whatever the verdict.
        let dtype_checked = prediction
            .expressions
            .iter()
            .any(|expected| expected.start.is_some() || expected.links.iter().any(Option::is_some));
        if let Some(difference) = dtype_difference(case, &prediction, &observed) {
            let text = match &prediction.verdict {
                Verdict::Allows => "allows".to_owned(),
                Verdict::Refuses(refused) => format!("refuses ({})", refused.reason),
                Verdict::Undetermined(why) => format!("cannot say ({why})"),
            };
            return failure(
                DTYPES,
                "dtype-differs",
                format!("{difference}; as a whole the tables say {text}; {}", seen()),
            );
        }
        let mut checked = Vec::new();
        if dtype_checked {
            checked.push(dtypes);
        }
        match prediction.verdict.clone() {
            Verdict::Allows => {
                if !allowed_by_polars {
                    if let Some(outcome) = excused("allowed, Polars refuses") {
                        return outcome;
                    }
                    let why = observed
                        .refusal
                        .as_ref()
                        .map_or_else(String::new, |refusal| reason_slug(&refusal.reason));
                    return failure(
                        ALLOWANCE,
                        &format!("allowed-but-refused-{why}"),
                        format!("the tables allow it; {}", seen()),
                    );
                }
                checked.push(allowance);
                tally("agree: allowed");
            }
            Verdict::Refuses(refused) => match (refused.source.clone(), allowed_by_polars) {
                (Source::Polars, true) => {
                    let why = reason_slug(&refused.reason);
                    // What Polars refuses of the part as written shows the tables
                    // right where it skips the part; the limits then say what it
                    // evaluates and accepts; what is left is asked of the refused
                    // call apart from what surrounds it.
                    if let Some(outcome) = stricter(probe_of(case, &refused), &why) {
                        return outcome;
                    }
                    if let Some(outcome) = excused("refused, Polars allows") {
                        return outcome;
                    }
                    if let Some(outcome) =
                        stricter(probe_apart(case, &refused, &prediction, &observed), &why)
                    {
                        return outcome;
                    }
                    return failure(
                        ALLOWANCE,
                        &format!("refused-but-allowed-{why}"),
                        format!(
                            "the tables refuse it at {}: {}; Polars allows it",
                            refused.at, refused.reason
                        ),
                    );
                }
                (Source::Rule(rule), true) => {
                    // A named rule is dageist's, and may be stricter than
                    // Polars only where it is so by design, which is named.
                    // Any other rule refusing what Polars allows is a
                    // disagreement.
                    let Some((_, why)) = STRICTER_BY_DESIGN.iter().find(|(name, _)| *name == rule)
                    else {
                        // Polars does not evaluate everything: a probe of the part
                        // says whether the rule is right where it does.
                        if let Some(outcome) = stricter(probe_of(case, &refused), rule) {
                            return outcome;
                        }
                        return failure(
                            ALLOWANCE,
                            &format!("rule-refuses-what-polars-allows-{rule}"),
                            format!(
                                "the rule {rule} refuses it at {}: {}; Polars allows it",
                                refused.at, refused.reason
                            ),
                        );
                    };
                    tally(format!("rule stricter than Polars: {rule}"));
                    return CaseOutcome::expected_rejection(
                        checked,
                        format!(
                            "the rule {rule} refuses what Polars allows, as {why}: {}",
                            refused.reason
                        ),
                    );
                }
                (Source::Silent, _) => {
                    tally(format!("tables silent: {}", one_line(&refused.reason)));
                    return CaseOutcome::unsupported(format!(
                        "the tables hold nothing for this: {}",
                        refused.reason
                    ));
                }
                (source, false) => {
                    checked.push(allowance);
                    tally(match source {
                        Source::Rule(rule) => format!("agree: refused by the rule {rule}"),
                        _ => "agree: refused".to_owned(),
                    });
                }
            },
            Verdict::Undetermined(why) => {
                tally("undetermined");
                if checked.is_empty() {
                    return CaseOutcome::unsupported(format!("the tables cannot be asked: {why}"));
                }
            }
        }
        CaseOutcome::evaluated(checked)
    }

    fn reduction_candidates(&self, case: &Case, emit: &mut dyn FnMut(Case) -> bool) -> Result<()> {
        for candidate in candidates(case) {
            if !emit(candidate) {
                break;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use dageist_engine::allowed::Representative;

    use super::*;

    #[test]
    fn a_wrong_prediction_of_the_dtype_before_the_refused_call_is_a_disagreement() {
        // A probe stands a value in for what reaches the refused call, so a
        // wrong prediction of that dtype must not be hidden by it: it is found
        // as a difference of the dtypes of the case itself, whatever is probed.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "tests/allowed_fuzz/regressions/stricter-a-window-over-the-length-of-a-literal.case",
        );
        let case = Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case");
        let observed = observe(&case);
        let right = predict(&case);
        assert_eq!(dtype_difference(&case, &right, &observed), None);
        let mut wrong = right.clone();
        for traced in &mut wrong.expressions {
            for held in &mut traced.links {
                // The comparison gives a `Boolean`; say it gives whole numbers.
                if *held == Some(Representative::Boolean) {
                    *held = Some(Representative::Int64);
                }
            }
        }
        let difference = dtype_difference(&case, &wrong, &observed).expect("a difference");
        assert!(difference.contains("Polars gives"), "{difference}");
    }
}
