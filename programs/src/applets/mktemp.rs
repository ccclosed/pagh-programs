//! `mktemp` — create one empty file with a PID/counter name and print it.
//!
//! The default directory is `/mnt` (the ext2 mount, the only writable tree for
//! user programs); `-p DIR` selects another one. The template is
//! `tmp.XXXXXX`, and the six `X`s are replaced by `<pid>-<counter>`, for
//! example `tmp.57-0`. This is **predictable, not random**: the ABI has no
//! `getrandom`, and this applet makes no randomness claim. An optional
//! TEMPLATE operand is accepted if it ends in exactly six `X` characters; the
//! replacement still contains the PID and counter.
//!
//! The ABI has no `O_EXCL`, so uniqueness is "`fs::stat` says absent, then
//! `File::create`" and the kernel runs one user process at a time. That is
//! sufficient here but is not an atomic exclusive create; it is documented
//! rather than hidden. The counter retries from 0 to 999 on `EEXIST`. The
//! created file is empty and is printed as an absolute path on fd 1.
//!
//! Failures (missing directory, a non-directory `-p`, a full counter range)
//! are reported on fd 2 and exit 1; bad options are usage error 2. There is no
//! `-d` (directory), `-u` (unlink-after-print), `-q`, `-t` or suffix option,
//! and no pipes/redirection.

use alloc::format;
use alloc::string::String;

use libpagh::eprintln;
use libpagh::fs::{self, File};
use libpagh::path;
use libpagh::{sys, Errno};

use crate::util;

const USAGE: &str = "usage: mktemp [-p DIR] [TEMPLATE]";
const DEFAULT_TEMPLATE: &str = "tmp.XXXXXX";
const DEFAULT_DIR: &str = "/mnt";
const MAX_ATTEMPTS: u64 = 1000;

pub fn run(args: &[String]) -> i32 {
    let mut directory: Option<&str> = None;
    let mut operands: alloc::vec::Vec<&str> = alloc::vec::Vec::new();
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg == "-p" {
            let Some(dir) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            if dir.is_empty() {
                eprintln!("mktemp: empty -p directory");
                return util::EXIT_USAGE;
            }
            directory = Some(dir);
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        if util::push(&mut operands, arg).is_err() {
            eprintln!("mktemp: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.len() > 1 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let template = operands.first().copied().unwrap_or(DEFAULT_TEMPLATE);
    let Some(prefix) = template.strip_suffix("XXXXXX") else {
        eprintln!("mktemp: template must end in XXXXXX: '{template}'");
        return util::EXIT_USAGE;
    };
    if prefix.contains('/') {
        eprintln!("mktemp: template must be a file name, not a path: '{template}'");
        return util::EXIT_USAGE;
    }

    let directory = path::resolve(directory.unwrap_or(DEFAULT_DIR));
    match fs::stat(&directory) {
        Ok(stat) if stat.is_dir() => {}
        Ok(_) => {
            util::report("mktemp", &directory, Errno::ENOTDIR);
            return util::EXIT_FAILURE;
        }
        Err(err) => {
            util::report("mktemp", &directory, err);
            return util::EXIT_FAILURE;
        }
    }

    let pid = sys::getpid().unwrap_or(0);
    for counter in 0..MAX_ATTEMPTS {
        let name = format!("{prefix}{pid}-{counter}");
        let candidate = util::join_path(&directory, &name);

        match fs::stat(&candidate) {
            Ok(_) => continue,
            Err(Errno::ENOENT) => {}
            Err(err) => {
                util::report("mktemp", &candidate, err);
                return util::EXIT_FAILURE;
            }
        }

        match File::create(&candidate) {
            Ok(_file) => {
                if let Err(err) = util::write_line(1, candidate.as_bytes()) {
                    eprintln!("mktemp: write error: {err}");
                    return util::EXIT_FAILURE;
                }
                return util::EXIT_OK;
            }
            Err(Errno::EEXIST) => continue,
            Err(err) => {
                util::report("mktemp", &candidate, err);
                return util::EXIT_FAILURE;
            }
        }
    }

    eprintln!("mktemp: could not create a unique file after {MAX_ATTEMPTS} attempts");
    util::EXIT_FAILURE
}