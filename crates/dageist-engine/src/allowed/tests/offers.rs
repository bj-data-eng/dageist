//! Offers against Polars' documented behavior, each result confirmed by
//! running Polars on its own API.

use polars::prelude::{
    DataType, Expr, IntoLazy, NULL, SortOptions, WindowMapping, col, element, lit,
};
use std::sync::Arc;

use crate::allowed::dtype::ResultDtype;
use crate::allowed::offers::RefusalSource;
use crate::allowed::{
    After, Chosen, ConditionState, Context, Ending, Nested, Outer, Place, Position, Representative,
    Rule, Shape, Stage, Unknown, Verdict, offer, offers, options,
    rule_forbids_a_key_known_not_elementwise, rule_forbids_a_repeated_pair_of_key_names,
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
    offer(
        entry(id),
        place,
        &Chosen::default().with_arguments(arguments),
    )
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
fn a_repeated_pair_of_key_names_is_refused_by_a_rule_only_in_a_joins_own_keys() {
    for context in [
        Context::step(Outer::JoinLeft),
        Context::step(Outer::JoinRight),
    ] {
        let refusal = rule_forbids_a_repeated_pair_of_key_names(context).expect_err("a key");
        assert_eq!(
            refusal.source(),
            RefusalSource::Rule(Rule::JoinKeyNamesAreNotRepeated),
            "{context}"
        );
    }
    // A key inside another expression is not one of the join's keys, and no
    // other position of a step has keys that are paired.
    for context in [
        Context::inside(Outer::JoinLeft, Nested::ConditionValue),
        Context::inside(Outer::JoinRight, Nested::PartitionBy),
        Context::step(Outer::Select),
        Context::step(Outer::Sort),
    ] {
        assert!(
            rule_forbids_a_repeated_pair_of_key_names(context).is_ok(),
            "{context}"
        );
    }

    // Polars, by hand: the same pair of names twice is refused when the join is
    // planned, and a name repeated on one side only is not.
    let frames = Frames::new(
        Representative::Int64,
        Representative::Int64,
        &DataType::Int64,
    );
    let planned = |left_on: Vec<Expr>, right_on: Vec<Expr>| {
        frames
            .rows
            .clone()
            .lazy()
            .join_builder()
            .with(frames.other_rows.clone().lazy())
            .left_on(left_on)
            .right_on(right_on)
            .finish()
            .and_then(|mut frame| frame.collect_schema())
    };
    assert!(
        planned(
            vec![col("k"), col("k").sum()],
            vec![col("rk"), col("rk").sum()]
        )
        .is_err()
    );
    assert!(planned(vec![col("k"), col("k").sum()], vec![col("rk"), col("ra")]).is_ok());
}

#[test]
fn a_key_with_a_single_value_beside_a_column_is_refused_by_a_rule_only_in_a_joins_own_keys() {
    for context in [
        Context::step(Outer::JoinLeft),
        Context::step(Outer::JoinRight),
    ] {
        let refusal = rule_forbids_a_key_known_not_elementwise(context).expect_err("a key");
        assert_eq!(
            refusal.source(),
            RefusalSource::Rule(Rule::KeyIsElementwise),
            "{context}"
        );
    }
    // Polars accepts a window whose key holds a single value beside a column,
    // in every position that holds a window, and a sort key.
    for context in [
        Context::inside(Outer::Select, Nested::PartitionBy),
        Context::inside(Outer::Select, Nested::OrderBy),
        Context::inside(Outer::Filter, Nested::PartitionBy),
        Context::inside(Outer::Filter, Nested::OrderBy),
        Context::inside(Outer::Sort, Nested::PartitionBy),
        Context::step(Outer::Select),
        Context::step(Outer::Sort),
        Context::inside(Outer::JoinLeft, Nested::ConditionValue),
        Context::inside(Outer::Select, Nested::ConditionPredicate),
    ] {
        assert!(
            rule_forbids_a_key_known_not_elementwise(context).is_ok(),
            "{context}"
        );
    }

    // Polars, by hand: a key that adds an aggregate to a column is
    // refused when the join is planned, and one that adds another column to it
    // is not.
    let frames = Frames::new(
        Representative::Int64,
        Representative::Int64,
        &DataType::Int64,
    );
    let planned = |key: Expr| {
        frames
            .rows
            .clone()
            .lazy()
            .join_builder()
            .with(frames.other_rows.clone().lazy())
            .left_on(vec![key])
            .right_on(vec![col("rk")])
            .finish()
            .and_then(|mut frame| frame.collect_schema())
    };
    let mixed = col("k") + col("u").max();
    let reason = planned(mixed).expect_err("not elementwise").to_string();
    assert!(reason.contains("elementwise"), "{reason}");
    assert!(planned(col("k") + col("u")).is_ok());
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
    assert!(sum.can_end().is_allowed());
    let running = verdict("Expr.cum_sum", &pivot(Shape::Many), &[]);
    let running = running
        .allowance()
        .expect("a running sum is offered to be continued");
    assert_eq!(running.shape(), Shape::Many);
    let refusal = running
        .can_end()
        .refusal()
        .expect("it cannot end the aggregate");
    assert_eq!(
        refusal.source(),
        RefusalSource::Rule(Rule::PivotAggregateGivesOneValue)
    );
    // A chain that has given one value keeps giving one.
    let one = verdict("Expr.cum_sum", &pivot(Shape::One), &[]);
    assert!(one.allowance().expect("offered").can_end().is_allowed());

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

/// The place of what follows a value of `dtype` in an expression that stands in
/// `positions`, the outermost first.
fn deep_place(positions: &[Position], dtype: Representative) -> Place {
    Place::new(
        Context::from_positions(positions).expect("a position of a step comes first"),
        after(dtype),
    )
}

#[test]
fn a_key_of_a_window_inside_the_value_of_a_filters_condition_need_not_be_boolean() {
    // `filter(when(p).then(x.over(key)))`: the key is judged as a key, not as
    // the value of the condition that the filter needs to be Boolean.
    let key = deep_place(
        &[
            Position::Outer(Outer::Filter),
            Position::Nested(Nested::ConditionValue),
            Position::Nested(Nested::PartitionBy),
        ],
        Representative::Int64,
    );
    let offered = verdict("Expr.cum_sum", &key, &[]);
    let ending = offered.allowance().expect("offered").can_end();
    assert!(ending.is_allowed(), "{ending:?}");
}

#[test]
fn a_predicate_inside_the_value_of_a_condition_must_be_boolean() {
    // `select(when(p).then(when(q).then(..)))`: `q` is a predicate, though the
    // place it stands in is also inside a value.
    let predicate = deep_place(
        &[
            Position::Outer(Outer::Select),
            Position::Nested(Nested::ConditionValue),
            Position::Nested(Nested::ConditionPredicate),
        ],
        Representative::Int64,
    );
    let offered = verdict("Expr.cum_sum", &predicate, &[]);
    let refusal = offered
        .allowance()
        .expect("offered, to be continued")
        .can_end()
        .refusal()
        .expect("a whole number is no condition");
    assert_eq!(
        refusal.source(),
        RefusalSource::Rule(Rule::ConditionPredicateIsBoolean)
    );
}

/// What is offered at `place` for the entry `id` that gives `dtype`, by what
/// the call decides.
fn giving(id: &str, place: &Place, dtype: Representative) -> Verdict {
    offer(entry(id), place, &Chosen::default().giving(dtype))
}

#[test]
fn a_start_given_the_dtype_of_its_column_gives_it() {
    let start = Place::new(Context::step(Outer::Select), After::Start);
    let unknown = verdict("polars.col", &start, &[]);
    assert_eq!(result_of(&unknown), None);
    for dtype in [Representative::Int64, Representative::Datetime] {
        let given = giving("polars.col", &start, dtype);
        assert_eq!(result_of(&given), Some(dtype.name()), "{given:?}");
        assert_eq!(given.allowance().expect("offered").shape(), Shape::Many);
    }
    // A literal is one value.
    let literal = giving("polars.lit", &start, Representative::Int8);
    assert_eq!(result_of(&literal), Some("Int8"), "{literal:?}");
    assert_eq!(literal.allowance().expect("offered").shape(), Shape::One);
}

#[test]
fn a_column_that_is_not_boolean_cannot_end_a_filters_predicate() {
    let start = Place::new(Context::step(Outer::Filter), After::Start);
    let number = giving("polars.col", &start, Representative::Int64);
    let refusal = number
        .allowance()
        .expect("offered, to be continued")
        .can_end()
        .refusal()
        .expect("a whole number is no predicate");
    assert_eq!(
        refusal.source(),
        RefusalSource::Rule(Rule::FilterPredicateIsBoolean)
    );
    let boolean = giving("polars.col", &start, Representative::Boolean);
    assert!(boolean.allowance().expect("offered").can_end().is_allowed());
}

#[test]
fn a_literal_of_text_cannot_end_a_conditions_predicate() {
    let start = Place::new(
        Context::inside(Outer::Select, Nested::ConditionPredicate),
        After::Start,
    );
    let text = giving("polars.lit", &start, Representative::String);
    let refusal = text
        .allowance()
        .expect("offered, to be continued")
        .can_end()
        .refusal()
        .expect("text is no condition");
    assert_eq!(
        refusal.source(),
        RefusalSource::Rule(Rule::ConditionPredicateIsBoolean)
    );
}

#[test]
fn the_values_of_a_condition_that_ends_in_otherwise_give_the_dtype_it_is_told() {
    let ended = Place::new(
        Context::step(Outer::Filter),
        After::Condition(ConditionState::Then),
    );
    let number = giving("Then.otherwise", &ended, Representative::Int64);
    assert_eq!(result_of(&number), Some("Int64"), "{number:?}");
    let refusal = number
        .allowance()
        .expect("offered")
        .can_end()
        .refusal()
        .expect("a whole number is no predicate");
    assert_eq!(
        refusal.source(),
        RefusalSource::Rule(Rule::FilterPredicateIsBoolean)
    );
    let boolean = giving("Then.otherwise", &ended, Representative::Boolean);
    assert!(boolean.allowance().expect("offered").can_end().is_allowed());
}

#[test]
fn a_start_whose_arguments_decide_whether_it_ends_is_not_known_until_its_dtype_is_given() {
    let ends = |verdict: &Verdict| verdict.allowance().expect("offered").can_end().clone();
    // A predicate needs a Boolean, which the column's dtype decides.
    let filter = Place::new(Context::step(Outer::Filter), After::Start);
    assert_eq!(
        ends(&verdict("polars.col", &filter, &[])),
        Ending::NotKnown(Unknown::TheStartsDtype)
    );
    // An untyped literal cannot end a join key's condition value, and a typed
    // one can: the table was measured with an untyped one.
    let key = Place::new(
        Context::inside(Outer::JoinLeft, Nested::ConditionValue),
        After::Start,
    );
    assert_eq!(
        ends(&verdict("polars.lit", &key, &[])),
        Ending::NotKnown(Unknown::TheStartsDtype)
    );
    assert_eq!(
        ends(&giving("polars.lit", &key, Representative::Int8)),
        Ending::Allowed
    );
    // In a pivot's aggregate, `element` stands for the values of a group.
    let predicate = Place::new(
        Context::inside(Outer::PivotAggregate, Nested::ConditionPredicate),
        After::Start,
    );
    assert_eq!(
        ends(&verdict("polars.element", &predicate, &[])),
        Ending::NotKnown(Unknown::TheStartsDtype)
    );
    assert_eq!(
        ends(&giving(
            "polars.element",
            &predicate,
            Representative::Boolean
        )),
        Ending::Allowed
    );
}

#[test]
fn a_start_that_its_arguments_do_not_decide_ends_without_its_dtype() {
    // Any column ends a select's item.
    let select = Place::new(Context::step(Outer::Select), After::Start);
    let offered = verdict("polars.col", &select, &[]);
    assert_eq!(
        offered.allowance().expect("offered").can_end(),
        &Ending::Allowed
    );
}

#[test]
fn a_method_whose_result_the_arguments_decide_is_not_known_to_end_a_predicate() {
    // A subtraction gives what its argument decides, so without it nothing says
    // whether a filter's predicate may end there.
    let place = Place::new(Context::step(Outer::Filter), after(Representative::Date));
    let undecided = verdict("Expr.sub", &place, &[]);
    assert_eq!(result_of(&undecided), None);
    assert_eq!(
        undecided.allowance().expect("offered").can_end(),
        &Ending::NotKnown(Unknown::TheResultsDtype)
    );
    // Where no rule needs a Boolean, the result does not matter.
    let select = verdict("Expr.sub", &select_after(Representative::Date), &[]);
    assert_eq!(
        select.allowance().expect("offered").can_end(),
        &Ending::Allowed
    );
}

#[test]
fn a_method_is_offered_with_the_options_chosen_and_refused_for_one_it_does_not_have() {
    let place = select_after(Representative::Int64);
    let chosen = |options: &'static [(&'static str, &'static str)]| {
        offer(
            entry("Expr.cum_sum"),
            &place,
            &Chosen::default().with_options(options),
        )
    };
    let reversed = chosen(&[("reverse", "true")]);
    assert_eq!(result_of(&reversed), Some("Int64"), "{reversed:?}");
    let refusal = chosen(&[("backwards", "true")])
        .refusal()
        .cloned()
        .expect("cum_sum has no such option");
    assert_eq!(refusal.source(), RefusalSource::Structure);
}

/// What the method `id` gives after `receiver` at a `select` item, with the
/// options `chosen` set, and for a window the partition it is measured with.
fn given_options(id: &str, receiver: Representative, chosen: &[(&str, &str)]) -> Option<String> {
    let partitioned = [("partition_by", Representative::Int64)];
    let arguments: &[(&str, Representative)] = if id == "Expr.over" { &partitioned } else { &[] };
    let verdict = offer(
        entry(id),
        &select_after(receiver),
        &Chosen::default()
            .with_arguments(arguments)
            .with_options(chosen),
    );
    verdict
        .allowance()
        .unwrap_or_else(|| panic!("{id} after {receiver} is offered: {verdict:?}"))
        .result()
        .map(|result| result.name().to_owned())
}

#[test]
fn a_window_that_joins_gives_a_list_and_the_other_strategies_what_it_follows() {
    let over = |receiver, strategy| {
        given_options("Expr.over", receiver, &[("mapping_strategy", strategy)])
    };
    assert_eq!(
        over(Representative::Int64, "join").as_deref(),
        Some("List(Int64)")
    );
    for strategy in ["group_to_rows", "explode"] {
        assert_eq!(
            over(Representative::Int64, strategy).as_deref(),
            Some("Int64"),
            "{strategy}"
        );
    }
    // The option table was measured on a whole number, and a list of what it
    // follows is what a window that joins gives after any dtype.
    assert_eq!(
        over(Representative::String, "join").as_deref(),
        Some("List(String)")
    );
    assert_eq!(
        over(Representative::String, "group_to_rows").as_deref(),
        Some("String")
    );
}

#[test]
fn a_window_that_joins_after_one_value_is_not_known_to_give_a_list() {
    // The option table was measured on a column. After an aggregate, a window
    // that joins gives the value again, and Polars says so.
    let place = Place::new(
        Context::step(Outer::Select),
        After::Value {
            dtype: Representative::Int64,
            shape: Shape::One,
        },
    );
    let over = |options: &'static [(&'static str, &'static str)]| {
        offer(
            entry("Expr.over"),
            &place,
            &Chosen::default()
                .with_arguments(&[("partition_by", Representative::Int64)])
                .with_options(options),
        )
    };
    let joined = over(&[("mapping_strategy", "join")]);
    assert_eq!(result_of(&joined), None, "{joined:?}");
    let grouped = over(&[("mapping_strategy", "group_to_rows")]);
    assert_eq!(result_of(&grouped), Some("Int64"), "{grouped:?}");

    let frames = Frames::new(
        Representative::Int64,
        Representative::Int64,
        &DataType::Int64,
    );
    let mut frame = frames.rows.clone().lazy().select([col("a")
        .sum()
        .over_with_options(Some([col("k")]), None, WindowMapping::Join)
        .expect("a window")]);
    assert_eq!(
        frame.collect_schema().expect("a schema").get("a"),
        Some(&DataType::Int64)
    );
}

