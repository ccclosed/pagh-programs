//! `base64` — RFC 4648 base64 encoding and decoding.
//!
//! Encoding writes the standard alphabet with `=` padding, wrapped at 76
//! characters by default; `-w N` selects another width and `-w 0` disables
//! wrapping. Decoding ignores ASCII whitespace, accepts the same padded form,
//! and rejects a stray character, a misplaced `=`, a non-canonical final byte
//! or a truncated group. `-d` selects decoding; `-w` is accepted there and has
//! no effect.
//!
//! One file operand or stdin (`-`). Exit: 0 success, 1 invalid input or
//! read/write error, 2 usage. There is no `-i` (GNU's ignore-garbage mode).

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use super::codec::{self, CodecError};
use crate::util;

const USAGE: &str = "usage: base64 [-d] [-w N] [FILE]";

pub fn run(args: &[String]) -> i32 {
    let mut decode = false;
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
        if parse_options && arg == "-d" {
            decode = true;
            continue;
        }
        if parse_options && arg == "-w" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match util::parse_usize(value) {
                Some(parsed) => wrap = parsed,
                None => {
                    eprintln!("base64: invalid wrap width: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-w") {
            match util::parse_usize(&arg[2..]) {
                Some(parsed) => wrap = parsed,
                None => {
                    eprintln!("base64: invalid wrap width: '{}'", &arg[2..]);
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("base64: out of memory");
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
            util::report("base64", util::display_name(name), err);
            return util::EXIT_FAILURE;
        }
    };

    if decode {
        match codec::decode(&data, codec::BASE64, 6, 4) {
            Ok(decoded) => {
                if let Err(err) = util::write_all(1, &decoded) {
                    eprintln!("base64: write error: {err}");
                    return util::EXIT_FAILURE;
                }
                util::EXIT_OK
            }
            Err(CodecError::Invalid) => {
                eprintln!("base64: invalid input");
                util::EXIT_FAILURE
            }
            Err(CodecError::Memory) => {
                eprintln!("base64: out of memory");
                util::EXIT_FAILURE
            }
        }
    } else {
        let encoded = match codec::encode(&data, codec::BASE64, 6, 4) {
            Some(encoded) => encoded,
            None => {
                eprintln!("base64: out of memory");
                return util::EXIT_FAILURE;
            }
        };
        match write_wrapped(&encoded, wrap) {
            Ok(()) => util::EXIT_OK,
            Err(err) => {
                eprintln!("base64: write error: {err}");
                util::EXIT_FAILURE
            }
        }
    }
}

fn write_wrapped(data: &[u8], wrap: usize) -> libpagh::Result<()> {
    if data.is_empty() {
        return Ok(());
    }
    let width = if wrap == 0 { data.len() } else { wrap };
    for chunk in data.chunks(width) {
        util::write_all(1, chunk)?;
        util::write_all(1, b"\n")?;
    }
    Ok(())
}