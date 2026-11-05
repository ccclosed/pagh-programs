//! `cksum` — POSIX CRC-32 checksum plus byte count.
//!
//! The CRC is the POSIX one: polynomial `0x04C11DB7`, MSB-first, initial
//! register 0, the file length appended as little-endian bytes, and a final
//! one's complement. That is the algorithm `/usr/bin/cksum` implements, not
//! zlib's reflected CRC-32, so the values match POSIX hosts.
//!
//! Output is `crc size [name]`. An implicit stdin stream has no name; explicit
//! operands, including `-`, are shown as typed. There are no options (POSIX
//! `cksum` has none). Exit: 0 success, 1 read/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: cksum [FILE...]";

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
            eprintln!("cksum: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let implicit_stdin = operands.is_empty();
    if implicit_stdin {
        operands.push("-");
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("cksum", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        let checksum = posix_crc32(&data);
        let mut line = String::new();
        line.push_str(&alloc::format!("{checksum} {}", data.len()));
        if !implicit_stdin {
            line.push(' ');
            line.push_str(name);
        }
        if let Err(err) = util::write_line(1, line.as_bytes()) {
            eprintln!("cksum: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    status
}

fn posix_crc32(data: &[u8]) -> u32 {
    let mut crc = 0u32;
    for byte in data {
        crc ^= (*byte as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
        }
    }

    // POSIX appends the length, least-significant byte first, then complements.
    let mut length = data.len() as u64;
    while length != 0 {
        crc ^= ((length & 0xff) as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
        }
        length >>= 8;
    }
    !crc
}