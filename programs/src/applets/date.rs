//! `date` — print the kernel clock as an epoch timestamp.
//!
//! The output is deliberately not a calendar date:
//!
//! ```text
//! epoch=1234.567000000
//! ```
//!
//! pagh has no wall-clock source and no timezone database; clock 0 is the same
//! tick clock as clock 1, so this is "seconds since boot, with nanoseconds"
//! despite the ABI calling clock 0 realtime. Formatting a human date would
//! require a timezone and leap-second policy that the platform does not have.
//! The fractional field is always nine digits.
//!
//! An operand is a usage error (2); a failed syscall exits 1.

use alloc::format;
use alloc::string::String;

use libpagh::{eprintln, sys, Timespec};

use crate::util;

const USAGE: &str = "usage: date";

pub fn run(args: &[String]) -> i32 {
    if !args.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut now = Timespec::default();
    if let Err(err) = sys::clock_gettime(sys::CLOCK_REALTIME, &mut now) {
        util::report("date", "clock_gettime", err);
        return util::EXIT_FAILURE;
    }
    let line = format!("epoch={}.{:09}", now.seconds.max(0), now.nanos.max(0));
    if let Err(err) = util::write_line(1, line.as_bytes()) {
        eprintln!("date: write error: {err}");
        return util::EXIT_FAILURE;
    }
    util::EXIT_OK
}