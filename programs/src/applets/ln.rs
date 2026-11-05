//! `ln` — hard links by default, symbolic links with `-s`.
//!
//! A symbolic link stores its target **verbatim**: `ln -s ../hello.txt link`
//! writes exactly `../hello.txt`, not a resolved path. That is the whole point
//! of a symlink and is the `symlinkat` contract. A hard link is `linkat` with
//! flags 0; the kernel refuses a directory with `EISDIR`, so this applet does
//! not need to (and must not) pre-check through a symlink.
//!
//! `TARGET... LINK`: with more than one target, `LINK` must be an existing
//! directory and each link is created inside it under the target's basename.
//! An existing link path is `EEXIST`; the applet never overwrites. Failures are
//! reported per target and the rest are attempted; exit status is 1 if any
//! failed. There are no `-f`/`-n`/`-b` options and no pipes in the shell.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;
use libpagh::path;

use crate::util;

const USAGE: &str = "usage: ln [-s] TARGET... LINK";

pub fn run(args: &[String]) -> i32 {
    let mut symbolic = false;
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
                    's' => symbolic = true,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("ln: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.len() < 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let destination = operands[operands.len() - 1];
    let targets = &operands[..operands.len() - 1];
    let destination_is_dir = match fs::stat(destination) {
        Ok(stat) => stat.is_dir(),
        Err(_) => false,
    };

    if targets.len() > 1 && !destination_is_dir {
        eprintln!("ln: target '{destination}' is not a directory");
        return util::EXIT_FAILURE;
    }

    let mut status = util::EXIT_OK;
    for target in targets {
        let link_path = if destination_is_dir {
            let base = path::basename(target);
            if base.is_empty() || base == "/" || base == "." || base == ".." {
                util::report("ln", target, libpagh::Errno::EINVAL);
                status = util::EXIT_FAILURE;
                continue;
            }
            util::join_path(destination, base)
        } else {
            String::from(destination)
        };

        let result = if symbolic {
            // The target is deliberately not resolved: a symlink stores the
            // spelling it was given.
            fs::symlink(target, &link_path)
        } else {
            fs::hard_link(target, &link_path)
        };

        if let Err(err) = result {
            util::report("ln", target, err);
            status = util::EXIT_FAILURE;
        }
    }
    status
}