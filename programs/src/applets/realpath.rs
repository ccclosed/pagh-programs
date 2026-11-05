//! `realpath` — print a normalized absolute path.
//!
//! This is **lexical** resolution via [`libpagh::path::resolve`]: a relative
//! operand is joined onto `PWD`, then `.`, `..` and duplicate slashes are
//! normalized (extra `..` at `/` is dropped). The ABI has no `realpath`-style
//! syscall and no way to ask for a symlink-free path, so links are **not**
//! followed and the operand need not exist. That matches `realpath -m`; it is
//! documented here because it differs from GNU `realpath` without `-m`.
//!
//! One or more operands, one line each. No operands (or an option) is a usage
//! error (2); lexical resolution itself cannot fail.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::path;

use crate::util;

const USAGE: &str = "usage: realpath PATH...";

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

    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    for name in operands {
        let resolved = path::resolve(name);
        if let Err(err) = util::write_line(1, resolved.as_bytes()) {
            eprintln!("realpath: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    util::EXIT_OK
}