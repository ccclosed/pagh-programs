//! `sha224sum` — SHA-224 message digest over files or stdin.
//!
//! SHA-224 is the FIPS 180-4 truncated SHA-256 construction: the same
//! compression function with the SHA-224 initial state, keeping the first 28
//! bytes of the state. The shared front end in [`super::hashsum`] implements
//! the coreutils interface (`-b`, `-c`, `--quiet`, `--status`, files/stdin).
//! See that module for the parse rules and exit codes.

use alloc::string::String;

use super::digest::Algorithm;
use super::hashsum;

pub fn run(args: &[String]) -> i32 {
    hashsum::run("sha224sum", Algorithm::Sha224, args)
}