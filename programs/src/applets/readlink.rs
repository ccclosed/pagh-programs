//! `readlink` — print the target of a symbolic link.
//!
//! Exactly one operand. The target is printed as stored (no normalization, no
//! trailing newline suppression) followed by a newline. The ABI has no `lstat`,
//! so a non-link is detected by `readlinkat` failing; the error is reported and
//! the exit status is 1. Options are a usage error (2).

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::{eprintln, fs};

use crate::util;

const USAGE: &str = "usage: readlink PATH";

pub fn run(args: &[String]) -> i32 {
    let mut operands: Vec<&str> = Vec::new();
    let mut options = true;
    for arg in args {
        if options && arg == "--" {
            options = false;
            continue;
        }
        if options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        operands.push(arg.as_str());
    }

    if operands.len() != 1 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let name = operands[0];

    match fs::read_link(name) {
        Ok(target) => {
            if let Err(err) = util::write_line(1, target.as_bytes()) {
                eprintln!("readlink: write error: {err}");
                return util::EXIT_FAILURE;
            }
            util::EXIT_OK
        }
        Err(err) => {
            util::report("readlink", name, err);
            util::EXIT_FAILURE
        }
    }
}