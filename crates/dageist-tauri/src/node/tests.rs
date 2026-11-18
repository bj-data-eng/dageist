//! The node commands through Tauri's IPC layer, and the snapshots of what they
//! return. Every expected value is written out by hand from the claim-timing
//! sample's seven rows and its steps.

use dageist_engine::claim_timing::ClaimTiming;
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};

use super::{node_view, rows_view, run_view};

/// Set to rewrite the snapshots instead of checking them.
const REFRESH: &str = "DAGEIST_REFRESH_STEP_REPORTS";

/// The command that refreshes the snapshots.
const REFRESH_COMMAND: &str = "cargo test --offline --locked -p dageist-tauri node::tests::the_snapshots_are_what_the_commands_return";

fn window() -> WebviewWindow<MockRuntime> {
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            super::node_inputs,
            super::run_document,
            super::rows_after_step,
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

/// A view as the JSON it serializes to.
fn json(view: &impl serde::Serialize) -> Value {
    serde_json::to_value(view).expect("a view serializes")
}

/// The words of every closed vocabulary the views use.
fn vocabulary() -> Value {
    let mut words = crate::report::fixtures::vocabulary();
    let (Some(words), Value::Object(catalogue)) = (
        words.as_object_mut(),
        crate::catalogue::fixtures::vocabulary(),
    ) else {
        panic!("a vocabulary is an object");
    };
    words.extend(catalogue);
    Value::Object(words.clone())
}

fn steps_text() -> &'static str {
    ClaimTiming::STEPS_JSON
}

/// The claims sorted by key, then a column whose sum Polars refuses, then a
/// step that is never run.
fn refused_text() -> String {
    let column = |name: &str| {
        json!({ "expression": { "start": {
            "entry": "polars.col",
            "arguments": { "name": { "columns": [name] } }
        }, "then": [] } })
    };
    json!({ "version": 1, "steps": [
        { "id": "sort", "call": { "entry": "LazyFrame.sort", "arguments": {
            "by": { "many": [column("dcn")] }
        } } },
        { "id": "total", "call": { "entry": "LazyFrame.with_columns", "arguments": {
            "named_exprs": { "pairs": [[
                { "columns": ["total"] },
                { "expression": { "start": {
                    "entry": "polars.col",
                    "arguments": { "name": { "columns": ["dcn"] } }
                }, "then": [{ "entry": "Expr.sum", "arguments": {} }] } }
            ]] }
        } } },
        { "id": "after", "call": { "entry": "LazyFrame.with_columns", "arguments": {
            "named_exprs": { "pairs": [[
                { "columns": ["days"] }, column("row_days")
            ]] }
        } } }
    ] })
    .to_string()
}

/// A sort, a column whose shift is by something still to be picked, and a step that is never
/// run.
fn incomplete_text() -> String {
    let column = |name: &str| {
        json!({ "expression": { "start": {
            "entry": "polars.col",
            "arguments": { "name": { "columns": [name] } }
        }, "then": [] } })
    };
    json!({ "version": 1, "steps": [
        { "id": "sort", "call": { "entry": "LazyFrame.sort", "arguments": {
            "by": { "many": [column("dcn")] }
        } } },
        { "id": "later", "call": { "entry": "LazyFrame.with_columns", "arguments": {
            "named_exprs": { "pairs": [[
                { "columns": ["earlier"] },
                { "expression": { "start": {
                    "entry": "polars.col",
                    "arguments": { "name": { "columns": ["timestamp"] } }
                }, "then": [{ "entry": "Expr.shift", "arguments": {
                    "n": { "expression": { "start": { "hole": null }, "then": [] } }
                } }] } }
            ]] }
        } } },
        { "id": "after", "call": { "entry": "LazyFrame.with_columns", "arguments": {
            "named_exprs": { "pairs": [[
                { "columns": ["days"] }, column("row_days")
            ]] }
        } } }
    ] })
    .to_string()
}

fn the_field(name: &str, dtype: &str) -> Value {
    json!({ "name": name, "dtype": dtype })
}

fn claim_columns() -> Value {
    json!([
        the_field("dcn", "String"),
        the_field("timestamp", "Datetime(us)"),
        the_field("step_to", "String"),
        the_field("owner", "String"),
        the_field("row_days", "Int64"),
    ])
}

