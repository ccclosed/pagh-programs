//! `chmod MODE FILE...` — set permission bits for the current run only.
//!
//! The kernel's `fchmodat` records `mode & 0o7777` in a per-run table keyed by
//! inode; `fstat` reports the override until the program exits, but **nothing is
//! written to the ext2 inode**, no access check consults it, and the next
//! program sees the on-disk mode again. This applet therefore prints one note
//! on fd 2:
//!
//! ```text
//! chmod: note: mode not persisted (run-local override)
//! ```
//!
//! and never claims otherwise. The note is per invocation, not per operand.
//! The root `/` is a synthetic node and answers `ENOTSUP`.
//!
//! `MODE` is octal, with an optional `0o` prefix (`644`, `0644`, `0o755`,
//! `4755`); symbolic modes (`u+x`) are not implemented. A missing mode or a
//! malformed one is a usage error (2). Per-file failures are reported and the
//! rest are attempted; exit status is 1 if any failed.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;

use crate::util;

const USAGE: &str = "usage: chmod MODE FILE...";

pub fn run(args: &[String]) -> i32 {
    let mut values: Vec<&str> = Vec::new();
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
        if util::push(&mut values, arg.as_str()).is_err() {
            eprintln!("chmod: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if values.len() < 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mode_text = values[0];
    let Some(mode) = parse_octal_mode(mode_text) else {
        eprintln!("chmod: invalid mode: '{mode_text}'");
        return util::EXIT_USAGE;
    };

    // The limitation is a property of the ABI, not of this invocation, so say
    // it once and make it impossible for a script to mistake the change for a
    // persisted one.
    eprintln!("chmod: note: mode not persisted (run-local override)");

    let mut status = util::EXIT_OK;
    for name in &values[1..] {
        if let Err(err) = fs::set_permissions(name, mode) {
            util::report("chmod", name, err);
            status = util::EXIT_FAILURE;
        }
    }
    status
}

fn parse_octal_mode(text: &str) -> Option<u32> {
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
    if value > 0o7777 {
        return None;
    }
    Some(value)
}