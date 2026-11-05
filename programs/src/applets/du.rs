//! `du` — report the byte total of files under each operand.
//!
//! Totals are the sum of `fs::stat(...).size` for regular files; the
//! filesystem's directory size (ext2 reports 4096) is **not** counted, because
//! it is allocation metadata, not file content. Symbolic links count as 0 and
//! are never followed: the walk uses the `getdents64` kind, so a link to a
//! directory cannot cause a cycle. Hard links are counted once per name — the
//! ABI exposes no link count, so `du` cannot deduplicate them; this is a
//! documented gap.
//!
//! Default output is one `SIZE\tPATH` line per directory, children before the
//! parent (post-order). `-s` prints only the operand's total, `-a` adds every
//! file, and `-h` formats the size with one decimal and a `K`/`M`/`G`/`T`/`P`/`E`
//! suffix (integer arithmetic only — the kernel does not enable the FPU).
//! Unreadable entries are reported on fd 2 and the walk continues; exit status
//! is 1 if anything failed. Usage errors are 2. No `-x`, `-c`, `-b`, `-k` or
//! `--apparent-size` variants, and no pipes/redirection.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::{self, Dir, EntryKind};

use crate::util;

const USAGE: &str = "usage: du [-s] [-a] [-h] [PATH...]";

#[derive(Clone, Copy, Default)]
struct Options {
    summary: bool,
    all: bool,
    human: bool,
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
                    's' => options.summary = true,
                    'a' => options.all = true,
                    'h' => options.human = true,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("du: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        operands.push(".");
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        let kind = match util::path_kind(name) {
            Ok(kind) => kind,
            Err(err) => {
                util::report("du", name, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        match measure(name, kind, true, &options) {
            Ok((_total, ok)) => {
                if !ok {
                    status = util::EXIT_FAILURE;
                }
            }
            Err(()) => return util::EXIT_FAILURE,
        }
    }
    status
}

/// Return `(byte total, all syscalls succeeded)`; `Err(())` means the output
/// write failed and the applet must stop with status 1.
fn measure(
    path: &str,
    kind: EntryKind,
    is_root: bool,
    options: &Options,
) -> Result<(u64, bool), ()> {
    match kind {
        EntryKind::Dir => {
            let mut total = 0u64;
            let mut ok = true;
            let mut entries: Vec<(String, EntryKind)> = Vec::new();

            match Dir::open(path) {
                Ok(listing) => {
                    for entry in listing {
                        let entry = match entry {
                            Ok(entry) => entry,
                            Err(err) => {
                                util::report("du", path, err);
                                ok = false;
                                break;
                            }
                        };
                        if entry.name() == "." || entry.name() == ".." {
                            continue;
                        }
                        if util::push(&mut entries, (String::from(entry.name()), entry.kind()))
                            .is_err()
                        {
                            eprintln!("du: out of memory");
                            return Err(());
                        }
                    }
                }
                Err(err) => {
                    util::report("du", path, err);
                    ok = false;
                }
            }
            entries.sort_by(|left, right| left.0.cmp(&right.0));

            for (name, child_kind) in entries {
                let child = util::join_path(path, &name);
                let (size, child_ok) = measure(&child, child_kind, false, options)?;
                total = total.saturating_add(size);
                if !child_ok {
                    ok = false;
                }
            }

            if !options.summary || is_root {
                emit(total, path, options)?;
            }
            Ok((total, ok))
        }
        EntryKind::Symlink => {
            // Not followed: a link's own "content" is its target string, but
            // that is metadata; both GNU du and this port count 0 bytes.
            if options.all || is_root {
                emit(0, path, options)?;
            }
            Ok((0, true))
        }
        _ => {
            let size = match fs::stat(path) {
                Ok(stat) => stat.size,
                Err(err) => {
                    util::report("du", path, err);
                    return Ok((0, false));
                }
            };
            if options.all || is_root {
                emit(size, path, options)?;
            }
            Ok((size, true))
        }
    }
}

fn emit(size: u64, path: &str, options: &Options) -> Result<(), ()> {
    let rendered = if options.human {
        human_size(size)
    } else {
        format!("{size}")
    };
    let line = format!("{rendered}\t{path}");
    if let Err(err) = util::write_line(1, line.as_bytes()) {
        eprintln!("du: write error: {err}");
        return Err(());
    }
    Ok(())
}

/// Integer-only one-decimal human size (`1536` -> `1.5K`).
fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes}");
    }
    let units = [b'K', b'M', b'G', b'T', b'P', b'E'];
    let mut unit = 0usize;
    let mut divisor = 1024u64;
    while unit + 1 < units.len() && bytes / divisor >= 1024 {
        divisor = divisor.saturating_mul(1024);
        unit += 1;
    }
    let whole = bytes / divisor;
    let remainder = (bytes % divisor) * 10 / divisor;
    format!("{whole}.{remainder}{}", units[unit] as char)
}