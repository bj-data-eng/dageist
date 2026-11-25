//! The bridge's request handling, first over in-memory buffers with no socket
//! and then through a socket on a loopback port.
//!
//! The expected views are the snapshots in the view's host package, which the
//! host's Tauri tests hold equal to what the Tauri commands return, so the
//! bridge is compared with the application and not with itself.

use std::fs;
use std::io::{Cursor, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use dageist_engine::SampleSpec;
use dageist_engine::claim_timing::ClaimTiming;
use dageist_engine::document::Document;
use serde_json::{Value, json};

use super::{Bridge, Deadlines, Session, admits, bind, serve, serve_connection};
use crate::dialog::Dialogs;
use crate::graph::Workspace;
use crate::sample_table::Sample;
use crate::test_support::split_block;

const SMALL: SampleSpec = SampleSpec {
    keys: 40,
    filler_columns: 4,
};

/// The port the requests without a socket name in their `Host`.
const PORT: u16 = 7878;

/// A snapshot of what a command returns, from the view's host package.
fn fixture(file: &str) -> Value {
    let path = format!(
        "{}/../../ui/packages/host/fixtures/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
    serde_json::from_str(&text).expect("a snapshot is JSON")
}

struct SampleFolder {
    path: PathBuf,
}

impl SampleFolder {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("dageist-bridge-{}-{name}", std::process::id()));
        Self { path }
    }
}

impl Drop for SampleFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// The starting graph, as the host starts with it, with every revision starting at 0.
fn sample_documents() -> Workspace {
    crate::graph::tests::sample_of(
        Document::read(ClaimTiming::STEPS_JSON).expect("the sample's steps are a document"),
    )
}

fn bridge(folder: &SampleFolder, port: u16) -> Bridge {
    Bridge::new(
        port,
        Session::new(
            Sample::new(folder.path.clone(), SMALL),
            sample_documents(),
            Dialogs::none(),
        ),
    )
}

/// What came back: the status, the head's text and the body.
struct Reply {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Reply {
    fn parse(raw: &[u8]) -> Self {
        let split = raw
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("a head ends in a blank line");
        let head = String::from_utf8(raw[..split].to_vec()).expect("a head is text");
        let status = head
            .split(' ')
            .nth(1)
            .and_then(|code| code.parse().ok())
            .expect("a status line");
        Self {
            status,
            head,
            body: raw[split + 4..].to_vec(),
        }
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).expect("the body is JSON")
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().skip(1).find_map(|line| {
            let (found, value) = line.split_once(": ")?;
            found.eq_ignore_ascii_case(name).then_some(value)
        })
    }

    fn message(&self) -> String {
        self.json()["message"]
            .as_str()
            .expect("a message")
            .to_owned()
    }
}

/// A request as bytes: `method`, `target`, the headers and a body that states its length.
fn raw_request(method: &str, target: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut raw = format!("{method} {target} HTTP/1.1\r\n").into_bytes();
    for (name, value) in headers {
        raw.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    raw.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    raw.extend_from_slice(body);
    raw
}

/// The headers of a well-formed command: the bridge's own `Host` and a JSON body.
fn command_headers(port: u16) -> Vec<(&'static str, String)> {
    vec![
        ("Host", format!("127.0.0.1:{port}")),
        ("Content-Type", "application/json".to_owned()),
    ]
}

fn exchange(bridge: &Bridge, raw: Vec<u8>) -> Reply {
    let mut answered = Vec::new();
    bridge
        .exchange(&mut Cursor::new(raw), &mut answered)
        .expect("the answer is written");
    Reply::parse(&answered)
}

/// Calls the command as a well-formed request does, with `arguments` as its JSON body.
fn call(bridge: &Bridge, command: &str, arguments: &Value) -> Reply {
    // A command on a transform names the starting graph's when the arguments name none.
    let mut arguments = arguments.clone();
    if matches!(
        command,
        "node_inputs" | "node_document" | "edit_document" | "rows_after_step"
    ) && let Some(fields) = arguments.as_object_mut()
    {
        fields
            .entry("node")
            .or_insert_with(|| json!(crate::graph::tests::NODE));
    }
    let headers = command_headers(PORT);
    let headers: Vec<(&str, &str)> = headers
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    exchange(
        bridge,
        raw_request(
            "POST",
            &format!("/bridge/{command}"),
            &headers,
            arguments.to_string().as_bytes(),
        ),
    )
}

#[test]
fn the_commands_answer_the_views_the_application_answers() {
    let folder = SampleFolder::new("views");
    let bridge = bridge(&folder, PORT);

    let reply = call(&bridge, "node_inputs", &json!({}));
    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-type"), Some("application/json"));
    assert_eq!(reply.json(), fixture("node-inputs.json"));

    let reply = call(&bridge, "operation_catalogue", &json!({}));
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json(), fixture("catalogue.json"));

    let reply = call(&bridge, "node_document", &json!({}));
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json(), fixture("node-document.json"));
    assert_eq!(reply.json()["run"], fixture("step-report.json"));

    let reply = call(
        &bridge,
        "rows_after_step",
        &json!({ "revision": 1, "step": "days_open", "limit": 7 }),
    );
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json(), fixture("step-rows.json"));

    let reply = call(&bridge, "core_identity", &json!({}));
    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.json(),
        json!({ "name": "dageist", "version": "0.1.0" })
    );
}

