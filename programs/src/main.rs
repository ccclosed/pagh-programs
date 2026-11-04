//! `paghbox`: one static ELF with a dispatch table of applets.
//!
//! Invocation is busybox-style:
//!
//! ```text
//! paghbox <applet> [args...]      # explicit
//! <applet> [args...]              # via an installer-created symlink
//! ```
//!
//! The applet sees `argv[0]` = its own name and the remaining arguments; there
//! is no shell yet, so no pipe/redirection support is implied.
//!
//! Exit codes: `0` success, `1` data/runtime error, `2` usage error, `127`
//! unknown applet, `101` panic (libpagh), `102` out of memory (libpagh).

#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::{env, path};

mod applets;
mod util;

#[no_mangle]
pub extern "C" fn main(
    _argc: i32,
    _argv: *const *const u8,
    _envp: *const *const u8,
) -> i32 {
    let argv: Vec<String> = env::args().collect();
    if argv.is_empty() {
        usage();
        return util::EXIT_FAILURE;
    }

    let invoked = path::basename(&argv[0]);
    let (name, rest): (&str, &[String]) = if invoked == "paghbox" {
        match argv.get(1) {
            Some(name) => (name.as_str(), &argv[2..]),
            None => {
                usage();
                return util::EXIT_FAILURE;
            }
        }
    } else {
        (invoked, &argv[1..])
    };

    match applets::dispatch(name, rest) {
        Some(code) => code,
        None => {
            eprintln!("paghbox: unknown applet '{name}'");
            eprintln!("applets: {}", applets::NAMES.join(" "));
            util::EXIT_NOAPPLET
        }
    }
}

fn usage() {
    eprintln!("usage: paghbox <applet> [args...]");
    eprintln!("       <applet> [args...]          (argv[0] symlink)");
    eprintln!("applets: {}", applets::NAMES.join(" "));
}