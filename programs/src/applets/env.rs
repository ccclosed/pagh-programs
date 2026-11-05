//! `env` — print the process environment, one `KEY=VALUE` per line.
//!
//! Entries come from the `envp` array the kernel builds on the initial stack
//! (read through `libpagh::env::vars`), so the output is exactly the
//! environment the shell passed: at least `PWD=/...`, plus anything a future
//! parent adds. `env` does **not** run a command: the ABI has `execve`, but
//! this port deliberately has no `env NAME=value command` mode, and the shell
//! cannot provide one anyway.
//!
//! No options and no operands. Exit: 0 success, 1 write error, 2 usage.

use alloc::string::String;

use libpagh::env;
use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: env";

pub fn run(args: &[String]) -> i32 {
    // `--` is accepted for consistency with the other applets; anything else
    // (including `-i` and a command) is not implemented and is a usage error.
    match args.first().map(String::as_str) {
        None => {}
        Some("--") if args.len() == 1 => {}
        Some(_) => {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
    }

    for (key, value) in env::vars() {
        if let Err(err) = util::write_all(1, key.as_bytes())
            .and_then(|()| util::write_all(1, b"="))
            .and_then(|()| util::write_all(1, value.as_bytes()))
            .and_then(|()| util::write_all(1, b"\n"))
        {
            eprintln!("env: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    util::EXIT_OK
}
