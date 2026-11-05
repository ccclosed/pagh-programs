//! `base85` — ASCII85 (Adobe `btoa`/`a85`) encoding and decoding.
//!
//! Four input bytes become five printable characters from `!` (0x21) to `u`
//! (0x75), most significant digit first; a full group of four zero bytes is
//! the short form `z`. A final group of 1–3 bytes is zero-padded, encoded, and
//! only the first `n + 1` characters are written, which is what makes the
//! encoding reversible. Decoding accepts `z` only at a group boundary, pads a
//! short final group with `u`, and rejects any character outside the alphabet,
//! a stray `z`, or a group whose value does not fit in 32 bits.
//!
//! ```text
//! base85 [-d] [-a] [FILE]
//! ```
//!
//! `-d`/`--decode` decodes; the default encodes. `-a`/`--adobe` wraps encoder
//! output in the Adobe delimiters `<~` and `~>`. Decoding always accepts the
//! delimiters (with or without surrounding whitespace), so both forms
//! round-trip; it also ignores ASCII whitespace between characters, which
//! makes an encoded file ending in a newline readable directly. There is no
//! line wrapping: encoder output is one line plus the final newline.
//!
//! Exit: 0 success, 1 invalid input or a read/write error, 2 usage. One file
//! operand or stdin (`-`).

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: base85 [-d] [-a] [FILE]";

pub fn run(args: &[String]) -> i32 {
    let mut decode = false;
    let mut adobe = false;
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
        if parse_options && (arg == "-d" || arg == "--decode") {
            decode = true;
            continue;
        }
        if parse_options && (arg == "-a" || arg == "--adobe") {
            adobe = true;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("base85: out of memory");
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
            util::report("base85", util::display_name(name), err);
            return util::EXIT_FAILURE;
        }
    };

    if decode {
        match decode_bytes(&data) {
            Ok(decoded) => {
                if let Err(err) = util::write_all(1, &decoded) {
                    eprintln!("base85: write error: {err}");
                    return util::EXIT_FAILURE;
                }
                util::EXIT_OK
            }
            Err(DecodeError::Invalid) => {
                eprintln!("base85: invalid input");
                util::EXIT_FAILURE
            }
            Err(DecodeError::Memory) => {
                eprintln!("base85: out of memory");
                util::EXIT_FAILURE
            }
        }
    } else {
        match encode_bytes(&data, adobe) {
            Some(encoded) => {
                if encoded.is_empty() {
                    return util::EXIT_OK;
                }
                if let Err(err) = util::write_line(1, &encoded) {
                    eprintln!("base85: write error: {err}");
                    return util::EXIT_FAILURE;
                }
                util::EXIT_OK
            }
            None => {
                eprintln!("base85: out of memory");
                util::EXIT_FAILURE
            }
        }
    }
}

/// Encode `data`; `None` is only an allocation failure.
fn encode_bytes(data: &[u8], adobe: bool) -> Option<Vec<u8>> {
    // 5 characters per 4 bytes, rounded up, plus the delimiters and the final
    // newline the caller adds.
    let capacity = data
        .len()
        .saturating_mul(5)
        .saturating_div(4)
        .saturating_add(11);
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(capacity).ok()?;
    if adobe {
        out.extend_from_slice(b"<~");
    }

    let mut chunks = data.chunks_exact(4);
    for chunk in &mut chunks {
        encode_group(&mut out, u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]), 5);
    }

    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        let mut padded = [0u8; 4];
        padded[..remainder.len()].copy_from_slice(remainder);
        let value = u32::from_be_bytes(padded);
        encode_group(&mut out, value, remainder.len() + 1);
    }

    if adobe {
        out.extend_from_slice(b"~>");
    }
    Some(out)
}

/// Append the first `digits` characters of the five-digit base-85 form.
///
/// A full four-zero group is the special short form `z`; a partial group is
/// never abbreviated (it is not a group boundary in the format).
fn encode_group(out: &mut Vec<u8>, value: u32, digits: usize) {
    if digits == 5 && value == 0 {
        out.push(b'z');
        return;
    }
    let mut encoded = [0u8; 5];
    let mut rest = value;
    for slot in encoded.iter_mut().rev() {
        *slot = b'!' + (rest % 85) as u8;
        rest /= 85;
    }
    out.extend_from_slice(&encoded[..digits]);
}

enum DecodeError {
    Invalid,
    Memory,
}

/// Decode ASCII85, accepting optional `<~`/`~>` delimiters and ASCII
/// whitespace. The output is binary, not text.
fn decode_bytes(data: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let mut start = 0usize;
    let mut end = data.len();
    while start < end && data[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && data[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if data[start..end].starts_with(b"<~") {
        start += 2;
    }
    if data[start..end].ends_with(b"~>") {
        end -= 2;
    }

    // A `z` expands to four bytes, so that is the worst case per input byte.
    let capacity = data.len().saturating_mul(4).saturating_add(4);
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(capacity).map_err(|_| DecodeError::Memory)?;

    let mut value: u64 = 0;
    let mut group = 0usize;
    for &byte in &data[start..end] {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'z' {
            if group != 0 {
                return Err(DecodeError::Invalid);
            }
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        if !(b'!'..=b'u').contains(&byte) {
            return Err(DecodeError::Invalid);
        }
        value = value * 85 + u64::from(byte - b'!');
        group += 1;
        if group == 5 {
            if value > u64::from(u32::MAX) {
                return Err(DecodeError::Invalid);
            }
            out.extend_from_slice(&(value as u32).to_be_bytes());
            value = 0;
            group = 0;
        }
    }

    match group {
        0 => {}
        1 => return Err(DecodeError::Invalid),
        count => {
            // Pad the short group with the maximum digit, as the format
            // specifies, and keep only the bytes the encoder wrote.
            for _ in count..5 {
                value = value * 85 + 84;
            }
            if value > u64::from(u32::MAX) {
                return Err(DecodeError::Invalid);
            }
            let decoded = (value as u32).to_be_bytes();
            out.extend_from_slice(&decoded[..count - 1]);
        }
    }
    Ok(out)
}