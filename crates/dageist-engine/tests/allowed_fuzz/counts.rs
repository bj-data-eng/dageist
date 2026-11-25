//! A count that the table's rows, or one value, equal by chance.
//!
//! The tables refuse a count of values that the data or the arguments decide where the table's
//! rows or one value are needed, and a call that meets such a count beside values it does not
//! match. That is by design: a count is no number of rows the editor can know, and a chain that
//! reads it so is refused though Polars allows it where the count happens to be the rows, as
//! `head(10)` of three rows is, or one value, as the first values that follow two differences of
//! three rows are. The sample's three rows make that happen often.
//!
//! What shows that the count is why Polars allows the case is the case run again with the count
//! made another, and nothing else changed: Polars then refuses it for lengths that do not meet.
//! The count is made another by the rows the tables hold, in three ways, and by the number that the
//! case types in where one decides the count:
//!
//! - twelve rows, which are the sample's three four times over: the first values that a number
//!   takes, and the values of a table that are unchanged, are then neither the rows nor one value;
//! - four rows that tie, the second and third twice: the values that differ and the values that
//!   occur most are then two, where the sample's columns leave one value, or all of them;
//! - three rows with a null in the middle one of every column that holds none: a test of a column,
//!   a comparison and a length, which the sample's rows leave as one value, give two then, and so
//!   do the values that differ of them;
//! - the number of `head`, `tail`, `limit`, `top_k`, `bottom_k`, `gather_every`, `slice` and
//!   `repeat` made 2 values, which is neither one value, which Polars broadcasts, nor the three
//!   rows.
//!
//! Every control changes every count the case holds, so it is asked of one count only: of the
//! item of the step that holds the refused place, and of nothing where that holds a count in more
//! than one place (a count in a chain and one in an expression a call of it is given), or none.
//! Another count's refusal for lengths says nothing of the refused one's.
//!
//! A refusal that stays when the count differs is not this: a sort key that Polars drops for
//! being one value, and a part it folds, are allowed with any rows, which the probes ask about
//! apart from the case.

use dageist_engine::catalog::{self, Counting};
use dageist_engine::document::{Call, Document, Expression, Segment, Start, Step, Value};

use super::case::Case;
use super::limits::{case_changed, changed};
use super::observe::{Observed, Rows, Tables, observe, observe_probe};
use super::predict::{Prediction, Refused, Source, Verdict, predict};
use super::probe::{counts_alone, is_a_count_that_may_not_end, is_a_refusal_of_lengths};

/// What the outcome of a case says where the tables refuse a count that Polars allows because the
/// count is the table's rows, or one value, by chance.
pub const STRICTER_BY_DESIGN: &str = "stricter by design where a count is the rows by chance";

/// The rules that refuse values that meet in other counts than they take, where Polars allows the
/// case if the counts are by chance the ones it takes.
const COUNT_RULES: [&str; 2] = ["values-meet-in-count", "window-input-is-not-counted"];

/// How often the case on more rows is run again while Polars names no refusal of lengths: which
/// of two refusals it names depends on which thread gets there first.
const RUNS: usize = 8;

/// What Polars said of the case run with the count made another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settled {
    /// How the count was made another.
    pub control: &'static str,
    /// Why Polars refused the case then.
    pub reason: String,
}

impl Settled {
    /// What was shown, in words, as the outcome and the replay say it.
    pub fn said(&self) -> String {
        format!(
            "{STRICTER_BY_DESIGN}: Polars refuses the case for lengths {}, where the count is no longer the rows or one value: {}",
            self.control, self.reason
        )
    }
}

/// Whether the tables refuse a count, in what it ends or what meets it.
fn is_a_count_refusal(refused: &Refused) -> bool {
    match &refused.source {
        Source::Rule(rule) => COUNT_RULES.contains(rule),
        Source::Polars => is_a_count_that_may_not_end(&refused.reason),
        Source::Silent => false,
    }
}

