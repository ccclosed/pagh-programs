//! A tiny HTTP/1.0 client shared by `curl` and `wget`.
//!
//! Only `http://IPv4[:PORT]/PATH` is accepted: the ABI's network stack has no
//! DNS, TLS or listening sockets, and the HTTP exchange is deliberately the
//! one-request kind. The client sends
//!
//! ```text
//! GET /PATH HTTP/1.0
//! Host: IP[:PORT]
//! Connection: close
//! ```
//!
//! and reads until the peer closes. Headers are normalised from CRLF to LF for
//! the console; the body is left byte-for-byte untouched. A non-2xx status is
//! an [`Error::Http`] (the applets exit 1), while a connect/send/receive
//! failure carries the ABI's [`Errno`]. No redirects, chunked decoding,
//! proxies or authentication.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::errno::Errno;
use libpagh::net::{Ipv4, TcpStream};
use libpagh::sys;

/// A parsed HTTP URL with an IPv4 literal host.
pub struct Url {
    host: Ipv4,
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
    /// Response body, byte-exact.
    pub body: Vec<u8>,
}

pub enum Error {
    Connect(Errno),
    Send(Errno),
    Receive(Errno),
    Malformed,
    Http(u16, String),
}

/// Parse `http://HOST[:PORT][/PATH]`; anything else is `None`.
pub fn parse_url(text: &str) -> Option<Url> {
    let rest = text.strip_prefix("http://")?;
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
        None => (authority, 80),
    };
    let host = Ipv4::parse(host_text)?;

    let path = match raw_path.split('#').next() {
        Some(path) if !path.is_empty() => path,
        _ => "/",
    };
    if path.bytes().any(|byte| byte <= 0x20 || byte == 0x7f) {
        return None;
    }

    let host_header = if port == 80 {
        String::from(host_text)
    } else {
        let mut header = String::from(host_text);
        header.push(':');
        header.push_str(&alloc::format!("{port}"));
        header
    };
    Some(Url {
        host,
        port,
        path: String::from(path),
        host_header,
    })
}

/// Perform the GET and return a 2xx response. The connection is closed by
/// dropping `TcpStream` before this returns.
pub fn fetch(url: &Url) -> Result<Response, Error> {
    let mut stream = TcpStream::connect(url.host, url.port).map_err(Error::Connect)?;
    let request = alloc::format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        url.path,
        url.host_header
    );
    send_all(&stream, request.as_bytes())?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(Error::Receive)?;
    parse_response(&raw)
}

fn send_all(stream: &TcpStream, mut buffer: &[u8]) -> Result<(), Error> {
    while !buffer.is_empty() {
        match sys::send(stream.fd(), buffer) {
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
    let (head_raw, body) = match find_separator(raw) {
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
    Ok(Response {
        head,
        body: Vec::from(body),
    })
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
