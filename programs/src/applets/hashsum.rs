//! Shared command-line front end for `md5sum`, `sha1sum`, `sha224sum`,
//! `sha256sum`, `sha384sum`, `sha512sum` and `b2sum`.
//!
//! Interfaces match the coreutils `*sum` family closely enough for real use:
//!
//! ```text
//! md5sum [-b] [-c] [--quiet] [--status] [FILE...]
//! ```
//!
//! Plain mode prints `hex  name` (two spaces, or `hex *name` with `-b`); an
//! implicit or explicit stdin operand is named `-`. Check mode reads the
//! coreutils format (`hex  name` or `hex *name`, with an optional leading
//! backslash for escaped names), verifies every listed file and prints
//! `name: OK` / `name: FAILED`. A malformed line or a missing file makes the
//! exit status 1; an unknown option is a usage error (2).
//!
//! The digest core lives in [`super::digest`]; this module only deals with
//! files, parsing and diagnostics.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use super::digest::{self, Algorithm};
use crate::util;

const HEX: &[u8; 16] = b"0123456789abcdef";

pub fn run(program: &str, algorithm: Algorithm, args: &[String]) -> i32 {
    let mut check = false;
    let mut binary = false;
    let mut quiet = false;
    let mut status = false;
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
        if parse_options && arg.starts_with("--") {
            match arg {
                "--check" => check = true,
                "--binary" => binary = true,
                "--quiet" => quiet = true,
                "--status" => status = true,
                "--strict" | "--warn" => {}
                _ => {
                    eprintln!("{program}: unrecognized option '{arg}'");
                    return usage(program);
                }
            }
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            for option in arg[1..].chars() {
                match option {
                    'b' => binary = true,
                    'c' => check = true,
                    _ => {
                        eprintln!("{program}: invalid option -- '{option}'");
                        return usage(program);
                    }
                }
            }
            continue;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("{program}: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        operands.push("-");
    }

    if check {
        check_files(program, algorithm, &operands, quiet, status)
    } else {
        print_sums(program, algorithm, &operands, binary)
    }
}

fn usage(program: &str) -> i32 {
    eprintln!("usage: {program} [-b] [-c] [--quiet] [--status] [FILE...]");
    util::EXIT_USAGE
}

fn print_sums(program: &str, algorithm: Algorithm, operands: &[&str], binary: bool) -> i32 {
    let mut status = util::EXIT_OK;
    for name in operands {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report(program, util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        let (digest, length) = digest::hash(algorithm, &data);
        let line = format_sum(&digest[..length], name, binary);
        if let Err(err) = util::write_line(1, line.as_bytes()) {
            eprintln!("{program}: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    status
}

fn format_sum(digest: &[u8], name: &str, binary: bool) -> String {
    let mut line = String::with_capacity(digest.len() * 2 + name.len() + 3);
    for byte in digest {
        line.push(HEX[((*byte >> 4) & 0x0f) as usize] as char);
        line.push(HEX[(*byte & 0x0f) as usize] as char);
    }
    line.push(' ');
    line.push(if binary { '*' } else { ' ' });
    line.push_str(name);
    line
}

fn check_files(
    program: &str,
    algorithm: Algorithm,
    operands: &[&str],
    quiet: bool,
    status_only: bool,
) -> i32 {
    let mut result = util::EXIT_OK;
    for list_name in operands {
        let data = match util::read_input(list_name) {
            Ok(data) => data,
            Err(err) => {
                util::report(program, util::display_name(list_name), err);
                result = util::EXIT_FAILURE;
                continue;
            }
        };
        let mut checked = 0usize;
        let mut failed = 0usize;
        let mut malformed = 0usize;

        for line in util::lines(&data) {
            if line.is_empty() || line.iter().all(|byte| byte.is_ascii_whitespace()) {
                continue;
            }
            let Some((expected, target)) = parse_check_line(line, algorithm.length() * 2) else {
                malformed += 1;
                continue;
            };
            checked += 1;

            let contents = match util::read_input(&target) {
                Ok(contents) => contents,
                Err(err) => {
                    util::report(program, util::display_name(&target), err);
                    if !status_only {
                        let line = alloc::format!("{target}: FAILED open or read");
                        if let Err(write_err) = util::write_line(1, line.as_bytes()) {
                            eprintln!("{program}: write error: {write_err}");
                            return util::EXIT_FAILURE;
                        }
                    }
                    failed += 1;
                    continue;
                }
            };
            let (actual, length) = digest::hash(algorithm, &contents);
            let Some(expected_bytes) = decode_hex(expected) else {
                malformed += 1;
                continue;
            };
            let matches = expected_bytes[..length] == actual[..length];

            if matches {
                if !quiet && !status_only {
                    let line = alloc::format!("{target}: OK");
                    if let Err(err) = util::write_line(1, line.as_bytes()) {
                        eprintln!("{program}: write error: {err}");
                        return util::EXIT_FAILURE;
                    }
                }
            } else {
                if !status_only {
                    let line = alloc::format!("{target}: FAILED");
                    if let Err(err) = util::write_line(1, line.as_bytes()) {
                        eprintln!("{program}: write error: {err}");
                        return util::EXIT_FAILURE;
                    }
                }
                failed += 1;
            }
        }

        if checked == 0 && malformed == 0 {
            eprintln!("{program}: {list_name}: no properly formatted checksum lines found");
            result = util::EXIT_FAILURE;
            continue;
        }
        if malformed > 0 {
            if !status_only {
                eprintln!("{program}: WARNING: {malformed} line(s) are improperly formatted");
            }
            result = util::EXIT_FAILURE;
        }
        if failed > 0 {
            if !status_only {
                eprintln!("{program}: WARNING: {failed} computed checksum(s) did NOT match");
            }
            result = util::EXIT_FAILURE;
        }
    }
    result
}

/// Parse one `<hex><space>[ *]<name>` line.
///
/// Returns the hex token and the file name. The hex token borrows `line`; the
/// name is unescaped into an owned `String` because the coreutils format may
/// spell a newline as `\n` (the kernel's paths cannot contain one, but a
/// hostile checksum file must not make this parser panic).
fn parse_check_line<'a>(line: &'a [u8], hex_len: usize) -> Option<(&'a [u8], String)> {
    let mut position = 0usize;
    let mut escaped = false;
    if line.first() == Some(&b'\\') {
        escaped = true;
        position = 1;
    }
    if line.len() < position + hex_len {
        return None;
    }
    let token = &line[position..position + hex_len];
    if !token.iter().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    position += hex_len;
    if line.get(position) != Some(&b' ') {
        return None;
    }
    position += 1;
    match line.get(position) {
        Some(&b'*') => position += 1,
        Some(&b' ') => position += 1,
        _ => {}
    }
    if position >= line.len() {
        return None;
    }
    let name_bytes = &line[position..];
    let name = if escaped {
        unescape_name(name_bytes)?
    } else {
        core::str::from_utf8(name_bytes).ok()?.into()
    };
    if name.is_empty() {
        return None;
    }
    Some((token, name))
}

fn unescape_name(bytes: &[u8]) -> Option<String> {
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(bytes.len()).ok()?;
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 1 < bytes.len() {
            match bytes[index + 1] {
                b'\\' => out.push(b'\\'),
                b'n' => out.push(b'\n'),
                other => {
                    out.push(b'\\');
                    out.push(other);
                }
            }
            index += 2;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}
/// Decode a hex token into at most 64 bytes. `None` means a stray character
/// (the caller already checked the length and the character class, but keeping
/// this total makes the parser independent of that check).
fn decode_hex(token: &[u8]) -> Option<[u8; 64]> {
    if token.len() % 2 != 0 || token.len() > 128 {
        return None;
    }
    let mut out = [0u8; 64];
    for (slot, pair) in out.iter_mut().zip(token.chunks_exact(2)) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        *slot = (high << 4) | low;
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
