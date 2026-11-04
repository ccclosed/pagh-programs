//! `seq` — print an integer sequence.
//!
//! Forms: `seq LAST`, `seq FIRST LAST`, `seq FIRST INCR LAST`, with the
//! optional `-s SEP` separator. Integers only (no floating point in the ABI
//! runtime and no `%g`); a zero increment is an error. Negative values work.
//!
//! Exit: 0 on success, 1 for an invalid/zero number, 2 for a usage error.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: seq [-s SEP] [FIRST [INCR]] LAST";

pub fn run(args: &[String]) -> i32 {
    let mut separator = String::from("\n");
    let mut operands: Vec<&str> = Vec::new();
    let mut options = true;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        if options && arg == "--" {
            options = false;
            index += 1;
            continue;
        }
        if options && arg == "-s" {
            index += 1;
            match args.get(index) {
                Some(value) => separator = value.clone(),
                None => {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if options && arg.starts_with('-') && arg.len() > 1 && util::parse_i64(arg).is_none() {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        operands.push(arg.as_str());
        index += 1;
    }

    let numbers: Vec<i64> = match operands
        .iter()
        .map(|text| {
            util::parse_i64(text).ok_or_else(|| {
                eprintln!("seq: invalid number: '{text}'");
                util::EXIT_FAILURE
            })
        })
        .collect::<Result<_, i32>>()
    {
        Ok(numbers) => numbers,
        Err(code) => return code,
    };

    let (first, increment, last) = match numbers.as_slice() {
        [last] => (1, 1, *last),
        [first, last] => (*first, 1, *last),
        [first, increment, last] => (*first, *increment, *last),
        _ => {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
    };

    if increment == 0 {
        eprintln!("seq: increment must not be zero");
        return util::EXIT_FAILURE;
    }

    let mut current = first;
    let mut wrote_any = false;
    loop {
        let in_range = if increment > 0 {
            current <= last
        } else {
            current >= last
        };
        if !in_range {
            break;
        }

        if wrote_any {
            if let Err(err) = util::write_all(1, separator.as_bytes()) {
                eprintln!("seq: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
        let line = format!("{current}");
        if let Err(err) = util::write_all(1, line.as_bytes()) {
            eprintln!("seq: write error: {err}");
            return util::EXIT_FAILURE;
        }
        wrote_any = true;

        current = match current.checked_add(increment) {
            Some(next) => next,
            None => break,
        };
    }

    if wrote_any {
        if let Err(err) = util::write_all(1, b"\n") {
            eprintln!("seq: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    util::EXIT_OK
}