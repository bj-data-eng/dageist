//! The bridge: the host's commands served over HTTP to a browser.
//!
//! In a plain browser the view has no Tauri to call, so `dageist bridge` serves
//! the same commands on the loopback address. It answers with the same
//! functions the Tauri commands call, and holds the state those commands keep
//! (the sample gate and the open table) for as long as it runs, so a view in a
//! browser sees what the application shows. It opens no window and starts no
//! Tauri.
//!
//! Each command is a `POST` to `/bridge/<command>` of the command's arguments
//! as a JSON object. It answers `200` with the command's JSON, or with the bytes
//! of an Arrow IPC stream for `table_block`, and `422` with the command's own
//! `{ "kind": string, "message": string }` when the command fails, the kind being
//! one of the closed list the view decides on. A request that is not a
//! command of the bridge is refused before any command runs: `403` for a peer or
//! `Host` that is not the bridge's own loopback address and port, `404` for a
//! path that names no command, `405` for another method, `415` for a body that
//! is not `application/json`, and `400` for a body that is not the command's
//! arguments. No answer carries a cross-origin header and no preflight is
//! answered, so a page from another site cannot drive the bridge.
//!
//! The bridge is a development tool for the browser preview, run from
//! `cargo run -- bridge`. It has no access control beyond being local, and no
//! command in it is meant to be exposed further.

mod cache;
mod http;
#[cfg(test)]
mod tests;

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::catalogue::english_catalogue;
use crate::identity::identity_view;
use crate::node::{node_view, rows_view, run_view};
use crate::offers::{
    AfterRequest, ArgumentRequest, CandidateRequest, ChosenOptionRequest, PositionRequest,
    TypedRequest, fits_view, offers_view, options_view, read_view, step_starts_view,
};
use crate::sample_table::{
    ErrorView, OpenTable, Sample, SampleGate, sample_result_view, sample_window_view,
    table_block_bytes,
};
use http::{ReadError, Request, Response, Status};

/// The path every command is under.
const COMMAND_PATH: &str = "/bridge/";
/// The content type the arguments of a command must be sent as.
const JSON: &str = "application/json";
/// The content type of the bytes of an Arrow IPC stream.
const ARROW: &str = "application/vnd.apache.arrow.stream";
/// How long a client may take between the bytes of a request.
const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a client may take to take a part of an answer.
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);
/// How long each part of one connection may take in all, so that a client that sends or takes
/// a byte before every timeout cannot hold its thread for as long as it likes.
const DEADLINES: Deadlines = Deadlines {
    request: Duration::from_secs(30),
    response: Duration::from_secs(60),
    drain: Duration::from_secs(5),
};
/// The most a client may still send after it was answered, which is read and dropped so that the
/// answer is not lost to a reset of the connection.
const MAX_DRAIN_BYTES: u64 = 1024 * 1024;

/// What the commands keep between calls, which the Tauri commands keep as managed state.
#[derive(Debug)]
pub struct Session {
    sample: Sample,
    gate: SampleGate,
    open: OpenTable,
}

impl Session {
    /// A session with no table open, over the sample table `sample`.
    pub fn new(sample: Sample) -> Self {
        Self {
            sample,
            gate: SampleGate::default(),
            open: OpenTable::default(),
        }
    }
}

