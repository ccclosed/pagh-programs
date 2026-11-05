//! `find PATH... -name GLOB -type f|d|l -maxdepth N -print` — walk a tree.
//!
//! The predicate subset is deliberately small: `-name` matches the **basename**
//! with `*`, `?`, `[...]`, `[!...]` (and `[^...]`) classes; `-type` accepts
//! `f` (regular file), `d` (directory) and `l` (symbolic link); `-maxdepth N`
//! counts the start path as depth 0. `-print` is accepted and is also the
//! implicit action when it is absent. There is no `-delete`, `-exec`, `-o`,
//! `-a`, `-newer`, `-perm` or `-size` — mutation belongs to the dedicated
//! applets, and the shell has no `exec`.
//!
//! Symlinks are never followed while walking: a child's kind comes from the
//! directory entry, and the start path is classified with `read_link` first.
//! That matters because the ABI has no `lstat` and `stat` follows the final
//! link. Output order is sorted per directory, so a transcript is reproducible
//! (the VFS itself returns ext2's directory order).
//!
//! A missing path, an unreadable directory or a write error is reported and
//! the remaining paths are still walked; the exit status is then 1. No paths
//! means `.`, like GNU find. Usage errors (bad option/`-type`/`-maxdepth`) exit
//! 2.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::{Dir, EntryKind};
use libpagh::path;

use crate::util;

const USAGE: &str = "usage: find [PATH...] [-name GLOB] [-type f|d|l] [-maxdepth N] [-print]";

struct Options {
    name: Option<String>,
    kind: Option<EntryKind>,
    max_depth: Option<usize>,
}

pub fn run(args: &[String]) -> i32 {
    let mut options = Options {
        name: None,
        kind: None,
        max_depth: None,
    };
    let mut paths: Vec<&str> = Vec::new();
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg == "-print" {
            // Printing is the implicit action; -print is accepted for scripts.
            continue;
        }
        if parse_options && arg == "-name" {
            let Some(pattern) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            let mut owned = String::new();
            if owned.try_reserve(pattern.len()).is_err() {
                eprintln!("find: out of memory");
                return util::EXIT_FAILURE;
            }
            owned.push_str(pattern);
            options.name = Some(owned);
            continue;
        }
        if parse_options && arg == "-type" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            options.kind = match value {
                "f" => Some(EntryKind::File),
                "d" => Some(EntryKind::Dir),
                "l" => Some(EntryKind::Symlink),
                _ => {
                    eprintln!("find: invalid type: '{value}'");
                    return util::EXIT_USAGE;
                }
            };
            continue;
        }
        if parse_options && arg == "-maxdepth" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match value.parse::<usize>() {
                Ok(depth) => options.max_depth = Some(depth),
                Err(_) => {
                    eprintln!("find: invalid maxdepth: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        if util::push(&mut paths, arg).is_err() {
            eprintln!("find: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if paths.is_empty() {
        paths.push(".");
    }

    let mut status = util::EXIT_OK;
    for start in paths {
        let kind = match util::path_kind(start) {
            Ok(kind) => kind,
            Err(err) => {
                util::report("find", start, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        if !walk(start, kind, 0, &options, &mut status) {
            // A write error is fatal for the run; a per-path error only marks
            // the status. Both have already been reported.
            if status == util::EXIT_OK {
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}

fn walk(path: &str, kind: EntryKind, depth: usize, options: &Options, status: &mut i32) -> bool {
    if matches(path, kind, options) {
        if let Err(err) = util::write_line(1, path.as_bytes()) {
            eprintln!("find: write error: {err}");
            *status = util::EXIT_FAILURE;
            return false;
        }
    }

    if !matches!(kind, EntryKind::Dir) {
        return true;
    }
    if let Some(max) = options.max_depth {
        if depth >= max {
            return true;
        }
    }

    let mut entries: Vec<(String, EntryKind)> = Vec::new();
    match Dir::open(path) {
        Ok(listing) => {
            for entry in listing {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        util::report("find", path, err);
                        *status = util::EXIT_FAILURE;
                        return true;
                    }
                };
                if entry.name() == "." || entry.name() == ".." {
                    continue;
                }
                if util::push(&mut entries, (String::from(entry.name()), entry.kind())).is_err() {
                    eprintln!("find: out of memory");
                    *status = util::EXIT_FAILURE;
                    return false;
                }
            }
        }
        Err(err) => {
            util::report("find", path, err);
            *status = util::EXIT_FAILURE;
            return true;
        }
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));

    for (name, child_kind) in entries {
        let child = util::join_path(path, &name);
        if !walk(&child, child_kind, depth + 1, options, status) {
            return false;
        }
    }
    true
}

fn matches(path: &str, kind: EntryKind, options: &Options) -> bool {
    if let Some(expected) = options.kind {
        if kind != expected {
            return false;
        }
    }
    if let Some(pattern) = &options.name {
        return glob_match(pattern, path::basename(path));
    }
    true
}

/// A small `*`, `?`, `[...]` matcher (no escaping, no brace expansion).
///
/// Iterative with one `*` backtrack point: that is linear in the common case
/// and cannot recurse once per character of a hostile name.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.as_bytes();
    let text = text.as_bytes();
    let mut pattern_index = 0usize;
    let mut text_index = 0usize;
    let mut star: Option<usize> = None;
    let mut star_text = 0usize;

    loop {
        let mut advanced = false;
        if pattern_index < pattern.len() {
            match pattern[pattern_index] {
                b'*' => {
                    star = Some(pattern_index);
                    star_text = text_index;
                    pattern_index += 1;
                    advanced = true;
                }
                b'?' => {
                    if text_index < text.len() {
                        pattern_index += 1;
                        text_index += 1;
                        advanced = true;
                    }
                }
                b'[' => match class_match(pattern, pattern_index, text.get(text_index).copied()) {
                    Some((true, next)) if text_index < text.len() => {
                        pattern_index = next;
                        text_index += 1;
                        advanced = true;
                    }
                    Some(_) => {}
                    None => {
                        // An unterminated class is a literal '['.
                        if text_index < text.len() && text[text_index] == b'[' {
                            pattern_index += 1;
                            text_index += 1;
                            advanced = true;
                        }
                    }
                },
                literal => {
                    if text_index < text.len() && literal == text[text_index] {
                        pattern_index += 1;
                        text_index += 1;
                        advanced = true;
                    }
                }
            }
        } else if text_index == text.len() {
            return true;
        }

        if advanced {
            continue;
        }
        match star {
            Some(position) if star_text < text.len() => {
                star_text += 1;
                pattern_index = position + 1;
                text_index = star_text;
            }
            _ => return false,
        }
    }
}

/// Parse one `[...]` class starting at `start`. Returns `(matched, next_index)`
/// or `None` when the class has no closing `]`.
fn class_match(pattern: &[u8], start: usize, byte: Option<u8>) -> Option<(bool, usize)> {
    let mut index = start + 1;
    let negated = matches!(pattern.get(index), Some(b'!') | Some(b'^'));
    if negated {
        index += 1;
    }
    let mut matched = false;
    let mut first = true;
    while index < pattern.len() {
        if pattern[index] == b']' && !first {
            return Some((matched != negated, index + 1));
        }
        first = false;
        if index + 2 < pattern.len()
            && pattern[index + 1] == b'-'
            && pattern[index + 2] != b']'
        {
            if let Some(value) = byte {
                if pattern[index] <= value && value <= pattern[index + 2] {
                    matched = true;
                }
            }
            index += 3;
        } else {
            if Some(pattern[index]) == byte {
                matched = true;
            }
            index += 1;
        }
    }
    None
}