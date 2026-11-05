//! `exit` — terminate the process with a status.
//!
//! Usage: `exit [N]`. `N` is a decimal `0..=255` (default 0); anything else is
//! usage error 2. The applet calls `libpagh::sys::exit(N)` directly, so it does
//! not return to the dispatcher: the whole `paghbox` process ends with that
//! code. That is only meaningful when `exit` really is the program entry point
//! (`run /mnt/bin/exit 7` or `paghbox exit 7`); a hypothetical in-process
//! caller would be terminated too, which is why there is no "return" path.
//!
//! The kernel's `exit(2)` status is an `i32`, but POSIX convention — and every
//! other applet in this bundle — uses 0..=255, so values outside that range are
//! rejected as usage rather than silently truncated.

use alloc::string::String;

use libpagh::{eprintln, sys};

use crate::util;

const USAGE: &str = "usage: exit [N]";

pub fn run(args: &[String]) -> i32 {
    if args.len() > 1 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let code = match args.first() {
        None => 0,
        Some(text) => match text.parse::<i32>() {
            Ok(value) if (0..=255).contains(&value) => value,
            _ => {
                eprintln!("exit: invalid status: '{text}'");
                return util::EXIT_USAGE;
            }
        },
    };
    sys::exit(code)
}