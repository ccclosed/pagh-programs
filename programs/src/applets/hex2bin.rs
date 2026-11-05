//! `hex2bin` — decode a hexadecimal stream to binary.
//!
//! ASCII whitespace (spaces, tabs, newlines, CR) is ignored between hex
//! digits, so both a bare digit stream and the line-wrapped output of
//! `bin2hex`/`xxd -p` decode. Every other character must be a hex digit
//! (`0-9`, `a-f`, `A-F`); an odd number of digits or any stray character is
//! invalid input (exit 1). The decoded bytes go to stdout, or to `-o FILE`
//! (`-o -` means stdout). One input operand or stdin (`-`).
//!
//! Exit: 0 success, 1 invalid hex or read/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;

use crate::util;

const USAGE: &str = "usage: hex2bin [-o FILE] [FILE]";

pub fn run(args: &[String]) -> i32 {
    let mut output: Option<&str> = None;
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
        if parse_options && (arg == "-o" || arg == "--output") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            output = Some(value);
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-o") {
            output = Some(&arg[2..]);
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("hex2bin: out of memory");
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
            util::report("hex2bin", util::display_name(name), err);
            return util::EXIT_FAILURE;
        }
    };
    let decoded = match decode_hex(&data) {
        Some(decoded) => decoded,
        None => {
            eprintln!("hex2bin: invalid hex input");
            return util::EXIT_FAILURE;
        }
    };

    let result = match output {
        Some(path) if path != "-" => fs::write(path, &decoded),
        _ => util::write_all(1, &decoded),
    };
    match result {
        Ok(()) => util::EXIT_OK,
        Err(err) => {
            match output {
                Some(path) if path != "-" => util::report("hex2bin", path, err),
                _ => eprintln!("hex2bin: write error: {err}"),
            }
            util::EXIT_FAILURE
        }
    }
}

/// Decode hex pairs, skipping ASCII whitespace. An odd trailing digit or a
/// non-hex character returns `None`.
fn decode_hex(data: &[u8]) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(data.len() / 2).ok()?;
    let mut high: Option<u8> = None;
    for &byte in data {
        if byte.is_ascii_whitespace() {
            continue;
        }
        let value = hex_value(byte)?;
        match high.take() {
            Some(first) => out.push((first << 4) | value),
            None => high = Some(value),
        }
    }
    if high.is_some() {
        return None;
    }
    Some(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}