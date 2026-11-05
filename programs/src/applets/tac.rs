//! `tac` — print each file's lines in reverse order.
//!
//! The last line of a file needs no trailing newline; every printed line gets
//! one, like `rev` and the other line filters here. CR is part of the line.
//! Each operand is reversed separately and printed in command-line order;
//! without operands the whole stdin is one stream.
//!
//! There is no `-b`/`-r` separator mode and no `--`-less GNU `-s`. An option is
//! a usage error (2). Read/write failures are reported per operand and the
//! exit status is 1; the remaining operands are still processed.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: tac [FILE...]";

pub fn run(args: &[String]) -> i32 {
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("tac: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        operands.push("-");
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("tac", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };

        let mut lines: Vec<&[u8]> = Vec::new();
        for line in util::lines(&data) {
            if util::push(&mut lines, line).is_err() {
                eprintln!("tac: out of memory");
                return util::EXIT_FAILURE;
            }
        }
        for line in lines.iter().rev() {
            if let Err(err) = util::write_line(1, line) {
                eprintln!("tac: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
    }
    status
}