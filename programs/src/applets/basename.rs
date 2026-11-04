//! `basename` — strip the directory (and optionally a suffix) from a path.
//!
//! Lexical only: no filesystem access, so `basename` works on paths that do not
//! exist. Trailing slashes are ignored; `basename /` is `/`.
//!
//! Exit: 0, or 2 on missing/extra operands or an unknown option.

use alloc::vec::Vec;

use libpagh::{eprintln, path, println};

use crate::util;

const USAGE: &str = "usage: basename PATH [SUFFIX]";

pub fn run(args: &[alloc::string::String]) -> i32 {
    let operands = match operands(args) {
        Ok(operands) => operands,
        Err(code) => return code,
    };
    if operands.is_empty() || operands.len() > 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let name = path::basename(operands[0]);
    let name = match operands.get(1) {
        // POSIX: do not delete a suffix that is the whole name.
        Some(suffix)
            if !suffix.is_empty()
                && name.len() > suffix.len()
                && name.ends_with(*suffix) =>
        {
            &name[..name.len() - suffix.len()]
        }
        _ => name,
    };

    println!("{name}");
    util::EXIT_OK
}

/// Collect operands, rejecting options. `--` ends option processing.
fn operands(args: &[alloc::string::String]) -> Result<Vec<&str>, i32> {
    let mut operands = Vec::new();
    let mut options = true;
    for arg in args {
        if options && arg == "--" {
            options = false;
            continue;
        }
        if options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return Err(util::EXIT_USAGE);
        }
        operands.push(arg.as_str());
    }
    Ok(operands)
}