//! `od` — dump bytes in hexadecimal, octal or C-escape form.
//!
//! Usage: `od [-A x|o|d] [-t x1|o1|c] [FILE...]`. Each line carries sixteen
//! bytes and starts with the byte offset in the selected radix (`o` is the
//! default, as in GNU od; `x` uses six hex digits and `d` seven decimal
//! digits). The type is `x1` (two hex digits), `o1` (three octal digits) or
//! `c` (C escapes: `\n`, `\t`, `\0`, `\a`… and three-digit octal for other
//! non-printing bytes) and defaults to `x1` here — GNU's default `o2` (two-byte
//! octal words) is not implemented.
//!
//! A final offset-only line is printed, so the output ends with the number of
//! bytes consumed. Several operands are one continuous stream (offsets cross
//! file boundaries); no operand means stdin. There is no `*` duplicate-line
//! compression, no endianness selection and no `-N`/`-j`/`-v`.
//!
//! Exit: 0 success, 1 open/read/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: od [-A x|o|d] [-t x1|o1|c] [FILE...]";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Radix {
    Octal,
    Hex,
    Decimal,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    X1,
    O1,
    C,
}

struct Dumper {
    pending: [u8; 16],
    pending_len: usize,
    offset: u64,
    radix: Radix,
    kind: Kind,
}

impl Dumper {
    fn new(radix: Radix, kind: Kind) -> Dumper {
        Dumper {
            pending: [0; 16],
            pending_len: 0,
            offset: 0,
            radix,
            kind,
        }
    }

    fn feed(&mut self, data: &[u8]) -> libpagh::Result<()> {
        for byte in data {
            self.pending[self.pending_len] = *byte;
            self.pending_len += 1;
            if self.pending_len == self.pending.len() {
                self.emit()?;
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> libpagh::Result<()> {
        self.emit()?;
        let mut line = String::new();
        self.push_offset(&mut line);
        util::write_line(1, line.as_bytes())
    }

    fn emit(&mut self) -> libpagh::Result<()> {
        if self.pending_len == 0 {
            return Ok(());
        }
        let mut line = String::with_capacity(80);
        self.push_offset(&mut line);
        match self.kind {
            Kind::X1 => {
                for index in 0..self.pending_len {
                    let _ = write!(line, " {:02x}", self.pending[index]);
                }
            }
            Kind::O1 => {
                for index in 0..self.pending_len {
                    let _ = write!(line, " {:03o}", self.pending[index]);
                }
            }
            Kind::C => {
                for index in 0..self.pending_len {
                    push_c(&mut line, self.pending[index]);
                }
            }
        }
        let consumed = self.pending_len as u64;
        self.pending_len = 0;
        self.offset = self.offset.saturating_add(consumed);
        util::write_line(1, line.as_bytes())
    }

    fn push_offset(&self, line: &mut String) {
        match self.radix {
            Radix::Octal => {
                let _ = write!(line, "{:07o}", self.offset);
            }
            Radix::Hex => {
                let _ = write!(line, "{:06x}", self.offset);
            }
            Radix::Decimal => {
                let _ = write!(line, "{:07}", self.offset);
            }
        }
    }
}

fn push_c(line: &mut String, byte: u8) {
    match byte {
        0x00 => line.push_str("  \\0"),
        0x07 => line.push_str("  \\a"),
        0x08 => line.push_str("  \\b"),
        b'\t' => line.push_str("  \\t"),
        b'\n' => line.push_str("  \\n"),
        0x0b => line.push_str("  \\v"),
        0x0c => line.push_str("  \\f"),
        b'\r' => line.push_str("  \\r"),
        0x20..=0x7e => {
            line.push_str("   ");
            line.push(char::from(byte));
        }
        _ => {
            let _ = write!(line, " {:03o}", byte);
        }
    }
}

pub fn run(args: &[String]) -> i32 {
    let mut radix = Radix::Octal;
    let mut kind = Kind::X1;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && (arg == "-A" || arg == "-t") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match arg {
                "-A" => match parse_radix(value) {
                    Some(parsed) => radix = parsed,
                    None => {
                        eprintln!("od: invalid offset radix: '{value}'");
                        return util::EXIT_USAGE;
                    }
                },
                _ => match parse_kind(value) {
                    Some(parsed) => kind = parsed,
                    None => {
                        eprintln!("od: invalid type: '{value}'");
                        return util::EXIT_USAGE;
                    }
                },
            }
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("-A") {
                if !value.is_empty() {
                    match parse_radix(value) {
                        Some(parsed) => {
                            radix = parsed;
                            continue;
                        }
                        None => {
                            eprintln!("od: invalid offset radix: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                }
            }
            if let Some(value) = arg.strip_prefix("-t") {
                if !value.is_empty() {
                    match parse_kind(value) {
                        Some(parsed) => {
                            kind = parsed;
                            continue;
                        }
                        None => {
                            eprintln!("od: invalid type: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("od: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        operands.push("-");
    }

    let mut dumper = Dumper::new(radix, kind);
    let mut status = util::EXIT_OK;
    let mut buffer = [0u8; 8192];
    for name in operands {
        let input = match util::Input::open(name) {
            Ok(input) => input,
            Err(err) => {
                util::report("od", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        loop {
            match util::read_retry(input.fd(), &mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if let Err(err) = dumper.feed(&buffer[..read]) {
                        eprintln!("od: write error: {err}");
                        return util::EXIT_FAILURE;
                    }
                }
                Err(err) => {
                    util::report("od", util::display_name(name), err);
                    status = util::EXIT_FAILURE;
                    break;
                }
            }
        }
    }

    if let Err(err) = dumper.finish() {
        eprintln!("od: write error: {err}");
        return util::EXIT_FAILURE;
    }
    status
}

fn parse_radix(text: &str) -> Option<Radix> {
    match text {
        "o" => Some(Radix::Octal),
        "x" => Some(Radix::Hex),
        "d" => Some(Radix::Decimal),
        _ => None,
    }
}

fn parse_kind(text: &str) -> Option<Kind> {
    match text {
        "x1" => Some(Kind::X1),
        "o1" => Some(Kind::O1),
        "c" => Some(Kind::C),
        _ => None,
    }
}
