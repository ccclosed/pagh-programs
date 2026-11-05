//! `wget` — one HTTP/1.0 GET written to a file.
//!
//! Usage: `wget [-O FILE] http://IPv4[:PORT]/PATH`. The body is written to
//! `FILE`, or to the basename of the URL path when `-O` is absent (`/` becomes
//! `index.html`); `-O -` writes to stdout. The request is the same native
//! HTTP/1.0 GET as `curl`'s: IPv4 literal only, `Connection: close`, no DNS,
//! TLS, redirects, retries or resume. After a successful write the single
//! summary line `wget: saved TARGET (N bytes)` goes to stderr, so it never
//! contaminates a body written to stdout.
//!
//! `-O` is deliberately the only option: no `-q`, `-c`, `-P`, `-O-` progress,
//! timestamping or recursive download. A non-2xx status is an HTTP error with
//! exit 1; a bad URL or option is exit 2.
//!
//! Exit: 0 success, 1 network/HTTP/write error, 2 usage.

use alloc::string::String;

use libpagh::eprintln;
use libpagh::fs::File;

use super::httpc::{self, Error};
use crate::util;

const USAGE: &str = "usage: wget [-O FILE] http://HOST[:PORT]/PATH";

pub fn run(args: &[String]) -> i32 {
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
        if parse_options && arg == "-O" {
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
            if let Some(value) = arg.strip_prefix("-O") {
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
        eprintln!("wget: unsupported URL: '{url_text}'");
        return util::EXIT_USAGE;
    };

    let response = match httpc::fetch(&url) {
        Ok(response) => response,
        Err(error) => return report_error(error),
    };

    let target = match output {
        Some(name) => String::from(name),
        None => url.default_name(),
    };
    if target == "-" {
        if let Err(err) = util::write_all(1, &response.body) {
            eprintln!("wget: write error: {err}");
            return util::EXIT_FAILURE;
        }
    } else {
        let file = match File::create(&target) {
            Ok(file) => file,
            Err(err) => {
                util::report("wget", &target, err);
                return util::EXIT_FAILURE;
            }
        };
        if let Err(err) = util::write_all(file.as_raw_fd(), &response.body) {
            util::report("wget", &target, err);
            return util::EXIT_FAILURE;
        }
    }

    eprintln!("wget: saved {target} ({} bytes)", response.body.len());
    util::EXIT_OK
}

fn report_error(error: Error) -> i32 {
    match error {
        Error::Connect(err) => eprintln!("wget: connect: {err}"),
        Error::Send(err) => eprintln!("wget: send: {err}"),
        Error::Receive(err) => eprintln!("wget: receive: {err}"),
        Error::Malformed => eprintln!("wget: malformed HTTP response"),
        Error::Http(code, reason) => {
            if reason.is_empty() {
                eprintln!("wget: HTTP {code}");
            } else {
                eprintln!("wget: HTTP {code} {reason}");
            }
        }
    }
    util::EXIT_FAILURE
}