/// What `Expr.over` gives after a whole number at a `select` item, given the
/// arguments `arguments` and the options `chosen`, with nothing else.
fn given_window(arguments: &[(&str, Representative)], chosen: &[(&str, &str)]) -> Option<String> {
    let verdict = offer(
        entry("Expr.over"),
        &select_after(Representative::Int64),
        &Chosen::default()
            .with_arguments(arguments)
            .with_options(chosen),
    );
    verdict
        .allowance()
        .unwrap_or_else(|| panic!("a window is offered: {verdict:?}"))
        .result()
        .map(|result| result.name().to_owned())
}

/// What Polars gives for `a` over a window ordered by `k` and given no
/// `partition_by`, run on its own API.
fn polars_window_ordered_by_k(options: SortOptions, mapping: WindowMapping) -> DataType {
    let frames = Frames::new(
        Representative::Int64,
        Representative::Int64,
        &DataType::Int64,
    );
    let mut frame = frames.rows.clone().lazy().select([col("a")
        .over_with_options(None::<Vec<Expr>>, Some((vec![col("k")], options)), mapping)
        .expect("a window with an order and no partition")]);
    frame
        .collect_schema()
        .expect("a schema")
        .get("a")
        .expect("a column")
        .clone()
}

#[test]
fn a_window_ordered_without_a_partition_gives_what_it_gives_with_one() {
    // Polars gives the window the same dtype, whatever its flags and strategy,
    // with no `partition_by` as with one: the rows are one group, and `join`
    // gives each row the list of the group.
    let ordered = [("order_by", Representative::Int64)];
    let sorted = |descending, nulls_last| SortOptions {
        descending,
        nulls_last,
        maintain_order: true,
        ..SortOptions::default()
    };
    let cases: [(&[(&str, &str)], SortOptions, WindowMapping, &str); 7] = [
        (
            &[],
            sorted(false, false),
            WindowMapping::GroupsToRows,
            "Int64",
        ),
        (
            &[("nulls_last", "true")],
            sorted(false, true),
            WindowMapping::GroupsToRows,
            "Int64",
        ),
        (
            &[("descending", "true")],
            sorted(true, false),
            WindowMapping::GroupsToRows,
            "Int64",
        ),
        (
            &[("descending", "true"), ("nulls_last", "true")],
            sorted(true, true),
            WindowMapping::GroupsToRows,
            "Int64",
        ),
        (
            &[("mapping_strategy", "join")],
            sorted(false, false),
            WindowMapping::Join,
            "List(Int64)",
        ),
        (
            &[("mapping_strategy", "explode")],
            sorted(false, false),
            WindowMapping::Explode,
            "Int64",
        ),
        (
            &[("nulls_last", "true"), ("mapping_strategy", "join")],
            sorted(false, true),
            WindowMapping::Join,
            "List(Int64)",
        ),
    ];
    for (chosen, options, mapping, expected) in cases {
        let polars = polars_window_ordered_by_k(options, mapping);
        assert_eq!(
            Representative::of(&polars).map(|class| class.to_string()),
            Some(expected.to_owned()),
            "Polars, {chosen:?}"
        );
        assert_eq!(
            given_window(&ordered, chosen).as_deref(),
            Some(expected),
            "the tables, {chosen:?}"
        );
    }
}

