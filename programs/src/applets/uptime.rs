//! `uptime` — print how long the kernel has been up.
//!
//! The machine has no wall-clock source; the ABI's clock 0 and clock 1 are both
//! the kernel tick clock. This applet asks for clock 1 (monotonic) and prints
//! `up <seconds>s` — no load average, no user count, no `-p`/`-s` policy, since
//! there is no scheduler accounting to report.
//!
//! An operand is a usage error (2); a failed syscall exits 1.

use alloc::format;
use alloc::string::String;

use libpagh::{eprintln, sys, Timespec};

use crate::util;

const USAGE: &str = "usage: uptime";

pub fn run(args: &[String]) -> i32 {
    if !args.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut now = Timespec::default();
    if let Err(err) = sys::clock_gettime(sys::CLOCK_MONOTONIC, &mut now) {
        util::report("uptime", "clock_gettime", err);
        return util::EXIT_FAILURE;
    }
    let line = format!("up {}s", now.seconds.max(0));
    if let Err(err) = util::write_line(1, line.as_bytes()) {
        eprintln!("uptime: write error: {err}");
        return util::EXIT_FAILURE;
    }
    util::EXIT_OK
}