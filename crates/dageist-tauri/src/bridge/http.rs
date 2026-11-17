//! The part of HTTP/1.1 the bridge speaks: one request on a connection, read
//! from any buffered reader, and one response written to any writer.
//!
//! The bridge needs no more than a request line, headers and a body whose
//! length the request states, and it answers once and closes. Anything else a
//! client might send, such as a chunked body, is refused, not guessed at.

use std::io::{self, BufRead, Read, Take, Write};

/// The most bytes a request line and its headers may take together.
pub const MAX_HEAD_BYTES: usize = 16 * 1024;
/// The most bytes a request body may take. A step document is text of a few
/// kilobytes, so this leaves room without letting a client fill memory.
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// Headers whose repeated appearance makes a request ambiguous about who it is
/// from, what it carries or how long it is, so a repeat is refused.
const SINGLE_HEADERS: [&str; 4] = [
    "host",
    "content-type",
    "content-length",
    "transfer-encoding",
];

/// A request as it arrived: what it asks for and the bytes it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub(super) method: String,
    pub(super) target: String,
    headers: Vec<(String, String)>,
    pub(super) body: Vec<u8>,
}

impl Request {
    /// The value of the header called `name`, however it is capitalised.
    pub(super) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _value)| candidate.eq_ignore_ascii_case(name))
            .map(|(_name, value)| value.as_str())
    }
}

/// Why a request could not be read.
#[derive(Debug)]
pub enum ReadError {
    /// The peer closed the connection before sending a request.
    Closed,
    /// What arrived is not a request this reader understands.
    Malformed(&'static str),
    /// The request line and headers are longer than `MAX_HEAD_BYTES`.
    HeadTooLarge,
    /// The body is longer than `MAX_BODY_BYTES`.
    BodyTooLarge,
    /// A framing the bridge does not take, such as a chunked body.
    Unsupported(&'static str),
    /// The peer was too slow to send what it began.
    TimedOut,
    /// The connection failed, and there is nobody to tell.
    Io,
}

impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => Self::TimedOut,
            _ => Self::Io,
        }
    }
}

/// The status of a response: only the ones the bridge answers with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    BadRequest,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    RequestTimeout,
    PayloadTooLarge,
    UnsupportedMediaType,
    Unprocessable,
    NotImplemented,
}

impl Status {
    /// The three-digit code.
    pub const fn code(self) -> u16 {
        match self {
            Self::Ok => 200,
            Self::BadRequest => 400,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::MethodNotAllowed => 405,
            Self::RequestTimeout => 408,
            Self::PayloadTooLarge => 413,
            Self::UnsupportedMediaType => 415,
            Self::Unprocessable => 422,
            Self::NotImplemented => 501,
        }
    }

    const fn reason(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::BadRequest => "Bad Request",
            Self::Forbidden => "Forbidden",
            Self::NotFound => "Not Found",
            Self::MethodNotAllowed => "Method Not Allowed",
            Self::RequestTimeout => "Request Timeout",
            Self::PayloadTooLarge => "Content Too Large",
            Self::UnsupportedMediaType => "Unsupported Media Type",
            Self::Unprocessable => "Unprocessable Content",
            Self::NotImplemented => "Not Implemented",
        }
    }
}

/// A response: a status and one body of one content type.
///
/// It carries no cross-origin headers, so a browser lets no other site read
/// it, and it asks to be neither cached nor sniffed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub(super) status: Status,
    pub(super) content_type: &'static str,
    pub(super) allow: Option<&'static str>,
    pub(super) body: Vec<u8>,
}

impl Response {
    /// A response of `status` that carries `body` as `content_type`.
    pub const fn new(status: Status, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type,
            allow: None,
            body,
        }
    }

    /// The same response, saying which methods the resource takes.
    #[must_use]
    pub const fn allowing(mut self, methods: &'static str) -> Self {
        self.allow = Some(methods);
        self
    }

    /// Writes the response, then flushes.
    ///
    /// # Errors
    ///
    /// Returns the writer's error.
    pub fn write_to(&self, writer: &mut impl Write) -> io::Result<()> {
        write!(
            writer,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
             Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
            self.status.code(),
            self.status.reason(),
            self.content_type,
            self.body.len(),
        )?;
        if let Some(methods) = self.allow {
            write!(writer, "Allow: {methods}\r\n")?;
        }
        writer.write_all(b"\r\n")?;
        writer.write_all(&self.body)?;
        writer.flush()
    }
}

