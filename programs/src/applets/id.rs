//! `id` — print the numeric and symbolic identity of the process.
//!
//! The pagh ABI has no uid/gid syscall yet, so the values are the boot
//! identity: uid 0, gid 0, one supplementary group 0, all named `root`. The
//! default line is exactly
//!
//! ```text
//! uid=0 gid=0(root) groups=0(root)
//! ```
//!
//! `-u`, `-g` and `-G` print the user id, group id and group list instead;
//! they may be combined (`-u -g` prints both values, space separated). GNU's
//! user-name operand is not supported because there is no user database (an
//! operand is a usage error). Unknown options are usage errors.
//!
//! Exit: 0 success, 1 write error, 2 usage.

use alloc::string::String;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: id [-u] [-g] [-G]";

pub fn run(args: &[String]) -> i32 {
    let mut user = false;
    let mut group = false;
    let mut groups = false;
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            // Accept bundled short flags (`-uG`) as well as separate ones.
            let flags = &arg[1..];
            let mut recognised = !flags.is_empty();
            for flag in flags.bytes() {
                match flag {
                    b'u' => user = true,
                    b'g' => group = true,
                    b'G' => groups = true,
                    _ => {
                        recognised = false;
                        break;
                    }
                }
            }
            if !recognised {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            }
            continue;
        }
        // There is no passwd/group database in the ABI; a user operand cannot
        // be resolved, so it is a usage error rather than a misleading answer.
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    if !user && !group && !groups {
        return write_line("uid=0 gid=0(root) groups=0(root)", "id");
    }

    let mut line = String::new();
    let mut first = true;
    for selected in [user, group, groups] {
        if !selected {
            continue;
        }
        if !first {
            line.push(' ');
        }
        line.push('0');
        first = false;
    }
    write_line(&line, "id")
}

fn write_line(line: &str, program: &str) -> i32 {
    match util::write_line(1, line.as_bytes()) {
        Ok(()) => util::EXIT_OK,
        Err(err) => {
            eprintln!("{program}: write error: {err}");
            util::EXIT_FAILURE
        }
    }
}
