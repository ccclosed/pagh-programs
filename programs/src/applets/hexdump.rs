//! `hexdump` — a minimal canonical hex+ASCII dump.
//!
//! Each line shows a 16-byte group: an eight-digit offset, the bytes as two
//! hex digits separated by spaces (with the usual extra gap after the eighth
//! byte), then the same bytes as ASCII in `|...|`. Printable bytes are copied;
//! everything else becomes `.`. Example:
//!
//! ```text
//! 00000000  68 65 6c 6c 6f 0a                           |hello.|
//! ```
//!
//! `-C` is accepted for compatibility; it is the only format implemented and is
//! also the default. There is no final offset-only line, no `-n`, no `-s`, no
//! endianness mode, and no octal dump (`od` is a separate catalog entry).
//! Several operands are read as one continuous stream, so lines and offsets
//! cross file boundaries; no operand means stdin.
//!
//! Exit: 0 success, 1 open/read/write error, 2 usage error.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use libpagh::eprintln;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: hexdump [-C] [FILE...]";

struct Dumper {
    pending: [u8; 16],
    pending_len: usize,
    offset: u64,
}

impl Dumper {
    fn new() -> Dumper {
        Dumper {
            pending: [0; 16],
            pending_len: 0,
            offset: 0,
        }
    }

    fn feed(&mut self, data: &[u8]) -> Result<(), Errno> {
        for byte in data {
            self.pending[self.pending_len] = *byte;
            self.pending_len += 1;
            if self.pending_len == self.pending.len() {
                self.emit()?;
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Errno> {
        self.emit()
    }

    fn emit(&mut self) -> Result<(), Errno> {
        if self.pending_len == 0 {
            return Ok(());
        }

        let mut line = String::with_capacity(80);
        let _ = write!(line, "{:08x}  ", self.offset);
        for index in 0..self.pending.len() {
            if index < self.pending_len {
                let _ = write!(line, "{:02x} ", self.pending[index]);
            } else {
                line.push_str("   ");
            }
            if index == 7 {
                line.push(' ');
            }
        }
        line.push_str(" |");
        for index in 0..self.pending_len {
            let byte = self.pending[index];
            line.push(if (0x20..=0x7e).contains(&byte) {
                char::from(byte)
            } else {
                '.'
            });
        }
        line.push('|');

        self.offset = self.offset.saturating_add(self.pending_len as u64);
        self.pending_len = 0;
        util::write_line(1, line.as_bytes())
    }
}

pub fn run(args: &[String]) -> i32 {
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            if arg != "-C" {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            }
            continue;
        }
        operands.push(arg.as_str());
    }

    if operands.is_empty() {
        operands.push("-");
    }

    let mut dumper = Dumper::new();
    let mut status = util::EXIT_OK;
    let mut buffer = [0u8; 8192];

    for name in operands {
        let input = match util::Input::open(name) {
            Ok(input) => input,
            Err(err) => {
                util::report("hexdump", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        loop {
            match util::read_retry(input.fd(), &mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if let Err(err) = dumper.feed(&buffer[..read]) {
                        eprintln!("hexdump: write error: {err}");
                        return util::EXIT_FAILURE;
                    }
                }
                Err(err) => {
                    util::report("hexdump", util::display_name(name), err);
                    status = util::EXIT_FAILURE;
                    break;
                }
            }
        }
    }

    if let Err(err) = dumper.finish() {
        eprintln!("hexdump: write error: {err}");
        return util::EXIT_FAILURE;
    }
    status
}