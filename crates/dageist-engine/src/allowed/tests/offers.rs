//! Offers against Polars' documented behavior, each result confirmed by
//! running Polars on its own API.

use polars::prelude::{
    DataType, Expr, IntoLazy, NULL, SortOptions, WindowMapping, col, element, lit,
};
use std::sync::Arc;

use crate::allowed::dtype::ResultDtype;
use crate::allowed::offers::RefusalSource;
use crate::allowed::{
    After, ConditionState, Context, Nested, Outer, Place, Representative, Rule, Shape, Stage,
    Verdict, offer, offers, options,
};
use crate::catalog;

use super::oracle::{Seen, see, selected};
use super::sample::Frames;

fn entry(id: &str) -> &'static catalog::Entry {
    catalog::entry(id).unwrap_or_else(|| panic!("no entry {id}"))
}

fn after(dtype: Representative) -> After {
    After::Value {
        dtype,
        shape: Shape::Many,
    }
}

fn select_after(dtype: Representative) -> Place {
    Place::new(Context::step(Outer::Select), after(dtype))
}

fn verdict(id: &str, place: &Place, arguments: &[(&str, Representative)]) -> Verdict {
    offer(entry(id), place, arguments)
}

fn result_of(verdict: &Verdict) -> Option<&str> {
    verdict
        .allowance()
        .and_then(|allowance| allowance.result())
        .map(ResultDtype::name)
}

#[test]
fn dt_date_is_offered_after_a_datetime_and_refused_after_text_with_polars_reason() {
    let offered = verdict("Expr.dt.date", &select_after(Representative::Datetime), &[]);
    assert_eq!(result_of(&offered), Some("Date"), "{offered:?}");
    let refused = verdict("Expr.dt.date", &select_after(Representative::String), &[]);
    let refusal = refused.refusal().expect("refused after text");
    // The schema resolves and the rows refuse it: only running it shows the mistake.
    assert_eq!(refusal.source(), RefusalSource::Polars(Stage::Rows));
    assert!(
        refusal.reason().contains("Datetime or Date"),
        "{}",
        refusal.reason()
    );

    // Polars, by hand: the schema of `dt.date` on text is a `Date`, and the rows fail.
    let text = Frames::new(
        Representative::String,
        Representative::String,
        &DataType::Int64,
    );
    let mut frame = text.rows.clone().lazy().select([col("a").dt().date()]);
    assert_eq!(
        frame.collect_schema().expect("a schema").get("a"),
        Some(&DataType::Date)
    );
    assert!(frame.collect().is_err());
    assert_eq!(
        selected(
            Representative::Datetime,
            Representative::Int64,
            col("a").dt().date()
        ),
        Seen::Ran(DataType::Date)
    );
}

#[test]
fn cum_count_gives_a_uint32_whatever_it_counts() {
    // Polars documents `cum_count` as giving the row count so far in `UInt32`.
    for dtype in Representative::ALL {
        let verdict = verdict("Expr.cum_count", &select_after(dtype), &[]);
        assert_eq!(result_of(&verdict), Some("UInt32"), "{dtype}: {verdict:?}");
        assert_eq!(
            selected(dtype, Representative::Int64, col("a").cum_count(false)),
            Seen::Ran(DataType::UInt32),
            "{dtype}"
        );
    }
}

/// `col("f")` windowed by `order_by` and by `partition_by`, built on Polars' own API.
fn windowed(order_by: Expr) -> Expr {
    col("f")
        .over_with_options(
            Some(vec![col("k")]),
            Some((vec![order_by], SortOptions::default())),
            WindowMapping::GroupsToRows,
        )
        .expect("a window")
}

