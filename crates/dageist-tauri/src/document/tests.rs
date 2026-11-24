//! The node's document and its edits through the host: what a window receives of the document,
//! each edit as the view sends it, and what the host answers when an edit is refused or stale.
//! Every expected value is written out by hand from the documents below and the engine's
//! document format.

use dageist_engine::document::{Place, Segment};
use serde_json::{Value, json};

use super::{
    Documents, EditAnswerView, EditRequest, ProblemView, ThingView, ValueKindView, WantedView,
    document_state_view, edit_view,
};
use crate::node::tests::{documents_of, invoke_on, json};
use crate::sample_table::{ErrorKind, ErrorView};

/// Set to rewrite the snapshots instead of checking them.
const REFRESH: &str = "DAGEIST_REFRESH_EDITS";

/// The command that refreshes the snapshots.
const REFRESH_COMMAND: &str = "cargo test --offline --locked -p dageist-tauri document::tests::the_snapshots_are_what_the_commands_return";

fn column(name: &str) -> Value {
    json!({ "expression": { "start": {
        "entry": "polars.col",
        "arguments": { "name": { "columns": [name] } }
    }, "then": [] } })
}

/// A sort by `dcn`, then a column `earlier` that is the timestamp shifted by a row.
fn small_text() -> String {
    json!({ "version": 1, "steps": [
        { "id": "sort", "call": { "entry": "LazyFrame.sort", "arguments": {
            "by": { "many": [column("dcn")] },
            "descending": { "many": [{ "flag": false }] }
        } } },
        { "id": "earlier", "call": { "entry": "LazyFrame.with_columns", "arguments": {
            "named_exprs": { "pairs": [[
                { "columns": ["earlier"] },
                { "expression": { "start": {
                    "entry": "polars.col",
                    "arguments": { "name": { "columns": ["timestamp"] } }
                }, "then": [{ "entry": "Expr.shift", "arguments": {
                    "n": { "expression": { "start": {
                        "entry": "polars.lit",
                        "arguments": { "value": { "integer": 1 } }
                    }, "then": [] } }
                } }] } }
            ]] }
        } } }
    ] })
    .to_string()
}

fn state(documents: &Documents) -> Value {
    json(&document_state_view(documents).expect("a state"))
}

/// Sends `edit` against `revision` on `documents`, as the view's IPC does.
fn edit(documents: Documents, revision: u64, edit: Value) -> Result<Value, Value> {
    invoke_on(
        documents,
        "edit_document",
        json!({ "revision": revision, "edit": edit }),
    )
}

fn step_place(index: usize, id: &str) -> Value {
    json!({ "stepIndex": index, "stepId": id, "path": [] })
}

fn place(index: usize, id: &str, path: Value) -> Value {
    json!({ "stepIndex": index, "stepId": id, "path": path })
}

#[test]
fn the_document_is_sent_as_the_calls_it_holds_with_numbers_as_the_engine_words_them() {
    let documents = documents_of(&small_text());
    let sent = state(&documents);
    assert_eq!(sent["revision"], 1);
    let steps = &sent["document"]["steps"];
    assert_eq!(steps[0]["id"], "sort");
    assert_eq!(
        steps[0]["call"],
        json!({
            "entry": "LazyFrame.sort",
            "arguments": [
                { "parameter": "by", "value": { "kind": "many", "items": [{
                    "kind": "expression",
                    "start": { "kind": "call", "call": {
                        "entry": "polars.col",
                        "arguments": [{ "parameter": "name", "value": {
                            "kind": "columns", "names": ["dcn"]
                        } }]
                    } },
                    "links": []
                }] } },
                { "parameter": "descending", "value": { "kind": "many", "items": [
                    { "kind": "flag", "value": false }
                ] } }
            ]
        })
    );
    // The shift's argument is a literal one, with the words the engine shows it as.
    let link = &steps[1]["call"]["arguments"][0]["value"]["entries"][0]["value"]["links"][0];
    assert_eq!(link["entry"], "Expr.shift");
    assert_eq!(
        link["arguments"][0]["value"]["start"]["call"]["arguments"][0]["value"],
        json!({ "kind": "integer", "shown": "1" })
    );
    // And what running it reported comes with it.
    assert_eq!(sent["run"]["steps"][0]["id"], "sort");
    assert_eq!(sent["run"]["steps"][0]["outcome"]["status"], "valid");
}

