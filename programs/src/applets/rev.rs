//! `rev` — reverse the bytes of each line.
//!
//! This is the byte-oriented subset: UTF-8 characters are not decoded, so a
//! multi-byte character comes out with its bytes reversed. Lines are split on
//! LF; CR stays part of the line. Every output line is newline-terminated, even
//! if the input's last line was not.
//!
//! Several files are read in order, or stdin when no operand is given. Missing
//! files are reported and the remaining files are still processed (exit 1).
//! There are no options (an option is a usage error, 2).

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: rev [FILE...]";

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
        operands.push(arg.as_str());
    }

    if operands.is_empty() {
        operands.push("-");
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("rev", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        for line in util::lines(&data) {
            let mut reversed: Vec<u8> = Vec::new();
            if reversed.try_reserve(line.len()).is_err() {
                eprintln!("rev: out of memory");
                return util::EXIT_FAILURE;
            }
            reversed.extend(line.iter().rev().copied());
            if let Err(err) = util::write_line(1, &reversed) {
                eprintln!("rev: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
    }
    status
}