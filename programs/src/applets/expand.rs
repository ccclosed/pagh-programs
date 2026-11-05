//! `expand` — turn tabs into spaces.
//!
//! `-t N` sets the tab stop width (default 8). Tabs are expanded to the next
//! multiple of N columns, not replaced by a fixed N spaces, so `\tx` at column
//! 0 becomes N spaces then `x`. Other bytes are copied unchanged (including
//! CR); the tab stop is counted in bytes.
//!
//! A tab width of zero or above 1,000,000 is rejected as a usage error: without
//! a cap a hostile `-t` value would make a single tab consume unbounded memory
//! and time. Missing operands mean stdin; several operands are read in order.
//!
//! Exit: 0 success, 1 open/read/write/memory error, 2 usage error.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: expand [-t N] [FILE...]";
const MAX_TAB: usize = 1_000_000;

pub fn run(args: &[String]) -> i32 {
    let mut tabstop = 8usize;
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
        if parse_options && arg == "-t" {
            index += 1;
            let Some(value) = args.get(index) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            match parse_tab(value) {
                Some(parsed) => tabstop = parsed,
                None => {
                    eprintln!("expand: invalid tab size: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-t") {
            match parse_tab(&arg[2..]) {
                Some(parsed) => tabstop = parsed,
                None => {
                    eprintln!("expand: invalid tab size: '{}'", &arg[2..]);
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
                util::report("expand", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        for line in util::lines(&data) {
            let mut out: Vec<u8> = Vec::new();
            if out.try_reserve(line.len().saturating_add(tabstop)).is_err() {
                eprintln!("expand: out of memory");
                return util::EXIT_FAILURE;
            }
            let mut column = 0usize;
            for byte in line {
                if *byte == b'\t' {
                    let spaces = tabstop - column % tabstop;
                    if out.try_reserve(spaces).is_err() {
                        eprintln!("expand: out of memory");
                        return util::EXIT_FAILURE;
                    }
                    out.resize(out.len() + spaces, b' ');
                    column = column.saturating_add(spaces);
                } else {
                    out.push(*byte);
                    column = column.saturating_add(1);
                }
            }
            if let Err(err) = util::write_line(1, &out) {
                eprintln!("expand: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
    }
    status
}

fn parse_tab(text: &str) -> Option<usize> {
    match util::parse_usize(text) {
        Some(value) if value >= 1 && value <= MAX_TAB => Some(value),
        _ => None,
    }
}