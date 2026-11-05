//! `sha384sum` — SHA-384 message digest over files or stdin.
//!
//! SHA-384 is the FIPS 180-4 truncated SHA-512 construction: the same
//! compression function with the SHA-384 initial state, keeping the first 48
//! bytes of the state. The shared front end in [`super::hashsum`] implements
//! the coreutils interface (`-b`, `-c`, `--quiet`, `--status`, files/stdin).
//! See that module for the parse rules and exit codes.

use alloc::string::String;

use super::digest::Algorithm;
use super::hashsum;

pub fn run(args: &[String]) -> i32 {
    hashsum::run("sha384sum", Algorithm::Sha384, args)
}