#[test]
fn the_offers_commands_answer_the_views_the_application_answers() {
    let folder = SampleFolder::new("offers");
    let bridge = bridge(&folder, PORT);
    let select = json!([{ "kind": "outer", "name": "select" }]);

    let reply = call(
        &bridge,
        "offers_at",
        &json!({ "positions": select, "after": { "kind": "start" } }),
    );
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json(), fixture("offers-start.json"));

    let reply = call(
        &bridge,
        "offers_at",
        &json!({
            "positions": select,
            "after": { "kind": "value", "dtype": "Datetime(us)", "shape": "many" }
        }),
    );
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json(), fixture("offers-value.json"));

    let reply = call(
        &bridge,
        "candidate_fits",
        &json!({
            "start": null,
            "argument": {
                "entry": "Expr.dt.offset_by",
                "parameter": "by",
                "place": {
                    "positions": select,
                    "after": { "kind": "value", "dtype": "Datetime(us)", "shape": "many" }
                }
            },
            "candidates": [
                { "kind": "field", "dtype": "String" },
                { "kind": "field", "dtype": "Int64" },
                { "kind": "input", "dtype": "String", "list": false }
            ]
        }),
    );
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json(), fixture("fits-argument.json"));

    let reply = call(
        &bridge,
        "read_typed",
        &json!({
            "text": "1.5",
            "typed": { "kind": "expression" },
            "start": null,
            "argument": {
                "entry": "Expr.dt.offset_by",
                "parameter": "by",
                "place": {
                    "positions": select,
                    "after": { "kind": "value", "dtype": "Datetime(us)", "shape": "many" }
                }
            }
        }),
    );
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json(), fixture("typed-expression.json"));

    for (file, typed, text) in [
        (
            "typed-whole.json",
            json!({ "kind": "value", "type": "integer" }),
            "12",
        ),
        (
            "typed-date.json",
            json!({ "kind": "value", "type": "date" }),
            "10-03-2026",
        ),
        ("typed-unread.json", json!({ "kind": "duration" }), "1x"),
    ] {
        let reply = call(
            &bridge,
            "read_typed",
            &json!({ "text": text, "typed": typed, "start": null, "argument": null }),
        );
        assert_eq!(reply.status, 200, "{file}");
        assert_eq!(reply.json(), fixture(file), "{file}");
    }

    let reply = call(
        &bridge,
        "option_offers",
        &json!({
            "entry": "LazyFrame.join",
            "chosen": [{ "parameter": "how", "word": "cross" }]
        }),
    );
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json(), fixture("options.json"));

    let reply = call(
        &bridge,
        "step_starts",
        &json!({
            "columns": ["dcn", "timestamp", "step_to", "owner", "row_days"],
            "tables": ["step_names", "step_sla"]
        }),
    );
    assert_eq!(reply.status, 200);
    assert_eq!(reply.json(), fixture("step-starts.json"));
}

