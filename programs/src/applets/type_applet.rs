//! `type` — describe how a name resolves.
//!
//! This is the thin alias of `command -V` asked for by the catalog: with no
//! option it prints `name is /path`; `-v` prints only the path; `-V` is the
//! explicit long form. It shares the parser and the fixed `/mnt/bin` →
//! `PWD` → literal search in [`crate::applets::command`], so the two applets
//! cannot drift apart. A missing name is exit 1 and a diagnostic; no operands
//! or an unknown option is usage error 2.

use alloc::string::String;

use crate::applets::command;

const USAGE: &str = "usage: type [-v|-V] NAME...";

pub fn run(args: &[String]) -> i32 {
    command::run_named("type", args, command::Mode::Verbose, USAGE)
}