//! `rm` — remove files, symbolic links and (with `-r`) directory trees.
//!
//! `-f` ignores a missing operand and does not change any other error into
//! success; `-d` removes an **empty** directory; `-r`/`-R` removes a tree
//! bottom-up. A symbolic link is always removed as a link: the ABI's
//! `unlinkat` never follows the final component, and this applet never stats
//! through a link before deciding (it uses `readlink` to recognise one).
//!
//! A directory without `-r` or `-d` is `EISDIR`, and a non-empty directory
//! without `-r` is `ENOTEMPTY` from `remove_dir`. Failures are reported per
//! operand and the remaining operands are still processed; the exit status is
//! then 1. `.`, `..` and `/` are refused even with `-r`: the VFS has no
//! `unlinkat` guard for the root and a recursive removal there would empty the
//! disk.
//!
//! No confirmation prompt (the console has no `y/n` reader here), no
//! `--preserve-root`, no `-i`/`-v`. There are no pipes or redirection in the
//! kernel shell.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::{self, Dir, EntryKind};
use libpagh::path;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: rm [-rR] [-f] [-d] FILE...";

#[derive(Clone, Copy, Default)]
struct Options {
    recursive: bool,
    force: bool,
    empty_dirs: bool,
}

pub fn run(args: &[String]) -> i32 {
    let mut options = Options::default();
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            for option in arg[1..].chars() {
                match option {
                    'r' | 'R' => options.recursive = true,
                    'f' => options.force = true,
                    'd' => options.empty_dirs = true,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("rm: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        if protected(name) {
            // A recursive `rm /` would unlink the mount's whole tree; refuse
            // the dot/root spellings even when -f is given.
            util::report("rm", name, Errno::EINVAL);
            status = util::EXIT_FAILURE;
            continue;
        }
        if !remove_one(name, &options) {
            status = util::EXIT_FAILURE;
        }
    }
    status
}

/// `.`, `..`, anything ending in those components, and `/` itself.
fn protected(name: &str) -> bool {
    let trimmed = name.trim_end_matches('/');
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.ends_with("/.")
        || trimmed.ends_with("/..")
    {
        return true;
    }
    path::resolve(name) == "/"
}

fn remove_one(name: &str, options: &Options) -> bool {
    let kind = match util::path_kind(name) {
        Ok(kind) => kind,
        Err(Errno::ENOENT) if options.force => return true,
        Err(err) => {
            util::report("rm", name, err);
            return false;
        }
    };

    match kind {
        EntryKind::Dir => {
            if options.recursive {
                remove_tree(name, options)
            } else if options.empty_dirs {
                match fs::remove_dir(name) {
                    Ok(()) => true,
                    Err(Errno::ENOENT) if options.force => true,
                    Err(err) => {
                        util::report("rm", name, err);
                        false
                    }
                }
            } else {
                util::report("rm", name, Errno::EISDIR);
                false
            }
        }
        _ => match fs::remove_file(name) {
            Ok(()) => true,
            Err(Errno::ENOENT) if options.force => true,
            Err(err) => {
                util::report("rm", name, err);
                false
            }
        },
    }
}

/// Remove `dir`'s children and then `dir` itself.
fn remove_tree(dir: &str, options: &Options) -> bool {
    let mut ok = true;
    let mut entries: Vec<(String, EntryKind)> = Vec::new();

    match Dir::open(dir) {
        Ok(listing) => {
            for entry in listing {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        util::report("rm", dir, err);
                        ok = false;
                        break;
                    }
                };
                let name = entry.name();
                if name == "." || name == ".." {
                    continue;
                }
                if util::push(&mut entries, (String::from(name), entry.kind())).is_err() {
                    eprintln!("rm: out of memory");
                    return false;
                }
            }
        }
        Err(err) => {
            util::report("rm", dir, err);
            return false;
        }
    }

    for (name, kind) in entries {
        let child = util::join_path(dir, &name);
        if matches!(kind, EntryKind::Dir) {
            if !remove_tree(&child, options) {
                ok = false;
            }
        } else {
            match fs::remove_file(&child) {
                Ok(()) => {}
                Err(Errno::ENOENT) if options.force => {}
                Err(err) => {
                    util::report("rm", &child, err);
                    ok = false;
                }
            }
        }
    }

    match fs::remove_dir(dir) {
        Ok(()) => {}
        Err(Errno::ENOENT) if options.force => {}
        Err(err) => {
            util::report("rm", dir, err);
            ok = false;
        }
    }
    ok
}