#[test]
fn a_window_with_nothing_to_group_or_order_by_is_refused_by_polars_whatever_its_options() {
    // Only the flags and the strategy, which mean nothing without an order,
    // are no window: Polars refuses it, so there is no dtype to carry.
    for mapping in [
        WindowMapping::GroupsToRows,
        WindowMapping::Join,
        WindowMapping::Explode,
    ] {
        let refused = col("a")
            .over_with_options(None::<Vec<Expr>>, None, mapping)
            .expect_err("Polars refuses a window with neither");
        assert!(
            refused
                .to_string()
                .contains("At least one of `partition_by`"),
            "{refused}"
        );
    }
    for chosen in [
        &[("nulls_last", "true")][..],
        &[("descending", "true")],
        &[("mapping_strategy", "join")],
    ] {
        assert_eq!(given_window(&[], chosen), None, "{chosen:?}");
    }
}

#[test]
fn a_window_without_a_partition_is_not_known_to_give_a_list_for_a_joining_strategy() {
    // The option table measured a window with `partition_by`, and a window
    // without one is not grouped.
    let place = select_after(Representative::Int64);
    let joined = offer(
        entry("Expr.over"),
        &place,
        &Chosen::default().with_options(&[("mapping_strategy", "join")]),
    );
    assert_eq!(result_of(&joined), None, "{joined:?}");
}

