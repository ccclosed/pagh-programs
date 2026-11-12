//! `iconv` — convert text between character encodings.
//!
//! ```text
//! iconv -f FROM -t TO [-o FILE] [FILE]
//! iconv -l
//! ```
//!
//! Supported encodings (the whole set; anything else is a usage error, never a
//! lossy guess): `UTF-8`, `ASCII`, `ISO-8859-1` (a.k.a. `LATIN1`), `UTF-16LE`,
//! `UTF-16BE`. A conversion is **fail-closed**: an invalid input sequence, an
//! odd UTF-16 length, an unpaired surrogate or a character the target cannot
//! represent stops the conversion and exits 1. The applet never substitutes
//! `?`/U+FFFD unless the input itself contained one.
//!
//! `-l` lists the supported names. There is no locale in this ABI, so `-f` and
//! `-t` are mandatory (GNU iconv would read them from the locale; there is
//! none to read). One input operand (or stdin/`-`); with `-o FILE` the result
//! is written there, otherwise to stdout. The input is buffered whole, so a
//! conversion error leaves the output file untouched.
//!
//! This is a documented subset, not a charset library: no `UTF-16` with BOM
//! sniffing, no `UTF-32`, no MIME/code-page aliases beyond the names listed by
//! `-l`.
//!
//! Exit: 0 success; 1 invalid input, unrepresentable character or read/write
//! error; 2 usage (missing `-f`/`-t`, unknown option, unsupported charset,
//! more than one operand).

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;

use crate::util;

const USAGE: &str = "usage: iconv -f FROM -t TO [-o FILE] [FILE]\n       iconv -l";

/// The encodings this applet implements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Charset {
    Utf8,
    Ascii,
    Latin1,
    Utf16Le,
    Utf16Be,
}

impl Charset {
    /// The canonical spelling used in diagnostics and in `-l`.
    fn name(self) -> &'static str {
        match self {
            Charset::Utf8 => "UTF-8",
            Charset::Ascii => "ASCII",
            Charset::Latin1 => "ISO-8859-1",
            Charset::Utf16Le => "UTF-16LE",
            Charset::Utf16Be => "UTF-16BE",
        }
    }
}

const CHARSETS: [Charset; 5] = [
    Charset::Utf8,
    Charset::Ascii,
    Charset::Latin1,
    Charset::Utf16Le,
    Charset::Utf16Be,
];

/// Decode a name case-insensitively, ignoring `-`/`_` and a few common
/// spellings (`utf8`, `us-ascii`, `iso8859-1`, `latin1`, ...).
fn parse_charset(text: &str) -> Option<Charset> {
    let lowered = text.to_ascii_lowercase();
    let mut key = String::new();
    for byte in lowered.bytes() {
        if byte != b'-' && byte != b'_' && byte != b' ' {
            key.push(byte as char);
        }
    }
    match key.as_str() {
        "utf8" => Some(Charset::Utf8),
        "ascii" | "usascii" | "ansi" => Some(Charset::Ascii),
        "latin1" | "iso88591" | "iso8859" => Some(Charset::Latin1),
        "utf16le" => Some(Charset::Utf16Le),
        "utf16be" => Some(Charset::Utf16Be),
        _ => None,
    }
}

