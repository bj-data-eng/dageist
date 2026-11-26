//! The graph commands through Tauri's IPC layer and the snapshot of the graph view, on the
//! timing corpus in `tests/corpus/timing`: a graph of four tables, two parameters, a transform and
//! an output, as a workspace's files. The expected values are written out by hand from that
//! graph.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use dageist::{Bindings, Session};
use dageist_engine::document::Document;
use dageist_graph::NodeId;
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};

use super::{Workspace, graph_view};

/// Set to rewrite the snapshot instead of checking it.
const REFRESH: &str = "DAGEIST_REFRESH_GRAPH";

/// The command that refreshes the snapshot.
const REFRESH_COMMAND: &str = "cargo test --offline --locked -p dageist-tauri graph::tests::the_snapshot_is_what_the_command_returns";

/// The ids of the corpus's nodes, in the order of their ids, as the view sends them.
const CLAIMS: &str = "00000000-0000-7000-8000-000000000001";
const SLA: &str = "00000000-0000-7000-8000-000000000002";
const NAMES: &str = "00000000-0000-7000-8000-000000000003";
const CALENDAR: &str = "00000000-0000-7000-8000-000000000004";
const PARAMETERS: &str = "00000000-0000-7000-8000-000000000005";
/// The id of the corpus's transform.
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

/// The host's timing corpus: a workspace's files, with its tables under `data/`.
fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
        .join("timing")
}

/// The id of the corpus's transform.
pub(crate) fn starting_transform() -> NodeId {
    NodeId::new(NODE).expect("an id")
}

/// The corpus's steps, as the text of its file.
pub(crate) fn corpus_steps() -> String {
    std::fs::read_to_string(corpus().join("nodes").join(NODE).join("steps.json"))
        .expect("the corpus's steps")
}

/// The corpus made a workspace in `root`, and the session it holds, with every revision
/// starting at `first_revision`.
fn corpus_in(root: &Path, first_revision: u64) -> (dageist::workspace::Workspace, Session) {
    let (kept, saved) = dageist::workspace::Workspace::create_from_folder(
        &corpus(),
        root,
        dageist::unsigned_author(),
        dageist::workspace::Options::default(),
    )
    .expect("the corpus is a workspace");
    let session = Session::from_saved(saved, first_revision).expect("the corpus's steps read");
    (kept, session)
}

/// The corpus kept in a workspace on disk in `workspace` under `folder`, as the application keeps
/// the workspace it opens, with every revision starting at `first_revision`.
pub(crate) fn kept_corpus(folder: &Path, first_revision: u64) -> Workspace {
    let (kept, session) = corpus_in(&folder.join("workspace"), first_revision);
    Workspace::kept(session, kept)
}

/// What the corpus holds, made a workspace once in each run of the tests, in a folder the run's
/// tests share: they read its tables and write nothing there.
fn shared_corpus() -> &'static dageist::workspace::Saved {
    static SHARED: std::sync::OnceLock<dageist::workspace::Saved> = std::sync::OnceLock::new();
    SHARED.get_or_init(|| {
        let root =
            std::env::temp_dir().join(format!("dageist-graph-corpus-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (_, saved) = dageist::workspace::Workspace::create_from_folder(
            &corpus(),
            &root,
            dageist::unsigned_author(),
            dageist::workspace::Options::default(),
        )
        .expect("the corpus is a workspace");
        saved
    })
}

/// The corpus with `steps` as its transform's document, saved nowhere. Every revision starts at
/// 0, and the document, set once, is at revision 1.
pub(crate) fn sample_of(steps: Document) -> Workspace {
    let mut session =
        Session::from_saved(shared_corpus().clone(), 0).expect("the corpus's steps read");
    session
        .edit_document(&starting_transform(), 0, |_| Ok::<_, ()>(steps))
        .expect("the transform's document")
        .expect("set");
    Workspace::of(session)
}

/// The corpus with its own steps.
pub(crate) fn sample() -> Workspace {
    sample_of(Document::read(&corpus_steps()).expect("the corpus's steps"))
}

/// What the corpus's transform runs on.
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

/// Runs git in `repository` and gives what it printed.
fn git(repository: &std::path::Path, arguments: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .expect("git runs; this test checks the workspace with git and needs it on the path");
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("text")
}