#[test]
fn a_window_inside_another_windows_order_by_is_allowed_as_polars_allows_it() {
    let place = Place::new(
        Context::inside(Outer::Select, Nested::OrderBy),
        after(Representative::Int64),
    );
    let inside = verdict("Expr.over", &place, &[]);
    assert!(inside.is_allowed(), "{inside:?}");
    // Polars, by hand: the window ordered by another window runs.
    let inner = col("a")
        .over_with_options(
            Some(vec![col("k")]),
            None::<(Vec<Expr>, SortOptions)>,
            WindowMapping::GroupsToRows,
        )
        .expect("a window");
    assert_eq!(
        selected(
            Representative::Int64,
            Representative::Int64,
            windowed(inner)
        ),
        Seen::Ran(DataType::Boolean)
    );
}

#[test]
fn a_window_is_refused_as_a_join_key_with_polars_reason() {
    let place = Place::new(Context::step(Outer::JoinLeft), after(Representative::Int64));
    let key = verdict("Expr.over", &place, &[]);
    let refusal = key.refusal().expect("a window cannot be a join key");
    assert_eq!(refusal.source(), RefusalSource::Polars(Stage::Schema));
    assert!(
        refusal.reason().contains("elementwise"),
        "{}",
        refusal.reason()
    );
    // Polars, by hand: a join on a window of the left table's column is refused.
    let frames = Frames::new(
        Representative::Int64,
        Representative::Int64,
        &DataType::Int64,
    );
    let window = col("a")
        .over_with_options(
            Some(vec![col("k")]),
            None::<(Vec<Expr>, SortOptions)>,
            WindowMapping::GroupsToRows,
        )
        .expect("a window");
    let joined = frames
        .rows
        .clone()
        .lazy()
        .join_builder()
        .with(frames.other_rows.clone().lazy())
        .left_on(vec![window])
        .right_on(vec![col("rk")])
        .finish();
    assert!(joined.and_then(|mut frame| frame.collect_schema()).is_err());
}

#[test]
fn a_pivot_aggregate_must_give_one_value_although_polars_lets_it_give_more() {
    let pivot = |shape| {
        Place::new(
            Context::step(Outer::PivotAggregate),
            After::Value {
                dtype: Representative::Int64,
                shape,
            },
        )
    };
    // A sum is one value for a group, a running sum is a value for each row of it.
    let sum = verdict("Expr.sum", &pivot(Shape::Many), &[]);
    let sum = sum.allowance().expect("a sum is offered");
    assert_eq!(sum.shape(), Shape::One);
    assert!(sum.can_end().is_ok());
    let running = verdict("Expr.cum_sum", &pivot(Shape::Many), &[]);
    let running = running
        .allowance()
        .expect("a running sum is offered to be continued");
    assert_eq!(running.shape(), Shape::Many);
    let refusal = running.can_end().expect_err("it cannot end the aggregate");
    assert_eq!(
        refusal.source(),
        RefusalSource::Rule(Rule::PivotAggregateGivesOneValue)
    );
    // A chain that has given one value keeps giving one.
    let one = verdict("Expr.cum_sum", &pivot(Shape::One), &[]);
    assert!(one.allowance().expect("offered").can_end().is_ok());

    // Polars, by hand: the first group holds two values, and a running sum
    // over them is not refused: the rule is dageist's, not Polars'.
    let frames = Frames::new(
        Representative::Int64,
        Representative::Int64,
        &DataType::Int64,
    );
    let pivoted = frames.rows.clone().lazy().pivot(
        polars::prelude::cols(["h"]),
        Arc::new(frames.on_h.clone()),
        polars::prelude::cols(["k"]),
        polars::prelude::cols(["a"]),
        element().cum_sum(false),
        false,
        "_".into(),
        polars::frame::PivotColumnNaming::Auto,
    );
    assert!(pivoted.collect().is_ok());
}

