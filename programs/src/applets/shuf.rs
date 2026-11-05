//! `shuf` — shuffle lines, arguments or an integer range.
//!
//! Modes:
//!
//! * no mode: the lines of one file operand, or stdin;
//! * `-e [ARG...]`: the arguments themselves are the lines;
//! * `-i LO-HI`: the inclusive integer range is the input.
//!
//! `-n N` prints only the first N lines after shuffling. Fisher-Yates is
//! driven by a **fixed-seed xorshift64\*** generator (`0x9E3779B97F4A7C15`,
//! shifts 12/25/27, multiplier `0x2545F4914F6CDD1D`), because the ABI has no
//! `getrandom` and no clock in the frozen surface this applet targets. The same
//! input therefore always produces the same order; that is a documented
//! property, not an accident, and it makes the live test reproducible. There is
//! no `--random-source` or `-r`.
//!
//! Exit: 0 success, 1 read/write/memory error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: shuf [-n N] [-e ARG...] [-i LO-HI] [FILE...]";
const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

pub fn run(args: &[String]) -> i32 {
    let mut echo = false;
    let mut range: Option<(i64, i64)> = None;
    let mut count: Option<usize> = None;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg == "-e" {
            echo = true;
            continue;
        }
        if parse_options && arg == "-n" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match util::parse_usize(value) {
                Some(parsed) => count = Some(parsed),
                None => {
                    eprintln!("shuf: invalid count: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-n") {
            match util::parse_usize(&arg[2..]) {
                Some(parsed) => count = Some(parsed),
                None => {
                    eprintln!("shuf: invalid count: '{}'", &arg[2..]);
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg == "-i" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match parse_range(value) {
                Some(parsed) => range = Some(parsed),
                None => {
                    eprintln!("shuf: invalid range: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-i") {
            match parse_range(&arg[2..]) {
                Some(parsed) => range = Some(parsed),
                None => {
                    eprintln!("shuf: invalid range: '{}'", &arg[2..]);
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("shuf: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if echo && range.is_some() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    if range.is_some() && !operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    if !echo && range.is_none() && operands.len() > 1 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut lines: Vec<Vec<u8>> = Vec::new();
    if let Some((low, high)) = range {
        let span = match high.checked_sub(low) {
            Some(span) => match span.checked_add(1) {
                Some(span) => span,
                None => {
                    eprintln!("shuf: range too large");
                    return util::EXIT_FAILURE;
                }
            },
            None => {
                eprintln!("shuf: invalid range");
                return util::EXIT_USAGE;
            }
        };
        let Ok(span) = usize::try_from(span) else {
            eprintln!("shuf: range too large");
            return util::EXIT_FAILURE;
        };
        if lines.try_reserve(span).is_err() {
            eprintln!("shuf: out of memory");
            return util::EXIT_FAILURE;
        }
        let mut value = low;
        for _ in 0..span {
            let text = alloc::format!("{value}");
            let mut bytes: Vec<u8> = Vec::new();
            if bytes.try_reserve(text.len()).is_err() {
                eprintln!("shuf: out of memory");
                return util::EXIT_FAILURE;
            }
            bytes.extend_from_slice(text.as_bytes());
            lines.push(bytes);
            value = match value.checked_add(1) {
                Some(next) => next,
                None => break,
            };
        }
    } else if echo {
        if lines.try_reserve(operands.len()).is_err() {
            eprintln!("shuf: out of memory");
            return util::EXIT_FAILURE;
        }
        for operand in operands {
            let mut bytes: Vec<u8> = Vec::new();
            if bytes.try_reserve(operand.len()).is_err() {
                eprintln!("shuf: out of memory");
                return util::EXIT_FAILURE;
            }
            bytes.extend_from_slice(operand.as_bytes());
            lines.push(bytes);
        }
    } else {
        let name = operands.first().copied().unwrap_or("-");
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("shuf", util::display_name(name), err);
                return util::EXIT_FAILURE;
            }
        };
        for line in util::lines(&data) {
            let mut bytes: Vec<u8> = Vec::new();
            if bytes.try_reserve(line.len()).is_err() {
                eprintln!("shuf: out of memory");
                return util::EXIT_FAILURE;
            }
            bytes.extend_from_slice(line);
            if util::push(&mut lines, bytes).is_err() {
                eprintln!("shuf: out of memory");
                return util::EXIT_FAILURE;
            }
        }
    }

    shuffle(&mut lines);

    let take = match count {
        Some(count) => count.min(lines.len()),
        None => lines.len(),
    };
    for line in lines.iter().take(take) {
        if let Err(err) = util::write_line(1, line) {
            eprintln!("shuf: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    util::EXIT_OK
}

fn shuffle(lines: &mut [Vec<u8>]) {
    let mut state = SEED;
    for index in (1..lines.len()).rev() {
        let pick = (next_random(&mut state) % (index as u64 + 1)) as usize;
        lines.swap(index, pick);
    }
}

fn next_random(state: &mut u64) -> u64 {
    let mut value = *state;
    value ^= value >> 12;
    value ^= value << 25;
    value ^= value >> 27;
    *state = value;
    value.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

fn parse_range(text: &str) -> Option<(i64, i64)> {
    let bytes = text.as_bytes();
    let split = (1..bytes.len()).find(|index| bytes[*index] == b'-')?;
    let low = text.get(..split)?.parse::<i64>().ok()?;
    let high = text.get(split + 1..)?.parse::<i64>().ok()?;
    if low > high {
        return None;
    }
    Some((low, high))
}