fn parameter() -> Value {
    json!({ "kind": "parameter" })
}

fn from_node(name: &str) -> Value {
    json!({ "kind": "node", "name": name })
}

#[test]
fn the_inputs_command_reports_the_source_and_every_input_by_name_with_what_supplies_it() {
    let answer = invoke("node_inputs", json!({})).expect("the inputs");
    assert_eq!(
        answer,
        json!({
            "source": {
                "name": "claims",
                "from": from_node("Claims"),
                "columns": claim_columns()
            },
            "inputs": [
                { "kind": "value", "name": "as_of", "from": parameter(),
                  "dtype": "Date", "value": "2026-03-02" },
                { "kind": "list", "name": "holidays",
                  "from": { "kind": "column", "node": "Holiday calendar", "column": "date" },
                  "dtype": "Date",
                  "values": { "shown": ["2026-03-03", "2026-03-05"], "more": 0 } },
                { "kind": "value", "name": "sla_cutoff", "from": parameter(),
                  "dtype": "Int64", "value": "1" },
                { "kind": "table", "name": "step_names", "from": from_node("Step names"),
                  "columns": [the_field("step_to", "String")] },
                { "kind": "table", "name": "step_sla", "from": from_node("SLA by step"),
                  "columns": [the_field("step", "String"), the_field("sla_days", "Int64")] },
            ]
        })
    );
}

/// What a step added or replaced, as the report writes one change.
fn change(name: &str, how: &str) -> Value {
    json!({ "name": name, "change": how })
}

/// The first values of a column, as the report writes them.
fn values(name: &str, shown: Value, more: usize) -> Value {
    json!({ "name": name, "values": { "shown": shown, "more": more } })
}

/// A step that was given `received` rows, kept `kept` and has `first` values.
fn counted(received: usize, kept: usize, first: Value) -> Value {
    json!({ "kind": "counted", "received": received, "kept": kept, "first": first })
}

/// The claims kept where their days are over one, then a step that writes `owner` as it was
/// and adds a column of the same test.
fn filter_text() -> String {
    let column = |name: &str| {
        json!({ "expression": { "start": {
            "entry": "polars.col",
            "arguments": { "name": { "columns": [name] } }
        }, "then": [] } })
    };
    let slow = json!({ "expression": { "start": {
        "entry": "polars.col",
        "arguments": { "name": { "columns": ["row_days"] } }
    }, "then": [{ "entry": "Expr.gt", "arguments": { "other": { "expression": {
        "start": { "entry": "polars.lit", "arguments": { "value": { "integer": 1 } } },
        "then": []
    } } } }] } });
    json!({ "version": 1, "steps": [
        { "id": "slow_only", "call": { "entry": "LazyFrame.filter", "arguments": {
            "predicates": { "many": [slow.clone()] }
        } } },
        { "id": "write", "call": { "entry": "LazyFrame.with_columns", "arguments": {
            "named_exprs": { "pairs": [
                [{ "columns": ["owner"] }, column("owner")],
                [{ "columns": ["slow"] }, slow],
            ] }
        } } }
    ] })
    .to_string()
}

/// A date part of text, which passes the schema and fails on rows, then a step that waits for it.
fn failing_text() -> String {
    json!({ "version": 1, "steps": [
        { "id": "date", "call": { "entry": "LazyFrame.with_columns", "arguments": {
            "named_exprs": { "pairs": [[
                { "columns": ["day"] },
                { "expression": { "start": {
                    "entry": "polars.col",
                    "arguments": { "name": { "columns": ["dcn"] } }
                }, "then": [{ "entry": "Expr.dt.date", "arguments": {} }] } }
            ]] }
        } } },
        { "id": "after", "call": { "entry": "LazyFrame.with_columns", "arguments": {
            "named_exprs": { "pairs": [[
                { "columns": ["days"] },
                { "expression": { "start": {
                    "entry": "polars.col",
                    "arguments": { "name": { "columns": ["row_days"] } }
                }, "then": [] } }
            ]] }
        } } }
    ] })
    .to_string()
}

