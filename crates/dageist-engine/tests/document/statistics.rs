//! What the aggregates of statistics, the arguments of a maximum and a minimum, the tests of
//! truth, the approximations, the running minimum and product and the bitwise methods do in a
//! step, on the document-timing sample, whose `row_days` column holds `0, 2, 2, 0, 2, 2, 1` and whose
//! `step_to` column holds `Intake, Pending, Complete, Intake, Review, Complete, Review`. Each
//! expected cell is worked out by hand from the definition of the statistic, and the dtypes are
//! what Python Polars documents for each method. The walk's part uses the table `a`, `b`, `f` and
//! `k` of the other walk tests.

use dageist_engine::allowed::{RefusalSource, Representative, Rule, Shape};
use dageist_engine::document::{Document, Outcome, StepId};
use polars::prelude::DataType;
use serde_json::{Value, json};

use crate::documents::{documents, inputs};
use crate::order::{
    after, cells, dtype_of, flag, gives, join_walk, method, select_after, select_walk, selecting,
    texts, with_gaps, word,
};
use crate::support::{call, chain, field, field_then, whole};

/// The expression argument `polars.lit(number)` of a number with a fraction.
fn float(number: f64) -> Value {
    chain(
        call("polars.lit", json!({ "value": { "float": number } })),
        vec![],
    )
}

fn dtype(name: &str) -> Value {
    json!({ "dtype": name })
}

/// `row_days` plus one, `1, 3, 3, 1, 3, 3, 2`, which has no zero to make a product or an
/// entropy of the whole of it nothing.
fn days_plus_one() -> Value {
    after(
        &field("row_days"),
        vec![call("Expr.add", json!({ "other": whole(1) }))],
    )
}

/// `row_days` equal to 2: `false, true, true, false, true, true, false`.
fn is_two() -> Value {
    field_then(
        "row_days",
        vec![call("Expr.eq", json!({ "other": whole(2) }))],
    )
}

/// The one cell the written step gives, read as a number.
fn number(written: &str) -> f64 {
    let shown = cells(written);
    assert_eq!(shown.len(), 1, "{shown:?}");
    shown[0]
        .as_deref()
        .expect("a number, not a null")
        .parse()
        .expect("a number")
}

/// Whether the step is one Polars accepts as the engine runs it on the documents: it resolves the
/// schema, and the rows it gives are the ones the preview shows. Polars refuses some calls only
/// when it runs the rows.
fn is_valid(written: &str) -> bool {
    match Document::read(written) {
        Err(_) => false,
        Ok(document) => {
            document
                .run_on(&inputs(), &documents())
                .is_ok_and(|report| matches!(report.steps()[0].outcome(), Outcome::Valid { .. }))
                && document
                    .preview(
                        &inputs(),
                        &documents(),
                        &StepId::new("s").expect("an id"),
                        100,
                    )
                    .is_ok()
        }
    }
}

/// The preview shows a float to six places, so a cell is the number to within a millionth.
fn close(found: f64, wanted: f64, what: &str) {
    assert!(
        (found - wanted).abs() < 1e-5,
        "{what}: {found} is not {wanted}"
    );
}

/// The moments of the days about their mean, as the definition of a statistic gives them: `n`
/// values, the mean, and the second, third and fourth central moments.
fn moments(values: &[f64]) -> (f64, f64, f64, f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let central = |power: i32| {
        values
            .iter()
            .map(|value| (value - mean).powi(power))
            .sum::<f64>()
            / n
    };
    (n, mean, central(2), central(3), central(4))
}

const DAYS: [f64; 7] = [0.0, 2.0, 2.0, 0.0, 2.0, 2.0, 1.0];

