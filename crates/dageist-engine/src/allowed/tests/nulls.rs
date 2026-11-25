//! What the tables and the offers say of the nulls, checks and conversions, against what Polars
//! does when its own API is called straight, which is the oracle of each test.

use polars::prelude::{DataType, Expr, FillNullStrategy, JoinType, LazyFrame, col};

use crate::allowed::dtype::{Inner, Representative, dtype_name};
use crate::allowed::offers::RefusalSource;
use crate::allowed::{
    After, Chosen, Context, Ending, Outer, Place, Shape, Stage, Verdict, offer, options,
};
use crate::catalog;

use super::oracle::{Seen, selected};
use super::sample::Frames;

fn entry(id: &str) -> &'static catalog::Entry {
    catalog::entry(id).unwrap_or_else(|| panic!("no entry {id}"))
}

fn at(context: Context, dtype: Representative, shape: Shape) -> Place {
    Place::new(context, After::Value { dtype, shape })
}

fn select_after(dtype: Representative) -> Place {
    at(Context::step(Outer::Select), dtype, Shape::Many)
}

fn result_of(verdict: &Verdict) -> Option<&str> {
    verdict
        .allowance()
        .and_then(|allowance| allowance.result())
        .map(|result| result.name())
}

/// Whether Polars refuses the join on `key` when it works out the schema of the join, with every
/// other part as the sample has it: `key` is cast to the dtype of the other table's key.
fn refuses_a_join_on(key: Expr) -> bool {
    let frames = Frames::new(
        Representative::Int64,
        Representative::Int64,
        &DataType::Int64,
    );
    let mut joined = frames
        .left(false)
        .join_builder()
        .with(frames.other(false))
        .left_on([key.cast(DataType::Int64)])
        .right_on([col("key")])
        .how(JoinType::Inner)
        .finish()
        .unwrap_or_else(|_| LazyFrame::default());
    crate::startup::ensure();
    joined.collect_schema().is_err()
}

fn strategy_of(word: &str) -> FillNullStrategy {
    match word {
        "forward" => FillNullStrategy::Forward(None),
        "backward" => FillNullStrategy::Backward(None),
        "min" => FillNullStrategy::Min,
        "max" => FillNullStrategy::Max,
        "mean" => FillNullStrategy::Mean,
        "zero" => FillNullStrategy::Zero,
        "one" => FillNullStrategy::One,
        other => panic!("{other} is no strategy"),
    }
}

#[test]
fn a_fill_by_a_strategy_is_offered_for_the_dtypes_polars_runs_it_on_and_gives_what_it_gives() {
    let cases = [
        (Representative::Int64, "mean"),
        (Representative::Int64, "zero"),
        (Representative::Int64, "forward"),
        (Representative::String, "forward"),
        (Representative::String, "min"),
        (Representative::String, "mean"),
        (Representative::String, "one"),
        (Representative::Boolean, "one"),
        (Representative::Boolean, "mean"),
        (Representative::Null, "forward"),
        (Representative::Null, "zero"),
        (Representative::List(Inner::Int64), "forward"),
        (Representative::List(Inner::Int64), "min"),
    ];
    for (receiver, word) in cases {
        let seen = selected(
            receiver,
            Representative::Int64,
            col("a").fill_null_with_strategy(strategy_of(word)),
        );
        let verdict = offer(
            entry("Expr.fill_null"),
            &select_after(receiver),
            &Chosen::default().with_options(&[("strategy", word)]),
        );
        match seen {
            Seen::Ran(dtype) => {
                assert_eq!(
                    result_of(&verdict),
                    Some(dtype_name(&dtype).as_str()),
                    "{receiver} {word}: {verdict:?}"
                );
            }
            Seen::Refused(stage) => {
                let refusal = verdict
                    .refusal()
                    .unwrap_or_else(|| panic!("{receiver} {word} is refused: {verdict:?}"));
                assert_eq!(
                    refusal.source(),
                    RefusalSource::Polars(stage),
                    "{receiver} {word}"
                );
            }
        }
    }
}

