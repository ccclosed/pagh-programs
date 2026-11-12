//! `uuidgen` — print RFC 4122 version-4 UUIDs from the kernel entropy pool.
//!
//! ```text
//! uuidgen [-r] [-n N]
//! ```
//!
//! Each UUID is 16 bytes from [`libpagh::random`] (`getrandom`, ABI 278) with
//! the version nibble forced to 4 and the variant bits to `10`. The kernel's
//! SHA-256 pool is the same source the kernel TLS client uses for session keys;
//! the applet neither seeds nor mixes in a clock or a pid, so the output is not
//! predictable from boot time.
//!
//! Fail-closed: when the pool has no seed (kernel refuses `getrandom` with
//! `EAGAIN`) this applet exits 1 with the ABI error instead of falling back to a
//! counter or the clock. A UUID is an identifier, not a key, but silently
//! substituting a weak source is exactly how a "random" identifier becomes a
//! fingerprint, and `libpagh` documents the pool as the one source.
//!
//! `-n N` prints N UUIDs, one per line; `-n 0` prints nothing and exits 0.
//! `-r` is accepted for command-line compatibility and is the only mode (there
//! is no name-based variant here).
//!
//! Exit: 0 success; 1 entropy/write error; 2 usage or an invalid `-n`.

use alloc::string::String;

use libpagh::eprintln;
use libpagh::random;

use crate::util;

const USAGE: &str = "usage: uuidgen [-r] [-n N]";

pub fn run(args: &[String]) -> i32 {
    let mut count = 1usize;
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && (arg == "-r" || arg == "--random") {
            continue;
        }
        if parse_options && (arg == "-n" || arg == "--count") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match util::parse_usize(value) {
                Some(parsed) => count = parsed,
                None => {
                    eprintln!("uuidgen: invalid count: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-n") {
            match util::parse_usize(&arg[2..]) {
                Some(parsed) => count = parsed,
                None => {
                    eprintln!("uuidgen: invalid count: '{}'", &arg[2..]);
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    for _ in 0..count {
        let mut bytes = [0u8; 16];
        if let Err(err) = random::fill(&mut bytes) {
            eprintln!("uuidgen: entropy pool unavailable: {err}");
            return util::EXIT_FAILURE;
        }
        bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
        bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant

        let text = format_uuid(&bytes);
        if let Err(err) = util::write_line(1, text.as_bytes()) {
            eprintln!("uuidgen: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    util::EXIT_OK
}

fn format_uuid(bytes: &[u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push(HEX[((*byte >> 4) & 0x0f) as usize] as char);
        out.push(HEX[(*byte & 0x0f) as usize] as char);
    }
    out
}