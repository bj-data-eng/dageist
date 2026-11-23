//! What the catalogue says of the parameters that hold a duration string, and what Polars says of
//! the text typed into one.

use polars::prelude::DataType;

use crate::catalog::{DurationUse, any_moment, check_duration, entries};

#[test]
fn only_the_duration_of_a_method_that_reads_a_text_as_one_is_a_duration_string() {
    let mut named: Vec<(&str, &str, DurationUse)> = entries()
        .iter()
        .flat_map(|entry| {
            entry.parameters().iter().filter_map(|parameter| {
                Some((
                    entry.id().as_str(),
                    parameter.name(),
                    parameter.duration_string()?,
                ))
            })
        })
        .collect();
    named.sort_by_key(|(id, _, _)| *id);
    // The signatures of `api/polars/python.txt`: `str | timedelta | Expr` for the length of the
    // period a moment is cut or rounded to, and the amount `offset_by` moves it by.
    assert_eq!(
        named,
        [
            ("Expr.dt.offset_by", "by", DurationUse::Offset),
            ("Expr.dt.round", "every", DurationUse::Round),
            ("Expr.dt.truncate", "every", DurationUse::Cut),
        ]
    );
}

#[test]
fn a_duration_string_is_whole_amounts_of_units_and_is_refused_in_the_words_polars_gives() {
    for using in [DurationUse::Offset, DurationUse::Cut, DurationUse::Round] {
        for text in ["1d", "1h", "30m", "1mo", "1w", "1y", "500ms", "+1d", "1q"] {
            assert_eq!(
                check_duration(text, using, &any_moment()),
                Ok(()),
                "{text} {using:?}"
            );
        }
        let refused = [
            (
                "d",
                "expected leading integer in the duration string, found 'd'",
            ),
            (
                "1",
                "expected a valid unit to follow integer in the duration string '1'",
            ),
            (
                "1x",
                "unit: 'x' not supported; available units are: 'y', 'mo', 'q', 'w', 'd', 'h', 'm', 's', 'ms', 'us', 'ns'",
            ),
            (
                "1d-1h",
                "only a single minus sign is allowed, at the front of the string",
            ),
            (
                "1.5d",
                "expected a valid unit to follow integer in the duration string '1.5d'",
            ),
            // Polars reads it and adds its amounts up unchecked, which a build cannot hold.
            (
                "99999999999y",
                "the duration is too large for Polars to hold",
            ),
        ];
        for (text, reason) in refused {
            let error = check_duration(text, using, &any_moment()).expect_err(text);
            assert_eq!(error.reason(), reason, "{text} {using:?}");
            assert_eq!(error.to_string(), reason, "{text} {using:?}");
        }
    }
}

#[test]
fn a_duration_that_is_read_is_still_refused_where_the_method_cannot_cut_or_round_by_it() {
    // A period is no negative length, which Polars says of the method it is given to; a move is
    // by a length that may be negative.
    assert_eq!(
        check_duration("-1d", DurationUse::Offset, &any_moment()),
        Ok(())
    );
    for (using, said) in [
        (
            DurationUse::Cut,
            "cannot truncate a Datetime to a negative duration",
        ),
        (
            DurationUse::Round,
            "cannot round a Datetime to a negative duration",
        ),
    ] {
        let error = check_duration("-1d", using, &any_moment()).expect_err("a negative period");
        assert_eq!(error.reason(), said, "{using:?}");
    }
}

#[test]
fn what_a_method_refuses_of_a_duration_depends_on_the_dtype_of_what_it_is_given_to() {
    // A moment is cut by a length of days and hours together; a date has no hours to cut by.
    assert_eq!(
        check_duration("2d12h", DurationUse::Cut, &any_moment()),
        Ok(())
    );
    let error = check_duration("2d12h", DurationUse::Cut, &DataType::Date).expect_err("a date");
    assert_eq!(
        error.reason(),
        "cannot mix month, week, day, and sub-daily units for this operation"
    );
    // A length of nothing is a period of no length to a date, and a time of day is cut by it.
    let error = check_duration("0d", DurationUse::Cut, &DataType::Date).expect_err("a date");
    assert_eq!(error.reason(), "duration cannot be zero");
    assert_eq!(
        check_duration("0d", DurationUse::Cut, &any_moment()),
        Ok(())
    );
}

#[test]
fn a_length_of_nanoseconds_is_read_of_a_moment_counted_in_them_and_refused_in_polars_words_of_others()
 {
    use polars::prelude::TimeUnit;
    let counted = |unit| DataType::Datetime(unit, None);
    assert_eq!(
        check_duration("1ns", DurationUse::Round, &counted(TimeUnit::Nanoseconds)),
        Ok(())
    );
    // Counted in microseconds the length is none, which Polars' rounding divides by and panics, in
    // the words it panics with.
    let error = check_duration("1ns", DurationUse::Round, &counted(TimeUnit::Microseconds))
        .expect_err("a length of nothing");
    assert!(
        error.reason().contains("divisor of zero"),
        "{}",
        error.reason()
    );
}