#[test]
fn an_unknown_name_in_an_offers_request_is_refused_with_its_kind() {
    let folder = SampleFolder::new("unknown-name");
    let bridge = bridge(&folder, PORT);

    let reply = call(
        &bridge,
        "option_offers",
        &json!({ "entry": "Expr.nothing", "chosen": [] }),
    );
    assert_eq!(reply.status, 422);
    assert_eq!(reply.json()["kind"], "unknownName");

    // A name the catalogue lacks in the slot a text is typed into is refused by the command.
    let reply = call(
        &bridge,
        "read_typed",
        &json!({
            "text": "1",
            "typed": { "kind": "expression" },
            "start": null,
            "argument": {
                "entry": "Expr.nothing",
                "parameter": "n",
                "place": {
                    "positions": [{ "kind": "outer", "name": "select" }],
                    "after": { "kind": "start" }
                }
            }
        }),
    );
    assert_eq!(reply.status, 422);
    assert_eq!(reply.json()["kind"], "unknownName");

    // A position the tables do not name is not the command's arguments.
    let reply = call(
        &bridge,
        "offers_at",
        &json!({
            "positions": [{ "kind": "outer", "name": "selects" }],
            "after": { "kind": "start" }
        }),
    );
    assert_eq!(reply.status, 400);
}

#[test]
fn a_command_that_fails_answers_its_error_view_with_a_422() {
    let folder = SampleFolder::new("failure");
    let bridge = bridge(&folder, PORT);

    let reply = call(
        &bridge,
        "rows_after_step",
        &json!({ "revision": 1, "step": "days_open", "limit": 1001 }),
    );

    assert_eq!(reply.status, 422);
    assert_eq!(reply.header("content-type"), Some("application/json"));
    assert_eq!(
        reply.json(),
        json!({ "kind": "limit", "message": "a preview holds at most 1000 rows" })
    );
}

#[test]
fn a_step_the_document_lacks_is_refused_with_its_kind() {
    let folder = SampleFolder::new("kind");
    let bridge = bridge(&folder, PORT);

    let reply = call(
        &bridge,
        "rows_after_step",
        &json!({ "revision": 1, "step": "nowhere", "limit": 1 }),
    );

    assert_eq!(reply.status, 422);
    assert_eq!(reply.json()["kind"], "noSuchStep");
    assert!(reply.message().contains("there is no step"));
}

/// The place of the step `index`, called `id`, as an edit names it.
fn step_place(index: usize, id: &str) -> Value {
    json!({ "stepIndex": index, "stepId": id, "path": [] })
}

#[test]
fn an_edit_is_made_on_the_document_the_bridge_holds_and_the_next_request_sees_it() {
    let folder = SampleFolder::new("edits");
    let bridge = bridge(&folder, PORT);

    let reply = call(
        &bridge,
        "edit_document",
        &json!({ "revision": 1, "edit": { "kind": "removeStep", "step": step_place(11, "pivot") } }),
    );

    assert_eq!(reply.status, 200);
    let answer = reply.json();
    assert_eq!(answer["status"], "edited");
    assert_eq!(answer["revision"], 2);
    assert_eq!(
        answer["document"]["steps"].as_array().map(Vec::len),
        Some(11)
    );
    assert_eq!(answer["run"]["steps"].as_array().map(Vec::len), Some(11));
    // The document is kept between requests, and a request for it gives what the edit answered.
    let held = call(&bridge, "node_document", &json!({}));
    assert_eq!(held.status, 200);
    assert_eq!(held.json()["revision"], 2);
    assert_eq!(held.json()["document"], answer["document"]);
    // Rows are asked of a revision: the old one is stale and the new one is the document now.
    let old = call(
        &bridge,
        "rows_after_step",
        &json!({ "revision": 1, "step": "days_open", "limit": 1 }),
    );
    assert_eq!(old.status, 422);
    assert_eq!(old.json()["kind"], "stale");
    let current = call(
        &bridge,
        "rows_after_step",
        &json!({ "revision": 2, "step": "days_open", "limit": 1 }),
    );
    assert_eq!(current.status, 200);
}

