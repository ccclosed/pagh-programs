//! `curl` — one HTTP/1.0 GET over the ABI's native TCP client.
//!
//! Usage: `curl [-i] [-o FILE] http://IPv4[:PORT]/PATH`. The URL must use an
//! IPv4 literal: there is no DNS in the kernel stack, and only `http://` is
//! supported (no TLS, no redirects, no proxies). The request uses HTTP/1.0
//! with `Connection: close`, and the body is printed to stdout unless
//! `-o FILE` names a destination (`-o -` means stdout). `-i` includes the
//! status line and headers, normalised to LF, before the body.
//!
//! A non-2xx status is an HTTP error: the diagnostic is
//! `curl: HTTP NNN Reason` and the exit code is 1. A failed connect/send/
//! receive is also exit 1; a bad option or unsupported URL is exit 2. There is
//! no `-f`, `-L`, `-s`, `-H`, POST, upload or cookie support.
//!
//! Exit: 0 success, 1 network/HTTP/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::File;

use super::httpc::{self, Error};
use crate::util;

const USAGE: &str = "usage: curl [-i] [-o FILE] http://HOST[:PORT]/PATH";

pub fn run(args: &[String]) -> i32 {
    let mut headers = false;
    let mut output: Option<&str> = None;
    let mut url_text: Option<&str> = None;
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg == "-i" {
            headers = true;
            continue;
        }
        if parse_options && arg == "-o" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            if value.is_empty() {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            }
            output = Some(value);
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("-o") {
                if !value.is_empty() {
                    output = Some(value);
                    continue;
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if url_text.is_some() {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        url_text = Some(arg);
    }

    let Some(url_text) = url_text else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };
    let Some(url) = httpc::parse_url(url_text) else {
        eprintln!("curl: unsupported URL: '{url_text}'");
        return util::EXIT_USAGE;
    };

    let response = match httpc::fetch(&url) {
        Ok(response) => response,
        Err(error) => return report_error(error),
    };

    let mut payload: Vec<u8> = Vec::new();
    if headers {
        if payload.try_reserve(response.head.len() + 2).is_err() {
            eprintln!("curl: out of memory");
            return util::EXIT_FAILURE;
        }
        payload.extend_from_slice(&response.head);
        payload.extend_from_slice(b"\n\n");
    }
    if payload.try_reserve(response.body.len()).is_err() {
        eprintln!("curl: out of memory");
        return util::EXIT_FAILURE;
    }
    payload.extend_from_slice(&response.body);

    match output {
        None | Some("-") => {
            if let Err(err) = util::write_all(1, &payload) {
                eprintln!("curl: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
        Some(name) => {
            let file = match File::create(name) {
                Ok(file) => file,
                Err(err) => {
                    util::report("curl", name, err);
                    return util::EXIT_FAILURE;
                }
            };
            if let Err(err) = util::write_all(file.as_raw_fd(), &payload) {
                util::report("curl", name, err);
                return util::EXIT_FAILURE;
            }
        }
    }
    util::EXIT_OK
}

fn report_error(error: Error) -> i32 {
    match error {
        Error::Connect(err) => eprintln!("curl: connect: {err}"),
        Error::Send(err) => eprintln!("curl: send: {err}"),
        Error::Receive(err) => eprintln!("curl: receive: {err}"),
        Error::Malformed => eprintln!("curl: malformed HTTP response"),
        Error::Http(code, reason) => {
            if reason.is_empty() {
                eprintln!("curl: HTTP {code}");
            } else {
                eprintln!("curl: HTTP {code} {reason}");
            }
        }
    }
    util::EXIT_FAILURE
}
