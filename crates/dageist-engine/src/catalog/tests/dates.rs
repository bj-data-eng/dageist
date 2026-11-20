//! The methods of the `dt` namespace built through the catalogue and run on small tables, against
//! the values and the dtypes Polars' documentation gives them, which are written here by hand from
//! the calendar: a moment is split into its parts, a length of time into its totals, and a time
//! stamp is counted in a unit.

use polars::prelude::{DataFrame, DataType, IntoLazy, TimeUnit, col, df, lit};

use crate::catalog::{Arguments, entry};

use super::build::{choice, expression_arg, flag, method, none, run};

/// 2024-02-29 13:45:30.123456 (a Thursday, in a leap year) and 2023-02-10, which are
/// 1,709,214,330,123,456 microseconds and 19,398 days after the start of 1970, each beside a null.
fn moments() -> DataFrame {
    df!(
        "t" => [Some(1_709_214_330_123_456_i64), None],
        "d" => [Some(19_398_i32), None],
    )
    .expect("a table")
    .lazy()
    .with_columns([
        col("t").cast(DataType::Datetime(TimeUnit::Microseconds, None)),
        col("d").cast(DataType::Date),
    ])
    .collect()
    .expect("a table")
}

/// The values `id` gives the column `column` of the moments, written as whole numbers.
fn whole_numbers(id: &str, column: &str, arguments: &Arguments) -> (DataType, Vec<Option<i64>>) {
    let expr = method(id, col(column), arguments).expect("the method builds");
    let out = run(moments().lazy().select([expr.alias("out")]));
    let column = out.column("out").expect("a column");
    let values = column
        .cast(&DataType::Int64)
        .expect("whole numbers")
        .i64()
        .expect("whole numbers")
        .iter()
        .collect();
    (column.dtype().clone(), values)
}

#[test]
fn a_moment_is_split_into_the_parts_of_the_calendar_with_the_dtypes_polars_documents() {
    // Each part of 2024-02-29 13:45:30.123456 and its dtype; the null stays a null.
    let parts: [(&str, DataType, i64); 14] = [
        ("Expr.dt.year", DataType::Int32, 2024),
        ("Expr.dt.iso_year", DataType::Int32, 2024),
        ("Expr.dt.quarter", DataType::Int8, 1),
        ("Expr.dt.month", DataType::Int8, 2),
        ("Expr.dt.week", DataType::Int8, 9),
        ("Expr.dt.weekday", DataType::Int8, 4),
        ("Expr.dt.day", DataType::Int8, 29),
        ("Expr.dt.ordinal_day", DataType::Int16, 60),
        ("Expr.dt.century", DataType::Int32, 21),
        ("Expr.dt.millennium", DataType::Int32, 3),
        ("Expr.dt.days_in_month", DataType::Int8, 29),
        ("Expr.dt.hour", DataType::Int8, 13),
        ("Expr.dt.minute", DataType::Int8, 45),
        ("Expr.dt.second", DataType::Int8, 30),
    ];
    for (id, dtype, expected) in parts {
        let (found, values) = whole_numbers(id, "t", &none());
        assert_eq!(found, dtype, "{id}");
        assert_eq!(values, [Some(expected), None], "{id}");
    }
    let fractions: [(&str, i64); 3] = [
        ("Expr.dt.millisecond", 123),
        ("Expr.dt.microsecond", 123_456),
        ("Expr.dt.nanosecond", 123_456_000),
    ];
    for (id, expected) in fractions {
        let (found, values) = whole_numbers(id, "t", &none());
        assert_eq!(found, DataType::Int32, "{id}");
        assert_eq!(values, [Some(expected), None], "{id}");
    }
}

#[test]
fn the_parts_of_a_date_are_those_of_the_calendar_and_a_year_of_28_days_is_no_leap_year() {
    // 2023-02-10 is a Friday, the 41st day of a year that is not a leap year, in a month of 28
    // days.
    let parts: [(&str, i64); 6] = [
        ("Expr.dt.year", 2023),
        ("Expr.dt.month", 2),
        ("Expr.dt.day", 10),
        ("Expr.dt.weekday", 5),
        ("Expr.dt.ordinal_day", 41),
        ("Expr.dt.days_in_month", 28),
    ];
    for (id, expected) in parts {
        let (_, values) = whole_numbers(id, "d", &none());
        assert_eq!(values, [Some(expected), None], "{id}");
    }
    let (dtype, leap) = whole_numbers("Expr.dt.is_leap_year", "d", &none());
    assert_eq!(dtype, DataType::Boolean);
    assert_eq!(leap, [Some(0), None]);
    let (_, leap) = whole_numbers("Expr.dt.is_leap_year", "t", &none());
    assert_eq!(leap, [Some(1), None]);
}