#[test]
fn an_edit_the_engine_refuses_is_a_200_that_says_so_and_leaves_the_document() {
    let folder = SampleFolder::new("refused");
    let bridge = bridge(&folder, PORT);

    let reply = call(
        &bridge,
        "edit_document",
        &json!({ "revision": 1, "edit": { "kind": "removeStep", "step": step_place(1, "absent") } }),
    );

    assert_eq!(reply.status, 200);
    let answer = reply.json();
    assert_eq!(answer["status"], "refused");
    assert_eq!(answer["revision"], 1);
    assert_eq!(
        answer["problem"],
        json!({ "kind": "stepChanged", "found": "step_index" })
    );
    assert_eq!(answer["place"], step_place(1, "absent"));
    let held = call(&bridge, "node_document", &json!({}));
    assert_eq!(held.json(), fixture("node-document.json"));
}

#[test]
fn an_edit_made_against_another_revision_is_a_422_that_is_stale_and_changes_nothing() {
    let folder = SampleFolder::new("stale");
    let bridge = bridge(&folder, PORT);

    let reply = call(
        &bridge,
        "edit_document",
        &json!({ "revision": 5, "edit": { "kind": "removeStep", "step": step_place(11, "pivot") } }),
    );

    assert_eq!(reply.status, 422);
    assert_eq!(reply.json()["kind"], "stale");
    assert_eq!(reply.message(), "the document is at revision 1 now, not 5");
    let held = call(&bridge, "node_document", &json!({}));
    assert_eq!(held.json(), fixture("node-document.json"));
}

#[test]
fn a_place_that_names_no_step_is_a_400() {
    let folder = SampleFolder::new("noplace");
    let bridge = bridge(&folder, PORT);

    let reply = call(
        &bridge,
        "edit_document",
        &json!({ "revision": 1, "edit": { "kind": "removeStep", "step": { "path": [] } } }),
    );

    assert_eq!(reply.status, 400);
}

#[test]
fn the_sample_commands_keep_the_open_table_between_requests() {
    let folder = SampleFolder::new("session");
    let bridge = bridge(&folder, PORT);

    let before = call(
        &bridge,
        "table_block",
        &json!({ "offset": 0, "length": 5, "columns": [] }),
    );
    assert_eq!(before.status, 422);
    assert_eq!(
        before.json(),
        json!({ "kind": "failed", "message": "no table is open" })
    );

    let opened = call(&bridge, "open_sample_window", &json!({ "keys": 10 }));
    assert_eq!(opened.status, 200);
    let shape = opened.json();
    assert_eq!(
        shape["columns"].as_array().expect("columns").len(),
        SampleSpec::CLAIM_COLUMNS.len() + 4
    );
    assert_eq!(
        shape["columns"][0],
        json!({ "name": "dcn", "dataType": "str", "numeric": false })
    );

    let block = call(
        &bridge,
        "table_block",
        &json!({ "offset": 3, "length": 7, "columns": ["dcn", "timestamp"] }),
    );
    assert_eq!(block.status, 200);
    assert_eq!(
        block.header("content-type"),
        Some("application/vnd.dageist.block")
    );
    // The block's frame holds the numbers of its rows, then the Arrow IPC stream, which begins
    // with a continuation marker.
    let (header, arrow) = split_block(&block.body);
    assert_eq!(
        header["numbers"],
        json!(["4", "5", "6", "7", "8", "9", "10"])
    );
    assert_eq!(&arrow[..4], &[0xff, 0xff, 0xff, 0xff]);

    let stored = call(&bridge, "open_sample_result", &json!({}));
    assert_eq!(stored.status, 200);
    assert!(stored.json()["rows"].as_u64().expect("rows") >= 80);
    let again = call(
        &bridge,
        "table_block",
        &json!({ "offset": 50, "length": 12, "columns": ["dcn", "measure_002"] }),
    );
    assert_eq!(again.status, 200);
}

