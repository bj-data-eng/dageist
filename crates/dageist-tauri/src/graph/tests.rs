//! The graph commands through Tauri's IPC layer and the snapshot of the graph view, on the
//! starting graph: the claim-timing sample's tables, its two parameters, its transform and an
//! output. The expected values are written out by hand from that graph.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use dageist::Bindings;
use dageist_engine::claim_timing::ClaimTiming;
use dageist_engine::document::Document;
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};

use super::{Workspace, graph_view, starting_session, starting_transform};

/// Set to rewrite the snapshot instead of checking it.
const REFRESH: &str = "DAGEIST_REFRESH_GRAPH";

/// The command that refreshes the snapshot.
const REFRESH_COMMAND: &str = "cargo test --offline --locked -p dageist-tauri graph::tests::the_snapshot_is_what_the_command_returns";

/// The ids of the starting graph's nodes, in the order they are added, as the view sends them.
const CLAIMS: &str = "00000000-0000-7000-8000-000000000001";
const SLA: &str = "00000000-0000-7000-8000-000000000002";
const NAMES: &str = "00000000-0000-7000-8000-000000000003";
const CALENDAR: &str = "00000000-0000-7000-8000-000000000004";
const PARAMETERS: &str = "00000000-0000-7000-8000-000000000005";
/// The id of the starting graph's transform.
pub(crate) const NODE: &str = "00000000-0000-7000-8000-000000000006";
const OUTPUT: &str = "00000000-0000-7000-8000-000000000007";

/// A folder of its own for each workspace a test makes, so that tests running at once do not
/// write each other's tables.
fn folder() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "dageist-graph-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// The starting graph with `steps` as its transform's document. Every revision starts at 0, so
/// the document, set once, is at revision 1.
pub(crate) fn sample_of(steps: Document) -> Workspace {
    Workspace::of(starting_session(0, &folder(), steps).expect("the starting graph"))
}

/// The starting graph with the sample's own steps.
pub(crate) fn sample() -> Workspace {
    sample_of(Document::read(ClaimTiming::STEPS_JSON).expect("the sample's steps"))
}

/// What the starting graph's transform runs on.
pub(crate) fn sample_bindings() -> Bindings {
    crate::node::bindings_of(&sample().lock(), &starting_transform()).expect("the bindings")
}

fn window(workspace: Workspace) -> WebviewWindow<MockRuntime> {
    let app = mock_builder()
        .manage(workspace)
        .invoke_handler(tauri::generate_handler![
            super::graph,
            super::edit_graph,
            super::place_node,
        ])
        .build(mock_context(noop_assets()))
        .expect("the mock application builds");
    WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .expect("the mock window builds")
}

/// Calls `command` as the view's IPC does on `workspace`.
fn invoke(workspace: Workspace, command: &str, arguments: Value) -> Result<Value, Value> {
    let request = InvokeRequest {
        cmd: command.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().expect("a valid origin"),
        body: InvokeBody::Json(arguments),
        headers: tauri::http::HeaderMap::default(),
        invoke_key: INVOKE_KEY.to_owned(),
    };
    match tauri::test::get_ipc_response(&window(workspace), request)? {
        InvokeResponseBody::Json(text) => Ok(serde_json::from_str(&text).expect("JSON")),
        other => panic!("expected JSON, got {other:?}"),
    }
}

fn node<'a>(graph: &'a Value, id: &str) -> &'a Value {
    graph["nodes"]
        .as_array()
        .expect("a list of nodes")
        .iter()
        .find(|node| node["id"] == id)
        .unwrap_or_else(|| panic!("no node {id}"))
}

/// The graph view with each file's folder written as `/sample`, so that the snapshot does not
/// hold the folder of the machine it was written on.
fn without_folders(mut graph: Value) -> Value {
    for node in graph["nodes"].as_array_mut().expect("a list of nodes") {
        if let Some(path) = node["file"]["path"].as_str() {
            let name = std::path::Path::new(path)
                .file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned();
            node["file"]["path"] = json!(format!("/sample/{name}"));
        }
    }
    graph
}

#[test]
fn the_starting_graph_lists_every_node_after_the_nodes_it_reads() {
    let graph = invoke(sample(), "graph", json!({})).expect("the graph");
    let order: Vec<_> = graph["nodes"]
        .as_array()
        .expect("a list of nodes")
        .iter()
        .map(|node| node["id"].as_str().expect("an id").to_owned())
        .collect();
    assert_eq!(
        order,
        [CLAIMS, SLA, NAMES, CALENDAR, PARAMETERS, NODE, OUTPUT]
    );
    let transform = node(&graph, NODE);
    assert_eq!(transform["kind"], "transform");
    assert_eq!(transform["summary"], "12 steps");
    assert_eq!(transform["problems"], json!([]));
    assert_eq!(
        transform["slots"][0],
        json!({
            "slot": { "kind": "start" },
            "takes": "table",
            "from": { "kind": "table", "node": CLAIMS },
            "fits": [
                { "kind": "table", "node": CLAIMS },
                { "kind": "table", "node": SLA },
                { "kind": "table", "node": NAMES },
                { "kind": "table", "node": CALENDAR },
            ],
        })
    );
    let output = node(&graph, OUTPUT);
    assert_eq!(output["problems"], json!(["no file is chosen"]));
    assert_eq!(
        output["columns"][0],
        json!({ "name": "dcn", "dtype": "String" })
    );
}