/// The arguments of a command that takes none: an empty object.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArguments {}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RunArguments {
    document: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RowsArguments {
    document: String,
    step: String,
    limit: usize,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OffersArguments {
    positions: Vec<PositionRequest>,
    after: AfterRequest,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FitsArguments {
    start: Option<Vec<PositionRequest>>,
    argument: Option<ArgumentRequest>,
    candidates: Vec<CandidateRequest>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TypedArguments {
    text: String,
    typed: TypedRequest,
    start: Option<Vec<PositionRequest>>,
    argument: Option<ArgumentRequest>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OptionsArguments {
    entry: String,
    chosen: Vec<ChosenOptionRequest>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StartsArguments {
    columns: Vec<String>,
    tables: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowArguments {
    keys: u32,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockArguments {
    offset: usize,
    length: usize,
    columns: Vec<String>,
}

/// Why a command did not answer.
enum Failure {
    /// The body is not the command's arguments.
    Arguments(String),
    /// The command ran and failed.
    Refused(ErrorView),
}

impl From<ErrorView> for Failure {
    fn from(error: ErrorView) -> Self {
        Self::Refused(error)
    }
}

/// What a command answered with.
enum Answer {
    Json(Vec<u8>),
    Arrow(Vec<u8>),
}

/// Reads `body` as the arguments `A` of a command.
fn arguments<A: DeserializeOwned>(body: &[u8]) -> Result<A, Failure> {
    serde_json::from_slice(body)
        .map_err(|error| Failure::Arguments(format!("the arguments are not valid: {error}")))
}

/// Answers with `view` as JSON.
fn json(view: &impl Serialize) -> Result<Answer, Failure> {
    serde_json::to_vec(view)
        .map(Answer::Json)
        .map_err(|error| ErrorView::new(format!("writing the answer: {error}")).into())
}

/// A command of the view the bridge serves. A window command is the application's own, so
/// the bridge has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    CoreIdentity,
    OperationCatalogue,
    NodeInputs,
    RunDocument,
    RowsAfterStep,
    OffersAt,
    CandidateFits,
    ReadTyped,
    OptionOffers,
    StepStarts,
    OpenSampleWindow,
    OpenSampleResult,
    TableBlock,
}

impl Command {
    /// The command called `name`, by the name Tauri registers it under.
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "core_identity" => Self::CoreIdentity,
            "operation_catalogue" => Self::OperationCatalogue,
            "node_inputs" => Self::NodeInputs,
            "run_document" => Self::RunDocument,
            "rows_after_step" => Self::RowsAfterStep,
            "offers_at" => Self::OffersAt,
            "candidate_fits" => Self::CandidateFits,
            "read_typed" => Self::ReadTyped,
            "option_offers" => Self::OptionOffers,
            "step_starts" => Self::StepStarts,
            "open_sample_window" => Self::OpenSampleWindow,
            "open_sample_result" => Self::OpenSampleResult,
            "table_block" => Self::TableBlock,
            _ => return None,
        })
    }

    /// Runs the command on `arguments`, the JSON the view sent, through the function the Tauri
    /// command calls.
    fn run(self, session: &Session, body: &[u8]) -> Result<Answer, Failure> {
        match self {
            Self::CoreIdentity => {
                arguments::<NoArguments>(body)?;
                json(&identity_view())
            }
            Self::OperationCatalogue => {
                arguments::<NoArguments>(body)?;
                json(&english_catalogue()?)
            }
            Self::NodeInputs => {
                arguments::<NoArguments>(body)?;
                json(&node_view()?)
            }
            Self::RunDocument => {
                let RunArguments { document } = arguments(body)?;
                json(&run_view(&document)?)
            }
            Self::RowsAfterStep => {
                let RowsArguments {
                    document,
                    step,
                    limit,
                } = arguments(body)?;
                json(&rows_view(&document, &step, limit)?)
            }
            Self::OffersAt => {
                let OffersArguments { positions, after } = arguments(body)?;
                json(&offers_view(&positions, after)?)
            }
            Self::CandidateFits => {
                let FitsArguments {
                    start,
                    argument,
                    candidates,
                } = arguments(body)?;
                json(&fits_view(
                    start.as_deref(),
                    argument.as_ref(),
                    &candidates,
                )?)
            }
            Self::ReadTyped => {
                let TypedArguments {
                    text,
                    typed,
                    start,
                    argument,
                } = arguments(body)?;
                json(&read_view(
                    &text,
                    typed,
                    start.as_deref(),
                    argument.as_ref(),
                )?)
            }
            Self::OptionOffers => {
                let OptionsArguments { entry, chosen } = arguments(body)?;
                json(&options_view(&entry, &chosen)?)
            }
            Self::StepStarts => {
                let StartsArguments { columns, tables } = arguments(body)?;
                json(&step_starts_view(&columns, &tables))
            }
            Self::OpenSampleWindow => {
                let WindowArguments { keys } = arguments(body)?;
                json(&sample_window_view(
                    &session.gate,
                    &session.open,
                    &session.sample,
                    keys,
                )?)
            }
            Self::OpenSampleResult => {
                arguments::<NoArguments>(body)?;
                json(&sample_result_view(
                    &session.gate,
                    &session.open,
                    &session.sample,
                )?)
            }
            Self::TableBlock => {
                let BlockArguments {
                    offset,
                    length,
                    columns,
                } = arguments(body)?;
                Ok(Answer::Arrow(table_block_bytes(
                    &session.open,
                    offset,
                    length,
                    &columns,
                )?))
            }
        }
    }
}

/// A response of `status` that carries `error` as a command's error is carried.
fn failure(status: Status, error: &ErrorView) -> Response {
    let body = serde_json::to_vec(error)
        .unwrap_or_else(|_unwritable| br#"{"kind":"failed","message":"refused"}"#.to_vec());
    Response::new(status, JSON, body)
}

/// A response that says why the request was refused, in the shape of a command's error.
fn refusal(status: Status, message: &str) -> Response {
    failure(status, &ErrorView::new(message))
}

/// Whether a connection from `peer` may be served: only a loopback peer may.
fn admits(peer: IpAddr) -> bool {
    peer.is_loopback()
}

/// Whether the content type says the body is JSON, whatever parameters follow it.
fn is_json(content_type: Option<&str>) -> bool {
    content_type
        .and_then(|value| value.split(';').next())
        .is_some_and(|media| media.trim().eq_ignore_ascii_case(JSON))
}

/// The bridge: a session and the port its requests must name.
#[derive(Debug)]
pub struct Bridge {
    port: u16,
    session: Session,
}

impl Bridge {
    /// A bridge that serves on `port` and keeps `session`. Only a request whose `Host` is the
    /// loopback address or `localhost` with this port is served.
    pub const fn new(port: u16, session: Session) -> Self {
        Self { port, session }
    }

    /// Whether `host`, a request's `Host` header, names this bridge's own loopback address and
    /// port. A page on another site reaches the bridge under its own name, which this refuses,
    /// even when that name resolves to the loopback address.
    fn is_own_host(&self, host: Option<&str>) -> bool {
        let Some((name, port)) = host.and_then(|host| host.rsplit_once(':')) else {
            return false;
        };
        port == self.port.to_string()
            && (name == Ipv4Addr::LOCALHOST.to_string() || name.eq_ignore_ascii_case("localhost"))
    }

    /// Answers `request`: refuses what is not a command of the bridge, and otherwise runs the
    /// command it names.
    pub fn handle(&self, request: &Request) -> Response {
        if !self.is_own_host(request.header("host")) {
            return refusal(
                Status::Forbidden,
                "the Host header is not the bridge's own loopback address and port",
            );
        }
        let Some(name) = request.target.strip_prefix(COMMAND_PATH) else {
            return refusal(Status::NotFound, "the path names no command");
        };
        let Some(command) = Command::named(name) else {
            return refusal(Status::NotFound, &format!("unknown command `{name}`"));
        };
        if request.method != "POST" {
            return refusal(Status::MethodNotAllowed, "a command is a POST").allowing("POST");
        }
        if !is_json(request.header("content-type")) {
            return refusal(
                Status::UnsupportedMediaType,
                "the body must be sent as application/json",
            );
        }
        match command.run(&self.session, &request.body) {
            Ok(Answer::Json(body)) => Response::new(Status::Ok, JSON, body),
            Ok(Answer::Arrow(body)) => Response::new(Status::Ok, ARROW, body),
            Err(Failure::Arguments(message)) => refusal(Status::BadRequest, &message),
            Err(Failure::Refused(error)) => failure(Status::Unprocessable, &error),
        }
    }

    /// Reads one request from `reader`, answers it on `writer` and has nothing more to say. A
    /// connection that sends nothing, or breaks, gets no answer.
    ///
    /// # Errors
    ///
    /// Returns the writer's error.
    pub fn exchange(&self, reader: &mut impl BufRead, writer: &mut impl Write) -> io::Result<()> {
        let response = match http::read_request(reader) {
            Ok(request) => self.handle(&request),
            Err(ReadError::Closed | ReadError::Io) => return Ok(()),
            Err(ReadError::Malformed(reason)) => refusal(Status::BadRequest, reason),
            Err(ReadError::HeadTooLarge) => {
                refusal(Status::PayloadTooLarge, "the request's head is too long")
            }
            Err(ReadError::BodyTooLarge) => {
                refusal(Status::PayloadTooLarge, "the request's body is too long")
            }
            Err(ReadError::Unsupported(what)) => refusal(
                Status::NotImplemented,
                &format!("the bridge takes no {what}"),
            ),
            Err(ReadError::TimedOut) => refusal(
                Status::RequestTimeout,
                "the request took too long to arrive",
            ),
        };
        response.write_to(writer)
    }
}

/// Listens on the loopback address and `port`; port 0 lets the system choose.
///
/// # Errors
///
/// Returns why the port cannot be listened on, such as being in use.
pub fn bind(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port))
}

/// How long each part of a connection may take in all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Deadlines {
    /// Reading the whole request, its head and its body, from the first byte asked for.
    request: Duration,
    /// Writing the whole answer, from its first byte. The command that makes the answer takes
    /// as long as it takes and is not counted.
    response: Duration,
    /// Reading what the client still sends after it was answered.
    drain: Duration,
}

/// A connection that gives up at a deadline: each read and write waits for the time that is left,
/// at most a timeout, and fails with `TimedOut` once none is.
struct Bounded<'a> {
    stream: &'a TcpStream,
    within: Duration,
    /// When the time is up; it is set at the first read or write.
    deadline: Option<Instant>,
}

