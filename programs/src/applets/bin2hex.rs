//! `bin2hex` — encode binary input as lowercase hexadecimal.
//!
//! Output is a bare run of hex digits with a final newline; `-u` selects
//! uppercase. `-w N` breaks the stream every N hex characters (not bytes), and
//! `-w 0` — the default — disables wrapping. Because wrapping only inserts
//! ASCII whitespace, the output always round-trips through `hex2bin`
//! regardless of the width. Empty input produces no output.
//!
//! One file operand or stdin (`-`); output goes to stdout. Exit: 0 success,
//! 1 read/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: bin2hex [-u] [-w N] [FILE]";
const LOWER: &[u8; 16] = b"0123456789abcdef";
const UPPER: &[u8; 16] = b"0123456789ABCDEF";

pub fn run(args: &[String]) -> i32 {
    let mut uppercase = false;
    let mut wrap = 0usize;
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
        if parse_options && (arg == "-u" || arg == "--uppercase") {
            uppercase = true;
            continue;
        }
        if parse_options && (arg == "-w" || arg == "--wrap") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match util::parse_usize(value) {
                Some(parsed) => wrap = parsed,
                None => {
                    eprintln!("bin2hex: invalid wrap width: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-w") {
            match util::parse_usize(&arg[2..]) {
                Some(parsed) => wrap = parsed,
                None => {
                    eprintln!("bin2hex: invalid wrap width: '{}'", &arg[2..]);
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
            eprintln!("bin2hex: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.len() > 1 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let name = operands.first().copied().unwrap_or("-");

    let data = match util::read_input(name) {
        Ok(data) => data,
        Err(err) => {
            util::report("bin2hex", util::display_name(name), err);
            return util::EXIT_FAILURE;
        }
    };
    let encoded = match encode_hex(&data, uppercase, wrap) {
        Some(encoded) => encoded,
        None => {
            eprintln!("bin2hex: out of memory");
            return util::EXIT_FAILURE;
        }
    };
    if let Err(err) = util::write_all(1, &encoded) {
        eprintln!("bin2hex: write error: {err}");
        return util::EXIT_FAILURE;
    }
    util::EXIT_OK
}

/// Encode `data`, optionally breaking every `wrap` hex characters. Empty
/// input returns an empty vector; non-empty output always ends in `\n`.
fn encode_hex(data: &[u8], uppercase: bool, wrap: usize) -> Option<Vec<u8>> {
    if data.is_empty() {
        return Some(Vec::new());
    }
    let digits = if uppercase { UPPER } else { LOWER };
    let line_breaks = if wrap == 0 {
        1
    } else {
        (data.len().saturating_mul(2) / wrap).saturating_add(1)
    };
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(data.len().saturating_mul(2).saturating_add(line_breaks))
        .ok()?;
    let mut column = 0usize;
    for byte in data {
        for shift in [4u8, 0u8] {
            out.push(digits[((*byte >> shift) & 0x0f) as usize]);
            column += 1;
            if wrap != 0 && column == wrap {
                out.push(b'\n');
                column = 0;
            }
        }
    }
    if column != 0 {
        out.push(b'\n');
    }
    Some(out)
}