pub fn run(args: &[String]) -> i32 {
    let mut from: Option<(Charset, String)> = None;
    let mut to: Option<(Charset, String)> = None;
    let mut output: Option<String> = None;
    let mut list = false;
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
        if parse_options && arg == "-l" {
            list = true;
            continue;
        }
        if parse_options && (arg == "-f" || arg == "-t" || arg == "-o") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            if let Err(code) = set_option(arg, value, &mut from, &mut to, &mut output) {
                return code;
            }
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with('-') {
            let flag = &arg[..2];
            if flag == "-f" || flag == "-t" || flag == "-o" {
                let value = &arg[2..];
                if let Err(code) = set_option(flag, value, &mut from, &mut to, &mut output) {
                    return code;
                }
                continue;
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("iconv: unknown option '{arg}'");
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("iconv: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if list {
        if !operands.is_empty() || from.is_some() || to.is_some() || output.is_some() {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        for charset in CHARSETS {
            libpagh::println!("{}", charset.name());
        }
        return util::EXIT_OK;
    }

    let (Some((from, from_name)), Some((to, to_name))) = (from, to) else {
        eprintln!("iconv: -f FROM and -t TO are required (this ABI has no locale)");
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };
    if operands.len() > 1 {
        eprintln!("iconv: at most one input file");
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let operand = operands.first().copied().unwrap_or("-");

    let input = match util::read_input(operand) {
        Ok(input) => input,
        Err(err) => {
            util::report("iconv", util::display_name(operand), err);
            return util::EXIT_FAILURE;
        }
    };

    let characters = match decode(from, &input) {
        Ok(characters) => characters,
        Err(DecodeError::InvalidInput { offset }) => {
            eprintln!("iconv: invalid input for {} at byte {}", from_name, offset);
            return util::EXIT_FAILURE;
        }
        Err(DecodeError::OutOfMemory) => {
            eprintln!("iconv: out of memory");
            return util::EXIT_FAILURE;
        }
    };
    let converted = match encode(to, &characters) {
        Ok(converted) => converted,
        Err(EncodeError::Unrepresentable { codepoint }) => {
            eprintln!("iconv: cannot convert U+{:04X} to {}", codepoint, to_name);
            return util::EXIT_FAILURE;
        }
        Err(EncodeError::OutOfMemory) => {
            eprintln!("iconv: out of memory");
            return util::EXIT_FAILURE;
        }
    };

    match output {
        Some(path) => {
            if let Err(err) = fs::write_all(&path, &converted) {
                util::report("iconv", &path, err);
                util::EXIT_FAILURE
            } else {
                util::EXIT_OK
            }
        }
        None => match util::write_all(1, &converted) {
            Ok(()) => util::EXIT_OK,
            Err(err) => {
                eprintln!("iconv: write error: {err}");
                util::EXIT_FAILURE
            }
        },
    }
}

fn set_option(
    flag: &str,
    value: &str,
    from: &mut Option<(Charset, String)>,
    to: &mut Option<(Charset, String)>,
    output: &mut Option<String>,
) -> Result<(), i32> {
    if flag == "-o" {
        if value.is_empty() {
            eprintln!("iconv: empty -o");
            return Err(util::EXIT_USAGE);
        }
        *output = Some(String::from(value));
        return Ok(());
    }
    let Some(charset) = parse_charset(value) else {
        eprintln!("iconv: unsupported charset '{value}' (see -l)");
        return Err(util::EXIT_USAGE);
    };
    if flag == "-f" {
        *from = Some((charset, String::from(charset.name())));
    } else {
        *to = Some((charset, String::from(charset.name())));
    }
    Ok(())
}

enum DecodeError {
    /// A byte sequence that is not valid in the source encoding.
    InvalidInput { offset: usize },
    /// The scratch buffer could not be allocated.
    OutOfMemory,
}

enum EncodeError {
    /// A code point the target encoding cannot represent.
    Unrepresentable { codepoint: u32 },
    /// The output buffer could not be allocated.
    OutOfMemory,
}

/// Decode `data` to Unicode scalar values.
fn decode(charset: Charset, data: &[u8]) -> Result<Vec<u32>, DecodeError> {
    let mut out: Vec<u32> = Vec::new();
    // Every encoding emits at most one code point per input byte (UTF-8) and
    // UTF-16 at most one per 2 bytes; reserving `len` is always enough.
    if out.try_reserve(data.len()).is_err() {
        return Err(DecodeError::OutOfMemory);
    }
    match charset {
        Charset::Ascii => {
            for (offset, byte) in data.iter().enumerate() {
                if *byte < 0x80 {
                    out.push(u32::from(*byte));
                } else {
                    return Err(DecodeError::InvalidInput { offset });
                }
            }
        }
        Charset::Latin1 => {
            for byte in data {
                out.push(u32::from(*byte));
            }
        }
        Charset::Utf8 => decode_utf8(data, &mut out)?,
        Charset::Utf16Le => decode_utf16(data, false, &mut out)?,
        Charset::Utf16Be => decode_utf16(data, true, &mut out)?,
    }
    Ok(out)
}

fn decode_utf8(data: &[u8], out: &mut Vec<u32>) -> Result<(), DecodeError> {
    let mut index = 0usize;
    while index < data.len() {
        let first = data[index];
        let (codepoint, length) = if first < 0x80 {
            (u32::from(first), 1)
        } else if (0xC2..=0xDF).contains(&first) {
            (u32::from(first & 0x1F), 2)
        } else if (0xE0..=0xEF).contains(&first) {
            (u32::from(first & 0x0F), 3)
        } else if (0xF0..=0xF4).contains(&first) {
            (u32::from(first & 0x07), 4)
        } else {
            return Err(DecodeError::InvalidInput { offset: index });
        };
        if index + length > data.len() {
            return Err(DecodeError::InvalidInput { offset: index });
        }
        let mut codepoint = codepoint;
        for offset in 1..length {
            let continuation = data[index + offset];
            if continuation & 0xC0 != 0x80 {
                return Err(DecodeError::InvalidInput {
                    offset: index + offset,
                });
            }
            codepoint = (codepoint << 6) | u32::from(continuation & 0x3F);
        }
        // Reject overlong forms, UTF-8-encoded surrogates and values above
        // the Unicode ceiling: the first-byte range alone is not enough (for
        // example E0 80 80 decodes to 0 only because it is overlong).
        let shortest = if codepoint < 0x80 {
            1
        } else if codepoint < 0x800 {
            2
        } else if codepoint < 0x1_0000 {
            3
        } else {
            4
        };
        if length != shortest || (0xD800..=0xDFFF).contains(&codepoint) || codepoint > 0x10FFFF {
            return Err(DecodeError::InvalidInput { offset: index });
        }
        out.push(codepoint);
        index += length;
    }
    Ok(())
}

fn decode_utf16(data: &[u8], big_endian: bool, out: &mut Vec<u32>) -> Result<(), DecodeError> {
    if !data.len().is_multiple_of(2) {
        // The trailing byte cannot be a unit; report the odd byte.
        return Err(DecodeError::InvalidInput {
            offset: data.len() - 1,
        });
    }
    let mut index = 0usize;
    while index < data.len() {
        let unit = read_unit(data, index, big_endian);
        if (0xD800..=0xDBFF).contains(&unit) {
            let next = index + 2;
            if next + 2 > data.len() {
                return Err(DecodeError::InvalidInput { offset: index });
            }
            let low = read_unit(data, next, big_endian);
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(DecodeError::InvalidInput { offset: next });
            }
            let codepoint =
                0x10000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
            out.push(codepoint);
            index += 4;
        } else if (0xDC00..=0xDFFF).contains(&unit) {
            return Err(DecodeError::InvalidInput { offset: index });
        } else {
            out.push(u32::from(unit));
            index += 2;
        }
    }
    Ok(())
}

fn read_unit(data: &[u8], index: usize, big_endian: bool) -> u16 {
    if big_endian {
        (u16::from(data[index]) << 8) | u16::from(data[index + 1])
    } else {
        (u16::from(data[index + 1]) << 8) | u16::from(data[index])
    }
}

/// Encode Unicode scalar values in `charset`.
fn encode(charset: Charset, characters: &[u32]) -> Result<Vec<u8>, EncodeError> {
    let mut out: Vec<u8> = Vec::new();
    // UTF-8 is at most 4 bytes per code point, UTF-16 at most 4 per pair; 4x
    // the number of code points is a safe upper bound for every target.
    if out.try_reserve(characters.len().saturating_mul(4)).is_err() {
        return Err(EncodeError::OutOfMemory);
    }
    for &codepoint in characters {
        match charset {
            Charset::Ascii => {
                if codepoint > 0x7F {
                    return Err(EncodeError::Unrepresentable { codepoint });
                }
                out.push(codepoint as u8);
            }
            Charset::Latin1 => {
                if codepoint > 0xFF {
                    return Err(EncodeError::Unrepresentable { codepoint });
                }
                out.push(codepoint as u8);
            }
            Charset::Utf8 => encode_utf8(codepoint, &mut out),
            Charset::Utf16Le => encode_utf16(codepoint, false, &mut out),
            Charset::Utf16Be => encode_utf16(codepoint, true, &mut out),
        }
    }
    Ok(out)
}

fn encode_utf8(codepoint: u32, out: &mut Vec<u8>) {
    if codepoint < 0x80 {
        out.push(codepoint as u8);
    } else if codepoint < 0x800 {
        out.push(0xC0 | (codepoint >> 6) as u8);
        out.push(0x80 | (codepoint & 0x3F) as u8);
    } else if codepoint < 0x1_0000 {
        out.push(0xE0 | (codepoint >> 12) as u8);
        out.push(0x80 | ((codepoint >> 6) & 0x3F) as u8);
        out.push(0x80 | (codepoint & 0x3F) as u8);
    } else {
        out.push(0xF0 | (codepoint >> 18) as u8);
        out.push(0x80 | ((codepoint >> 12) & 0x3F) as u8);
        out.push(0x80 | ((codepoint >> 6) & 0x3F) as u8);
        out.push(0x80 | (codepoint & 0x3F) as u8);
    }
}

fn encode_utf16(codepoint: u32, big_endian: bool, out: &mut Vec<u8>) {
    let mut push = |unit: u16| {
        if big_endian {
            out.push((unit >> 8) as u8);
            out.push((unit & 0xFF) as u8);
        } else {
            out.push((unit & 0xFF) as u8);
            out.push((unit >> 8) as u8);
        }
    };
    if codepoint < 0x1_0000 {
        // Decode already rejected unpaired surrogates, so this is a BMP scalar.
        push(codepoint as u16);
    } else {
        let offset = codepoint - 0x1_0000;
        push(0xD800 + (offset >> 10) as u16);
        push(0xDC00 + (offset & 0x3FF) as u16);
    }
}