#[test]
fn every_edit_the_host_makes_is_saved_as_a_commit_and_the_workspace_opens_as_it_was() {
    let folder = folder();
    let workspace = kept_corpus(&folder, 1);
    let kept = folder.join("workspace");
    let subjects = || git(&kept, &["log", "--format=%s"]);
    assert_eq!(subjects(), "Make the workspace\n");
    let rename: super::GraphEditRequest = serde_json::from_value(
        json!({ "edit": "rename", "node": NODE, "name": "Document timing" }),
    )
    .expect("a request");
    let revision = graph_view(&workspace).revision;
    super::edit_graph_view(&workspace, revision, &rename).expect("renamed");
    let layout = graph_view(&workspace).layout_revision;
    let frame = super::FrameView {
        x: 40,
        y: -20,
        width: None,
        height: None,
    };
    super::place_view(&workspace, layout, NODE, frame).expect("placed");
    // A refused edit makes no commit.
    let refused: super::GraphEditRequest =
        serde_json::from_value(json!({ "edit": "rename", "node": NODE, "name": "Claims" }))
            .expect("a request");
    let revision = graph_view(&workspace).revision;
    super::edit_graph_view(&workspace, revision, &refused).expect("an answer");
    // An edit of the steps is saved, and one against another revision of them is not.
    let remove: crate::document::EditRequest = serde_json::from_value(json!({
        "kind": "removeStep",
        "step": { "stepIndex": 0, "stepId": "sort", "path": [] },
    }))
    .expect("a request");
    crate::document::edit_view(&workspace, NODE, 7, &remove).expect_err("stale");
    crate::document::edit_view(&workspace, NODE, 1, &remove).expect("removed");
    assert_eq!(
        subjects(),
        "Change the steps of Document timing\nMove Document timing\n\
         Rename Claim timing to Document timing\nMake the workspace\n"
    );
    let steps =
        std::fs::read_to_string(kept.join(format!("nodes/{NODE}/steps.json"))).expect("the steps");
    assert!(!steps.contains("\"sort\""), "{steps}");
    assert_eq!(
        git(&kept, &["log", "-1", "--format=%an <%ae>"]).trim(),
        "dageist <workspace@dageist.invalid>"
    );
    assert_eq!(git(&kept, &["status", "--porcelain"]), "");
    let held = workspace.lock().saved();
    workspace.close().expect("closed");
    // An edit made once the workspace is closed is refused its save, and the session holds it.
    let late: super::GraphEditRequest =
        serde_json::from_value(json!({ "edit": "rename", "node": NODE, "name": "Late" }))
            .expect("a request");
    let revision = graph_view(&workspace).revision;
    super::edit_graph_view(&workspace, revision, &late).expect_err("not saved");
    assert_eq!(
        workspace
            .lock()
            .graph()
            .node(&starting_transform())
            .expect("the transform")
            .name()
            .as_str(),
        "Late"
    );
    let (_, saved) = dageist::workspace::Workspace::open(
        &kept,
        dageist::unsigned_author(),
        dageist::workspace::Options::default(),
    )
    .expect("opens");
    let _ = std::fs::remove_dir_all(&folder);
    assert_eq!(saved, held);
}

/// Makes a workspace on disk in `folder` holding one source, `Elsewhere`.
pub(crate) fn other_workspace(folder: &std::path::Path) -> dageist_graph::NodeId {
    let mut session = dageist::Session::new(0);
    let id = dageist_graph::NodeId::mint();
    session
        .edit_graph(
            0,
            &dageist_graph::Edit::Add {
                id: id.clone(),
                kind: dageist_graph::Kind::Source,
                follows: None,
                name: Some(dageist_graph::Name::new("Elsewhere").expect("a name")),
            },
        )
        .expect("at its revision")
        .expect("added");
    dageist::workspace::Workspace::create(
        folder,
        &session.saved(),
        dageist::unsigned_author(),
        dageist::workspace::Options::default(),
    )
    .expect("made")
    .close()
    .expect("closed");
    id
}

