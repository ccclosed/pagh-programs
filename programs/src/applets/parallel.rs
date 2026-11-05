//! `parallel` — the sequential sibling of `xargs -n 1`.
//!
//! Usage: `parallel [-j N] [-a FILE] [-r] COMMAND [ARG...]`. The command runs
//! once per whitespace-separated input token, strictly **sequentially**: the
//! catalog calls for a sequential version, and `-j N` is accepted only so a
//! script written for a parallel tool parses. There are no worker threads and
//! no scheduler priorities in the ABI; `-j` is documented as a no-op.
//!
//! Like `xargs`, `-a FILE` is a port extension because console stdin has no
//! EOF. Output is whatever the child writes; each invocation is waited for
//! before the next starts. Exit: `0` all succeeded, `123` a command failed,
//! `124` it could not be run, `2` usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::applets::xargs;
use crate::util;

const USAGE: &str = "usage: parallel [-j N] [-a FILE] [-r] COMMAND [ARG...]";

pub fn run(args: &[String]) -> i32 {
    let mut cleaned: Vec<String> = Vec::new();
    let mut index = 0usize;
    let parse_options = true;

    while index < args.len() {
        let arg = args[index].as_str();
        if parse_options && arg == "--" {
            // Keep the sentinel so xargs' parser also stops treating the
            // command's own `-x` arguments as options.
            if util::push(&mut cleaned, String::from("--")).is_err() {
                eprintln!("parallel: out of memory");
                return util::EXIT_FAILURE;
            }
            index += 1;
            // Everything after `--` belongs to the command.
            while index < args.len() {
                if util::push(&mut cleaned, String::from(args[index].as_str())).is_err() {
                    eprintln!("parallel: out of memory");
                    return util::EXIT_FAILURE;
                }
                index += 1;
            }
            break;
        }
        if parse_options && (arg == "-j" || arg == "--jobs") {
            let Some(value) = args.get(index + 1).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            if parse_jobs(value).is_none() {
                eprintln!("parallel: invalid job count: '{value}'");
                return util::EXIT_USAGE;
            }
            // Accepted and deliberately ignored: sequential execution.
            index += 2;
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("--jobs=") {
                if parse_jobs(value).is_none() {
                    eprintln!("parallel: invalid job count: '{value}'");
                    return util::EXIT_USAGE;
                }
                index += 1;
                continue;
            }
            if let Some(value) = arg.strip_prefix("-j") {
                if !value.is_empty() {
                    if parse_jobs(value).is_none() {
                        eprintln!("parallel: invalid job count: '{value}'");
                        return util::EXIT_USAGE;
                    }
                    index += 1;
                    continue;
                }
            }
        }
        if parse_options && (arg == "-r" || arg == "--no-run-if-empty") {
            if util::push(&mut cleaned, String::from(arg)).is_err() {
                eprintln!("parallel: out of memory");
                return util::EXIT_FAILURE;
            }
            index += 1;
            continue;
        }
        if parse_options && (arg == "-a" || arg == "--arg-file") {
            let Some(value) = args.get(index + 1).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            if util::push(&mut cleaned, String::from(arg)).is_err()
                || util::push(&mut cleaned, String::from(value)).is_err()
            {
                eprintln!("parallel: out of memory");
                return util::EXIT_FAILURE;
            }
            index += 2;
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("--arg-file=") {
                if util::push(&mut cleaned, String::from(arg)).is_err() {
                    eprintln!("parallel: out of memory");
                    return util::EXIT_FAILURE;
                }
                let _ = value;
                index += 1;
                continue;
            }
            if let Some(value) = arg.strip_prefix("-a") {
                if !value.is_empty() {
                    if util::push(&mut cleaned, String::from(arg)).is_err() {
                        eprintln!("parallel: out of memory");
                        return util::EXIT_FAILURE;
                    }
                    index += 1;
                    continue;
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        // First non-option: copy it and the rest verbatim.
        while index < args.len() {
            if util::push(&mut cleaned, String::from(args[index].as_str())).is_err() {
                eprintln!("parallel: out of memory");
                return util::EXIT_FAILURE;
            }
            index += 1;
        }
        break;
    }

    xargs::run_as("parallel", &cleaned, Some(1))
}

fn parse_jobs(text: &str) -> Option<usize> {
    let value = util::parse_usize(text)?;
    if value == 0 {
        None
    } else {
        Some(value)
    }
}