#[test]
fn a_mean_and_a_median_are_a_float_and_a_boolean_has_a_mean_of_its_trues() {
    let mean = selecting(method("row_days", "Expr.mean", json!({})));
    assert_eq!(dtype_of(&mean), DataType::Float64);
    close(number(&mean), 9.0 / 7.0, "the mean of the days");
    let median = selecting(method("row_days", "Expr.median", json!({})));
    assert_eq!(dtype_of(&median), DataType::Float64);
    assert_eq!(cells(&median), texts(&["2.0"]));
    // Four of the seven days are 2.
    let of_trues = selecting(after(&is_two(), vec![call("Expr.mean", json!({}))]));
    assert_eq!(dtype_of(&of_trues), DataType::Float64);
    close(number(&of_trues), 4.0 / 7.0, "the mean of the trues");
    // A float of a narrow dtype stays that dtype.
    let narrow = selecting(after(
        &field("row_days"),
        vec![
            call("Expr.cast", json!({ "dtype": dtype("Float32") })),
            call("Expr.mean", json!({})),
        ],
    ));
    assert_eq!(dtype_of(&narrow), DataType::Float32);
}

#[test]
fn a_mean_of_moments_is_a_moment_and_text_has_none() {
    let at = selecting(method("timestamp", "Expr.mean", json!({})));
    assert_eq!(
        dtype_of(&at),
        DataType::Datetime(polars::prelude::TimeUnit::Microseconds, None)
    );
    assert!(!is_valid(&selecting(method(
        "step_to",
        "Expr.mean",
        json!({})
    ))));
    assert!(!is_valid(&selecting(method(
        "step_to",
        "Expr.median",
        json!({})
    ))));
}

#[test]
fn a_variance_and_a_deviation_divide_by_the_count_less_ddof() {
    // The sum of the squares about the mean 9/7 is 38/7 for the seven days.
    let by = |entry: &str, arguments: Value| {
        let written = selecting(method("row_days", entry, arguments));
        assert_eq!(dtype_of(&written), DataType::Float64, "{entry}");
        number(&written)
    };
    close(by("Expr.var", json!({})), 38.0 / 42.0, "var, ddof 1");
    close(
        by("Expr.var", json!({ "ddof": { "integer": 0 } })),
        38.0 / 49.0,
        "var, ddof 0",
    );
    close(
        by("Expr.std", json!({})),
        (38.0_f64 / 42.0).sqrt(),
        "std, ddof 1",
    );
    close(
        by("Expr.std", json!({ "ddof": { "integer": 0 } })),
        (38.0_f64 / 49.0).sqrt(),
        "std, ddof 0",
    );
    // As many as the values or more leaves no value to divide by.
    let none = selecting(method(
        "row_days",
        "Expr.var",
        json!({ "ddof": { "integer": 7 } }),
    ));
    assert_eq!(cells(&none), vec![None]);
}

#[test]
fn a_ddof_is_a_whole_number_that_fits_an_unsigned_byte() {
    for ddof in [0, 1, 255] {
        let written = selecting(method(
            "row_days",
            "Expr.std",
            json!({ "ddof": { "integer": ddof } }),
        ));
        assert!(is_valid(&written), "ddof {ddof}");
    }
    for ddof in [-1, 256] {
        let written = selecting(method(
            "row_days",
            "Expr.var",
            json!({ "ddof": { "integer": ddof } }),
        ));
        assert!(!is_valid(&written), "ddof {ddof}");
    }
    assert!(!is_valid(&selecting(method(
        "step_to",
        "Expr.std",
        json!({})
    ))));
}

#[test]
fn a_quantile_is_a_float_and_the_interpolation_says_where_between_two_values_it_falls() {
    // The days in order are `0, 0, 1, 2, 2, 2, 2`, so a quarter of the way along six steps is at
    // 1.5: between the second value, 0, and the third, 1.
    let at = |quantile: f64, interpolation: Option<&str>| {
        let mut arguments = json!({ "quantile": float(quantile) });
        if let Some(interpolation) = interpolation {
            arguments["interpolation"] = word(interpolation);
        }
        let written = selecting(method("row_days", "Expr.quantile", arguments));
        assert_eq!(dtype_of(&written), DataType::Float64);
        number(&written)
    };
    close(at(0.5, None), 2.0, "the median");
    close(at(0.75, None), 2.0, "three quarters");
    close(at(0.25, None), 1.0, "a quarter, nearest, the default");
    close(at(0.25, Some("nearest")), 1.0, "nearest");
    close(at(0.25, Some("higher")), 1.0, "higher");
    close(at(0.25, Some("lower")), 0.0, "lower");
    close(at(0.25, Some("midpoint")), 0.5, "midpoint");
    close(at(0.25, Some("linear")), 0.5, "linear");
    close(at(0.25, Some("equiprobable")), 0.0, "equiprobable");
}

