//! `md5sum` — MD5 message digest over files or stdin.
//!
//! The shared front end in [`super::hashsum`] implements the coreutils
//! interface: plain `hex  name` output and `-c` check mode. See that module
//! for the parse rules and exit codes.

use alloc::string::String;

use super::digest::Algorithm;
use super::hashsum;

pub fn run(args: &[String]) -> i32 {
    hashsum::run("md5sum", Algorithm::Md5, args)
}
