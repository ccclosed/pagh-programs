//! `cut` — select fields from each line.
//!
//! `-f LIST` is required; LIST is a comma-separated list of 1-based field
//! numbers and ranges: `1`, `3-5`, `2,4-6`, `4-` (to the end), `-3` (from the
//! start). `-d CHAR` sets a one-byte delimiter (default tab). Fields are output
//! in file order, joined with the same delimiter, one line per input line.
//!
//! A line without the delimiter is treated as one field: it is printed whole
//! when field 1 is selected and as an empty line otherwise (GNU behaviour for
//! the simple case). Input is byte-oriented; a multi-byte UTF-8 delimiter is
//! rejected because the ABI's files are byte streams and there is no character
//! unit here.
//!
//! Several operands are read in order; no operand means stdin. No `-c`, `-b`,
//! `-s`, `--complement` or output delimiter. Pipes/redirection do not exist in
//! the kernel shell.
//!
//! Exit: 0 success, 1 open/read/write/memory error, 2 usage error.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: cut -d CHAR -f LIST [FILE...]";

pub fn run(args: &[String]) -> i32 {
    let mut delimiter = b'\t';
    let mut ranges: Option<Vec<(usize, usize)>> = None;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        if parse_options && arg == "--" {
            parse_options = false;
            index += 1;
            continue;
        }
        if parse_options && arg == "-d" {
            index += 1;
            let Some(value) = args.get(index) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            match parse_delimiter(value) {
                Some(parsed) => delimiter = parsed,
                None => {
                    eprintln!("cut: invalid delimiter: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-d") {
            match parse_delimiter(&arg[2..]) {
                Some(parsed) => delimiter = parsed,
                None => {
                    eprintln!("cut: invalid delimiter: '{}'", &arg[2..]);
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg == "-f" {
            index += 1;
            let Some(value) = args.get(index) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            match parse_ranges(value) {
                Some(parsed) => ranges = Some(parsed),
                None => {
                    eprintln!("cut: invalid field list: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-f") {
            match parse_ranges(&arg[2..]) {
                Some(parsed) => ranges = Some(parsed),
                None => {
                    eprintln!("cut: invalid field list: '{}'", &arg[2..]);
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        operands.push(arg.as_str());
        index += 1;
    }

    let Some(ranges) = ranges else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };

    if operands.is_empty() {
        operands.push("-");
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("cut", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        for line in util::lines(&data) {
            let mut out: Vec<u8> = Vec::new();
            if out.try_reserve(line.len()).is_err() {
                eprintln!("cut: out of memory");
                return util::EXIT_FAILURE;
            }
            if !line.contains(&delimiter) {
                if is_selected(&ranges, 1) {
                    out.extend_from_slice(line);
                }
            } else {
                let mut field = 1usize;
                let mut start = 0usize;
                let mut first = true;
                loop {
                    let end = match line[start..].iter().position(|byte| *byte == delimiter) {
                        Some(offset) => start + offset,
                        None => line.len(),
                    };
                    if is_selected(&ranges, field) {
                        if !first {
                            out.push(delimiter);
                        }
                        out.extend_from_slice(&line[start..end]);
                        first = false;
                    }
                    field = field.saturating_add(1);
                    if end == line.len() {
                        break;
                    }
                    start = end + 1;
                }
            }
            if let Err(err) = util::write_line(1, &out) {
                eprintln!("cut: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
    }
    status
}

fn parse_delimiter(text: &str) -> Option<u8> {
    match text.as_bytes() {
        [byte] => Some(*byte),
        _ => None,
    }
}

fn parse_ranges(text: &str) -> Option<Vec<(usize, usize)>> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for part in text.split(',') {
        if part.is_empty() {
            return None;
        }
        let (start, end) = match part.split_once('-') {
            Some((start, end)) => {
                let start = if start.is_empty() {
                    1
                } else {
                    parse_field(start)?
                };
                let end = if end.is_empty() {
                    usize::MAX
                } else {
                    parse_field(end)?
                };
                (start, end)
            }
            None => {
                let field = parse_field(part)?;
                (field, field)
            }
        };
        if start > end {
            return None;
        }
        ranges.try_reserve(1).ok()?;
        ranges.push((start, end));
    }
    Some(ranges)
}

fn parse_field(text: &str) -> Option<usize> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    match util::parse_usize(text) {
        Some(0) | None => None,
        Some(field) => Some(field),
    }
}

fn is_selected(ranges: &[(usize, usize)], field: usize) -> bool {
    ranges
        .iter()
        .any(|(start, end)| field >= *start && field <= *end)
}