//! `cp` — copy files, symbolic links and (with `-r`) directory trees.
//!
//! A symbolic link is copied **as a link**: `read_link` returns the stored
//! target and `symlink` recreates it verbatim, so the copy does not depend on
//! the target existing and never follows it. Directories are copied
//! recursively, merging into an existing destination directory. A regular
//! file is copied byte-for-byte through an 8 KiB buffer.
//!
//! Overwriting is the default, and `-f` is accepted for compatibility but does
//! not change anything (there is no prompt to suppress). For a regular file the
//! kernel's `openat(O_TRUNC)` replaces the contents; for a symbolic-link
//! destination the existing entry is unlinked first, because `symlinkat` is
//! `EEXIST` rather than "replace". A directory destination is never removed.
//!
//! `.`/`..`/`/` are refused as sources when the destination is a directory
//! (the target name would be nonsense), and a directory is refused when the
//! lexically resolved destination is the source itself or lies inside it. The
//! guard is lexical: a symlinked destination is not resolved, and there is no
//! `lstat` to break a link cycle. Modes and timestamps are not copied — the
//! ABI has no mode persistence and no `utimes`.
//!
//! Multiple sources require a directory destination, exactly like `cp`.
//! Failures are reported per source and processing continues; the exit status
//! is 1 if anything failed. No pipes or redirection exist in the shell.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::{self, Dir, EntryKind, File};
use libpagh::path;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: cp [-rR] [-f] SOURCE... DEST";

#[derive(Clone, Copy, Default)]
struct Options {
    recursive: bool,
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
                    // Overwrite is already the default; see the module docs.
                    'f' => {}
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("cp: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.len() < 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let destination = operands[operands.len() - 1];
    let sources = &operands[..operands.len() - 1];
    let destination_is_dir = match fs::stat(destination) {
        Ok(stat) => stat.is_dir(),
        Err(_) => false,
    };

    if sources.len() > 1 && !destination_is_dir {
        eprintln!("cp: target '{destination}' is not a directory");
        return util::EXIT_FAILURE;
    }

    let mut status = util::EXIT_OK;
    for source in sources {
        let target = if destination_is_dir {
            let base = path::basename(source);
            if base.is_empty() || base == "/" || base == "." || base == ".." {
                util::report("cp", source, Errno::EINVAL);
                status = util::EXIT_FAILURE;
                continue;
            }
            util::join_path(destination, base)
        } else {
            String::from(destination)
        };

        if !copy_path(source, &target, &options) {
            status = util::EXIT_FAILURE;
        }
    }
    status
}

fn copy_path(source: &str, target: &str, options: &Options) -> bool {
    let kind = match util::path_kind(source) {
        Ok(kind) => kind,
        Err(err) => {
            util::report("cp", source, err);
            return false;
        }
    };

    match kind {
        EntryKind::Dir => {
            if !options.recursive {
                util::report("cp", source, Errno::EISDIR);
                return false;
            }
            if same_or_inside(source, target) {
                eprintln!("cp: cannot copy '{source}' into itself ('{target}')");
                return false;
            }
            copy_dir(source, target, options)
        }
        EntryKind::Symlink => copy_symlink(source, target),
        _ => copy_file(source, target),
    }
}

fn copy_file(source: &str, target: &str) -> bool {
    if same_file(source, target) {
        eprintln!("cp: '{source}' and '{target}' are the same file");
        return false;
    }

    let source_file = match File::open(source) {
        Ok(file) => file,
        Err(err) => {
            util::report("cp", source, err);
            return false;
        }
    };
    let target_file = match File::create(target) {
        Ok(file) => file,
        Err(err) => {
            util::report("cp", target, err);
            return false;
        }
    };

    let mut buffer = [0u8; 8192];
    loop {
        let read = match util::read_retry(source_file.as_raw_fd(), &mut buffer) {
            Ok(0) => return true,
            Ok(read) => read,
            Err(err) => {
                util::report("cp", source, err);
                return false;
            }
        };
        if let Err(err) = util::write_all(target_file.as_raw_fd(), &buffer[..read]) {
            util::report("cp", target, err);
            return false;
        }
    }
}

fn copy_symlink(source: &str, target: &str) -> bool {
    let link_target = match fs::read_link(source) {
        Ok(link) => link,
        Err(err) => {
            util::report("cp", source, err);
            return false;
        }
    };

    // `symlinkat` is EEXIST on any existing entry. The default overwrite
    // semantics remove a file/link first, but never a directory.
    match util::path_kind(target) {
        Ok(EntryKind::Dir) => {
            util::report("cp", target, Errno::EISDIR);
            return false;
        }
        Ok(_) => {
            if let Err(err) = fs::remove_file(target) {
                util::report("cp", target, err);
                return false;
            }
        }
        Err(Errno::ENOENT) => {}
        Err(err) => {
            util::report("cp", target, err);
            return false;
        }
    }

    match fs::symlink(&link_target, target) {
        Ok(()) => true,
        Err(err) => {
            util::report("cp", target, err);
            false
        }
    }
}

fn copy_dir(source: &str, target: &str, options: &Options) -> bool {
    match util::path_kind(target) {
        Ok(EntryKind::Dir) => {}
        Ok(_) => {
            util::report("cp", target, Errno::ENOTDIR);
            return false;
        }
        Err(Errno::ENOENT) => {
            if let Err(err) = fs::create_dir(target) {
                util::report("cp", target, err);
                return false;
            }
        }
        Err(err) => {
            util::report("cp", target, err);
            return false;
        }
    }

    let mut entries: Vec<(String, EntryKind)> = Vec::new();
    match Dir::open(source) {
        Ok(listing) => {
            for entry in listing {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        util::report("cp", source, err);
                        return false;
                    }
                };
                if entry.name() == "." || entry.name() == ".." {
                    continue;
                }
                if util::push(&mut entries, (String::from(entry.name()), entry.kind())).is_err() {
                    eprintln!("cp: out of memory");
                    return false;
                }
            }
        }
        Err(err) => {
            util::report("cp", source, err);
            return false;
        }
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));

    let mut ok = true;
    for (name, kind) in entries {
        let child_source = util::join_path(source, &name);
        let child_target = util::join_path(target, &name);
        let copied = match kind {
            EntryKind::Dir => copy_dir(&child_source, &child_target, options),
            EntryKind::Symlink => copy_symlink(&child_source, &child_target),
            _ => copy_file(&child_source, &child_target),
        };
        if !copied {
            ok = false;
        }
    }
    ok
}

/// The same lexical path, or the same inode (a hard link or a symlink to the
/// source). `fs::stat` follows the final link, which is exactly what `openat`
/// will do for the regular-file copy.
fn same_file(source: &str, target: &str) -> bool {
    if path::resolve(source) == path::resolve(target) {
        return true;
    }
    match (fs::stat(source), fs::stat(target)) {
        (Ok(left), Ok(right)) => left.ino == right.ino && left.kind == right.kind,
        _ => false,
    }
}

/// Lexical containment check used to refuse `cp -r dir dir/inside`.
fn same_or_inside(source: &str, target: &str) -> bool {
    let source = path::resolve(source);
    let target = path::resolve(target);
    if target == source {
        return true;
    }
    target.starts_with(&source) && target.as_bytes().get(source.len()) == Some(&b'/')
}