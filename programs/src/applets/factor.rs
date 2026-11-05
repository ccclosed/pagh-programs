//! `factor` — prime factorisation of unsigned 64-bit integers.
//!
//! Output is `N: p q ...`, one line per operand, factors in non-decreasing
//! order and with multiplicity (`factor 12` -> `12: 2 2 3`). The special cases
//! are printed as `0: 0` and `1: 1`: zero has no prime factorisation, and one
//! is the empty product; spelling that out is friendlier than an empty line.
//!
//! The algorithm is trial division: 2 and 3 first, then the 6k±1 candidates up
//! to `sqrt(rest)`. It is exact and needs no primality test, but a 64-bit prime
//! near `2^64` costs about 1.4 billion iterations — there is no Miller-Rabin
//! or Pollard rho in this port. Small and medium inputs (the useful case on the
//! console) are fast.
//!
//! An operand that is not a `u64` is reported as
//! `factor: '...': invalid number`, the remaining operands are still
//! factorised, and the exit status is 1. No operands is usage error 2. There
//! are no options, and no pipes/redirection in the shell.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: factor NUMBER...";

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
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("factor: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut status = util::EXIT_OK;
    for text in operands {
        let value = match text.parse::<u64>() {
            Ok(value) => value,
            Err(_) => {
                eprintln!("factor: '{text}': invalid number");
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        let line = match factor_line(value) {
            Ok(line) => line,
            Err(()) => {
                eprintln!("factor: out of memory");
                return util::EXIT_FAILURE;
            }
        };
        if let Err(err) = util::write_line(1, line.as_bytes()) {
            eprintln!("factor: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    status
}

fn factor_line(value: u64) -> Result<String, ()> {
    if value == 0 {
        return Ok(String::from("0: 0"));
    }
    if value == 1 {
        return Ok(String::from("1: 1"));
    }

    let mut factors: Vec<u64> = Vec::new();
    let mut rest = value;

    for small in [2u64, 3u64] {
        while rest % small == 0 {
            factors.try_reserve(1).map_err(|_| ())?;
            factors.push(small);
            rest /= small;
        }
    }

    let mut divisor = 5u64;
    let mut step = 2u64;
    while divisor <= rest / divisor {
        while rest % divisor == 0 {
            factors.try_reserve(1).map_err(|_| ())?;
            factors.push(divisor);
            rest /= divisor;
        }
        divisor = divisor.saturating_add(step);
        step = 6 - step;
    }
    if rest > 1 {
        factors.try_reserve(1).map_err(|_| ())?;
        factors.push(rest);
    }

    let mut line = format!("{value}:");
    for factor in factors {
        let text = format!("{factor}");
        line.try_reserve(text.len() + 1).map_err(|_| ())?;
        line.push(' ');
        line.push_str(&text);
    }
    Ok(line)
}