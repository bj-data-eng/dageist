//! The maths and rounding methods built through the catalogue and run on small tables, against
//! the values mathematics gives, which are written here by hand, and the dtypes Polars
//! documents: a root, a logarithm or an angle is a float, and a rounding keeps the dtype it is
//! given.

use polars::prelude::{DataType, Expr, IntoLazy, col, df};

use crate::catalog::{Argument, Arguments, BuildError};

use super::build::{choice, dtype_of, expression_arg, floats, integers, method, none, run, sample};

/// `entry` applied to the column `name` of `frame`, run, and the floats it gives.
fn floats_of(frame: &polars::prelude::DataFrame, id: &str, arguments: &Arguments) -> Vec<f64> {
    let expr = method(id, col("x"), arguments).expect("the method builds");
    let out = run(frame.clone().lazy().select([expr.alias("out")]));
    floats(&out, "out")
        .into_iter()
        .map(|value| value.expect("a value"))
        .collect()
}

fn assert_close(found: &[f64], expected: &[f64], what: &str) {
    assert_eq!(found.len(), expected.len(), "{what}: {found:?}");
    for (found, expected) in found.iter().zip(expected) {
        assert!(
            (found - expected).abs() < 1e-9,
            "{what}: {found} is not {expected}"
        );
    }
}

fn whole(value: i64) -> Argument {
    Argument::Integer(value)
}

#[test]
fn roots_exponentials_and_logarithms_give_the_values_mathematics_gives() {
    let squares = df!("x" => [1.0_f64, 4.0, 9.0]).unwrap();
    assert_close(
        &floats_of(&squares, "Expr.sqrt", &none()),
        &[1.0, 2.0, 3.0],
        "sqrt",
    );
    let cubes = df!("x" => [1.0_f64, 8.0, 27.0]).unwrap();
    assert_close(
        &floats_of(&cubes, "Expr.cbrt", &none()),
        &[1.0, 2.0, 3.0],
        "cbrt",
    );
    let zero = df!("x" => [0.0_f64]).unwrap();
    assert_close(&floats_of(&zero, "Expr.exp", &none()), &[1.0], "exp 0");
    assert_close(&floats_of(&zero, "Expr.log1p", &none()), &[0.0], "log1p 0");
    assert_close(&floats_of(&zero, "Expr.erf", &none()), &[0.0], "erf 0");
    assert_close(&floats_of(&zero, "Expr.erfc", &none()), &[1.0], "erfc 0");
}

#[test]
fn a_logarithm_is_to_the_base_e_unless_another_base_is_given() {
    let powers = df!("x" => [1.0_f64, std::f64::consts::E, 8.0]).unwrap();
    // The natural logarithms of 1 and e are 0 and 1.
    let natural = floats_of(&powers, "Expr.log", &none());
    assert_close(&natural[..2], &[0.0, 1.0], "log");
    // The logarithm of 8 to the base 2 is 3, and of 1 is 0.
    let base_two = Arguments::new().with("base", expression_arg(polars::prelude::lit(2.0_f64)));
    let to_two = floats_of(&powers, "Expr.log", &base_two);
    assert_close(&[to_two[0], to_two[2]], &[0.0, 3.0], "log base 2");
}

#[test]
fn the_default_base_of_a_logarithm_is_a_float64_wherever_it_stands() {
    // Polars leaves the logarithm of a whole number to a base typed without a dtype an unresolved
    // float; the default base is typed, so the logarithm of a whole number is a `Float64`.
    let log = method("Expr.log", col("i"), &none()).expect("the method builds");
    assert_eq!(dtype_of(&log), DataType::Float64);
    let single = method("Expr.log", col("f"), &none()).expect("the method builds");
    assert_eq!(dtype_of(&single), DataType::Float64);
}

#[test]
fn the_trigonometric_functions_give_the_values_mathematics_gives() {
    let at = |id: &str, x: f64| {
        let frame = df!("x" => [x]).unwrap();
        floats_of(&frame, id, &none())[0]
    };
    let quarter = std::f64::consts::FRAC_PI_4;
    let cases = [
        ("Expr.sin", 0.0, 0.0),
        ("Expr.cos", 0.0, 1.0),
        ("Expr.tan", 0.0, 0.0),
        // The cotangent of a quarter of half a turn is 1.
        ("Expr.cot", quarter, 1.0),
        ("Expr.arcsin", 1.0, std::f64::consts::FRAC_PI_2),
        ("Expr.arccos", 1.0, 0.0),
        ("Expr.arctan", 1.0, quarter),
        ("Expr.sinh", 0.0, 0.0),
        ("Expr.cosh", 0.0, 1.0),
        ("Expr.tanh", 0.0, 0.0),
        ("Expr.arcsinh", 0.0, 0.0),
        ("Expr.arccosh", 1.0, 0.0),
        ("Expr.arctanh", 0.0, 0.0),
        // Half a turn is 180 degrees, and 180 degrees is half a turn.
        ("Expr.degrees", std::f64::consts::PI, 180.0),
        ("Expr.radians", 180.0, std::f64::consts::PI),
    ];
    for (id, x, expected) in cases {
        assert_close(&[at(id, x)], &[expected], id);
    }
}

