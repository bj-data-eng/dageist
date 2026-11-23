//! What the catalogue says of the parameters that hold a time zone, and what Polars says of the text
//! typed into one.

use crate::catalog::{Kind, check_time_zone, entries};

#[test]
fn only_the_zone_of_a_call_that_localizes_converts_or_makes_a_moment_is_a_time_zone() {
    let mut named: Vec<(&str, &str, bool)> = entries()
        .iter()
        .flat_map(|entry| {
            entry
                .parameters()
                .iter()
                .filter(|parameter| parameter.kind() == Kind::TimeZone)
                .map(|parameter| {
                    (
                        entry.id().as_str(),
                        parameter.name(),
                        parameter.takes_no_time_zone(),
                    )
                })
        })
        .collect();
    named.sort_unstable();
    // The signatures of `api/polars/python.txt`: `time_zone: str | None` of `replace_time_zone`, which
    // takes the zone off with `None`, `time_zone: str` of `convert_time_zone`, which does not, and
    // `time_zone: str | None` of `polars.datetime`, a moment of none where it is not given.
    assert_eq!(
        named,
        [
            ("Expr.dt.convert_time_zone", "time_zone", false),
            ("Expr.dt.replace_time_zone", "time_zone", true),
            ("polars.datetime", "time_zone", true),
        ]
    );
}

#[test]
fn a_time_zone_is_a_name_of_the_database_or_an_offset_and_is_refused_in_the_words_polars_gives() {
    for (text, zone) in [
        ("UTC", "UTC"),
        ("Europe/Berlin", "Europe/Berlin"),
        ("America/New_York", "America/New_York"),
        ("Etc/GMT+5", "Etc/GMT+5"),
        // Polars reads a fixed offset as the zone of the database that has it.
        ("+01:00", "Etc/GMT-1"),
    ] {
        let read = check_time_zone(text).expect(text).expect("a zone");
        assert_eq!(read.to_string(), zone, "{text}");
    }
    // The empty text is no zone, which Polars reads so.
    assert_eq!(check_time_zone(""), Ok(None));
    // What Polars says up to its first blank line, and not the hint and the advice that follow.
    for text in ["Not/AZone", "europe/berlin", "Z", "America/New_York "] {
        let error = check_time_zone(text).expect_err(text);
        assert_eq!(
            error.reason(),
            format!(
                "unable to parse time zone: '{text}'. Please check the Time Zone Database for a list of available time zones."
            ),
            "{text}"
        );
        assert_eq!(error.to_string(), error.reason(), "{text}");
    }
}
