//! The methods that fill, drop and count nulls and NaNs, test what a value is, compare with a
//! null as a value, and convert one dtype to another, built through the catalogue and run on
//! small tables, against the values their documentation gives, which are written here by hand.

use polars::prelude::{DataFrame, DataType, Expr, IntoLazy, col, df, lit};

use crate::catalog::{Argument, Arguments, BuildError};

use super::build::{choice, dtype_of, expression_arg, flag, floats, integers, method, none, run};

/// `entry` applied to the column `x` of `frame` and run: the column it gives.
fn run_on(frame: &DataFrame, id: &str, arguments: &Arguments) -> DataFrame {
    let expr = method(id, col("x"), arguments).expect("the method builds");
    run(frame.clone().lazy().select([expr.alias("out")]))
}

fn whole(value: i64) -> Argument {
    Argument::Integer(value)
}

fn number(value: i64) -> Argument {
    expression_arg(lit(value))
}

fn booleans(frame: &DataFrame) -> Vec<Option<bool>> {
    frame
        .column("out")
        .expect("a column")
        .bool()
        .expect("booleans")
        .iter()
        .collect()
}

/// Booleans from `of`, written as a list of `Some(bool)` and `None` for a null.
fn test_of(frame: &DataFrame, id: &str, arguments: &Arguments) -> Vec<Option<bool>> {
    booleans(&run_on(frame, id, arguments))
}

/// A column of whole numbers with a null before, between and after the values.
fn gaps() -> DataFrame {
    df!("x" => [None, Some(1_i64), None, None, Some(5), None]).unwrap()
}

fn fill(strategy: &str) -> Arguments {
    Arguments::new().with("strategy", choice(strategy))
}

#[test]
fn a_value_fills_the_nulls_of_a_column() {
    let frame = df!("x" => [Some(1_i64), None, Some(3)]).unwrap();
    let given = Arguments::new().with("value", number(0));
    let out = run_on(&frame, "Expr.fill_null", &given);
    assert_eq!(integers(&out, "out"), [Some(1), Some(0), Some(3)]);
}

#[test]
fn each_strategy_fills_the_nulls_as_its_name_says() {
    let frame = gaps();
    let filled =
        |strategy: &str| integers(&run_on(&frame, "Expr.fill_null", &fill(strategy)), "out");
    // The value before, the value after, the lowest, the highest, the mean of 1 and 5, zero and one.
    assert_eq!(
        filled("forward"),
        [None, Some(1), Some(1), Some(1), Some(5), Some(5)]
    );
    assert_eq!(
        filled("backward"),
        [Some(1), Some(1), Some(5), Some(5), Some(5), None]
    );
    assert_eq!(
        filled("min"),
        [Some(1), Some(1), Some(1), Some(1), Some(5), Some(1)]
    );
    assert_eq!(
        filled("max"),
        [Some(5), Some(1), Some(5), Some(5), Some(5), Some(5)]
    );
    assert_eq!(
        filled("mean"),
        [Some(3), Some(1), Some(3), Some(3), Some(5), Some(3)]
    );
    assert_eq!(
        filled("zero"),
        [Some(0), Some(1), Some(0), Some(0), Some(5), Some(0)]
    );
    assert_eq!(
        filled("one"),
        [Some(1), Some(1), Some(1), Some(1), Some(5), Some(1)]
    );
}

#[test]
fn a_limit_stops_a_forward_or_backward_fill_after_that_many_nulls_in_a_row() {
    let frame = gaps();
    let limited = |id: &str, given: Arguments| integers(&run_on(&frame, id, &given), "out");
    let one = Arguments::new().with("limit", whole(1));
    // The first of the two nulls after the 1 is filled, and the second is not.
    assert_eq!(
        limited("Expr.forward_fill", one.clone()),
        [None, Some(1), Some(1), None, Some(5), Some(5)]
    );
    assert_eq!(
        limited("Expr.backward_fill", one.clone()),
        [Some(1), Some(1), None, Some(5), Some(5), None]
    );
    let strategy_and_limit = fill("forward").with("limit", whole(1));
    assert_eq!(
        limited("Expr.fill_null", strategy_and_limit),
        [None, Some(1), Some(1), None, Some(5), Some(5)]
    );
    // Without a limit every null with a value before it is filled.
    assert_eq!(
        limited("Expr.forward_fill", none()),
        [None, Some(1), Some(1), Some(1), Some(5), Some(5)]
    );
    assert_eq!(
        limited("Expr.backward_fill", none()),
        [Some(1), Some(1), Some(5), Some(5), Some(5), None]
    );
}

