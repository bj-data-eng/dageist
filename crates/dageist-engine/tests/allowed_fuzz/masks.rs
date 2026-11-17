//! The part that Polars does not evaluate, and a refusal that it does.
//!
//! The tables refuse a part, and Polars allows the step because it never evaluates
//! the part: the target asks Polars about the part with a probe, and counts the case
//! as refused where Polars does not evaluate it. That is so only with the cause that
//! makes Polars skip the part. The same part without the cause is evaluated, and
//! Polars refuses it for the tables' reason, which is agreement, not a part Polars
//! does not evaluate. Each cause is shown here by a case that has it and one that
//! differs from it in that alone, both run in Polars.

use dageist_fuzz::{CaseTarget, OutcomeKind};

use super::case::Case;
use super::limits::Limit;
use super::target::{AllowedTarget, STRICTER_WHERE_UNEVALUATED};

/// `a.cum_sum().len()` of a `Date`, the only item of a `select`: Polars never reads what the length counts.
const LEN_IN_A_SELECT_OF_ONE_VALUES: &str = r#"{"a":"Date","b":"Int64","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[{"entry":"Expr.cum_sum","arguments":{}},{"entry":"Expr.len","arguments":{}}]}}]}}}}]}"#;

/// The same length beside a column item of the `select`: Polars reads what it counts.
const LEN_BESIDE_A_COLUMN: &str = r#"{"a":"Date","b":"Int64","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[{"entry":"Expr.cum_sum","arguments":{}},{"entry":"Expr.len","arguments":{}}]}},{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[]}}]}}}}]}"#;

/// The same length in `with_columns`.
const LEN_IN_WITH_COLUMNS: &str = r#"{"a":"Date","b":"Int64","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.with_columns","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[{"entry":"Expr.cum_sum","arguments":{}},{"entry":"Expr.len","arguments":{}}]}}]}}}}]}"#;

/// The same length compared in a `filter`.
const LEN_IN_A_FILTER: &str = r#"{"a":"Date","b":"Int64","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.filter","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[{"entry":"Expr.cum_sum","arguments":{}},{"entry":"Expr.len","arguments":{}},{"entry":"Expr.gt","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":1},"dtype":{"dtype":"UInt32"}}},"then":[]}}}}]}}]}}}}]}"#;

/// A sort key that is one value, whose condition's predicate (a `Date`) Polars never meets: it drops the key.
const SORT_KEY_OF_ONE_VALUE: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.sort","arguments":{"by":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["g"]}}},"then":[]}}}},{"entry":"Expr.min","arguments":{}}]}}]}}}}]}"#;

/// The same condition as the key, not one value: Polars evaluates its predicate.
const SORT_KEY_OF_A_CONDITION: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.sort","arguments":{"by":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["g"]}}},"then":[]}}}}]}}]}}}}]}"#;

/// A condition whose two values are one expression, which Polars takes for that expression and drops the predicate of.
const BRANCHES_THAT_ARE_ONE: &str = r#"{"a":"Int64","b":"Int64","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["h"]}}},"then":[{"entry":"Expr.dt.month_start","arguments":{}},{"entry":"Expr.is_not_null","arguments":{}}]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[]}}}},{"entry":"Then.otherwise","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[]}}}}]}}]}}}}]}"#;

/// The same condition whose values differ: Polars evaluates the predicate.
const BRANCHES_THAT_DIFFER: &str = r#"{"a":"Int64","b":"Int64","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["h"]}}},"then":[{"entry":"Expr.dt.month_start","arguments":{}},{"entry":"Expr.is_not_null","arguments":{}}]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[]}}}},{"entry":"Then.otherwise","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["g"]}}},"then":[]}}}}]}}]}}}}]}"#;

/// A condition that the literal `true` decides: Polars drops the predicates after it.
const DECIDED_CONDITION: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":true}}},"then":[]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["g"]}}},"then":[]}}}},{"entry":"Then.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["h"]}}},"then":[{"entry":"Expr.dt.month_start","arguments":{}}]}}]}}},{"entry":"ChainedWhen.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["g"]}}},"then":[]}}}},{"entry":"ChainedThen.otherwise","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["g"]}}},"then":[]}}}}]}}]}}}}]}"#;