#[test]
fn the_run_command_reports_the_columns_a_step_added_or_replaced() {
    let answer = invoke("run_document", json!({ "document": steps_text() })).expect("a report");
    let steps = &answer["steps"];
    // `resolved_day` is the fifth step, and adds the one column.
    assert_eq!(
        steps[4]["outcome"]["changes"],
        json!([change("resolved_day", "added")])
    );
    // The sort and the join: nothing, then the days each step may take.
    assert_eq!(steps[0]["outcome"]["changes"], json!([]));
    assert_eq!(
        steps[7]["outcome"]["changes"],
        json!([change("sla_days", "added")])
    );
    // A pivot adds a column for each step name it was given, and keeps the index.
    assert_eq!(
        steps[11]["outcome"]["changes"],
        json!([
            change("Intake", "added"),
            change("Review", "added"),
            change("Pending", "added"),
            change("Complete", "added"),
        ])
    );
    // A column the step writes is replaced, even as the text it was.
    let written = invoke("run_document", json!({ "document": filter_text() })).expect("a report");
    assert_eq!(
        written["steps"][1]["outcome"]["changes"],
        json!([change("owner", "replaced"), change("slow", "added")])
    );
}

#[test]
fn the_run_command_reports_the_rows_each_step_kept_with_the_first_values_it_added() {
    let answer = invoke("run_document", json!({ "document": steps_text() })).expect("a report");
    let steps = &answer["steps"];
    assert_eq!(steps[0]["count"], counted(7, 7, json!([])));
    // After the sort A-1001 has four rows. The step before the first in a claim has none.
    assert_eq!(
        steps[1]["count"],
        counted(
            7,
            7,
            json!([values("step_index", json!(["1", "2", "3", "4"]), 3)])
        )
    );
    assert_eq!(
        steps[4]["count"],
        counted(
            7,
            7,
            json!([values(
                "resolved_day",
                json!([null, "2026-03-02", "2026-03-03", "2026-03-05"]),
                3
            )])
        )
    );
    assert_eq!(
        steps[7]["count"],
        counted(
            7,
            7,
            json!([values("sla_days", json!(["1", "1", "2", "3"]), 3)])
        )
    );
    // Seven rows of two claims make a pivot of two rows, A-1001 first.
    let pivot = &steps[11]["count"];
    assert_eq!((&pivot["received"], &pivot["kept"]), (&json!(7), &json!(2)));
    assert_eq!(pivot["first"][2], values("Pending", json!(["2", null]), 0));
}

#[test]
fn a_filter_keeps_the_rows_that_pass_of_the_rows_it_received() {
    let answer = invoke("run_document", json!({ "document": filter_text() })).expect("a report");
    // Four of the seven rows have more than one day; the step after is given those four.
    assert_eq!(answer["steps"][0]["count"], counted(7, 4, json!([])));
    assert_eq!(
        answer["steps"][1]["count"],
        counted(
            4,
            4,
            json!([
                values(
                    "owner",
                    json!(["Company A", "Company B", "Company A", "Company B"]),
                    0
                ),
                values("slow", json!(["true", "true", "true", "true"]), 0),
            ])
        )
    );
}

#[test]
fn a_step_that_is_refused_is_not_counted_and_nor_are_the_steps_after_it() {
    let answer = invoke("run_document", json!({ "document": refused_text() })).expect("a report");
    let steps = &answer["steps"];
    assert_eq!(steps[0]["count"], counted(7, 7, json!([])));
    assert_eq!(steps[1]["count"], json!({ "kind": "notCounted" }));
    assert_eq!(steps[2]["count"], json!({ "kind": "notCounted" }));
}

#[test]
fn a_step_polars_cannot_run_on_the_rows_fails_there_and_the_steps_after_it_are_not_counted() {
    let answer = invoke("run_document", json!({ "document": failing_text() })).expect("a report");
    let steps = &answer["steps"];
    // The step is valid by its schema.
    assert_eq!(steps[0]["outcome"]["status"], "valid");
    assert_eq!(steps[0]["count"]["kind"], "failed");
    assert!(
        steps[0]["count"]["reason"]
            .as_str()
            .expect("a reason")
            .contains("date"),
        "Polars says what it could not do: {}",
        steps[0]["count"]
    );
    assert_eq!(steps[1]["count"], json!({ "kind": "notCounted" }));
}

