//! `numfmt` — convert numbers between plain, SI (1000-based) and IEC
//! (1024-based) representations.
//!
//! Options:
//!
//! * `--to=si|iec|auto`, `--from=si|iec|auto` select the output/input family;
//! * `--to-unit=N` divides the value by the integer N first;
//! * `--suffix=S` appends S to every result;
//! * `--padding=N` left-pads the whole result to width N.
//!
//! Inputs may be integers or decimals with an optional `K`/`M`/`G`/`T`/`P`/`E`
//! suffix, an optional `i` (IEC) and an optional trailing `B`; `--from=auto`
//! reads `Ki` as 1024 and `K` as 1000. With no `--from`, a suffix is an error.
//! Negative values are accepted. Arithmetic is exact decimal/u128 — no floating
//! point is used, so the host kernel's FPU policy cannot change a result.
//!
//! Output with a selected unit has one decimal place (rounded half away from
//! zero); a value below the smallest unit is printed as an exact decimal.
//! `--to=auto` chooses IEC for a value that is at least 1024 and exactly
//! divisible by 1024, otherwise SI at 1000 and above, otherwise plain. SI uses
//! the lowercase `k`, IEC the uppercase `K`.
//!
//! `--to-unit` takes a plain positive decimal integer (the task's `N`), not a
//! suffixed size. There is no stdin reader: the kernel console has no EOF, so
//! numbers are operands. Exit: 0 success, 1 invalid number/data error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str =
    "usage: numfmt [--to=si|iec|auto] [--from=si|iec|auto] [--to-unit=N] [--suffix=S] [--padding=N] NUMBER...";
const SI_UNITS: [&str; 7] = ["", "k", "M", "G", "T", "P", "E"];
const IEC_UNITS: [&str; 7] = ["", "K", "M", "G", "T", "P", "E"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    None,
    Si,
    Iec,
    Auto,
}

