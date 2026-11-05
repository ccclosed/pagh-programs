//! `sort` — sort lines lexicographically or numerically.
//!
//! Options: `-r` reverse the result, `-n` compare the leading numeric value,
//! `-u` drop lines that are byte-identical to the previous output line. A
//! numeric comparison falls back to the whole line when the numbers are equal,
//! so the order stays deterministic; a line with no leading number counts as 0.
//! Numeric keys are plain decimals — an optional sign, digits, and an optional
//! fraction — compared exactly with integer arithmetic. There is no exponent
//! syntax, thousands separator, `+inf`, or locale collation (and no FPU use:
//! the kernel does not enable floating point for user mode).
//!
//! Multiple operands are read in order and treated as one stream (a newline is
//! inserted between files when one lacks a trailing newline, so lines never
//! merge across operands). No operands means stdin. Input is read into one
//! `Vec<u8>` and sorted as `(start, end)` ranges, so the line data is stored
//! once; the sort itself uses `sort_by` (stable, in-memory).
//!
//! Pipes and redirection do not exist in the kernel shell, so the result always
//! goes to fd 1. `-o`, `-k`, `-t`, `-f` and locale collation are out of scope.
//!
//! Exit: 0 success, 1 open/read/write or memory error, 2 usage error.

use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: sort [-r] [-n] [-u] [FILE...]";

#[derive(Clone, Copy, Default)]
struct Options {
    reverse: bool,
    numeric: bool,
    unique: bool,
}

pub fn run(args: &[String]) -> i32 {
    let mut options = Options::default();
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
                    'r' => options.reverse = true,
                    'n' => options.numeric = true,
                    'u' => options.unique = true,
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
    let mut data: Vec<u8> = Vec::new();
    let mut first = true;

    for name in &operands {
        match util::read_input(name) {
            Ok(chunk) => {
                if !first && !data.is_empty() && data.last() != Some(&b'\n') {
                    if data.try_reserve(1).is_err() {
                        eprintln!("sort: out of memory");
                        return util::EXIT_FAILURE;
                    }
                    data.push(b'\n');
                }
                if data.try_reserve(chunk.len()).is_err() {
                    eprintln!("sort: out of memory");
                    return util::EXIT_FAILURE;
                }
                data.extend_from_slice(&chunk);
                first = false;
            }
            Err(err) => {
                util::report("sort", util::display_name(name), err);
                status = util::EXIT_FAILURE;
            }
        }
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    while start < data.len() {
        let (end, next) = match data[start..].iter().position(|byte| *byte == b'\n') {
            Some(offset) => (start + offset, start + offset + 1),
            None => (data.len(), data.len()),
        };
        if ranges.try_reserve(1).is_err() {
            eprintln!("sort: out of memory");
            return util::EXIT_FAILURE;
        }
        ranges.push((start, end));
        start = next;
    }

    let numeric = options.numeric;
    let reverse = options.reverse;
    ranges.sort_by(|left, right| {
        let order = compare_lines(&data[left.0..left.1], &data[right.0..right.1], numeric);
        if reverse {
            order.reverse()
        } else {
            order
        }
    });

    let mut previous: Option<&[u8]> = None;
    for &(start, end) in &ranges {
        let line = &data[start..end];
        if options.unique && previous == Some(line) {
            continue;
        }
        if let Err(err) = util::write_line(1, line) {
            eprintln!("sort: write error: {err}");
            return util::EXIT_FAILURE;
        }
        previous = Some(line);
    }

    status
}

fn compare_lines(left: &[u8], right: &[u8], numeric: bool) -> Ordering {
    if numeric {
        let left_number = parse_number(left).unwrap_or(Number::zero());
        let right_number = parse_number(right).unwrap_or(Number::zero());
        let order = compare_numbers(left_number, right_number);
        if order != Ordering::Equal {
            return order;
        }
    }
    left.cmp(right)
}

/// A leading decimal number: optional sign, integer digits, optional fraction.
#[derive(Clone, Copy)]
struct Number<'a> {
    negative: bool,
    integer: &'a [u8],
    fraction: &'a [u8],
}

impl<'a> Number<'a> {
    fn zero() -> Number<'static> {
        Number {
            negative: false,
            integer: &[],
            fraction: &[],
        }
    }

    fn is_zero(self) -> bool {
        self.integer.iter().all(|byte| *byte == b'0')
            && self.fraction.iter().all(|byte| *byte == b'0')
    }
}

/// Parse the leading number; `None` when the line does not start with one.
///
/// No exponent is recognized, so `1e3` is the number 1 and `e3` is not a
/// number at all (and therefore compares as zero).
fn parse_number(line: &[u8]) -> Option<Number<'_>> {
    let mut index = 0usize;
    while index < line.len() && line[index].is_ascii_whitespace() {
        index += 1;
    }
    let mut negative = false;
    if index < line.len() && (line[index] == b'+' || line[index] == b'-') {
        negative = line[index] == b'-';
        index += 1;
    }

    let integer_start = index;
    while index < line.len() && line[index].is_ascii_digit() {
        index += 1;
    }
    let integer_end = index;

    let mut fraction_start = index;
    let mut fraction_end = index;
    if index < line.len() && line[index] == b'.' {
        index += 1;
        fraction_start = index;
        while index < line.len() && line[index].is_ascii_digit() {
            index += 1;
        }
        fraction_end = index;
    }

    if integer_start == integer_end && fraction_start == fraction_end {
        return None;
    }
    Some(Number {
        negative,
        integer: &line[integer_start..integer_end],
        fraction: &line[fraction_start..fraction_end],
    })
}

fn compare_numbers(left: Number<'_>, right: Number<'_>) -> Ordering {
    let left_zero = left.is_zero();
    let right_zero = right.is_zero();
    if left_zero && right_zero {
        return Ordering::Equal;
    }
    if left_zero {
        return if right.negative {
            Ordering::Greater
        } else {
            Ordering::Less
        };
    }
    if right_zero {
        return if left.negative {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    if left.negative != right.negative {
        return if left.negative {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }

    let magnitude = compare_magnitude(left, right);
    if left.negative {
        magnitude.reverse()
    } else {
        magnitude
    }
}

fn compare_magnitude(left: Number<'_>, right: Number<'_>) -> Ordering {
    let left_integer = trim_leading_zeros(left.integer);
    let right_integer = trim_leading_zeros(right.integer);

    let order = left_integer.len().cmp(&right_integer.len());
    if order != Ordering::Equal {
        return order;
    }
    let order = left_integer.cmp(right_integer);
    if order != Ordering::Equal {
        return order;
    }

    let width = left.fraction.len().max(right.fraction.len());
    for index in 0..width {
        let left_digit = left.fraction.get(index).copied().unwrap_or(b'0');
        let right_digit = right.fraction.get(index).copied().unwrap_or(b'0');
        let order = left_digit.cmp(&right_digit);
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}

fn trim_leading_zeros(mut bytes: &[u8]) -> &[u8] {
    while bytes.first() == Some(&b'0') {
        bytes = &bytes[1..];
    }
    bytes
}