#[test]
fn a_quantile_of_the_moments_is_a_moment_one_of_a_boolean_is_a_float_and_one_of_text_is_refused() {
    let written = selecting(after(
        &field("timestamp"),
        vec![call("Expr.quantile", json!({ "quantile": float(0.5) }))],
    ));
    assert_eq!(
        dtype_of(&written),
        DataType::Datetime(polars::prelude::TimeUnit::Microseconds, None)
    );
    // Python Polars 2.2.0 takes the quantile of a boolean as it takes a number's.
    assert_eq!(
        dtype_of(&selecting(after(
            &is_two(),
            vec![call("Expr.quantile", json!({ "quantile": float(0.5) }))]
        ))),
        DataType::Float64
    );
    assert!(!is_valid(&selecting(after(
        &field("step_to"),
        vec![call("Expr.quantile", json!({ "quantile": float(0.5) }))]
    ))));
    // A quantile given no value is no step Polars could run.
    assert!(!is_valid(&selecting(method(
        "row_days",
        "Expr.quantile",
        json!({})
    ))));
}

#[test]
fn a_product_multiplies_the_values_and_gives_a_whole_number_of_sixty_four_bits() {
    let product = selecting(after(
        &days_plus_one(),
        vec![call("Expr.product", json!({}))],
    ));
    assert_eq!(dtype_of(&product), DataType::Int64);
    assert_eq!(cells(&product), texts(&["162"]));
    // A false is a zero.
    let of_booleans = selecting(after(&is_two(), vec![call("Expr.product", json!({}))]));
    assert_eq!(dtype_of(&of_booleans), DataType::Int64);
    assert_eq!(cells(&of_booleans), texts(&["0"]));
    assert!(!is_valid(&selecting(method(
        "step_to",
        "Expr.product",
        json!({})
    ))));
}

#[test]
fn a_skew_is_the_third_moment_over_the_second_to_the_power_one_and_a_half() {
    let (n, _mean, m2, m3, _m4) = moments(&DAYS);
    let biased = m3 / m2.powf(1.5);
    let written = selecting(method("row_days", "Expr.skew", json!({})));
    assert_eq!(dtype_of(&written), DataType::Float64);
    close(number(&written), biased, "the biased skew");
    // Unbiased, it is that times the square root of n(n-1) over n-2.
    let unbiased = selecting(method(
        "row_days",
        "Expr.skew",
        json!({ "bias": flag(false) }),
    ));
    close(
        number(&unbiased),
        biased * (n * (n - 1.0)).sqrt() / (n - 2.0),
        "the unbiased skew",
    );
}

#[test]
fn a_kurtosis_is_the_fourth_moment_over_the_square_of_the_second_less_three_for_fisher() {
    let (n, _mean, m2, _m3, m4) = moments(&DAYS);
    let pearson = m4 / (m2 * m2);
    let fisher =
        |arguments: Value| number(&selecting(method("row_days", "Expr.kurtosis", arguments)));
    close(fisher(json!({})), pearson - 3.0, "fisher, biased");
    close(
        fisher(json!({ "fisher": flag(false) })),
        pearson,
        "pearson, biased",
    );
    let unbias = |excess: f64| (n - 1.0) / ((n - 2.0) * (n - 3.0)) * ((n + 1.0) * excess + 6.0);
    close(
        fisher(json!({ "bias": flag(false) })),
        unbias(pearson - 3.0),
        "fisher, unbiased",
    );
    close(
        fisher(json!({ "fisher": flag(false), "bias": flag(false) })),
        unbias(pearson - 3.0) + 3.0,
        "pearson, unbiased",
    );
    let written = selecting(method("row_days", "Expr.kurtosis", json!({})));
    assert_eq!(dtype_of(&written), DataType::Float64);
}

