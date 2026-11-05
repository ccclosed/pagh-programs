//! `base58` — Bitcoin base58 encoding and decoding.
//!
//! The alphabet is Bitcoin's `123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkm
//! nopqrstuvwxyz` (base58 excludes the visually ambiguous `0`, `O`, `I` and
//! `l`). Encoding is big-endian base conversion with one leading `1` per
//! leading zero byte, so arbitrary binary data round-trips exactly. Decoding
//! is the inverse: leading `1`s become zero bytes, and ASCII whitespace is
//! ignored so an encoded file (which ends in a newline) can be decoded
//! directly. Any other non-alphabet character is invalid input.
//!
//! One file operand or stdin (`-`), `-d`/`--decode` selects decoding. Exit:
//! 0 success, 1 invalid input or read/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: base58 [-d] [FILE]";
const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

pub fn run(args: &[String]) -> i32 {
    let mut decode = false;
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
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("base58: out of memory");
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
            util::report("base58", util::display_name(name), err);
            return util::EXIT_FAILURE;
        }
    };

    if decode {
        match decode_bytes(&data) {
            Ok(decoded) => {
                if let Err(err) = util::write_all(1, &decoded) {
                    eprintln!("base58: write error: {err}");
                    return util::EXIT_FAILURE;
                }
                util::EXIT_OK
            }
            Err(DecodeError::Invalid) => {
                eprintln!("base58: invalid input");
                util::EXIT_FAILURE
            }
            Err(DecodeError::Memory) => {
                eprintln!("base58: out of memory");
                util::EXIT_FAILURE
            }
        }
    } else {
        match encode_bytes(&data) {
            Some(encoded) => {
                if encoded.is_empty() {
                    return util::EXIT_OK;
                }
                if let Err(err) = util::write_line(1, &encoded) {
                    eprintln!("base58: write error: {err}");
                    return util::EXIT_FAILURE;
                }
                util::EXIT_OK
            }
            None => {
                eprintln!("base58: out of memory");
                util::EXIT_FAILURE
            }
        }
    }
}

enum DecodeError {
    Invalid,
    Memory,
}

/// Encode bytes with Bitcoin base58; `None` only on allocation failure.
fn encode_bytes(data: &[u8]) -> Option<Vec<u8>> {
    let zeros = data.iter().take_while(|byte| **byte == 0).count();
    // log(256)/log(58) < 1.38, so this reservation always covers the digits.
    let capacity = data.len().saturating_mul(138) / 100 + 1;
    let mut digits: Vec<u8> = Vec::new();
    digits.try_reserve(capacity).ok()?;

    for byte in data {
        let mut carry = u32::from(*byte);
        for digit in digits.iter_mut() {
            carry += u32::from(*digit) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }

    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(zeros.saturating_add(digits.len())).ok()?;
    for _ in 0..zeros {
        out.push(b'1');
    }
    for digit in digits.iter().rev() {
        out.push(ALPHABET[*digit as usize]);
    }
    Some(out)
}

/// Decode Bitcoin base58, ignoring ASCII whitespace between characters.
fn decode_bytes(data: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let capacity = data.len().saturating_mul(3) / 4 + 1;
    let mut bytes: Vec<u8> = Vec::new();
    bytes
        .try_reserve(capacity)
        .map_err(|_| DecodeError::Memory)?;

    let mut zeros = 0usize;
    let mut started = false;
    for &byte in data {
        if byte.is_ascii_whitespace() {
            continue;
        }
        let value = base58_value(byte).ok_or(DecodeError::Invalid)?;
        if value == 0 && !started {
            zeros += 1;
            continue;
        }
        started = true;
        let mut carry = u32::from(value);
        for slot in bytes.iter_mut() {
            carry += u32::from(*slot) * 58;
            *slot = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }

    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(zeros.saturating_add(bytes.len()))
        .map_err(|_| DecodeError::Memory)?;
    for _ in 0..zeros {
        out.push(0);
    }
    for byte in bytes.iter().rev() {
        out.push(*byte);
    }
    Ok(out)
}

fn base58_value(byte: u8) -> Option<u8> {
    ALPHABET.iter().position(|candidate| *candidate == byte).map(|index| index as u8)
}