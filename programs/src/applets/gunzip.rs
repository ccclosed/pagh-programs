//! `gunzip` — the decompression half of [`super::gzip`].
//!
//! It is a distinct applet name, but its contract is exactly `gzip -d`: same
//! stored-block decoder, same `-c`, `-k` and `-f` options, same naming rules
//! (`FILE.gz` -> `FILE`) and exit codes. Keeping it separate means an
//! installed `gunzip` symlink works without the caller remembering `-d`.

use alloc::string::String;

use super::gzip;

pub fn run(args: &[String]) -> i32 {
    gzip::run_with("gunzip", args, true)
}
