//! `sum` — BSD 16-bit rotating checksum (default) or SysV checksum (`-s`).
//!
//! BSD: for every byte, rotate the 16-bit accumulator right by one bit and add
//! the byte modulo 2^16. The block count is `ceil(size / 1024)`.
//!
//! SysV: add every byte, then fold the carries back into 16 bits; the block
//! count is `ceil(size / 512)`.
//!
//! Output is the traditional `checksum blocks [name]`; the checksum is
//! zero-padded to five digits in BSD mode and the block column is five wide.
//! File names are printed for explicit operands (including `-`); a bare stdin
//! stream has no name. Exit: 0 success, 1 read/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: sum [-r] [-s] [FILE...]";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Algorithm {
    Bsd,
    SysV,
}

pub fn run(args: &[String]) -> i32 {
    let mut algorithm = Algorithm::Bsd;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            for option in arg[1..].chars() {
                match option {
                    'r' => algorithm = Algorithm::Bsd,
                    's' => algorithm = Algorithm::SysV,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        parse_options = false;
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("sum: out of memory");
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
                util::report("sum", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        let size = data.len() as u64;
        let (checksum, blocks) = match algorithm {
            Algorithm::Bsd => (
                bsd_checksum(&data) as u64,
                size / 1024 + u64::from(size % 1024 != 0),
            ),
            Algorithm::SysV => (
                sysv_checksum(&data),
                size / 512 + u64::from(size % 512 != 0),
            ),
        };
        let mut line = String::new();
        if algorithm == Algorithm::Bsd {
            line.push_str(&alloc::format!("{checksum:05} {blocks:5}"));
        } else {
            line.push_str(&alloc::format!("{checksum} {blocks}"));
        }
        if !implicit_stdin {
            line.push(' ');
            line.push_str(name);
        }
        if let Err(err) = util::write_line(1, line.as_bytes()) {
            eprintln!("sum: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    status
}

fn bsd_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u16;
    for byte in data {
        sum = sum.rotate_right(1);
        sum = sum.wrapping_add(*byte as u16);
    }
    sum
}

fn sysv_checksum(data: &[u8]) -> u64 {
    let mut sum: u64 = 0;
    for byte in data {
        sum = sum.saturating_add(*byte as u64);
    }
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    sum
}