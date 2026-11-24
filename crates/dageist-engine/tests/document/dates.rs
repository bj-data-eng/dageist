//! What the walk and a run say of the methods of the `dt` namespace that take a duration string,
//! on the claim-timing sample, whose `timestamp` column holds seven moments of March 2026: the
//! expected cells are worked out by hand from them, and the reasons are Polars' own.

use dageist_engine::allowed::{RefusalSource, Representative};
use dageist_engine::document::{Document, Outcome, Report, RunCause, Segment, StepId, StepRows};
use polars::prelude::IntoLazy;
use serde_json::{Value, json};

use crate::claims::{claims, inputs};
use crate::support::{call, chain, document, field_then, named, step, text};

/// The one-step document that selects `expression` as the column `out`.
fn selecting(expression: Value) -> String {
    document(vec![step(
        "s",
        "LazyFrame.select",
        json!({ "named_exprs": named(vec![("out", expression)]) }),
    )])
}

fn report_of(text: &str) -> Report {
    Document::read(text)
        .expect("a document")
        .run_on(&inputs(), &claims())
        .expect("the claims have a schema")
}

/// The moments with one method of the `dt` namespace after them.
fn moments_then(id: &str, arguments: Value) -> Value {
    field_then("timestamp", vec![call(id, arguments)])
}

fn cells(written: &str) -> Vec<Option<String>> {
    Document::read(written)
        .expect("a document")
        .preview(&inputs(), &claims(), &StepId::new("s").expect("an id"), 100)
        .expect("the rows")
        .rows()
        .iter()
        .map(|row| row[0].clone())
        .collect()
}

#[test]
fn moments_are_cut_to_the_start_of_their_day_and_rounded_to_the_nearest_hour() {
    let day = selecting(moments_then(
        "Expr.dt.truncate",
        json!({ "every": text("1d") }),
    ));
    let report = report_of(&day);
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Valid { .. }),
        "{:?}",
        report.steps()[0].outcome()
    );
    assert!(matches!(report.steps()[0].rows(), StepRows::Counted(_)));
    let days = [
        "2026-03-02",
        "2026-03-05",
        "2026-03-06",
        "2026-03-02",
        "2026-03-04",
        "2026-03-09",
        "2026-03-03",
    ];
    let expected: Vec<Option<String>> = days
        .iter()
        .map(|day| Some(format!("{day} 00:00:00")))
        .collect();
    assert_eq!(cells(&day), expected);

    let hour = selecting(moments_then(
        "Expr.dt.round",
        json!({ "every": text("1h") }),
    ));
    // 13:30 rounds up, 07:50 up, 10:00 stays, 09:15 down, 17:20 down, 11:05 down, 16:40 up.
    let hours = [
        "2026-03-02 14:00:00",
        "2026-03-05 08:00:00",
        "2026-03-06 10:00:00",
        "2026-03-02 09:00:00",
        "2026-03-04 17:00:00",
        "2026-03-09 11:00:00",
        "2026-03-03 17:00:00",
    ];
    let expected: Vec<Option<String>> = hours.iter().map(|hour| Some((*hour).to_owned())).collect();
    assert_eq!(cells(&hour), expected);
}

