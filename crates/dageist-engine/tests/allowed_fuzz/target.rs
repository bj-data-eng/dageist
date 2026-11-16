//! The case target: generating, the codec, the comparison of what the tables
//! predict with what Polars does, and the smaller cases reduction tries.

use std::collections::BTreeMap;
use std::sync::Mutex;

use dageist_fuzz::registry::FuzzManifest;
use dageist_fuzz::{
    CaseOutcome, CaseTarget, FailureClass, FailureKey, FuzzError, FuzzErrorKind, Generation,
    GenerationRejection, PropertyId, Result, TargetDescriptor,
};

use super::case::Case;
use super::generate::generate;
use super::limits::excuse;
use super::observe::{Observed, Seen, observe};
use super::predict::{Prediction, Source, Verdict, predict};
use super::reduce::candidates;

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

/// How many cases ended each way, and why not when they did not check one.
static TALLY: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());

fn tally(what: impl Into<String>) {
    *TALLY
        .lock()
        .expect("the tally is not poisoned")
        .entry(what.into())
        .or_default() += 1;
}

/// What was tallied since the last call, one line each.
pub fn take_tally() -> Vec<String> {
    std::mem::take(&mut *TALLY.lock().expect("the tally is not poisoned"))
        .into_iter()
        .map(|(what, count)| format!("{count:>6}  {what}"))
        .collect()
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
            excuse(case, &prediction, &observed).map(|limit| {
                tally(format!("not covered ({what}): {}", limit.statement()));
                CaseOutcome::expected_rejection(Vec::new(), limit.statement())
            })
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
            Verdict::Refuses(refused) => match (refused.source, allowed_by_polars) {
                (Source::Polars, true) => {
                    if let Some(outcome) = excused("refused, Polars allows") {
                        return outcome;
                    }
                    let why = reason_slug(&refused.reason);
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
                    // Polars. It is a result of the target's own, not a
                    // property of the tables.
                    tally(format!("rule stricter than Polars: {rule}"));
                    return CaseOutcome::expected_rejection(
                        checked,
                        format!(
                            "the rule {rule} refuses what Polars allows: {}",
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
