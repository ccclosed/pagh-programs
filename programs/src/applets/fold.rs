//! `fold` — wrap long lines at a fixed width.
//!
//! `-w N` sets the width (default 80); a width of zero is a usage error. The
//! count is in bytes, not display columns: a UTF-8 character is not treated
//! specially and a tab counts as one byte. No `-s` (break at spaces) and no
//! byte/character modes — the minimal byte-oriented subset.
//!
//! Input is one or more files, or stdin when there are no operands. Each line,
//! including an empty one, is terminated by a newline; a final input line
//! without a newline still gets one.
//!
//! Exit: 0 success, 1 open/read/write error, 2 usage error.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: fold [-w N] [FILE...]";

pub fn run(args: &[String]) -> i32 {
    let mut width = 80usize;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        if parse_options && arg == "--" {
            parse_options = false;
            index += 1;
            continue;
        }
        if parse_options && arg == "-w" {
            index += 1;
            let Some(value) = args.get(index) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            match parse_width(value) {
                Some(parsed) => width = parsed,
                None => {
                    eprintln!("fold: invalid number of columns: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-w") {
            match parse_width(&arg[2..]) {
                Some(parsed) => width = parsed,
                None => {
                    eprintln!("fold: invalid number of columns: '{}'", &arg[2..]);
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        operands.push(arg.as_str());
        index += 1;
    }

    if operands.is_empty() {
        operands.push("-");
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("fold", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        for line in util::lines(&data) {
            let mut start = 0usize;
            loop {
                let end = start.saturating_add(width).min(line.len());
                if let Err(err) = util::write_line(1, &line[start..end]) {
                    eprintln!("fold: write error: {err}");
                    return util::EXIT_FAILURE;
                }
                start = end;
                if start >= line.len() {
                    break;
                }
            }
        }
    }
    status
}

fn parse_width(text: &str) -> Option<usize> {
    match util::parse_usize(text) {
        Some(0) | None => None,
        Some(width) => Some(width),
    }
}