#[test]
fn a_duration_typed_as_no_duration_is_refused_at_the_parameter_in_polars_words() {
    for (id, parameter) in [
        ("Expr.dt.truncate", "every"),
        ("Expr.dt.round", "every"),
        ("Expr.dt.offset_by", "by"),
    ] {
        let written = selecting(moments_then(id, json!({ parameter: text("1x") })));
        let report = report_of(&written);
        let step = &report.steps()[0];
        let Outcome::Refused(refusal) = step.outcome() else {
            panic!("{id}: {:?}", step.outcome());
        };
        // The refusal is at the duration of the method: the step's expression, its link and its
        // argument.
        assert_eq!(
            refusal.place().path(),
            [
                Segment::Parameter("named_exprs".to_owned()),
                Segment::Value(0),
                Segment::Link(0),
                Segment::Parameter(parameter.to_owned()),
            ],
            "{id}"
        );
        assert!(matches!(refusal.cause(), RunCause::Build(_)), "{refusal}");
        let words = refusal.cause().to_string();
        assert!(words.contains("not a duration"), "{id}: {words}");
        assert!(
            words.contains("unit: 'x' not supported; available units are"),
            "{id}: {words}"
        );
        // The walk says it before the step is built, by the rule, with Polars' words in its own.
        let walked = step.walk().refusal().expect("the walk refuses it");
        assert!(
            matches!(walked.refusal().source(), RefusalSource::Rule(rule)
                if rule.name() == "duration-is-valid"),
            "{id}: {:?}",
            walked.refusal().source()
        );
        assert!(
            walked
                .refusal()
                .reason()
                .contains("unit: 'x' not supported"),
            "{id}"
        );
    }
}

#[test]
fn a_duration_that_is_a_column_is_polars_to_refuse_when_it_runs_the_rows() {
    // The owners are text and no duration, which Polars reads when the rows run and refuses.
    let written = selecting(moments_then(
        "Expr.dt.truncate",
        json!({ "every": crate::support::field("owner") }),
    ));
    let report = report_of(&written);
    let step = &report.steps()[0];
    assert_eq!(step.walk().refusal(), None);
    assert!(
        matches!(step.rows(), StepRows::Failed(_)),
        "{:?}",
        step.rows()
    );
}

#[test]
fn a_cut_moment_is_a_moment_and_the_walk_says_so() {
    let written = selecting(moments_then(
        "Expr.dt.truncate",
        json!({ "every": text("1mo") }),
    ));
    let report = report_of(&written);
    // The expression of the step is the column and the cut, and the duration is one of its own.
    let walked = report.steps()[0]
        .walk()
        .expressions()
        .iter()
        .find(|walked| walked.calls().len() == 2)
        .expect("the step's expression");
    let standing = walked.calls().last().expect("the call stands");
    let result = standing
        .verdict()
        .and_then(|verdict| verdict.allowance())
        .and_then(|allowance| allowance.result())
        .expect("the walk says what it gives");
    assert_eq!(result.class(), Some(Representative::Datetime));
}

/// A call whose argument `time_zone` is the zone `zone`, or no zone for `None`.
fn in_zone(id: &str, zone: Option<&str>) -> Value {
    call(id, json!({ "time_zone": { "time_zone": zone } }))
}

/// A call whose arguments are `time_zone` and the words `ambiguous` and `non_existent`.
fn localized(zone: &str, ambiguous: &str, non_existent: &str) -> Value {
    call(
        "Expr.dt.replace_time_zone",
        json!({
            "time_zone": { "time_zone": zone },
            "ambiguous": { "choice": ambiguous },
            "non_existent": { "choice": non_existent },
        }),
    )
}

/// The moment `microseconds` after the start of 1970, a `Datetime`, with `then` after it.
fn moment_then(microseconds: i64, then: Vec<Value>) -> Value {
    chain(
        call(
            "polars.lit",
            json!({
                "value": { "integer": microseconds },
                "dtype": { "dtype": "Datetime(time_unit='us', time_zone=None)" },
            }),
        ),
        then,
    )
}

fn hour() -> Value {
    call("Expr.dt.hour", json!({}))
}

#[test]
fn the_clock_of_a_moment_is_kept_by_a_new_zone_and_its_instant_by_a_converted_one() {
    // 13:30 on the 2nd of March 2026 is 18:30 in UTC where it was read in New York, which keeps
    // to the standard time, five hours behind UTC, until the second Sunday of March, the 8th, and
    // four behind after it.
    let written = selecting(moments_then_all(vec![
        in_zone("Expr.dt.replace_time_zone", Some("America/New_York")),
        in_zone("Expr.dt.convert_time_zone", Some("UTC")),
        hour(),
    ]));
    let report = report_of(&written);
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Valid { .. }),
        "{:?}",
        report.steps()[0].outcome()
    );
    let expected: Vec<Option<String>> = [18, 12, 15, 14, 22, 15, 21]
        .iter()
        .map(|hour| Some(hour.to_string()))
        .collect();
    assert_eq!(cells(&written), expected);
    // The clock is what it was, in the zone it was given.
    let written = selecting(moments_then_all(vec![
        in_zone("Expr.dt.replace_time_zone", Some("America/New_York")),
        hour(),
    ]));
    let expected: Vec<Option<String>> = [13, 7, 10, 9, 17, 11, 16]
        .iter()
        .map(|hour| Some(hour.to_string()))
        .collect();
    assert_eq!(cells(&written), expected);
}

