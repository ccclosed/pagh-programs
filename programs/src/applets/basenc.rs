//! `basenc` — encode or decode with the RFC 4648 bases used by coreutils.
//!
//! Supported encodings:
//!
//! * `--base64` and `--base64url` (6-bit alphabet, `=` padding);
//! * `--base32` and `--base32hex` (5-bit alphabet, `=` padding);
//! * `--base16` (uppercase hex).
//!
//! `-d`/`--decode` selects decoding, `-w N`/`--wrap=N` changes the line width
//! for encoding (76 by default, 0 disables wrapping), `-i`/`--ignore-garbage`
//! skips non-alphabet bytes while decoding. Decoding ignores ASCII whitespace,
//! accepts unpadded input (like GNU `basenc`), and rejects a misplaced `=`, an
//! impossible group length, a non-zero trailing bit or an invalid character.
//! Files are concatenated as one stream; `-` is stdin and no operand means
//! stdin. Exit: 0 success, 1 invalid input or I/O error, 2 usage.
//!
//! `base32hex` is included because it is just another 32-character alphabet;
//! `base2`/`base8`/`base64url` with the `zbase32` alphabet are not.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str =
    "usage: basenc (--base64|--base64url|--base32|--base32hex|--base16) [-d] [-w N] [FILE...]";

const BASE64: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE64URL: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const BASE32HEX: &[u8; 32] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";
const BASE16: &[u8; 16] = b"0123456789ABCDEF";

#[derive(Clone, Copy, Eq, PartialEq)]
enum Encoding {
    Base64,
    Base64Url,
    Base32,
    Base32Hex,
    Base16,
}

impl Encoding {
    fn bits(self) -> u32 {
        match self {
            Encoding::Base64 | Encoding::Base64Url => 6,
            Encoding::Base32 | Encoding::Base32Hex => 5,
            Encoding::Base16 => 4,
        }
    }

    fn group(self) -> usize {
        match self {
            Encoding::Base64 | Encoding::Base64Url => 4,
            Encoding::Base32 | Encoding::Base32Hex => 8,
            Encoding::Base16 => 2,
        }
    }

    fn alphabet(self) -> &'static [u8] {
        match self {
            Encoding::Base64 => BASE64,
            Encoding::Base64Url => BASE64URL,
            Encoding::Base32 => BASE32,
            Encoding::Base32Hex => BASE32HEX,
            Encoding::Base16 => BASE16,
        }
    }
}

