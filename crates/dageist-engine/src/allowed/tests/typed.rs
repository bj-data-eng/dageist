//! What a typed text reads as, against independent authorities: the grammar of
//! a JSON number (RFC 8259), the formats Polars lists for reading a date and the
//! units it lists for a duration, and Polars run on the same text.

use polars::prelude::{
    DataFrame, DataType, IntoLazy, NamedFrom, PlSmallStr, Series, StrptimeOptions, TimeUnit, col,
    lit,
};

use crate::allowed::{Literal, Read, Representative, Typed, Unread, read_typed};
use crate::catalog::ValueType;

/// The readings of `text` typed into `typed`, each by its literal and the name of its dtype, or
/// by why it is not read.
fn readings(text: &str, typed: Typed) -> Vec<Result<(Literal, Option<&'static str>), Unread>> {
    read_typed(text, typed)
        .into_iter()
        .map(|reading| {
            reading.map(|read: Read| {
                (
                    read.literal().clone(),
                    read.dtype().map(Representative::name),
                )
            })
        })
        .collect()
}

fn text(value: &str) -> Result<(Literal, Option<&'static str>), Unread> {
    Ok((Literal::String(value.to_owned()), Some("String")))
}

fn integer(value: i64, dtype: &'static str) -> Result<(Literal, Option<&'static str>), Unread> {
    Ok((Literal::Integer(value), Some(dtype)))
}

fn float(value: f64) -> Result<(Literal, Option<&'static str>), Unread> {
    Ok((Literal::Float(value), Some("Float64")))
}

#[test]
fn a_number_typed_into_an_expression_reads_as_the_document_writes_it_and_as_text() {
    // A whole number is the narrowest integer it fits, a number with a fraction or an exponent
    // is a float, and the text itself is always a reading.
    for (typed, number) in [
        ("12", integer(12, "Int32")),
        ("-5", integer(-5, "Int32")),
        ("0", integer(0, "Int32")),
        ("3000000000", integer(3_000_000_000, "Int64")),
        ("1.5", float(1.5)),
        ("-0.25", float(-0.25)),
        ("1.0", float(1.0)),
        ("1e3", float(1000.0)),
        ("2.5E-1", float(0.25)),
    ] {
        assert_eq!(
            readings(typed, Typed::Expression),
            [number, text(typed)],
            "{typed}"
        );
    }
}

#[test]
fn white_space_around_a_number_is_not_part_of_it_and_the_text_is_kept_as_typed() {
    assert_eq!(
        readings(" 5\t", Typed::Expression),
        [integer(5, "Int32"), text(" 5\t")]
    );
}

#[test]
fn what_json_does_not_write_as_a_number_is_only_text() {
    // A leading zero, a plus, a bare fraction or exponent, a number that is more than one, and
    // what is no number at all.
    for typed in [
        "01", "+1", ".5", "1.", "1e", "1 2", "1,5", "0x10", "abc", "NaN", "inf", "1_000", "--1",
        "-",
    ] {
        assert_eq!(
            readings(typed, Typed::Expression),
            [text(typed)],
            "{typed:?}"
        );
    }
}

#[test]
fn a_whole_number_past_what_a_json_number_carries_exactly_reads_as_a_number_with_a_fraction() {
    // 2^53 - 1 is the largest whole number every one of whose predecessors an f64 holds.
    assert_eq!(
        readings("9007199254740991", Typed::Expression)[0],
        integer(9_007_199_254_740_991, "Int64")
    );
    assert_eq!(
        readings("-9007199254740991", Typed::Expression)[0],
        integer(-9_007_199_254_740_991, "Int64")
    );
    // The next one, one an i64 holds, and one no i64 holds.
    assert_eq!(
        readings("9007199254740992", Typed::Expression)[0],
        float(9_007_199_254_740_992.0)
    );
    assert_eq!(
        readings("99999999999999999999", Typed::Expression)[0],
        float(1e20)
    );
}