impl<'a> Bounded<'a> {
    const fn new(stream: &'a TcpStream, within: Duration) -> Self {
        Self {
            stream,
            within,
            deadline: None,
        }
    }

    /// The time that is left, or the error that says it is up.
    fn left(&mut self) -> io::Result<Duration> {
        let within = self.within;
        let deadline = *self.deadline.get_or_insert_with(|| Instant::now() + within);
        deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "the connection's time is up"))
    }
}

impl Read for Bounded<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let left = self.left()?;
        self.stream.set_read_timeout(Some(left.min(READ_TIMEOUT)))?;
        let mut stream = self.stream;
        stream.read(buffer)
    }
}

impl Write for Bounded<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let left = self.left()?;
        self.stream
            .set_write_timeout(Some(left.min(WRITE_TIMEOUT)))?;
        let mut stream = self.stream;
        stream.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut stream = self.stream;
        stream.flush()
    }
}

/// Serves one connection: one request, one answer, then closes. A peer that is not loopback is
/// closed on without a word. A client that is too slow for `deadlines` is answered `408`, or
/// closed on when it takes the answer too slowly.
fn serve_connection(stream: &TcpStream, bridge: &Bridge, deadlines: Deadlines) -> io::Result<()> {
    if !stream.peer_addr().is_ok_and(|peer| admits(peer.ip())) {
        return Ok(());
    }
    let mut reader = BufReader::new(Bounded::new(stream, deadlines.request));
    let mut writer = Bounded::new(stream, deadlines.response);
    bridge.exchange(&mut reader, &mut writer)?;
    stream.shutdown(Shutdown::Write)?;
    // What the client still sends is read and dropped: closing on unread bytes resets the
    // connection, and the client could lose the answer, which is what refuses a body that is
    // too large without reading it. A client that sends more than that, or is too slow, is
    // closed on, as it was told what it needs to know.
    let _ = io::copy(
        &mut Bounded::new(stream, deadlines.drain).take(MAX_DRAIN_BYTES),
        &mut io::sink(),
    );
    Ok(())
}