#[test]
fn an_entropy_is_minus_the_sum_of_each_share_times_its_logarithm() {
    // The days plus one are `1, 3, 3, 1, 3, 3, 2`, of which the shares are a sixteenth each of them.
    let values = [1.0_f64, 3.0, 3.0, 1.0, 3.0, 3.0, 2.0];
    let total: f64 = values.iter().sum();
    let natural: f64 = -values
        .iter()
        .map(|value| (value / total) * (value / total).ln())
        .sum::<f64>();
    let entropy = |arguments: Value| {
        let written = selecting(after(
            &days_plus_one(),
            vec![call("Expr.entropy", arguments)],
        ));
        assert_eq!(dtype_of(&written), DataType::Float64);
        number(&written)
    };
    close(entropy(json!({})), natural, "in nats");
    close(
        entropy(json!({ "base": { "float": 2.0 } })),
        natural / 2.0_f64.ln(),
        "in bits",
    );
    // Not normalised, the values are the shares: minus the sum of each value times its logarithm.
    let raw: f64 = -values.iter().map(|value| value * value.ln()).sum::<f64>();
    close(
        entropy(json!({ "normalize": flag(false) })),
        raw,
        "of the values as they are",
    );
}

#[test]
fn an_entropy_of_text_or_a_moment_is_refused_and_one_of_numbers_is_not() {
    assert!(is_valid(&selecting(method(
        "row_days",
        "Expr.entropy",
        json!({})
    ))));
    assert!(!is_valid(&selecting(method(
        "step_to",
        "Expr.entropy",
        json!({})
    ))));
    assert!(!is_valid(&selecting(method(
        "timestamp",
        "Expr.entropy",
        json!({})
    ))));
}

#[test]
fn an_arg_max_and_an_arg_min_give_a_row_that_holds_the_largest_and_smallest() {
    // Polars does not say which of equal values it gives the row of, and gives another at another
    // number of threads, and another for text in memory, so the row is one of those that hold the
    // value: the days are `0, 2, 2, 0, 2, 2, 1`, and the steps `Intake, Pending, Complete, Intake,
    // Review, Complete, Review`.
    let row_of = |expression: Value| -> String {
        let mut rows = cells(&selecting(expression));
        assert_eq!(rows.len(), 1);
        rows.remove(0).expect("a row")
    };
    let one_of = |expression: Value, rows: &[&str]| {
        let row = row_of(expression);
        assert!(rows.contains(&row.as_str()), "{row} is none of {rows:?}");
    };
    let largest = selecting(method("row_days", "Expr.arg_max", json!({})));
    assert_eq!(dtype_of(&largest), DataType::UInt32);
    one_of(
        method("row_days", "Expr.arg_max", json!({})),
        &["1", "2", "4", "5"],
    );
    let smallest = selecting(method("row_days", "Expr.arg_min", json!({})));
    assert_eq!(dtype_of(&smallest), DataType::UInt32);
    one_of(method("row_days", "Expr.arg_min", json!({})), &["0", "3"]);
    // Text is ordered too: `Review` is the largest of the steps, at rows 4 and 6, and `Complete`
    // the smallest, at rows 2 and 5.
    one_of(method("step_to", "Expr.arg_max", json!({})), &["4", "6"]);
    one_of(method("step_to", "Expr.arg_min", json!({})), &["2", "5"]);
    // The rows with no value are not the largest or the smallest: `0, null, null, 0, null, null,
    // 1`, where the one largest is at row 6.
    let with_a_gap = |entry: &str| selecting(after(&with_gaps(), vec![call(entry, json!({}))]));
    assert_eq!(cells(&with_a_gap("Expr.arg_max")), texts(&["6"]));
    let smallest_with_a_gap = cells(&with_a_gap("Expr.arg_min"));
    assert!(
        smallest_with_a_gap == texts(&["0"]) || smallest_with_a_gap == texts(&["3"]),
        "{smallest_with_a_gap:?}"
    );
}