#[test]
fn a_step_added_is_started_by_the_engine_and_the_revision_moves_on() {
    let answer = edit(
        documents_of(&small_text()),
        1,
        json!({
            "kind": "addStep",
            "at": 1,
            "entry": "LazyFrame.filter",
            "frame": { "columns": ["dcn", "timestamp"], "tables": [] }
        }),
    )
    .expect("an answer");
    assert_eq!(answer["status"], "edited");
    assert_eq!(answer["revision"], 2);
    let steps = answer["document"]["steps"].as_array().expect("steps");
    let ids: Vec<&str> = steps
        .iter()
        .filter_map(|step| step["id"].as_str())
        .collect();
    assert_eq!(ids, ["sort", "filter", "earlier"]);
    // A new filter starts as an equality of a field and a value still to be picked.
    assert_eq!(
        steps[1]["call"]["arguments"][0],
        json!({ "parameter": "predicates", "value": { "kind": "many", "items": [{
            "kind": "expression",
            "start": { "kind": "hole" },
            "links": [{ "entry": "Expr.eq", "arguments": [{
                "parameter": "other",
                "value": { "kind": "expression", "start": { "kind": "hole" }, "links": [] }
            }] }]
        }] } })
    );
    // The run is of the edited document: the filter is incomplete, and the step after it waits.
    let run = answer["run"]["steps"].as_array().expect("a run");
    assert_eq!(run.len(), 3);
    assert_eq!(run[1]["outcome"]["status"], "incomplete");
    assert_eq!(run[2]["outcome"]["status"], "notRun");
}

#[test]
fn an_argument_is_set_from_what_the_view_sends_and_the_next_edit_follows_the_new_revision() {
    let documents = documents_of(&small_text());
    let shift_n = place(
        1,
        "earlier",
        json!([
            { "kind": "parameter", "name": "named_exprs" },
            { "kind": "value", "index": 0 },
            { "kind": "link", "index": 0 },
            { "kind": "parameter", "name": "n" }
        ]),
    );
    let sent = invoke_on(
        documents,
        "edit_document",
        json!({ "revision": 1, "edit": {
            "kind": "setArgument",
            "place": shift_n,
            "value": { "kind": "expression", "expression": {
                "kind": "literal", "literal": { "kind": "integer", "value": 3 }
            } }
        } }),
    )
    .expect("an answer");
    assert_eq!(sent["revision"], 2);
    let link = &sent["document"]["steps"][1]["call"]["arguments"][0]["value"]["entries"][0]["value"]
        ["links"][0];
    assert_eq!(
        link["arguments"][0]["value"]["start"]["call"]["arguments"][0]["value"],
        json!({ "kind": "integer", "shown": "3" })
    );
}

#[test]
fn a_method_inserted_where_the_report_says_it_stands_gets_the_default_the_tables_allow() {
    let documents = documents_of(&small_text());
    let standing = json!({
        "positions": [{ "kind": "outer", "name": "select" }],
        "after": { "kind": "value", "dtype": "Datetime(us)", "shape": "many" }
    });
    let sent = invoke_on(
        documents,
        "edit_document",
        json!({ "revision": 1, "edit": {
            "kind": "insertLink",
            "expression": place(1, "earlier", json!([
                { "kind": "parameter", "name": "named_exprs" },
                { "kind": "value", "index": 0 }
            ])),
            "at": 1,
            "entry": "Expr.dt.date",
            "standing": standing
        } }),
    )
    .expect("an answer");
    let links = &sent["document"]["steps"][1]["call"]["arguments"][0]["value"]["entries"][0]["value"]
        ["links"];
    assert_eq!(links[1]["entry"], "Expr.dt.date");
    assert_eq!(links[1]["arguments"], json!([]));
}