fn moments_then_all(then: Vec<Value>) -> Value {
    field_then("timestamp", then)
}

#[test]
fn the_offsets_of_a_zone_are_a_length_of_time_that_is_one_hour_more_in_the_summer() {
    // New York is five hours behind UTC all year, and an hour ahead of that from the 8th of March.
    let total = |id: &str| {
        selecting(moments_then_all(vec![
            in_zone("Expr.dt.replace_time_zone", Some("America/New_York")),
            call(id, json!({})),
            call("Expr.dt.total_hours", json!({})),
        ]))
    };
    let base: Vec<Option<String>> = vec![Some("-5".to_owned()); 7];
    assert_eq!(cells(&total("Expr.dt.base_utc_offset")), base);
    let summer: Vec<Option<String>> = [0, 0, 0, 0, 0, 1, 0]
        .iter()
        .map(|hours| Some(hours.to_string()))
        .collect();
    assert_eq!(cells(&total("Expr.dt.dst_offset")), summer);
}

#[test]
fn a_clock_that_is_read_twice_or_never_is_resolved_as_the_words_say() {
    // 01:30 on the 3rd of November 2024 happens twice in New York, at 05:30 and at 06:30 UTC.
    let twice = 1_730_597_400_000_000;
    let in_utc = |ambiguous: &str| {
        selecting(moment_then(
            twice,
            vec![
                localized("America/New_York", ambiguous, "raise"),
                in_zone("Expr.dt.convert_time_zone", Some("UTC")),
                hour(),
            ],
        ))
    };
    assert_eq!(cells(&in_utc("earliest")), [Some("5".to_owned())]);
    assert_eq!(cells(&in_utc("latest")), [Some("6".to_owned())]);
    assert_eq!(cells(&in_utc("null")), [None]);
    let report = report_of(&in_utc("raise"));
    assert!(
        matches!(report.steps()[0].rows(), StepRows::Failed(_)),
        "{:?}",
        report.steps()[0].rows()
    );
    // 02:30 on the 10th of March 2024 never happens in New York, which skips from 02:00 to 03:00.
    let never = 1_710_037_800_000_000;
    let skipped = |non_existent: &str| {
        selecting(moment_then(
            never,
            vec![localized("America/New_York", "raise", non_existent), hour()],
        ))
    };
    assert_eq!(cells(&skipped("null")), [None]);
    let report = report_of(&skipped("raise"));
    assert!(
        matches!(report.steps()[0].rows(), StepRows::Failed(_)),
        "{:?}",
        report.steps()[0].rows()
    );
}

#[test]
fn a_time_zone_that_is_none_is_refused_at_the_parameter_in_polars_words() {
    for id in ["Expr.dt.replace_time_zone", "Expr.dt.convert_time_zone"] {
        let written = selecting(moments_then_all(vec![in_zone(id, Some("Not/AZone"))]));
        let report = report_of(&written);
        let step = &report.steps()[0];
        let Outcome::Refused(refusal) = step.outcome() else {
            panic!("{id}: {:?}", step.outcome());
        };
        assert_eq!(
            refusal.place().path().last(),
            Some(&Segment::Parameter("time_zone".to_owned())),
            "{id}"
        );
        let words = refusal.cause().to_string();
        assert!(
            words.contains("unable to parse time zone: 'Not/AZone'"),
            "{id}: {words}"
        );
        let walked = step.walk().refusal().expect("the walk refuses it");
        assert!(
            matches!(walked.refusal().source(), RefusalSource::Rule(rule)
                if rule.name() == "time-zone-is-valid"),
            "{id}: {:?}",
            walked.refusal().source()
        );
    }
}

