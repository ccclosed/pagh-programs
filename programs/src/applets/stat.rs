//! `stat` — print the ABI metadata of each operand.
//!
//! The format is pagh's own, not GNU `stat`'s:
//!
//! ```text
//! path: file, size=35, mode=0o644, ino=17
//! ```
//!
//! `mode` is printed with Rust's `0o` prefix (the low permission bits are the
//! useful part). A missing operand is reported on fd 2 and the remaining
//! operands are still printed; the status is then 1. No operands is a usage
//! error (2). There is no `-c` format string and no `lstat` in the ABI, so a
//! symlink is followed by `fs::stat`.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;

use crate::util;

const USAGE: &str = "usage: stat FILE...";

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

    let mut status = util::EXIT_OK;
    for name in operands {
        match fs::stat(name) {
            Ok(stat) => {
                let line = format!(
                    "{name}: {}, size={}, mode={:#o}, ino={}",
                    util::kind_name(stat.entry_kind()),
                    stat.size,
                    stat.mode,
                    stat.ino
                );
                if let Err(err) = util::write_line(1, line.as_bytes()) {
                    eprintln!("stat: write error: {err}");
                    return util::EXIT_FAILURE;
                }
            }
            Err(err) => {
                util::report("stat", name, err);
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}