#[test]
fn the_last_day_of_the_month_keeps_the_dtype_of_what_it_follows() {
    // 2023-02-28 is 19,416 days after the start of 1970.
    let (dtype, days) = whole_numbers("Expr.dt.month_end", "d", &none());
    assert_eq!(dtype, DataType::Date);
    assert_eq!(days, [Some(19_416), None]);
    let (dtype, _) = whole_numbers("Expr.dt.month_end", "t", &none());
    assert_eq!(dtype, DataType::Datetime(TimeUnit::Microseconds, None));
}

#[test]
fn the_second_has_a_fraction_only_when_it_is_asked_for() {
    let expr = method(
        "Expr.dt.second",
        col("t"),
        &Arguments::new().with("fractional", flag(true)),
    )
    .expect("the method builds");
    let out = run(moments().lazy().select([expr.alias("out")]));
    let column = out.column("out").expect("a column");
    assert_eq!(column.dtype(), &DataType::Float64);
    let seconds: Vec<_> = column.f64().expect("floats").iter().collect();
    assert_eq!(seconds, [Some(30.123_456), None]);
    let expr = method(
        "Expr.dt.second",
        col("t"),
        &Arguments::new().with("fractional", flag(false)),
    )
    .expect("the method builds");
    let out = run(moments().lazy().select([expr.alias("out")]));
    assert_eq!(
        out.column("out").expect("a column").dtype(),
        &DataType::Int8
    );
}

/// 1 day, 2 hours, 3 minutes and 4.5 seconds, in microseconds.
fn a_length_of_time() -> DataFrame {
    df!("n" => [Some(93_784_500_000_i64), None])
        .expect("a table")
        .lazy()
        .with_columns([col("n").cast(DataType::Duration(TimeUnit::Microseconds))])
        .collect()
        .expect("a table")
}

#[test]
fn a_length_of_time_is_counted_in_whole_units_and_in_units_with_a_fraction() {
    let totals: [(&str, i64); 7] = [
        ("Expr.dt.total_days", 1),
        ("Expr.dt.total_hours", 26),
        ("Expr.dt.total_minutes", 1_563),
        ("Expr.dt.total_seconds", 93_784),
        ("Expr.dt.total_milliseconds", 93_784_500),
        ("Expr.dt.total_microseconds", 93_784_500_000),
        ("Expr.dt.total_nanoseconds", 93_784_500_000_000),
    ];
    for (id, expected) in totals {
        let expr = method(id, col("n"), &none()).expect("the method builds");
        let out = run(a_length_of_time().lazy().select([expr.alias("out")]));
        let column = out.column("out").expect("a column");
        assert_eq!(column.dtype(), &DataType::Int64, "{id}");
        let values: Vec<_> = column.i64().expect("whole numbers").iter().collect();
        assert_eq!(values, [Some(expected), None], "{id}");
    }
    let fractions: [(&str, f64); 3] = [
        ("Expr.dt.total_seconds", 93_784.5),
        ("Expr.dt.total_milliseconds", 93_784_500.0),
        ("Expr.dt.total_minutes", 1_563.075),
    ];
    for (id, expected) in fractions {
        let given = Arguments::new().with("fractional", flag(true));
        let expr = method(id, col("n"), &given).expect("the method builds");
        let out = run(a_length_of_time().lazy().select([expr.alias("out")]));
        let column = out.column("out").expect("a column");
        assert_eq!(column.dtype(), &DataType::Float64, "{id}");
        let values: Vec<_> = column.f64().expect("floats").iter().collect();
        assert_eq!(values.len(), 2, "{id}");
        assert!(
            (values[0].expect("a value") - expected).abs() < 1e-9,
            "{id}"
        );
        assert_eq!(values[1], None, "{id}");
    }
}

#[test]
fn a_moment_is_counted_since_the_start_of_1970_in_the_unit_asked_for() {
    // 2024-02-29 13:45:30.123456.
    let counts: [(&str, &str, i64); 7] = [
        ("Expr.dt.timestamp", "us", 1_709_214_330_123_456),
        ("Expr.dt.timestamp", "ms", 1_709_214_330_123),
        ("Expr.dt.timestamp", "ns", 1_709_214_330_123_456_000),
        ("Expr.dt.epoch", "us", 1_709_214_330_123_456),
        ("Expr.dt.epoch", "ms", 1_709_214_330_123),
        ("Expr.dt.epoch", "s", 1_709_214_330),
        ("Expr.dt.epoch", "d", 19_782),
    ];
    for (id, unit, expected) in counts {
        let given = Arguments::new().with("time_unit", choice(unit));
        let (dtype, values) = whole_numbers(id, "t", &given);
        let wanted = if (id, unit) == ("Expr.dt.epoch", "d") {
            DataType::Int32
        } else {
            DataType::Int64
        };
        assert_eq!(dtype, wanted, "{id} {unit}");
        assert_eq!(values, [Some(expected), None], "{id} {unit}");
    }
    // The unit is `us` where none is given.
    let (_, values) = whole_numbers("Expr.dt.timestamp", "t", &none());
    assert_eq!(values, [Some(1_709_214_330_123_456), None]);
    let (_, values) = whole_numbers("Expr.dt.epoch", "t", &none());
    assert_eq!(values, [Some(1_709_214_330_123_456), None]);
    // A date is counted from midnight, in days or in seconds.
    let given = Arguments::new().with("time_unit", choice("d"));
    let (_, values) = whole_numbers("Expr.dt.epoch", "d", &given);
    assert_eq!(values, [Some(19_398), None]);
    let given = Arguments::new().with("time_unit", choice("s"));
    let (_, values) = whole_numbers("Expr.dt.epoch", "d", &given);
    assert_eq!(values, [Some(19_398 * 86_400), None]);
}