#[test]
fn no_time_zone_takes_the_zone_off_and_only_replace_takes_it() {
    let written = selecting(moments_then_all(vec![
        in_zone("Expr.dt.replace_time_zone", Some("UTC")),
        in_zone("Expr.dt.replace_time_zone", None),
    ]));
    let report = report_of(&written);
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Valid { .. }),
        "{:?}",
        report.steps()[0].outcome()
    );
    // The walk says each gives a moment, with a zone and without one.
    let walked = report.steps()[0]
        .walk()
        .expressions()
        .iter()
        .find(|walked| walked.calls().len() == 3)
        .expect("the step's expression");
    let classes: Vec<Option<Representative>> = walked
        .calls()
        .iter()
        .skip(1)
        .map(|standing| {
            standing
                .verdict()
                .and_then(|verdict| verdict.allowance())
                .and_then(|allowance| allowance.result())
                .and_then(|result| result.class())
        })
        .collect();
    assert_eq!(
        classes,
        [
            Some(Representative::DatetimeZoned),
            Some(Representative::Datetime)
        ]
    );
    // A conversion to no zone is no conversion: Python's `convert_time_zone` takes a zone.
    let written = selecting(moments_then_all(vec![in_zone(
        "Expr.dt.convert_time_zone",
        None,
    )]));
    let report = report_of(&written);
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Refused(_)),
        "{:?}",
        report.steps()[0].outcome()
    );
}

#[test]
fn an_empty_zone_converts_to_utc_and_replaces_by_no_zone_as_polars_reads_it() {
    // Polars reads the empty text as no zone, and `convert_time_zone` gives a moment that has none
    // the zone UTC, so a moment converted to it has a zone whose offsets Polars reads.
    let written = selecting(moments_then_all(vec![
        in_zone("Expr.dt.replace_time_zone", Some("UTC")),
        in_zone("Expr.dt.convert_time_zone", Some("")),
        call("Expr.dt.base_utc_offset", json!({})),
        call("Expr.dt.total_hours", json!({})),
    ]));
    let report = report_of(&written);
    assert_eq!(report.steps()[0].walk().refusal(), None);
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Valid { .. }),
        "{:?}",
        report.steps()[0].outcome()
    );
    assert_eq!(cells(&written), vec![Some("0".to_owned()); 7]);
    // Replacing the zone by the empty text takes it off, as it does by none.
    let written = selecting(moments_then_all(vec![
        in_zone("Expr.dt.replace_time_zone", Some("UTC")),
        in_zone("Expr.dt.replace_time_zone", Some("")),
        call("Expr.dt.base_utc_offset", json!({})),
    ]));
    let report = report_of(&written);
    assert!(
        report.steps()[0].walk().refusal().is_some(),
        "a moment with no zone has no offsets"
    );
}

/// What the walk says the last call of the step's expression gives, by its name.
fn last_result_name(report: &Report) -> String {
    let walked = report.steps()[0]
        .walk()
        .expressions()
        .iter()
        .find(|walked| walked.calls().len() > 1)
        .expect("the step's expression");
    walked
        .calls()
        .last()
        .and_then(|standing| standing.verdict())
        .and_then(|verdict| verdict.allowance())
        .and_then(|allowance| allowance.result())
        .map(|result| result.name().to_owned())
        .expect("the walk says what it gives")
}

fn in_unit(unit: &str) -> Value {
    call(
        "Expr.dt.cast_time_unit",
        json!({ "time_unit": { "choice": unit } }),
    )
}