/// The same condition where the first predicate is a column: Polars evaluates the predicates after it.
const UNDECIDED_CONDITION: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["g"]}}},"then":[]}}}},{"entry":"Then.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["h"]}}},"then":[{"entry":"Expr.dt.month_start","arguments":{}}]}}]}}},{"entry":"ChainedWhen.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["g"]}}},"then":[]}}}},{"entry":"ChainedThen.otherwise","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["g"]}}},"then":[]}}}}]}}]}}}}]}"#;

/// A predicate inside a predicate of a condition whose last values are one expression.
const PREDICATE_IN_BRANCHES_THAT_ARE_ONE: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.with_columns","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":null}}},"then":[]}}}},{"entry":"Expr.le","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":1}}},"then":[]}}}}]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["h"]}}},"then":[]}}}},{"entry":"Then.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[{"entry":"Expr.ge","arguments":{"other":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[]}}}}]}}]}}},{"entry":"ChainedWhen.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["h"]}}},"then":[]}}}},{"entry":"ChainedThen.otherwise","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["h"]}}},"then":[]}}}}]}}]}}}}]}"#;

/// The same where the last value differs.
const PREDICATE_IN_BRANCHES_THAT_DIFFER: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.with_columns","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":null}}},"then":[]}}}},{"entry":"Expr.le","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"integer":1}}},"then":[]}}}}]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["h"]}}},"then":[]}}}},{"entry":"Then.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["b"]}}},"then":[{"entry":"Expr.ge","arguments":{"other":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["a"]}}},"then":[]}}}}]}}]}}},{"entry":"ChainedWhen.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["h"]}}},"then":[]}}}},{"entry":"ChainedThen.otherwise","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["g"]}}},"then":[]}}}}]}}]}}}}]}"#;

/// A comparison with a null literal of another dtype in a `then` that no row takes (`k > k` is false in
/// each): Polars never meets the comparison, and the column of its dtype alone is refused as the tables say.
const LITERAL_IN_A_THEN_NO_ROW_TAKES: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[{"entry":"Expr.gt","arguments":{"other":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[]}}}}]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[{"entry":"Expr.lt","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":null},"dtype":{"dtype":"List(String)"}}},"then":[]}}}}]}}}}]}}]}}}}]}"#;

/// The same where some rows take it (`f` is true in two): Polars folds the comparison with the null
/// literal and allows it, where a column of its dtype is refused. The literal is why.
const LITERAL_IN_A_THEN_SOME_ROW_TAKES: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.select","arguments":{"exprs":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["f"]}}},"then":[{"entry":"Expr.lt","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":null},"dtype":{"dtype":"List(String)"}}},"then":[]}}}}]}}}}]}}]}}}}]}"#;

/// A comparison with a null literal in a sort key that is one value (a `max`), which Polars drops.
const LITERAL_IN_A_SORT_KEY_OF_ONE_VALUE: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.sort","arguments":{"by":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["h"]}}},"then":[{"entry":"Expr.le","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":null},"dtype":{"dtype":"UInt8"}}},"then":[]}}}}]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[]}}}},{"entry":"Expr.max","arguments":{}}]}}]}}}}]}"#;

/// The same sort key that is not one value: Polars evaluates it, and folds the comparison with the null
/// literal, where a column of its dtype is refused. The literal is why.
const LITERAL_IN_A_SORT_KEY_OF_A_CONDITION: &str = r#"{"a":"Date","b":"Date","revision":1}
{"version":1,"steps":[{"id":"s","call":{"entry":"LazyFrame.sort","arguments":{"by":{"many":[{"expression":{"start":{"entry":"polars.when","arguments":{"predicates":{"many":[{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["h"]}}},"then":[{"entry":"Expr.le","arguments":{"other":{"expression":{"start":{"entry":"polars.lit","arguments":{"value":{"flag":null},"dtype":{"dtype":"UInt8"}}},"then":[]}}}}]}}]}}},"then":[{"entry":"When.then","arguments":{"statement":{"expression":{"start":{"entry":"polars.col","arguments":{"name":{"columns":["k"]}}},"then":[]}}}}]}}]}}}}]}"#;

