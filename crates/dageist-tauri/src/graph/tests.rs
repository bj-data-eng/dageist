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

/// The id of the starting graph's transform, as the view sends it.
pub(crate) const NODE: &str = "claim-timing";

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
        [
            "claims",
            "holiday-calendar",
            "parameters",
            "sla-by-step",
            "step-names",
            "claim-timing",
            "claim-timing-output",
        ]
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
            "from": { "kind": "table", "node": "claims" },
            "fits": [
                { "kind": "table", "node": "claims" },
                { "kind": "table", "node": "holiday-calendar" },
                { "kind": "table", "node": "sla-by-step" },
                { "kind": "table", "node": "step-names" },
            ],
        })
    );
    let output = node(&graph, "claim-timing-output");
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
        json!({ "revision": revision, "edit": { "edit": "add", "kind": "transform", "follows": "claims" } }),
    )
    .expect("an answer");
    assert_eq!(added["graph"]["revision"], revision + 1);
    assert_eq!(added["kind"], "edited");
    assert_eq!(added["added"], "transform");
    assert_eq!(
        node(&added["graph"], "transform")["slots"][0]["from"],
        json!({ "kind": "table", "node": "claims" })
    );
    let workspace = sample();
    let revision = graph_view(&workspace).revision;
    let refused = invoke(
        workspace,
        "edit_graph",
        json!({ "revision": revision, "edit": {
            "edit": "connect", "node": "claims", "slot": { "kind": "start" },
            "from": { "kind": "table", "node": "claim-timing" },
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
        json!({ "revision": 99_999, "edit": { "edit": "remove", "node": "claims" } }),
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