#[test]
fn a_window_over_the_limit_and_an_unknown_column_are_refused_by_the_command() {
    let folder = SampleFolder::new("limits");
    let bridge = bridge(&folder, PORT);

    let window = call(&bridge, "open_sample_window", &json!({ "keys": 50_001 }));
    assert_eq!(window.status, 422);
    assert_eq!(
        window.message(),
        "a working window holds at most 50000 keys"
    );

    call(&bridge, "open_sample_window", &json!({ "keys": 5 }));
    let block = call(
        &bridge,
        "table_block",
        &json!({ "offset": 0, "length": 5, "columns": ["no_such_column"] }),
    );
    assert_eq!(block.status, 422);
    assert!(block.message().starts_with("selecting block columns"));
}

#[test]
fn a_request_from_a_host_that_is_not_the_bridges_own_is_refused() {
    let folder = SampleFolder::new("host");
    let bridge = bridge(&folder, PORT);
    let hosts = [
        "evil.example:7878",
        "evil.example",
        "127.0.0.1:7879",
        "127.0.0.1",
        "localhost.evil.example:7878",
        "192.168.1.5:7878",
        "[::1]:7878",
        "",
    ];

    for host in hosts {
        let reply = exchange(
            &bridge,
            raw_request(
                "POST",
                "/bridge/core_identity",
                &[("Host", host), ("Content-Type", "application/json")],
                b"{}",
            ),
        );
        assert_eq!(reply.status, 403, "Host {host:?}");
    }
    let missing = exchange(
        &bridge,
        raw_request(
            "POST",
            "/bridge/core_identity",
            &[("Content-Type", "application/json")],
            b"{}",
        ),
    );
    assert_eq!(missing.status, 403);
}

#[test]
fn the_loopback_address_and_localhost_with_the_bridges_port_are_its_own_host() {
    let folder = SampleFolder::new("own-host");
    let bridge = bridge(&folder, PORT);

    for host in ["127.0.0.1:7878", "localhost:7878", "LocalHost:7878"] {
        let reply = exchange(
            &bridge,
            raw_request(
                "POST",
                "/bridge/core_identity",
                &[("Host", host), ("Content-Type", "application/json")],
                b"{}",
            ),
        );
        assert_eq!(reply.status, 200, "Host {host:?}");
    }
}

#[test]
fn a_body_that_is_not_sent_as_json_is_refused_before_any_command_runs() {
    let folder = SampleFolder::new("content-type");
    let bridge = bridge(&folder, PORT);
    let wrong = [
        Some("text/plain"),
        Some("application/x-www-form-urlencoded"),
        Some("multipart/form-data; boundary=x"),
        Some("application/json-patch+json"),
        Some("application/jsonx"),
        None,
    ];

    for content_type in wrong {
        let mut headers = vec![("Host", "127.0.0.1:7878")];
        if let Some(content_type) = content_type {
            headers.push(("Content-Type", content_type));
        }
        let reply = exchange(
            &bridge,
            raw_request(
                "POST",
                "/bridge/open_sample_window",
                &headers,
                br#"{"keys": 1}"#,
            ),
        );
        assert_eq!(reply.status, 415, "Content-Type {content_type:?}");
    }
    // A command that was refused did not run, so no sample was generated.
    assert!(!SampleFolder::new("content-type").path.exists());
}

#[test]
fn json_with_a_charset_or_another_capitalisation_is_json() {
    let folder = SampleFolder::new("charset");
    let bridge = bridge(&folder, PORT);

    for content_type in ["application/json; charset=utf-8", "Application/JSON"] {
        let reply = exchange(
            &bridge,
            raw_request(
                "POST",
                "/bridge/core_identity",
                &[("Host", "127.0.0.1:7878"), ("Content-Type", content_type)],
                b"{}",
            ),
        );
        assert_eq!(reply.status, 200, "{content_type}");
    }
}