/// What the target made of `text`: the kind of outcome, and what it said.
fn outcome_of(text: &'static str) -> (OutcomeKind, String) {
    let case = Case::decode(text.as_bytes()).expect("a case");
    let outcome = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || AllowedTarget::new().evaluate(&case))
        .expect("a thread")
        .join()
        .expect("the case is evaluated")
        .expect("an outcome");
    (
        outcome.kind(),
        outcome.detail().unwrap_or_default().to_owned(),
    )
}

/// The tables refuse a part and Polars allows the step because it skips the part.
fn assert_skipped(text: &'static str) {
    let (kind, said) = outcome_of(text);
    assert_eq!(kind, OutcomeKind::ExpectedRejection, "{text}: {said}");
    assert!(said.starts_with(STRICTER_WHERE_UNEVALUATED), "{said}");
}

/// The tables refuse a part and Polars allows the step because a literal in it is one value, which
/// Polars broadcasts and casts as it does not a column of its dtype.
fn assert_excused_by_the_literal(text: &'static str) {
    let (kind, said) = outcome_of(text);
    assert_eq!(kind, OutcomeKind::ExpectedRejection, "{text}: {said}");
    assert_eq!(said, Limit::ALiteral.statement(), "{text}");
}

/// The tables refuse a part and Polars refuses it too, for the same reason.
fn assert_refused_by_polars(text: &'static str) {
    let (kind, said) = outcome_of(text);
    assert_eq!(kind, OutcomeKind::Evaluated, "{text}: {said}");
}

#[test]
fn a_length_is_not_read_only_in_a_select_whose_items_are_all_one_value() {
    assert_skipped(LEN_IN_A_SELECT_OF_ONE_VALUES);
    for read in [LEN_BESIDE_A_COLUMN, LEN_IN_WITH_COLUMNS, LEN_IN_A_FILTER] {
        assert_refused_by_polars(read);
    }
}

#[test]
fn a_sort_key_that_is_one_value_is_dropped_and_one_that_is_not_is_evaluated() {
    assert_skipped(SORT_KEY_OF_ONE_VALUE);
    assert_refused_by_polars(SORT_KEY_OF_A_CONDITION);
}

#[test]
fn a_condition_whose_values_are_one_expression_drops_its_predicate_and_one_that_differs_does_not() {
    assert_skipped(BRANCHES_THAT_ARE_ONE);
    assert_refused_by_polars(BRANCHES_THAT_DIFFER);
    assert_skipped(PREDICATE_IN_BRANCHES_THAT_ARE_ONE);
    assert_refused_by_polars(PREDICATE_IN_BRANCHES_THAT_DIFFER);
}

#[test]
fn a_condition_that_a_literal_decides_drops_the_predicates_after_it_and_one_that_is_not_decided_does_not()
 {
    assert_skipped(DECIDED_CONDITION);
    assert_refused_by_polars(UNDECIDED_CONDITION);
}

#[test]
fn a_literal_is_skipped_where_no_row_takes_its_branch_and_is_the_cause_where_one_does() {
    assert_skipped(LITERAL_IN_A_THEN_NO_ROW_TAKES);
    assert_excused_by_the_literal(LITERAL_IN_A_THEN_SOME_ROW_TAKES);
}

#[test]
fn a_literal_is_skipped_in_a_sort_key_of_one_value_and_is_the_cause_in_one_that_is_not() {
    assert_skipped(LITERAL_IN_A_SORT_KEY_OF_ONE_VALUE);
    assert_excused_by_the_literal(LITERAL_IN_A_SORT_KEY_OF_A_CONDITION);
}