#[test]
fn an_edit_answers_with_the_edited_graph_and_a_refusal_with_why() {
    let workspace = sample();
    let revision = graph_view(&workspace).revision;
    let added = invoke(
        workspace,
        "edit_graph",
        json!({ "revision": revision, "edit": { "edit": "add", "kind": "transform", "follows": CLAIMS } }),
    )
    .expect("an answer");
    assert_eq!(added["graph"]["revision"], revision + 1);
    assert_eq!(added["kind"], "edited");
    // The host mints the new node's id.
    let id = added["added"].as_str().expect("the id of the node added");
    assert!(![CLAIMS, SLA, NAMES, CALENDAR, PARAMETERS, NODE, OUTPUT].contains(&id));
    assert_eq!(node(&added["graph"], id)["name"], "Transform");
    assert_eq!(
        node(&added["graph"], id)["slots"][0]["from"],
        json!({ "kind": "table", "node": CLAIMS })
    );
    let workspace = sample();
    let revision = graph_view(&workspace).revision;
    let refused = invoke(
        workspace,
        "edit_graph",
        json!({ "revision": revision, "edit": {
            "edit": "connect", "node": CLAIMS, "slot": { "kind": "start" },
            "from": { "kind": "table", "node": NODE },
        } }),
    )
    .expect("an answer");
    assert_eq!(refused["kind"], "refused");
    assert_eq!(refused["revision"], revision);
}

#[test]
fn an_edit_against_another_revision_is_stale_and_an_id_that_is_not_one_is_unknown() {
    let stale = invoke(
        sample(),
        "edit_graph",
        json!({ "revision": 99_999, "edit": { "edit": "remove", "node": CLAIMS } }),
    )
    .expect_err("a rejection");
    assert_eq!(stale["kind"], "stale");
    let workspace = sample();
    let revision = graph_view(&workspace).revision;
    let unknown = invoke(
        workspace,
        "edit_graph",
        json!({ "revision": revision, "edit": { "edit": "remove", "node": "Not An Id" } }),
    )
    .expect_err("a rejection");
    assert_eq!(unknown["kind"], "unknownName");
}

#[test]
fn placing_a_box_moves_the_layout_on_and_leaves_the_graph_where_it_was() {
    let workspace = sample();
    let before = graph_view(&workspace);
    let placed = super::place_view(
        &workspace,
        before.layout_revision,
        NODE,
        super::FrameView {
            x: 40,
            y: -20,
            width: Some(320),
            height: Some(400),
        },
    )
    .expect("placed");
    let after = graph_view(&workspace);
    assert_eq!(placed.layout_revision, before.layout_revision + 1);
    assert_eq!(after.revision, before.revision);
    let graph = serde_json::to_value(&after).expect("a view");
    assert_eq!(
        node(&graph, NODE)["frame"],
        json!({ "x": 40, "y": -20, "width": 320, "height": 400 })
    );
    let stale = invoke(
        sample(),
        "place_node",
        json!({ "revision": 99_999, "node": NODE, "frame": { "x": 0, "y": 0, "width": null, "height": null } }),
    )
    .expect_err("a rejection");
    assert_eq!(stale["kind"], "stale");
}

#[test]
fn the_snapshot_is_what_the_command_returns() {
    let graph = without_folders(serde_json::to_value(graph_view(&sample())).expect("a view"));
    crate::snapshot::check("graph.json", &graph, REFRESH, REFRESH_COMMAND);
}

#[test]
fn a_transform_with_nothing_to_start_from_says_so_and_a_stale_edit_on_it_is_stale() {
    let workspace = sample();
    let revision = graph_view(&workspace).revision;
    let request: super::GraphEditRequest = serde_json::from_value(
        json!({ "edit": "add", "kind": "transform", "follows": null, "name": "Loose" }),
    )
    .expect("a request");
    let super::GraphEditAnswerView::Edited {
        added: Some(loose), ..
    } = super::edit_graph_view(&workspace, revision, &request).expect("an answer")
    else {
        panic!("the transform was not added");
    };
    let state = crate::document::document_state_view(&workspace, &loose).expect_err("no state");
    let state = serde_json::to_value(&state).expect("a view");
    assert_eq!(state["kind"], "failed");
    assert!(
        state["message"]
            .as_str()
            .is_some_and(|message| message.contains("no table to start from")),
        "{state}"
    );
    // The document starts at revision 0, so an edit made against another is stale before the
    // missing table is mentioned.
    let edit: crate::document::EditRequest = serde_json::from_value(
        json!({ "kind": "removeStep", "step": { "stepIndex": 0, "stepId": "sort", "path": [] } }),
    )
    .expect("a request");
    let stale = crate::document::edit_view(&workspace, &loose, 7, &edit).expect_err("stale");
    assert_eq!(
        serde_json::to_value(&stale).expect("a view")["kind"],
        "stale"
    );
}

#[test]
fn a_size_needs_both_its_width_and_its_height() {
    let workspace = sample();
    let revision = graph_view(&workspace).layout_revision;
    let half = super::FrameView {
        x: 0,
        y: 0,
        width: Some(300),
        height: None,
    };
    assert!(super::place_view(&workspace, revision, NODE, half).is_err());
    assert_eq!(graph_view(&workspace).layout_revision, revision);
}