#[test]
fn the_unit_a_moment_is_held_in_is_the_one_the_walk_says_it_gives_and_asks_polars_with() {
    // A moment counted in nanoseconds is cut by a length of one nanosecond, which Polars rounds to
    // where it is counted in microseconds it cannot.
    let in_ns = selecting(moments_then_all(vec![
        in_unit("ns"),
        call("Expr.dt.round", json!({ "every": text("1ns") })),
    ]));
    let report = report_of(&in_ns);
    assert_eq!(report.steps()[0].walk().refusal(), None);
    assert!(
        matches!(report.steps()[0].outcome(), Outcome::Valid { .. }),
        "{:?}",
        report.steps()[0].outcome()
    );
    assert_eq!(last_result_name(&report), "Datetime(ns)");
    // What keeps the unit says it: a zone and a cut of a moment counted in nanoseconds.
    let zoned = selecting(moments_then_all(vec![
        in_unit("ns"),
        in_zone("Expr.dt.replace_time_zone", Some("UTC")),
    ]));
    assert_eq!(last_result_name(&report_of(&zoned)), "Datetime(ns, UTC)");
    let cut = selecting(moments_then_all(vec![
        in_unit("ms"),
        call("Expr.dt.truncate", json!({ "every": text("1d") })),
    ]));
    assert_eq!(last_result_name(&report_of(&cut)), "Datetime(ms)");
    // The same length of a moment counted in microseconds is none Polars can use.
    let in_us = selecting(moments_then_all(vec![call(
        "Expr.dt.round",
        json!({ "every": text("1ns") }),
    )]));
    let report = report_of(&in_us);
    let walked = report.steps()[0]
        .walk()
        .refusal()
        .expect("the walk refuses it");
    assert!(
        matches!(walked.refusal().source(), RefusalSource::Rule(rule)
            if rule.name() == "duration-is-valid"),
        "{:?}",
        walked.refusal().source()
    );
}

/// The date 1970-01-02, a value of one row, with `then` after it.
fn a_date_then(then: Vec<Value>) -> Value {
    chain(
        call(
            "polars.lit",
            json!({
                "value": { "integer": 1 },
                "dtype": { "dtype": "Date" },
            }),
        ),
        then,
    )
}

fn replaced_by_the_days_open() -> Value {
    call(
        "Expr.dt.replace",
        json!({ "month": crate::support::field("row_days") }),
    )
}

fn over_the_owners() -> Value {
    call(
        "Expr.over",
        json!({ "partition_by": crate::support::many(vec![crate::support::field("owner")]) }),
    )
}

#[test]
fn a_date_of_one_row_that_a_part_widens_is_no_input_of_a_window() {
    // Polars meets the one date with each of the parts where it is selected, and refuses it when a
    // window must map it to as many values as the table has rows.
    let widened = selecting(a_date_then(vec![
        replaced_by_the_days_open(),
        over_the_owners(),
    ]));
    let report = report_of(&widened);
    let walked = report.steps()[0]
        .walk()
        .refusal()
        .expect("the walk refuses it");
    assert!(
        matches!(walked.refusal().source(), RefusalSource::Rule(rule)
            if rule.name() == "window-input-is-not-widened"),
        "{:?}",
        walked.refusal().source()
    );
    assert!(
        walked.refusal().reason().contains("window"),
        "{}",
        walked.refusal().reason()
    );
    // Selected alone it is met with each part, and a value of many rows is no value that a part
    // widens, so a window takes it.
    let alone = selecting(a_date_then(vec![replaced_by_the_days_open()]));
    assert_eq!(report_of(&alone).steps()[0].walk().refusal(), None);
    let many = selecting(moments_then_all(vec![
        replaced_by_the_days_open(),
        over_the_owners(),
    ]));
    assert_eq!(report_of(&many).steps()[0].walk().refusal(), None);
}

