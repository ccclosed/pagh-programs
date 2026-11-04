//! `head` — print the first N lines of each input (default 10).
//!
//! Options: `-n N`, `-nN`, and the historical `-N`; `--` ends options. With
//! more than one operand a `==> name <==` banner is printed before each file
//! (GNU-compatible shape); `-` means stdin and is shown as `standard input`.
//!
//! The reader streams: it stops as soon as the N-th newline has been written,
//! so `head -n 1` on a huge file does not read the file. `EAGAIN` on stdin is
//! retried with `sched_yield` by `util::read_retry`.
//!
//! Exit: 0 success, 1 open/read/write error (remaining files are processed),
//! 2 usage error.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::{eprintln, Errno};

use crate::util;

const USAGE: &str = "usage: head [-n N | -N] [FILE...]";

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
                    eprintln!("head: invalid number of lines: '{value}'");
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
                    eprintln!("head: invalid number of lines: '{}'", &arg[2..]);
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
                    eprintln!("head: invalid number of lines: '{}'", &arg[1..]);
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
                eprintln!("head: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
        let input = match util::Input::open(name) {
            Ok(input) => input,
            Err(err) => {
                util::report("head", name, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        if let Err(err) = copy_head(input.fd(), count) {
            util::report("head", name, err);
            status = util::EXIT_FAILURE;
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

/// Copy at most `count` newline-terminated lines from `fd` to fd 1.
fn copy_head(fd: i32, count: usize) -> Result<(), Errno> {
    if count == 0 {
        return Ok(());
    }

    let mut remaining = count;
    let mut buf = [0u8; 8192];

    while remaining > 0 {
        let read = util::read_retry(fd, &mut buf)?;
        if read == 0 {
            break;
        }

        let mut take = read;
        let mut newlines = 0usize;
        for (offset, byte) in buf[..read].iter().enumerate() {
            if *byte == b'\n' {
                newlines += 1;
                if newlines == remaining {
                    take = offset + 1;
                    break;
                }
            }
        }

        util::write_all(1, &buf[..take])?;

        if newlines >= remaining {
            remaining = 0;
        } else {
            remaining -= newlines;
        }
    }

    Ok(())
}