pub fn run(args: &[String]) -> i32 {
    let mut encoding: Option<Encoding> = None;
    let mut decode = false;
    let mut ignore_garbage = false;
    let mut wrap = 76usize;
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
                "--decode" => {
                    decode = true;
                    continue;
                }
                "--ignore-garbage" => {
                    ignore_garbage = true;
                    continue;
                }
                "--base64" => {
                    if !set_encoding(&mut encoding, Encoding::Base64) {
                        return usage();
                    }
                    continue;
                }
                "--base64url" => {
                    if !set_encoding(&mut encoding, Encoding::Base64Url) {
                        return usage();
                    }
                    continue;
                }
                "--base32" => {
                    if !set_encoding(&mut encoding, Encoding::Base32) {
                        return usage();
                    }
                    continue;
                }
                "--base32hex" => {
                    if !set_encoding(&mut encoding, Encoding::Base32Hex) {
                        return usage();
                    }
                    continue;
                }
                "--base16" => {
                    if !set_encoding(&mut encoding, Encoding::Base16) {
                        return usage();
                    }
                    continue;
                }
                _ if arg.starts_with("--wrap=") => {
                    let Some(value) = parse_width(&arg[7..]) else {
                        eprintln!("basenc: invalid wrap width: '{}'", &arg[7..]);
                        return util::EXIT_USAGE;
                    };
                    wrap = value;
                    continue;
                }
                "--wrap" => {
                    let Some(value) = args.get(index).map(String::as_str) else {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    };
                    index += 1;
                    let Some(parsed) = parse_width(value) else {
                        eprintln!("basenc: invalid wrap width: '{value}'");
                        return util::EXIT_USAGE;
                    };
                    wrap = parsed;
                    continue;
                }
                _ => {
                    eprintln!("basenc: unrecognized option '{arg}'");
                    return usage();
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            if arg == "-d" {
                decode = true;
                continue;
            }
            if arg == "-i" {
                ignore_garbage = true;
                continue;
            }
            if arg == "-w" {
                let Some(value) = args.get(index).map(String::as_str) else {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                };
                index += 1;
                let Some(parsed) = parse_width(value) else {
                    eprintln!("basenc: invalid wrap width: '{value}'");
                    return util::EXIT_USAGE;
                };
                wrap = parsed;
                continue;
            }
            if arg.len() > 2 && arg.starts_with("-w") {
                let Some(parsed) = parse_width(&arg[2..]) else {
                    eprintln!("basenc: invalid wrap width: '{}'", &arg[2..]);
                    return util::EXIT_USAGE;
                };
                wrap = parsed;
                continue;
            }
            eprintln!("basenc: invalid option -- '{}'", &arg[1..]);
            return usage();
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("basenc: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let Some(encoding) = encoding else {
        eprintln!("basenc: missing encoding type");
        return usage();
    };

    if operands.is_empty() {
        operands.push("-");
    }

    let mut data: Vec<u8> = Vec::new();
    for name in operands {
        match util::read_input(name) {
            Ok(part) => {
                if data.try_reserve(part.len()).is_err() {
                    eprintln!("basenc: out of memory");
                    return util::EXIT_FAILURE;
                }
                data.extend_from_slice(&part);
            }
            Err(err) => {
                util::report("basenc", util::display_name(name), err);
                return util::EXIT_FAILURE;
            }
        }
    }

    if decode {
        let decoded = match encoding {
            Encoding::Base16 => decode_base16(&data, ignore_garbage),
            _ => decode_bits(&data, encoding, ignore_garbage),
        };
        match decoded {
            Ok(bytes) => {
                if let Err(err) = util::write_all(1, &bytes) {
                    eprintln!("basenc: write error: {err}");
                    return util::EXIT_FAILURE;
                }
                util::EXIT_OK
            }
            Err(()) => {
                eprintln!("basenc: invalid input");
                util::EXIT_FAILURE
            }
        }
    } else {
        let encoded = match encoding {
            Encoding::Base16 => encode_base16(&data),
            _ => encode_bits(&data, encoding),
        };
        let Some(encoded) = encoded else {
            eprintln!("basenc: out of memory");
            return util::EXIT_FAILURE;
        };
        match write_wrapped(&encoded, wrap) {
            Ok(()) => util::EXIT_OK,
            Err(err) => {
                eprintln!("basenc: write error: {err}");
                util::EXIT_FAILURE
            }
        }
    }
}

fn usage() -> i32 {
    eprintln!("{USAGE}");
    util::EXIT_USAGE
}

fn set_encoding(slot: &mut Option<Encoding>, value: Encoding) -> bool {
    if slot.is_some() {
        eprintln!("basenc: only one encoding type may be specified");
        return false;
    }
    *slot = Some(value);
    true
}

fn parse_width(text: &str) -> Option<usize> {
    text.parse::<usize>().ok()
}

fn write_wrapped(data: &[u8], wrap: usize) -> libpagh::Result<()> {
    if data.is_empty() {
        return Ok(());
    }
    if wrap == 0 {
        return util::write_all(1, data);
    }
    for chunk in data.chunks(wrap) {
        util::write_all(1, chunk)?;
        util::write_all(1, b"\n")?;
    }
    Ok(())
}

fn encode_bits(data: &[u8], encoding: Encoding) -> Option<Vec<u8>> {
    let bits = encoding.bits();
    let alphabet = encoding.alphabet();
    let group = encoding.group();
    let mask = (1u64 << bits) - 1;
    let mut out: Vec<u8> = Vec::new();
    let estimate = if bits == 6 {
        (data.len() / 3 + 1) * 4
    } else {
        (data.len() / 5 + 1) * 8
    };
    out.try_reserve(estimate).ok()?;

    let mut accumulator = 0u64;
    let mut count = 0u32;
    for byte in data {
        accumulator = (accumulator << 8) | *byte as u64;
        count += 8;
        while count >= bits {
            count -= bits;
            let index = ((accumulator >> count) & mask) as usize;
            out.push(alphabet[index]);
        }
        accumulator &= if count == 0 { 0 } else { (1u64 << count) - 1 };
    }
    if count > 0 {
        let index = ((accumulator << (bits - count)) & mask) as usize;
        out.push(alphabet[index]);
    }
    let padding = (group - out.len() % group) % group;
    for _ in 0..padding {
        out.push(b'=');
    }
    Some(out)
}

fn decode_bits(data: &[u8], encoding: Encoding, ignore_garbage: bool) -> Result<Vec<u8>, ()> {
    let bits = encoding.bits();
    let group = encoding.group();
    let mut symbols: Vec<u8> = Vec::new();
    symbols.try_reserve(data.len()).map_err(|_| ())?;
    let mut saw_padding = false;
    for byte in data {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if *byte == b'=' {
            saw_padding = true;
            symbols.push(u8::MAX);
            continue;
        }
        if saw_padding {
            return Err(());
        }
        match symbol_value(*byte, encoding) {
            Some(value) => symbols.push(value),
            None if ignore_garbage => {}
            None => return Err(()),
        }
    }

    let data_len = symbols.iter().position(|value| *value == u8::MAX).unwrap_or(symbols.len());
    let padding = symbols.len() - data_len;
    let remainder = data_len % group;
    let valid = match encoding {
        Encoding::Base64 | Encoding::Base64Url => remainder != 1,
        Encoding::Base32 | Encoding::Base32Hex => {
            matches!(remainder, 0 | 2 | 4 | 5 | 7)
        }
        Encoding::Base16 => false,
    };
    if !valid {
        return Err(());
    }
    if padding > 0 {
        let expected = (group - remainder) % group;
        if padding != expected {
            return Err(());
        }
    }

    let mut out: Vec<u8> = Vec::new();
    let estimate = data_len * bits as usize / 8 + 1;
    out.try_reserve(estimate).map_err(|_| ())?;
    let mut accumulator = 0u32;
    let mut count = 0u32;
    for value in &symbols[..data_len] {
        accumulator = (accumulator << bits) | *value as u32;
        count += bits;
        while count >= 8 {
            count -= 8;
            out.push(((accumulator >> count) & 0xff) as u8);
        }
        accumulator &= if count == 0 { 0 } else { (1u32 << count) - 1 };
    }
    if count > 0 && accumulator != 0 {
        return Err(());
    }
    Ok(out)
}

fn symbol_value(byte: u8, encoding: Encoding) -> Option<u8> {
    match encoding {
        Encoding::Base64 => match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        },
        Encoding::Base64Url => match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        },
        Encoding::Base32 => match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a'),
            b'2'..=b'7' => Some(byte - b'2' + 26),
            _ => None,
        },
        Encoding::Base32Hex => match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'A'..=b'V' => Some(byte - b'A' + 10),
            b'a'..=b'v' => Some(byte - b'a' + 10),
            _ => None,
        },
        Encoding::Base16 => match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        },
    }
}

fn encode_base16(data: &[u8]) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(data.len().saturating_mul(2)).ok()?;
    for byte in data {
        out.push(BASE16[(*byte >> 4) as usize]);
        out.push(BASE16[(*byte & 0x0f) as usize]);
    }
    Some(out)
}

fn decode_base16(data: &[u8], ignore_garbage: bool) -> Result<Vec<u8>, ()> {
    let mut digits: Vec<u8> = Vec::new();
    digits.try_reserve(data.len()).map_err(|_| ())?;
    for byte in data {
        if byte.is_ascii_whitespace() {
            continue;
        }
        match symbol_value(*byte, Encoding::Base16) {
            Some(value) => digits.push(value),
            None if ignore_garbage => {}
            None => return Err(()),
        }
    }
    if digits.len() % 2 != 0 {
        return Err(());
    }
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(digits.len() / 2).map_err(|_| ())?;
    for pair in digits.chunks_exact(2) {
        out.push((pair[0] << 4) | pair[1]);
    }
    Ok(out)
}