#[test]
fn a_nan_max_and_a_nan_min_give_a_nan_where_the_values_hold_one() {
    // The days over themselves are `NaN, 1, 1, NaN, 1, 1, 1`: a zero over a zero is not a number.
    let quotients = after(
        &field("row_days"),
        vec![call("Expr.truediv", json!({ "other": field("row_days") }))],
    );
    for entry in ["Expr.nan_max", "Expr.nan_min"] {
        let written = selecting(after(&quotients, vec![call(entry, json!({}))]));
        assert_eq!(dtype_of(&written), DataType::Float64, "{entry}");
        assert_eq!(cells(&written), texts(&["NaN"]), "{entry}");
    }
    // Where none is a NaN they are the maximum and the minimum, of any dtype that has them.
    let largest = selecting(method("row_days", "Expr.nan_max", json!({})));
    assert_eq!(dtype_of(&largest), DataType::Int64);
    assert_eq!(cells(&largest), texts(&["2"]));
    let smallest = selecting(method("step_to", "Expr.nan_min", json!({})));
    assert_eq!(dtype_of(&smallest), DataType::String);
    assert_eq!(cells(&smallest), texts(&["Complete"]));
}

#[test]
fn an_any_and_an_all_say_whether_some_or_every_value_is_true() {
    let truth = |expression: &Value, entry: &str, arguments: Value| {
        let written = selecting(after(expression, vec![call(entry, arguments)]));
        assert_eq!(dtype_of(&written), DataType::Boolean, "{entry}");
        cells(&written)
    };
    // Some days are 2, and not all.
    assert_eq!(truth(&is_two(), "Expr.any", json!({})), texts(&["true"]));
    assert_eq!(truth(&is_two(), "Expr.all", json!({})), texts(&["false"]));
    // A whole number is true where it is not zero: some days are, and not all.
    assert_eq!(
        truth(&field("row_days"), "Expr.any", json!({})),
        texts(&["true"])
    );
    assert_eq!(
        truth(&field("row_days"), "Expr.all", json!({})),
        texts(&["false"])
    );
    // Over `0, null, null, 0, null, null, 1`: greater than minus one is `true, null, null, true,
    // null, null, true`, less than zero is `false, null, null, false, null, null, false`.
    let above = after(
        &with_gaps(),
        vec![call("Expr.gt", json!({ "other": whole(-1) }))],
    );
    let below = after(
        &with_gaps(),
        vec![call("Expr.lt", json!({ "other": whole(0) }))],
    );
    let leaving = json!({ "ignore_nulls": flag(false) });
    assert_eq!(truth(&above, "Expr.all", json!({})), texts(&["true"]));
    assert_eq!(truth(&above, "Expr.all", leaving.clone()), vec![None]);
    assert_eq!(truth(&above, "Expr.any", leaving.clone()), texts(&["true"]));
    assert_eq!(truth(&below, "Expr.any", json!({})), texts(&["false"]));
    assert_eq!(truth(&below, "Expr.any", leaving.clone()), vec![None]);
    assert_eq!(truth(&below, "Expr.all", leaving), texts(&["false"]));
    assert!(!is_valid(&selecting(method(
        "step_to",
        "Expr.any",
        json!({})
    ))));
    assert!(!is_valid(&selecting(method(
        "timestamp",
        "Expr.all",
        json!({})
    ))));
}

#[test]
fn an_approximate_count_of_distinct_values_counts_a_null_and_gives_a_count() {
    let counted = selecting(method("row_days", "Expr.approx_n_unique", json!({})));
    assert_eq!(dtype_of(&counted), DataType::UInt32);
    assert_eq!(cells(&counted), texts(&["3"]));
    // `0, null, 1` are three.
    let with_a_gap = selecting(after(
        &with_gaps(),
        vec![call("Expr.approx_n_unique", json!({}))],
    ));
    assert_eq!(cells(&with_a_gap), texts(&["3"]));
    let of_text = selecting(method("step_to", "Expr.approx_n_unique", json!({})));
    assert_eq!(cells(&of_text), texts(&["4"]));
}