#[test]
fn the_run_command_reports_each_step_with_its_columns_and_laziness() {
    let answer = invoke("run_document", json!({ "document": steps_text() })).expect("a report");
    assert_eq!(answer["status"], "run");
    assert_eq!(answer["source"], claim_columns());
    let steps = answer["steps"].as_array().expect("steps");
    let ids: Vec<&str> = steps
        .iter()
        .map(|step| step["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(
        ids,
        [
            "sort",
            "step_index",
            "resolved_at",
            "received_at",
            "resolved_day",
            "days_open",
            "a_days",
            "join",
            "sla_status",
            "flag",
            "holiday_count",
            "pivot"
        ]
    );
    for step in steps {
        assert_eq!(step["outcome"]["status"], "valid", "{}", step["id"]);
        // Only the pivot needs a collect.
        assert_eq!(step["lazy"], step["id"] != "pivot", "{}", step["id"]);
    }
    assert_eq!(steps[0]["outcome"]["columns"], claim_columns());
    // A pivot has the index, then a column for each step name it was given.
    assert_eq!(
        steps[11]["outcome"]["columns"],
        json!([
            the_field("dcn", "String"),
            the_field("Intake", "Int64"),
            the_field("Review", "Int64"),
            the_field("Pending", "Int64"),
            the_field("Complete", "Int64"),
        ])
    );
}

#[test]
fn the_run_command_reports_the_dtype_after_each_link_and_where_the_expression_is() {
    let answer = invoke("run_document", json!({ "document": steps_text() })).expect("a report");
    // `resolved_day` is the fifth step: the timestamp column, then its date.
    let step = &answer["steps"][4];
    assert_eq!(
        step["expressions"],
        json!([{
            "place": {
                "stepIndex": 4,
                "stepId": "resolved_day",
                "path": [
                    { "kind": "parameter", "name": "named_exprs" },
                    { "kind": "value", "index": 0 }
                ]
            },
            "start": { "kind": "dtype", "dtype": "Datetime(us)" },
            "links": [{ "kind": "dtype", "dtype": "Date" }]
        }])
    );
}

#[test]
fn the_run_command_reports_what_the_tables_say_of_each_call() {
    let answer = invoke("run_document", json!({ "document": steps_text() })).expect("a report");
    let walked = &answer["steps"][4]["walk"]["expressions"][0];
    // A computed column is a select item. The date of a timestamp follows a
    // value of every row, and is allowed there and gives a date.
    assert_eq!(
        walked["context"],
        json!({ "outer": "select", "nested": null })
    );
    assert_eq!(
        walked["calls"][1]["follows"],
        json!({ "kind": "value", "dtype": "Datetime(us)", "shape": "many" })
    );
    assert_eq!(walked["calls"][1]["verdict"]["kind"], "allowed");
    assert_eq!(walked["calls"][1]["verdict"]["result"], "Date");
    assert_eq!(walked["ending"], json!({ "kind": "allowed" }));
    assert_eq!(answer["steps"][4]["walk"]["refusal"], Value::Null);
}

#[test]
fn a_step_polars_refuses_is_reported_at_its_link_and_the_next_is_not_run() {
    let answer = invoke("run_document", json!({ "document": refused_text() })).expect("a report");
    let steps = &answer["steps"];
    assert_eq!(steps[0]["outcome"]["status"], "valid");
    let refused = &steps[1]["outcome"];
    assert_eq!(refused["status"], "refused");
    // The sum is the first link, after the start, of the pair's value.
    assert_eq!(
        refused["place"],
        json!({
            "stepIndex": 1,
            "stepId": "total",
            "path": [
                { "kind": "parameter", "name": "named_exprs" },
                { "kind": "value", "index": 0 },
                { "kind": "link", "index": 0 }
            ]
        })
    );
    assert_eq!(refused["cause"]["kind"], "polars");
    assert!(
        refused["cause"]["reason"]
            .as_str()
            .expect("a reason")
            .contains("sum"),
        "Polars says which operation: {refused}"
    );
    assert_eq!(
        steps[2]["outcome"],
        json!({ "status": "notRun", "blockedBy": "total" })
    );
    // The tables refuse the same call before Polars is asked.
    let walk = &steps[1]["walk"];
    assert_eq!(walk["refusal"]["place"], refused["place"]);
    assert_eq!(walk["refusal"]["atEnd"], false);
    assert_eq!(walk["refusal"]["refusal"]["source"]["by"], "polars");
}

#[test]
fn a_step_with_something_still_to_be_picked_is_incomplete_at_it_and_the_next_is_not_run() {
    let answer =
        invoke("run_document", json!({ "document": incomplete_text() })).expect("a report");
    let steps = &answer["steps"];
    assert_eq!(steps[0]["outcome"]["status"], "valid");
    // The shift's argument `n`, whose start is the hole, in the first link of the pair's value.
    assert_eq!(
        steps[1]["outcome"],
        json!({
            "status": "incomplete",
            "place": {
                "stepIndex": 1,
                "stepId": "later",
                "path": [
                    { "kind": "parameter", "name": "named_exprs" },
                    { "kind": "value", "index": 0 },
                    { "kind": "link", "index": 0 },
                    { "kind": "parameter", "name": "n" },
                    { "kind": "start" }
                ]
            }
        })
    );
    // The step is walked up to the hole: the shift stands after a timestamp, for each row.
    let walked = &steps[1]["walk"]["expressions"];
    let item = walked
        .as_array()
        .expect("expressions")
        .iter()
        .find(|expression| {
            expression["place"]["path"]
                .as_array()
                .is_some_and(|path| path.len() == 2)
        })
        .expect("the value");
    assert_eq!(
        item["places"][0],
        json!({
            "context": { "outer": "select", "nested": null },
            "after": { "kind": "value", "dtype": "Datetime(us)", "shape": "many" }
        })
    );
    assert_eq!(
        steps[2]["outcome"],
        json!({ "status": "notRun", "blockedBy": "later" })
    );
}

#[test]
fn the_rows_of_an_incomplete_step_are_refused_and_those_after_it_are_not_run() {
    let rows = |step: &str| {
        invoke(
            "rows_after_step",
            json!({ "document": incomplete_text(), "step": step, "limit": 2 }),
        )
        .expect_err("no rows")
    };
    // The step that holds something still to be picked is refused, and the step after it is
    // not run.
    assert_eq!(rows("later")["kind"], "refused");
    assert_eq!(rows("after")["kind"], "notRun");
}

#[test]
fn a_text_that_is_not_a_document_is_reported_with_its_place_not_as_a_failure() {
    let unreadable = invoke(
        "run_document",
        json!({ "document": r#"{"version":2,"steps":[]}"# }),
    )
    .expect("an answer");
    assert_eq!(
        unreadable,
        json!({
            "status": "unreadable",
            "problem": {
                "kind": "refused",
                "place": { "stepIndex": null, "stepId": null, "path": [] },
                "reason": "version 2 is not one this engine reads; it reads version 1"
            }
        })
    );
    let syntax =
        invoke("run_document", json!({ "document": "{\n\"version\":" })).expect("an answer");
    assert_eq!(syntax["status"], "unreadable");
    assert_eq!(syntax["problem"]["kind"], "syntax");
    assert_eq!(syntax["problem"]["line"], 2);
}

#[test]
fn the_rows_command_gives_cells_as_text_and_a_null_as_null() {
    let answer = invoke(
        "rows_after_step",
        json!({ "document": steps_text(), "step": "days_open", "limit": 2 }),
    )
    .expect("rows");
    let columns = answer["columns"].as_array().expect("columns");
    let position = |name: &str| {
        columns
            .iter()
            .position(|column| column["name"] == name)
            .unwrap_or_else(|| panic!("no column {name}"))
    };
    assert_eq!(columns[position("resolved_day")]["dtype"], "Date");
    let rows = answer["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 2);
    // Sorted by key then time, the first row of A-1001 has no row before it.
    assert_eq!(rows[0][position("dcn")], "A-1001");
    assert_eq!(rows[0][position("timestamp")], "2026-03-02 09:15:00");
    assert_eq!(rows[0][position("resolved_day")], Value::Null);
    assert_eq!(rows[1][position("resolved_day")], "2026-03-02");
    assert_eq!(rows[1][position("days_open")], "1");
}

/// The cause a step is refused with, when it is the only step of the document.
fn cause_of(step: Value) -> Value {
    let text = json!({ "version": 1, "steps": [step] }).to_string();
    let answer = invoke("run_document", json!({ "document": text })).expect("a report");
    assert_eq!(
        answer["steps"][0]["outcome"]["status"], "refused",
        "{answer}"
    );
    answer["steps"][0]["outcome"]["cause"].clone()
}

#[test]
fn a_step_is_refused_for_what_is_not_connected_or_not_allowed_where_it_stands() {
    let column = |name: &str| {
        json!({ "expression": { "start": {
            "entry": "polars.col",
            "arguments": { "name": { "columns": [name] } }
        }, "then": [] } })
    };
    let join = json!({ "id": "join", "call": { "entry": "LazyFrame.join", "arguments": {
        "other": { "table": "nope" },
        "left_on": { "many": [column("dcn")] },
        "right_on": { "many": [column("dcn")] },
    } } });
    assert_eq!(
        cause_of(join),
        json!({ "kind": "unknownTable", "name": "nope" })
    );
    let with = |start: Value| {
        json!({ "id": "with", "call": { "entry": "LazyFrame.with_columns", "arguments": {
            "named_exprs": { "pairs": [[
                { "columns": ["x"] },
                { "expression": { "start": start, "then": [] } }
            ]] }
        } } })
    };
    assert_eq!(
        cause_of(with(json!({ "input": "nope" }))),
        json!({ "kind": "unknownInput", "name": "nope" })
    );
    assert_eq!(
        cause_of(with(json!({ "entry": "polars.element", "arguments": {} }))),
        json!({
            "kind": "notAllowed",
            "refusal": {
                "source": { "by": "rule", "rule": "element-only-in-pivot-aggregate" },
                "reason": "`polars.element` stands only in a pivot's aggregate"
            }
        })
    );
}

/// What `rows_after_step` rejects with, as the JSON the view receives.
fn rejection(arguments: Value) -> Value {
    invoke("rows_after_step", arguments).expect_err("a rejection")
}

/// The kind and the message of a rejection.
fn kind_and_message(rejection: &Value) -> (String, String) {
    let text = |field: &str| {
        rejection[field]
            .as_str()
            .unwrap_or_else(|| panic!("no {field} in {rejection}"))
            .to_owned()
    };
    (text("kind"), text("message"))
}

#[test]
fn a_preview_over_the_window_is_rejected_as_a_limit() {
    let (kind, message) = kind_and_message(&rejection(
        json!({ "document": steps_text(), "step": "sort", "limit": 1_001 }),
    ));
    assert_eq!(kind, "limit");
    assert_eq!(message, "a preview holds at most 1000 rows");
}

#[test]
fn a_step_the_document_lacks_is_rejected_as_no_such_step() {
    let (kind, message) = kind_and_message(&rejection(
        json!({ "document": steps_text(), "step": "nowhere", "limit": 1 }),
    ));
    assert_eq!(kind, "noSuchStep");
    assert!(
        message.contains("there is no step \"nowhere\""),
        "{message}"
    );
    // An identifier with no characters names no step either.
    let (kind, message) = kind_and_message(&rejection(
        json!({ "document": steps_text(), "step": "", "limit": 1 }),
    ));
    assert_eq!(kind, "noSuchStep");
    assert!(message.starts_with("naming the step"), "{message}");
}

#[test]
fn a_text_that_is_not_a_document_is_rejected_as_unreadable() {
    let (kind, message) = kind_and_message(&rejection(
        json!({ "document": "[]", "step": "sort", "limit": 1 }),
    ));
    assert_eq!(kind, "unreadable");
    assert!(message.starts_with("reading the document"), "{message}");
}

#[test]
fn a_refused_step_is_rejected_as_refused_and_the_step_after_it_as_not_run() {
    let (kind, message) = kind_and_message(&rejection(
        json!({ "document": refused_text(), "step": "total", "limit": 1 }),
    ));
    assert_eq!(kind, "refused");
    assert!(message.starts_with("previewing the rows"), "{message}");
    let (kind, message) = kind_and_message(&rejection(
        json!({ "document": refused_text(), "step": "after", "limit": 1 }),
    ));
    assert_eq!(kind, "notRun");
    assert!(message.starts_with("previewing the rows"), "{message}");
}

/// The `text` and the `step` of each line of the module a run reports.
fn module_lines(run: &Value) -> Vec<(String, Option<String>)> {
    run["python"]
        .as_array()
        .expect("a run reports the module written from its document")
        .iter()
        .map(|line| {
            (
                line["text"].as_str().expect("a line of text").to_owned(),
                line["step"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

#[test]
fn a_run_reports_each_steps_source_and_summary_and_the_module_written_from_the_document() {
    let run = json(&run_view(steps_text()).expect("a report"));
    let steps = run["steps"].as_array().expect("steps");
    let said = |id: &str| {
        let step = steps
            .iter()
            .find(|step| step["id"] == id)
            .unwrap_or_else(|| panic!("no step {id}"));
        (
            step["source"].as_str().expect("a source").to_owned(),
            step["summary"].as_str().expect("a summary").to_owned(),
        )
    };
    // Worked out by hand from the sample's steps: Python's method call, and what it works on.
    assert_eq!(
        said("sort"),
        (
            ".sort(\"dcn\", \"timestamp\")".to_owned(),
            "by dcn, timestamp".to_owned()
        )
    );
    assert_eq!(
        said("step_index"),
        (
            ".with_columns(pl.col(\"dcn\").cum_count().over(pl.col(\"dcn\")).alias(\"step_index\"))"
                .to_owned(),
            "step_index".to_owned()
        )
    );
    assert_eq!(
        said("join"),
        (
            ".join(step_sla, how=\"left\", left_on=\"step_to\", right_on=\"step\", maintain_order=\"left\")"
                .to_owned(),
            "step_sla on step_to".to_owned()
        )
    );
    assert_eq!(said("pivot").1, "on step_to");

    // The module is the document's own, line by line, each step's line naming its step.
    let lines = module_lines(&run);
    assert_eq!(lines[0], ("from datetime import date".to_owned(), None));
    assert!(lines.contains(&("def run(".to_owned(), None)));
    assert!(lines.contains(&("    claims: pl.LazyFrame,".to_owned(), None)));
    assert!(lines.contains(&("    holidays: pl.Series,".to_owned(), None)));
    assert!(lines.contains(&(
        "        .sort(\"dcn\", \"timestamp\")".to_owned(),
        Some("sort".to_owned())
    )));
    assert_eq!(
        lines.iter().filter(|(_, step)| step.is_some()).count(),
        steps.len()
    );
}

#[test]
fn a_run_marks_the_module_it_writes_for_a_step_that_is_not_finished() {
    let run = json(&run_view(&incomplete_text()).expect("a report"));
    let lines = module_lines(&run);
    assert_eq!(
        lines[0],
        (
            "# Not runnable as written: step \"later\", something in it is still to be picked."
                .to_owned(),
            Some("later".to_owned())
        )
    );
}

#[test]
fn text_that_is_not_a_document_has_no_module() {
    let run = json(&run_view(r#"{"version":2,"steps":[]}"#).expect("an answer"));
    assert_eq!(run["status"], "unreadable");
    assert!(run.get("python").is_none());
}

#[test]
fn the_snapshots_are_what_the_commands_return() {
    let snapshots: [(&str, Value); 9] = [
        ("node-inputs.json", json(&node_view().expect("the node"))),
        (
            "step-document.json",
            serde_json::from_str(steps_text()).expect("the sample steps are JSON"),
        ),
        (
            "step-report.json",
            json(&run_view(steps_text()).expect("a report")),
        ),
        (
            "step-report-refused.json",
            json(&run_view(&refused_text()).expect("a report")),
        ),
        (
            "step-report-incomplete.json",
            json(&run_view(&incomplete_text()).expect("a report")),
        ),
        (
            "step-report-unreadable.json",
            json(&run_view(r#"{"version":2,"steps":[]}"#).expect("an answer")),
        ),
        (
            "step-report-variants.json",
            json(&crate::report::fixtures::variants()),
        ),
        ("vocabulary.json", vocabulary()),
        (
            "step-rows.json",
            json(&rows_view(steps_text(), "days_open", 7).expect("rows")),
        ),
    ];
    for (file, returned) in &snapshots {
        crate::snapshot::check(file, returned, REFRESH, REFRESH_COMMAND);
    }
}
