//! `more` — print a file in screen-sized chunks.
//!
//! Usage: `more [-d] [-n LINES] [FILE...]` (`-` means stdin; no operand also
//! means stdin). Every `LINES` lines (default 24) a `--More--` separator is
//! printed, with `-d` adding the conventional "Press space to continue, 'q' to
//! quit" help text. The last chunk is never followed by a separator.
//!
//! # No interactive paging
//!
//! The ABI has no `isatty` and the shell has no key handling inside a child,
//! so this port does not wait for a keypress: the separator is an honest
//! marker and all chunks are printed immediately. That keeps the output
//! testable in QEMU and usable when the serial console is the only output.
//! Files are read whole; multiple operands are concatenated in order without
//! GNU's `::::::::::::::` banners. A file that cannot be read is reported and
//! the remaining operands still run (exit 1). Bad options are usage error 2.
//!
//! The console fd 0 still has no EOF; an stdin `more` therefore waits forever,
//! as every filter in this bundle does.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: more [-d] [-n LINES] [FILE...]";

pub fn run(args: &[String]) -> i32 {
    let mut lines_per_chunk = 24usize;
    let mut help = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0usize;
    let mut parse_options = true;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && (arg == "-n" || arg == "--lines") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match util::parse_usize(value) {
                Some(parsed) if parsed > 0 => lines_per_chunk = parsed,
                _ => {
                    eprintln!("more: invalid line count: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && (arg == "-d" || arg == "--help-prompt") {
            help = true;
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("--lines=") {
                match util::parse_usize(value) {
                    Some(parsed) if parsed > 0 => {
                        lines_per_chunk = parsed;
                        continue;
                    }
                    _ => {
                        eprintln!("more: invalid line count: '{value}'");
                        return util::EXIT_USAGE;
                    }
                }
            }
            if let Some(value) = arg.strip_prefix("-n") {
                if !value.is_empty() {
                    match util::parse_usize(value) {
                        Some(parsed) if parsed > 0 => {
                            lines_per_chunk = parsed;
                            continue;
                        }
                        _ => {
                            eprintln!("more: invalid line count: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("more: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        operands.push("-");
    }

    let prompt: &[u8] = if help {
        b"--More-- (Press space to continue, 'q' to quit.)"
    } else {
        b"--More--"
    };

    let mut status = util::EXIT_OK;
    for name in operands {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("more", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        let mut lines = util::lines(&data).peekable();
        let mut count = 0usize;
        while let Some(line) = lines.next() {
            if let Err(err) = util::write_line(1, line) {
                eprintln!("more: write error: {err}");
                return util::EXIT_FAILURE;
            }
            count += 1;
            if count % lines_per_chunk == 0 && lines.peek().is_some() {
                if let Err(err) = util::write_line(1, prompt) {
                    eprintln!("more: write error: {err}");
                    return util::EXIT_FAILURE;
                }
            }
        }
    }
    status
}