#[test]
fn a_fill_is_offered_with_a_strategy_and_no_value_and_with_a_value_and_no_strategy() {
    let place = select_after(Representative::Int64);
    let by_value = offer(
        entry("Expr.fill_null"),
        &place,
        &Chosen::default().with_arguments(&[("value", Representative::Int64)]),
    );
    assert_eq!(result_of(&by_value), Some("Int64"), "{by_value:?}");
    let by_strategy = offer(
        entry("Expr.fill_null"),
        &place,
        &Chosen::default().with_options(&[("strategy", "forward")]),
    );
    assert_eq!(result_of(&by_strategy), Some("Int64"), "{by_strategy:?}");
    // Nothing chosen is judged for some value that fits.
    assert!(offer(entry("Expr.fill_null"), &place, &Chosen::default()).is_allowed());
}

#[test]
fn a_strategy_that_some_dtype_runs_is_not_refused_when_no_dtype_is_asked_of() {
    // The options alone name no dtype: a strategy is refused only when Polars refuses it on all
    // of them, and `mean` runs on numbers.
    assert!(options(entry("Expr.fill_null"), &[("strategy", "mean")]).is_ok());
    assert!(options(entry("Expr.fill_null"), &[("strategy", "forward")]).is_ok());
}

#[test]
fn a_fill_by_a_strategy_that_looks_at_other_rows_is_not_elementwise_and_a_join_key_must_be() {
    let key = |word: &str| {
        offer(
            entry("Expr.fill_null"),
            &at(
                Context::step(Outer::JoinLeft),
                Representative::Int64,
                Shape::Many,
            ),
            &Chosen::default().with_options(&[("strategy", word)]),
        )
    };
    // Polars calls only a fill by zero or by one elementwise, and refuses the others.
    for word in ["forward", "backward", "min", "max", "mean"] {
        assert!(refuses_a_join_on(
            col("a").fill_null_with_strategy(strategy_of(word))
        ));
        let refusal = key(word)
            .refusal()
            .cloned()
            .unwrap_or_else(|| panic!("{word} is refused in a join's key"));
        assert!(
            refusal.reason().contains("elementwise"),
            "{word}: {refusal}"
        );
    }
    for word in ["zero", "one"] {
        assert!(!refuses_a_join_on(
            col("a").fill_null_with_strategy(strategy_of(word))
        ));
        assert!(key(word).is_allowed(), "{word}");
    }
    // A fill by a value is elementwise.
    assert!(!refuses_a_join_on(col("a").fill_null(col("b"))));
    let by_value = offer(
        entry("Expr.fill_null"),
        &at(
            Context::step(Outer::JoinLeft),
            Representative::Int64,
            Shape::Many,
        ),
        &Chosen::default().with_arguments(&[("value", Representative::Int64)]),
    );
    assert!(by_value.is_allowed(), "{by_value:?}");
}

#[test]
fn a_cast_gives_the_dtype_it_is_given_and_is_strict_unless_it_is_not_or_wraps() {
    let cast = |receiver: Representative, options: &[(&str, &str)]| {
        offer(
            entry("Expr.cast"),
            &select_after(receiver),
            &Chosen::default().with_options(options),
        )
    };
    // Text that is a duration converts to no number, and Polars refuses it as a strict cast.
    let strict = cast(Representative::String, &[("dtype", "UInt64")]);
    assert_eq!(
        strict.refusal().map(|refusal| refusal.source()),
        Some(RefusalSource::Polars(Stage::Rows))
    );
    assert_eq!(
        selected(
            Representative::String,
            Representative::Int64,
            col("a").strict_cast(DataType::UInt64),
        ),
        Seen::Refused(Stage::Rows)
    );
    // Not strict, it gives a null where a value does not convert, and wrapping does too.
    for options in [
        &[("dtype", "UInt64"), ("strict", "false")],
        &[("dtype", "UInt64"), ("wrap_numerical", "true")],
    ] {
        let lenient = cast(Representative::String, options);
        assert_eq!(
            result_of(&lenient),
            Some("UInt64"),
            "{options:?}: {lenient:?}"
        );
    }
    assert_eq!(
        selected(
            Representative::String,
            Representative::Int64,
            col("a").cast(DataType::UInt64),
        ),
        Seen::Ran(DataType::UInt64)
    );
    // A cast to a dtype that Polars gives nothing to cast to is refused, whatever the options.
    let list = cast(
        Representative::Int64,
        &[("dtype", "List(Int64)"), ("strict", "false")],
    );
    assert_eq!(
        list.refusal().map(|refusal| refusal.source()),
        Some(RefusalSource::Polars(Stage::Rows)),
        "{list:?}"
    );
}

