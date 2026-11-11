//! A tiny HTTP client shared by `curl` and `wget`.
//!
//! Both `http://` and `https://` URLs are accepted; the host may be an IPv4
//! literal or a DNS name.
//!
//! * **Plain HTTP** is one `GET` over the ABI's native TCP client.
//! * **HTTPS** makes the same TCP connection and then hands the connected
//!   descriptor to [`TlsStream::connect`]: the kernel performs a **fail-closed
//!   TLS 1.3 handshake** — the certificate must chain to the committed CA
//!   bundle, the requested name must be authorized by its SAN, the clock gate
//!   must pass and `CertificateVerify` must prove the leaf key. There is no
//!   flag in this module (or in `libpagh`) that turns verification off.
//! * A **DNS name** is resolved with [`libpagh::dns`]: one `A` query over UDP,
//!   QEMU's resolver by default, no DNSSEC. That is safe for HTTPS because the
//!   name is authenticated by the certificate after resolution, not by DNS.
//!
//! The request is deliberately one-shot:
//!
//! ```text
//! GET /PATH HTTP/1.0
//! Host: HOST[:PORT]
//! Connection: close
//! ```
//!
//! HTTP/1.0 makes a server end the body by closing the connection (or by a
//! `Content-Length`); the three framing modes are handled — close-delimited,
//! `Content-Length` and `Transfer-Encoding: chunked` — because a peer that
//! ignores the version may still answer chunked. Headers are normalised from
//! CRLF to LF for the console; the body is left byte-for-byte untouched. A
//! non-2xx status is an [`Error::Http`] (the applets exit 1), while a
//! resolve/connect/TLS/send/receive failure carries the ABI's [`Errno`]. No
//! redirects, proxies, authentication, POST, uploads or cookies.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::dns;
use libpagh::errno::Errno;
use libpagh::io::{Read, Write};
use libpagh::net::{Ipv4, TcpStream};
use libpagh::sys;
use libpagh::tls::TlsStream;

/// Upper bound on one response this client will buffer, head included.
///
/// The applets run on a 256 MiB machine and the allocator is a bump allocator;
/// an unbounded `read_to_end` on a hostile peer would be an OOM abort. 8 MiB is
/// the same cap the kernel's own `https_get` example uses.
const MAX_RESPONSE: usize = 8 * 1024 * 1024;

/// The URL scheme; it selects the transport and the default port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// `http://` — plaintext over the native TCP client.
    Http,
    /// `https://` — kernel-verified TLS 1.3 over the same TCP client.
    Https,
}

impl Scheme {
    const fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }
}

/// A parsed HTTP(S) URL.
pub struct Url {
    scheme: Scheme,
    host: String,
    port: u16,
    path: String,
    host_header: String,
}

impl Url {
    /// The default local file name: the last path segment without a query,
    /// or `index.html` for `/` and other empty/`.`/`..` segments.
    pub fn default_name(&self) -> String {
        let path = match self.path.split('?').next() {
            Some(path) => path,
            None => "/",
        };
        let name = path.rsplit('/').next().unwrap_or("");
        if name.is_empty() || name == "." || name == ".." {
            String::from("index.html")
        } else {
            String::from(name)
        }
    }
}

/// The result of a successful request (2xx only).
pub struct Response {
    /// Status line and headers, normalised to LF, without the final blank line.
    pub head: Vec<u8>,
    /// Response body, byte-exact and de-chunked.
    pub body: Vec<u8>,
}

pub enum Error {
    Resolve(Errno),
    Connect(Errno),
    Tls(Errno),
    Send(Errno),
    Receive(Errno),
    Malformed,
    /// The peer's response exceeded [`MAX_RESPONSE`].
    TooLarge,
    /// An allocation failed; the response is not held in memory.
    OutOfMemory,
    Http(u16, String),
}

