//! `unexpand` — turn spaces into tabs.
//!
//! Without options only the leading blanks of each line are converted; `-a`
//! converts every run of spaces. The tab stop is fixed at 8 columns (the
//! default in the rest of the toolbox); `-t` is not implemented. Existing tabs
//! are preserved and advance the column to the next stop.
//!
//! A run of spaces becomes tabs for every full stop it reaches and spaces for
//! the remainder, so `\t` is only emitted where a real tab reproduces the same
//! column. Byte-oriented; no UTF-8 column handling.
//!
//! Exit: 0 success, 1 open/read/write/memory error, 2 usage error.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: unexpand [-a] [FILE...]";
const TAB: usize = 8;

pub fn run(args: &[String]) -> i32 {
    let mut all = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            for option in arg[1..].chars() {
                match option {
                    'a' => all = true,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
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
                util::report("unexpand", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        for line in util::lines(&data) {
            let mut out: Vec<u8> = Vec::new();
            if out.try_reserve(line.len()).is_err() {
                eprintln!("unexpand: out of memory");
                return util::EXIT_FAILURE;
            }
            convert(line, all, &mut out);
            if let Err(err) = util::write_line(1, &out) {
                eprintln!("unexpand: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
    }
    status
}

fn convert(line: &[u8], all: bool, out: &mut Vec<u8>) {
    let mut column = 0usize;
    let mut leading = !all;
    let mut index = 0usize;

    while index < line.len() {
        let byte = line[index];
        if byte == b' ' && (leading || all) {
            let start = index;
            while index < line.len() && line[index] == b' ' {
                index += 1;
            }
            emit_spaces(index - start, &mut column, out);
        } else if byte == b'\t' {
            out.push(b'\t');
            index += 1;
            column = next_stop(column);
        } else {
            leading = false;
            out.push(byte);
            index += 1;
            column = column.saturating_add(1);
        }
    }
}

fn next_stop(column: usize) -> usize {
    match column.checked_add(TAB - column % TAB) {
        Some(next) => next,
        None => column,
    }
}

fn emit_spaces(mut count: usize, column: &mut usize, out: &mut Vec<u8>) {
    while count > 0 {
        let next = next_stop(*column);
        let need = next.saturating_sub(*column);
        if need == 0 {
            // Only reachable at usize::MAX, which a real line cannot reach;
            // emit the rest literally rather than loop forever.
            out.extend(core::iter::repeat(b' ').take(count));
            *column = column.saturating_add(count);
            return;
        }
        if count >= need {
            out.push(b'\t');
            *column = next;
            count -= need;
        } else {
            out.extend(core::iter::repeat(b' ').take(count));
            *column = column.saturating_add(count);
            count = 0;
        }
    }
}