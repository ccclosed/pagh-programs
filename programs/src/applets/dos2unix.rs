//! `dos2unix` — convert CRLF line endings to LF, in place.
//!
//! ```text
//! dos2unix [FILE...]
//! ```
//!
//! Each named file is read completely, every `\r\n` pair becomes `\n`, and the
//! result is written back to the same path (open with `O_TRUNC`). A lone `\r`
//! is left alone, exactly like the classic tool: only a CR that immediately
//! precedes LF is part of a line ending. Files are rewritten only after the
//! whole input was read and converted, so a read error leaves the original
//! untouched; a write error is reported and the remaining files are still
//! processed.
//!
//! With no operands (or with `-`) the input is stdin and the converted bytes go
//! to stdout. The console's fd 0 has no EOF, so on this shell that mode waits
//! for input forever, like the other stdin filters; live coverage uses a file
//! operand.
//!
//! The buffer is the whole file: the ABI has `lseek`, but rewriting a file in
//! place while reading from it would truncate the data being read, so a
//! streaming in-place conversion is not expressible here. The 1 MiB user image
//! and the applet's own files are small; the limit is documented in README.
//!
//! `unix2dos` is the reverse conversion; the two are separate commands (this
//! port does not implement busybox's combined `-u` switch).
//!
//! Exit: 0 success; 1 open/read/write error (continues with the other files);
//! 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: dos2unix [FILE...]";

pub fn run(args: &[String]) -> i32 {
    run_mode(args, false)
}

/// Shared implementation of `dos2unix` (`to_dos = false`) and `unix2dos`
/// (`to_dos = true`).
pub fn run_mode(args: &[String], to_dos: bool) -> i32 {
    let program = if to_dos { "unix2dos" } else { "dos2unix" };
    let mut files: Vec<&str> = Vec::new();
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
        if util::push(&mut files, arg.as_str()).is_err() {
            eprintln!("{program}: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let stdin_mode = files.is_empty();
    if stdin_mode {
        files.push("-");
    }

    let mut status = util::EXIT_OK;
    for name in files {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report(program, name, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        let converted = match convert(&data, to_dos) {
            Ok(converted) => converted,
            Err(err) => {
                util::report(program, name, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        if stdin_mode || name == "-" {
            if let Err(err) = util::write_all(1, &converted) {
                eprintln!("{program}: write error: {err}");
                return util::EXIT_FAILURE;
            }
        } else if let Err(err) = fs::write_all(name, &converted) {
            util::report(program, name, err);
            status = util::EXIT_FAILURE;
        }
    }
    status
}

/// Convert `\r\n` -> `\n` (`to_dos = false`) or the reverse.
///
/// `unix2dos` leaves an existing CRLF alone instead of producing `\r\r\n`, so
/// it is idempotent on a DOS file; `dos2unix` only strips a CR that is directly
/// followed by LF.
fn convert(data: &[u8], to_dos: bool) -> Result<Vec<u8>, Errno> {
    // Worst case: every byte is a bare LF, so `unix2dos` can double the input.
    let capacity = if to_dos {
        data.len().checked_mul(2).ok_or(Errno::ENOMEM)?
    } else {
        data.len()
    };
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(capacity).map_err(|_| Errno::ENOMEM)?;

    if to_dos {
        for (index, byte) in data.iter().enumerate() {
            if *byte == b'\n' && (index == 0 || data[index - 1] != b'\r') {
                out.push(b'\r');
            }
            out.push(*byte);
        }
    } else {
        let mut index = 0usize;
        while index < data.len() {
            if data[index] == b'\r' && index + 1 < data.len() && data[index + 1] == b'\n' {
                out.push(b'\n');
                index += 2;
            } else {
                out.push(data[index]);
                index += 1;
            }
        }
    }
    Ok(out)
}