#[test]
fn an_unknown_command_or_path_is_a_404() {
    let folder = SampleFolder::new("unknown");
    let bridge = bridge(&folder, PORT);

    for command in [
        "no_such_command",
        "open_view_window",
        "",
        "core_identity/",
        "../x",
    ] {
        let reply = call(&bridge, command, &json!({}));
        assert_eq!(reply.status, 404, "command {command:?}");
    }
    let outside = exchange(
        &bridge,
        raw_request(
            "POST",
            "/core_identity",
            &[
                ("Host", "127.0.0.1:7878"),
                ("Content-Type", "application/json"),
            ],
            b"{}",
        ),
    );
    assert_eq!(outside.status, 404);
    assert_eq!(
        call(&bridge, "no_such_command", &json!({})).message(),
        "unknown command `no_such_command`"
    );
}

#[test]
fn a_method_other_than_post_is_a_405_that_allows_no_cross_origin_use() {
    let folder = SampleFolder::new("method");
    let bridge = bridge(&folder, PORT);

    for method in ["GET", "OPTIONS", "PUT", "DELETE", "HEAD"] {
        let reply = exchange(
            &bridge,
            raw_request(
                method,
                "/bridge/core_identity",
                &[
                    ("Host", "127.0.0.1:7878"),
                    ("Origin", "https://evil.example"),
                    ("Access-Control-Request-Method", "POST"),
                    ("Access-Control-Request-Headers", "content-type"),
                ],
                b"",
            ),
        );
        assert_eq!(reply.status, 405, "{method}");
        assert_eq!(reply.header("allow"), Some("POST"));
        assert!(
            !reply.head.to_ascii_lowercase().contains("access-control"),
            "{method} answered with a cross-origin header: {}",
            reply.head
        );
    }
}

#[test]
fn no_answer_carries_a_cross_origin_header() {
    let folder = SampleFolder::new("cors");
    let bridge = bridge(&folder, PORT);
    let headers = [
        ("Host", "127.0.0.1:7878"),
        ("Content-Type", "application/json"),
        ("Origin", "https://evil.example"),
    ];

    let answered = exchange(
        &bridge,
        raw_request("POST", "/bridge/core_identity", &headers, b"{}"),
    );
    let refused = exchange(
        &bridge,
        raw_request("POST", "/bridge/no_such_command", &headers, b"{}"),
    );

    for reply in [answered, refused] {
        assert!(!reply.head.to_ascii_lowercase().contains("access-control"));
    }
}

