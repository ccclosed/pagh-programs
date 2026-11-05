//! `hostname` — print the `nodename` field of `uname`.
//!
//! There is no sethostname syscall and no `-s`/`-f`/`-d` policy: the ABI exposes
//! exactly one fixed 16-byte nodename. Operands are a usage error (2).

use alloc::string::String;

use libpagh::{eprintln, sys, Uname};

use crate::util;

const USAGE: &str = "usage: hostname";

pub fn run(args: &[String]) -> i32 {
    if !args.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut info = Uname::default();
    if let Err(err) = sys::uname(&mut info) {
        util::report("hostname", "uname", err);
        return util::EXIT_FAILURE;
    }
    if let Err(err) = util::write_line(1, info.nodename().as_bytes()) {
        eprintln!("hostname: write error: {err}");
        return util::EXIT_FAILURE;
    }
    util::EXIT_OK
}