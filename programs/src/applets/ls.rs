//! `ls` — list directory entries.
//!
//! The VFS has no `.`/`..` entries, so `-a` is accepted and changes nothing.
//! Output is one name per line by default and with `-1`; directories get a
//! trailing `/` (including the operand under `-d`). `-l` prints the minimal
//! `drwxr-xr-x 4096 name/` form: the kind character and the nine permission
//! bits from `fs::stat`, the size, and the name. There is no `lstat` in the
//! ABI, so `-l` on a symlink prints the target's metadata with the directory
//! entry's kind character.
//!
//! Multiple operands get a `ls: path:` header (blank line between groups).
//! A missing operand is reported and the remaining operands are still listed;
//! the exit status is then 1. Options themselves are a usage error (2).
//!
//! No colors, no recursion, no `-h`/`-i`/`-s`; pipes and redirection do not
//! exist in the kernel shell, so output always goes to fd 1.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::{self, Dir, EntryKind};
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: ls [-a] [-1] [-l] [-d] [FILE...]";

#[derive(Clone, Copy, Default)]
struct Options {
    long: bool,
    dir: bool,
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
                    // The VFS never reports dot entries, and the default layout
                    // is already one per line, so these are accepted no-ops.
                    'a' | '1' => {}
                    'l' => options.long = true,
                    'd' => options.dir = true,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        operands.push(arg.as_str());
    }

    if operands.is_empty() {
        operands.push(".");
    }

    let multiple = operands.len() > 1;
    let mut status = util::EXIT_OK;

    for (index, name) in operands.iter().enumerate() {
        if multiple {
            if index > 0 {
                if let Err(err) = util::write_all(1, b"\n") {
                    eprintln!("ls: write error: {err}");
                    return util::EXIT_FAILURE;
                }
            }
            let header = format!("ls: {name}:");
            if let Err(err) = util::write_line(1, header.as_bytes()) {
                eprintln!("ls: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }

        match list_operand(name, &options) {
            Ok(true) => {}
            Ok(false) => status = util::EXIT_FAILURE,
            Err(err) => {
                util::report("ls", name, err);
                status = util::EXIT_FAILURE;
            }
        }
    }

    status
}

/// List one operand. `Ok(false)` means the listing was produced but at least
/// one entry could not be stat'ed (the caller exits 1).
fn list_operand(name: &str, options: &Options) -> Result<bool, Errno> {
    let stat = fs::stat(name)?;

    if options.dir || !stat.is_dir() {
        emit_entry(name, stat.entry_kind(), Some(&stat), options.long)?;
        return Ok(true);
    }

    let dir = Dir::open(name)?;
    let mut entries: Vec<(String, EntryKind)> = Vec::new();
    for entry in dir {
        let entry = entry?;
        entries.try_reserve(1).map_err(|_| Errno::ENOMEM)?;
        entries.push((String::from(entry.name()), entry.kind()));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));

    let mut all_ok = true;
    for (entry_name, kind) in entries {
        let child = child_path(name, &entry_name);
        let meta = match fs::stat(&child) {
            Ok(meta) => Some(meta),
            Err(err) => {
                // Keep listing: a single unreadable entry is not a reason to
                // drop the rest of the directory, but it does affect status.
                util::report("ls", &child, err);
                all_ok = false;
                None
            }
        };
        emit_entry(&entry_name, kind, meta.as_ref(), options.long)?;
    }

    Ok(all_ok)
}

fn child_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

/// Print one entry, adding the trailing `/` for directories.
fn emit_entry(
    name: &str,
    kind: EntryKind,
    stat: Option<&libpagh::Stat>,
    long: bool,
) -> Result<(), Errno> {
    let slash = matches!(kind, EntryKind::Dir);
    let mut line = if long {
        let mode = stat.map(|meta| meta.mode).unwrap_or(0);
        let size = stat.map(|meta| meta.size).unwrap_or(0);
        format!("{} {size} {name}", mode_string(kind, mode))
    } else {
        String::from(name)
    };
    if slash {
        line.push('/');
    }
    util::write_line(1, line.as_bytes())
}

/// `drwxr-xr-x`-style mode text: kind character plus the low nine bits.
fn mode_string(kind: EntryKind, mode: u32) -> String {
    let mut out = String::with_capacity(10);
    out.push(match kind {
        EntryKind::File => '-',
        EntryKind::Dir => 'd',
        EntryKind::Symlink => 'l',
        EntryKind::Other => '?',
    });
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        out.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        out.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        out.push(if bits & 0o1 != 0 { 'x' } else { '-' });
    }
    out
}