#[test]
fn a_cast_given_no_dtype_gives_no_dtype_and_is_not_known_to_end_a_predicate() {
    // The dtype table measured a cast to the dtype it follows; what a cast gives is what it is
    // told, so until it is told nothing is said of it.
    let place = at(
        Context::step(Outer::Filter),
        Representative::Int64,
        Shape::Many,
    );
    let told_nothing = offer(entry("Expr.cast"), &place, &Chosen::default());
    assert_eq!(result_of(&told_nothing), None, "{told_nothing:?}");
    assert!(matches!(
        told_nothing
            .allowance()
            .map(|allowance| allowance.can_end()),
        Some(Ending::NotKnown(_))
    ));
    let boolean = offer(
        entry("Expr.cast"),
        &place,
        &Chosen::default().with_options(&[("dtype", "Boolean")]),
    );
    assert_eq!(result_of(&boolean), Some("Boolean"), "{boolean:?}");
}

#[test]
fn the_tests_of_nan_and_infinity_take_numbers_and_null_and_refuse_the_rest() {
    let checks = [
        ("Expr.is_nan", col("a").is_nan()),
        ("Expr.is_not_nan", col("a").is_not_nan()),
        ("Expr.is_finite", col("a").is_finite()),
        ("Expr.is_infinite", col("a").is_infinite()),
    ];
    for (id, expr) in checks {
        for receiver in [
            Representative::Int8,
            Representative::Float32,
            Representative::Decimal,
            Representative::String,
            Representative::Boolean,
            Representative::Date,
            Representative::Null,
        ] {
            let seen = selected(receiver, Representative::Int64, expr.clone());
            let verdict = offer(entry(id), &select_after(receiver), &Chosen::default());
            assert_eq!(
                verdict.is_allowed(),
                matches!(seen, Seen::Ran(_)),
                "{id} after {receiver}: {verdict:?} against {seen:?}"
            );
            if matches!(seen, Seen::Ran(_)) {
                assert_eq!(result_of(&verdict), Some("Boolean"), "{id} {receiver}");
            }
        }
    }
}

#[test]
fn a_count_of_nulls_is_one_value_of_the_index_type_and_has_nulls_is_one_boolean() {
    let place = select_after(Representative::String);
    let count = offer(entry("Expr.null_count"), &place, &Chosen::default());
    assert_eq!(result_of(&count), Some("UInt32"), "{count:?}");
    assert_eq!(
        count.allowance().map(|allowance| allowance.shape()),
        Some(Shape::One)
    );
    let has = offer(entry("Expr.has_nulls"), &place, &Chosen::default());
    assert_eq!(result_of(&has), Some("Boolean"), "{has:?}");
    assert_eq!(
        has.allowance().map(|allowance| allowance.shape()),
        Some(Shape::One)
    );
    assert_eq!(
        selected(
            Representative::String,
            Representative::Int64,
            col("a").null_count()
        ),
        Seen::Ran(DataType::UInt32)
    );
}

#[test]
fn the_tests_of_repetition_give_a_value_for_each_row_and_are_not_elementwise() {
    for id in [
        "Expr.is_duplicated",
        "Expr.is_unique",
        "Expr.is_first_distinct",
        "Expr.is_last_distinct",
    ] {
        let select = offer(
            entry(id),
            &select_after(Representative::String),
            &Chosen::default(),
        );
        assert_eq!(result_of(&select), Some("Boolean"), "{id}: {select:?}");
        assert_eq!(
            select.allowance().map(|allowance| allowance.shape()),
            Some(Shape::Many)
        );
        // A join's key must be elementwise, which the whole column is not.
        let key = offer(
            entry(id),
            &at(
                Context::step(Outer::JoinLeft),
                Representative::Int64,
                Shape::Many,
            ),
            &Chosen::default(),
        );
        assert!(
            key.refusal()
                .is_some_and(|refusal| refusal.reason().contains("elementwise")),
            "{id}: {key:?}"
        );
    }
    assert!(refuses_a_join_on(col("a").is_unique()));
}