#[test]
fn a_body_that_is_not_the_commands_arguments_is_a_400() {
    let folder = SampleFolder::new("malformed");
    let bridge = bridge(&folder, PORT);
    let bodies: [(&str, &[u8]); 9] = [
        ("edit_document", b"{not json"),
        ("edit_document", b""),
        ("edit_document", b"[]"),
        ("edit_document", br#"{"revision": 5}"#),
        (
            "edit_document",
            br#"{"revision": 1, "edit": {"kind": "removeStep", "step": {"stepIndex": 0, "stepId": null, "path": []}}, "extra": true}"#,
        ),
        (
            "edit_document",
            br#"{"revision": 1, "edit": {"kind": "levitate"}}"#,
        ),
        (
            "rows_after_step",
            br#"{"revision": 1, "step": "a", "limit": -1}"#,
        ),
        ("table_block", br#"{"offset": 0, "length": 5}"#),
        ("core_identity", br#"{"unexpected": 1}"#),
    ];

    for (command, body) in bodies {
        let reply = exchange(
            &bridge,
            raw_request(
                "POST",
                &format!("/bridge/{command}"),
                &[
                    ("Host", "127.0.0.1:7878"),
                    ("Content-Type", "application/json"),
                ],
                body,
            ),
        );
        assert_eq!(
            reply.status,
            400,
            "{command} {}",
            String::from_utf8_lossy(body)
        );
        assert!(reply.message().starts_with("the arguments are not valid"));
    }
}

#[test]
fn what_is_not_a_request_is_answered_or_dropped_without_running_a_command() {
    let folder = SampleFolder::new("framing");
    let bridge = bridge(&folder, PORT);

    let garbage = exchange(&bridge, b"hello\r\n\r\n".to_vec());
    assert_eq!(garbage.status, 400);

    let chunked = exchange(
        &bridge,
        b"POST /bridge/core_identity HTTP/1.1\r\nHost: 127.0.0.1:7878\r\n\
          Content-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
            .to_vec(),
    );
    assert_eq!(chunked.status, 501);

    let huge = exchange(
        &bridge,
        b"POST /bridge/edit_document HTTP/1.1\r\nHost: 127.0.0.1:7878\r\n\
          Content-Type: application/json\r\nContent-Length: 9000000\r\n\r\n"
            .to_vec(),
    );
    assert_eq!(huge.status, 413);

    let mut silent = Vec::new();
    bridge
        .exchange(&mut Cursor::new(Vec::new()), &mut silent)
        .expect("nothing to write");
    assert!(
        silent.is_empty(),
        "a connection that sent nothing gets no answer"
    );
}

#[test]
fn only_a_loopback_peer_is_served() {
    assert!(admits(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    assert!(admits(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2))));
    assert!(admits(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    for peer in [
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)),
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6("fe80::1".parse().expect("an address")),
    ] {
        assert!(!admits(peer), "{peer}");
    }
}

/// Sends `raw` to `address` and reads everything the bridge answers until it closes.
fn over_the_socket(address: SocketAddr, raw: &[u8]) -> Reply {
    let mut stream = TcpStream::connect(address).expect("the bridge accepts");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(60)))
        .expect("a timeout is set");
    stream.write_all(raw).expect("the request is sent");
    let mut answered = Vec::new();
    stream
        .read_to_end(&mut answered)
        .expect("the answer is read");
    Reply::parse(&answered)
}

#[test]
fn through_a_socket_the_bridge_answers_a_command_and_refuses_a_wrong_host_and_type() {
    let folder = SampleFolder::new("socket");
    let listener_port = {
        // The bridge must name its own port in the Host, so bind first and build it after.
        let listener = bind(0).expect("a loopback port is free");
        let address = listener.local_addr().expect("a bound address");
        let bridge = bridge(&folder, address.port());
        thread::spawn(move || {
            let _ = serve(&listener, bridge);
        });
        address
    };
    assert_eq!(listener_port.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    let own_host = format!("127.0.0.1:{}", listener_port.port());

    let answered = over_the_socket(
        listener_port,
        &raw_request(
            "POST",
            "/bridge/node_inputs",
            &[("Host", &own_host), ("Content-Type", "application/json")],
            br#"{"node": "00000000-0000-7000-8000-000000000006"}"#,
        ),
    );
    assert_eq!(answered.status, 200);
    assert_eq!(answered.json(), fixture("node-inputs.json"));

    let rebound = over_the_socket(
        listener_port,
        &raw_request(
            "POST",
            "/bridge/node_inputs",
            &[
                ("Host", &format!("rebind.example:{}", listener_port.port())),
                ("Content-Type", "application/json"),
            ],
            b"{}",
        ),
    );
    assert_eq!(rebound.status, 403);

    let form = over_the_socket(
        listener_port,
        &raw_request(
            "POST",
            "/bridge/node_inputs",
            &[("Host", &own_host), ("Content-Type", "text/plain")],
            b"{}",
        ),
    );
    assert_eq!(form.status, 415);

    let unknown = over_the_socket(
        listener_port,
        &raw_request(
            "POST",
            "/bridge/no_such_command",
            &[("Host", &own_host), ("Content-Type", "application/json")],
            b"{}",
        ),
    );
    assert_eq!(unknown.status, 404);

    let malformed = over_the_socket(
        listener_port,
        &raw_request(
            "POST",
            "/bridge/edit_document",
            &[("Host", &own_host), ("Content-Type", "application/json")],
            b"{not json",
        ),
    );
    assert_eq!(malformed.status, 400);
}

#[test]
fn through_a_socket_the_open_table_is_kept_across_connections() {
    let folder = SampleFolder::new("socket-state");
    let listener = bind(0).expect("a loopback port is free");
    let address = listener.local_addr().expect("a bound address");
    let bridge = bridge(&folder, address.port());
    thread::spawn(move || {
        let _ = serve(&listener, bridge);
    });
    let own_host = format!("127.0.0.1:{}", address.port());
    let headers = [
        ("Host", own_host.as_str()),
        ("Content-Type", "application/json"),
    ];

    let opened = over_the_socket(
        address,
        &raw_request(
            "POST",
            "/bridge/open_sample_window",
            &headers,
            br#"{"keys": 10}"#,
        ),
    );
    assert_eq!(opened.status, 200);
    let block = over_the_socket(
        address,
        &raw_request(
            "POST",
            "/bridge/table_block",
            &headers,
            br#"{"offset": 0, "length": 3, "columns": ["dcn"]}"#,
        ),
    );

    assert_eq!(block.status, 200);
    let (header, arrow) = split_block(&block.body);
    assert_eq!(header["numbers"], json!(["1", "2", "3"]));
    assert_eq!(&arrow[..4], &[0xff, 0xff, 0xff, 0xff]);
}

#[test]
fn the_bridge_listens_on_the_loopback_address_only() {
    let listener = bind(0).expect("a loopback port is free");
    let address = listener.local_addr().expect("a bound address");
    assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    thread::spawn(move || {
        let _ = serve(
            &listener,
            Bridge::new(
                address.port(),
                Session::new(
                    Sample::new(std::env::temp_dir(), SMALL),
                    sample_documents(),
                    Dialogs::none(),
                ),
            ),
        );
    });

    // The address this machine uses to reach other hosts, found by asking the system which
    // interface a route would use. A UDP socket sends nothing until it is written to.
    let own_address = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|socket| {
            socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9))?;
            socket.local_addr()
        })
        .map(|local| local.ip());
    if let Some(ip) = own_address
        .ok()
        .filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
    {
        let attempt = TcpStream::connect_timeout(
            &SocketAddr::new(ip, address.port()),
            std::time::Duration::from_secs(2),
        );
        assert!(
            attempt.is_err(),
            "the bridge accepted a connection on {ip}, which is not the loopback address"
        );
    }
}

