//! `echo` — write arguments separated by spaces.
//!
//! Supported options (only before the first operand, like GNU echo):
//! `-n` (no trailing newline), `-e` (interpret backslash escapes),
//! `-E` (do not interpret; the default). `-e` understands
//! `\n \t \r \\ \a \b \f \v \e`, `\0NNN` (up to three octal digits),
//! `\xHH` (up to two hex digits) and `\c` (stop, no newline).
//!
//! Exit: 0 normally, 1 if fd 1 cannot be written.

use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

pub fn run(args: &[alloc::string::String]) -> i32 {
    let mut newline = true;
    let mut escapes = false;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        if arg.len() > 1
            && arg.starts_with('-')
            && arg[1..]
                .chars()
                .all(|c| matches!(c, 'n' | 'e' | 'E'))
        {
            for c in arg[1..].chars() {
                match c {
                    'n' => newline = false,
                    'e' => escapes = true,
                    'E' => escapes = false,
                    _ => {}
                }
            }
            index += 1;
        } else {
            break;
        }
    }

    let mut out = Vec::new();
    let mut stopped = false;
    for (position, arg) in args[index..].iter().enumerate() {
        if position > 0 {
            out.push(b' ');
        }
        if escapes {
            if append_escaped(&mut out, arg) {
                stopped = true;
                break;
            }
        } else {
            out.extend_from_slice(arg.as_bytes());
        }
    }

    if newline && !stopped {
        out.push(b'\n');
    }

    match util::write_all(1, &out) {
        Ok(()) => util::EXIT_OK,
        Err(err) => {
            eprintln!("echo: write error: {err}");
            util::EXIT_FAILURE
        }
    }
}

/// Append `text` with `-e` escapes. Returns true when `\c` asked to stop.
fn append_escaped(out: &mut Vec<u8>, text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            out.push(bytes[index]);
            index += 1;
            continue;
        }
        index += 1;
        if index == bytes.len() {
            out.push(b'\\');
            break;
        }
        match bytes[index] {
            b'n' => {
                out.push(b'\n');
                index += 1;
            }
            b't' => {
                out.push(b'\t');
                index += 1;
            }
            b'r' => {
                out.push(b'\r');
                index += 1;
            }
            b'a' => {
                out.push(0x07);
                index += 1;
            }
            b'b' => {
                out.push(0x08);
                index += 1;
            }
            b'f' => {
                out.push(0x0c);
                index += 1;
            }
            b'v' => {
                out.push(0x0b);
                index += 1;
            }
            b'e' => {
                out.push(0x1b);
                index += 1;
            }
            b'\\' => {
                out.push(b'\\');
                index += 1;
            }
            b'c' => return true,
            b'0' => {
                index += 1;
                let (value, used) = parse_radix(&bytes[index..], 8, 3);
                if used == 0 {
                    out.push(0);
                } else {
                    out.push(value);
                    index += used;
                }
            }
            b'x' => {
                index += 1;
                let (value, used) = parse_radix(&bytes[index..], 16, 2);
                if used == 0 {
                    out.push(b'\\');
                    out.push(b'x');
                } else {
                    out.push(value);
                    index += used;
                }
            }
            other => {
                // Unknown escape: keep both bytes, as GNU echo does.
                out.push(b'\\');
                out.push(other);
                index += 1;
            }
        }
    }
    false
}

/// Parse at most `limit` digits in `radix`; returns `(value, digits_used)`.
/// Overflowing a byte is masked, which is what the shell builtins do.
fn parse_radix(bytes: &[u8], radix: u32, limit: usize) -> (u8, usize) {
    let mut value: u32 = 0;
    let mut used = 0;
    while used < limit && used < bytes.len() {
        let digit = match (bytes[used] as char).to_digit(radix) {
            Some(digit) => digit,
            None => break,
        };
        value = value * radix + digit;
        used += 1;
    }
    ((value & 0xff) as u8, used)
}
