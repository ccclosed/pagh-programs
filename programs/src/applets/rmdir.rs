//! `rmdir` — remove empty directories, optionally with their parents.
//!
//! `-p` removes the operand and then each of its parents in turn, stopping at
//! the first failure or at `/`. The kernel's `unlinkat(AT_REMOVEDIR)` answers
//! `ENOTEMPTY` for a directory with entries, `ENOENT` for a missing one and
//! `ENOTDIR` for a file; each is reported with [`util::report`] and the
//! remaining operands are still attempted (exit 1).
//!
//! `.`, `..` and `/` are refused with `EINVAL`: removing the root or a dot
//! component is never what the user meant, and the VFS would either reject it
//! anyway or climb out of the intended tree. There is no `--ignore-fail-on-
//! non-empty` and no `-v`.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: rmdir [-p] DIR...";

pub fn run(args: &[String]) -> i32 {
    let mut parents = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg == "-p" {
            parents = true;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("rmdir: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut status = util::EXIT_OK;
    for operand in operands {
        if !remove_operand(operand, parents) {
            status = util::EXIT_FAILURE;
        }
    }
    status
}

fn remove_operand(operand: &str, parents: bool) -> bool {
    if protected(operand) {
        util::report("rmdir", operand, Errno::EINVAL);
        return false;
    }
    if !remove_one(operand) {
        return false;
    }
    if !parents {
        return true;
    }

    let mut current = String::from(operand.trim_end_matches('/'));
    loop {
        let parent = parent_of(&current);
        match parent {
            Some(path) if !path.is_empty() && path != "/" && path != "." && path != ".." => {
                if !remove_one(&path) {
                    return false;
                }
                current = path;
            }
            _ => return true,
        }
    }
}

fn remove_one(name: &str) -> bool {
    match fs::remove_dir(name) {
        Ok(()) => true,
        Err(err) => {
            util::report("rmdir", name, err);
            false
        }
    }
}

/// The lexical parent of a path, or `None` when there is nothing safe to
/// remove (`/`, `.`, `..`, or an empty name).
fn parent_of(path: &str) -> Option<String> {
    let parent = libpagh::path::dirname(path);
    if parent == "." || parent == ".." || parent == "/" {
        return None;
    }
    // `a/../b` has the lexical parent `a/..`, which resolves to the current
    // directory; never feed that back to the filesystem as a removal target.
    let resolved = libpagh::path::resolve(parent);
    if resolved == "/" || resolved == "." || resolved == ".." {
        return None;
    }
    Some(String::from(parent))
}

fn protected(name: &str) -> bool {
    let trimmed = name.trim_end_matches('/');
    trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.ends_with("/.")
        || trimmed.ends_with("/..")
        || libpagh::path::resolve(name) == "/"
}