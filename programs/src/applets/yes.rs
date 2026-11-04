//! `yes` — repeat a line forever.
//!
//! With no arguments the line is `y`; otherwise the arguments are joined with
//! one space, exactly once, and the newline is appended to the cached buffer.
//! The loop only ends if fd 1 reports an error (there is no SIGPIPE in the ABI),
//! in which case the applet exits 1.

use alloc::string::String;

use libpagh::eprintln;

use crate::util;

pub fn run(args: &[String]) -> i32 {
    let mut line = String::new();
    if args.is_empty() {
        line.push('y');
    } else {
        for (index, arg) in args.iter().enumerate() {
            if index > 0 {
                line.push(' ');
            }
            line.push_str(arg);
        }
    }
    line.push('\n');
    let bytes = line.into_bytes();

    loop {
        if let Err(err) = util::write_all(1, &bytes) {
            eprintln!("yes: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
}