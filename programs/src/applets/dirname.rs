//! `dirname` — strip the last component from a path.
//!
//! Lexical only. `/a` is `/`, `a` is `.`; trailing slashes are ignored.
//!
//! Exit: 0, or 2 on missing/extra operands or an unknown option.

use libpagh::{eprintln, path, println};

use crate::util;

const USAGE: &str = "usage: dirname PATH";

pub fn run(args: &[alloc::string::String]) -> i32 {
    let mut operand: Option<&str> = None;
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
        if operand.is_some() {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        operand = Some(arg.as_str());
    }

    let Some(operand) = operand else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };

    println!("{}", path::dirname(operand));
    util::EXIT_OK
}