/// The whole number that a count is made by, in the kind the parameter takes: a typed number, or
/// an expression that is a literal of it.
fn a_whole_number(call: &Call, name: &str, whole: i64) -> Option<Value> {
    let given: Vec<(&str, Value)> = call
        .arguments()
        .iter()
        .filter(|given| given.parameter() != name)
        .map(|given| (given.parameter(), given.value().clone()))
        .collect();
    let literal = || {
        let lit = Call::new(
            catalog::entry("polars.lit")?,
            vec![("value", Value::Integer(whole))],
        )
        .ok()?;
        Some(Value::Expression(
            Expression::new(Start::Call(lit), Vec::new()).ok()?,
        ))
    };
    [Some(Value::Integer(whole)), literal()]
        .into_iter()
        .flatten()
        .find(|value| {
            let mut asked = given.clone();
            asked.push((name, value.clone()));
            catalog::entry(call.entry().id().as_str())
                .is_some_and(|entry| Call::new(entry, asked).is_ok())
        })
}

/// The call of a method that gives the first, the last or every nth values, or the values from a
/// place, with the number that decides the count made one that is neither one value nor three
/// rows: 2 values, from the first row.
fn with_another_count(call: &Call) -> Option<Call> {
    let entry = catalog::entry(call.entry().id().as_str())?;
    let made: Vec<(&str, i64)> = match Counting::of(entry, &[])? {
        Counting::AtMost(name) => vec![(name, 2)],
        Counting::EveryNth => vec![("n", 2), ("offset", 0)],
        Counting::FromAPlace => vec![("offset", 0), ("length", 2)],
        // A value repeated the number of times it is told to: the other ranges are made by bounds
        // and a step, which a typed number alone does not make another.
        Counting::TheArguments if entry.id().as_str() == "polars.repeat" => vec![("n", 2)],
        _ => return None,
    };
    let mut given: Vec<(&str, Value)> = call
        .arguments()
        .iter()
        .filter(|given| made.iter().all(|(name, _)| *name != given.parameter()))
        .map(|given| (given.parameter(), given.value().clone()))
        .collect();
    for (name, whole) in made {
        given.push((name, a_whole_number(call, name, whole)?));
    }
    Call::new(entry, given).ok()
}

/// The case with the number of every first, last, nth or placed count made 2.
fn case_with_other_counts(case: &Case) -> Option<Case> {
    changed(case_changed(case, &with_another_count), case)
}

/// Whether Polars refuses `case` for lengths with the count made another, in the ways above.
fn refused_for_lengths_once_the_count_is_another(case: &Case) -> Option<Settled> {
    let controls = [
        ("on twelve rows", Some(case.clone()), Rows::Repeated),
        ("on four rows that tie", Some(case.clone()), Rows::Tied),
        (
            "on three rows where every column holds a null",
            Some(case.clone()),
            Rows::Nulled,
        ),
        (
            "with the number of each count made 2",
            case_with_other_counts(case),
            Rows::Sample,
        ),
    ];
    controls.into_iter().find_map(|(control, case, rows)| {
        let case = case?;
        let tables = Tables {
            rows,
            ..Tables::default()
        };
        (0..RUNS).find_map(|_| {
            let observed = if rows == Rows::Sample {
                observe(&case)
            } else {
                observe_probe(&case, tables)
            };
            observed
                .refusal
                .filter(|refusal| is_a_refusal_of_lengths(&refusal.reason))
                .map(|refusal| Settled {
                    control,
                    reason: refusal.reason,
                })
        })
    })
}

/// Whether the call decides a count of values that the data or the arguments make: a method
/// that takes some of the values it follows, the values that differ or occur most, a range, and
/// the test for membership, whose count follows the dtype of the values it is given.
fn is_counting(call: &Call) -> bool {
    let options: Vec<(&str, &str)> = call
        .arguments()
        .iter()
        .filter_map(|given| match given.value() {
            Value::Choice(word) => Some((given.parameter(), word.as_str())),
            _ => None,
        })
        .collect();
    Counting::of(call.entry(), &options).is_some() || call.entry().id().as_str() == "Expr.is_in"
}

/// The expressions a value holds, at any depth of lists and pairs.
fn expressions_of(value: &Value) -> Vec<&Expression> {
    match value {
        Value::Expression(expression) => vec![expression],
        Value::Many(items) => items.iter().flat_map(expressions_of).collect(),
        Value::Pairs(pairs) => pairs
            .iter()
            .flat_map(|(key, entry)| expressions_of(key).into_iter().chain(expressions_of(entry)))
            .collect(),
        _ => Vec::new(),
    }
}

