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
use super::observe::{Observed, Refusal, Seen, is_the_targets_own, observe};
use super::predict::{Prediction, Refused, Source, Verdict, predict};
use super::probe::{Settled, same_reason, settle, settle_apart};
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

/// What the outcome of a case says where the tables and Polars both refuse it, and
/// not for the same reason: Polars never judged what the tables did, so the case is
/// no agreement about the refusal, and is counted apart from the agreeing ones.
pub const REFUSED_FOR_ANOTHER_REASON: &str = "refused by both, for another reason";

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
/// totals of cases that agreed, that both refused for another reason than each
/// other's, that a limit excused, that a named rule was stricter in, and that were
/// disagreements.
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
        ("refused by both, for another reason", total("both refuse")),
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
        format!("\nprobe: {}", settled.said())
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
        one_line(&format!("{STRICTER_WHERE_UNEVALUATED}: {}", settled.said())),
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

/// How a refusal of the tables is worded when it is of where an expression may end.
const CANNOT_END: &str = "it cannot end here: ";

/// What Polars says of what a named rule is about, where it refuses the same. The
/// rule that a pivot's aggregate give one value is not Polars': it lets a running
/// value stand, so it has no words, and Polars refusing a case it refuses is never
/// for it.
fn polars_words(rule: &str) -> Option<&'static str> {
    match rule {
        "key-is-elementwise" => Some("elementwise"),
        "element-only-in-pivot-aggregate" => Some("stands only in a pivot's aggregate"),
        "join-key-names-are-not-repeated" => Some("repeated key names"),
        _ => None,
    }
}