#[test]
fn a_test_for_nan_of_a_number_is_a_boolean_that_may_end_a_filters_predicate() {
    let place = at(
        Context::step(Outer::Filter),
        Representative::Float64,
        Shape::Many,
    );
    let verdict = offer(entry("Expr.is_nan"), &place, &Chosen::default());
    assert_eq!(
        verdict.allowance().map(|allowance| allowance.can_end()),
        Some(&Ending::Allowed),
        "{verdict:?}"
    );
    let count = offer(
        entry("Expr.null_count"),
        &at(
            Context::step(Outer::Filter),
            Representative::Float64,
            Shape::Many,
        ),
        &Chosen::default(),
    );
    assert!(
        count
            .allowance()
            .is_some_and(|allowance| matches!(allowance.can_end(), Ending::Refused(_))),
        "a count of nulls is no Boolean: {count:?}"
    );
}

#[test]
fn an_array_may_be_replaced_by_an_array_given_what_replaces_it_that_polars_meets_it_with() {
    // What an array is replaced by cannot be another array beside a default, which Polars finds
    // no dtype for, so the dtype of what is replaced is not refused for that: with whole numbers
    // for what replaces it and for the default, Polars allows an array for it, and the tables,
    // which ask each dtype apart, say so.
    assert_eq!(
        selected(
            Representative::Array(Inner::Int64),
            Representative::Array(Inner::Int64),
            col("a").replace_strict(col("b"), col("k"), Some(col("k")), None::<DataType>),
        ),
        Seen::Ran(DataType::Int64)
    );
    let verdict = offer(
        entry("Expr.replace_strict"),
        &select_after(Representative::Array(Inner::Int64)),
        &Chosen::default().with_arguments(&[("old", Representative::Array(Inner::Int64))]),
    );
    assert!(verdict.allowance().is_some(), "{verdict:?}");
    // Beside a default of arrays, what replaces them is refused as Polars refuses it.
    assert_eq!(
        selected(
            Representative::Array(Inner::Int64),
            Representative::Array(Inner::Int64),
            col("a").replace_strict(col("a"), col("b"), Some(col("b")), None::<DataType>),
        ),
        Seen::Refused(Stage::Schema)
    );
}

#[test]
fn a_strict_replacement_converts_what_replaces_leniently_beside_a_default_and_strictly_without_one()
{
    let replace = |default: bool| {
        let values = col("a").replace_strict(
            col("a"),
            col("b"),
            default.then(|| col("b")),
            Some(DataType::UInt16),
        );
        selected(Representative::String, Representative::String, values)
    };
    // Text that is a duration reads as no number: Polars gives a null for it beside a default,
    // as a cast that is not strict does, and refuses it without one.
    assert_eq!(replace(true), Seen::Ran(DataType::UInt16));
    assert_eq!(replace(false), Seen::Refused(Stage::Rows));
    let asked = |arguments: &[(&str, Representative)]| {
        offer(
            entry("Expr.replace_strict"),
            &select_after(Representative::String),
            &Chosen::default()
                .with_arguments(arguments)
                .with_options(&[("return_dtype", "UInt16")]),
        )
    };
    let text = Representative::String;
    let with_a_default = asked(&[("old", text), ("new", text), ("default", text)]);
    assert!(with_a_default.allowance().is_some(), "{with_a_default:?}");
    // A default whose dtype is not known is a default all the same.
    let unclassed = offer(
        entry("Expr.replace_strict"),
        &select_after(Representative::String),
        &Chosen::default()
            .with_arguments(&[("old", text), ("new", text)])
            .with_unclassed(&["default"])
            .with_options(&[("return_dtype", "UInt16")]),
    );
    assert!(unclassed.allowance().is_some(), "{unclassed:?}");
    let without = asked(&[("old", text), ("new", text)]);
    assert_eq!(
        without.refusal().map(|refusal| refusal.source()),
        Some(RefusalSource::Polars(Stage::Rows)),
        "{without:?}"
    );
}
