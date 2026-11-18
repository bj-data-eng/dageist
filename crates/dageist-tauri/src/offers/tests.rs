//! The offers commands through Tauri's IPC layer, and the snapshots of what
//! they return. Every expected value is worked out by hand from what Polars
//! does with the combination and what the tables of allowed combinations say
//! of it, not read from a run.

use std::fmt::Write as _;

use dageist_engine::allowed::{
    After, ConditionState, Context, Nested, Outer, Representative, Shape,
};
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};

use super::{
    AfterRequest, ArgumentRequest, CandidateRequest, ChosenOptionRequest, PlaceRequest,
    PositionRequest, TypedRequest, ValueTypeRequest, fits_view, offers_view, options_view,
    read_view, step_starts_view,
};
use crate::report::{ConditionStateView, ValueShapeView};

/// Set to rewrite the snapshots instead of checking them.
const REFRESH: &str = "DAGEIST_REFRESH_OFFERS";

/// The command that refreshes the snapshots.
const REFRESH_COMMAND: &str = "cargo test --offline --locked -p dageist-tauri offers::tests::the_snapshots_are_what_the_commands_return";

fn window() -> WebviewWindow<MockRuntime> {
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            super::offers_at,
            super::candidate_fits,
            super::read_typed,
            super::option_offers,
            super::step_starts,
        ])
        .build(mock_context(noop_assets()))
        .expect("the mock application builds");
    WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .expect("the mock window builds")
}

/// Calls `command` as the view's IPC does, and gives the JSON it answers with
/// or the JSON it rejects with.
fn invoke(command: &str, arguments: Value) -> Result<Value, Value> {
    let request = InvokeRequest {
        cmd: command.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().expect("a valid origin"),
        body: InvokeBody::Json(arguments),
        headers: tauri::http::HeaderMap::default(),
        invoke_key: INVOKE_KEY.to_owned(),
    };
    match tauri::test::get_ipc_response(&window(), request)? {
        InvokeResponseBody::Json(text) => Ok(serde_json::from_str(&text).expect("JSON")),
        other => panic!("expected JSON, got {other:?}"),
    }
}

fn json_of(view: &impl serde::Serialize) -> Value {
    serde_json::to_value(view).expect("a view serializes")
}

fn outer(name: Outer) -> Vec<PositionRequest> {
    vec![PositionRequest::Outer { name }]
}

fn value(dtype: Representative, shape: ValueShapeView) -> AfterRequest {
    AfterRequest::Value { dtype, shape }
}

/// The entry `id` of the offers `answer`.
fn offered<'a>(answer: &'a Value, id: &str) -> &'a Value {
    answer["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["entry"] == id)
        .unwrap_or_else(|| panic!("no offer for {id}"))
}

