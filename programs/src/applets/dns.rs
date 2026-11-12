//! `dns` — resolve one IPv4 `A` record through the ABI's UDP socket.
//!
//! ```text
//! dns [-s SERVER] HOST
//! ```
//!
//! `HOST` is a dotted DNS name (an IPv4 literal is accepted and echoed without
//! touching the network). `-s SERVER` overrides the resolver; the default is
//! [`libpagh::dns::DEFAULT_SERVER`] (`10.0.2.3`), which is where QEMU's user
//! network forwards DNS — the ABI has no `getsockopt(IP_DNS)` and no
//! `/etc/resolv.conf`, so the address is a parameter, not a discovery result.
//!
//! The lookup is the same one [`libpagh::dns`] documents: one `A` query over
//! UDP, one datagram reply, no `AAAA`, no `CNAME` chasing, no caching and **no
//! DNSSEC validation**. It is a name-to-address helper for a single-user
//! machine, not a trust boundary; a client that cares about authenticity checks
//! a certificate against the *name* afterwards (which is exactly what
//! `curl`/`wget` do for `https://`).
//!
//! Output is one line, `HOST -> ADDRESS`, so a script (and the live transcript)
//! can match it exactly.
//!
//! Exit: 0 success; 1 resolve/socket error; 2 usage.

use alloc::string::String;

use libpagh::dns::{self as resolver, DEFAULT_SERVER};
use libpagh::eprintln;
use libpagh::net::Ipv4;
use libpagh::println;

use crate::util;

const USAGE: &str = "usage: dns [-s SERVER] HOST";

pub fn run(args: &[String]) -> i32 {
    let mut server = DEFAULT_SERVER;
    let mut host: Option<&str> = None;
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg == "-s" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match Ipv4::parse(value) {
                Some(parsed) => server = parsed,
                None => {
                    eprintln!("dns: invalid resolver: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-s") {
            let value = &arg[2..];
            match Ipv4::parse(value) {
                Some(parsed) => {
                    server = parsed;
                    continue;
                }
                None => {
                    eprintln!("dns: invalid resolver: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if host.is_some() {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        host = Some(arg);
    }

    let Some(host) = host else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };

    // A literal is already the answer; asking DNS about it would be a wasted
    // datagram (and QEMU's resolver answers it the same way anyway).
    if let Some(address) = Ipv4::parse(host) {
        println!("{host} -> {address}");
        return util::EXIT_OK;
    }

    match resolver::resolve(host, server) {
        Ok(address) => {
            println!("{host} -> {address}");
            util::EXIT_OK
        }
        Err(err) => {
            eprintln!("dns: {host}: {err}");
            util::EXIT_FAILURE
        }
    }
}