/// How many places an expression holds a count in: the chain it is, where any call of it, of its
/// start and its links, counts, since what follows a count is made of it and the chain gives one
/// count, and each expression a call of it is given, for that is a count of its own, meeting the
/// chain's.
fn counting_places_in(expression: &Expression) -> usize {
    let calls: Vec<&Call> = match expression.start() {
        Start::Call(start) => std::iter::once(start).chain(expression.links()).collect(),
        Start::Input(_) | Start::Hole => expression.links().iter().collect(),
    };
    usize::from(calls.iter().any(|call| is_counting(call)))
        + calls
            .iter()
            .flat_map(|call| call.arguments())
            .flat_map(|given| expressions_of(given.value()))
            .map(counting_places_in)
            .sum::<usize>()
}

/// How many places the step of the case holds a count in, in all the expressions it is given.
fn counting_places(case: &Case) -> usize {
    case.document.steps()[0]
        .call()
        .arguments()
        .iter()
        .flat_map(|given| expressions_of(given.value()))
        .map(counting_places_in)
        .sum()
}

/// The case with the step holding only the item of its list that holds the refused place, where
/// the step takes a list of expressions that stand alone: the other items are other expressions,
/// and the counts they hold are not the refused one's. A join's keys are paired, so they stay.
fn only_the_item(case: &Case, refused: &Refused) -> Option<Case> {
    let step = &case.document.steps()[0];
    let call = step.call();
    let [Segment::Parameter(name), Segment::Item(at), ..] = refused.place.path() else {
        return Some(case.clone());
    };
    let Some(Value::Many(items)) = call.argument(name) else {
        return Some(case.clone());
    };
    if items.len() < 2 || call.entry().id().as_str() == "LazyFrame.join" {
        return Some(case.clone());
    }
    let kept = Value::Many(vec![items.get(*at)?.clone()]);
    let given: Vec<(&str, Value)> = call
        .arguments()
        .iter()
        .map(|given| {
            let value = if given.parameter() == name {
                kept.clone()
            } else {
                given.value().clone()
            };
            (given.parameter(), value)
        })
        .collect();
    let rebuilt = Call::new(catalog::entry(call.entry().id().as_str())?, given).ok()?;
    Some(Case {
        receiver: case.receiver,
        argument: case.argument,
        structs: case.structs.clone(),
        document: Document::new(vec![Step::new(step.id().clone(), rebuilt).ok()?]).ok()?,
    })
}

/// Whether the case holds a count in one place, Polars allows it on the sample's rows, and the
/// tables refuse it for a count: the case a control may ask of the count that was refused.
fn is_asked_of_one_count(case: &Case) -> bool {
    counting_places(case) == 1
        && observe(case).refusal.is_none()
        && matches!(&predict(case).verdict, Verdict::Refuses(refused) if is_a_count_refusal(refused))
}

/// Whether the count the tables refuse is no count the rows decide: Polars allows the case on the
/// sample's rows and on every control above that makes a count another, so what is counted is
/// one value, or the rows, by what it is made of, as a test of a struct, which is never null, a
/// comparison with a null and a column of nulls are. The tables cannot know it, and no rows show it
/// by chance, so it is no count that is by design the rows by chance: it is what the rows cannot
/// change. `false` where the tables' refusal is not of a count, the case holds no count, or Polars
/// refuses the case on the sample's rows or on any control.
pub fn is_a_count_no_rows_change(
    case: &Case,
    refused: &Refused,
    prediction: &Prediction,
    observed: &Observed,
) -> bool {
    if !is_a_count_refusal(refused) {
        return false;
    }
    // A count that a control makes another, or a part that Polars does not evaluate, which a probe
    // of the part alone shows, is settled by that evidence and is no limit.
    if settle(case, refused).is_some()
        || super::probe::settle(case, refused).is_some()
        || super::probe::settle_apart(case, refused, prediction, observed).is_some()
    {
        return false;
    }
    // The controls change every count the case holds, which is no matter where none of them makes a
    // count another: Polars allowing the case with all of them changed shows that no count is the
    // rows by chance, whatever their number.
    let Some(item) = only_the_item(case, refused).filter(|item| {
        counting_places(item) >= 1
            && observe(item).refusal.is_none()
            && matches!(&predict(item).verdict, Verdict::Refuses(refused) if is_a_count_refusal(refused))
    }) else {
        return false;
    };
    let unchanged = |case: &Case, rows: Rows| {
        let tables = Tables {
            rows,
            ..Tables::default()
        };
        (0..RUNS).all(|_| {
            let observed = if rows == Rows::Sample {
                observe(case)
            } else {
                observe_probe(case, tables)
            };
            observed.refusal.is_none()
        })
    };
    [Rows::Repeated, Rows::Tied, Rows::Nulled]
        .into_iter()
        .all(|rows| unchanged(&item, rows))
        && case_with_other_counts(&item).is_none_or(|other| unchanged(&other, Rows::Sample))
}