/// Parse `http://` or `https://` + `HOST[:PORT][/PATH]`; anything else is
/// `None`. `HOST` is an IPv4 literal or a dotted DNS name.
pub fn parse_url(text: &str) -> Option<Url> {
    let (scheme, rest) = if let Some(rest) = text.strip_prefix("http://") {
        (Scheme::Http, rest)
    } else if let Some(rest) = text.strip_prefix("https://") {
        (Scheme::Https, rest)
    } else {
        return None;
    };
    if rest.is_empty() {
        return None;
    }
    let (authority, raw_path) = match rest.find('/') {
        Some(position) => (&rest[..position], &rest[position..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || authority.contains('@') {
        return None;
    }

    let default_port = scheme.default_port();
    let (host_text, port) = match authority.rfind(':') {
        Some(position) => {
            let host = &authority[..position];
            let port_text = &authority[position + 1..];
            if port_text.is_empty() {
                return None;
            }
            let port = port_text.parse::<u16>().ok()?;
            if port == 0 {
                return None;
            }
            (host, port)
        }
        None => (authority, default_port),
    };
    if !valid_host(host_text) {
        return None;
    }

    let path = match raw_path.split('#').next() {
        Some(path) if !path.is_empty() => path,
        _ => "/",
    };
    if path.bytes().any(|byte| byte <= 0x20 || byte == 0x7f) {
        return None;
    }

    let host_header = if port == default_port {
        String::from(host_text)
    } else {
        let mut header = String::from(host_text);
        header.push(':');
        header.push_str(&alloc::format!("{port}"));
        header
    };
    Some(Url {
        scheme,
        host: String::from(host_text),
        port,
        path: String::from(path),
        host_header,
    })
}

/// A host this client will try: an IPv4 literal or a dotted ASCII name whose
/// labels are 1..=63 bytes of `[A-Za-z0-9-]` and do not start/end with `-`.
fn valid_host(host: &str) -> bool {
    if Ipv4::parse(host).is_some() {
        return true;
    }
    if host.is_empty() || host.len() > 255 {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

/// Perform the GET and return a 2xx response.
///
/// The connection is closed by dropping the transport before this returns.
pub fn fetch(url: &Url) -> Result<Response, Error> {
    let address = match Ipv4::parse(&url.host) {
        Some(address) => address,
        None => dns::resolve(&url.host, dns::DEFAULT_SERVER).map_err(Error::Resolve)?,
    };
    let request = alloc::format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        url.path, url.host_header
    );
    let raw = match url.scheme {
        Scheme::Http => {
            let stream = TcpStream::connect(address, url.port).map_err(Error::Connect)?;
            exchange(stream, request.as_bytes())?
        }
        Scheme::Https => {
            let socket = TcpStream::connect(address, url.port).map_err(Error::Connect)?;
            let stream = TlsStream::connect(socket, &url.host).map_err(Error::Tls)?;
            exchange(stream, request.as_bytes())?
        }
    };
    parse_response(&raw)
}

/// Write the request and read the peer's bytes until it closes the stream.
fn exchange<S: Read + Write>(mut stream: S, request: &[u8]) -> Result<Vec<u8>, Error> {
    send_all(&mut stream, request)?;
    let mut raw: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(raw),
            Ok(read) => {
                if raw.len() + read > MAX_RESPONSE {
                    return Err(Error::TooLarge);
                }
                raw.try_reserve(read).map_err(|_| Error::OutOfMemory)?;
                raw.extend_from_slice(&buffer[..read]);
            }
            Err(Errno::EAGAIN) => {
                let _ = sys::sched_yield();
            }
            Err(err) => return Err(Error::Receive(err)),
        }
    }
}

fn send_all<S: Write>(stream: &mut S, mut buffer: &[u8]) -> Result<(), Error> {
    while !buffer.is_empty() {
        match stream.write(buffer) {
            Ok(0) => return Err(Error::Send(Errno::EIO)),
            Ok(written) if written <= buffer.len() => buffer = &buffer[written..],
            Ok(_) => return Err(Error::Send(Errno::EIO)),
            Err(Errno::EAGAIN) => {
                let _ = sys::sched_yield();
            }
            Err(err) => return Err(Error::Send(err)),
        }
    }
    Ok(())
}

fn parse_response(raw: &[u8]) -> Result<Response, Error> {
    let (head_raw, body_raw) = match find_separator(raw) {
        Some((position, width)) => (&raw[..position], &raw[position + width..]),
        None => return Err(Error::Malformed),
    };
    let head = normalise_head(head_raw);
    let status_line_end = head
        .iter()
        .position(|byte| *byte == b'\n')
        .unwrap_or(head.len());
    let status_line = &head[..status_line_end];

    let mut fields = status_line
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|field| !field.is_empty());
    let version = fields.next().ok_or(Error::Malformed)?;
    if !version.starts_with(b"HTTP/") {
        return Err(Error::Malformed);
    }
    let code_text = fields.next().ok_or(Error::Malformed)?;
    if code_text.len() != 3 || !code_text.iter().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::Malformed);
    }
    let code = core::str::from_utf8(code_text)
        .ok()
        .and_then(|text| text.parse::<u16>().ok())
        .ok_or(Error::Malformed)?;
    let reason_start = status_line
        .windows(code_text.len())
        .position(|window| window == code_text)
        .map(|position| position + code_text.len())
        .unwrap_or(status_line.len());
    let reason = String::from(String::from_utf8_lossy(&status_line[reason_start..]).trim());

    if !(200..=299).contains(&code) {
        return Err(Error::Http(code, reason));
    }

    let body = if header_contains(&head, b"transfer-encoding", b"chunked") {
        decode_chunked(body_raw)?
    } else if let Some(length) = content_length(&head) {
        if body_raw.len() < length {
            return Err(Error::Malformed);
        }
        Vec::from(&body_raw[..length])
    } else {
        Vec::from(body_raw)
    };
    Ok(Response { head, body })
}

