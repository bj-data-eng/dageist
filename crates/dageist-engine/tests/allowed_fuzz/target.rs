//! The case target: generating, the codec, the comparison of what the tables
//! predict with what Polars does, and the smaller cases reduction tries.

use std::cell::RefCell;
use std::collections::BTreeMap;

use dageist_engine::document::Place;
use dageist_fuzz::registry::FuzzManifest;
use dageist_fuzz::{
    CaseOutcome, CaseTarget, FailureClass, FailureKey, FuzzError, FuzzErrorKind, Generation,
    GenerationRejection, PropertyId, Result, TargetDescriptor,
};

use super::case::Case;
use super::counts;
use super::generate::{Focus, generate};
use super::limits::{excuses, strict};
use super::observe::{
    Observed, Refusal, Seen, Stage, is_the_targets_own, observe, observe_checked,
};
use super::predict::{Prediction, Refused, Source, Verdict, predict};
use super::probe::{Settled, is_a_predicate_rule, same_reason, settle, settle_apart};
use super::reduce::candidates;

/// The named rules that are stricter than Polars by design, each with why. A
/// rule that refuses what Polars allows and is not here is a disagreement.
const STRICTER_BY_DESIGN: [(&str, &str); 4] = [
    (
        "pivot-aggregate-gives-one-value",
        "a pivot's aggregate gives one value for each group, though Polars lets a running value stand",
    ),
    (
        "arrays-meet-at-one-width",
        "arrays that meet are of one width that is known, though Polars may not evaluate the branch that holds them",
    ),
    (
        "element-dtype-is-measured",
        "a method that depends on the element is not run on one the tables do not measure, which Polars may panic on",
    ),
    (
        "element-dtype-is-implemented",
        "a method that Polars panics on for an element is not run on it, though Polars may not evaluate it",
    ),
];

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
pub const ASKER: &str = "dageist-engine::asker-agrees-with-engine";

pub struct AllowedTarget {
    descriptor: TargetDescriptor,
    /// The methods the cases it makes are focused on, if any.
    focus: Option<Focus>,
}

impl Default for AllowedTarget {
    fn default() -> Self {
        Self::new()
    }
}

impl AllowedTarget {
    pub fn new() -> Self {
        Self::described_by(include_str!("descriptor.toml"))
    }

    /// The target of the descriptor `text` writes, which is `descriptor.toml` for the target a
    /// campaign runs and another where a test needs a ceiling or a bound of its own.
    pub fn described_by(text: &str) -> Self {
        let manifest: FuzzManifest =
            toml::from_str(text).expect("the descriptor of the target is valid");
        let descriptor = manifest
            .targets
            .into_iter()
            .find(|target| target.id().as_str() == TARGET)
            .expect("the descriptor declares the target");
        Self {
            descriptor,
            focus: None,
        }
    }

