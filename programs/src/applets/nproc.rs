//! `nproc` — print the number of usable CPUs.
//!
//! pagh is a single-hart cooperative kernel with one user task at a time, so the
//! answer is always `1`. There is no `sched_getaffinity` syscall and no
//! `--all`/`--ignore=N` policy to implement. An operand is a usage error (2).

use alloc::string::String;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: nproc";

pub fn run(args: &[String]) -> i32 {
    if !args.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    if let Err(err) = util::write_line(1, b"1") {
        eprintln!("nproc: write error: {err}");
        return util::EXIT_FAILURE;
    }
    util::EXIT_OK
}