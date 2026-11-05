//! `uudecode` — decode a classic uuencoded message.
//!
//! Usage: `uudecode [-o FILE] [FILE]`. The `begin MODE NAME` line supplies the
//! default output path, and `end` terminates the payload; `-o FILE` overrides
//! the path and `-o -` writes the decoded bytes to stdout. The mode is accepted
//! and ignored (the ABI has no persistent chmod), and base64 messages (`begin-base64`)
//! are not accepted.
//!
//! The whole message is decoded into memory before the output is created, so a
//! malformed line never leaves a truncated destination. One message per run;
//! there is no `-p`/`-v`/`-s` mode. A missing `begin`, a missing `end`, or an
//! invalid length/encoding character is a data error (exit 1).
//!
//! Exit: 0 success, 1 decode/read/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::File;

use super::uu::{self, DecodeError};
use crate::util;

const USAGE: &str = "usage: uudecode [-o FILE] [FILE]";

pub fn run(args: &[String]) -> i32 {
    let mut output: Option<&str> = None;
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
        if parse_options && arg == "-o" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            if value.is_empty() {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            }
            output = Some(value);
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("-o") {
                if !value.is_empty() {
                    output = Some(value);
                    continue;
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("uudecode: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.len() > 1 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let source = operands.first().copied().unwrap_or("-");
    let data = match util::read_input(source) {
        Ok(data) => data,
        Err(err) => {
            util::report("uudecode", util::display_name(source), err);
            return util::EXIT_FAILURE;
        }
    };

    let decoded = match uu::decode(&data) {
        Ok(decoded) => decoded,
        Err(DecodeError::NoBegin) => {
            eprintln!("uudecode: no begin line");
            return util::EXIT_FAILURE;
        }
        Err(DecodeError::NoEnd) => {
            eprintln!("uudecode: missing end line");
            return util::EXIT_FAILURE;
        }
        Err(DecodeError::Invalid) => {
            eprintln!("uudecode: invalid input");
            return util::EXIT_FAILURE;
        }
        Err(DecodeError::OutOfMemory) => {
            eprintln!("uudecode: out of memory");
            return util::EXIT_FAILURE;
        }
    };

    let target = output.unwrap_or(decoded.name.as_str());
    if target == "-" {
        if let Err(err) = util::write_all(1, &decoded.data) {
            eprintln!("uudecode: write error: {err}");
            return util::EXIT_FAILURE;
        }
        return util::EXIT_OK;
    }

    let file = match File::create(target) {
        Ok(file) => file,
        Err(err) => {
            util::report("uudecode", target, err);
            return util::EXIT_FAILURE;
        }
    };
    if let Err(err) = util::write_all(file.as_raw_fd(), &decoded.data) {
        util::report("uudecode", target, err);
        return util::EXIT_FAILURE;
    }
    util::EXIT_OK
}