#[test]
fn a_fill_takes_a_value_or_a_strategy_as_python_does_and_a_limit_only_for_forward_or_backward() {
    let refused = |given: Arguments| match method("Expr.fill_null", col("x"), &given) {
        Err(BuildError::Polars(error)) => error.to_string(),
        other => panic!("expected Polars' refusal, found {:?}", other.map(|_| ())),
    };
    let value = Arguments::new().with("value", number(0));
    assert!(refused(Arguments::new()).contains("must specify either a fill `value` or `strategy`"));
    assert!(
        refused(fill("forward").with("value", number(0)))
            .contains("cannot specify both `value` and `strategy`")
    );
    // Python refuses a limit beside a value, and beside every strategy but the two it bounds.
    for given in [
        value.with("limit", whole(1)),
        fill("min").with("limit", whole(1)),
        fill("mean").with("limit", whole(1)),
    ] {
        assert!(refused(given).contains("can only specify `limit`"));
    }
    for strategy in ["forward", "backward"] {
        let given = fill(strategy).with("limit", whole(1));
        assert!(
            method("Expr.fill_null", col("x"), &given).is_ok(),
            "{strategy}"
        );
    }
}

#[test]
fn a_limit_python_cannot_take_as_an_unsigned_32_bit_number_is_out_of_range() {
    for id in ["Expr.forward_fill", "Expr.backward_fill", "Expr.fill_null"] {
        for limit in [-1, 4_294_967_296] {
            let mut given = Arguments::new().with("limit", whole(limit));
            if id == "Expr.fill_null" {
                given = given.with("strategy", choice("forward"));
            }
            let error = method(id, col("x"), &given).unwrap_err();
            assert!(
                matches!(
                    error,
                    BuildError::OutOfRange {
                        parameter: "limit",
                        ..
                    }
                ),
                "{id} {limit}"
            );
        }
        let mut given = Arguments::new().with("limit", whole(4_294_967_295));
        if id == "Expr.fill_null" {
            given = given.with("strategy", choice("forward"));
        }
        assert!(method(id, col("x"), &given).is_ok(), "{id}");
    }
}

#[test]
fn nan_is_filled_and_dropped_apart_from_null() {
    let frame = df!("x" => [Some(1.0_f64), Some(f64::NAN), None, Some(3.0)]).unwrap();
    let given = Arguments::new().with("value", number(0));
    let filled = run_on(&frame, "Expr.fill_nan", &given);
    // The null stays a null, and the NaN becomes the value.
    assert_eq!(
        floats(&filled, "out"),
        [Some(1.0), Some(0.0), None, Some(3.0)]
    );
    let dropped = run_on(&frame, "Expr.drop_nans", &none());
    assert_eq!(floats(&dropped, "out"), [Some(1.0), None, Some(3.0)]);
}

#[test]
fn drop_nulls_leaves_the_values_that_are_not_null_and_so_fewer_rows() {
    let frame = df!("x" => [Some(1_i64), None, Some(3), None]).unwrap();
    let out = run_on(&frame, "Expr.drop_nulls", &none());
    assert_eq!(integers(&out, "out"), [Some(1), Some(3)]);
}

