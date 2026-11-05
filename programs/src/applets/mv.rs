//! `mv` — rename an entry or move it into a directory.
//!
//! This is a thin wrapper over `fs::rename` (`renameat2` with flags 0). A
//! destination that is an existing directory receives the source under its
//! basename; with several sources (or `-t DIR`) the destination must be a
//! directory. The move is one kernel transaction: this applet **never** falls
//! back to copy+delete. A destination on another filesystem answers `ENOTSUP`;
//! that is reported as an error and the source is left untouched. This is the
//! honest behaviour for an atomic rename and is documented in the README.
//!
//! `.`, `..` and `/` are refused as sources. A failed move is reported per
//! source and the remaining sources are still attempted; the status is then 1.
//! There is no `-f`, `-i`, `-n` or `-v`, no prompt, and no pipes/redirection in
//! the shell.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;
use libpagh::path;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: mv [-t DIR] SOURCE... DEST";

pub fn run(args: &[String]) -> i32 {
    let mut operands: Vec<&str> = Vec::new();
    let mut target_directory: Option<&str> = None;
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg == "-t" {
            let Some(dir) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            target_directory = Some(dir);
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            // Combined `-tDIR` is accepted too; no other options exist.
            if let Some(dir) = arg.strip_prefix("-t") {
                if !dir.is_empty() {
                    target_directory = Some(dir);
                    continue;
                }
            }
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        if util::push(&mut operands, arg).is_err() {
            eprintln!("mv: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let (sources, destination): (&[&str], &str) = match target_directory {
        Some(dir) => {
            if operands.is_empty() {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            }
            (&operands[..], dir)
        }
        None => {
            if operands.len() < 2 {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            }
            (
                &operands[..operands.len() - 1],
                operands[operands.len() - 1],
            )
        }
    };

    let destination_is_dir = match fs::stat(destination) {
        Ok(stat) => stat.is_dir(),
        Err(_) => false,
    };
    if sources.len() > 1 && !destination_is_dir {
        eprintln!("mv: target '{destination}' is not a directory");
        return util::EXIT_FAILURE;
    }

    let mut status = util::EXIT_OK;
    let mut cross_filesystem_note = false;

    for source in sources {
        if protected(source) {
            util::report("mv", source, Errno::EINVAL);
            status = util::EXIT_FAILURE;
            continue;
        }

        let target = if destination_is_dir {
            let base = path::basename(source);
            if base.is_empty() || base == "/" || base == "." || base == ".." {
                util::report("mv", source, Errno::EINVAL);
                status = util::EXIT_FAILURE;
                continue;
            }
            util::join_path(destination, base)
        } else {
            String::from(destination)
        };

        match fs::rename(source, &target) {
            Ok(()) => {}
            Err(Errno::ENOTSUP) => {
                util::report("mv", source, Errno::ENOTSUP);
                if !cross_filesystem_note {
                    eprintln!(
                        "mv: note: rename cannot cross filesystems; nothing was copied or deleted"
                    );
                    cross_filesystem_note = true;
                }
                status = util::EXIT_FAILURE;
            }
            Err(err) => {
                util::report("mv", source, err);
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}

fn protected(name: &str) -> bool {
    let trimmed = name.trim_end_matches('/');
    trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.ends_with("/.")
        || trimmed.ends_with("/..")
        || path::resolve(name) == "/"
}