/// Reads one request: its line, its headers and the body its `Content-Length`
/// states, or none.
///
/// # Errors
///
/// Returns why what arrived is not a request the bridge takes.
pub fn read_request(reader: &mut impl BufRead) -> Result<Request, ReadError> {
    let mut head = reader.by_ref().take(MAX_HEAD_BYTES as u64);
    let request_line = loop {
        match read_line(&mut head)? {
            // A client may put blank lines before the request line.
            Some(line) if line.is_empty() => {}
            Some(line) => break line,
            None => return Err(ReadError::Closed),
        }
    };
    let (method, target) = parse_request_line(&request_line)?;
    let mut headers: Vec<(String, String)> = Vec::new();
    loop {
        let line = read_line(&mut head)?
            .ok_or(ReadError::Malformed("the request ends inside its headers"))?;
        if line.is_empty() {
            break;
        }
        let (name, value) = parse_header(&line)?;
        let repeated = headers
            .iter()
            .any(|(known, _value)| known.eq_ignore_ascii_case(&name));
        if repeated
            && SINGLE_HEADERS
                .iter()
                .any(|single| name.eq_ignore_ascii_case(single))
        {
            return Err(ReadError::Malformed(
                "a header that must be single is repeated",
            ));
        }
        headers.push((name, value));
    }
    let mut request = Request {
        method,
        target,
        headers,
        body: Vec::new(),
    };
    if request.header("transfer-encoding").is_some() {
        return Err(ReadError::Unsupported("a body with a transfer encoding"));
    }
    let length = match request.header("content-length") {
        None => 0,
        Some(text) => parse_length(text)?,
    };
    if length > MAX_BODY_BYTES {
        return Err(ReadError::BodyTooLarge);
    }
    let mut body = Vec::with_capacity(length.min(64 * 1024));
    // The head's limit covered the head only; the body is read from the reader itself.
    reader.by_ref().take(length as u64).read_to_end(&mut body)?;
    if body.len() != length {
        return Err(ReadError::Malformed("the request ends inside its body"));
    }
    request.body = body;
    Ok(request)
}

/// Reads one line ended by CRLF, without it, or `None` at the end of input
/// before any byte of it.
fn read_line<R: BufRead>(head: &mut Take<&mut R>) -> Result<Option<String>, ReadError> {
    let mut line = Vec::new();
    head.read_until(b'\n', &mut line)?;
    if line.is_empty() {
        return Ok(None);
    }
    if !line.ends_with(b"\r\n") {
        // The line stopped at the limit, or the input ended inside it, or it ends in a bare LF.
        return Err(if head.limit() == 0 {
            ReadError::HeadTooLarge
        } else {
            ReadError::Malformed("a line does not end in CRLF")
        });
    }
    line.truncate(line.len() - 2);
    String::from_utf8(line)
        .map(Some)
        .map_err(|_invalid| ReadError::Malformed("a line is not UTF-8"))
}

fn parse_request_line(line: &str) -> Result<(String, String), ReadError> {
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(ReadError::Malformed("the request line has not three parts"));
    };
    if method.is_empty() || !method.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(ReadError::Malformed("the method is not a word of capitals"));
    }
    if !target.starts_with('/') {
        return Err(ReadError::Malformed("the target is not a path"));
    }
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(ReadError::Malformed(
            "the version is not HTTP/1.1 or HTTP/1.0",
        ));
    }
    Ok((method.to_owned(), target.to_owned()))
}

fn parse_header(line: &str) -> Result<(String, String), ReadError> {
    let Some((name, value)) = line.split_once(':') else {
        return Err(ReadError::Malformed("a header has no colon"));
    };
    let is_token = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte));
    if !is_token {
        return Err(ReadError::Malformed("a header's name is not a token"));
    }
    Ok((name.to_owned(), value.trim_matches([' ', '\t']).to_owned()))
}

