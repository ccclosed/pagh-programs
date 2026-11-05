//! `which` — locate a command by a fixed two-directory search.
//!
//! **The kernel does not set `PATH`.** Its environment has only `PWD`, so
//! there is nothing to split and no inherited search path. This port follows a
//! documented pagh convention instead:
//!
//! 1. `/mnt/bin/<name>` (where `pkg install` puts symlinks);
//! 2. `<PWD>/<name>`.
//!
//! The first candidate that exists **as a regular file** is printed on fd 1
//! (an absolute path) and the search for that operand stops. Symlinks are
//! followed by `stat`, so installer-created links are found; a directory is
//! not a command. The executable bit is not required: this is "where is it",
//! not an access check (`-x` in `test` inspects the mode bits). A name that is
//! not found produces no output and exit status 1; other operands are still
//! searched. No operands is usage error 2. There are no options beyond `--`,
//! no `-a` (print all matches) and no pipes/redirection.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;
use libpagh::env;

use crate::util;

const USAGE: &str = "usage: which COMMAND...";

pub fn run(args: &[String]) -> i32 {
    let mut operands: Vec<&str> = Vec::new();
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
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("which: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let pwd = env::current_dir();
    let mut status = util::EXIT_OK;

    for name in operands {
        let installed = format!("/mnt/bin/{name}");
        let local = util::join_path(&pwd, name);
        let mut found = false;

        for candidate in [&installed, &local] {
            if is_command(candidate) {
                if let Err(err) = util::write_line(1, candidate.as_bytes()) {
                    eprintln!("which: write error: {err}");
                    return util::EXIT_FAILURE;
                }
                found = true;
                break;
            }
        }

        if !found {
            status = util::EXIT_FAILURE;
        }
    }
    status
}

fn is_command(candidate: &str) -> bool {
    fs::stat(candidate).map(|stat| stat.is_file()).unwrap_or(false)
}