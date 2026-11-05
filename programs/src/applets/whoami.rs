//! `whoami` — print the effective user name.
//!
//! The answer is always `root`: the pagh ABI has no `getuid`/`geteuid`
//! syscall, and the kernel runs user processes with the single identity it
//! boots with. This is a real command for scripts that expect `whoami`, not a
//! placeholder; when a uid syscall exists the lookup can be added here without
//! changing the interface.
//!
//! No options and no operands (an operand is a usage error, exit 2). Output
//! goes to fd 1; a write failure is reported on fd 2 and exits 1.

use alloc::string::String;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: whoami";

pub fn run(args: &[String]) -> i32 {
    if !args.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    match util::write_line(1, b"root") {
        Ok(()) => util::EXIT_OK,
        Err(err) => {
            eprintln!("whoami: write error: {err}");
            util::EXIT_FAILURE
        }
    }
}