#[test]
fn moments_in_different_zones_are_no_operands_of_a_comparison_a_difference_or_a_fill() {
    // 02:00 in Berlin and in UTC are not the same dtype to Polars, which finds no dtype the two have
    // in common, so it refuses to compare, subtract or fill them, as it does for any two dtypes that
    // have none.
    let berlin = || {
        vec![
            in_zone("Expr.dt.replace_time_zone", Some("UTC")),
            in_zone("Expr.dt.convert_time_zone", Some("Europe/Berlin")),
        ]
    };
    let utc = || moments_then_all(vec![in_zone("Expr.dt.replace_time_zone", Some("UTC"))]);
    for (id, parameter) in [
        ("Expr.le", "other"),
        ("Expr.eq", "other"),
        ("Expr.eq_missing", "other"),
        ("Expr.ne_missing", "other"),
        ("Expr.ne", "other"),
        ("Expr.lt", "other"),
        ("Expr.gt", "other"),
        ("Expr.ge", "other"),
        ("Expr.is_between", "lower_bound"),
        ("Expr.sub", "other"),
        ("Expr.fill_null", "value"),
    ] {
        let mut links = berlin();
        links.push(call(id, json!({ parameter: utc() })));
        let report = report_of(&selecting(moments_then_all(links)));
        let walked = report.steps()[0]
            .walk()
            .refusal()
            .unwrap_or_else(|| panic!("{id}: the walk refuses it"));
        assert!(
            matches!(walked.refusal().source(), RefusalSource::Rule(rule)
                if rule.name() == "moments-share-a-zone"),
            "{id}: {:?}",
            walked.refusal().source()
        );
        assert!(
            walked.refusal().reason().contains("Europe/Berlin"),
            "{id}: {}",
            walked.refusal().reason()
        );
    }
    // The same zone is the same dtype, however the moment came to be in it.
    let mut links = berlin();
    links.push(call(
        "Expr.le",
        json!({ "other": moments_then_all(berlin()) }),
    ));
    let report = report_of(&selecting(moments_then_all(links)));
    assert_eq!(report.steps()[0].walk().refusal(), None);
}

/// The moment 1 microsecond after the start of 1970 in `zone`, a literal that has the zone.
fn moment_in(zone: &str) -> Value {
    chain(
        call(
            "polars.lit",
            json!({
                "value": { "integer": 1 },
                "dtype": { "dtype": format!("Datetime(time_unit='us', time_zone='{zone}')") },
            }),
        ),
        vec![],
    )
}

fn in_utc() -> Vec<Value> {
    vec![in_zone("Expr.dt.replace_time_zone", Some("UTC"))]
}

fn refused_for_the_zones(report: &Report, id: &str, zones: &[&str]) {
    let walked = report.steps()[0]
        .walk()
        .refusal()
        .unwrap_or_else(|| panic!("{id}: the walk refuses it"));
    assert!(
        matches!(walked.refusal().source(), RefusalSource::Rule(rule)
            if rule.name() == "moments-share-a-zone"),
        "{id}: {:?}",
        walked.refusal().source()
    );
    for zone in zones {
        assert!(
            walked.refusal().reason().contains(zone),
            "{id}: {}",
            walked.refusal().reason()
        );
    }
}

#[test]
fn a_replacement_is_judged_by_its_new_values_and_its_default_and_not_by_the_value_it_follows() {
    // `replace_strict` of a moment in UTC by moments in Berlin gives moments in Berlin, whatever zone
    // the value it follows is in; its default is met with the new values and must share their zone.
    let replace = |new: Value, default: Value| {
        let mut links = in_utc();
        links.push(call(
            "Expr.replace_strict",
            json!({ "old": moment_in("UTC"), "new": new, "default": default }),
        ));
        selecting(moments_then_all(links))
    };
    let same = replace(moment_in("Europe/Berlin"), moment_in("Europe/Berlin"));
    assert_eq!(report_of(&same).steps()[0].walk().refusal(), None);
    let apart = replace(moment_in("Europe/Berlin"), moment_in("UTC"));
    refused_for_the_zones(
        &report_of(&apart),
        "replace_strict",
        &["Europe/Berlin", "UTC"],
    );
}