#[test]
fn a_time_unit_is_one_of_the_words_polars_takes_and_epoch_also_counts_seconds_and_days() {
    let words = |id: &str| {
        let parameter = entry(id)
            .expect("an entry")
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == "time_unit")
            .expect("a time unit");
        match parameter.kind() {
            crate::catalog::Kind::Choice(words) => words,
            other => panic!("{id} takes {other:?}"),
        }
    };
    for id in ["Expr.dt.cast_time_unit", "Expr.dt.timestamp"] {
        assert_eq!(words(id), ["ns", "us", "ms"], "{id}");
    }
    assert_eq!(words("Expr.dt.epoch"), ["ns", "us", "ms", "s", "d"]);
}

#[test]
fn a_moment_is_cast_to_another_time_unit_and_keeps_its_value() {
    for (unit, wanted, expected) in [
        ("ms", TimeUnit::Milliseconds, 1_709_214_330_123_i64),
        ("ns", TimeUnit::Nanoseconds, 1_709_214_330_123_456_000_i64),
    ] {
        let given = Arguments::new().with("time_unit", choice(unit));
        let expr = method("Expr.dt.cast_time_unit", col("t"), &given).expect("the method builds");
        let out = run(moments().lazy().select([expr.alias("out")]));
        let column = out.column("out").expect("a column");
        assert_eq!(column.dtype(), &DataType::Datetime(wanted, None), "{unit}");
        let counted: Vec<_> = column
            .cast(&DataType::Int64)
            .expect("whole numbers")
            .i64()
            .expect("whole numbers")
            .iter()
            .collect();
        assert_eq!(counted, [Some(expected), None], "{unit}");
    }
}

#[test]
fn a_moment_is_cut_to_the_start_of_its_period_and_rounded_to_the_nearest_one() {
    // 2024-02-29 13:45:30.123456 and the date 2023-02-10, cut and rounded by a duration string.
    let cut = |id: &str, column: &str, every: &str| {
        let given = Arguments::new().with("every", expression_arg(lit(every)));
        let expr = method(id, col(column), &given).expect("the method builds");
        let out = run(moments().lazy().select([expr.alias("out")]));
        let column = out.column("out").expect("a column");
        let dtype = column.dtype().clone();
        let values: Vec<_> = column
            .cast(&DataType::Int64)
            .expect("whole numbers")
            .i64()
            .expect("whole numbers")
            .iter()
            .collect();
        (dtype, values)
    };
    let datetime = DataType::Datetime(TimeUnit::Microseconds, None);
    // The start of the hour is 13:00 and of the month 2024-02-01: 1,709,211,600 and
    // 1,706,745,600 seconds after the start of 1970.
    let (dtype, values) = cut("Expr.dt.truncate", "t", "1h");
    assert_eq!(dtype, datetime);
    assert_eq!(values, [Some(1_709_211_600_000_000), None]);
    let (_, values) = cut("Expr.dt.truncate", "t", "1mo");
    assert_eq!(values, [Some(1_706_745_600_000_000), None]);
    // 13:45:30 is nearer 14:00 than 13:00, and the afternoon is nearer the next midnight.
    let (_, values) = cut("Expr.dt.round", "t", "1h");
    assert_eq!(values, [Some(1_709_215_200_000_000), None]);
    let (_, values) = cut("Expr.dt.round", "t", "1d");
    assert_eq!(values, [Some(1_709_251_200_000_000), None]);
    // 2023-02-01 is the 19,389th day of the epoch, 9 days before the 10th and nearer than March.
    let (dtype, values) = cut("Expr.dt.truncate", "d", "1mo");
    assert_eq!(dtype, DataType::Date);
    assert_eq!(values, [Some(19_389), None]);
    let (_, values) = cut("Expr.dt.round", "d", "1mo");
    assert_eq!(values, [Some(19_389), None]);
}