#[test]
fn an_edit_made_against_an_older_revision_is_refused_and_changes_nothing() {
    let documents = documents_of(&small_text());
    // The edit at the right revision is made; the host now holds revision 2.
    let after = edit_view(
        &documents,
        1,
        &serde_json::from_value::<EditRequest>(
            json!({ "kind": "removeStep", "step": step_place(1, "earlier") }),
        )
        .expect("a request"),
    )
    .expect("an answer");
    assert_eq!(json(&after)["revision"], 2);
    // A second edit that was made against revision 1 is stale, whatever it says.
    let again = invoke_on(
        documents_clone(&documents),
        "edit_document",
        json!({ "revision": 1, "edit": { "kind": "removeStep", "step": step_place(0, "sort") } }),
    )
    .expect_err("a rejection");
    assert_eq!(again["kind"], "stale");
    assert!(
        again["message"]
            .as_str()
            .is_some_and(|message| message.contains("revision 2")),
        "{again}"
    );
    // The document is as the first edit left it.
    assert_eq!(state(&documents)["revision"], 2);
    assert_eq!(
        state(&documents)["document"]["steps"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
}

fn documents_clone(documents: &Documents) -> Documents {
    let (document, revision) = documents.snapshot();
    let held = Documents::of(document);
    held.set_revision(revision);
    held
}

#[test]
fn an_edit_the_engine_refuses_is_answered_with_its_place_and_its_kind_and_changes_nothing() {
    let documents = documents_of(&small_text());
    let answer = edit(
        documents_clone(&documents),
        1,
        json!({ "kind": "removeStep", "step": step_place(7, "nowhere") }),
    )
    .expect("an answer");
    assert_eq!(answer["status"], "refused");
    assert_eq!(answer["revision"], 1);
    assert_eq!(answer["place"], step_place(7, "nowhere"));
    assert_eq!(answer["problem"], json!({ "kind": "noStep", "steps": 2 }));
    assert_eq!(
        answer["message"],
        "step \"nowhere\": there is no such step: the document has 2"
    );
    // A place that is out of date names the step that is there.
    let answer = edit(
        documents_clone(&documents),
        1,
        json!({ "kind": "removeStep", "step": step_place(1, "sort") }),
    )
    .expect("an answer");
    assert_eq!(
        answer["problem"],
        json!({ "kind": "stepChanged", "found": "earlier" })
    );
    // What the engine says of a part that is not what the edit needs is a kind of thing, not words.
    let answer = edit(
        documents_clone(&documents),
        1,
        json!({ "kind": "insertItem",
            "list": place(0, "sort", json!([{ "kind": "parameter", "name": "descending" }, { "kind": "item", "index": 0 }])),
            "at": 0,
            "item": { "kind": "flag", "value": true } }),
    )
    .expect("an answer");
    assert_eq!(
        answer["problem"],
        json!({ "kind": "wrongPlace", "needs": "list", "found": { "kind": "value", "of": "flag" } })
    );
    // The model's own refusal says why in words.
    let answer = edit(
        documents_clone(&documents),
        1,
        json!({ "kind": "setArgument",
            "place": place(0, "sort", json!([{ "kind": "parameter", "name": "descending" }])),
            "value": { "kind": "integer", "value": 1 } }),
    )
    .expect("an answer");
    assert_eq!(answer["problem"]["kind"], "refused");
    assert!(
        answer["problem"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("descending")),
        "{answer}"
    );
    // None of them moved the revision.
    assert_eq!(state(&documents)["revision"], 1);
}

#[test]
fn an_option_that_needs_its_value_is_refused_with_the_parameter_it_needs_it_for() {
    let documents = documents_of(
        &json!({ "version": 1, "steps": [
            { "id": "indexed", "call": { "entry": "LazyFrame.with_row_index", "arguments": {} } }
        ] })
        .to_string(),
    );
    let answer = edit(
        documents,
        1,
        json!({ "kind": "addOption", "call": step_place(0, "indexed"), "parameter": "name" }),
    )
    .expect("an answer");
    assert_eq!(
        answer["problem"],
        json!({ "kind": "needsValue", "parameter": "name" })
    );
    assert_eq!(
        answer["place"],
        place(
            0,
            "indexed",
            json!([{ "kind": "parameter", "name": "name" }])
        )
    );
}

#[test]
fn the_rows_are_asked_of_the_document_at_a_revision_and_a_stale_ask_is_refused() {
    let documents = documents_of(&small_text());
    let rows = invoke_on(
        documents_clone(&documents),
        "rows_after_step",
        json!({ "revision": 1, "step": "sort", "limit": 2 }),
    )
    .expect("rows");
    assert_eq!(rows["rows"].as_array().map(Vec::len), Some(2));
    let stale = invoke_on(
        documents,
        "rows_after_step",
        json!({ "revision": 9, "step": "sort", "limit": 2 }),
    )
    .expect_err("a rejection");
    assert_eq!(stale["kind"], "stale");
}

#[test]
fn a_request_that_names_a_dtype_the_engine_has_no_word_for_is_a_failed_command() {
    let documents = documents_of(&small_text());
    let failed = edit(
        documents,
        1,
        json!({ "kind": "setArgument",
            "place": place(0, "sort", json!([{ "kind": "parameter", "name": "descending" }])),
            "value": { "kind": "dtype", "name": "Quark" } }),
    )
    .expect_err("a rejection");
    assert_eq!(failed["kind"], "unknownName");
}

#[test]
fn every_edit_the_engine_makes_is_a_request_the_host_reads() {
    // Each request, written out as the view sends it, reads; applying them is the engine's own.
    let requests = [
        json!({ "kind": "addStep", "at": 0, "entry": "LazyFrame.head", "frame": { "columns": [], "tables": [] } }),
        json!({ "kind": "swapStep", "step": step_place(0, "sort"), "entry": "LazyFrame.head", "frame": { "columns": [], "tables": [] } }),
        json!({ "kind": "removeStep", "step": step_place(0, "sort") }),
        json!({ "kind": "moveStep", "step": step_place(0, "sort"), "to": 1 }),
        json!({ "kind": "setArgument", "place": step_place(0, "sort"), "value": { "kind": "many", "items": [] } }),
        json!({ "kind": "clearArgument", "place": step_place(0, "sort") }),
        json!({ "kind": "addOption", "call": step_place(0, "sort"), "parameter": "descending" }),
        json!({ "kind": "setForItem", "call": step_place(0, "sort"), "parameter": "descending", "item": 0, "value": { "kind": "flag", "value": true } }),
        json!({ "kind": "insertItem", "list": step_place(0, "sort"), "at": 0, "item": { "kind": "columns", "names": ["a"] } }),
        json!({ "kind": "removeItem", "item": step_place(0, "sort") }),
        json!({ "kind": "insertTest", "list": step_place(0, "sort"), "at": 0 }),
        json!({ "kind": "insertEntry", "pairs": step_place(0, "sort"), "at": 0,
            "key": { "kind": "columns", "names": ["a"] }, "value": { "kind": "string", "value": "b" } }),
        json!({ "kind": "removeEntry", "entry": step_place(0, "sort") }),
        json!({ "kind": "setStart", "expression": step_place(0, "sort"), "start": { "kind": "field", "name": "a" } }),
        json!({ "kind": "startWith", "expression": step_place(0, "sort"), "entry": "polars.when" }),
        json!({ "kind": "insertLink", "expression": step_place(0, "sort"), "at": 0, "entry": "Expr.abs", "standing": null }),
        json!({ "kind": "swapLink", "link": step_place(0, "sort"), "entry": "Expr.abs", "standing": null }),
        json!({ "kind": "removeLink", "link": step_place(0, "sort") }),
        json!({ "kind": "moveLink", "link": step_place(0, "sort"), "to": 0 }),
    ];
    for request in requests {
        serde_json::from_value::<EditRequest>(request.clone())
            .unwrap_or_else(|error| panic!("{request}: {error}"));
    }
}

#[test]
fn a_request_with_a_field_the_edit_lacks_or_a_kind_it_has_not_is_not_read() {
    for request in [
        json!({ "kind": "removeStep", "step": step_place(0, "sort"), "extra": 1 }),
        json!({ "kind": "levitate" }),
        json!({ "kind": "addStep", "at": -1, "entry": "LazyFrame.head", "frame": { "columns": [], "tables": [] } }),
    ] {
        assert!(
            serde_json::from_value::<EditRequest>(request.clone()).is_err(),
            "{request}"
        );
    }
}

/// The documents the snapshots are written from: the small document, after one edit that was made
/// and one that was refused.
fn made_and_refused() -> (Value, Value) {
    let documents = documents_of(&small_text());
    let made = edit(
        documents_clone(&documents),
        1,
        json!({ "kind": "addStep", "at": 1, "entry": "LazyFrame.head",
            "frame": { "columns": ["dcn", "timestamp"], "tables": [] } }),
    )
    .expect("an answer");
    let refused = edit(
        documents,
        1,
        json!({ "kind": "removeStep", "step": step_place(7, "nowhere") }),
    )
    .expect("an answer");
    (made, refused)
}

/// The words of the closed lists of an edit's refusal.
pub(crate) fn vocabulary() -> Value {
    json!({
        "valueKinds": ValueKindView::ALL.map(|kind| json(&kind)),
        "wanted": WantedView::ALL.map(|word| json(&word)),
        "thingKinds": ThingView::KINDS,
        "problemKinds": ProblemView::KINDS,
    })
}

#[test]
fn the_snapshots_are_what_the_commands_return() {
    let (made, refused) = made_and_refused();
    let snapshots: [(&str, Value); 2] =
        [("edit-edited.json", made), ("edit-refused.json", refused)];
    for (file, returned) in &snapshots {
        crate::snapshot::check(file, returned, REFRESH, REFRESH_COMMAND);
    }
}

#[test]
fn the_places_an_edit_is_asked_at_are_the_places_a_report_gives() {
    // A report's place and a request's place are one shape, so a place is sent back as it came.
    let sent = place(
        1,
        "earlier",
        json!([{ "kind": "parameter", "name": "named_exprs" }]),
    );
    let view: crate::report::EditPlace = serde_json::from_value(sent.clone()).expect("a place");
    let made: Place = view.place().expect("a place");
    assert_eq!(made.step_index(), Some(1));
    assert_eq!(made.step_id().map(|id| id.as_str()), Some("earlier"));
    assert_eq!(made.path(), [Segment::Parameter("named_exprs".to_owned())]);
    assert_eq!(json(&crate::report::place_view(&made)), sent);
}

#[test]
fn a_place_that_names_no_step_or_names_one_without_its_identifier_is_not_a_request() {
    // An edit works inside a step. A place with no step, or one without the identifier that
    // checks it is the step the person saw, is refused when it is read, not answered as a step
    // that is not there.
    for place in [
        json!({ "path": [] }),
        json!({ "stepIndex": 0, "path": [] }),
        json!({ "stepId": "sort", "path": [] }),
        json!({ "stepIndex": 0, "stepId": null, "path": [] }),
        json!({ "stepIndex": null, "stepId": "sort", "path": [] }),
    ] {
        assert!(
            serde_json::from_value::<EditRequest>(json!({ "kind": "removeStep", "step": place }))
                .is_err(),
            "{place}"
        );
    }
    // A step named by an empty identifier is a failed command.
    let refused = edit(
        documents_of(&small_text()),
        1,
        json!({ "kind": "removeStep", "step": step_place(0, "") }),
    )
    .expect_err("a failed command");
    assert_eq!(refused["kind"], "unknownName");
}

#[test]
fn an_edit_whose_answer_cannot_be_given_is_not_made() {
    // The document is held only once the answer to it has been made, so an answer that fails
    // leaves the document and its revision as they were.
    let documents = documents_of(&small_text());
    let before = state(&documents);
    let request: EditRequest =
        serde_json::from_value(json!({ "kind": "removeStep", "step": step_place(1, "earlier") }))
            .expect("a request");

    let failed = documents.edit(1, &request, |_, _| {
        Err(ErrorView::of(ErrorKind::Failed, "the run failed"))
    });

    assert!(failed.is_err());
    assert_eq!(state(&documents), before);
    // The same edit then goes through, against the same revision.
    let made = documents
        .edit(1, &request, super::state_of)
        .expect("an answer");
    assert!(matches!(made, EditAnswerView::Edited(_)));
    assert_eq!(state(&documents)["revision"], 2);
}

#[test]
fn two_writers_on_one_revision_make_one_edit_and_the_other_is_stale() {
    // Eight clients send an edit against revision 1 at once: one wins, and the revision moves by
    // exactly one.
    let documents = documents_of(&small_text());
    let outcomes: Vec<Result<Value, Value>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    let request: EditRequest = serde_json::from_value(json!({
                        "kind": "removeStep",
                        "step": step_place(1, "earlier")
                    }))
                    .expect("a request");
                    super::edit_view(&documents, 1, &request)
                        .map(|answer| json(&answer))
                        .map_err(|error| json(&error))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a client"))
            .collect()
    });

    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    for outcome in outcomes.iter().filter_map(|outcome| outcome.as_ref().err()) {
        assert_eq!(outcome["kind"], "stale");
    }
    let held = state(&documents);
    assert_eq!(held["revision"], 2);
    assert_eq!(held["document"]["steps"].as_array().map(Vec::len), Some(1));
}
