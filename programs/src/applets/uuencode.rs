//! `uuencode` — classic Unix-to-Unix encoding.
//!
//! Usage: `uuencode [FILE] NAME`. With one operand, `NAME` names the decoded
//! output and the data comes from stdin; with two, `FILE` is the source and
//! `NAME` is the remote name. Output is the historical format:
//!
//! ```text
//! begin 644 NAME
//! M<encoded 45 bytes>
//! ...
//! end
//! ```
//!
//! The mode is always `644` (the ABI does not persist modes), groups are 45
//! bytes, and six-bit zero is written as a backtick, the classic form that
//! survives trailing-space stripping. `-m` (base64 mode) is not implemented
//! and is a usage error. Several source files are not accepted.
//!
//! Exit: 0 success, 1 read/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use super::uu;
use crate::util;

const USAGE: &str = "usage: uuencode [FILE] NAME";

pub fn run(args: &[String]) -> i32 {
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;
    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            // -m is a real uuencode option but selects base64, which this
            // classic-only port does not implement.
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("uuencode: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let (source, name) = match operands.as_slice() {
        [name] => ("-", *name),
        [source, name] => (*source, *name),
        _ => {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
    };
    if name.is_empty() || name.contains('\n') || name.contains('\r') {
        eprintln!("uuencode: invalid output name: '{name}'");
        return util::EXIT_USAGE;
    }

    let data = match util::read_input(source) {
        Ok(data) => data,
        Err(err) => {
            util::report("uuencode", util::display_name(source), err);
            return util::EXIT_FAILURE;
        }
    };

    let mut header = String::from("begin 644 ");
    header.push_str(name);
    header.push('\n');
    if let Err(err) = util::write_all(1, header.as_bytes()) {
        eprintln!("uuencode: write error: {err}");
        return util::EXIT_FAILURE;
    }

    let mut encoded = String::with_capacity(64);
    for chunk in data.chunks(45) {
        encoded.clear();
        uu::encode_line(chunk, &mut encoded);
        if let Err(err) = util::write_all(1, encoded.as_bytes()) {
            eprintln!("uuencode: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    if let Err(err) = util::write_all(1, b"end\n") {
        eprintln!("uuencode: write error: {err}");
        return util::EXIT_FAILURE;
    }
    util::EXIT_OK
}
