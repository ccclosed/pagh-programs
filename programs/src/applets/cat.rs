//! `cat` — concatenate files to stdout.
//!
//! No operands means stdin. `-` names stdin explicitly; `-u` is accepted and
//! ignored (there is no buffering to disable). Unknown options are a usage
//! error. A per-file open/read error is reported on fd 2 and the remaining
//! files are still processed; the applet exits 1 if anything failed.
//!
//! Reading fd 0 uses the `read` + `EAGAIN` + `sched_yield` loop from
//! `util::read_retry`; see the API README for why there is no `poll`.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: cat [-u] [FILE...]";

pub fn run(args: &[String]) -> i32 {
    let mut files: Vec<&str> = Vec::new();
    let mut options = true;
    for arg in args {
        if options && arg == "--" {
            options = false;
            continue;
        }
        if options && arg == "-u" {
            // Accepted for compatibility; output is already unbuffered here.
            continue;
        }
        if options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        files.push(arg.as_str());
    }

    let default_stdin = files.is_empty();
    if default_stdin {
        files.push("-");
    }

    let mut status = util::EXIT_OK;
    let mut buf = [0u8; 8192];

    for name in files {
        let input = match util::Input::open(name) {
            Ok(input) => input,
            Err(err) => {
                util::report("cat", name, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };

        loop {
            match util::read_retry(input.fd(), &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if let Err(err) = util::write_all(1, &buf[..n]) {
                        eprintln!("cat: write error: {err}");
                        return util::EXIT_FAILURE;
                    }
                }
                Err(err) => {
                    util::report("cat", name, err);
                    status = util::EXIT_FAILURE;
                    break;
                }
            }
        }
    }

    status
}