//! `uname` — print the kernel identity from the `uname` syscall.
//!
//! Options are the five fields of the ABI's `struct Uname`: `-s` sysname,
//! `-n` nodename, `-r` release, `-v` version, `-m` machine, plus `-a` for all
//! five. With no option only the sysname is printed; selected fields are
//! printed in the fixed `s n r v m` order, separated by spaces (GNU shape).
//! Unknown options and operands are a usage error (2); a failed syscall is a
//! runtime error (1).

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::{eprintln, sys, Uname};

use crate::util;

const USAGE: &str = "usage: uname [-snrvma]";

pub fn run(args: &[String]) -> i32 {
    let mut selected = [false; 5];
    let mut all = false;
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            for option in arg[1..].chars() {
                match option {
                    's' => selected[0] = true,
                    'n' => selected[1] = true,
                    'r' => selected[2] = true,
                    'v' => selected[3] = true,
                    'm' => selected[4] = true,
                    'a' => all = true,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut info = Uname::default();
    if let Err(err) = sys::uname(&mut info) {
        util::report("uname", "uname", err);
        return util::EXIT_FAILURE;
    }

    let fields = [
        info.sysname(),
        info.nodename(),
        info.release(),
        info.version(),
        info.machine(),
    ];
    let mut parts: Vec<&str> = Vec::new();
    if all {
        parts.extend(fields.iter().copied());
    } else if selected.iter().all(|flag| !*flag) {
        parts.push(fields[0]);
    } else {
        for (index, field) in fields.iter().enumerate() {
            if selected[index] {
                parts.push(*field);
            }
        }
    }

    if let Err(err) = util::write_line(1, parts.join(" ").as_bytes()) {
        eprintln!("uname: write error: {err}");
        return util::EXIT_FAILURE;
    }
    util::EXIT_OK
}