/// Serves connections on `listener` for as long as the process runs, each on a thread of its own.
///
/// # Errors
///
/// Returns the error that stopped the listener accepting.
pub fn serve(listener: &TcpListener, bridge: Bridge) -> io::Result<()> {
    let bridge = Arc::new(bridge);
    loop {
        let stream = match listener.accept() {
            Ok((stream, _peer)) => stream,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionAborted | io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        };
        let bridge = Arc::clone(&bridge);
        // A connection that cannot get a thread is closed; the client asks again.
        let _ = thread::Builder::new()
            .name("bridge-connection".to_owned())
            .spawn(move || {
                // A connection that breaks has no one left to tell.
                let _ = serve_connection(&stream, &bridge, DEADLINES);
            });
    }
}

/// Runs the bridge for the application whose Tauri identifier is `identifier`, over the sample
/// table kept in that application's cache, until the process is stopped. Prints the address it
/// listens on.
///
/// # Errors
///
/// Returns why the bridge cannot start or stops.
pub fn run(port: u16, identifier: &str) -> io::Result<()> {
    let cache = cache::application_cache(identifier).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "the platform's cache folder is not known",
        )
    })?;
    let listener = bind(port)?;
    let address = listener.local_addr()?;
    println!("dageist bridge listening on http://{address}");
    let session = Session::new(Sample::in_cache(&cache));
    serve(&listener, Bridge::new(address.port(), session))
}