#[test]
fn an_option_set_to_its_default_is_not_a_choice() {
    let chosen = [
        ("descending", "false"),
        ("nulls_last", "false"),
        ("mapping_strategy", "join"),
    ];
    assert_eq!(
        given_options("Expr.over", Representative::Int64, &chosen).as_deref(),
        Some("List(Int64)")
    );
}

#[test]
fn options_whose_lines_give_different_results_leave_the_result_unknown() {
    // Each of the three pairs is measured: the two flags together give what
    // `over` follows, and each with a window that joins gives a list.
    let chosen = [
        ("descending", "true"),
        ("nulls_last", "true"),
        ("mapping_strategy", "join"),
    ];
    assert_eq!(
        given_options("Expr.over", Representative::Int64, &chosen),
        None
    );
}

#[test]
fn a_duration_in_fractional_days_is_a_float() {
    let days = |fractional| {
        given_options(
            "Expr.dt.total_days",
            Representative::Duration,
            &[("fractional", fractional)],
        )
    };
    assert_eq!(days("true").as_deref(), Some("Float64"));
    assert_eq!(days("false").as_deref(), Some("Int64"));
}

#[test]
fn an_option_that_does_not_change_the_result_leaves_what_the_method_gives() {
    let first = |receiver| given_options("Expr.first", receiver, &[("ignore_nulls", "true")]);
    assert_eq!(first(Representative::Int64).as_deref(), Some("Int64"));
    assert_eq!(first(Representative::String).as_deref(), Some("String"));
}