/// Whether the tables' refusal of a count that Polars allows is by design, shown by Polars
/// refusing the case for lengths where the count is no longer the rows or one value by chance:
/// on more rows, on rows that tie, or with the number that decides each count typed in made 2.
/// A control changes every count the case holds, so it is asked of the item of the step that holds
/// the refused place, and of nothing where that holds a count in more than one place, or none,
/// or Polars refuses it on the sample's rows, or the tables do not refuse it for a count: another
/// count's refusal for lengths is no sign of the refused one's. Where Polars drops the part in the
/// case, as a sort key of one value, it is the part alone, allowed on the sample's rows and holding
/// one count, that is asked the same. `None` where the tables' refusal is not of a count, or
/// Polars allows the case then too, panics on it, or refuses it for another reason than lengths.
pub fn settle(case: &Case, refused: &Refused) -> Option<Settled> {
    if !is_a_count_refusal(refused) {
        return None;
    }
    only_the_item(case, refused)
        .filter(is_asked_of_one_count)
        .and_then(|item| refused_for_lengths_once_the_count_is_another(&item))
        .or_else(|| {
            counts_alone(case, refused)
                .iter()
                .filter(|alone| counting_places(alone) == 1 && observe(alone).refusal.is_none())
                .find_map(refused_for_lengths_once_the_count_is_another)
                .map(|settled| Settled {
                    control: settled.control,
                    reason: format!("{}, of the part alone", settled.reason),
                })
        })
}

#[cfg(test)]
mod tests {
    use dageist_engine::document::{Place, Segment, StepId};

    use super::super::predict::{Verdict, predict};
    use super::*;

    fn kept(name: &str) -> Case {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/allowed_fuzz/regressions")
            .join(name);
        Case::decode(&std::fs::read(path).expect("a kept case")).expect("a case")
    }

    fn refusal(case: &Case) -> Refused {
        match predict(case).verdict {
            Verdict::Refuses(refused) => refused,
            other => panic!("the tables refuse the case: {other:?}"),
        }
    }

    #[test]
    fn a_count_that_is_the_rows_or_one_value_by_chance_is_refused_for_lengths_on_more_rows() {
        // A filter's predicate of one value, a sort key of one value, and the bound of a clip that
        // is one value beside a column: each is refused for lengths once the rows are twelve.
        for name in [
            "by-design-a-count-of-one-by-chance-as-a-filter-predicate.case",
            "by-design-a-count-of-one-by-chance-as-a-sort-key.case",
            "by-design-a-count-met-by-chance-as-the-bound-of-a-clip.case",
        ] {
            let case = kept(name);
            let settled = settle(&case, &refusal(&case)).unwrap_or_else(|| panic!("{name}"));
            assert!(
                is_a_refusal_of_lengths(&settled.reason),
                "{name}: {settled:?}"
            );
        }
    }

    #[test]
    fn a_count_that_polars_never_evaluates_is_no_count_by_chance() {
        // A sort key of one value is dropped whatever the rows are: the case is allowed on more
        // rows too, which is not the count's doing, and a probe asks about such a part.
        for name in [
            "stricter-a-count-met-by-an-operand-in-a-sort-key-of-one-value.case",
            "stricter-a-count-given-to-a-condition-in-a-sort-key-of-one-value.case",
            "stricter-a-window-over-a-count-in-a-sort-key-of-one-value.case",
        ] {
            let case = kept(name);
            assert_eq!(settle(&case, &refusal(&case)), None, "{name}");
        }
    }

    #[test]
    fn a_refusal_that_is_no_count_is_not_asked() {
        // The tables refuse a comparison in a sort key of one value, which more rows do not change.
        let case = kept("stricter-a-comparison-in-a-sort-key-of-one-value.case");
        assert_eq!(settle(&case, &refusal(&case)), None);
    }