#[test]
fn the_tests_of_a_number_tell_nan_and_infinity_from_the_rest() {
    let frame = df!(
        "x" => [Some(1.0_f64), Some(f64::NAN), Some(f64::INFINITY), Some(f64::NEG_INFINITY), None]
    )
    .unwrap();
    let test = |id: &str| test_of(&frame, id, &none());
    assert_eq!(
        test("Expr.is_nan"),
        [Some(false), Some(true), Some(false), Some(false), None]
    );
    assert_eq!(
        test("Expr.is_not_nan"),
        [Some(true), Some(false), Some(true), Some(true), None]
    );
    // Neither infinity nor NaN is finite.
    assert_eq!(
        test("Expr.is_finite"),
        [Some(true), Some(false), Some(false), Some(false), None]
    );
    assert_eq!(
        test("Expr.is_infinite"),
        [Some(false), Some(false), Some(true), Some(true), None]
    );
}

#[test]
fn the_tests_of_repetition_look_at_the_whole_column_and_count_a_null_as_a_value() {
    let frame = df!("x" => [Some(1_i64), Some(2), Some(1), Some(3), None, None]).unwrap();
    let test = |id: &str| test_of(&frame, id, &none());
    // 1 occurs twice and null twice, 2 and 3 once each.
    let (yes, no) = (Some(true), Some(false));
    assert_eq!(test("Expr.is_duplicated"), [yes, no, yes, no, yes, yes]);
    assert_eq!(test("Expr.is_unique"), [no, yes, no, yes, no, no]);
    // The first and the last of each value.
    assert_eq!(test("Expr.is_first_distinct"), [yes, yes, no, yes, yes, no]);
    assert_eq!(test("Expr.is_last_distinct"), [no, yes, yes, yes, no, yes]);
}

#[test]
fn the_nulls_of_a_column_are_counted_and_told_apart_from_none() {
    let with = df!("x" => [Some(1_i64), None, Some(3), None]).unwrap();
    let without = df!("x" => [1_i64, 2]).unwrap();
    let count = run_on(&with, "Expr.null_count", &none());
    assert_eq!(integers_u32(&count), [2]);
    assert_eq!(
        integers_u32(&run_on(&without, "Expr.null_count", &none())),
        [0]
    );
    assert_eq!(test_of(&with, "Expr.has_nulls", &none()), [Some(true)]);
    assert_eq!(test_of(&without, "Expr.has_nulls", &none()), [Some(false)]);
    // The count is an index-sized number, and `has_nulls` gives one value that is a Boolean.
    let counted = method("Expr.null_count", col("i"), &none()).unwrap();
    assert_eq!(dtype_of(&counted), DataType::UInt32);
    let held = method("Expr.has_nulls", col("i"), &none()).unwrap();
    assert_eq!(dtype_of(&held), DataType::Boolean);
}

fn integers_u32(frame: &DataFrame) -> Vec<u32> {
    frame
        .column("out")
        .expect("a column")
        .u32()
        .expect("unsigned numbers")
        .iter()
        .map(|value| value.expect("a count"))
        .collect()
}

#[test]
fn the_sides_of_an_interval_that_are_closed_are_the_ones_that_hold_their_bound() {
    let frame = df!("x" => [1_i64, 2, 3, 4, 5]).unwrap();
    let between = |closed: Option<&str>| {
        let mut given = Arguments::new()
            .with("lower_bound", number(2))
            .with("upper_bound", number(4));
        if let Some(closed) = closed {
            given = given.with("closed", choice(closed));
        }
        test_of(&frame, "Expr.is_between", &given)
    };
    let (yes, no) = (Some(true), Some(false));
    assert_eq!(between(None), [no, yes, yes, yes, no]);
    assert_eq!(between(Some("both")), [no, yes, yes, yes, no]);
    assert_eq!(between(Some("left")), [no, yes, yes, no, no]);
    assert_eq!(between(Some("right")), [no, no, yes, yes, no]);
    assert_eq!(between(Some("none")), [no, no, yes, no, no]);
}