#[test]
fn the_maths_of_one_value_give_a_float_that_stays_as_wide_as_the_one_it_was_given() {
    let wide: Vec<(&str, DataType)> = vec![("i", DataType::Int64), ("f", DataType::Float64)];
    for id in [
        "Expr.sqrt",
        "Expr.cbrt",
        "Expr.exp",
        "Expr.log1p",
        "Expr.sin",
        "Expr.arctanh",
        "Expr.erf",
        "Expr.degrees",
    ] {
        for (column, _) in &wide {
            let built = method(id, col(*column), &none()).expect("the method builds");
            assert_eq!(dtype_of(&built), DataType::Float64, "{id} of {column}");
        }
        let narrow = method(id, col("i").cast(DataType::Float32), &none()).expect("it builds");
        assert_eq!(dtype_of(&narrow), DataType::Float32, "{id} of a Float32");
    }
}

#[test]
fn the_sign_is_minus_one_zero_or_one_in_the_dtype_it_is_given() {
    let frame = df!("x" => [-5_i64, 0, 7]).unwrap();
    let expr = method("Expr.sign", col("x"), &none()).unwrap();
    let out = run(frame.lazy().select([expr.alias("out")]));
    assert_eq!(integers(&out, "out"), [Some(-1), Some(0), Some(1)]);
    let built = method("Expr.sign", col("i"), &none()).unwrap();
    assert_eq!(dtype_of(&built), DataType::Int64);
}

#[test]
fn ceil_and_floor_round_to_the_whole_number_above_and_below_and_leave_whole_numbers() {
    let frame = df!("x" => [1.2_f64, -1.2, 3.0]).unwrap();
    assert_close(
        &floats_of(&frame, "Expr.ceil", &none()),
        &[2.0, -1.0, 3.0],
        "ceil",
    );
    assert_close(
        &floats_of(&frame, "Expr.floor", &none()),
        &[1.0, -2.0, 3.0],
        "floor",
    );
    for id in ["Expr.ceil", "Expr.floor", "Expr.round", "Expr.truncate"] {
        let built = method(id, col("i"), &none()).unwrap();
        assert_eq!(dtype_of(&built), DataType::Int64, "{id} of whole numbers");
    }
}

#[test]
fn a_tie_rounds_to_the_even_neighbour_unless_it_is_told_to_round_away_from_zero() {
    let ties = df!("x" => [0.5_f64, 1.5, 2.5, -2.5]).unwrap();
    // Banker's rounding, which the documentation of the method calls the default.
    assert_close(
        &floats_of(&ties, "Expr.round", &none()),
        &[0.0, 2.0, 2.0, -2.0],
        "half_to_even",
    );
    let away = Arguments::new().with("mode", choice("half_away_from_zero"));
    assert_close(
        &floats_of(&ties, "Expr.round", &away),
        &[1.0, 2.0, 3.0, -3.0],
        "half_away_from_zero",
    );
    // A quarter is a tie between 0.2 and 0.3 at one decimal.
    let quarters = df!("x" => [0.25_f64, 0.75]).unwrap();
    let one = Arguments::new().with("decimals", whole(1));
    assert_close(
        &floats_of(&quarters, "Expr.round", &one),
        &[0.2, 0.8],
        "one decimal, half_to_even",
    );
    let one_away = one.with("mode", choice("half_away_from_zero"));
    assert_close(
        &floats_of(&quarters, "Expr.round", &one_away),
        &[0.3, 0.8],
        "one decimal, half_away_from_zero",
    );
}

#[test]
fn python_takes_two_words_for_the_mode_of_a_rounding_and_not_to_zero() {
    // Polars' documentation also lists `to_zero`, which the binding refuses; `truncate` does it.
    let refused = crate::catalog::entry("Expr.round")
        .expect("an entry")
        .arguments([("mode", choice("to_zero"))]);
    assert!(matches!(
        refused,
        Err(BuildError::UnknownChoice {
            parameter: "mode",
            ..
        })
    ));
}

