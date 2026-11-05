//! `truncate -s SIZE FILE...` — resize regular files.
//!
//! `fs::truncate` opens the file for writing and calls `ftruncate`, so a
//! missing file is `ENOENT` — this applet never creates one. `-c` is accepted
//! and makes a missing operand a non-error (the same meaning it has in GNU
//! `truncate`: "do not create", and since creation never happens anyway, it
//! only suppresses `ENOENT`). Growing a file fills the new range with zeroes in
//! the kernel, not with stale bytes.
//!
//! `SIZE` is an unsigned decimal byte count (no `K`/`M` suffixes and no
//! `+`/`-`/`%` prefixes). A missing `-s` or a bad size is a usage error (2);
//! per-file failures are reported and processing continues, with exit status
//! 1. There are no pipes or redirection in the shell.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: truncate [-c] -s SIZE FILE...";

pub fn run(args: &[String]) -> i32 {
    let mut size: Option<u64> = None;
    let mut no_create = false;
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
        if parse_options && arg == "-c" {
            no_create = true;
            continue;
        }
        if parse_options && (arg == "-s" || arg.starts_with("-s")) {
            let value = if arg == "-s" {
                let Some(value) = args.get(index).map(String::as_str) else {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                };
                index += 1;
                value
            } else {
                &arg[2..]
            };
            match value.parse::<u64>() {
                Ok(parsed) => size = Some(parsed),
                Err(_) => {
                    eprintln!("truncate: invalid size: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        if util::push(&mut operands, arg).is_err() {
            eprintln!("truncate: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let Some(size) = size else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };
    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        match fs::truncate(name, size) {
            Ok(()) => {}
            Err(Errno::ENOENT) if no_create => {}
            Err(err) => {
                util::report("truncate", name, err);
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}