fn parse_length(text: &str) -> Result<usize, ReadError> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ReadError::Malformed("the content length is not a number"));
    }
    // A number too long for `usize` is longer than any body that is taken.
    text.parse().or(Ok(usize::MAX))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{
        MAX_BODY_BYTES, MAX_HEAD_BYTES, ReadError, Request, Response, Status, read_request,
    };

    fn read(raw: &[u8]) -> Result<Request, ReadError> {
        read_request(&mut Cursor::new(raw.to_vec()))
    }

    #[test]
    fn a_request_is_read_with_its_headers_and_the_body_its_length_states() {
        let request = read(
            b"POST /bridge/run_document HTTP/1.1\r\nHost: 127.0.0.1:1\r\nContent-Length: 5\r\n\
              X-Spaced:   value \t\r\n\r\nhelloEXTRA",
        )
        .expect("a request");

        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/bridge/run_document");
        assert_eq!(request.body, b"hello");
        assert_eq!(request.header("host"), Some("127.0.0.1:1"));
        assert_eq!(request.header("HOST"), Some("127.0.0.1:1"));
        assert_eq!(request.header("x-spaced"), Some("value"));
        assert_eq!(request.header("absent"), None);
    }

    #[test]
    fn a_request_without_a_length_has_no_body() {
        let request = read(b"GET / HTTP/1.0\r\n\r\n").expect("a request");

        assert!(request.body.is_empty());
    }

    #[test]
    fn nothing_sent_is_a_closed_connection() {
        assert!(matches!(read(b""), Err(ReadError::Closed)));
        assert!(matches!(read(b"\r\n\r\n"), Err(ReadError::Closed)));
    }

    #[test]
    fn what_is_not_a_request_is_malformed() {
        let cases: [&[u8]; 9] = [
            b"POST /x\r\n\r\n",
            b"post /x HTTP/1.1\r\n\r\n",
            b"POST x HTTP/1.1\r\n\r\n",
            b"POST /x HTTP/2\r\n\r\n",
            b"POST /x HTTP/1.1\r\nNoColon\r\n\r\n",
            b"POST /x HTTP/1.1\r\nBad Name: x\r\n\r\n",
            b"POST /x HTTP/1.1\r\nHost: a\r\n",
            b"POST /x HTTP/1.1\nHost: a\n\n",
            b"POST /x HTTP/1.1\r\nContent-Length: 4\r\n\r\nab",
        ];
        for raw in cases {
            assert!(
                matches!(read(raw), Err(ReadError::Malformed(_))),
                "{}",
                String::from_utf8_lossy(raw)
            );
        }
    }

    #[test]
    fn a_length_that_is_not_a_number_is_malformed() {
        for length in ["", "-1", "1e3", "0x10", "1 2"] {
            let raw = format!("POST /x HTTP/1.1\r\nContent-Length: {length}\r\n\r\n");
            assert!(
                matches!(read(raw.as_bytes()), Err(ReadError::Malformed(_))),
                "length {length:?}"
            );
        }
    }

    #[test]
    fn a_header_that_must_be_single_is_refused_when_repeated() {
        for name in ["Host", "content-type", "Content-Length"] {
            let raw = format!("POST /x HTTP/1.1\r\n{name}: 1\r\n{name}: 1\r\n\r\n");
            assert!(
                matches!(read(raw.as_bytes()), Err(ReadError::Malformed(_))),
                "{name}"
            );
        }
    }

    #[test]
    fn a_body_with_a_transfer_encoding_is_unsupported() {
        let raw = b"POST /x HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n";

        assert!(matches!(read(raw), Err(ReadError::Unsupported(_))));
    }

    #[test]
    fn a_head_longer_than_the_limit_is_too_large() {
        let raw = format!(
            "POST /x HTTP/1.1\r\nX-Long: {}\r\n\r\n",
            "a".repeat(MAX_HEAD_BYTES)
        );

        assert!(matches!(read(raw.as_bytes()), Err(ReadError::HeadTooLarge)));
    }

    #[test]
    fn a_body_longer_than_the_limit_is_refused_before_it_is_read() {
        let raw = format!(
            "POST /x HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        );
        assert!(matches!(read(raw.as_bytes()), Err(ReadError::BodyTooLarge)));

        let absurd = "POST /x HTTP/1.1\r\nContent-Length: 99999999999999999999999\r\n\r\n";
        assert!(matches!(
            read(absurd.as_bytes()),
            Err(ReadError::BodyTooLarge)
        ));
    }

    #[test]
    fn a_response_is_written_with_its_length_and_no_cross_origin_headers() {
        let mut written = Vec::new();

        Response::new(Status::MethodNotAllowed, "application/json", b"{}".to_vec())
            .allowing("POST")
            .write_to(&mut written)
            .expect("it is written");

        assert_eq!(
            String::from_utf8(written).expect("text"),
            "HTTP/1.1 405 Method Not Allowed\r\nContent-Type: application/json\r\n\
             Content-Length: 2\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\
             Connection: close\r\nAllow: POST\r\n\r\n{}"
        );
    }
}