#[test]
fn element_stands_in_a_pivot_aggregate_and_nowhere_else() {
    let start = |context| Place::new(context, After::Start);
    let in_pivot = verdict(
        "polars.element",
        &start(Context::step(Outer::PivotAggregate)),
        &[],
    );
    assert!(in_pivot.is_allowed(), "{in_pivot:?}");
    for outer in [
        Outer::Select,
        Outer::Filter,
        Outer::Sort,
        Outer::JoinLeft,
        Outer::JoinRight,
    ] {
        let outside = verdict("polars.element", &start(Context::step(outer)), &[]);
        let refusal = outside
            .refusal()
            .unwrap_or_else(|| panic!("{outer:?}: {outside:?}"));
        assert_eq!(
            refusal.source(),
            RefusalSource::Rule(Rule::ElementOnlyInPivotAggregate),
            "{outer:?}"
        );
        assert!(refusal.reason().contains("element"), "{}", refusal.reason());
    }
    // Polars, by hand: `element` selected from a table is refused.
    let frames = Frames::new(
        Representative::Int64,
        Representative::Int64,
        &DataType::Int64,
    );
    assert_eq!(
        see(&frames.rows, &frames.empty, |frame| frame
            .select([element()])),
        Seen::Refused(Stage::Schema)
    );
}

#[test]
fn a_condition_predicate_that_is_not_boolean_cannot_end_the_predicate() {
    let predicate = |dtype| {
        Place::new(
            Context::inside(Outer::Select, Nested::ConditionPredicate),
            after(dtype),
        )
    };
    // `first` gives what it follows, so after a whole number it gives one.
    let first = verdict("Expr.first", &predicate(Representative::Int64), &[]);
    let refusal = first
        .allowance()
        .expect("offered, to be continued")
        .can_end()
        .expect_err("a whole number is no condition");
    assert_eq!(
        refusal.source(),
        RefusalSource::Rule(Rule::ConditionPredicateIsBoolean)
    );
    let boolean = verdict("Expr.first", &predicate(Representative::Boolean), &[]);
    assert!(boolean.allowance().expect("offered").can_end().is_ok());
    // A comparison gives a Boolean whatever it compares.
    let comparison = verdict(
        "Expr.gt",
        &predicate(Representative::Int64),
        &[("other", Representative::Int64)],
    );
    assert_eq!(result_of(&comparison), Some("Boolean"));
    assert!(comparison.allowance().expect("offered").can_end().is_ok());

    // Polars, by hand: a whole number as the predicate of a condition resolves
    // a schema, and its rows are refused.
    let frames = Frames::new(
        Representative::Int64,
        Representative::Int64,
        &DataType::Int64,
    );
    let condition = polars::prelude::when(col("k"))
        .then(lit(1))
        .otherwise(lit(NULL));
    assert_eq!(
        see(&frames.rows, &frames.empty, |frame| frame
            .select([condition.clone()])),
        Seen::Refused(Stage::Rows)
    );
}

#[test]
fn an_argument_decides_what_a_subtraction_gives() {
    let place = select_after(Representative::Date);
    // A date less a date is a duration; less a duration, a date.
    let dates = verdict("Expr.sub", &place, &[("other", Representative::Date)]);
    assert_eq!(result_of(&dates), Some("Duration(us)"), "{dates:?}");
    let duration = verdict("Expr.sub", &place, &[("other", Representative::Duration)]);
    assert_eq!(result_of(&duration), Some("Date"), "{duration:?}");
    let text = verdict("Expr.sub", &place, &[("other", Representative::String)]);
    assert!(text.refusal().is_some(), "{text:?}");
    // Without an argument the method is offered where some dtype fits.
    let undecided = verdict("Expr.sub", &place, &[]);
    assert!(undecided.is_allowed());
    assert_eq!(result_of(&undecided), None);
    assert!(
        verdict("Expr.sub", &select_after(Representative::String), &[])
            .refusal()
            .is_some()
    );
    // Polars, by hand.
    assert_eq!(
        selected(
            Representative::Date,
            Representative::Date,
            col("a") - col("b")
        ),
        Seen::Ran(DataType::Duration(polars::prelude::TimeUnit::Microseconds))
    );
    assert_eq!(
        selected(
            Representative::Date,
            Representative::Duration,
            col("a") - col("b")
        ),
        Seen::Ran(DataType::Date)
    );
}