/// The ids of the entries of `answer` that may stand.
fn allowed(answer: &Value) -> Vec<&str> {
    answer["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|entry| entry["verdict"]["kind"] == "allowed")
        .map(|entry| entry["entry"].as_str().expect("an id"))
        .collect()
}

#[test]
fn what_follows_a_datetime_is_what_polars_allows_of_it_with_the_reason_for_the_rest() {
    let answer = invoke(
        "offers_at",
        json!({
            "positions": [{ "kind": "outer", "name": "select" }],
            "after": { "kind": "value", "dtype": "Datetime(us)", "shape": "many" }
        }),
    )
    .expect("offers");
    assert_eq!(
        answer["place"],
        json!({
            "context": { "outer": "select", "nested": null },
            "after": { "kind": "value", "dtype": "Datetime(us)", "shape": "many" }
        })
    );
    // The date of a timestamp is a date; its time, a time; the first of them one timestamp.
    let result = |id: &str| {
        (
            offered(&answer, id)["verdict"]["result"].clone(),
            offered(&answer, id)["verdict"]["shape"].clone(),
        )
    };
    assert_eq!(result("Expr.dt.date"), (json!("Date"), json!("many")));
    assert_eq!(result("Expr.dt.time"), (json!("Time"), json!("many")));
    assert_eq!(result("Expr.first"), (json!("Datetime(us)"), json!("one")));
    assert_eq!(result("Expr.len"), (json!("UInt32"), json!("one")));
    // A running sum of timestamps and a length in days of what is no duration are refused by
    // Polars, which says so when it runs rows.
    for id in ["Expr.cum_sum", "Expr.sum", "Expr.dt.total_days"] {
        let verdict = &offered(&answer, id)["verdict"];
        assert_eq!(verdict["kind"], "refused", "{id}");
        assert_eq!(
            verdict["refusal"]["source"],
            json!({ "by": "polars", "stage": "rows" }),
            "{id}"
        );
        assert!(
            verdict["refusal"]["reason"]
                .as_str()
                .expect("a reason")
                .contains("datetime"),
            "{id}: {verdict}"
        );
    }
    // A start is refused after a value by the shape of an expression, and steps are not offered.
    assert_eq!(
        offered(&answer, "polars.col")["verdict"]["refusal"]["source"],
        json!({ "by": "structure" })
    );
    assert!(
        answer["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .all(|entry| !entry["entry"]
                .as_str()
                .expect("an id")
                .starts_with("LazyFrame."))
    );
    assert_eq!(answer["entries"].as_array().expect("entries").len(), 35);
}

#[test]
fn a_method_that_takes_a_whole_number_by_default_is_offered_with_it() {
    let answer = invoke(
        "offers_at",
        json!({
            "positions": [{ "kind": "outer", "name": "select" }],
            "after": { "kind": "value", "dtype": "Datetime(us)", "shape": "many" }
        }),
    )
    .expect("offers");
    // `shift(n=1)`: one row.
    assert_eq!(
        offered(&answer, "Expr.shift")["argument"],
        json!({ "parameter": "n", "literal": { "kind": "integer", "value": 1 } })
    );
    // `date` takes none, and a refused method is given none.
    assert_eq!(offered(&answer, "Expr.dt.date")["argument"], Value::Null);
    assert_eq!(offered(&answer, "Expr.sum")["argument"], Value::Null);
}

#[test]
fn what_may_start_an_expression_depends_on_where_it_stands() {
    let at = |outer_name: &str| {
        invoke(
            "offers_at",
            json!({
                "positions": [{ "kind": "outer", "name": outer_name }],
                "after": { "kind": "start" }
            }),
        )
        .expect("offers")
    };
    // `element` stands only in a pivot's aggregate, by a rule; there Polars refuses a column.
    let select = at("select");
    assert_eq!(
        offered(&select, "polars.element")["verdict"]["refusal"]["source"],
        json!({ "by": "rule", "rule": "element-only-in-pivot-aggregate" })
    );
    assert_eq!(
        allowed(&select),
        ["polars.col", "polars.lit", "polars.when"]
    );
    let pivot = at("pivot_aggregate");
    assert_eq!(
        allowed(&pivot),
        ["polars.element", "polars.lit", "polars.when"]
    );
    assert_eq!(
        offered(&pivot, "polars.col")["verdict"]["refusal"]["source"],
        json!({ "by": "polars", "stage": "schema" })
    );
    // A pivot's aggregate gives one value for each group, which a rule says of what a condition
    // gives there: it may stand, and may not end.
    assert_eq!(
        offered(&pivot, "polars.when")["verdict"]["ending"]["refusal"]["source"],
        json!({ "by": "rule", "rule": "pivot-aggregate-gives-one-value" })
    );
}

#[test]
fn a_condition_that_is_not_finished_is_continued_by_the_links_that_follow_it() {
    let answer = invoke(
        "offers_at",
        json!({
            "positions": [{ "kind": "outer", "name": "select" }],
            "after": { "kind": "condition", "state": "then" }
        }),
    )
    .expect("offers");
    // After `then`: `otherwise` ends it, a further `when` goes on.
    assert_eq!(allowed(&answer), ["Then.otherwise", "Then.when"]);
}

#[test]
fn an_expression_is_found_by_a_position_of_a_step_first() {
    let refused = invoke(
        "offers_at",
        json!({ "positions": [], "after": { "kind": "start" } }),
    )
    .expect_err("no position");
    assert_eq!(refused["kind"], "unknownName");
    let nested_first = invoke(
        "offers_at",
        json!({
            "positions": [{ "kind": "nested", "name": "over.partition_by" }],
            "after": { "kind": "start" }
        }),
    )
    .expect_err("a nested position cannot be first");
    assert_eq!(nested_first["kind"], "unknownName");
    // A place inside a window's key is judged by the position of the step and the innermost one.
    let inside = offers_view(
        &[
            PositionRequest::Outer {
                name: Outer::Select,
            },
            PositionRequest::Nested {
                name: Nested::PartitionBy,
            },
            PositionRequest::Nested {
                name: Nested::ConditionValue,
            },
        ],
        AfterRequest::Start,
    )
    .expect("offers");
    assert_eq!(
        json_of(&inside)["place"]["context"],
        json!({ "outer": "select", "nested": "condition.value" })
    );
}

#[test]
fn a_position_or_dtype_that_the_engine_does_not_name_is_refused_as_an_argument() {
    for arguments in [
        json!({ "positions": [{ "kind": "outer", "name": "selects" }], "after": { "kind": "start" } }),
        json!({
            "positions": [{ "kind": "outer", "name": "select" }],
            "after": { "kind": "value", "dtype": "Categorical", "shape": "many" }
        }),
        json!({ "positions": [{ "kind": "outer", "name": "select" }], "after": { "kind": "end" } }),
    ] {
        assert!(
            invoke("offers_at", arguments.clone()).is_err(),
            "{arguments}"
        );
    }
}

/// The fit of each candidate of `answer`, by the dtype it was judged as and
/// whether it may stand, and whether it may end where it stands.
fn judged(answer: &Value) -> Vec<(String, String, String)> {
    answer["fits"]
        .as_array()
        .expect("fits")
        .iter()
        .map(|fit| {
            assert_eq!(fit["kind"], "judged", "{fit}");
            let verdict = &fit["verdict"];
            (
                fit["dtype"].as_str().expect("a dtype").to_owned(),
                verdict["kind"].as_str().expect("a kind").to_owned(),
                verdict["ending"]["kind"].as_str().unwrap_or("-").to_owned(),
            )
        })
        .collect()
}

#[test]
fn a_field_and_an_input_start_a_predicate_by_their_dtype() {
    let answer = invoke(
        "candidate_fits",
        json!({
            "start": [{ "kind": "outer", "name": "filter" }],
            "argument": null,
            "candidates": [
                { "kind": "field", "dtype": "Boolean" },
                { "kind": "field", "dtype": "Int64" },
                { "kind": "input", "dtype": "Boolean", "list": false },
                { "kind": "input", "dtype": "Date", "list": true },
            ]
        }),
    )
    .expect("fits");
    let triple = |dtype: &str, kind: &str, ending: &str| {
        (dtype.to_owned(), kind.to_owned(), ending.to_owned())
    };
    // Each may stand, to be continued; one may end a predicate only if it is a Boolean.
    assert_eq!(
        judged(&answer),
        [
            triple("Boolean", "allowed", "allowed"),
            triple("Int64", "allowed", "refused"),
            triple("Boolean", "allowed", "allowed"),
            triple("Date", "allowed", "refused"),
        ]
    );
    assert_eq!(
        answer["fits"][1]["verdict"]["ending"]["refusal"]["source"],
        json!({ "by": "rule", "rule": "filter-predicate-is-boolean" })
    );
    // A connected value gives one value, a list many.
    assert_eq!(answer["fits"][2]["verdict"]["shape"], "one");
    assert_eq!(answer["fits"][3]["verdict"]["shape"], "many");
}

#[test]
fn an_argument_is_judged_by_what_the_method_takes() {
    let answer = invoke(
        "candidate_fits",
        json!({
            "start": null,
            "argument": {
                "entry": "Expr.dt.offset_by",
                "parameter": "by",
                "place": {
                    "positions": [{ "kind": "outer", "name": "select" }],
                    "after": { "kind": "value", "dtype": "Datetime(us)", "shape": "many" }
                }
            },
            "candidates": [
                { "kind": "field", "dtype": "String" },
                { "kind": "field", "dtype": "Int64" },
                { "kind": "field", "dtype": "Categorical" },
            ]
        }),
    )
    .expect("fits");
    // `offset_by` takes a duration written as text: a field of numbers is no duration, and Polars
    // says so when it runs rows.
    let fits = answer["fits"].as_array().expect("fits");
    let kinds: Vec<&str> = fits
        .iter()
        .map(|fit| fit["verdict"]["kind"].as_str().unwrap_or("-"))
        .collect();
    assert_eq!(kinds, ["allowed", "refused", "-"]);
    assert_eq!(
        fits[1]["verdict"]["refusal"]["source"],
        json!({ "by": "polars", "stage": "rows" })
    );
    // A dtype the tables cover no class of is not judged, and says so.
    assert_eq!(
        fits[2],
        json!({ "kind": "unjudged", "reason": "the tables cover no class of the dtype Categorical" })
    );
}

#[test]
fn a_typed_literal_is_no_candidate_of_a_slot_for_the_engine_reads_what_is_typed() {
    // A number or text typed in is asked of `read_typed`, which reads it and judges what it
    // reads, so `candidate_fits` takes only what a list holds: fields and connected inputs.
    for literal in [
        json!({ "kind": "integer", "value": 1 }),
        json!({ "kind": "float", "value": 1.5 }),
        json!({ "kind": "text", "value": "1d" }),
    ] {
        let refused = invoke(
            "candidate_fits",
            json!({
                "start": [{ "kind": "outer", "name": "select" }],
                "argument": null,
                "candidates": [{ "kind": "literal", "literal": literal }]
            }),
        );
        assert!(refused.is_err(), "{literal}");
    }
}

#[test]
fn a_slot_that_has_no_place_and_a_name_the_catalogue_lacks_are_refused() {
    let none = invoke(
        "candidate_fits",
        json!({ "start": null, "argument": null, "candidates": [] }),
    )
    .expect_err("no slot");
    assert_eq!(none["kind"], "unknownName");
    let place = json!({
        "positions": [{ "kind": "outer", "name": "select" }],
        "after": { "kind": "value", "dtype": "Int64", "shape": "many" }
    });
    for (entry, parameter) in [("Expr.nothing", "n"), ("Expr.shift", "m")] {
        let refused = invoke(
            "candidate_fits",
            json!({
                "start": null,
                "argument": { "entry": entry, "parameter": parameter, "place": place },
                "candidates": []
            }),
        )
        .expect_err("an unknown name");
        assert_eq!(refused["kind"], "unknownName", "{entry} {parameter}");
    }
}

/// The kinds of the readings of `answer`, in order.
fn reading_kinds(answer: &Value) -> Vec<&str> {
    answer["readings"]
        .as_array()
        .expect("readings")
        .iter()
        .map(|reading| reading["kind"].as_str().expect("a kind"))
        .collect()
}

#[test]
fn a_number_typed_into_an_expression_reads_as_a_number_and_as_text_with_the_dtypes_polars_gives() {
    let read = |text: &str| {
        invoke(
            "read_typed",
            json!({ "text": text, "typed": { "kind": "expression" }, "start": null, "argument": null }),
        )
        .expect("readings")
    };
    // A whole number is the narrowest integer it fits, a number with a fraction a float, and
    // the text the typed text itself; nothing is judged where no slot is given.
    assert_eq!(
        read("12"),
        json!({ "readings": [
            { "kind": "read", "literal": { "kind": "integer", "value": 12 }, "dtype": "Int32", "fit": null },
            { "kind": "read", "literal": { "kind": "text", "value": "12" }, "dtype": "String", "fit": null },
        ] })
    );
    let fraction = read("1.0");
    assert_eq!(
        fraction["readings"][0]["literal"],
        json!({ "kind": "float", "value": 1.0 })
    );
    assert_eq!(fraction["readings"][0]["dtype"], "Float64");
    // What is no number is text alone, and nothing typed is nothing.
    assert_eq!(
        read("abc"),
        json!({ "readings": [
            { "kind": "read", "literal": { "kind": "text", "value": "abc" }, "dtype": "String", "fit": null },
        ] })
    );
    assert_eq!(read(""), json!({ "readings": [] }));
}

#[test]
fn a_typed_text_is_judged_by_what_the_method_takes_where_a_slot_is_given() {
    let answer = invoke(
        "read_typed",
        json!({
            "text": "1.5",
            "typed": { "kind": "expression" },
            "start": null,
            "argument": {
                "entry": "Expr.dt.offset_by",
                "parameter": "by",
                "place": {
                    "positions": [{ "kind": "outer", "name": "select" }],
                    "after": { "kind": "value", "dtype": "Datetime(us)", "shape": "many" }
                }
            }
        }),
    )
    .expect("readings");
    // `offset_by` takes a duration written as text: the number is refused by Polars when it runs
    // rows, and the text is allowed.
    assert_eq!(reading_kinds(&answer), ["read", "read"]);
    let number = &answer["readings"][0]["fit"];
    assert_eq!(number["kind"], "judged");
    assert_eq!(number["dtype"], "Float64");
    assert_eq!(number["verdict"]["kind"], "refused");
    assert_eq!(
        number["verdict"]["refusal"]["source"],
        json!({ "by": "polars", "stage": "rows" })
    );
    let text = &answer["readings"][1]["fit"];
    assert_eq!(text["dtype"], "String");
    assert_eq!(text["verdict"]["kind"], "allowed");
}

#[test]
fn a_typed_text_reads_as_the_slot_it_is_typed_into_takes_it() {
    let read = |typed: Value, text: &str| {
        invoke(
            "read_typed",
            json!({ "text": text, "typed": typed, "start": null, "argument": null }),
        )
        .expect("readings")
    };
    let value = |value_type: &str| json!({ "kind": "value", "type": value_type });
    // A whole number slot reads a whole number, and says why a number with a fraction is not.
    assert_eq!(
        read(value("integer"), "7")["readings"][0]["literal"],
        json!({ "kind": "integer", "value": 7 })
    );
    assert_eq!(
        read(value("integer"), "7.5")["readings"],
        json!([{ "kind": "unread", "reason": "not a whole number" }])
    );
    // A number slot reads a whole number as a number with a fraction.
    assert_eq!(
        read(value("float"), "7")["readings"][0],
        json!({ "kind": "read", "literal": { "kind": "float", "value": 7.0 }, "dtype": "Float64", "fit": null })
    );
    // Text is text, though it is a number; a path has no dtype.
    assert_eq!(
        read(value("string"), "7")["readings"],
        json!([{ "kind": "read", "literal": { "kind": "text", "value": "7" }, "dtype": "String", "fit": null }])
    );
    assert_eq!(
        read(json!({ "kind": "path" }), "data/claims.parquet")["readings"],
        json!([{ "kind": "read", "literal": { "kind": "text", "value": "data/claims.parquet" }, "dtype": null, "fit": null }])
    );
    // A date is read as Polars reads one and carried as the day, a duration as Polars reads a
    // duration string and carried as typed.
    assert_eq!(
        read(value("date"), "10-03-2026")["readings"],
        json!([{ "kind": "read", "literal": { "kind": "text", "value": "2026-03-10" }, "dtype": "Date", "fit": null }])
    );
    assert_eq!(
        read(json!({ "kind": "duration" }), "2d12h")["readings"],
        json!([{ "kind": "read", "literal": { "kind": "text", "value": "2d12h" }, "dtype": null, "fit": null }])
    );
    // What Polars does not read is unread, with Polars' own words.
    for (typed, text) in [
        (value("date"), "yesterday"),
        (json!({ "kind": "duration" }), "1x"),
    ] {
        let answer = read(typed, text);
        assert_eq!(reading_kinds(&answer), ["unread"], "{text}");
        assert!(
            !answer["readings"][0]["reason"]
                .as_str()
                .expect("a reason")
                .is_empty(),
            "{text}"
        );
    }
}

#[test]
fn a_typed_text_that_asks_about_a_name_the_catalogue_lacks_is_refused_with_its_kind() {
    let place = json!({
        "positions": [{ "kind": "outer", "name": "select" }],
        "after": { "kind": "value", "dtype": "Int64", "shape": "many" }
    });
    for (entry, parameter) in [("Expr.nothing", "n"), ("Expr.shift", "m")] {
        let refused = invoke(
            "read_typed",
            json!({
                "text": "1",
                "typed": { "kind": "expression" },
                "start": null,
                "argument": { "entry": entry, "parameter": parameter, "place": place }
            }),
        )
        .expect_err("an unknown name");
        assert_eq!(refused["kind"], "unknownName", "{entry} {parameter}");
    }
    let refused = invoke(
        "read_typed",
        json!({
            "text": "1",
            "typed": { "kind": "expression" },
            "start": [],
            "argument": null
        }),
    )
    .expect_err("no position");
    assert_eq!(refused["kind"], "unknownName");
    // A kind that takes no typed text, and a type that is no type, are not arguments at all.
    for typed in [
        json!({ "kind": "flag" }),
        json!({ "kind": "value", "type": "boolean" }),
        json!({ "kind": "value" }),
    ] {
        assert!(
            invoke(
                "read_typed",
                json!({ "text": "1", "typed": typed, "start": null, "argument": null }),
            )
            .is_err(),
            "{typed}"
        );
    }
}

#[test]
fn the_options_of_a_cross_join_refuse_every_validation_with_polars_reason() {
    let answer = invoke(
        "option_offers",
        json!({
            "entry": "LazyFrame.join",
            "chosen": [{ "parameter": "how", "word": "cross" }]
        }),
    )
    .expect("options");
    let options = answer["options"].as_array().expect("options");
    let option = |name: &str| {
        options
            .iter()
            .find(|option| option["parameter"] == name)
            .unwrap_or_else(|| panic!("no option {name}"))
    };
    // `validate` is a choice of four words, whose default is `m:m`: many matches on both sides.
    let validate = option("validate");
    assert_eq!(validate["default"], "m:m");
    let words: Vec<&str> = validate["words"]
        .as_array()
        .expect("words")
        .iter()
        .map(|word| word["word"].as_str().expect("a word"))
        .collect();
    assert_eq!(words, ["m:m", "m:1", "1:m", "1:1"]);
    for word in validate["words"].as_array().expect("words") {
        assert_eq!(
            word["refusal"]["source"],
            json!({ "by": "polars", "stage": "schema" }),
            "{word}"
        );
    }
    // The option chosen is judged against the others: `how` itself offers its words.
    assert!(option("how")["words"][0]["refusal"].is_null());
    assert_eq!(option("how")["default"], "inner");
    // A flag is true or false, and `unset` is the default of one that Polars decides.
    assert_eq!(option("nulls_equal")["default"], "false");
    assert_eq!(option("coalesce")["default"], "unset");
    let flag_words: Vec<&str> = option("coalesce")["words"]
        .as_array()
        .expect("words")
        .iter()
        .map(|word| word["word"].as_str().expect("a word"))
        .collect();
    assert_eq!(flag_words, ["true", "false"]);
}

#[test]
fn an_option_says_which_word_it_starts_with_when_it_is_added() {
    let answer = invoke(
        "option_offers",
        json!({
            "entry": "LazyFrame.join",
            "chosen": [{ "parameter": "how", "word": "cross" }]
        }),
    )
    .expect("options");
    let starter = |name: &str| {
        answer["options"]
            .as_array()
            .expect("options")
            .iter()
            .find(|option| option["parameter"] == name)
            .unwrap_or_else(|| panic!("no option {name}"))["starter"]
            .clone()
    };
    // Polars refuses every validation but its default beside a cross join, so there is none.
    assert!(starter("validate").is_null());
    // `how` is judged against the other options and not its own word: its default is `inner`,
    // and `left` is the next word Polars lists.
    assert_eq!(starter("how"), "left");
}

/// The names `texts` as the strings a request holds.
fn names(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

#[test]
fn every_step_of_the_list_starts_with_what_the_engine_gives_it() {
    let answer = invoke(
        "step_starts",
        json!({ "columns": ["dcn", "owner"], "tables": ["step_names", "step_sla"] }),
    )
    .expect("the starts");
    let unset = || json!({ "kind": "unset" });
    let unset_list = || json!({ "kind": "list", "items": [{ "kind": "unset" }] });
    let argument =
        |parameter: &str, setting: Value| json!({ "parameter": parameter, "setting": setting });
    // Worked out by hand: a join takes the first table and keeps the rows that have no match,
    // a select every column in order, and a pivot takes the first table and the first value.
    assert_eq!(
        answer,
        json!({ "starts": [
            { "entry": "LazyFrame.filter", "arguments": [argument("predicates", unset_list())] },
            { "entry": "LazyFrame.join", "arguments": [
                argument("other", json!({ "kind": "word", "value": "step_names" })),
                argument("left_on", unset_list()),
                argument("right_on", unset_list()),
                argument("how", json!({ "kind": "word", "value": "left" })),
            ] },
            { "entry": "LazyFrame.pivot", "arguments": [
                argument("on", unset()),
                argument("on_columns", json!({ "kind": "word", "value": "step_names" })),
                argument("index", unset()),
                argument("values", unset()),
                argument("aggregate_function", json!({ "kind": "element", "method": "first" })),
            ] },
            { "entry": "LazyFrame.rename", "arguments": [
                argument("mapping", json!({ "kind": "pairs", "entries": [] })),
            ] },
            { "entry": "LazyFrame.select", "arguments": [
                argument("exprs", json!({ "kind": "list", "items": [
                    { "kind": "field", "column": "dcn" },
                    { "kind": "field", "column": "owner" },
                ] })),
            ] },
            { "entry": "LazyFrame.sort", "arguments": [argument("by", unset_list())] },
            { "entry": "LazyFrame.with_columns", "arguments": [
                argument("named_exprs", json!({ "kind": "pairs", "entries": [
                    { "key": { "kind": "text", "value": "new_column" }, "value": unset() },
                ] })),
            ] },
        ] })
    );
}

#[test]
fn a_join_and_a_pivot_with_no_connected_table_start_without_one() {
    let view = json_of(&step_starts_view(&names(&["a"]), &[]));
    let starts = view["starts"].as_array().expect("starts");
    let argument = |entry: &str, parameter: &str| {
        starts
            .iter()
            .find(|start| start["entry"] == entry)
            .and_then(|start| {
                start["arguments"]
                    .as_array()?
                    .iter()
                    .find(|argument| argument["parameter"] == parameter)
            })
            .map(|argument| argument["setting"].clone())
    };
    assert_eq!(
        argument("LazyFrame.join", "other"),
        Some(json!({ "kind": "unset" }))
    );
    assert_eq!(
        argument("LazyFrame.pivot", "on_columns"),
        Some(json!({ "kind": "unset" }))
    );
}

#[test]
fn an_entry_that_has_no_options_has_none_and_an_unknown_one_is_refused() {
    let none = invoke(
        "option_offers",
        json!({ "entry": "Expr.is_null", "chosen": [] }),
    )
    .expect("options");
    assert_eq!(none, json!({ "options": [] }));
    for arguments in [
        json!({ "entry": "Expr.nothing", "chosen": [] }),
        json!({ "entry": "Expr.over", "chosen": [{ "parameter": "nothing", "word": "true" }] }),
    ] {
        let refused = invoke("option_offers", arguments).expect_err("an unknown name");
        assert_eq!(refused["kind"], "unknownName");
    }
}

/// The contexts a place can stand in, each as the positions that hold it.
fn contexts() -> Vec<(Context, Vec<PositionRequest>)> {
    Context::all()
        .into_iter()
        .map(|context| {
            let mut positions = vec![PositionRequest::Outer {
                name: context.outer(),
            }];
            positions.extend(
                context
                    .nested()
                    .map(|name| PositionRequest::Nested { name }),
            );
            (context, positions)
        })
        .collect()
}

/// What an expression may have where a call would follow it, of every kind:
/// nothing, a value of each class of dtype and each shape, and a condition at
/// each state it comes to.
fn afters() -> Vec<After> {
    let mut afters = vec![After::Start];
    for dtype in Representative::ALL {
        for shape in Shape::ALL {
            afters.push(After::Value { dtype, shape });
        }
    }
    afters.extend(ConditionState::ALL.map(After::Condition));
    afters
}

fn after_request(after: After) -> AfterRequest {
    match after {
        After::Start => AfterRequest::Start,
        After::Value { dtype, shape } => AfterRequest::Value {
            dtype,
            shape: match shape {
                Shape::One => ValueShapeView::One,
                Shape::Many => ValueShapeView::Many,
            },
        },
        After::Condition(state) => AfterRequest::Condition {
            state: match state {
                ConditionState::When => ConditionStateView::When,
                ConditionState::Then => ConditionStateView::Then,
                ConditionState::ChainedWhen => ConditionStateView::ChainedWhen,
                ConditionState::ChainedThen => ConditionStateView::ChainedThen,
            },
        },
    }
}

/// Every call the engine allows at every place of every kind, as the offers
/// command answers them: for each context and each of what an expression may
/// have, a number that holds one bit for each entry that stands in an
/// expression, in the order of `entries`, set when that entry may stand there.
fn grid() -> Value {
    let mut entries: Vec<String> = Vec::new();
    let mut masks: Vec<Vec<u64>> = Vec::new();
    let afters = afters();
    for (_, positions) in contexts() {
        let mut row = Vec::new();
        for after in &afters {
            let answer = json_of(&offers_view(&positions, after_request(*after)).expect("offers"));
            let ids: Vec<&str> = answer["entries"]
                .as_array()
                .expect("entries")
                .iter()
                .map(|entry| entry["entry"].as_str().expect("an id"))
                .collect();
            if entries.is_empty() {
                entries = ids.iter().map(|id| (*id).to_owned()).collect();
            }
            assert_eq!(entries, ids, "the entries keep their order");
            let mask = answer["entries"]
                .as_array()
                .expect("entries")
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry["verdict"]["kind"] == "allowed")
                .fold(0_u64, |mask, (at, _)| mask | (1 << at));
            row.push(mask);
        }
        masks.push(row);
    }
    json!({
        "entries": entries,
        "contexts": contexts().iter().map(|(context, _)| json_of(&crate::report::context_view(*context))).collect::<Vec<_>>(),
        "afters": afters.iter().map(|after| json_of(&crate::report::after_view(*after))).collect::<Vec<_>>(),
        "allowed": masks,
    })
}

#[test]
fn the_grid_holds_a_cell_for_every_place_and_sets_a_bit_for_every_entry_that_may_stand() {
    let grid = grid();
    let contexts = grid["contexts"].as_array().expect("contexts").len();
    let afters = grid["afters"].as_array().expect("afters").len();
    // Six positions of a step, each alone and with each of four positions inside an expression;
    // nothing, twenty-six dtypes in two shapes, and four states of a condition.
    assert_eq!((contexts, afters), (30, 57));
    let rows = grid["allowed"].as_array().expect("rows");
    assert_eq!(rows.len(), contexts);
    assert!(
        rows.iter()
            .all(|row| row.as_array().expect("a row").len() == afters)
    );
    // A select start is `col`, `lit` and `when`, which are the entries 31, 33 and 34 of the 35.
    let entries = grid["entries"].as_array().expect("entries");
    let at = |id: &str| {
        entries
            .iter()
            .position(|entry| entry == id)
            .expect("an entry")
    };
    let select_start = rows[0][0].as_u64().expect("a mask");
    let expected = [at("polars.col"), at("polars.lit"), at("polars.when")]
        .iter()
        .fold(0_u64, |mask, bit| mask | (1 << bit));
    assert_eq!(select_start, expected);
    let mut report = String::new();
    write!(report, "{entries:?}").expect("a string takes text");
    assert!(report.contains("polars.element"));
}

#[test]
fn the_snapshots_are_what_the_commands_return() {
    let select = outer(Outer::Select);
    let snapshots = [
        (
            "offers-start.json",
            json_of(&offers_view(&select, AfterRequest::Start).expect("offers")),
        ),
        (
            "offers-value.json",
            json_of(
                &offers_view(
                    &select,
                    value(Representative::Datetime, ValueShapeView::Many),
                )
                .expect("offers"),
            ),
        ),
        (
            "offers-condition.json",
            json_of(
                &offers_view(
                    &select,
                    AfterRequest::Condition {
                        state: ConditionStateView::Then,
                    },
                )
                .expect("offers"),
            ),
        ),
        (
            "fits-start.json",
            json_of(
                &fits_view(
                    Some(&outer(Outer::Filter)),
                    None,
                    &[
                        CandidateRequest::Field {
                            dtype: "Boolean".to_owned(),
                        },
                        CandidateRequest::Field {
                            dtype: "Int64".to_owned(),
                        },
                        CandidateRequest::Input {
                            dtype: "Boolean".to_owned(),
                            list: false,
                        },
                        CandidateRequest::Input {
                            dtype: "Date".to_owned(),
                            list: true,
                        },
                        CandidateRequest::Field {
                            dtype: "Categorical".to_owned(),
                        },
                    ],
                )
                .expect("fits"),
            ),
        ),
        (
            "fits-argument.json",
            json_of(
                &fits_view(
                    None,
                    Some(&ArgumentRequest {
                        entry: "Expr.dt.offset_by".to_owned(),
                        parameter: "by".to_owned(),
                        place: PlaceRequest {
                            positions: select.clone(),
                            after: value(Representative::Datetime, ValueShapeView::Many),
                        },
                    }),
                    &[
                        CandidateRequest::Field {
                            dtype: "String".to_owned(),
                        },
                        CandidateRequest::Field {
                            dtype: "Int64".to_owned(),
                        },
                        CandidateRequest::Input {
                            dtype: "String".to_owned(),
                            list: false,
                        },
                    ],
                )
                .expect("fits"),
            ),
        ),
        (
            "typed-expression.json",
            json_of(
                &read_view(
                    "1.5",
                    TypedRequest::Expression,
                    None,
                    Some(&ArgumentRequest {
                        entry: "Expr.dt.offset_by".to_owned(),
                        parameter: "by".to_owned(),
                        place: PlaceRequest {
                            positions: select.clone(),
                            after: value(Representative::Datetime, ValueShapeView::Many),
                        },
                    }),
                )
                .expect("readings"),
            ),
        ),
        (
            "typed-start-and-argument.json",
            json_of(
                &read_view(
                    "1.5",
                    TypedRequest::Expression,
                    Some(&outer(Outer::Filter)),
                    Some(&ArgumentRequest {
                        entry: "Expr.shift".to_owned(),
                        parameter: "n".to_owned(),
                        place: PlaceRequest {
                            positions: select.clone(),
                            after: value(Representative::Int64, ValueShapeView::Many),
                        },
                    }),
                )
                .expect("readings"),
            ),
        ),
        (
            "typed-whole.json",
            json_of(
                &read_view(
                    "12",
                    TypedRequest::Value {
                        value_type: ValueTypeRequest::Integer,
                    },
                    None,
                    None,
                )
                .expect("readings"),
            ),
        ),
        (
            "typed-date.json",
            json_of(
                &read_view(
                    "10-03-2026",
                    TypedRequest::Value {
                        value_type: ValueTypeRequest::Date,
                    },
                    None,
                    None,
                )
                .expect("readings"),
            ),
        ),
        (
            "typed-unread.json",
            json_of(&read_view("1x", TypedRequest::Duration, None, None).expect("readings")),
        ),
        (
            "options.json",
            json_of(
                &options_view(
                    "LazyFrame.join",
                    &[ChosenOptionRequest {
                        parameter: "how".to_owned(),
                        word: "cross".to_owned(),
                    }],
                )
                .expect("options"),
            ),
        ),
        (
            "step-starts.json",
            json_of(&step_starts_view(
                &names(&["dcn", "owner"]),
                &names(&["step_names", "step_sla"]),
            )),
        ),
        ("offers-grid.json", grid()),
    ];
    for (file, returned) in &snapshots {
        crate::snapshot::check(file, returned, REFRESH, REFRESH_COMMAND);
    }
}