#[test]
fn an_approximate_quantile_is_a_value_of_the_column_at_about_the_place_asked() {
    // Seven values are few enough to be exact for places that are clear: the days in order are
    // `0, 0, 1, 2, 2, 2, 2`, so the middle and nine tenths of the way are 2.
    for quantile in [0.5, 0.9] {
        let written = selecting(method(
            "row_days",
            "Expr.approx_quantile",
            json!({ "quantile": float(quantile) }),
        ));
        assert_eq!(dtype_of(&written), DataType::Int64, "{quantile}");
        assert_eq!(cells(&written), texts(&["2"]), "{quantile}");
    }
    for method_word in ["auto", "kll", "req_lo", "req_hi", "req_both"] {
        let written = selecting(method(
            "row_days",
            "Expr.approx_quantile",
            json!({ "quantile": float(0.5), "method": word(method_word), "error_tightness": word("formal") }),
        ));
        assert_eq!(cells(&written), texts(&["2"]), "{method_word}");
    }
    let tight = selecting(method(
        "row_days",
        "Expr.approx_quantile",
        json!({ "quantile": float(0.5), "error": { "float": 0.01 } }),
    ));
    assert_eq!(cells(&tight), texts(&["2"]));
    // An error of one or more, or of none, is no rank error Polars accepts.
    for error in [0.0, 1.0, 2.0] {
        let written = selecting(method(
            "row_days",
            "Expr.approx_quantile",
            json!({ "quantile": float(0.5), "error": { "float": error } }),
        ));
        assert!(!is_valid(&written), "error {error}");
    }
}

#[test]
fn a_running_minimum_keeps_the_smallest_so_far_and_reverse_runs_from_the_last_row() {
    let running = selecting(method("row_days", "Expr.cum_min", json!({})));
    assert_eq!(dtype_of(&running), DataType::Int64);
    assert_eq!(cells(&running), texts(&["0", "0", "0", "0", "0", "0", "0"]));
    let backwards = selecting(method(
        "row_days",
        "Expr.cum_min",
        json!({ "reverse": flag(true) }),
    ));
    assert_eq!(
        cells(&backwards),
        texts(&["0", "0", "0", "0", "1", "1", "1"])
    );
    // Text is ordered: `Intake, Pending, Complete, Intake, Review, Complete, Review`.
    let of_text = selecting(method("step_to", "Expr.cum_min", json!({})));
    assert_eq!(dtype_of(&of_text), DataType::String);
    assert_eq!(
        cells(&of_text),
        texts(&[
            "Intake", "Intake", "Complete", "Complete", "Complete", "Complete", "Complete",
        ])
    );
}

#[test]
fn a_running_product_multiplies_the_values_so_far_and_gives_sixty_four_bits_to_a_narrow_number() {
    let running = selecting(after(
        &days_plus_one(),
        vec![call("Expr.cum_prod", json!({}))],
    ));
    assert_eq!(dtype_of(&running), DataType::Int64);
    assert_eq!(
        cells(&running),
        texts(&["1", "3", "9", "9", "27", "81", "162"])
    );
    let backwards = selecting(after(
        &days_plus_one(),
        vec![call("Expr.cum_prod", json!({ "reverse": flag(true) }))],
    ));
    assert_eq!(
        cells(&backwards),
        texts(&["162", "162", "54", "18", "18", "6", "2"])
    );
    assert!(!is_valid(&selecting(method(
        "step_to",
        "Expr.cum_prod",
        json!({})
    ))));
}

