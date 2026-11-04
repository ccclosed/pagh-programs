//! `pwd` — print the working directory.
//!
//! The kernel has no `getcwd`; the program's `PWD` environment variable is the
//! working directory by the entry contract. `libpagh::env::current_dir` falls
//! back to `/` if `PWD` is absent or not absolute (documented in the API
//! README). `-L`/`-P` are accepted and have the same lexical result because no
//! symlink resolution is possible without `stat`-based tree walking.

use libpagh::{eprintln, println};

use crate::util;

const USAGE: &str = "usage: pwd [-L|-P]";

pub fn run(args: &[alloc::string::String]) -> i32 {
    let mut options = true;
    for arg in args {
        if options && arg == "--" {
            options = false;
            continue;
        }
        if options && (arg == "-L" || arg == "-P") {
            continue;
        }
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    println!("{}", libpagh::env::current_dir());
    util::EXIT_OK
}