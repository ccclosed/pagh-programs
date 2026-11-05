//! `mkdir` — create directories, with `-p` for missing parents.
//!
//! `-m MODE` is accepted and **ignored**: the kernel's `mkdirat` takes a mode
//! but the VFS has no `set_mode`, so ext2's default (`0o755`) stands. The
//! argument is still validated as an octal mode, so `mkdir -m nonsense` is a
//! usage error (2) instead of silently ignoring a typo. No note is printed:
//! the limitation is documented here and in `docs/abi.md`, and a successful
//! `mkdir` should stay quiet.
//!
//! Multiple operands are created in order; a failure is reported and the rest
//! are still attempted, with exit status 1. Without `-p`, an existing
//! directory is `EEXIST`; with `-p`, an existing directory is accepted, but an
//! existing non-directory in the path is still an error.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;

use crate::util;

const USAGE: &str = "usage: mkdir [-p] [-m MODE] DIR...";

pub fn run(args: &[String]) -> i32 {
    let mut parents = false;
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
        if parse_options && arg == "-p" {
            parents = true;
            continue;
        }
        if parse_options && arg == "-m" {
            let Some(mode) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            if parse_octal(mode).is_none() {
                eprintln!("mkdir: invalid mode: '{mode}'");
                return util::EXIT_USAGE;
            }
            // Deliberately ignored: see the module docs.
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        if util::push(&mut operands, arg).is_err() {
            eprintln!("mkdir: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        let result = if parents {
            util::create_dir_all(name)
        } else {
            fs::create_dir(name)
        };
        if let Err(err) = result {
            util::report("mkdir", name, err);
            status = util::EXIT_FAILURE;
        }
    }
    status
}

/// Parse an octal mode (with an optional `0o` prefix). The value is not used;
/// this exists so an invalid `-m` argument is a usage error.
fn parse_octal(text: &str) -> Option<u32> {
    let digits = text.strip_prefix("0o").unwrap_or(text);
    if digits.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for byte in digits.bytes() {
        if !(b'0'..=b'7').contains(&byte) {
            return None;
        }
        value = value.checked_mul(8)?.checked_add((byte - b'0') as u32)?;
    }
    Some(value)
}