#[test]
fn the_bitwise_aggregates_fold_the_values_bit_by_bit_into_one() {
    // The days plus one are `1, 3, 3, 1, 3, 3, 2`: the and of 1 and 2 is 0, the or of all is 3 and
    // the exclusive or runs 1, 2, 1, 0, 3, 0, 2.
    for (entry, expected) in [
        ("Expr.bitwise_and", "0"),
        ("Expr.bitwise_or", "3"),
        ("Expr.bitwise_xor", "2"),
    ] {
        let written = selecting(after(&days_plus_one(), vec![call(entry, json!({}))]));
        assert_eq!(dtype_of(&written), DataType::Int64, "{entry}");
        assert_eq!(cells(&written), texts(&[expected]), "{entry}");
    }
    // Of truths they are the truth of every, of some and of an odd number of them.
    for (entry, expected) in [
        ("Expr.bitwise_and", "false"),
        ("Expr.bitwise_or", "true"),
        ("Expr.bitwise_xor", "false"),
    ] {
        let written = selecting(after(&is_two(), vec![call(entry, json!({}))]));
        assert_eq!(dtype_of(&written), DataType::Boolean, "{entry}");
        assert_eq!(cells(&written), texts(&[expected]), "{entry}");
    }
    assert!(!is_valid(&selecting(method(
        "step_to",
        "Expr.bitwise_and",
        json!({})
    ))));
    let of_floats = after(
        &field("row_days"),
        vec![
            call("Expr.cast", json!({ "dtype": dtype("Float64") })),
            call("Expr.bitwise_or", json!({})),
        ],
    );
    assert!(!is_valid(&selecting(of_floats)));
}

#[test]
fn the_bit_counts_give_a_count_of_bits_for_each_value() {
    // The days are `0, 2, 2, 0, 2, 2, 1`, which are 64 bits each: 0 is all unset, 2 is `10` and 1
    // is `1`.
    let counted = |entry: &str, expected: [&str; 7]| {
        let written = selecting(method("row_days", entry, json!({})));
        assert_eq!(dtype_of(&written), DataType::UInt32, "{entry}");
        assert_eq!(cells(&written), texts(&expected), "{entry}");
    };
    counted(
        "Expr.bitwise_count_ones",
        ["0", "1", "1", "0", "1", "1", "1"],
    );
    counted(
        "Expr.bitwise_count_zeros",
        ["64", "63", "63", "64", "63", "63", "63"],
    );
    counted(
        "Expr.bitwise_leading_zeros",
        ["64", "62", "62", "64", "62", "62", "63"],
    );
    counted(
        "Expr.bitwise_trailing_zeros",
        ["64", "1", "1", "64", "1", "1", "0"],
    );
    counted(
        "Expr.bitwise_leading_ones",
        ["0", "0", "0", "0", "0", "0", "0"],
    );
    counted(
        "Expr.bitwise_trailing_ones",
        ["0", "0", "0", "0", "0", "0", "1"],
    );
    // Less one the days are `-1, 1, 1, -1, 1, 1, 0`, and -1 is every bit set.
    let less_one = after(
        &field("row_days"),
        vec![call("Expr.sub", json!({ "other": whole(1) }))],
    );
    let ones = selecting(after(
        &less_one,
        vec![call("Expr.bitwise_count_ones", json!({}))],
    ));
    assert_eq!(cells(&ones), texts(&["64", "1", "1", "64", "1", "1", "0"]));
    assert!(!is_valid(&selecting(method(
        "step_to",
        "Expr.bitwise_count_ones",
        json!({})
    ))));
}

// ---------------------------------------------------------------------------
// The walk

