//! `sha3sum` — SHA3-256 message digest over files or stdin.
//!
//! The algorithm is FIPS 202 SHA3-256 (Keccak-f[1600], a 136-byte rate and the
//! `0x06` domain-separation suffix), implemented in [`super::digest`]. The
//! shared front end in [`super::hashsum`] gives the coreutils interface:
//! `hex  name` output, `-b` (`hex *name`), `-c` check mode, `--quiet`,
//! `--status`, and files or stdin (`-`). See that module for the parse rules
//! and exit codes.

use alloc::string::String;

use super::digest::Algorithm;
use super::hashsum;

pub fn run(args: &[String]) -> i32 {
    hashsum::run("sha3sum", Algorithm::Sha3_256, args)
}