#[test]
fn the_values_of_a_condition_and_a_fill_value_are_moments_in_one_zone() {
    let berlin = || {
        let mut links = in_utc();
        links.push(in_zone("Expr.dt.convert_time_zone", Some("Europe/Berlin")));
        moments_then_all(links)
    };
    let utc = || moments_then_all(in_utc());
    let condition = |then: Value, otherwise: Value| {
        selecting(chain(
            call(
                "polars.when",
                json!({ "predicates": crate::support::many(vec![field_then(
                    "owner",
                    vec![call("Expr.is_null", json!({}))],
                )]) }),
            ),
            vec![
                call("When.then", json!({ "statement": then })),
                call("Then.otherwise", json!({ "statement": otherwise })),
            ],
        ))
    };
    let report = report_of(&condition(berlin(), utc()));
    refused_for_the_zones(&report, "otherwise", &["Europe/Berlin", "UTC"]);
    assert_eq!(
        report_of(&condition(utc(), utc())).steps()[0]
            .walk()
            .refusal(),
        None
    );
    // A fill of a shift is one of the values, and Polars cannot place a moment of another zone in it.
    let mut links = berlin_links();
    links.push(call(
        "Expr.shift",
        json!({ "fill_value": moment_in("UTC") }),
    ));
    // Polars panics when it runs the rows of such a fill, so the step is only run on the schema.
    let written = selecting(moments_then_all(links));
    let report = Document::read(&written)
        .expect("a document")
        .run(&inputs(), claims().lazy())
        .expect("the claims have a schema");
    refused_for_the_zones(&report, "shift", &["Europe/Berlin", "UTC"]);
}

#[test]
fn the_values_of_a_chained_condition_are_moments_in_one_zone_whether_or_not_it_has_an_otherwise() {
    // Polars meets the values of a condition where it works out the schema, so a `then` of a chain
    // that has another zone than the values before it is refused, with an `otherwise` after it or
    // not.
    let berlin = || {
        let mut links = in_utc();
        links.push(in_zone("Expr.dt.convert_time_zone", Some("Europe/Berlin")));
        moments_then_all(links)
    };
    let utc = || moments_then_all(in_utc());
    let predicate = || {
        crate::support::many(vec![field_then(
            "owner",
            vec![call("Expr.is_null", json!({}))],
        )])
    };
    let chained = |values: Vec<Value>, otherwise: Option<Value>| {
        let mut values = values.into_iter();
        let mut links = vec![call(
            "When.then",
            json!({ "statement": values.next().expect("a first value") }),
        )];
        for (at, value) in values.enumerate() {
            let when = if at == 0 {
                "Then.when"
            } else {
                "ChainedThen.when"
            };
            links.push(call(when, json!({ "predicates": predicate() })));
            links.push(call("ChainedWhen.then", json!({ "statement": value })));
        }
        if let Some(value) = otherwise {
            let id = if links.len() > 1 {
                "ChainedThen.otherwise"
            } else {
                "Then.otherwise"
            };
            links.push(call(id, json!({ "statement": value })));
        }
        selecting(chain(
            call("polars.when", json!({ "predicates": predicate() })),
            links,
        ))
    };
    // The second value, and a later one, and with an `otherwise` that agrees.
    for (values, otherwise) in [
        (vec![berlin(), utc()], None),
        (vec![utc(), utc(), berlin()], None),
        (vec![berlin(), utc()], Some(utc())),
    ] {
        let report = report_of(&chained(values, otherwise));
        refused_for_the_zones(&report, "a chained condition", &["Europe/Berlin", "UTC"]);
    }
    // The same zone is one dtype, and a condition that stops at its first `then` has one value.
    for (values, otherwise) in [
        (vec![utc(), utc()], None),
        (vec![berlin(), berlin(), berlin()], None),
        (vec![berlin()], None),
    ] {
        let report = report_of(&chained(values, otherwise));
        assert_eq!(report.steps()[0].walk().refusal(), None);
    }
}

fn berlin_links() -> Vec<Value> {
    let mut links = in_utc();
    links.push(in_zone("Expr.dt.convert_time_zone", Some("Europe/Berlin")));
    links
}