#[test]
fn two_numbers_are_close_within_the_tolerances_and_nan_is_equal_to_itself_only_when_asked() {
    // Polars' documentation: 1.5 and 1.55 are close within 0.1, 2.0 and 2.2 are not.
    let frame = df!(
        "x" => [1.5_f64, 2.0, 2.5],
        "y" => [1.55_f64, 2.2, 3.0],
    )
    .unwrap();
    let other = || expression_arg(col("y"));
    let run_close = |frame: &DataFrame, given: Arguments| {
        test_of(frame, "Expr.is_close", &given.with("other", other()))
    };
    let tolerance = Arguments::new().with("abs_tol", Argument::Float(0.1));
    assert_eq!(
        run_close(&frame, tolerance),
        [Some(true), Some(false), Some(false)]
    );
    // With no tolerance but the default relative one, only values equal to nine digits are close.
    assert_eq!(
        run_close(&frame, none()),
        [Some(false), Some(false), Some(false)]
    );
    let near = df!(
        "x" => [1.0_f64, 1.0],
        "y" => [1.000_000_000_1_f64, 1.01],
    )
    .unwrap();
    assert_eq!(run_close(&near, none()), [Some(true), Some(false)]);
    let loose = Arguments::new().with("rel_tol", Argument::Float(0.1));
    assert_eq!(run_close(&near, loose), [Some(true), Some(true)]);
    let nans = df!("x" => [f64::NAN], "y" => [f64::NAN]).unwrap();
    assert_eq!(run_close(&nans, none()), [Some(false)]);
    let equal = Arguments::new().with("nans_equal", flag(true));
    assert_eq!(run_close(&nans, equal), [Some(true)]);
}

#[test]
fn a_comparison_that_treats_null_as_a_value_is_true_for_two_nulls_where_a_plain_one_is_null() {
    let frame = df!(
        "x" => [Some(1_i64), None, None],
        "y" => [Some(1_i64), Some(2), None],
    )
    .unwrap();
    let other = Arguments::new().with("other", expression_arg(col("y")));
    let (yes, no) = (Some(true), Some(false));
    assert_eq!(test_of(&frame, "Expr.eq_missing", &other), [yes, no, yes]);
    assert_eq!(test_of(&frame, "Expr.ne_missing", &other), [no, yes, no]);
    // The plain comparison leaves a null where either side is one.
    assert_eq!(test_of(&frame, "Expr.eq", &other), [yes, None, None]);
}

fn cast_to(frame: &DataFrame, dtype: DataType, extra: Arguments) -> Result<DataFrame, String> {
    let given = extra.with("dtype", Argument::Dtype(dtype));
    let expr = method("Expr.cast", col("x"), &given).expect("the method builds");
    frame
        .clone()
        .lazy()
        .select([expr.alias("out")])
        .collect()
        .map_err(|error| error.to_string())
}

#[test]
fn a_cast_to_a_dtype_gives_that_dtype_and_must_give_it() {
    let missing = method("Expr.cast", col("i"), &none()).unwrap_err();
    assert!(matches!(
        missing,
        BuildError::Missing { parameter: "dtype" }
    ));
    for dtype in [DataType::String, DataType::Float32, DataType::UInt8] {
        let given = Arguments::new().with("dtype", Argument::Dtype(dtype.clone()));
        let built = method("Expr.cast", col("i"), &given).unwrap();
        assert_eq!(dtype_of(&built), dtype);
    }
    let frame = df!("x" => [1_i64, 2, 3]).unwrap();
    let text = cast_to(&frame, DataType::String, none()).unwrap();
    let values: Vec<_> = text
        .column("out")
        .unwrap()
        .str()
        .unwrap()
        .iter()
        .map(|value| value.map(str::to_owned))
        .collect();
    assert_eq!(
        values,
        [
            Some("1".to_owned()),
            Some("2".to_owned()),
            Some("3".to_owned())
        ]
    );
}