/// Whether header `name` (case-insensitive) carries `token` in its value.
fn header_contains(head: &[u8], name: &[u8], token: &[u8]) -> bool {
    for line in head.split(|byte| *byte == b'\n').skip(1) {
        let Some(colon) = line.iter().position(|byte| *byte == b':') else {
            continue;
        };
        if !line[..colon].eq_ignore_ascii_case(name) {
            continue;
        }
        let value = trim_ascii(&line[colon + 1..]);
        if value.windows(token.len()).any(|window| window.eq_ignore_ascii_case(token)) {
            return true;
        }
    }
    false
}

fn content_length(head: &[u8]) -> Option<usize> {
    for line in head.split(|byte| *byte == b'\n').skip(1) {
        let Some(colon) = line.iter().position(|byte| *byte == b':') else {
            continue;
        };
        if !line[..colon].eq_ignore_ascii_case(b"content-length") {
            continue;
        }
        let value = trim_ascii(&line[colon + 1..]);
        if value.is_empty() || !value.iter().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        return core::str::from_utf8(value).ok()?.parse::<usize>().ok();
    }
    None
}

/// Decode a `Transfer-Encoding: chunked` body into the plain body.
///
/// The chunk-size line may carry chunk extensions (`;...`), which are ignored;
/// the terminal zero chunk ends the body (trailers are not exposed).
fn decode_chunked(raw: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out: Vec<u8> = Vec::new();
    let mut rest = raw;
    loop {
        let (line_end, next) = find_line(rest).ok_or(Error::Malformed)?;
        let line = &rest[..line_end];
        let size_text = match line.iter().position(|byte| *byte == b';') {
            Some(position) => &line[..position],
            None => line,
        };
        let size_text = trim_ascii(size_text);
        if size_text.is_empty() {
            return Err(Error::Malformed);
        }
        let mut size = 0usize;
        for byte in size_text {
            let digit = match byte {
                b'0'..=b'9' => (byte - b'0') as usize,
                b'a'..=b'f' => (byte - b'a' + 10) as usize,
                b'A'..=b'F' => (byte - b'A' + 10) as usize,
                _ => return Err(Error::Malformed),
            };
            size = size
                .checked_mul(16)
                .and_then(|value| value.checked_add(digit))
                .ok_or(Error::TooLarge)?;
        }
        rest = &rest[next..];
        if size == 0 {
            return Ok(out);
        }
        if rest.len() < size {
            return Err(Error::Malformed);
        }
        if out.len() + size > MAX_RESPONSE {
            return Err(Error::TooLarge);
        }
        out.try_reserve(size).map_err(|_| Error::OutOfMemory)?;
        out.extend_from_slice(&rest[..size]);
        rest = &rest[size..];
        if let Some(after) = rest.strip_prefix(b"\r\n") {
            rest = after;
        } else if let Some(after) = rest.strip_prefix(b"\n") {
            rest = after;
        } else {
            return Err(Error::Malformed);
        }
    }
}

/// The next LF-terminated line: `(line length without the terminator, bytes
/// consumed)`.
fn find_line(raw: &[u8]) -> Option<(usize, usize)> {
    if let Some(position) = raw.windows(2).position(|window| window == b"\r\n") {
        return Some((position, position + 2));
    }
    raw.iter()
        .position(|byte| *byte == b'\n')
        .map(|position| (position, position + 1))
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while let Some((first, rest)) = bytes.split_first() {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    while let Some((last, rest)) = bytes.split_last() {
        if last.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

fn find_separator(raw: &[u8]) -> Option<(usize, usize)> {
    if let Some(position) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
        return Some((position, 4));
    }
    raw.windows(2)
        .position(|window| window == b"\n\n")
        .map(|position| (position, 2))
}

fn normalise_head(raw: &[u8]) -> Vec<u8> {
    let mut head = Vec::new();
    let mut index = 0usize;
    while index < raw.len() {
        if raw[index] == b'\r' && raw.get(index + 1) == Some(&b'\n') {
            head.push(b'\n');
            index += 2;
        } else {
            head.push(raw[index]);
            index += 1;
        }
    }
    if head.last() == Some(&b'\n') {
        head.pop();
    }
    head
}