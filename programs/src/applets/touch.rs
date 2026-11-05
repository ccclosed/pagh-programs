//! `touch` — create an empty file when it is absent.
//!
//! The ABI has no `utimes`/`utimensat`, so an existing file is opened and left
//! completely alone: its size and contents do not change and its timestamps
//! are not updated. That is the honest subset of `touch`; the missing part is
//! documented here and in the README.
//!
//! Multiple operands are attempted in order. Creating a file whose parent is
//! missing, or opening an existing entry the kernel refuses, is reported as
//! `touch: operand: error`; the exit status is then 1. There are no options
//! (`-c`, `-a`, `-m`, `-d` are absent), so anything starting with `-` except
//! `--` is a usage error (2).

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::File;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: touch FILE...";

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
            eprintln!("touch: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        let result = ensure_exists(name);
        if let Err(err) = result {
            util::report("touch", name, err);
            status = util::EXIT_FAILURE;
        }
    }
    status
}

fn ensure_exists(name: &str) -> Result<(), Errno> {
    // Open first: `File::create` would truncate an existing file, which touch
    // must never do. A successful open is all that is needed (no utime).
    match File::open(name) {
        Ok(_existing) => Ok(()),
        Err(Errno::ENOENT) => {
            let _created = File::create(name)?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}