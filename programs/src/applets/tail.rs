//! `tail` — print the last N lines of each input (default 10).
//!
//! Options: `-n N`, `-nN`, and `-N`; `--` ends options; `-` is stdin.
//! Multiple operands get `==> name <==` banners like `head`.
//!
//! This implementation reads the whole input into memory and then finds the
//! start of the last N lines. That is deliberate: the ABI has `lseek` but no
//! `statat`/mmap, and reading backwards through a file in 4 KiB pages would be
//! more code than the port needs. The memory cost is documented in the README
//! and is acceptable for the current 1 MiB image limit; a future `tail -c`/large
//! file variant can seek instead.
//!
//! Exit: 0 success, 1 open/read/write error (remaining files are processed),
//! 2 usage error.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::{eprintln, Errno};

use crate::util;

const USAGE: &str = "usage: tail [-n N | -N] [FILE...]";

pub fn run(args: &[String]) -> i32 {
    let mut count = 10usize;
    let mut files: Vec<&str> = Vec::new();
    let mut options = true;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        if options && arg == "--" {
            options = false;
            index += 1;
            continue;
        }
        if options && arg == "-n" {
            index += 1;
            let Some(value) = args.get(index) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            match util::parse_usize(value) {
                Some(value) => count = value,
                None => {
                    eprintln!("tail: invalid number of lines: '{value}'");
                    return util::EXIT_FAILURE;
                }
            }
            index += 1;
            continue;
        }
        if options && arg.len() > 2 && arg.starts_with("-n") {
            match util::parse_usize(&arg[2..]) {
                Some(value) => count = value,
                None => {
                    eprintln!("tail: invalid number of lines: '{}'", &arg[2..]);
                    return util::EXIT_FAILURE;
                }
            }
            index += 1;
            continue;
        }
        if options && arg.len() > 1 && arg.starts_with('-') && arg[1..].bytes().all(|b| b.is_ascii_digit()) {
            match util::parse_usize(&arg[1..]) {
                Some(value) => count = value,
                None => {
                    eprintln!("tail: invalid number of lines: '{}'", &arg[1..]);
                    return util::EXIT_FAILURE;
                }
            }
            index += 1;
            continue;
        }
        if options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        files.push(arg.as_str());
        index += 1;
    }

    if files.is_empty() {
        files.push("-");
    }
    let banners = files.len() > 1;
    let mut status = util::EXIT_OK;

    for (position, name) in files.iter().enumerate() {
        if banners {
            if let Err(err) = write_banner(name, position > 0) {
                eprintln!("tail: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
        let input = match util::Input::open(name) {
            Ok(input) => input,
            Err(err) => {
                util::report("tail", name, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        match util::read_all(input.fd()) {
            Ok(data) => {
                let start = last_lines_start(&data, count);
                if let Err(err) = util::write_all(1, &data[start..]) {
                    eprintln!("tail: write error: {err}");
                    return util::EXIT_FAILURE;
                }
            }
            Err(err) => {
                util::report("tail", name, err);
                status = util::EXIT_FAILURE;
            }
        }
    }

    status
}

fn write_banner(name: &str, blank_line_before: bool) -> Result<(), Errno> {
    if blank_line_before {
        util::write_all(1, b"\n")?;
    }
    let banner = format!("==> {} <==\n", util::display_name(name));
    util::write_all(1, banner.as_bytes())
}

/// Byte offset where the last `count` lines of `data` begin.
///
/// A trailing newline does not create an extra empty line: `b"a\nb\n"` with
/// `count == 1` starts at `b`, yielding `b"b\n"`. `count == 0` starts at the
/// end (empty output), and a file with fewer than `count` lines starts at 0.
fn last_lines_start(data: &[u8], count: usize) -> usize {
    if count == 0 {
        return data.len();
    }

    let mut end = data.len();
    if end > 0 && data[end - 1] == b'\n' {
        end -= 1;
    }

    let mut newlines = 0usize;
    let mut index = end;
    while index > 0 {
        if data[index - 1] == b'\n' {
            newlines += 1;
            if newlines == count {
                return index;
            }
        }
        index -= 1;
    }
    0
}