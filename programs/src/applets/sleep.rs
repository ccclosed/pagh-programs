//! `sleep` — wait for a duration.
//!
//! The operand is a non-negative number of seconds, either an integer (`2`) or
//! a decimal with up to nine fractional digits (`0.5`, `1.25`). It is converted
//! to a [`libpagh::Timespec`] and handed to `nanosleep`; the kernel rounds the
//! wait up to its next 10 ms tick, so a short sleep may last one tick longer.
//!
//! No suffixes (`1m`, `1h`), no `infinity`, no multiple operands. A bad or
//! missing duration is a usage error (2); a failed syscall is a runtime error
//! (1). There is no `--`-style option and no `sleep` option in this subset.

use alloc::string::String;

use libpagh::{eprintln, sys, Timespec};

use crate::util;

const USAGE: &str = "usage: sleep SECONDS";

pub fn run(args: &[String]) -> i32 {
    if args.len() != 1 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let text = args[0].as_str();
    let Some(request) = parse_duration(text) else {
        eprintln!("sleep: invalid time interval: '{text}'");
        return util::EXIT_USAGE;
    };

    if let Err(err) = sys::nanosleep(&request) {
        util::report("sleep", text, err);
        return util::EXIT_FAILURE;
    }
    util::EXIT_OK
}

/// Parse `SECONDS` or `SECONDS.FRACTION` without floating point.
fn parse_duration(text: &str) -> Option<Timespec> {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let has_whole = !whole.is_empty();
    let has_fraction = fraction.map_or(false, |part| !part.is_empty());
    if !has_whole && !has_fraction {
        return None;
    }

    let seconds: i64 = if whole.is_empty() {
        0
    } else {
        if !whole.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        whole.parse::<i64>().ok()?
    };

    let mut nanos: i64 = 0;
    if let Some(fraction) = fraction {
        if fraction.len() > 9 {
            return None;
        }
        let mut scale: i64 = 100_000_000;
        for byte in fraction.bytes() {
            if !byte.is_ascii_digit() {
                return None;
            }
            nanos += i64::from(byte - b'0') * scale;
            scale /= 10;
        }
    }

    Some(Timespec { seconds, nanos })
}