#[test]
fn the_cache_folder_is_the_one_tauri_keeps_for_the_identifier() {
    use tauri::Manager;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("the mock application builds");
    let identifier = app.config().identifier.clone();

    let tauri_cache = app.path().app_cache_dir().expect("Tauri knows its cache");

    assert_eq!(
        super::cache::application_cache(&identifier),
        Some(tauri_cache)
    );
}

#[test]
fn a_client_that_trickles_its_request_is_answered_408_when_the_deadline_passes() {
    let folder = SampleFolder::new("trickle");
    let listener = bind(0).expect("a loopback port is free");
    let address = listener.local_addr().expect("a bound address");
    let bridge = bridge(&folder, address.port());
    let deadlines = Deadlines {
        request: Duration::from_millis(300),
        response: Duration::from_secs(5),
        drain: Duration::from_secs(1),
    };
    let server = thread::spawn(move || {
        let (stream, _peer) = listener.accept().expect("the client connects");
        serve_connection(&stream, &bridge, deadlines)
    });
    let mut client = TcpStream::connect(address).expect("the bridge accepts");
    client
        .set_read_timeout(Some(Duration::from_secs(60)))
        .expect("a timeout is set");
    let mut trickling = client.try_clone().expect("the stream is cloned");
    let head = format!(
        "POST /bridge/core_identity HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-Slow: ",
        address.port()
    );
    let started = Instant::now();
    let trickler = thread::spawn(move || {
        trickling
            .write_all(head.as_bytes())
            .expect("the head begins");
        for _ in 0..100 {
            // The time under test: a byte arrives far inside the per-read timeout and the head
            // never ends, so only a deadline on the whole request can stop it.
            thread::sleep(Duration::from_millis(100));
            if trickling.write_all(b"a").is_err() {
                break;
            }
        }
    });

    let mut answered = Vec::new();
    client
        .read_to_end(&mut answered)
        .expect("the answer is read");
    let elapsed = started.elapsed();

    assert_eq!(Reply::parse(&answered).status, 408);
    assert!(
        elapsed < Duration::from_secs(5),
        "the connection was held for {elapsed:?}"
    );
    server
        .join()
        .expect("the connection ends")
        .expect("the connection is served");
    trickler.join().expect("the trickle ends");
}
