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

use libpagh::{eprintln, sys};

use crate::util;

const USAGE: &str = "usage: sleep SECONDS";

pub fn run(args: &[String]) -> i32 {
    if args.len() != 1 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let text = args[0].as_str();
    let Some(request) = util::parse_duration(text) else {
        eprintln!("sleep: invalid time interval: '{text}'");
        return util::EXIT_USAGE;
    };

    if let Err(err) = sys::nanosleep(&request) {
        util::report("sleep", text, err);
        return util::EXIT_FAILURE;
    }
    util::EXIT_OK
}