#[test]
fn truncate_cuts_after_the_decimals_toward_zero() {
    let frame = df!("x" => [1.99_f64, -1.99, 0.5]).unwrap();
    assert_close(
        &floats_of(&frame, "Expr.truncate", &none()),
        &[1.0, -1.0, 0.0],
        "no decimals",
    );
    let one = Arguments::new().with("decimals", whole(1));
    assert_close(
        &floats_of(&frame, "Expr.truncate", &one),
        &[1.9, -1.9, 0.5],
        "one decimal",
    );
}

#[test]
fn significant_figures_keep_the_leading_digits_of_a_number() {
    // The example of Polars' documentation.
    let frame = df!("x" => [0.01234_f64, 3.333, 1234.0]).unwrap();
    let two = Arguments::new().with("digits", whole(2));
    assert_close(
        &floats_of(&frame, "Expr.round_sig_figs", &two),
        &[0.012, 3.3, 1200.0],
        "two figures",
    );
}

#[test]
fn clip_sets_what_is_outside_the_bounds_to_the_bound() {
    let frame = df!("x" => [-50_i64, 5, 50]).unwrap();
    let clipped = |arguments: Arguments| {
        let expr = method("Expr.clip", col("x"), &arguments).unwrap();
        integers(
            &run(frame.clone().lazy().select([expr.alias("out")])),
            "out",
        )
    };
    let lit = |value: i64| expression_arg(polars::prelude::lit(value));
    let both = Arguments::new()
        .with("lower_bound", lit(1))
        .with("upper_bound", lit(10));
    assert_eq!(clipped(both), [Some(1), Some(5), Some(10)]);
    let lower = Arguments::new().with("lower_bound", lit(1));
    assert_eq!(clipped(lower), [Some(1), Some(5), Some(50)]);
    let upper = Arguments::new().with("upper_bound", lit(10));
    assert_eq!(clipped(upper), [Some(-50), Some(5), Some(10)]);
    // With neither bound the values are as they were.
    assert_eq!(clipped(none()), [Some(-50), Some(5), Some(50)]);
}

#[test]
fn clip_takes_a_bound_from_a_column_and_keeps_the_dtype_of_what_it_clips() {
    // The column 1, 2, 3 held to no less than the same column read backwards, 3, 2, 1.
    let given = Arguments::new().with("lower_bound", expression_arg(col("i").reverse()));
    let built = method("Expr.clip", col("i"), &given).unwrap();
    assert_eq!(dtype_of(&built), DataType::Int64);
    let out = run(sample().select([built.alias("out")]));
    assert_eq!(integers(&out, "out"), [Some(3), Some(2), Some(3)]);
}

#[test]
fn a_number_of_places_python_cannot_take_as_an_unsigned_32_bit_number_is_out_of_range() {
    let refused = |decimals: i64, id: &str| {
        let given = Arguments::new().with("decimals", whole(decimals));
        method(id, col("f"), &given).unwrap_err()
    };
    for id in ["Expr.round", "Expr.truncate"] {
        for decimals in [-1, 4_294_967_296] {
            assert!(
                matches!(
                    refused(decimals, id),
                    BuildError::OutOfRange {
                        parameter: "decimals",
                        ..
                    }
                ),
                "{id} {decimals}"
            );
        }
        // The largest is taken.
        let given = Arguments::new().with("decimals", whole(4_294_967_295));
        assert!(method(id, col("f"), &given).is_ok(), "{id}");
    }
}

#[test]
fn significant_figures_must_be_given_and_fit_a_signed_32_bit_number() {
    let missing = method("Expr.round_sig_figs", col("f"), &none()).unwrap_err();
    assert!(matches!(
        missing,
        BuildError::Missing {
            parameter: "digits"
        }
    ));
    for digits in [2_147_483_648, -2_147_483_649] {
        let given = Arguments::new().with("digits", whole(digits));
        let error = method("Expr.round_sig_figs", col("f"), &given).unwrap_err();
        assert!(
            matches!(
                error,
                BuildError::OutOfRange {
                    parameter: "digits",
                    ..
                }
            ),
            "{digits}"
        );
    }
    // Polars refuses fewer than one figure when it runs the rows, not when it builds: a negative
    // number of figures is a number the binding takes.
    let given = Arguments::new().with("digits", whole(-3));
    let built: Expr = method("Expr.round_sig_figs", col("f"), &given).unwrap();
    assert_eq!(dtype_of(&built), DataType::Float64);
}