/// Whether Polars refused the case for the reason the tables gave: the objection
/// and the call it names, for the tables' own refusals, and what Polars says of
/// what the rule is about, for a rule. What the target says of a schema is neither
/// the tables' nor Polars' reason, and agrees only with the same words.
fn refused_for_the_same_reason(refused: &Refused, refusal: &Refusal) -> bool {
    let tables = refused
        .reason
        .strip_prefix(CANNOT_END)
        .unwrap_or(&refused.reason);
    if is_the_targets_own(tables) || is_the_targets_own(&refusal.reason) {
        return tables == refusal.reason;
    }
    match &refused.source {
        Source::Polars => same_reason(&refused.source, tables, &refusal.reason),
        Source::Rule(rule) => match polars_words(rule) {
            Some(words) => refusal.reason.contains(words),
            None => same_reason(&refused.source, tables, &refusal.reason),
        },
        Source::Silent => false,
    }
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
                        one_line(&format!(
                            "the rule {rule} refuses what Polars allows, as {why}: {}",
                            refused.reason
                        )),
                    );
                }
                (Source::Silent, _) => {
                    tally(format!("tables silent: {}", one_line(&refused.reason)));
                    return CaseOutcome::unsupported(one_line(&format!(
                        "the tables hold nothing for this: {}",
                        refused.reason
                    )));
                }
                (source, false) => {
                    checked.push(allowance);
                    // Both refuse; whether for the same reason decides what the case
                    // is evidence of: Polars judged what the tables did, or did not.
                    if let Some(refusal) = &observed.refusal
                        && !refused_for_the_same_reason(&refused, refusal)
                    {
                        tally(format!(
                            "both refuse, for another reason: the tables {} and Polars {}",
                            match &source {
                                Source::Rule(rule) => format!("by the rule {rule}"),
                                _ => reason_slug(&refused.reason),
                            },
                            reason_slug(&refusal.reason)
                        ));
                        return CaseOutcome::expected_rejection(
                            checked,
                            one_line(&format!(
                                "{REFUSED_FOR_ANOTHER_REASON}: the tables say {}; Polars says {}",
                                refused.reason, refusal.reason
                            )),
                        );
                    }
                    tally(match source {
                        Source::Rule(rule) => format!("agree: refused by the rule {rule}"),
                        _ => "agree: refused".to_owned(),
                    });
                }
            },
            Verdict::Undetermined(why) => {
                tally("undetermined");
                if checked.is_empty() {
                    return CaseOutcome::unsupported(one_line(&format!(
                        "the tables cannot be asked: {why}"
                    )));
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

    /// The outcome of `text` and what was tallied while it was evaluated.
    fn evaluated_and_tallied(text: &'static str) -> (CaseOutcome, Vec<String>) {
        let case = Case::decode(text.as_bytes()).expect("a case");
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || {
                let outcome = AllowedTarget::new().evaluate(&case).expect("an outcome");
                (outcome, take_tally())
            })
            .expect("a thread")
            .join()
            .expect("the case is evaluated")
    }

    fn counted(tally: &[String], what: &str) -> Option<u64> {
        tally
            .iter()
            .find(|line| line.contains(what))
            .and_then(|line| line.split_whitespace().next()?.parse().ok())
    }

    /// `f.dt.date()` as a filter: the tables refuse the date part of a `Boolean`,
    /// and Polars refuses the filter, for a predicate that is a `Date`: it never
    /// judged what the tables did.
    const FILTER_OF_A_DATE_PART_OF_A_BOOLEAN: &str = r#"{"a":"String","b":"Int32","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.filter","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[{"entry":"Expr.dt.date","arguments":{}}]}}]}}}}]}"#;

    /// A running sum of a null: refused by the tables and by Polars for it.
    const RUNNING_SUM_OF_A_NULL: &str = r#"{"a":"Int16","b":"Null","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":null}}},"then":[{"entry":"Expr.cum_sum","arguments":{}}]}}]}}}}]}"#;

    /// A filter of the whole number 1: the rule that a predicate be a `Boolean`
    /// refuses it, and Polars does for the same reason.
    const FILTER_OF_A_NUMBER: &str = r#"{"a":"Date","b":"Int8","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.filter","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":1}}},"then":[]}}]}}}}]}"#;

    /// A join on `k == true.min()`: the rule that a key be elementwise refuses it,
    /// and Polars refuses the join for a column that the other table lacks.
    const JOIN_KEY_OF_A_COLUMN_THE_OTHER_TABLE_LACKS: &str = r#"{"a":"UInt16","b":"Struct(x: Int64, y: String)","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.join","arguments":{"other":{"table":"other"},"on":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[{"entry":"Expr.eq","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":true}}},"then":[{"entry":"Expr.min","arguments":{}}]}}}}]}}]}}}}]}"#;

    #[test]
    fn a_refusal_by_both_for_different_reasons_is_counted_apart_from_agreement() {
        // The tables refuse the date part of a `Boolean` and Polars refuses the
        // filter for its predicate: both refuse, but Polars never judged what the
        // tables did, so the case is no agreement about that refusal.
        for text in [
            FILTER_OF_A_DATE_PART_OF_A_BOOLEAN,
            JOIN_KEY_OF_A_COLUMN_THE_OTHER_TABLE_LACKS,
        ] {
            let (outcome, tally) = evaluated_and_tallied(text);
            assert_eq!(
                outcome.kind(),
                dageist_fuzz::OutcomeKind::ExpectedRejection,
                "{text}"
            );
            assert!(
                outcome
                    .detail()
                    .is_some_and(|detail| detail.starts_with("refused by both, for another reason")),
                "{:?}",
                outcome.detail()
            );
            assert_eq!(
                counted(&tally, "cases refused by both, for another reason"),
                Some(1),
                "{tally:?}"
            );
            assert_eq!(counted(&tally, "cases agreeing"), Some(0), "{tally:?}");
        }
    }

    /// A refusal by `rule`, and the refusal of Polars, as the target meets them.
    fn same_reason_for_rule(rule: &'static str, polars: &str) -> bool {
        use super::super::observe::Stage;
        use dageist_engine::document::{Place, StepId};
        let place = Place::of_step(0, Some(StepId::new("s").expect("an identifier")));
        let refused = Refused {
            source: Source::Rule(rule),
            at: place.to_string(),
            place: place.clone(),
            stage: None,
            reason: "it cannot end here: the rule's own words".to_owned(),
            argument_unknown: false,
        };
        let refusal = Refusal {
            stage: Stage::Schema,
            at: place.to_string(),
            place: Some(place),
            reason: polars.to_owned(),
        };
        refused_for_the_same_reason(&refused, &refusal)
    }

    #[test]
    fn what_polars_says_of_the_element_rule_is_either_of_its_two_refusals() {
        // The engine says where `element` stands only, and Polars says it of an
        // `element` that has no pivot to be in (polars-error: "`element` is not
        // allowed in this context").
        let rule = "element-only-in-pivot-aggregate";
        for polars in [
            "`element` stands only in a pivot's aggregate",
            "`element` is not allowed in this context",
        ] {
            assert!(same_reason_for_rule(rule, polars), "{polars}");
        }
        assert!(!same_reason_for_rule(
            rule,
            "cannot compare string with numeric type (i64)"
        ));
    }

    #[test]
    fn what_polars_says_of_the_other_rules_is_what_each_is_about() {
        for (rule, said) in [
            (
                "key-is-elementwise",
                "all join key expressions must be elementwise.",
            ),
            (
                "join-key-names-are-not-repeated",
                "joining with repeated key names; already joined on u and u",
            ),
            (
                "filter-predicate-is-boolean",
                "filter predicate must be of type `Boolean`, got `Int64`",
            ),
            (
                "condition-predicate-is-boolean",
                "invalid series dtype: expected `Boolean`, got `date` for series with name `b`",
            ),
        ] {
            assert!(same_reason_for_rule(rule, said), "{rule}");
            assert!(
                !same_reason_for_rule(rule, "`element` is not allowed in this context"),
                "{rule}"
            );
        }
        // Polars lets a running value stand in a pivot's aggregate, so it has no
        // words of its own to refuse it by: whatever it says, it is not the rule's.
        for polars in [
            "`element` stands only in a pivot's aggregate",
            "`element` is not allowed in this context",
            "all join key expressions must be elementwise.",
        ] {
            assert!(
                !same_reason_for_rule("pivot-aggregate-gives-one-value", polars),
                "{polars}"
            );
        }
    }

    #[test]
    fn a_refusal_by_both_for_the_same_reason_agrees() {
        // The tables and Polars refuse for the objection and the call that they
        // name, or for the rule's own words, whatever the dtypes and names quoted.
        for text in [RUNNING_SUM_OF_A_NULL, FILTER_OF_A_NUMBER] {
            let (outcome, tally) = evaluated_and_tallied(text);
            assert_eq!(
                outcome.kind(),
                dageist_fuzz::OutcomeKind::Evaluated,
                "{text}"
            );
            assert_eq!(counted(&tally, "cases agreeing"), Some(1), "{tally:?}");
            assert_eq!(
                counted(&tally, "cases refused by both, for another reason").unwrap_or(0),
                0,
                "{tally:?}"
            );
        }
    }

    #[test]
    fn a_probe_longer_than_an_outcome_may_say_is_still_an_outcome() {
        // The outcome of a case that the probe settles names the probe, which for a
        // deep case is longer than the fuzz contract admits of a detail. The case
        // is still one where the tables are stricter, and its outcome is a line.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "tests/allowed_fuzz/regressions/stricter-a-probe-longer-than-an-outcome-may-say.case",
        );
        let case = Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case");
        let outcome = std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || AllowedTarget::new().evaluate(&case))
            .expect("a thread")
            .join()
            .expect("the case is evaluated")
            .expect("an outcome that meets the fuzz contract");
        assert_eq!(outcome.kind(), dageist_fuzz::OutcomeKind::ExpectedRejection);
        let detail = outcome.detail().expect("a detail");
        assert!(detail.starts_with(STRICTER_WHERE_UNEVALUATED), "{detail}");
        assert!(detail.len() <= 4096 && !detail.chars().any(char::is_control));
    }

    #[test]
    fn the_outcome_of_a_schema_mismatch_probe_says_what_the_probe_showed() {
        // The tables' reason is the target's own comparison of plan and rows, and
        // the probe in the position is refused by Polars with a panic that says
        // nothing of it, so the outcome says what was shown: the call's plan and
        // rows differ as the tables say, and a call whose plan and rows agree is
        // allowed there.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/allowed_fuzz/regressions/stricter-a-sort-key-of-one-value-whose-schema-differs.case");
        let case = Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case");
        let outcome = std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || AllowedTarget::new().evaluate(&case))
            .expect("a thread")
            .join()
            .expect("the case is evaluated")
            .expect("an outcome");
        let detail = outcome.detail().expect("a detail");
        assert!(detail.contains("plan and rows"), "{detail}");
        assert!(!detail.contains("for the tables' reason"), "{detail}");
    }

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
