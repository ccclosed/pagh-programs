//! Classic uuencode/uudecode helpers shared by `uuencode` and `uudecode`.
//!
//! The data format is the historical one: a `begin MODE NAME` line, lines of
//! at most 45 source bytes, a length character, four encoded characters per
//! three bytes, and an `end` line. Six-bit value 0 is written as a backtick
//! (the form that survives trailing-whitespace stripping); the decoder also
//! accepts a space for 0, which is what some encoders emit. There is no
//! base64 (`-m`) mode.

use alloc::string::String;
use alloc::vec::Vec;

use crate::util;

const MAX_LINE: usize = 45;

pub enum DecodeError {
    NoBegin,
    NoEnd,
    Invalid,
    OutOfMemory,
}

pub struct Decoded {
    pub name: String,
    pub data: Vec<u8>,
}

/// Append one uuencoded data line (at most 45 bytes) to `out`, including the
/// terminating newline. The caller has already checked the chunk length.
pub fn encode_line(chunk: &[u8], out: &mut String) {
    let length = chunk.len().min(MAX_LINE);
    if length == 0 {
        out.push('`');
        out.push('\n');
        return;
    }
    out.push(char::from(b' ' + length as u8));
    let mut index = 0usize;
    while index < length {
        let first = chunk[index];
        let second = chunk.get(index + 1).copied().unwrap_or(0);
        let third = chunk.get(index + 2).copied().unwrap_or(0);
        let value = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        push_char(out, ((value >> 18) & 0x3f) as u8);
        push_char(out, ((value >> 12) & 0x3f) as u8);
        push_char(out, ((value >> 6) & 0x3f) as u8);
        push_char(out, (value & 0x3f) as u8);
        index += 3;
    }
    out.push('\n');
}

fn push_char(out: &mut String, value: u8) {
    if value == 0 {
        out.push('`');
    } else {
        out.push(char::from(value + 0x20));
    }
}

/// Decode a complete uuencoded message.
pub fn decode(data: &[u8]) -> Result<Decoded, DecodeError> {
    let mut name: Option<String> = None;
    let mut bytes: Vec<u8> = Vec::new();
    let mut finished = false;

    for raw_line in util::lines(data) {
        let line = trim_cr(raw_line);
        if name.is_none() {
            let Some(rest) = line.strip_prefix(b"begin ") else {
                continue;
            };
            // `MODE NAME`: the mode is one token, the name is everything after
            // the following space (names may contain spaces).
            let split = rest.iter().position(|byte| *byte == b' ');
            let (mode, remainder) = match split {
                Some(position) => (&rest[..position], &rest[position + 1..]),
                None => (rest, &b""[..]),
            };
            if mode.is_empty() || remainder.is_empty() {
                return Err(DecodeError::Invalid);
            }
            let decoded_name = String::from_utf8_lossy(remainder).into_owned();
            if decoded_name.is_empty() || decoded_name.contains('\n') {
                return Err(DecodeError::Invalid);
            }
            name = Some(decoded_name);
            continue;
        }

        if line == b"end" {
            finished = true;
            break;
        }
        if line.is_empty() {
            continue;
        }
        decode_line(line, &mut bytes)?;
    }

    if name.is_none() {
        return Err(DecodeError::NoBegin);
    }
    if !finished {
        return Err(DecodeError::NoEnd);
    }
    Ok(Decoded {
        name: name.unwrap_or_default(),
        data: bytes,
    })
}

fn decode_line(line: &[u8], out: &mut Vec<u8>) -> Result<(), DecodeError> {
    let Some((first, rest)) = line.split_first() else {
        return Ok(());
    };
    let length = decode_char(*first).ok_or(DecodeError::Invalid)? as usize;
    if length > MAX_LINE {
        return Err(DecodeError::Invalid);
    }
    let groups = (length + 2) / 3;
    let needed = groups.saturating_mul(4);
    if rest.len() < needed {
        return Err(DecodeError::Invalid);
    }
    if out.try_reserve(length).is_err() {
        return Err(DecodeError::OutOfMemory);
    }
    let mut produced = 0usize;
    for group in rest[..needed].chunks(4) {
        let a = decode_char(group[0]).ok_or(DecodeError::Invalid)? as u32;
        let b = decode_char(group[1]).ok_or(DecodeError::Invalid)? as u32;
        let c = decode_char(group[2]).ok_or(DecodeError::Invalid)? as u32;
        let d = decode_char(group[3]).ok_or(DecodeError::Invalid)? as u32;
        let value = (a << 18) | (b << 12) | (c << 6) | d;
        for byte in [(value >> 16) as u8, (value >> 8) as u8, value as u8] {
            if produced >= length {
                break;
            }
            out.push(byte);
            produced += 1;
        }
    }
    Ok(())
}

fn decode_char(byte: u8) -> Option<u8> {
    match byte {
        b'`' | b' ' => Some(0),
        0x21..=0x5f => Some(byte - 0x20),
        _ => None,
    }
}

fn trim_cr(line: &[u8]) -> &[u8] {
    match line.split_last() {
        Some((b'\r', rest)) => rest,
        _ => line,
    }
}