#[test]
fn nothing_typed_reads_as_nothing() {
    for typed in [
        Typed::Expression,
        Typed::Value(ValueType::Integer),
        Typed::Value(ValueType::Float),
        Typed::Value(ValueType::String),
        Typed::Value(ValueType::Date),
        Typed::Value(ValueType::Any),
        Typed::Duration,
        Typed::Path,
    ] {
        assert!(read_typed("", typed).is_empty(), "{typed:?}");
    }
}

#[test]
fn a_slot_that_takes_a_whole_number_reads_one_and_says_why_it_is_not() {
    let whole = Typed::Value(ValueType::Integer);
    assert_eq!(readings("12", whole), [integer(12, "Int32")]);
    assert_eq!(
        readings("-3000000000", whole),
        [integer(-3_000_000_000, "Int64")]
    );
    for (typed, why) in [
        ("1.5", Unread::NotAWholeNumber),
        ("1.0", Unread::NotAWholeNumber),
        ("1e3", Unread::NotAWholeNumber),
        ("9007199254740992", Unread::TooLargeForAWholeNumber),
        ("99999999999999999999", Unread::TooLargeForAWholeNumber),
        ("01", Unread::LeadingZero),
        ("abc", Unread::NotANumber),
        ("1e999", Unread::OutOfRange),
    ] {
        assert_eq!(readings(typed, whole), [Err(why)], "{typed}");
    }
}

#[test]
fn a_slot_that_takes_a_number_with_a_fraction_reads_a_whole_number_as_one() {
    let number = Typed::Value(ValueType::Float);
    assert_eq!(readings("2", number), [float(2.0)]);
    assert_eq!(readings("2.5", number), [float(2.5)]);
    assert_eq!(readings("99999999999999999999", number), [float(1e20)]);
    assert_eq!(readings("abc", number), [Err(Unread::NotANumber)]);
    assert_eq!(readings("1e999", number), [Err(Unread::OutOfRange)]);
}

#[test]
fn a_slot_that_takes_a_number_or_text_reads_a_number_and_then_text() {
    assert_eq!(
        readings("7", Typed::Value(ValueType::Any)),
        [integer(7, "Int32"), text("7")]
    );
    assert_eq!(
        readings("seven", Typed::Value(ValueType::Any)),
        [text("seven")]
    );
}

#[test]
fn a_slot_that_takes_text_reads_it_as_it_is_even_if_it_is_a_number() {
    assert_eq!(readings("7", Typed::Value(ValueType::String)), [text("7")]);
    assert_eq!(
        readings("  two words ", Typed::Value(ValueType::String)),
        [text("  two words ")]
    );
}

#[test]
fn a_path_is_text_that_has_no_dtype() {
    assert_eq!(
        readings("data/claims.parquet", Typed::Path),
        [Ok((
            Literal::String("data/claims.parquet".to_owned()),
            None
        ))]
    );
}

#[test]
fn a_date_reads_as_polars_reads_one_given_no_format_and_is_written_as_the_day() {
    let date = Typed::Value(ValueType::Date);
    let day = |value: &str| Ok((Literal::String(value.to_owned()), Some("Date")));
    // Year, month, day with each separator Polars lists, and day, month, year.
    for typed in [
        "2026-03-10",
        "2026/03/10",
        "2026.03.10",
        "10-03-2026",
        "10/03/2026",
        "10.03.2026",
    ] {
        assert_eq!(readings(typed, date), [day("2026-03-10")], "{typed}");
    }
}

#[test]
fn text_that_is_no_date_to_polars_is_refused_with_its_reason() {
    let date = Typed::Value(ValueType::Date);
    // A month that does not exist, a day that does not exist, a word, and a moment, which is a
    // datetime and not a date.
    for typed in [
        "2026-13-01",
        "2026-02-30",
        "yesterday",
        "2026-03-10 12:30:00",
        "2026",
    ] {
        let read = readings(typed, date);
        let [Err(Unread::NotADate(reason))] = read.as_slice() else {
            panic!("{typed} is no date");
        };
        assert!(!reason.is_empty(), "{typed}");
    }
}