    #[test]
    fn a_count_that_the_data_or_a_typed_number_make_one_value_is_refused_for_lengths_once_it_is_not()
     {
        // The values that occur most are one on the sample's rows, as are the first value that a
        // typed 1 takes and the every first value, which is every row; made two by rows that tie
        // or by the number, Polars refuses them for lengths. And a count that is one by chance in
        // a part that Polars drops as a sort key is one the part alone shows.
        for name in [
            "by-design-the-values-that-occur-most-are-one-by-chance.case",
            "by-design-a-count-typed-as-one-as-a-sort-key.case",
            "by-design-a-count-typed-as-one-beside-the-rows.case",
            "by-design-a-count-by-chance-in-a-sort-key-that-is-one-value.case",
            "by-design-an-every-nth-value-from-a-place-by-chance.case",
            "by-design-a-value-repeated-once.case",
        ] {
            let case = kept(name);
            let settled = settle(&case, &refusal(&case)).unwrap_or_else(|| panic!("{name}"));
            assert!(
                is_a_refusal_of_lengths(&settled.reason),
                "{name}: {settled:?}"
            );
        }
    }

    /// `with_columns(a.arg_sort().mode())`: the places that sort a column are all different, so
    /// the values that occur most are as many as the rows whatever the rows are.
    const THE_MODE_OF_PLACES: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.with_columns","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[{"entry":"Expr.arg_sort","arguments":{}},{"entry":"Expr.mode","arguments":{}}]}}]}}}}]}"#;

    #[test]
    fn a_count_that_is_the_rows_whatever_they_are_is_not_settled() {
        // Polars allows it with any rows, so no control refuses it: it is no count by chance, and
        // the finding stays.
        let case = Case::decode(THE_MODE_OF_PLACES.as_bytes()).expect("a case");
        assert_eq!(settle(&case, &refusal(&case)), None);
    }

    #[test]
    fn each_count_is_made_another_by_the_control_that_shows_it() {
        // The values that occur most are one on the sample and two on rows that tie; the numbers
        // that a typed 1 decides are made 2; the first values that a default takes, and a count
        // that is the rows by chance, are no longer the rows on twelve.
        for (name, control) in [
            (
                "by-design-the-values-that-occur-most-are-one-by-chance.case",
                "on four rows that tie",
            ),
            (
                "by-design-a-count-typed-as-one-as-a-sort-key.case",
                "with the number of each count made 2",
            ),
            (
                "by-design-a-count-typed-as-one-beside-the-rows.case",
                "with the number of each count made 2",
            ),
            (
                "by-design-a-count-by-chance-in-a-sort-key-that-is-one-value.case",
                "on twelve rows",
            ),
            (
                "by-design-an-every-nth-value-from-a-place-by-chance.case",
                "with the number of each count made 2",
            ),
            (
                "by-design-a-value-repeated-once.case",
                "with the number of each count made 2",
            ),
        ] {
            let case = kept(name);
            let settled = settle(&case, &refusal(&case)).expect(name);
            assert_eq!(settled.control, control, "{name}");
        }
    }

    #[test]
    fn a_part_is_asked_alone_only_where_it_is_allowed_on_the_sample() {
        // The part of the sort key that Polars drops is asked apart from the case, and said to be
        // so, where it is allowed on the sample's rows: a part that is refused there is no count
        // by chance, but one the probes settle.
        let case = kept("by-design-a-count-by-chance-in-a-sort-key-that-is-one-value.case");
        let refused = refusal(&case);
        let settled = settle(&case, &refused).expect("a count by chance");
        assert!(settled.reason.ends_with("of the part alone"), "{settled:?}");
        for alone in counts_alone(&case, &refused) {
            assert_eq!(observe(&alone).refusal, None);
        }
        let dropped = kept("stricter-a-count-met-by-an-operand-in-a-sort-key-of-one-value.case");
        let refused = refusal(&dropped);
        // Refused on the sample's rows as it stands alone, so asked of no control.
        assert!(
            counts_alone(&dropped, &refused)
                .iter()
                .all(|alone| observe(alone).refusal.is_some())
        );
    }

    /// `with_columns(a.arg_sort().mode(), b.head(n=3))`: the first item is a count that is the rows
    /// whatever they are, which stays a finding; the second is a count by chance on its own.
    const A_STRUCTURAL_COUNT_BESIDE_A_COUNT_BY_CHANCE: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.with_columns","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[{"entry":"Expr.arg_sort","arguments":{}},{"entry":"Expr.mode","arguments":{}}]}},{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[{"entry":"Expr.head","arguments":{"n":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":3}}},"then":[]}}}}]}}]}}}}]}"#;

    /// `with_columns(k.mode() + k.head(n=3))`: both counts are in one expression.
    const TWO_COUNTS_IN_ONE_EXPRESSION: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.with_columns","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[{"entry":"Expr.mode","arguments":{}},{"entry":"Expr.add","arguments":{"other":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[{"entry":"Expr.head","arguments":{"n":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":3}}},"then":[]}}}}]}}}}]}}]}}}}]}"#;

    /// `with_columns(k.mode(), u.cum_sum())`: a count by chance beside an item that holds none.
    const A_COUNT_BESIDE_AN_ITEM_WITH_NONE: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.with_columns","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[{"entry":"Expr.mode","arguments":{}}]}},{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["u"]}}},"then":[{"entry":"Expr.cum_sum","arguments":{}}]}}]}}}}]}"#;

    /// `with_columns(k.cum_sum())`: no call counts.
    const NO_COUNT: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.with_columns","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[{"entry":"Expr.cum_sum","arguments":{}}]}}]}}}}]}"#;

    fn decoded(text: &str) -> Case {
        Case::decode(text.as_bytes()).expect("a case")
    }

    #[test]
    fn a_second_count_in_another_item_settles_no_refusal_of_the_first() {
        // The twelve rows refuse `b.head(3)` for lengths, which says nothing of the places that
        // sort a column: the item that holds the refused place is asked alone, and it is allowed
        // on every rows, so the refusal stays a finding.
        let case = decoded(A_STRUCTURAL_COUNT_BESIDE_A_COUNT_BY_CHANCE);
        let refused = refusal(&case);
        assert!(matches!(
            refused.source,
            Source::Rule("values-meet-in-count")
        ));
        assert_eq!(settle(&case, &refused), None);
    }

    #[test]
    fn two_counts_in_one_expression_settle_no_refusal_of_either() {
        // Which of the two the control makes another is not the refused one's to say.
        let case = decoded(TWO_COUNTS_IN_ONE_EXPRESSION);
        let refused = refusal(&case);
        assert!(is_a_count_refusal(&refused), "{refused:?}");
        assert_eq!(settle(&case, &refused), None);
    }

    #[test]
    fn a_count_by_chance_is_settled_though_another_item_holds_no_count() {
        let case = decoded(A_COUNT_BESIDE_AN_ITEM_WITH_NONE);
        let settled = settle(&case, &refusal(&case)).expect("a count by chance");
        assert!(is_a_refusal_of_lengths(&settled.reason), "{settled:?}");
    }

    #[test]
    fn a_case_that_holds_no_count_settles_nothing_whatever_the_refusal_says() {
        let case = decoded(NO_COUNT);
        for source in [Source::Rule("values-meet-in-count"), Source::Polars] {
            let refused = Refused {
                source,
                at: String::new(),
                place: Place::of_step(0, StepId::new("s").ok())
                    .then(Segment::Parameter("exprs".to_owned()))
                    .then(Segment::Item(0)),
                stage: None,
                reason: "it cannot end here: a count of values that the data or the arguments decide, which may not end an expression in select: lengths don't match".to_owned(),
                argument_unknown: false,
            };
            assert_eq!(settle(&case, &refused), None);
        }
    }

    #[test]
    fn only_a_refusal_for_lengths_that_do_not_meet_settles_a_count() {
        for reason in [
            "lengths don't match: zip node received non-equal length inputs",
            "cannot evaluate two Series of different lengths (0 and 3)",
            "sort expressions must have same length as DataFrame, got DataFrame height: 3",
            "shapes of `self`, `mask` and `other` are not suitable for `zip_with` operation",
        ] {
            assert!(is_a_refusal_of_lengths(reason), "{reason}");
        }
        for reason in [
            "gather indices are out of bounds",
            "attempt to multiply with overflow",
            "conversion from `str` to `i64` failed in column 'a' for 1 out of 1 values",
            "Polars panicked: index out of bounds",
            "`exp` operation not supported for dtype `date` as argument 0",
            "dtype `null` has no time unit",
        ] {
            assert!(!is_a_refusal_of_lengths(reason), "{reason}");
        }
    }
}