pub fn run(args: &[String]) -> i32 {
    let mut to = Family::None;
    let mut from = Family::None;
    let mut to_unit: u128 = 1;
    let mut suffix: Option<String> = None;
    let mut padding: Option<usize> = None;
    let mut numbers: Vec<&str> = Vec::new();
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        let is_option = arg.starts_with("--")
            || (arg.len() > 1
                && arg.starts_with('-')
                && !arg.as_bytes()[1].is_ascii_digit()
                && arg.as_bytes()[1] != b'.');
        if parse_options && is_option {
            let (name, inline) = match arg.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (arg, None),
            };
            match name {
                "--to" | "--from" | "--to-unit" | "--suffix" | "--padding" => {
                    let value = match inline {
                        Some(value) => value,
                        None => match args.get(index).map(String::as_str) {
                            Some(value) => {
                                index += 1;
                                value
                            }
                            None => {
                                eprintln!("{USAGE}");
                                return util::EXIT_USAGE;
                            }
                        },
                    };
                    match name {
                        "--to" => match parse_family(value) {
                            Some(family) => to = family,
                            None => {
                                eprintln!("numfmt: invalid --to: '{value}'");
                                return util::EXIT_USAGE;
                            }
                        },
                        "--from" => match parse_family(value) {
                            Some(family) => from = family,
                            None => {
                                eprintln!("numfmt: invalid --from: '{value}'");
                                return util::EXIT_USAGE;
                            }
                        },
                        "--to-unit" => match parse_unit(value) {
                            Some(unit) => to_unit = unit,
                            None => {
                                eprintln!("numfmt: invalid --to-unit: '{value}'");
                                return util::EXIT_USAGE;
                            }
                        },
                        "--suffix" => suffix = Some(String::from(value)),
                        "--padding" => match util::parse_usize(value) {
                            Some(width) => padding = Some(width),
                            None => {
                                eprintln!("numfmt: invalid --padding: '{value}'");
                                return util::EXIT_USAGE;
                            }
                        },
                        _ => {
                            eprintln!("{USAGE}");
                            return util::EXIT_USAGE;
                        }
                    }
                }
                _ => {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }

        parse_options = false;
        if util::push(&mut numbers, arg).is_err() {
            eprintln!("numfmt: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if numbers.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    for number in numbers {
        let parsed = match parse_number(number, from) {
            Some(parsed) => parsed,
            None => {
                eprintln!("numfmt: invalid number: '{number}'");
                return util::EXIT_FAILURE;
            }
        };
        let denominator = match parsed.denominator.checked_mul(to_unit) {
            Some(denominator) => denominator,
            None => {
                eprintln!("numfmt: number out of range: '{number}'");
                return util::EXIT_FAILURE;
            }
        };
        let numerator = parsed.numerator;
        let negative = parsed.negative;

        let mut result = match to {
            Family::None => format_exact(numerator, denominator, negative),
            Family::Si => format_scaled(numerator, denominator, Family::Si, negative),
            Family::Iec => format_scaled(numerator, denominator, Family::Iec, negative),
            Family::Auto => {
                let family = choose_auto(numerator, denominator);
                format_scaled(numerator, denominator, family, negative)
            }
        };

        if let Some(text) = &suffix {
            result.push_str(text);
        }
        if let Some(width) = padding {
            let length = result.chars().count();
            if length < width {
                let mut padded = String::new();
                if padded.try_reserve(width - length + result.len()).is_err() {
                    eprintln!("numfmt: out of memory");
                    return util::EXIT_FAILURE;
                }
                for _ in 0..width - length {
                    padded.push(' ');
                }
                padded.push_str(&result);
                result = padded;
            }
        }

        if let Err(err) = util::write_line(1, result.as_bytes()) {
            eprintln!("numfmt: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    util::EXIT_OK
}

fn parse_family(value: &str) -> Option<Family> {
    match value {
        "si" => Some(Family::Si),
        "iec" => Some(Family::Iec),
        "auto" => Some(Family::Auto),
        _ => None,
    }
}

fn parse_unit(value: &str) -> Option<u128> {
    let unit = value.parse::<u128>().ok()?;
    if unit == 0 {
        None
    } else {
        Some(unit)
    }
}

struct Parsed {
    negative: bool,
    numerator: u128,
    denominator: u128,
}

fn parse_number(text: &str, from: Family) -> Option<Parsed> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let mut negative = false;
    match bytes.first() {
        Some(b'+') => index = 1,
        Some(b'-') => {
            negative = true;
            index = 1;
        }
        _ => {}
    }

    let mut mantissa: u128 = 0;
    let mut digits = 0usize;
    let mut fraction_digits = 0usize;
    while let Some(byte) = bytes.get(index) {
        if !byte.is_ascii_digit() {
            break;
        }
        mantissa = mantissa.checked_mul(10)?.checked_add((*byte - b'0') as u128)?;
        digits += 1;
        index += 1;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while let Some(byte) = bytes.get(index) {
            if !byte.is_ascii_digit() {
                break;
            }
            mantissa = mantissa.checked_mul(10)?.checked_add((*byte - b'0') as u128)?;
            digits += 1;
            fraction_digits += 1;
            index += 1;
        }
        if index == start {
            return None;
        }
    }
    if digits == 0 {
        return None;
    }

    let suffix = bytes.get(index..)?;
    let (multiplier, suffix_ok) = match parse_suffix(suffix, from) {
        Some(result) => result,
        None => return None,
    };
    if !suffix_ok {
        return None;
    }

    let numerator = mantissa.checked_mul(multiplier)?;
    let mut denominator: u128 = 1;
    for _ in 0..fraction_digits {
        denominator = denominator.checked_mul(10)?;
    }
    Some(Parsed {
        negative: negative && numerator != 0,
        numerator,
        denominator,
    })
}

/// Returns `(multiplier, valid)`.
fn parse_suffix(suffix: &[u8], from: Family) -> Option<(u128, bool)> {
    if suffix.is_empty() {
        return Some((1, true));
    }
    if from == Family::None {
        return Some((1, false));
    }

    let mut index = 0usize;
    let power = match suffix.get(index).copied()? {
        b'K' | b'k' => 1u32,
        b'M' | b'm' => 2,
        b'G' | b'g' => 3,
        b'T' | b't' => 4,
        b'P' | b'p' => 5,
        b'E' | b'e' => 6,
        _ => return Some((1, false)),
    };
    index += 1;
    let mut iec = false;
    if suffix.get(index) == Some(&b'i') {
        iec = true;
        index += 1;
    }
    if suffix.get(index) == Some(&b'B') {
        index += 1;
    }
    if index != suffix.len() {
        return Some((1, false));
    }

    match from {
        Family::Si => {
            if iec {
                return Some((1, false));
            }
            Some((power_of(1000, power)?, true))
        }
        Family::Iec => Some((power_of(1024, power)?, true)),
        Family::Auto => {
            let base = if iec { 1024 } else { 1000 };
            Some((power_of(base, power)?, true))
        }
        Family::None => Some((1, false)),
    }
}

fn power_of(base: u128, power: u32) -> Option<u128> {
    let mut result = 1u128;
    for _ in 0..power {
        result = result.checked_mul(base)?;
    }
    Some(result)
}

fn format_exact(numerator: u128, denominator: u128, negative: bool) -> String {
    let integer = numerator / denominator;
    let mut remainder = numerator % denominator;
    let mut out = alloc::format!("{integer}");
    if remainder != 0 {
        out.push('.');
        for _ in 0..9 {
            remainder = match remainder.checked_mul(10) {
                Some(value) => value,
                None => break,
            };
            let digit = remainder / denominator;
            remainder %= denominator;
            out.push((b'0' + digit as u8) as char);
            if remainder == 0 {
                break;
            }
        }
    }
    if negative && (integer != 0 || remainder != 0) {
        let mut signed = String::with_capacity(out.len() + 1);
        signed.push('-');
        signed.push_str(&out);
        return signed;
    }
    out
}

fn choose_auto(numerator: u128, denominator: u128) -> Family {
    let iec_unit = match denominator.checked_mul(1024) {
        Some(value) => value,
        None => return Family::None,
    };
    if numerator >= iec_unit && numerator % iec_unit == 0 {
        return Family::Iec;
    }
    let si_unit = match denominator.checked_mul(1000) {
        Some(value) => value,
        None => return Family::None,
    };
    if numerator >= si_unit {
        Family::Si
    } else {
        Family::None
    }
}

fn format_scaled(
    numerator: u128,
    denominator: u128,
    family: Family,
    negative: bool,
) -> String {
    let (base, units): (u128, &[&str; 7]) = match family {
        Family::Iec => (1024, &IEC_UNITS),
        _ => (1000, &SI_UNITS),
    };

    // Largest k in 1..=6 with denominator * base^k <= numerator.
    let mut power = 0u32;
    let mut scaled_denominator = denominator;
    while power < 6 {
        let next = match scaled_denominator.checked_mul(base) {
            Some(next) => next,
            None => break,
        };
        if next > numerator {
            break;
        }
        scaled_denominator = next;
        power += 1;
    }

    if power == 0 {
        return format_exact(numerator, scaled_denominator, negative);
    }

    let mut integer = numerator / scaled_denominator;
    let remainder = numerator % scaled_denominator;
    let mut tenths = match remainder.checked_mul(10) {
        Some(value) => (value + scaled_denominator / 2) / scaled_denominator,
        None => 0,
    };
    if tenths >= 10 {
        integer = integer.saturating_add(1);
        tenths = 0;
    }
    let mut out = alloc::format!("{}.{}{}", integer, tenths, units[power as usize]);
    if negative && numerator != 0 {
        let mut signed = String::with_capacity(out.len() + 1);
        signed.push('-');
        signed.push_str(&out);
        out = signed;
    }
    out
}