#[test]
fn opening_a_workspace_holds_its_graph_saves_edits_to_it_and_packs_the_one_let_go() {
    let folder = folder();
    let workspace = kept_corpus(&folder, 1);
    let first = folder.join("workspace");
    let other = folder.join("other");
    let elsewhere = other_workspace(&other);
    let before = graph_view(&workspace);

    let opened =
        super::open_workspace_view(&workspace, other.to_str().expect("a path")).expect("opened");
    let nodes: Vec<(String, String)> = serde_json::to_value(&opened).expect("a view")["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|node| {
            (
                node["id"].as_str().expect("an id").to_owned(),
                node["name"].as_str().expect("a name").to_owned(),
            )
        })
        .collect();
    assert_eq!(nodes, [(elsewhere.to_string(), "Elsewhere".to_owned())]);
    // A view of the workspace let go is stale against the one opened.
    assert_ne!(opened.revision, before.revision);
    let stale: super::GraphEditRequest =
        serde_json::from_value(json!({ "edit": "remove", "node": NODE })).expect("a request");
    let refused = super::edit_graph_view(&workspace, before.revision, &stale).expect_err("stale");
    assert_eq!(
        serde_json::to_value(&refused).expect("a view")["kind"],
        "stale"
    );

    let rename: super::GraphEditRequest = serde_json::from_value(
        json!({ "edit": "rename", "node": elsewhere.to_string(), "name": "Here" }),
    )
    .expect("a request");
    super::edit_graph_view(&workspace, opened.revision, &rename).expect("renamed");
    assert_eq!(
        git(&other, &["log", "--format=%s"]),
        "Rename Elsewhere to Here\nMake the workspace\n"
    );
    // The workspace let go took no edit, and was packed whole as it was closed.
    assert_eq!(git(&first, &["log", "--format=%s"]), "Make the workspace\n");
    let counted = git(&first, &["count-objects", "-v"]);
    assert!(counted.contains("count: 0\n"), "{counted}");
    assert!(counted.contains("packs: 1\n"), "{counted}");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn no_workspace_is_opened_once_the_one_held_is_closed() {
    let folder = folder();
    let workspace = kept_corpus(&folder, 1);
    let other = folder.join("other");
    other_workspace(&other);
    workspace.close().expect("closed");
    let refused = super::open_workspace_view(&workspace, other.to_str().expect("a path"))
        .expect_err("closing");
    assert!(
        serde_json::to_value(&refused).expect("a view")["message"]
            .as_str()
            .is_some_and(|message| message.contains("closing")),
        "{refused:?}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_folder_that_holds_no_workspace_is_not_opened_and_leaves_the_one_held() {
    let folder = folder();
    let workspace = kept_corpus(&folder, 1);
    let empty = folder.join("empty");
    std::fs::create_dir_all(&empty).expect("a folder");
    let before = serde_json::to_value(graph_view(&workspace)).expect("a view");
    super::open_workspace_view(&workspace, empty.to_str().expect("a path"))
        .expect_err("no workspace");
    assert_eq!(
        serde_json::to_value(graph_view(&workspace)).expect("a view"),
        before
    );
    // The workspace held still takes edits.
    let rename: super::GraphEditRequest =
        serde_json::from_value(json!({ "edit": "rename", "node": NODE, "name": "Still here" }))
            .expect("a request");
    let revision = graph_view(&workspace).revision;
    super::edit_graph_view(&workspace, revision, &rename).expect("renamed");
    assert_eq!(
        git(&folder.join("workspace"), &["log", "-1", "--format=%s"]).trim(),
        "Rename Claim timing to Still here"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

/// The names of the nodes of `view`, in its order.
fn names(view: &Value) -> Vec<String> {
    view["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|node| node["name"].as_str().expect("a name").to_owned())
        .collect()
}

#[test]
fn the_application_starts_on_the_sample_workspace_and_keeps_it_from_one_start_to_the_next() {
    let data = folder();
    let first = Workspace::sample(&data, 0).expect("the sample");
    let root = data.join("samples").join("orders");
    let view = serde_json::to_value(graph_view(&first)).expect("a view");
    let mut shown = names(&view);
    shown.sort();
    assert_eq!(
        shown,
        [
            "Categories",
            "Category names",
            "Customer lines",
            "Customers",
            "Order lines",
            "Orders",
            "Parameters",
            "Products",
            "Region summary",
            "Region summary output",
        ]
    );
    for node in view["nodes"].as_array().expect("nodes") {
        if node["kind"] != "output" {
            assert_eq!(node["problems"], json!([]), "{}", node["name"]);
        }
    }
    // The data was unpacked beside the graph, where git does not see it.
    for file in [
        "orders.parquet",
        "products.parquet",
        "customers.csv",
        "categories.csv",
    ] {
        assert!(root.join("data").join(file).is_file(), "{file}");
    }
    assert_eq!(git(&root, &["log", "--format=%s"]), "Make the workspace\n");
    assert_eq!(git(&root, &["status", "--porcelain"]), "");

    let summary = view["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["name"] == "Region summary")
        .expect("the summary")["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let rename: super::GraphEditRequest = serde_json::from_value(
        json!({ "edit": "rename", "node": summary, "name": "Totals by region" }),
    )
    .expect("a request");
    super::edit_graph_view(&first, graph_view(&first).revision, &rename).expect("renamed");
    first.close().expect("closed");

    // The next start opens the sample as it was left, and makes no new one.
    let second = Workspace::sample(&data, 1).expect("the sample again");
    let again = names(&serde_json::to_value(graph_view(&second)).expect("a view"));
    assert!(again.contains(&"Totals by region".to_owned()), "{again:?}");
    assert_eq!(
        git(&root, &["log", "--format=%s"]),
        "Rename Region summary to Totals by region\nMake the workspace\n"
    );
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn a_sample_folder_with_files_but_no_workspace_is_not_overwritten() {
    let data = folder();
    let root = data.join("samples").join("orders");
    std::fs::create_dir_all(&root).expect("a folder");
    std::fs::write(root.join("notes.txt"), b"mine").expect("written");
    Workspace::sample(&data, 0).expect_err("not a workspace");
    assert_eq!(
        std::fs::read(root.join("notes.txt")).expect("kept"),
        b"mine"
    );
    let left: Vec<_> = std::fs::read_dir(data.join("samples"))
        .expect("the folder")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    assert_eq!(left, ["orders"], "the files unpacked beside it are removed");
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn two_starts_that_unpack_the_sample_at_once_leave_one_whole_sample() {
    let data = folder();
    let root = crate::sample_workspace::sample_root(&data);
    // An empty folder in the sample's place is no sample.
    std::fs::create_dir_all(&root).expect("a folder");
    let starts: Vec<_> = (0..4)
        .map(|_| {
            let root = root.clone();
            std::thread::spawn(move || crate::sample_workspace::unpack(&root))
        })
        .collect();
    for start in starts {
        start.join().expect("the start ends").expect("unpacked");
    }
    let sample = Workspace::sample(&data, 0).expect("the sample opens");
    assert_eq!(git(&root, &["log", "--format=%s"]), "Make the workspace\n");
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    let left: Vec<_> = std::fs::read_dir(data.join("samples"))
        .expect("the folder")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    assert_eq!(left, ["orders"], "what each start unpacked into is removed");
    drop(sample);
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn a_second_host_on_the_sample_is_refused_while_the_first_holds_it() {
    let data = folder();
    let first = Workspace::sample(&data, 0).expect("the sample");
    let refused = Workspace::sample(&data, 1).expect_err("held");
    let message = serde_json::to_value(&refused).expect("a view")["message"]
        .as_str()
        .expect("a message")
        .to_owned();
    assert!(message.contains("open in another dageist"), "{message}");
    // Opening the workspace held, in the host that holds it, keeps it as it is.
    let root = crate::sample_workspace::sample_root(&data);
    let before = graph_view(&first).revision;
    first.open(&root, 99).expect("the same workspace");
    assert_eq!(graph_view(&first).revision, before);
    first.close().expect("closed");
    Workspace::sample(&data, 2).expect("the sample once it is closed");
    let _ = std::fs::remove_dir_all(&data);
}

// A folder is opened to set its time, which Unix allows.
#[cfg(unix)]
#[test]
fn what_a_start_that_stopped_left_beside_the_sample_is_removed_once_it_is_old() {
    let data = folder();
    let samples = data.join("samples");
    let left = samples.join(".unpacking-files-1-1");
    let recent = samples.join(".unpacking-files-2-2");
    for folder in [&left, &recent] {
        std::fs::create_dir_all(folder).expect("a folder");
    }
    // A folder's age is its last change: the one left was changed two hours ago.
    let two_hours_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 60 * 60);
    std::fs::File::open(&left)
        .expect("the folder")
        .set_modified(two_hours_ago)
        .expect("aged");
    Workspace::sample(&data, 0).expect("the sample");
    assert!(!left.exists(), "the old folder is removed");
    assert!(recent.exists(), "a recent one may be another start's");
    let _ = std::fs::remove_dir_all(&data);
}
