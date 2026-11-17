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
    let mut words = crate::report::vocabulary();
    let (Some(words), Value::Object(catalogue)) =
        (words.as_object_mut(), crate::catalogue::vocabulary())
    else {
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

#[test]
fn the_inputs_command_reports_the_source_and_every_input_by_name() {
    let answer = invoke("node_inputs", json!({})).expect("the inputs");
    assert_eq!(
        answer,
        json!({
            "source": { "name": "claims", "columns": claim_columns() },
            "inputs": [
                { "kind": "value", "name": "as_of", "dtype": "Date" },
                { "kind": "list", "name": "holidays", "dtype": "Date" },
                { "kind": "value", "name": "sla_cutoff", "dtype": "Int64" },
                { "kind": "table", "name": "step_names",
                  "columns": [the_field("step_to", "String")] },
                { "kind": "table", "name": "step_sla",
                  "columns": [the_field("step", "String"), the_field("sla_days", "Int64")] },
            ]
        })
    );
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

#[test]
fn the_rows_command_rejects_what_it_cannot_answer_with_a_message() {
    let message = |arguments: Value| {
        invoke("rows_after_step", arguments).expect_err("a rejection")["message"]
            .as_str()
            .expect("a message")
            .to_owned()
    };
    assert_eq!(
        message(json!({ "document": steps_text(), "step": "sort", "limit": 1_001 })),
        "a preview holds at most 1000 rows"
    );
    assert!(
        message(json!({ "document": steps_text(), "step": "nowhere", "limit": 1 }))
            .contains("there is no step \"nowhere\"")
    );
    assert!(
        message(json!({ "document": steps_text(), "step": "", "limit": 1 }))
            .starts_with("naming the step")
    );
    assert!(
        message(json!({ "document": "[]", "step": "sort", "limit": 1 }))
            .starts_with("reading the document")
    );
    // A step after a refused one has no rows.
    assert!(
        message(json!({ "document": refused_text(), "step": "after", "limit": 1 }))
            .starts_with("previewing the rows")
    );
}

#[test]
fn the_snapshots_are_what_the_commands_return() {
    let snapshots: [(&str, Value); 7] = [
        ("node-inputs.json", json(&node_view().expect("the node"))),
        (
            "step-report.json",
            json(&run_view(steps_text()).expect("a report")),
        ),
        (
            "step-report-refused.json",
            json(&run_view(&refused_text()).expect("a report")),
        ),
        (
            "step-report-unreadable.json",
            json(&run_view(r#"{"version":2,"steps":[]}"#).expect("an answer")),
        ),
        (
            "step-report-variants.json",
            json(&crate::report::variants()),
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