/// Every aggregate of the batch, with what it gives of the whole number `a` and what it is given.
fn aggregates() -> Vec<(&'static str, Value, Representative)> {
    vec![
        ("Expr.mean", json!({}), Representative::Float64),
        ("Expr.median", json!({}), Representative::Float64),
        ("Expr.std", json!({}), Representative::Float64),
        ("Expr.var", json!({}), Representative::Float64),
        (
            "Expr.quantile",
            json!({ "quantile": float(0.5) }),
            Representative::Float64,
        ),
        ("Expr.product", json!({}), Representative::Int64),
        ("Expr.skew", json!({}), Representative::Float64),
        ("Expr.kurtosis", json!({}), Representative::Float64),
        ("Expr.entropy", json!({}), Representative::Float64),
        ("Expr.arg_max", json!({}), Representative::UInt32),
        ("Expr.arg_min", json!({}), Representative::UInt32),
        ("Expr.nan_max", json!({}), Representative::Int64),
        ("Expr.nan_min", json!({}), Representative::Int64),
        ("Expr.approx_n_unique", json!({}), Representative::UInt32),
        (
            "Expr.approx_quantile",
            json!({ "quantile": float(0.5) }),
            Representative::Int64,
        ),
        ("Expr.bitwise_and", json!({}), Representative::Int64),
        ("Expr.bitwise_or", json!({}), Representative::Int64),
        ("Expr.bitwise_xor", json!({}), Representative::Int64),
        ("Expr.any", json!({}), Representative::Boolean),
        ("Expr.all", json!({}), Representative::Boolean),
    ]
}

#[test]
fn the_walk_gives_one_value_after_each_aggregate_and_the_dtype_polars_gives() {
    for (entry, arguments, dtype) in aggregates() {
        assert_eq!(
            gives(&select_walk(method("a", entry, arguments))),
            select_after(dtype, Shape::One),
            "{entry}"
        );
    }
}

#[test]
fn the_walk_gives_a_value_for_each_row_after_the_running_and_bitwise_methods_that_keep_the_rows() {
    let each = |entry: &str, arguments: Value, dtype: Representative| {
        assert_eq!(
            gives(&select_walk(method("a", entry, arguments))),
            select_after(dtype, Shape::Many),
            "{entry}"
        );
    };
    each("Expr.cum_min", json!({}), Representative::Int64);
    each("Expr.cum_prod", json!({}), Representative::Int64);
    for entry in [
        "Expr.bitwise_count_ones",
        "Expr.bitwise_count_zeros",
        "Expr.bitwise_leading_ones",
        "Expr.bitwise_leading_zeros",
        "Expr.bitwise_trailing_ones",
        "Expr.bitwise_trailing_zeros",
    ] {
        each(entry, json!({}), Representative::UInt32);
    }
}

#[test]
fn an_aggregate_beside_a_column_is_refused_in_a_join_key_as_any_aggregate_is() {
    // Each aggregate is one value, which a join's key may not hold beside the column `k`; it is cast
    // to a whole number first, so that the refusal is for the key and not for the comparison.
    for (entry, arguments, _) in aggregates() {
        let key = field_then(
            "a",
            vec![
                call(entry, arguments),
                call("Expr.cast", json!({ "dtype": dtype("Int64") })),
                call("Expr.gt", json!({ "other": field("k") })),
            ],
        );
        let walk = join_walk(key);
        let refusal = walk
            .refusal()
            .unwrap_or_else(|| panic!("{entry} beside a column is no join key"));
        assert_eq!(
            refusal.refusal().source(),
            RefusalSource::Rule(Rule::KeyIsElementwise),
            "{entry}"
        );
    }
}

#[test]
fn an_aggregate_alone_is_a_join_key_polars_takes_as_one_value() {
    for (entry, arguments, _) in aggregates() {
        let key = field_then(
            "a",
            vec![
                call(entry, arguments),
                call("Expr.cast", json!({ "dtype": dtype("Int64") })),
            ],
        );
        let walk = join_walk(key);
        assert!(
            walk.refusal().is_none(),
            "{entry} alone is a key: {:?}",
            walk.refusal().map(|refusal| refusal.refusal().source())
        );
    }
}

#[test]
fn a_bit_count_is_elementwise_so_a_join_may_key_on_it() {
    let key = field_then(
        "a",
        vec![
            call("Expr.bitwise_count_ones", json!({})),
            call("Expr.cast", json!({ "dtype": dtype("Int64") })),
        ],
    );
    let walk = join_walk(key);
    assert!(
        walk.refusal().is_none(),
        "{:?}",
        walk.refusal().map(|refusal| refusal.refusal().source())
    );
}
