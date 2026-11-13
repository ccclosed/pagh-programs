//! `nc` — a small netcat: one TCP connection, client or single-connection
//! listener, relaying bytes between the peer and stdin/stdout.
//!
//! ```text
//! nc HOST PORT          # connect
//! nc -l [-p PORT] PORT  # listen on 0.0.0.0:PORT for one connection
//! ```
//!
//! This is deliberately the TCP subset the ABI actually has: BSD
//! `socket`/`connect`/`bind`/`listen`/`accept` plus `poll` on the socket and on
//! the console. `HOST` may be an IPv4 literal or a DNS name (one `A` query
//! through [`libpagh::dns`], QEMU's resolver by default — the same resolution
//! `curl`/`wget` use, with no DNSSEC).
//!
//! The relay is the `poll(2)` loop: whichever side is readable is copied to the
//! other. The ABI's TCP `recv` is non-blocking (`EAGAIN` = nothing queued yet)
//! and `poll` is what makes this wait without spinning; console input is also
//! `EAGAIN`-based, so fd 0 is polled alongside the socket. When the peer closes
//! (a `read` of 0) the applet exits 0. When stdin reaches EOF (a pipe, not this
//! console) the socket stays open for reading: the ABI's `shutdown` closes both
//! directions, so a half-close is not expressible and silently closing the
//! socket would cut off the reply.
//!
//! Not implemented, and refused rather than faked: `-u` (UDP; the ABI has no
//! UDP `connect`, and a datagram relay has no EOF), `-e` (exec on a socket; the
//! ABI has `execve` but not a socket fd pass to it), `-z` (scan), `-i`/`-w`
//! (poll has a timeout, but there is no wall-clock option parsing worth
//! promising), and `-s` (bind a source address; `bind` only accepts the DHCP
//! lease here). Exit: 0 peer closed cleanly; 1 resolve/connect/bind/accept/
//! relay error; 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::abi::PollFd;
use libpagh::dns;
use libpagh::eprintln;
use libpagh::errno::Errno;
use libpagh::io::{Read, Write};
use libpagh::net::{Ipv4, TcpListener, TcpStream};
use libpagh::sys;

use crate::util;

const USAGE: &str = "usage: nc HOST PORT\n       nc -l [-p PORT] PORT";

pub fn run(args: &[String]) -> i32 {
    let mut listen = false;
    let mut port_option: Option<u16> = None;
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if arg == "--" {
            for rest in &args[index..] {
                if util::push(&mut operands, rest.as_str()).is_err() {
                    eprintln!("nc: out of memory");
                    return util::EXIT_FAILURE;
                }
            }
            break;
        }
        if arg == "-l" {
            listen = true;
            continue;
        }
        if arg == "-p" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match parse_port(value) {
                Some(port) => port_option = Some(port),
                None => {
                    eprintln!("nc: invalid port: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("-p") {
            match parse_port(value) {
                Some(port) => {
                    port_option = Some(port);
                    continue;
                }
                None => {
                    eprintln!("nc: invalid port: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
        }
        // Every other option is a real `nc` option this ABI cannot support;
        // naming it is more useful than a generic usage line.
        if arg.len() > 1 && arg.starts_with('-') {
            eprintln!("nc: unsupported option '{arg}' (TCP client/listener only)");
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        if util::push(&mut operands, arg).is_err() {
            eprintln!("nc: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if listen {
        run_listener(port_option, &operands)
    } else {
        run_client(port_option, &operands)
    }
}

fn run_client(port_option: Option<u16>, operands: &[&str]) -> i32 {
    if operands.len() != 2 || port_option.is_some() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let host = operands[0];
    let Some(port) = parse_port(operands[1]) else {
        eprintln!("nc: invalid port: '{}'", operands[1]);
        return util::EXIT_USAGE;
    };

    let address = match resolve(host) {
        Ok(address) => address,
        Err(err) => {
            eprintln!("nc: resolve {host}: {err}");
            return util::EXIT_FAILURE;
        }
    };

    let mut stream = match TcpStream::connect(address, port) {
        Ok(stream) => stream,
        Err(err) => {
            eprintln!("nc: {host}:{port}: {err}");
            return util::EXIT_FAILURE;
        }
    };
    match relay(&mut stream) {
        Ok(()) => util::EXIT_OK,
        Err(err) => {
            eprintln!("nc: {err}");
            util::EXIT_FAILURE
        }
    }
}

fn run_listener(port_option: Option<u16>, operands: &[&str]) -> i32 {
    let port = match (port_option, operands) {
        (Some(port), []) => port,
        (None, [port]) => match parse_port(port) {
            Some(port) => port,
            None => {
                eprintln!("nc: invalid port: '{port}'");
                return util::EXIT_USAGE;
            }
        },
        _ => {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
    };

    let mut listener = match TcpListener::bind(port) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("nc: bind 0.0.0.0:{port}: {err}");
            return util::EXIT_FAILURE;
        }
    };
    // One connection, then the applet is done: the ABI has a single accept
    // queue and the shell can only run one program at a time, so a loop would
    // hold the console with no way to serve a second client concurrently.
    let (mut stream, _peer) = match listener.accept() {
        Ok(accepted) => accepted,
        Err(err) => {
            eprintln!("nc: accept 0.0.0.0:{port}: {err}");
            return util::EXIT_FAILURE;
        }
    };
    match relay(&mut stream) {
        Ok(()) => util::EXIT_OK,
        Err(err) => {
            eprintln!("nc: {err}");
            util::EXIT_FAILURE
        }
    }
}

/// Resolve an IPv4 literal or a DNS name.
fn resolve(host: &str) -> Result<Ipv4, Errno> {
    match Ipv4::parse(host) {
        Some(address) => Ok(address),
        None => dns::resolve(host, dns::DEFAULT_SERVER),
    }
}

/// Copy bytes both ways until the peer closes.
fn relay(stream: &mut TcpStream) -> Result<(), Errno> {
    let mut fds = [
        PollFd {
            fd: 0,
            events: sys::POLLIN,
            revents: 0,
        },
        PollFd {
            fd: stream.fd(),
            events: sys::POLLIN,
            revents: 0,
        },
    ];
    let mut buffer = [0u8; 1024];
    let mut stdin_open = true;

    loop {
        for entry in &mut fds {
            entry.revents = 0;
        }
        match sys::poll(&mut fds, -1) {
            Ok(_) => {}
            // No handlers are installed by this applet, but a signal that a
            // parent could deliver must not turn into a spurious failure.
            Err(Errno::EINTR) => continue,
            Err(err) => return Err(err),
        }

        if fds[1].revents & (sys::POLLIN | sys::POLLERR | sys::POLLHUP) != 0 {
            match stream.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(read) => util::write_all(1, &buffer[..read])?,
                Err(Errno::EAGAIN) => {}
                Err(err) => return Err(err),
            }
        }

        if stdin_open && fds[0].revents & (sys::POLLIN | sys::POLLHUP) != 0 {
            match util::read_retry(0, &mut buffer) {
                Ok(0) => {
                    stdin_open = false;
                    // A negative descriptor is skipped by `poll`; stop asking
                    // for a readable console once it has reached EOF.
                    fds[0].fd = -1;
                }
                Ok(read) => stream.write_all(&buffer[..read])?,
                Err(err) => return Err(err),
            }
        }
    }
}

/// A TCP port: decimal, 1..=65535. Port 0 is refused because the ABI has no
/// `getsockname`, so the caller could never learn the chosen port.
fn parse_port(text: &str) -> Option<u16> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    match text.parse::<u16>() {
        Ok(port) if port != 0 => Some(port),
        _ => None,
    }
}
