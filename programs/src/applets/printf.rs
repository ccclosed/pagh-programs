//! `printf FORMAT [ARG...]` — formatted output with a small conversion set.
//!
//! Supported conversions: `%s`, `%d`, `%i`, `%x`, `%X`, `%o`, `%c`, `%%`.
//! Supported format escapes: `\n \t \r \\ \" \a \b \f \v \e`, `\0NNN`/`\NNN`
//! (up to three octal digits) and `\xHH` (one or two hex digits). There are no
//! width, precision, flag or positional forms; `%5d` is a malformed format.
//!
//! The format is reused while arguments remain (POSIX `printf` behaviour):
//! `printf '%s\n' a b` prints two lines. Within one pass, conversions consume
//! arguments left to right; a missing argument becomes `0` for the numeric
//! conversions and the empty string for `%s`/`%c`. The arguments are **not**
//! wrapped around: after the last one, further conversions see the defaults.
//!
//! A malformed format (trailing `%` or `\`, unknown conversion, `\x` without a
//! hex digit) or a numeric argument that is not an `i64` prints a diagnostic
//! and exits 1. No operands at all is a usage error (2). Output always goes to
//! fd 1; the shell has no pipes or redirection.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: printf FORMAT [ARG...]";

enum FormatError {
    Malformed(&'static str),
    Number(String),
    OutOfMemory,
}

pub fn run(args: &[String]) -> i32 {
    let mut start = 0usize;
    if args.first().map(String::as_str) == Some("--") {
        start = 1;
    }
    let Some(format) = args.get(start).map(String::as_str) else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };
    let values = &args[start + 1..];

    let mut index = 0usize;
    loop {
        let before = index;
        match render(format, values, &mut index) {
            Ok(output) => {
                if let Err(err) = util::write_all(1, &output) {
                    eprintln!("printf: write error: {err}");
                    return util::EXIT_FAILURE;
                }
            }
            Err(error) => {
                match error {
                    FormatError::Malformed(reason) => {
                        eprintln!("printf: invalid format: {reason}");
                    }
                    FormatError::Number(value) => {
                        eprintln!("printf: '{value}': expected an integer");
                    }
                    FormatError::OutOfMemory => eprintln!("printf: out of memory"),
                }
                return util::EXIT_FAILURE;
            }
        }

        if index >= values.len() || index == before {
            break;
        }
    }
    util::EXIT_OK
}

fn render(
    format: &str,
    values: &[String],
    index: &mut usize,
) -> Result<Vec<u8>, FormatError> {
    let bytes = format.as_bytes();
    let mut output: Vec<u8> = Vec::new();
    output
        .try_reserve(bytes.len().saturating_add(16))
        .map_err(|_| FormatError::OutOfMemory)?;

    let mut position = 0usize;
    while position < bytes.len() {
        match bytes[position] {
            b'\\' => {
                position += 1;
                if position >= bytes.len() {
                    return Err(FormatError::Malformed("trailing backslash"));
                }
                let escaped = bytes[position];
                position += 1;
                match escaped {
                    b'n' => push(&mut output, b'\n')?,
                    b't' => push(&mut output, b'\t')?,
                    b'r' => push(&mut output, b'\r')?,
                    b'\\' => push(&mut output, b'\\')?,
                    b'"' => push(&mut output, b'"')?,
                    b'a' => push(&mut output, 0x07)?,
                    b'b' => push(&mut output, 0x08)?,
                    b'f' => push(&mut output, 0x0c)?,
                    b'v' => push(&mut output, 0x0b)?,
                    b'e' => push(&mut output, 0x1b)?,
                    b'0'..=b'7' => {
                        let mut value = (escaped - b'0') as u32;
                        // `\0` is followed by up to three octal digits; the
                        // shorter `\NNN` form already used one digit.
                        let limit = if escaped == b'0' { 3 } else { 2 };
                        let mut taken = 0;
                        while taken < limit
                            && position < bytes.len()
                            && (b'0'..=b'7').contains(&bytes[position])
                        {
                            value = value * 8 + (bytes[position] - b'0') as u32;
                            position += 1;
                            taken += 1;
                        }
                        push(&mut output, (value & 0xff) as u8)?;
                    }
                    b'x' => {
                        let mut value = 0u32;
                        let mut taken = 0;
                        while taken < 2 && position < bytes.len() {
                            let Some(digit) = hex_value(bytes[position]) else {
                                break;
                            };
                            value = value * 16 + digit;
                            position += 1;
                            taken += 1;
                        }
                        if taken == 0 {
                            return Err(FormatError::Malformed("\\x without hex digits"));
                        }
                        push(&mut output, (value & 0xff) as u8)?;
                    }
                    other => {
                        // An unknown escape is kept literally in both bytes,
                        // which is what sh's printf does; it is not malformed.
                        push(&mut output, b'\\')?;
                        push(&mut output, other)?;
                    }
                }
            }
            b'%' => {
                position += 1;
                if position >= bytes.len() {
                    return Err(FormatError::Malformed("trailing %"));
                }
                let conversion = bytes[position];
                position += 1;
                match conversion {
                    b'%' => push(&mut output, b'%')?,
                    b's' => {
                        let value = take_value(values, index);
                        extend(&mut output, value.as_bytes())?;
                    }
                    b'c' => {
                        let value = take_value(values, index);
                        if let Some(first) = value.as_bytes().first() {
                            push(&mut output, *first)?;
                        }
                    }
                    b'd' | b'i' => {
                        let number = take_number(values, index)?;
                        let text = format!("{number}");
                        extend(&mut output, text.as_bytes())?;
                    }
                    b'x' => {
                        let number = take_number(values, index)? as u64;
                        let text = format!("{number:x}");
                        extend(&mut output, text.as_bytes())?;
                    }
                    b'X' => {
                        let number = take_number(values, index)? as u64;
                        let text = format!("{number:X}");
                        extend(&mut output, text.as_bytes())?;
                    }
                    b'o' => {
                        let number = take_number(values, index)? as u64;
                        let text = format!("{number:o}");
                        extend(&mut output, text.as_bytes())?;
                    }
                    other => {
                        let reason = match other {
                            b'q' => "unsupported conversion %q",
                            _ => "unsupported conversion",
                        };
                        return Err(FormatError::Malformed(reason));
                    }
                }
            }
            literal => {
                push(&mut output, literal)?;
                position += 1;
            }
        }
    }
    Ok(output)
}

fn take_value<'a>(values: &'a [String], index: &mut usize) -> &'a str {
    match values.get(*index) {
        Some(value) => {
            *index += 1;
            value.as_str()
        }
        None => "",
    }
}

fn take_number(values: &[String], index: &mut usize) -> Result<i64, FormatError> {
    let value = take_value(values, index);
    if value.is_empty() {
        return Ok(0);
    }
    value
        .parse::<i64>()
        .map_err(|_| FormatError::Number(String::from(value)))
}

fn push(output: &mut Vec<u8>, byte: u8) -> Result<(), FormatError> {
    output.try_reserve(1).map_err(|_| FormatError::OutOfMemory)?;
    output.push(byte);
    Ok(())
}

fn extend(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), FormatError> {
    output
        .try_reserve(bytes.len())
        .map_err(|_| FormatError::OutOfMemory)?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn hex_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some((byte - b'0') as u32),
        b'a'..=b'f' => Some((byte - b'a' + 10) as u32),
        b'A'..=b'F' => Some((byte - b'A' + 10) as u32),
        _ => None,
    }
}