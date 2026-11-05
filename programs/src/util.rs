//! Shared plumbing for the applets: console reads that honour `EAGAIN`, whole
//! output writes, input opening (`-` means stdin) and small option helpers.
//!
//! Everything returns [`Errno`]; applets decide the diagnostics and the exit
//! code. Nothing here panics on bad input.

use alloc::vec::Vec;

use libpagh::errno::{Errno, Result};
use libpagh::fs::File;
use libpagh::sys;
use libpagh::EntryKind;

/// Exit code for a normal result.
pub const EXIT_OK: i32 = 0;
/// Exit code for a data/runtime failure (a file could not be read, a number
/// was invalid, writing failed).
pub const EXIT_FAILURE: i32 = 1;
/// Exit code for a command-line syntax error.
pub const EXIT_USAGE: i32 = 2;
/// Exit code for an unknown applet name (busybox convention).
pub const EXIT_NOAPPLET: i32 = 127;

/// Read from `fd`, retrying `EAGAIN` by yielding the CPU.
///
/// `0` means EOF. This is the ABI's console-input contract: fd 0 returns
/// `-EAGAIN` until a byte is queued, and there is no `poll` to wait on.
pub fn read_retry(fd: i32, buf: &mut [u8]) -> Result<usize> {
    loop {
        match sys::read(fd, buf) {
            Ok(n) if n <= buf.len() => return Ok(n),
            // A kernel that claims more than the buffer is broken; never slice
            // past it (that would panic, and this process has no unwinding).
            Ok(_) => return Err(Errno::EIO),
            Err(Errno::EAGAIN) => {
                // The only voluntary switch available; ignore a yield failure
                // and retry the read (a broken yield must not hang output).
                let _ = sys::sched_yield();
            }
            other => return other,
        }
    }
}

/// Write all of `buf` to `fd`, retrying `EAGAIN` with `sched_yield`.
pub fn write_all(fd: i32, mut buf: &[u8]) -> Result<()> {
    while !buf.is_empty() {
        match sys::write(fd, buf) {
            Ok(0) => return Err(Errno::EIO),
            Ok(n) if n <= buf.len() => buf = &buf[n..],
            Ok(_) => return Err(Errno::EIO),
            Err(Errno::EAGAIN) => {
                let _ = sys::sched_yield();
            }
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// Read `fd` to EOF into a `Vec`. Used by `tail` and the whole-file readers.
///
/// `try_reserve` turns memory exhaustion into `ENOMEM` instead of an abort.
pub fn read_all(fd: i32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = read_retry(fd, &mut buf)?;
        if n == 0 {
            return Ok(out);
        }
        if n > buf.len() {
            return Err(Errno::EIO);
        }
        out.try_reserve(n).map_err(|_| Errno::ENOMEM)?;
        out.extend_from_slice(&buf[..n]);
    }
}

/// An input operand: a file, or stdin for the conventional `-`.
///
/// The `File` is owned (and closed on drop); stdin is not closed.
pub struct Input {
    file: Option<File>,
}

impl Input {
    /// Open an operand; `"-"` selects fd 0.
    pub fn open(name: &str) -> Result<Input> {
        if name == "-" {
            Ok(Input { file: None })
        } else {
            Ok(Input {
                file: Some(File::open(name)?),
            })
        }
    }

    /// The descriptor to read from.
    pub fn fd(&self) -> i32 {
        match &self.file {
            Some(file) => file.as_raw_fd(),
            None => 0,
        }
    }
}

/// The name shown in headers for an operand.
pub fn display_name(name: &str) -> &str {
    if name == "-" {
        "standard input"
    } else {
        name
    }
}

/// Read a whole operand into memory (`-` is stdin).
///
/// The line-oriented applets use this instead of a hand-written reader: the
/// input has to be sorted/grouped/cut anyway, and [`read_all`] already retries
/// `EAGAIN` and uses `try_reserve`.
pub fn read_input(name: &str) -> Result<Vec<u8>> {
    let input = Input::open(name)?;
    read_all(input.fd())
}

/// Write `line` and a terminating newline to `fd`.
///
/// Output errors are returned rather than ignored, so an applet can print the
/// conventional `program: write error: ...` diagnostic and exit 1.
pub fn write_line(fd: i32, line: &[u8]) -> Result<()> {
    write_all(fd, line)?;
    write_all(fd, b"\n")
}

/// Split a byte buffer into lines, without the trailing `\n`.
///
/// The rules are the usual text-tool ones:
///
/// * `"a\nb\n"` yields `"a"`, `"b"`;
/// * `"a\nb"` yields `"a"`, `"b"` (the last line needs no newline);
/// * `"\n"` yields one empty line; `""` yields no lines.
///
/// Splitting is on ASCII LF only; a CR, if present, stays in the line. The
/// iterator borrows the buffer, so a whole file costs one allocation plus the
/// line offsets the caller stores.
pub fn lines(data: &[u8]) -> Lines<'_> {
    Lines {
        data,
        start: 0,
        done: false,
    }
}

/// Iterator returned by [`lines`].
pub struct Lines<'a> {
    data: &'a [u8],
    start: usize,
    done: bool,
}

impl<'a> Iterator for Lines<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        if self.done || self.start >= self.data.len() {
            self.done = true;
            return None;
        }
        let rest = &self.data[self.start..];
        match rest.iter().position(|byte| *byte == b'\n') {
            Some(offset) => {
                self.start += offset + 1;
                Some(&rest[..offset])
            }
            None => {
                self.done = true;
                Some(rest)
            }
        }
    }
}

/// A human-readable name for an ABI [`EntryKind`], as used by `stat`.
pub fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "file",
        EntryKind::Dir => "directory",
        EntryKind::Symlink => "symlink",
        EntryKind::Other => "other",
    }
}

/// Print the conventional `program: operand: error` line to fd 2.
pub fn report(program: &str, operand: &str, err: Errno) {
    libpagh::eprintln!("{program}: {operand}: {err}");
}

/// Parse a signed decimal integer without panicking.
pub fn parse_i64(text: &str) -> Option<i64> {
    text.parse::<i64>().ok()
}

/// Parse an unsigned decimal integer without panicking.
pub fn parse_usize(text: &str) -> Option<usize> {
    text.parse::<usize>().ok()
}

/// Format a number into a decimal digit count (for `wc`'s column width).
pub fn digits(mut value: u64) -> usize {
    let mut width = 1;
    while value >= 10 {
        value /= 10;
        width += 1;
    }
    width
}