#[test]
fn a_date_is_read_as_polars_reads_it_when_it_runs_a_string() {
    // The same texts given to `str.to_date` as a column run by Polars: it reads exactly those
    // that the typed text is read as.
    let options = || StrptimeOptions::default();
    for typed in [
        "2026-03-10",
        "2026/03/10",
        "10-03-2026",
        "2026-13-01",
        "2026-02-30",
        "yesterday",
        "2026-03-10 12:30:00",
        "2026",
        "10 March 2026",
        " 2026-03-10",
    ] {
        let frame = DataFrame::new(1, vec![Series::new(PlSmallStr::from("t"), [typed]).into()])
            .expect("a frame");
        let run = frame
            .lazy()
            .select([col("t").str().to_date(options())])
            .collect();
        let read = read_typed(typed, Typed::Value(ValueType::Date));
        assert_eq!(read[0].is_ok(), run.is_ok(), "{typed}: {run:?}");
    }
}

#[test]
fn a_duration_reads_as_polars_reads_a_duration_string() {
    let duration = Typed::Duration;
    let kept = |value: &str| Ok((Literal::String(value.to_owned()), None));
    // Whole amounts of units one after another, each of the units Polars lists, a sign in front.
    for typed in [
        "1d", "2d12h", "1mo", "-1y", "1w", "3q", "15m", "30s", "500ms", "7us", "9ns", "1i",
        "1y2mo3d",
    ] {
        assert_eq!(readings(typed, duration), [kept(typed)], "{typed}");
    }
    // No unit, a unit that is not one, a fraction, a space, a second sign, and no amount.
    for typed in [
        "1", "1x", "1.5d", "1d 2h", "1d-2h", "--1d", "d", "abc", "1D",
    ] {
        let read = readings(typed, duration);
        let [Err(Unread::NotADuration(reason))] = read.as_slice() else {
            panic!("{typed} is no duration");
        };
        assert!(!reason.is_empty(), "{typed}");
    }
}

#[test]
fn an_amount_too_large_for_polars_to_hold_is_no_duration_and_does_not_stop_the_engine() {
    for typed in ["99999999999999999999d", "9999999999999999h"] {
        assert!(
            matches!(
                readings(typed, Typed::Duration).as_slice(),
                [Err(Unread::NotADuration(_))]
            ),
            "{typed}"
        );
    }
}

#[test]
fn a_duration_is_read_as_polars_reads_the_text_given_to_offset_by() {
    // The texts a person types as a duration, given to `dt.offset_by` as a literal and run on a
    // timestamp: Polars accepts those that are read as durations and refuses the rest.
    for typed in [
        "1d", "2d12h", "1mo", "-1y", "1w", "15m", "1", "1x", "1.5d", "1d 2h", "abc",
    ] {
        let frame = DataFrame::new(1, vec![Series::new(PlSmallStr::from("t"), [0_i64]).into()])
            .expect("a frame");
        let moment = col("t").cast(DataType::Datetime(TimeUnit::Microseconds, None));
        let run = frame
            .lazy()
            .select([moment.dt().offset_by(lit(typed))])
            .collect();
        let read = read_typed(typed, Typed::Duration);
        assert_eq!(read[0].is_ok(), run.is_ok(), "{typed}: {run:?}");
    }
}

#[test]
fn the_words_that_say_why_a_text_is_not_read_are_those_a_person_reads() {
    for (why, words) in [
        (Unread::NotANumber, "not a number"),
        (Unread::LeadingZero, "a number has no leading zero"),
        (Unread::OutOfRange, "the number is too large"),
        (Unread::NotAWholeNumber, "not a whole number"),
        (
            Unread::TooLargeForAWholeNumber,
            "the whole number is too large to be written exactly; write it with a fraction",
        ),
        (Unread::NotADate("x".to_owned()), "x"),
        (Unread::NotADuration("y".to_owned()), "y"),
    ] {
        assert_eq!(why.to_string(), words);
    }
}

