//! `split` — split a file into pieces.
//!
//! Usage: `split [-l N] [-b N] [-d] [FILE [PREFIX]]`. The default mode is
//! 1000 lines per piece; `-l N` selects another line count and `-b N` selects
//! a byte count. The default prefix is `x`, with two-letter suffixes
//! (`xaa`, `xab`, ...); `-d` uses two-digit numeric suffixes (`x00`, `x01`,
//! ...). `FILE` is `-` for stdin when omitted.
//!
//! Empty input produces no pieces (GNU behaviour). At most 676 alphabetic or
//! 100 numeric pieces fit in the two-character suffix, so a split that would
//! need more is reported as an error instead of silently overwriting a piece.
//! Splitting at exact line boundaries preserves the input bytes, including the
//! absence of a final newline only in the final piece (`-l` mode).
//!
//! Exit: 0 success, 1 read/open/write or suffix-overflow error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::File;

use crate::util;

const USAGE: &str = "usage: split [-l N] [-b N] [-d] [FILE [PREFIX]]";
const ALPHA_LIMIT: usize = 26 * 26;
const NUMERIC_LIMIT: usize = 100;

#[derive(Clone, Copy)]
enum Mode {
    Lines(usize),
    Bytes(usize),
}

pub fn run(args: &[String]) -> i32 {
    let mut mode: Option<Mode> = None;
    let mut numeric = false;
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
        if parse_options && arg == "-d" {
            numeric = true;
            continue;
        }
        if parse_options && (arg == "-l" || arg == "-b") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match parse_size(value) {
                Some(parsed) => {
                    let selected = if arg == "-l" {
                        Mode::Lines(parsed)
                    } else {
                        Mode::Bytes(parsed)
                    };
                    if mode.is_some() {
                        eprintln!("split: only one of -l and -b may be given");
                        return util::EXIT_USAGE;
                    }
                    mode = Some(selected);
                }
                None => {
                    eprintln!("split: invalid number: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("-l") {
                if !value.is_empty() {
                    match parse_size(value) {
                        Some(parsed) => {
                            if mode.is_some() {
                                eprintln!("split: only one of -l and -b may be given");
                                return util::EXIT_USAGE;
                            }
                            mode = Some(Mode::Lines(parsed));
                        }
                        None => {
                            eprintln!("split: invalid number: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                    continue;
                }
            }
            if let Some(value) = arg.strip_prefix("-b") {
                if !value.is_empty() {
                    match parse_size(value) {
                        Some(parsed) => {
                            if mode.is_some() {
                                eprintln!("split: only one of -l and -b may be given");
                                return util::EXIT_USAGE;
                            }
                            mode = Some(Mode::Bytes(parsed));
                        }
                        None => {
                            eprintln!("split: invalid number: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                    continue;
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("split: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.len() > 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let source = operands.first().copied().unwrap_or("-");
    let prefix = operands.get(1).copied().unwrap_or("x");
    let mode = mode.unwrap_or(Mode::Lines(1000));

    let data = match util::read_input(source) {
        Ok(data) => data,
        Err(err) => {
            util::report("split", util::display_name(source), err);
            return util::EXIT_FAILURE;
        }
    };
    if data.is_empty() {
        return util::EXIT_OK;
    }

    let pieces: Vec<(usize, usize)> = match mode {
        Mode::Lines(count) => line_pieces(&data, count),
        Mode::Bytes(count) => byte_pieces(data.len(), count),
    };
    let limit = if numeric { NUMERIC_LIMIT } else { ALPHA_LIMIT };
    if pieces.len() > limit {
        eprintln!(
            "split: '{prefix}': too many pieces ({} > {limit})",
            pieces.len()
        );
        return util::EXIT_FAILURE;
    }

    for (number, (start, end)) in pieces.into_iter().enumerate() {
        let name = piece_name(prefix, number, numeric);
        let file = match File::create(&name) {
            Ok(file) => file,
            Err(err) => {
                util::report("split", &name, err);
                return util::EXIT_FAILURE;
            }
        };
        if let Err(err) = util::write_all(file.as_raw_fd(), &data[start..end]) {
            util::report("split", &name, err);
            return util::EXIT_FAILURE;
        }
        // The descriptor is closed by `Drop`; a close error has no recovery.
    }
    util::EXIT_OK
}

fn parse_size(text: &str) -> Option<usize> {
    let value = util::parse_usize(text)?;
    if value == 0 {
        None
    } else {
        Some(value)
    }
}

fn line_pieces(data: &[u8], per_piece: usize) -> Vec<(usize, usize)> {
    let mut pieces = Vec::new();
    let mut start = 0usize;
    let mut lines = 0usize;
    for (offset, byte) in data.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        lines += 1;
        if lines == per_piece {
            pieces.push((start, offset + 1));
            start = offset + 1;
            lines = 0;
        }
    }
    if start < data.len() {
        pieces.push((start, data.len()));
    }
    pieces
}

fn byte_pieces(length: usize, per_piece: usize) -> Vec<(usize, usize)> {
    let mut pieces = Vec::new();
    let mut start = 0usize;
    while start < length {
        let end = start.saturating_add(per_piece).min(length);
        pieces.push((start, end));
        start = end;
    }
    pieces
}

fn piece_name(prefix: &str, number: usize, numeric: bool) -> String {
    let mut name = String::from(prefix);
    if numeric {
        name.push(char::from(b'0' + (number / 10) as u8));
        name.push(char::from(b'0' + (number % 10) as u8));
    } else {
        name.push(char::from(b'a' + (number / 26) as u8));
        name.push(char::from(b'a' + (number % 26) as u8));
    }
    name
}