    /// The target that makes cases focused on `focus`, if it is one.
    pub fn with_focus(self, focus: Option<Focus>) -> Self {
        Self { focus, ..self }
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

/// How many cases ended each way, and why not when they did not check one.
pub type Tally = BTreeMap<String, u64>;

/// What was tallied on this thread since the last call, which a campaign adds to what
/// the cases it ran in other processes tallied.
pub fn take_counts() -> Tally {
    TALLY.with(|tally| std::mem::take(&mut *tally.borrow_mut()))
}

/// What was tallied on this thread since the last call, one line each, and the
/// totals of cases that agreed, that both refused for another reason than each
/// other's, that a limit excused, that a named rule was stricter in, and that were
/// disagreements.
pub fn take_tally() -> Vec<String> {
    tally_lines(&take_counts())
}

/// `tallied` as lines: one for each way a case ended, then the totals.
pub fn tally_lines(tallied: &Tally) -> Vec<String> {
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
        (
            "refused where a count is the rows by chance",
            total("stricter by design"),
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

/// The outcome of a case where the tables refuse a count that Polars allows because the count is
/// the table's rows, or one value, by chance, which the case on more rows shows.
fn by_design(case: &Case, refused: &Refused, what: &str) -> Option<Result<CaseOutcome>> {
    if strict() {
        return None;
    }
    let settled = counts::settle(case, refused)?;
    tally(format!("{}: {what}", counts::STRICTER_BY_DESIGN));
    Some(CaseOutcome::expected_rejection(
        Vec::new(),
        one_line(&settled.said()),
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

/// Whether Polars refused at a call that comes after the one the tables could not be asked of, in
/// the same chain: `why` names the place of that call before its colon, and `refused` is the place
/// of Polars' refusal, which differ only by the number of the link. The dtype before the call that
/// the tables were not told is not known, so what follows it is not asked either, and Polars may
/// refuse one of those calls where the tables say nothing.
fn is_a_later_call_of_the_chain(why: &str, refused: &str) -> bool {
    let link_of = |place: &str| -> Option<(String, usize)> {
        let (base, number) = place.rsplit_once(", link ")?;
        Some((base.to_owned(), number.parse().ok()?))
    };
    let Some((place, _)) = why.split_once(": ") else {
        return false;
    };
    match (link_of(place), link_of(refused)) {
        (Some((base, blind)), Some((other, refused))) => base == other && refused >= blind,
        _ => false,
    }
}

/// What the tables predict of the dtypes, compared with what Polars says: the
/// first difference, if there is one.
fn dtype_difference(case: &Case, prediction: &Prediction, observed: &Observed) -> Option<String> {
    // The runner works out the dtype of a prefix of an expression in a plain
    // `select`, where `element` stands nowhere: what it says of a pivot's
    // aggregate that holds `element` in a nested expression is not Polars'
    // verdict on the pivot.
    // A refusal predicted anywhere explains a prefix that Polars refuses: the
    // case is refused, and the prefix is one place where it is. So does a refusal
    // that Polars gives when it works out the schema at the very call the tables
    // said they could not be asked about, for the expressions that hold the call:
    // such an expression has the dtype its own calls give, and the tables did not
    // say the call allows it. Another expression is refused for itself.
    let refused_anyway = |expression: &Place| match &prediction.verdict {
        Verdict::Refuses(_) => true,
        Verdict::Undetermined(why) => observed.refusal.as_ref().is_some_and(|refusal| {
            refusal.stage == Stage::Schema
                && (why.starts_with(&format!("{}: ", refusal.at))
                    || is_a_later_call_of_the_chain(why, &refusal.at))
                && refusal
                    .place
                    .as_ref()
                    .is_some_and(|at| at.path().starts_with(expression.path()))
        }),
        Verdict::Allows => false,
    };
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
                Seen::Refused(reason) if !pivot && !refused_anyway(&expected.place) => {
                    Some(format!(
                        "{} {what}: the tables allow it, Polars refuses it: {reason}",
                        expected.place
                    ))
                }
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

/// How Polars words its refusal of a join whose keys have dtypes that do not match.
const KEYS_DO_NOT_MATCH: &str = "datatypes of join keys don't match";

/// How a refusal of the tables is worded when it is of where an expression may end.
const CANNOT_END: &str = "it cannot end here: ";

/// What Polars said, which the step words after the parameter it is about and `marker`, up to its
/// first blank line.
fn said_after<'a>(reason: &'a str, marker: &str) -> &'a str {
    let said = reason.split_once(marker).map_or(reason, |(_, said)| said);
    said.split("\n\n").next().unwrap_or(said)
}

/// The rule that a duration typed in be one Polars reads.
const DURATION_RULE: &str = "duration-is-valid";

/// The rule that a time zone be one Polars reads.
const TIME_ZONE_RULE: &str = "time-zone-is-valid";

/// What Polars says of what a named rule is about, where it refuses the same: any
/// of these words. The rule that a pivot's aggregate give one value is not Polars':
/// it lets a running value stand, so it has no words, and Polars refusing a case it
/// refuses is never for it.
fn polars_words(rule: &str) -> &'static [&'static str] {
    match rule {
        "key-is-elementwise" => &["elementwise"],
        // The engine says where `element` stands only, and Polars says it is not
        // allowed outside a pivot in its own words.
        "element-only-in-pivot-aggregate" => &[
            "stands only in a pivot's aggregate",
            "is not allowed in this context",
        ],
        "join-key-names-are-not-repeated" => &["repeated key names"],
        // Polars makes a literal of a moment of numbers typed in and names it by an alias.
        "folded-function-is-no-join-key" => &["'alias' is not allowed in a join key"],
        // The engine words a pattern it refuses with Polars' own words after its own, and
        // Polars words every refusal of a regular expression, which a size limit is one of,
        // as a regex error.
        "pattern-is-valid" => &["regex error"],
        "replacement-values-match-in-number" => &["must have the same length as"],
        // Polars finds no dtype that two zones have in common, and says it of a comparison as it
        // does of a difference or a fill.
        "moments-share-a-zone" => &[
            "could not evaluate",
            "failed to determine supertype",
            "is incompatible with expected type",
        ],
        // Polars refuses a window the value that a part widened with the length of its output.
        "window-input-is-not-widened" => &["output length of `map`"],
        _ => &[],
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
        // The engine words a duration it refuses with Polars' own reason after its own statement.
        // Polars says which of the two dtypes it cut, which is the one it holds the moments as.
        Source::Rule(rule) if *rule == DURATION_RULE => {
            let said = said_after(&refusal.reason, " is not a duration: ");
            let alike = |text: &str| text.replace("a Datetime", "a Date");
            alike(tables).ends_with(&alike(said))
        }
        // The engine words a time zone it refuses with Polars' own reason after its own statement.
        Source::Rule(rule) if *rule == TIME_ZONE_RULE => {
            tables.ends_with(said_after(&refusal.reason, " is not a time zone: "))
        }
        Source::Rule(rule) => match polars_words(rule) {
            [] => same_reason(&refused.source, tables, &refusal.reason),
            words => words.iter().any(|said| refusal.reason.contains(said)),
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
        let Some(case) = generate(input, self.focus.as_ref()) else {
            return Ok(Generation::Rejected(GenerationRejection::new(
                "too little entropy to make a case from",
            )?));
        };
        // A focused campaign checks the methods in focus in every case, so a case drawn without
        // one is drawn again.
        if self
            .focus
            .as_ref()
            .is_some_and(|focus| !focus.is_held_by(&case.document))
        {
            tally("drawn again: no method in focus");
            return Ok(Generation::Rejected(GenerationRejection::new(
                "the case holds no method in focus",
            )?));
        }
        // The engine ends the whole run for an input over the ceiling the descriptor declares,
        // so a case that long is drawn again, as one that holds no method in focus is.
        if case.encode().len() as u64 > self.descriptor.limits().bytes() {
            tally("drawn again: longer than the target admits");
            return Ok(Generation::Rejected(GenerationRejection::new(
                "the case is longer than the target admits",
            )?));
        }
        Ok(Generation::Generated(case))
    }

    fn evaluate(&self, case: &Case) -> Result<CaseOutcome> {
        let prediction = predict(case);
        let observed = observe_checked(case);
        // What the target asks of Polars for a step the walk refuses must be what the step
        // engine would give were it asked: where the walk allows the step both are asked.
        if let Some(drift) = &observed.drift {
            return failure(ASKER, "asker-differs-from-the-engine", drift.clone());
        }
        let allowed_by_polars = observed.refusal.is_none();
        if case.document.steps()[0].call().entry().id().as_str() == "LazyFrame.pivot" {
            tally("pivot case");
        }
        // One column of the other table holds one dtype, so where Polars gives keys that are paired
        // with it different ones and refuses the join for the dtypes of the keys, no dtype of the
        // column fits and the refusal is no matter of the tables. Polars joins whole numbers of
        // different widths, so the keys differing is not enough.
        if observed.keys_conflict
            && observed
                .refusal
                .as_ref()
                .is_some_and(|refusal| refusal.reason.starts_with(KEYS_DO_NOT_MATCH))
        {
            tally("join keys of different dtypes share a column");
            return CaseOutcome::unsupported(
                "one column of the other table is paired with keys of different dtypes",
            );
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
                    // A count that is the table's rows, or one value, by chance.
                    if let Some(outcome) = by_design(case, &refused, &why) {
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
                        // What Polars does with the part that the tables did not measure, which a
                        // limit names, as a rule is the tables' own.
                        if let Some(outcome) = excused("refused by a rule, Polars allows") {
                            return outcome;
                        }
                        // A predicate the rule refuses may hold a call Polars refuses first,
                        // or may not be evaluated, so it is asked on a column of its dtype.
                        if is_a_predicate_rule(rule)
                            && let Some(outcome) =
                                stricter(probe_apart(case, &refused, &prediction, &observed), rule)
                        {
                            return outcome;
                        }
                        // A count that is the table's rows, or one value, by chance.
                        if let Some(outcome) = by_design(case, &refused, rule) {
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
    use dageist_engine::document::{Place, Segment};

    use super::super::observe::SeenExpression;
    use super::super::predict::Traced;
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

    /// The target of the descriptor, admitting cases of up to `bytes` bytes.
    fn target_admitting(bytes: u64) -> AllowedTarget {
        AllowedTarget::described_by(
            &include_str!("descriptor.toml").replace("bytes = 65536", &format!("bytes = {bytes}")),
        )
    }

    #[test]
    fn a_case_longer_than_the_target_admits_is_drawn_again_and_never_made() {
        // The engine stops a run for an input over the ceiling, which would end a campaign after
        // hours of cases and lose what it found, so the target draws such a case again.
        let ceiling = 700;
        let target = target_admitting(ceiling);
        let (mut made, mut again, mut longest) = (0, 0, 0);
        for offset in 0..300_u64 {
            let entropy: Vec<u8> = (0..1024_u64)
                .map(|at| {
                    let mut mixed = (offset << 16 | at).wrapping_add(0x9e37_79b9_7f4a_7c15);
                    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                    (mixed ^ (mixed >> 31)) as u8
                })
                .collect();
            match target.generate(&entropy).expect("a generation") {
                Generation::Generated(case) => {
                    longest = longest.max(case.encode().len() as u64);
                    made += 1;
                }
                Generation::Rejected(_) => again += 1,
            }
        }
        assert!(made > 20, "only {made} cases were made");
        assert!(again > 20, "only {again} cases were drawn again");
        assert!(longest <= ceiling, "a case of {longest} bytes was made");
        take_counts();
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
    fn a_refusal_is_taken_for_the_calls_after_the_one_the_tables_cannot_be_asked_of_in_its_chain() {
        let blind = "step \"s\", exprs item 1, the start, predicates item 1, link 1: the dtype of an argument is not known";
        // The call itself and the ones after it in the chain that holds it.
        for refused in [
            "step \"s\", exprs item 1, the start, predicates item 1, link 1",
            "step \"s\", exprs item 1, the start, predicates item 1, link 2",
        ] {
            assert!(is_a_later_call_of_the_chain(blind, refused), "{refused}");
        }
        // One before it, one of another chain, and a start have none to be taken for.
        for refused in [
            "step \"s\", exprs item 1, the start, predicates item 1, link 0",
            "step \"s\", exprs item 2, the start, predicates item 1, link 2",
            "step \"s\", exprs item 1, the start, predicates item 1, the start",
        ] {
            assert!(!is_a_later_call_of_the_chain(blind, refused), "{refused}");
        }
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
                "folded-function-is-no-join-key",
                "'alias' is not allowed in a join key, use 'with_columns' first",
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
    fn a_predicate_that_polars_drops_is_a_part_it_does_not_evaluate_and_no_limit() {
        // `when(f.clip(..)).then(a).when(..).then(a).otherwise(a)`: the values are one expression,
        // so Polars drops the predicates and never evaluates the clip of a `Boolean`, which the
        // tables refuse. The editor holds every part to its types, so the tables are stricter
        // where Polars does not evaluate the part, which is shown by the case with the values
        // made to differ, and no limit says Polars accepts it.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/allowed_fuzz/regressions/stricter-a-predicate-that-is-dropped.case");
        let case = Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case");
        let outcome = std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || AllowedTarget::new().evaluate(&case))
            .expect("a thread")
            .join()
            .expect("the case is evaluated")
            .expect("an outcome");
        assert_eq!(outcome.kind(), dageist_fuzz::OutcomeKind::ExpectedRejection);
        let detail = outcome.detail().expect("a detail");
        assert!(detail.starts_with(STRICTER_WHERE_UNEVALUATED), "{detail}");
    }

    #[test]
    fn a_prefix_polars_refuses_at_the_call_the_tables_could_not_be_asked_about_is_no_difference() {
        // `when(when(g + 1).then(a)).then(f).otherwise(a)`: the tables cannot say what a number
        // given no dtype is added to text, and Polars refuses the sum when it works out the
        // schema. The condition that holds it has the dtype of its values, and Polars refuses it
        // for the call the tables could not judge, which is not a dtype the tables got wrong.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "tests/allowed_fuzz/regressions/fixed-a-prefix-refused-at-the-call-the-tables-could-not-be-asked-about.case",
        );
        let case = Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case");
        let observed = observe(&case);
        let prediction = predict(&case);
        assert!(matches!(prediction.verdict, Verdict::Undetermined(_)));
        assert_eq!(dtype_difference(&case, &prediction, &observed), None);
        // Polars refusing somewhere else, which the tables did not say they could not judge, is
        // one: the prefix is refused where the tables gave it a dtype.
        let mut elsewhere = observed.clone();
        let refusal = elsewhere.refusal.as_mut().expect("Polars refuses the case");
        refusal.at = "step \"s\", exprs item 1, link 1".to_owned();
        let difference = dtype_difference(&case, &prediction, &elsewhere).expect("a difference");
        assert!(difference.contains("Polars refuses it"), "{difference}");
        // And a refusal of the rows is no refusal of the schema the tables were asked about.
        let mut at_the_rows = observed;
        at_the_rows.refusal.as_mut().expect("a refusal").stage = Stage::Rows;
        assert!(dtype_difference(&case, &prediction, &at_the_rows).is_some());
    }

    /// A join whose two right keys are the one column `key`, beside left keys that Polars gives
    /// different dtypes: `array[f64, 2]` and `array[i64, 2]`, which the tables give the one class
    /// Array. One column of the other table cannot hold both.
    const TWO_KEYS_OF_DIFFERENT_DTYPES_SHARING_A_COLUMN: &str = r#"{"a":"Array(Int64, 2)","b":"Decimal(10, 2)","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.join","arguments":{"other":{"table":"other"},"left_on":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[{"entry":"Expr.le","arguments":{"other":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[]}}}},{"entry":"Expr.erf","arguments":{}},{"entry":"Expr.is_not_null","arguments":{}}]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[{"entry":"Expr.cosh","arguments":{}},{"entry":"Expr.floordiv","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":null}}},"then":[]}}}},{"entry":"Expr.sinh","arguments":{}},{"entry":"Expr.truediv","arguments":{"other":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[]}}}}]}}}}]}},{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[]}}]},"right_on":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["key"]}}},"then":[]}},{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["key"]}}},"then":[]}}]}}}}]}"#;

    #[test]
    fn a_join_that_pairs_one_column_with_keys_of_different_dtypes_is_not_a_case_for_the_tables() {
        // Polars refuses the join, as it must, for no dtype of the column `key` fits both keys,
        // which the tables are not asked about.
        let (outcome, _) = evaluated_and_tallied(TWO_KEYS_OF_DIFFERENT_DTYPES_SHARING_A_COLUMN);
        assert_eq!(
            outcome.kind(),
            dageist_fuzz::OutcomeKind::Unsupported,
            "{outcome:?}"
        );
    }

    /// A join of the two keys `a` and `k` with the one column `key`, as the left keys of the dtypes
    /// that `a` and `k` are: whole numbers of the widths the two are.
    fn keys_sharing_a_column(first: &str, second: &str) -> &'static str {
        let key = |name: &str| {
            format!(
                r#"{{"expression":{{"start":{{"entry":"polars.col","arguments":{{"name":{{"columns":["{name}"]}}}}}},"then":[]}}}}"#
            )
        };
        let text = format!(
            r#"{{"a":"Int32","b":"Date","revision":1}}
{{"version":1,"steps":[{{"id":"s","call":{{"entry":"LazyFrame.join","arguments":{{"other":{{"table":"other"}},"left_on":{{"many":[{},{}]}},"right_on":{{"many":[{},{}]}}}}}}}}]}}"#,
            key(first),
            key(second),
            key("key"),
            key("key")
        );
        Box::leak(text.into_boxed_str())
    }

    #[test]
    fn a_join_that_pairs_one_column_with_whole_numbers_of_two_widths_is_compared() {
        // Polars joins an `Int32` and an `Int64` key with the one column, so there is nothing
        // that makes the case one the tables are not asked about.
        let (outcome, tally) = evaluated_and_tallied(keys_sharing_a_column("a", "k"));
        assert_ne!(
            outcome.kind(),
            dageist_fuzz::OutcomeKind::Unsupported,
            "{outcome:?}"
        );
        assert!(counted(&tally, "share a column").is_none(), "{tally:?}");
    }

    #[test]
    fn a_join_that_pairs_one_column_with_keys_of_the_same_dtype_is_still_a_case() {
        // The same two keys, both the column `k`, paired with the one column `key`.
        let key = r#"{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[]}}"#;
        let bare = r#"{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["key"]}}},"then":[]}}"#;
        let text = format!(
            r#"{{"a":"Date","b":"Date","revision":1}}
{{"version":1,"steps":[{{"id":"s","call":{{"entry":"LazyFrame.join","arguments":{{"other":{{"table":"other"}},"left_on":{{"many":[{key},{key}]}},"right_on":{{"many":[{bare},{bare}]}}}}}}}}]}}"#
        );
        let text: &'static str = Box::leak(text.into_boxed_str());
        let (outcome, _) = evaluated_and_tallied(text);
        assert_ne!(
            outcome.kind(),
            dageist_fuzz::OutcomeKind::Unsupported,
            "{outcome:?}"
        );
    }

    #[test]
    fn a_refusal_at_the_call_the_tables_could_not_judge_explains_only_the_prefixes_that_hold_it() {
        // Another item of the same step, which the tables gave a dtype and Polars refuses, is
        // refused where the call is not: the refusal at the call that the tables could not be
        // asked about explains the expression that holds it, and no other.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "tests/allowed_fuzz/regressions/fixed-a-prefix-refused-at-the-call-the-tables-could-not-be-asked-about.case",
        );
        let case = Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case");
        let mut observed = observe(&case);
        let mut prediction = predict(&case);
        let other = Place::of_step(0, Some(case.document.steps()[0].id().clone()))
            .then(Segment::Parameter("exprs".to_owned()))
            .then(Segment::Item(1));
        prediction.expressions.push(Traced {
            place: other.clone(),
            start: Some(Representative::Boolean),
            links: Vec::new(),
        });
        observed.expressions.push(SeenExpression {
            place: other,
            start: Seen::Refused("another refusal".to_owned()),
            links: Vec::new(),
        });
        let difference = dtype_difference(&case, &prediction, &observed).expect("a difference");
        assert!(difference.contains("exprs item 2"), "{difference}");
        assert!(difference.contains("another refusal"), "{difference}");
    }

    #[test]
    fn a_wrong_prediction_of_the_dtype_before_the_refused_call_is_a_disagreement() {
        // A probe stands a value in for what reaches the refused call, so a
        // wrong prediction of that dtype must not be hidden by it: it is found
        // as a difference of the dtypes of the case itself, whatever is probed.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "tests/allowed_fuzz/regressions/fixed-polars-evaluates-a-window-over-the-length-of-a-literal.case",
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
    /// `sort(by=k / b.diff(drop))`: the rule refuses a division by a count of two beside rows of
    /// three, and Polars refuses the key too, for it is not dropped: no call makes it one value.
    const A_COUNT_POLARS_REFUSES: &str = r#"{"a":"Date","b":"Int8","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.sort","arguments":{"by":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[{"entry":"Expr.truediv","arguments":{"other":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[{"entry":"Expr.diff","arguments":{"null_behavior":{"choice":"drop"}}}]}}}}]}}]}}}}]}"#;

    #[test]
    fn a_count_that_is_one_value_by_chance_is_stricter_by_design() {
        // `filter(b.drop_nulls().diff(drop) < 1)`: the count is one value on the sample's rows,
        // which a filter's predicate takes, and Polars refuses it for lengths on more rows.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/allowed_fuzz/regressions/by-design-a-count-of-one-by-chance-as-a-filter-predicate.case");
        let text: &'static str = Box::leak(
            std::fs::read_to_string(path)
                .expect("a kept case")
                .into_boxed_str(),
        );
        let (outcome, tally) = evaluated_and_tallied(text);
        assert_eq!(outcome.kind(), dageist_fuzz::OutcomeKind::ExpectedRejection);
        assert!(
            outcome
                .detail()
                .is_some_and(|detail| detail.starts_with(counts::STRICTER_BY_DESIGN)),
            "{outcome:?}"
        );
        assert_eq!(
            counted(&tally, "refused where a count is the rows by chance"),
            Some(1),
            "{tally:?}"
        );
        assert_eq!(counted(&tally, "disagreeing").unwrap_or(0), 0, "{tally:?}");
    }

    #[test]
    fn a_count_that_polars_refuses_too_is_agreement_and_no_design() {
        // The same rule refusing a count that is not the rows, where Polars refuses it as well,
        // is a refusal both make: nothing is excused, and nothing is stricter by design.
        let (outcome, tally) = evaluated_and_tallied(A_COUNT_POLARS_REFUSES);
        assert_eq!(outcome.kind(), dageist_fuzz::OutcomeKind::Evaluated);
        assert_eq!(counted(&tally, "cases agreeing"), Some(1), "{tally:?}");
        assert_eq!(
            counted(&tally, "refused where a count is the rows by chance").unwrap_or(0),
            0,
            "{tally:?}"
        );
    }

    #[test]
    fn a_gather_beyond_the_rows_is_a_case_the_tables_do_not_cover() {
        // `select(g.gather(b))`: the tables measured a gather that gives a null for a place beyond
        // the rows, and Polars refuses the place that the rows of a table give it when they run.
        // The case is excused, and no disagreement.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/allowed_fuzz/regressions/uncovered-a-gather-beyond-the-rows.case");
        let text: &'static str = Box::leak(
            std::fs::read_to_string(path)
                .expect("a kept case")
                .into_boxed_str(),
        );
        let (outcome, tally) = evaluated_and_tallied(text);
        assert_eq!(outcome.kind(), dageist_fuzz::OutcomeKind::ExpectedRejection);
        assert_eq!(counted(&tally, "excused by a limit"), Some(1), "{tally:?}");
        assert_eq!(counted(&tally, "disagreeing").unwrap_or(0), 0, "{tally:?}");
    }
}