#[test]
fn the_smallest_and_largest_whole_numbers_an_i64_holds_are_read_without_overflowing() {
    // 2^53 - 1 is the largest whole number read as one, on either side of zero; the numbers
    // beyond it, up to and including the ends of an i64, read as the number with a fraction
    // nearest to them, and a slot that takes a whole number refuses them.
    let whole = Typed::Value(ValueType::Integer);
    for (typed, nearest) in [
        ("9007199254740991", None),
        ("-9007199254740991", None),
        ("9007199254740992", Some(9_007_199_254_740_992.0)),
        ("-9007199254740992", Some(-9_007_199_254_740_992.0)),
        ("9223372036854775807", Some(9_223_372_036_854_775_808.0)),
        ("-9223372036854775807", Some(-9_223_372_036_854_775_808.0)),
        ("-9223372036854775808", Some(-9_223_372_036_854_775_808.0)),
    ] {
        match nearest {
            None => {
                let value: i64 = typed.parse().expect("a whole number");
                assert_eq!(
                    readings(typed, Typed::Expression)[0],
                    integer(value, "Int64")
                );
                assert_eq!(readings(typed, whole), [integer(value, "Int64")]);
            }
            Some(value) => {
                assert_eq!(
                    readings(typed, Typed::Expression)[0],
                    float(value),
                    "{typed}"
                );
                assert_eq!(
                    readings(typed, whole),
                    [Err(Unread::TooLargeForAWholeNumber)],
                    "{typed}"
                );
                assert_eq!(
                    readings(typed, Typed::Value(ValueType::Float)),
                    [float(value)]
                );
            }
        }
    }
    // One past either end of an i64 is a whole number no i64 holds, and reads the same way.
    assert_eq!(
        readings("-9223372036854775809", Typed::Expression)[0],
        float(-9_223_372_036_854_775_808.0)
    );
}

#[test]
fn an_amount_that_does_not_fit_in_nanoseconds_is_no_duration_whatever_the_build() {
    // The number of each unit in an i64 of nanoseconds, worked out from the lengths of the
    // units: a day is 24 hours, a week 7 days.
    let ns: i64 = 1;
    let limit = |unit_ns: i64| i64::MAX / unit_ns;
    let hour = 3_600 * 1_000_000_000_i64;
    let day = 24 * hour;
    let week = 7 * day;
    let read = |text: &str| readings(text, Typed::Duration);
    let kept = |text: &str| Ok((Literal::String(text.to_owned()), None));
    for (unit, per) in [
        ("ns", ns),
        ("us", 1_000),
        ("ms", 1_000_000),
        ("s", 1_000_000_000),
    ] {
        let largest = format!("{}{unit}", limit(per));
        assert_eq!(read(&largest), [kept(&largest)], "{largest}");
        let beyond = format!("{}{unit}", limit(per) as i128 + 1);
        assert!(
            matches!(read(&beyond).as_slice(), [Err(Unread::NotADuration(_))]),
            "{beyond}"
        );
    }
    for (unit, per) in [
        ("m", 60 * 1_000_000_000_i64),
        ("h", hour),
        ("d", day),
        ("w", week),
    ] {
        let largest = format!("{}{unit}", limit(per));
        assert_eq!(read(&largest), [kept(&largest)], "{largest}");
        let beyond = format!("{}{unit}", limit(per) + 1);
        assert!(
            matches!(read(&beyond).as_slice(), [Err(Unread::NotADuration(_))]),
            "{beyond}"
        );
    }
    // The amounts add up: a week and a day past the limit of days, and a sign changes nothing.
    for text in [
        "106751d1d",
        "15250w106751d",
        "-106751d24h",
        "99999999999999w",
        "9999999d",
    ] {
        assert!(
            matches!(read(text).as_slice(), [Err(Unread::NotADuration(_))]),
            "{text}"
        );
    }
    // Months are counted, not timed: a quarter is 3 and a year 12 of them, and what an i64 of
    // months cannot count is refused.
    for text in [
        "9223372036854775807mo",
        "3074457345618258602q",
        "768614336404564650y",
    ] {
        assert_eq!(read(text), [kept(text)], "{text}");
    }
    for text in [
        "9223372036854775808mo",
        "3074457345618258603q",
        "768614336404564651y",
    ] {
        assert!(
            matches!(read(text).as_slice(), [Err(Unread::NotADuration(_))]),
            "{text}"
        );
    }
}
