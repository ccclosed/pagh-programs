//! `zcat` — decompress gzip members to stdout.
//!
//! This is the `gzip -dc` operation with its own applet name: every operand is
//! read whole, decoded by [`super::gzip`] and written to file descriptor 1.
//! There is no suffix requirement (unlike `gunzip` without `-c`), no output
//! file, and no option other than `--`; `-` is stdin. Concatenated members and
//! zero padding are handled by the shared decoder.
//!
//! The decoder accepts the stored-block (DEFLATE BTYPE=00) subset that
//! `gzip` writes; a stream with compressed blocks, bad magic, a bad CRC/ISIZE
//! or truncation is reported as an error and the exit status is 1. Other
//! operands are still attempted. Exit: 0 success, 1 data/I/O error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use super::gzip;
use crate::util;

const USAGE: &str = "usage: zcat [FILE...]";

pub fn run(args: &[String]) -> i32 {
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("zcat: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        operands.push("-");
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("zcat", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        match gzip::decode_stream(&data) {
            Ok(output) => {
                if let Err(err) = util::write_all(1, &output) {
                    eprintln!("zcat: write error: {err}");
                    return util::EXIT_FAILURE;
                }
            }
            Err(message) => {
                eprintln!("zcat: {}: {message}", util::display_name(name));
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}