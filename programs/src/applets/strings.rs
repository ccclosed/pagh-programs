//! `strings` — print runs of printable ASCII bytes.
//!
//! A string is a run of at least N bytes in `0x20..=0x7e` (space through `~`);
//! `-n N` changes N (default 4). Tab, CR and all non-ASCII bytes terminate a
//! run. Each found run is printed on its own line. Reading is streamed, so a
//! large binary does not have to fit in memory; a run that crosses a read
//! boundary is kept whole.
//!
//! Several files are scanned in order; no operand means stdin. Runs do not span
//! file boundaries. `-a`/`-t`/`-e` and encodings are not implemented.
//!
//! Exit: 0 success, 1 open/read/write/memory error, 2 usage error.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: strings [-n N] [FILE...]";

enum ScanError {
    Read(Errno),
    Write(Errno),
    Memory,
}

pub fn run(args: &[String]) -> i32 {
    let mut minimum = 4usize;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        if parse_options && arg == "--" {
            parse_options = false;
            index += 1;
            continue;
        }
        if parse_options && arg == "-n" {
            index += 1;
            let Some(value) = args.get(index) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            match parse_minimum(value) {
                Some(parsed) => minimum = parsed,
                None => {
                    eprintln!("strings: invalid minimum length: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-n") {
            match parse_minimum(&arg[2..]) {
                Some(parsed) => minimum = parsed,
                None => {
                    eprintln!("strings: invalid minimum length: '{}'", &arg[2..]);
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        operands.push(arg.as_str());
        index += 1;
    }

    if operands.is_empty() {
        operands.push("-");
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        match scan(name, minimum) {
            Ok(()) => {}
            Err(ScanError::Read(err)) => {
                util::report("strings", util::display_name(name), err);
                status = util::EXIT_FAILURE;
            }
            Err(ScanError::Write(err)) => {
                eprintln!("strings: write error: {err}");
                return util::EXIT_FAILURE;
            }
            Err(ScanError::Memory) => {
                eprintln!("strings: out of memory");
                return util::EXIT_FAILURE;
            }
        }
    }
    status
}

fn scan(name: &str, minimum: usize) -> Result<(), ScanError> {
    let input = util::Input::open(name).map_err(ScanError::Read)?;
    let mut run: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 8192];

    loop {
        let read = util::read_retry(input.fd(), &mut buffer).map_err(ScanError::Read)?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            if (0x20..=0x7e).contains(byte) {
                run.try_reserve(1).map_err(|_| ScanError::Memory)?;
                run.push(*byte);
            } else {
                flush(&mut run, minimum)?;
            }
        }
    }
    flush(&mut run, minimum)
}

fn flush(run: &mut Vec<u8>, minimum: usize) -> Result<(), ScanError> {
    if run.len() >= minimum {
        util::write_line(1, run).map_err(ScanError::Write)?;
    }
    run.clear();
    Ok(())
}

fn parse_minimum(text: &str) -> Option<usize> {
    match util::parse_usize(text) {
        Some(0) | None => None,
        Some(value) => Some(value),
    }
}