//! `crc32` — IEEE 802.3 CRC-32 plus byte count.
//!
//! The polynomial is the reflected `0xEDB88320` (the zlib/POSIX `crc32`
//! value), with the usual `0xffffffff` initial register and final complement.
//! For `"123456789"` the result is `cbf43926`, the standard CRC-32 check
//! value. This is deliberately **not** `cksum`: POSIX `cksum` uses the
//! non-reflected `0x04C11DB7` polynomial over the file length too, while this
//! applet reports the widely used IEEE/zlib value.
//!
//! Output is `cccccccc size [name]` — an eight-digit lowercase hexadecimal
//! CRC, the decimal byte count, and the operand name. An implicit stdin
//! stream has no name; explicit operands, including `-`, are shown as typed
//! (the same rule as `cksum` and `sum`). `-c` reads checksum-list files in
//! that same `crc size name` format, verifies the named files and prints
//! `name: OK` / `name: FAILED`; it exits 1 if a line is malformed, a file
//! cannot be read, or a value differs.
//!
//! Exit: 0 success, 1 data/I/O error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: crc32 [-c] [FILE...]";

pub fn run(args: &[String]) -> i32 {
    let mut check = false;
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
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            if arg == "-c" || arg == "--check" {
                check = true;
                continue;
            }
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("crc32: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let implicit_stdin = operands.is_empty();
    if implicit_stdin {
        operands.push("-");
    }

    if check {
        check_files(&operands)
    } else {
        print_sums(&operands, implicit_stdin)
    }
}

fn print_sums(operands: &[&str], implicit_stdin: bool) -> i32 {
    let mut status = util::EXIT_OK;
    for name in operands {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("crc32", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        let mut line = alloc::format!("{:08x} {}", ieee_crc32(&data), data.len());
        if !implicit_stdin {
            line.push(' ');
            line.push_str(name);
        }
        if let Err(err) = util::write_line(1, line.as_bytes()) {
            eprintln!("crc32: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    status
}

fn check_files(operands: &[&str]) -> i32 {
    let mut result = util::EXIT_OK;
    for list_name in operands {
        let data = match util::read_input(list_name) {
            Ok(data) => data,
            Err(err) => {
                util::report("crc32", util::display_name(list_name), err);
                result = util::EXIT_FAILURE;
                continue;
            }
        };
        let mut checked = 0usize;
        let mut failed = 0usize;
        let mut malformed = 0usize;

        for line in util::lines(&data) {
            let line = trim_ascii(line);
            if line.is_empty() {
                continue;
            }
            let Some((expected_crc, expected_size, target)) = parse_check_line(line) else {
                malformed += 1;
                continue;
            };
            checked += 1;

            let contents = match util::read_input(&target) {
                Ok(contents) => contents,
                Err(err) => {
                    util::report("crc32", util::display_name(&target), err);
                    let line = alloc::format!("{target}: FAILED open or read");
                    if let Err(write_err) = util::write_line(1, line.as_bytes()) {
                        eprintln!("crc32: write error: {write_err}");
                        return util::EXIT_FAILURE;
                    }
                    failed += 1;
                    continue;
                }
            };
            let actual = ieee_crc32(&contents);
            let matches = actual == expected_crc && contents.len() == expected_size;
            let line = alloc::format!(
                "{target}: {}",
                if matches { "OK" } else { "FAILED" }
            );
            if let Err(err) = util::write_line(1, line.as_bytes()) {
                eprintln!("crc32: write error: {err}");
                return util::EXIT_FAILURE;
            }
            if !matches {
                failed += 1;
            }
        }

        if checked == 0 && malformed == 0 {
            eprintln!("crc32: {list_name}: no properly formatted checksum lines found");
            result = util::EXIT_FAILURE;
            continue;
        }
        if malformed > 0 {
            eprintln!("crc32: WARNING: {malformed} line(s) are improperly formatted");
            result = util::EXIT_FAILURE;
        }
        if failed > 0 {
            eprintln!("crc32: WARNING: {failed} computed checksum(s) did NOT match");
            result = util::EXIT_FAILURE;
        }
    }
    result
}

/// Parse `<crc> <size> <name>`; the name may contain spaces and runs to the
/// end of the line. `crc` is one to eight hex digits with an optional `0x`.
fn parse_check_line(line: &[u8]) -> Option<(u32, usize, String)> {
    let mut position = 0usize;
    while position < line.len() && line[position].is_ascii_whitespace() {
        position += 1;
    }
    let crc_start = position;
    while position < line.len() && !line[position].is_ascii_whitespace() {
        position += 1;
    }
    if crc_start == position || position >= line.len() {
        return None;
    }
    let crc_token = core::str::from_utf8(&line[crc_start..position]).ok()?;
    let crc_token = crc_token
        .strip_prefix("0x")
        .or_else(|| crc_token.strip_prefix("0X"))
        .unwrap_or(crc_token);
    if crc_token.is_empty() || crc_token.len() > 8 {
        return None;
    }
    let expected_crc = u32::from_str_radix(crc_token, 16).ok()?;

    while position < line.len() && line[position].is_ascii_whitespace() {
        position += 1;
    }
    let size_start = position;
    while position < line.len() && !line[position].is_ascii_whitespace() {
        position += 1;
    }
    if size_start == position {
        return None;
    }
    let size_token = core::str::from_utf8(&line[size_start..position]).ok()?;
    if !size_token.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let expected_size: usize = size_token.parse().ok()?;

    while position < line.len() && line[position].is_ascii_whitespace() {
        position += 1;
    }
    if position >= line.len() {
        return None;
    }
    let target = core::str::from_utf8(&line[position..]).ok()?;
    if target.is_empty() {
        return None;
    }
    Some((expected_crc, expected_size, String::from(target)))
}

fn trim_ascii(mut line: &[u8]) -> &[u8] {
    while let Some((last, rest)) = line.split_last() {
        if last.is_ascii_whitespace() {
            line = rest;
        } else {
            break;
        }
    }
    line
}

/// IEEE/Zlib CRC-32: reflected polynomial `0xEDB88320`, initial `0xffffffff`,
/// final complement.
fn ieee_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}