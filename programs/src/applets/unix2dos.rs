//! `unix2dos` — convert LF line endings to CRLF, in place.
//!
//! ```text
//! unix2dos [FILE...]
//! ```
//!
//! Each named file is read completely, every bare `\n` becomes `\r\n`, and the
//! result is written back to the same path. An existing `\r\n` is kept as it is
//! (no `\r\r\n`), so running the command twice is idempotent. With no operands
//! (or with `-`) the input is stdin and the converted bytes go to stdout; on
//! this console that mode waits forever because fd 0 has no EOF.
//!
//! `dos2unix` is the reverse conversion and shares this implementation; the
//! rationale for buffering the whole file (in-place rewrite cannot stream) is
//! documented there and in README.
//!
//! Exit: 0 success; 1 open/read/write error (continues with the other files);
//! 2 usage.

use alloc::string::String;

use crate::applets::dos2unix;

pub fn run(args: &[String]) -> i32 {
    dos2unix::run_mode(args, true)
}