#[test]
fn a_cast_refuses_a_value_that_does_not_fit_unless_it_is_not_strict_or_wraps() {
    let frame = df!("x" => [1_i64, 2, 300]).unwrap();
    let strict = cast_to(&frame, DataType::UInt8, none());
    assert!(
        strict
            .unwrap_err()
            .contains("conversion from `i64` to `u8` failed")
    );
    // The value that does not fit becomes a null.
    let lenient = cast_to(
        &frame,
        DataType::UInt8,
        Arguments::new().with("strict", flag(false)),
    );
    let lenient = lenient.unwrap();
    let kept: Vec<_> = lenient
        .column("out")
        .unwrap()
        .u8()
        .unwrap()
        .iter()
        .collect();
    assert_eq!(kept, [Some(1), Some(2), None]);
    // 300 is 44 more than a byte holds, and wrapping keeps what is over.
    let wrapped = Arguments::new().with("wrap_numerical", flag(true));
    let wrapped = cast_to(&frame, DataType::UInt8, wrapped).unwrap();
    let kept: Vec<_> = wrapped
        .column("out")
        .unwrap()
        .u8()
        .unwrap()
        .iter()
        .collect();
    assert_eq!(kept, [Some(1), Some(2), Some(44)]);
    // Python lets wrapping win over `strict`.
    let both = Arguments::new()
        .with("wrap_numerical", flag(true))
        .with("strict", flag(true));
    assert!(cast_to(&frame, DataType::UInt8, both).is_ok());
}

#[test]
fn a_replacement_swaps_the_values_it_lists_and_keeps_the_dtype() {
    let frame = df!("x" => [1_i64, 2, 3]).unwrap();
    let given = Arguments::new()
        .with("old", number(2))
        .with("new", number(20));
    let out = run_on(&frame, "Expr.replace", &given);
    assert_eq!(integers(&out, "out"), [Some(1), Some(20), Some(3)]);
    assert_eq!(out.column("out").unwrap().dtype(), &DataType::Int64);
    let without = Arguments::new().with("old", number(2));
    let error = method("Expr.replace", col("x"), &without).unwrap_err();
    assert!(matches!(error, BuildError::Missing { parameter: "new" }));
}

#[test]
fn a_strict_replacement_gives_the_default_for_the_values_it_does_not_list() {
    let frame = df!("x" => [1_i64, 2, 3]).unwrap();
    let given = Arguments::new()
        .with("old", number(1))
        .with("new", number(10))
        .with("default", number(0));
    let out = run_on(&frame, "Expr.replace_strict", &given);
    // The dtype is the one of what replaces, not of what is replaced, unless it is asked for:
    // a number typed without a dtype is a 32-bit number.
    assert_eq!(out.column("out").unwrap().dtype(), &DataType::Int32);
    let values: Vec<_> = out.column("out").unwrap().i32().unwrap().iter().collect();
    assert_eq!(values, [Some(10), Some(0), Some(0)]);
    let typed = given.with("return_dtype", Argument::Dtype(DataType::Int64));
    let out = run_on(&frame, "Expr.replace_strict", &typed);
    assert_eq!(out.column("out").unwrap().dtype(), &DataType::Int64);
    // A value that is not listed is refused when the rows run, where there is no default.
    let no_default = Arguments::new()
        .with("old", number(1))
        .with("new", number(10));
    let expr: Expr = method("Expr.replace_strict", col("x"), &no_default).unwrap();
    let error = frame.lazy().select([expr]).collect().unwrap_err();
    assert!(error.to_string().contains("incomplete mapping"));
}

#[test]
fn the_nulls_methods_keep_the_dtype_they_are_given_and_the_checks_give_a_boolean() {
    for id in [
        "Expr.drop_nulls",
        "Expr.drop_nans",
        "Expr.forward_fill",
        "Expr.backward_fill",
    ] {
        for (column, expected) in [("i", DataType::Int64), ("s", DataType::String)] {
            let built = method(id, col(column), &none()).unwrap();
            assert_eq!(dtype_of(&built), expected, "{id} of {column}");
        }
    }
    for id in [
        "Expr.is_nan",
        "Expr.is_not_nan",
        "Expr.is_finite",
        "Expr.is_infinite",
        "Expr.is_duplicated",
        "Expr.is_unique",
        "Expr.is_first_distinct",
        "Expr.is_last_distinct",
    ] {
        let built = method(id, col("f"), &none()).unwrap();
        assert_eq!(dtype_of(&built), DataType::Boolean, "{id}");
    }
}