#[test]
fn the_entries_a_place_offers_are_those_that_can_follow_what_it_has() {
    let listed = offers(&select_after(Representative::Datetime));
    let allowed = |id: &str| {
        listed
            .iter()
            .find(|offer| offer.entry().id().as_str() == id)
            .unwrap_or_else(|| panic!("{id} is not listed"))
            .verdict()
            .is_allowed()
    };
    assert!(allowed("Expr.dt.date"));
    assert!(allowed("Expr.cum_count"));
    assert!(
        !allowed("Expr.dt.total_days"),
        "a datetime has no days to count"
    );
    // Not a method after a value, and no step stands in an expression.
    assert!(!allowed("polars.col"));
    assert!(
        listed
            .iter()
            .all(|offer| !offer.entry().id().as_str().starts_with("LazyFrame."))
    );

    let start = offers(&Place::new(Context::step(Outer::Select), After::Start));
    let at_start = |id: &str| {
        start
            .iter()
            .find(|offer| offer.entry().id().as_str() == id)
            .expect("listed")
            .verdict()
            .is_allowed()
    };
    assert!(at_start("polars.col") && at_start("polars.lit") && at_start("polars.when"));
    assert!(!at_start("Expr.cum_sum") && !at_start("When.then"));
    assert!(!at_start("polars.element"));
}

#[test]
fn a_condition_link_is_offered_only_where_the_condition_has_come_to() {
    let place = |state| Place::new(Context::step(Outer::Select), After::Condition(state));
    let allowed = |id: &str, state| verdict(id, &place(state), &[]).is_allowed();
    assert!(allowed("When.then", ConditionState::When));
    assert!(!allowed("Then.otherwise", ConditionState::When));
    assert!(allowed("Then.otherwise", ConditionState::Then));
    assert!(allowed("Then.when", ConditionState::Then));
    assert!(allowed("ChainedWhen.then", ConditionState::ChainedWhen));
    assert!(allowed(
        "ChainedThen.otherwise",
        ConditionState::ChainedThen
    ));
    assert!(!allowed("Expr.cum_sum", ConditionState::Then));
}

#[test]
fn options_that_polars_refuses_together_are_refused_with_its_reason() {
    let join = entry("LazyFrame.join");
    assert!(options(join, &[("how", "inner"), ("validate", "1:1")]).is_ok());
    let refusal = options(join, &[("validate", "1:1"), ("how", "semi")])
        .expect_err("a semi join cannot be validated");
    assert_eq!(refusal.source(), RefusalSource::Polars(Stage::Schema));
    assert!(refusal.reason().contains("SEMI"), "{}", refusal.reason());
    // Polars, by hand.
    let frames = Frames::new(
        Representative::Int64,
        Representative::Int64,
        &DataType::Int64,
    );
    let semi = frames
        .rows
        .clone()
        .lazy()
        .join_builder()
        .with(frames.rows.clone().lazy())
        .left_on(vec![col("u")])
        .right_on(vec![col("u")])
        .how(polars::prelude::JoinType::Semi)
        .validate(polars::prelude::JoinValidation::OneToOne)
        .finish();
    assert!(semi.and_then(|mut frame| frame.collect_schema()).is_err());
}

/// What Polars does with `filter(predicate)` on the sample table whose column
/// `a` has the dtype `receiver`.
fn filtered(receiver: Representative, predicate: Expr) -> Seen {
    let frames = Frames::new(receiver, Representative::Int64, &DataType::Int64);
    see(&frames.rows, &frames.empty, |frame| {
        frame.filter(predicate.clone())
    })
}

fn filter_after(dtype: Representative) -> Place {
    Place::new(Context::step(Outer::Filter), after(dtype))
}

