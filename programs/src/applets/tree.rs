//! `tree` — draw a directory hierarchy with `|--` / `` `-- `` branches.
//!
//! `-L N` limits the displayed depth (the operand itself is depth 0, so
//! `-L 1` shows its immediate children; `-L 0` shows only the operand). `-a`
//! is accepted and changes nothing: the VFS has no hidden-file convention and
//! every entry is listed. The final line counts displayed directories and
//! files; the start directory is included in the directory count, and a
//! symbolic link is counted as a file and never followed.
//!
//! Children are sorted by name, so the drawing (and the transcript) is stable.
//! Multiple operands are drawn one after another, with one combined summary at
//! the end. Errors are reported per path, the remaining paths are still drawn,
//! and the exit status is 1 if anything failed. There is no `-d`, `-f`, `-s`,
//! `-C` or color output, and no pipes/redirection.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::{Dir, EntryKind};

use crate::util;

const USAGE: &str = "usage: tree [-a] [-L N] [PATH...]";

#[derive(Clone, Copy, Default)]
struct Options {
    max_depth: Option<usize>,
}

#[derive(Clone, Copy, Default)]
struct Counts {
    dirs: u64,
    files: u64,
}

pub fn run(args: &[String]) -> i32 {
    let mut options = Options::default();
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
        if parse_options && arg == "-a" {
            // No hidden-file convention exists in this VFS.
            continue;
        }
        if parse_options && arg == "-L" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match value.parse::<usize>() {
                Ok(depth) => options.max_depth = Some(depth),
                Err(_) => {
                    eprintln!("tree: invalid level: '{value}'");
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
            eprintln!("tree: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        operands.push(".");
    }

    let mut counts = Counts::default();
    let mut status = util::EXIT_OK;

    for name in operands {
        let kind = match util::path_kind(name) {
            Ok(kind) => kind,
            Err(err) => {
                util::report("tree", name, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };

        if let Err(err) = util::write_line(1, name.as_bytes()) {
            eprintln!("tree: write error: {err}");
            return util::EXIT_FAILURE;
        }

        if matches!(kind, EntryKind::Dir) {
            counts.dirs += 1;
            let show_children = options.max_depth.map(|max| max > 0).unwrap_or(true);
            if show_children {
                match walk(name, "", 1, &options, &mut counts) {
                    Ok(ok) => {
                        if !ok {
                            status = util::EXIT_FAILURE;
                        }
                    }
                    Err(()) => return util::EXIT_FAILURE,
                }
            }
        } else {
            counts.files += 1;
        }
    }

    if let Err(err) = util::write_line(1, b"") {
        eprintln!("tree: write error: {err}");
        return util::EXIT_FAILURE;
    }
    let summary = format!(
        "{} {}, {} {}",
        counts.dirs,
        plural(counts.dirs, "directory", "directories"),
        counts.files,
        plural(counts.files, "file", "files")
    );
    if let Err(err) = util::write_line(1, summary.as_bytes()) {
        eprintln!("tree: write error: {err}");
        return util::EXIT_FAILURE;
    }
    status
}

/// Draw the children of `dir` at `level` (1 = immediate child of the root).
/// `Err(())` means a write failed; `Ok(false)` means a read failed but drawing
/// continued.
fn walk(
    dir: &str,
    prefix: &str,
    level: usize,
    options: &Options,
    counts: &mut Counts,
) -> Result<bool, ()> {
    let mut entries: Vec<(String, EntryKind)> = Vec::new();
    let mut ok = true;

    match Dir::open(dir) {
        Ok(listing) => {
            for entry in listing {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        util::report("tree", dir, err);
                        ok = false;
                        break;
                    }
                };
                if entry.name() == "." || entry.name() == ".." {
                    continue;
                }
                if util::push(&mut entries, (String::from(entry.name()), entry.kind())).is_err() {
                    eprintln!("tree: out of memory");
                    return Err(());
                }
            }
        }
        Err(err) => {
            util::report("tree", dir, err);
            return Ok(false);
        }
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let count = entries.len();

    for (index, (name, kind)) in entries.into_iter().enumerate() {
        let last = index + 1 == count;
        let branch = if last { "`-- " } else { "|-- " };
        let line = format!("{prefix}{branch}{name}");
        if let Err(err) = util::write_line(1, line.as_bytes()) {
            eprintln!("tree: write error: {err}");
            return Err(());
        }

        match kind {
            EntryKind::Dir => {
                counts.dirs += 1;
                let descend = options.max_depth.map(|max| level < max).unwrap_or(true);
                if descend {
                    let mut child_prefix =
                        String::with_capacity(prefix.len() + if last { 4 } else { 4 });
                    child_prefix.push_str(prefix);
                    child_prefix.push_str(if last { "    " } else { "|   " });
                    let child = util::join_path(dir, &name);
                    if !walk(&child, &child_prefix, level + 1, options, counts)? {
                        ok = false;
                    }
                }
            }
            _ => counts.files += 1,
        }
    }
    Ok(ok)
}

fn plural<'a>(count: u64, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 {
        one
    } else {
        many
    }
}