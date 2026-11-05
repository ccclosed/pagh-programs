//! `xxd` — hex dump, and the basic reverse direction.
//!
//! Usage: `xxd [-r] [-l N] [-s OFF] [FILE]`. The dump format is the familiar
//! one: an eight-digit lowercase hex offset, a colon, sixteen bytes as
//! two-digit hex in groups of two, then two spaces and the ASCII column
//! (`.` for non-printing bytes). `-l N` stops after N bytes and `-s OFF`
//! starts at that byte offset (decimal, or hexadecimal with a `0x` prefix).
//!
//! `-r` turns a dump back into bytes: each line may be a full xxd line
//! (`OFFSET: HEX  ASCII`) or a bare run of hex digits; the offset and the
//! ASCII column are ignored. It is the basic reverse mode: `*` compression,
//! bit/byte order options and output files are not implemented. `-l`/`-s`
//! apply to the dump direction; in reverse they are accepted and ignored.
//!
//! One file operand or stdin (`-`). Exit: 0 success, 1 open/read/write error
//! or malformed reverse input, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: xxd [-r] [-l N] [-s OFF] [FILE]";
const LINE_BYTES: usize = 16;

pub fn run(args: &[String]) -> i32 {
    let mut reverse = false;
    let mut limit: Option<usize> = None;
    let mut skip: u64 = 0;
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
        if parse_options && arg == "-r" {
            reverse = true;
            continue;
        }
        if parse_options && (arg == "-l" || arg == "-s") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            if arg == "-l" {
                match util::parse_usize(value) {
                    Some(parsed) => limit = Some(parsed),
                    None => {
                        eprintln!("xxd: invalid length: '{value}'");
                        return util::EXIT_USAGE;
                    }
                }
            } else {
                match parse_offset(value) {
                    Some(parsed) => skip = parsed,
                    None => {
                        eprintln!("xxd: invalid offset: '{value}'");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("-l") {
                if !value.is_empty() {
                    match util::parse_usize(value) {
                        Some(parsed) => {
                            limit = Some(parsed);
                            continue;
                        }
                        None => {
                            eprintln!("xxd: invalid length: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                }
            }
            if let Some(value) = arg.strip_prefix("-s") {
                if !value.is_empty() {
                    match parse_offset(value) {
                        Some(parsed) => {
                            skip = parsed;
                            continue;
                        }
                        None => {
                            eprintln!("xxd: invalid offset: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("xxd: out of memory");
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
            util::report("xxd", util::display_name(name), err);
            return util::EXIT_FAILURE;
        }
    };

    if reverse {
        return match decode(&data) {
            Ok(decoded) => {
                if let Err(err) = util::write_all(1, &decoded) {
                    eprintln!("xxd: write error: {err}");
                    util::EXIT_FAILURE
                } else {
                    util::EXIT_OK
                }
            }
            Err(message) => {
                eprintln!("xxd: {message}");
                util::EXIT_FAILURE
            }
        };
    }

    let start = if skip > data.len() as u64 {
        data.len()
    } else {
        skip as usize
    };
    let end = match limit {
        Some(limit) => start.saturating_add(limit).min(data.len()),
        None => data.len(),
    };
    for (index, chunk) in data[start..end].chunks(LINE_BYTES).enumerate() {
        let offset = (start + index * LINE_BYTES) as u64;
        if let Err(err) = dump_line(offset, chunk) {
            eprintln!("xxd: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    util::EXIT_OK
}

fn parse_offset(text: &str) -> Option<u64> {
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        if hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        u64::from_str_radix(hex, 16).ok()
    } else {
        text.parse::<u64>().ok()
    }
}

fn dump_line(offset: u64, chunk: &[u8]) -> libpagh::Result<()> {
    let mut line = String::with_capacity(80);
    let _ = write!(line, "{offset:08x}: ");
    for index in 0..LINE_BYTES {
        if index > 0 && index % 2 == 0 {
            line.push(' ');
        }
        match chunk.get(index) {
            Some(byte) => {
                let _ = write!(line, "{byte:02x}");
            }
            None => line.push_str("  "),
        }
    }
    line.push_str("  ");
    for byte in chunk {
        line.push(if (0x20..=0x7e).contains(byte) {
            char::from(*byte)
        } else {
            '.'
        });
    }
    util::write_line(1, line.as_bytes())
}

fn decode(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::new();
    for line in util::lines(data) {
        let line = trim_ascii(line);
        if line.is_empty() {
            continue;
        }
        let hex_part = match line.iter().position(|byte| *byte == b':') {
            Some(colon) => trim_ascii(&line[colon + 1..]),
            None => line,
        };
        // In an xxd dump the ASCII column follows the hex area after at least
        // two spaces. `find` gives the shortest prefix, so single spaces
        // between the two-byte groups survive.
        let mut end = hex_part.len();
        let mut index = 0usize;
        while index + 1 < hex_part.len() {
            if hex_part[index] == b' ' && hex_part[index + 1] == b' ' {
                end = index;
                break;
            }
            index += 1;
        }
        let stripe = trim_ascii(&hex_part[..end]);
        if stripe.is_empty() {
            continue;
        }
        let mut digits: Vec<u8> = Vec::new();
        for byte in stripe {
            if *byte == b' ' || *byte == b'\t' {
                continue;
            }
            if !byte.is_ascii_hexdigit() {
                return Err("invalid hex input");
            }
            if util::push(&mut digits, *byte).is_err() {
                return Err("out of memory");
            }
        }
        if digits.len() % 2 != 0 {
            return Err("odd number of hex digits");
        }
        let mut pair = 0usize;
        while pair < digits.len() {
            let high = hex_value(digits[pair]).ok_or("invalid hex input")?;
            let low = hex_value(digits[pair + 1]).ok_or("invalid hex input")?;
            if out.try_reserve(1).is_err() {
                return Err("out of memory");
            }
            out.push((high << 4) | low);
            pair += 2;
        }
    }
    Ok(out)
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while let Some((first, rest)) = bytes.split_first() {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    while let Some((last, rest)) = bytes.split_last() {
        if last.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