/// Asserts that `id` is offered after `receiver` in a filter's predicate, as a
/// method to be continued: it gives `shape`, and the chain may not end there,
/// since a predicate ends `Boolean`.
fn assert_stands_in_the_middle_of_a_filter_predicate(
    id: &str,
    receiver: Representative,
    shape: Shape,
) {
    let offered = verdict(id, &filter_after(receiver), &[]);
    let allowance = offered
        .allowance()
        .unwrap_or_else(|| panic!("{id} after {receiver}: {offered:?}"));
    assert_eq!(allowance.shape(), shape, "{id} after {receiver}");
    let refusal = allowance
        .can_end()
        .expect_err("a filter's predicate must end Boolean, and this is not one");
    assert!(
        matches!(refusal.source(), RefusalSource::Rule(_)),
        "{id} after {receiver}: {refusal:?}"
    );
    assert!(
        refusal.reason().contains("Boolean"),
        "{id} after {receiver}: {}",
        refusal.reason()
    );
}

#[test]
fn dt_date_after_a_datetime_may_stand_in_the_middle_of_a_filter_predicate() {
    // Polars, by hand: the date of a timestamp, tested for a value, filters.
    assert!(matches!(
        filtered(Representative::Datetime, col("a").dt().date().is_not_null()),
        Seen::Ran(_)
    ));
    assert_stands_in_the_middle_of_a_filter_predicate(
        "Expr.dt.date",
        Representative::Datetime,
        Shape::Many,
    );
}

#[test]
fn cum_sum_after_a_whole_number_may_stand_in_the_middle_of_a_filter_predicate() {
    // Polars, by hand: a running sum compared with a number filters.
    assert!(matches!(
        filtered(Representative::Int64, col("a").cum_sum(false).gt(2)),
        Seen::Ran(_)
    ));
    assert_stands_in_the_middle_of_a_filter_predicate(
        "Expr.cum_sum",
        Representative::Int64,
        Shape::Many,
    );
}

#[test]
fn sum_after_a_whole_number_may_stand_in_the_middle_of_a_filter_predicate() {
    // Polars, by hand: a total compared with a number filters.
    assert!(matches!(
        filtered(Representative::Int64, col("a").sum().gt(2)),
        Seen::Ran(_)
    ));
    assert_stands_in_the_middle_of_a_filter_predicate(
        "Expr.sum",
        Representative::Int64,
        Shape::One,
    );
}

#[test]
fn len_after_a_whole_number_may_stand_in_the_middle_of_a_filter_predicate() {
    // Polars, by hand: a count compared with a number filters.
    assert!(matches!(
        filtered(Representative::Int64, col("a").len().gt(0)),
        Seen::Ran(_)
    ));
    assert_stands_in_the_middle_of_a_filter_predicate(
        "Expr.len",
        Representative::Int64,
        Shape::One,
    );
}

#[test]
fn the_methods_that_may_stand_in_a_filter_predicate_are_offered_in_the_values_of_its_conditions() {
    // `filter(when(f).then(a.cum_sum() > 2).otherwise(true))`: the value of a
    // condition in a filter is continued to a Boolean as the predicate is.
    // Polars, by hand.
    let condition = |predicate: Expr| {
        polars::prelude::when(col("f"))
            .then(predicate)
            .otherwise(lit(true))
    };
    for (receiver, predicate) in [
        (Representative::Datetime, col("a").dt().date().is_not_null()),
        (Representative::Int64, col("a").cum_sum(false).gt(2)),
        (Representative::Int64, col("a").sum().gt(2)),
        (Representative::Int64, col("a").len().gt(0)),
    ] {
        assert!(
            matches!(filtered(receiver, condition(predicate)), Seen::Ran(_)),
            "{receiver}"
        );
    }

    let place = |dtype| {
        Place::new(
            Context::inside(Outer::Filter, Nested::ConditionValue),
            after(dtype),
        )
    };
    for (id, receiver) in [
        ("Expr.dt.date", Representative::Datetime),
        ("Expr.cum_sum", Representative::Int64),
        ("Expr.sum", Representative::Int64),
        ("Expr.len", Representative::Int64),
    ] {
        let offered = verdict(id, &place(receiver), &[]);
        assert!(offered.is_allowed(), "{id} after {receiver}: {offered:?}");
    }
}
