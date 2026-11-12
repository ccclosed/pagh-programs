//! `printenv` — print environment variables from the initial `envp` array.
//!
//! ```text
//! printenv            # every entry as KEY=VALUE, one per line
//! printenv NAME...    # the value of each named variable, one per line
//! ```
//!
//! The environment is exactly what the kernel built on the initial stack and
//! what `libpagh::env::vars` re-reads: the ABI guarantees at least
//! `PWD=<shell cwd>` and nothing else unless a parent adds it (there is no
//! `/etc/environment`, no login shell and no `NAME=value` assignment syntax on
//! the kernel command line). The output is therefore short, and it is the
//! truth, not a truncated view.
//!
//! With no operands this is `env`'s listing. With operands a missing variable
//! prints nothing and the exit status is 1 (GNU behaviour, silent); found
//! values are printed even when another name is missing. `--` ends option
//! parsing; any other option is a usage error because nothing else is
//! implemented.
//!
//! Exit: 0 all operands found (or no operands); 1 some operand not found or a
//! write error; 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::env;
use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: printenv [NAME...]";

pub fn run(args: &[String]) -> i32 {
    let mut names: Vec<&str> = Vec::new();
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
        parse_options = false;
        if util::push(&mut names, arg.as_str()).is_err() {
            eprintln!("printenv: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if names.is_empty() {
        for (key, value) in env::vars() {
            if let Err(err) = util::write_all(1, key.as_bytes())
                .and_then(|()| util::write_all(1, b"="))
                .and_then(|()| util::write_all(1, value.as_bytes()))
                .and_then(|()| util::write_all(1, b"\n"))
            {
                eprintln!("printenv: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
        return util::EXIT_OK;
    }

    let mut status = util::EXIT_OK;
    for name in names {
        // A variable that does not exist is not an error message, only a
        // non-zero status (the same contract GNU printenv has).
        if let Some(value) = env::var(name) {
            if let Err(err) =
                util::write_all(1, value.as_bytes()).and_then(|()| util::write_all(1, b"\n"))
            {
                eprintln!("printenv: write error: {err}");
                return util::EXIT_FAILURE;
            }
        } else {
            status = util::EXIT_FAILURE;
        }
    }
    status
}