#[test]
fn a_method_gives_what_the_tables_say_whatever_it_is_told() {
    let place = select_after(Representative::Int64);
    let told = giving("Expr.cum_sum", &place, Representative::String);
    assert_eq!(result_of(&told), Some("Int64"), "{told:?}");
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
        .refusal()
        .expect("a whole number is no condition");
    assert_eq!(
        refusal.source(),
        RefusalSource::Rule(Rule::ConditionPredicateIsBoolean)
    );
    let boolean = verdict("Expr.first", &predicate(Representative::Boolean), &[]);
    assert!(boolean.allowance().expect("offered").can_end().is_allowed());
    // A comparison gives a Boolean whatever it compares.
    let comparison = verdict(
        "Expr.gt",
        &predicate(Representative::Int64),
        &[("other", Representative::Int64)],
    );
    assert_eq!(result_of(&comparison), Some("Boolean"));
    assert!(
        comparison
            .allowance()
            .expect("offered")
            .can_end()
            .is_allowed()
    );

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
        .refusal()
        .expect("a filter's predicate must end Boolean, and this is not one");
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

#[test]
fn a_fill_value_decides_what_a_shift_gives() {
    // Polars, by hand: a column of whole numbers shifted and filled with text
    // is text, not the whole numbers it was.
    assert_eq!(
        selected(
            Representative::UInt32,
            Representative::String,
            col("a").shift_and_fill(lit(1), lit(1).cast(DataType::String)),
        ),
        Seen::Ran(DataType::String)
    );
    let place = select_after(Representative::UInt32);
    let filled = verdict(
        "Expr.shift",
        &place,
        &[("fill_value", Representative::String)],
    );
    assert_eq!(result_of(&filled), Some("String"), "{filled:?}");
    // With nothing to fill with, the shift gives what it shifts.
    let plain = verdict("Expr.shift", &place, &[]);
    assert_eq!(result_of(&plain), Some("UInt32"), "{plain:?}");
}

#[test]
fn two_arguments_that_give_the_same_result_give_it() {
    // Polars, by hand: a shift by whole numbers of 32 bits and filled with
    // whole numbers of 64 bits gives the 64 bits of what it shifts.
    assert_eq!(
        selected(
            Representative::Int64,
            Representative::Int64,
            col("a").shift_and_fill(lit(1).cast(DataType::Int32), lit(1)),
        ),
        Seen::Ran(DataType::Int64)
    );
    let place = select_after(Representative::Int64);
    let given = verdict(
        "Expr.shift",
        &place,
        &[
            ("n", Representative::Int32),
            ("fill_value", Representative::Int64),
        ],
    );
    assert_eq!(result_of(&given), Some("Int64"), "{given:?}");
}

#[test]
fn two_arguments_that_give_different_results_leave_the_result_unknown() {
    // The tables measure each parameter with the other at its default, so what
    // the two give together is not in them: a `n` that leaves the dtype and a
    // `fill_value` of text that makes it text are not combined by guessing.
    let place = select_after(Representative::Int64);
    let given = verdict(
        "Expr.shift",
        &place,
        &[
            ("n", Representative::Int32),
            ("fill_value", Representative::String),
        ],
    );
    let allowance = given.allowance().expect("each argument fits alone");
    assert_eq!(allowance.result(), None, "{given:?}");
    // Each alone is known.
    for (parameter, dtype, expected) in [
        ("n", Representative::Int32, "Int64"),
        ("fill_value", Representative::String, "String"),
    ] {
        let alone = verdict("Expr.shift", &place, &[(parameter, dtype)]);
        assert_eq!(result_of(&alone), Some(expected), "{parameter}: {alone:?}");
    }
}
