//! A count that the table's rows, or one value, equal by chance.
//!
//! The tables refuse a count of values that the data or the arguments decide where the table's
//! rows or one value are needed, and a call that meets such a count beside values it does not
//! match. That is by design: a count is no number of rows the editor can know, and a chain that
//! reads it so is refused though Polars allows it where the count happens to be the rows, as
//! `head(10)` of three rows is, or one value, as the first values that follow two differences of
//! three rows are. The sample's three rows make that happen often.
//!
//! What shows that the count is why Polars allows the case is the case run again on more rows,
//! with nothing else changed: the count is then no longer the rows, and Polars refuses it for
//! lengths that do not meet. A refusal that stays when the count differs is not this: a sort key
//! that Polars drops for being one value, and a part it folds, are allowed with any rows, which
//! the probes ask about apart from the case.

use super::case::Case;
use super::observe::{Tables, observe_probe};
use super::predict::{Refused, Source};
use super::probe::{is_a_count_that_may_not_end, is_a_refusal_of_lengths};

/// What the outcome of a case says where the tables refuse a count that Polars allows because the
/// count is the table's rows, or one value, by chance.
pub const STRICTER_BY_DESIGN: &str = "stricter by design where a count is the rows by chance";

/// The rules that refuse values that meet in other counts than they take, where Polars allows the
/// case if the counts are by chance the ones it takes.
const COUNT_RULES: [&str; 2] = ["values-meet-in-count", "window-input-is-not-counted"];

/// How often the case on more rows is run again while Polars names no refusal of lengths: which
/// of two refusals it names depends on which thread gets there first.
const RUNS: usize = 8;

/// What Polars said of the case run on more rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settled {
    /// Why Polars refused the case on more rows.
    pub reason: String,
}

impl Settled {
    /// What was shown, in words, as the outcome and the replay say it.
    pub fn said(&self) -> String {
        format!(
            "{STRICTER_BY_DESIGN}: the case is refused for lengths on more rows, where the count is no longer the rows: {}",
            self.reason
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

/// Whether the tables' refusal of a count that Polars allows is by design, shown by Polars
/// refusing the case for lengths on more rows. `None` where the tables' refusal is not of a
/// count, or Polars allows the case on more rows, panics on them, or refuses them for another
/// reason than lengths.
pub fn settle(case: &Case, refused: &Refused) -> Option<Settled> {
    if !is_a_count_refusal(refused) {
        return None;
    }
    let tables = Tables {
        more_rows: true,
        ..Tables::default()
    };
    (0..RUNS).find_map(|_| {
        observe_probe(case, tables)
            .refusal
            .filter(|refusal| is_a_refusal_of_lengths(&refusal.reason))
            .map(|refusal| Settled {
                reason: refusal.reason,
            })
    })
}

#[cfg(test)]
mod tests {
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
}
