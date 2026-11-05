//! `csplit` — split a file at line numbers or regular-expression matches.
//!
//! Usage: `csplit [-s] [-f PREFIX] [-n N] FILE PATTERN...`. Supported
//! patterns are the two basic GNU forms:
//!
//! * `N` — start a new piece before 1-based line `N`;
//! * `/RE/` — start a new piece before the next line matching the
//!   `bre`-subset regular expression (`^ $ . * [] [^] \`).
//!
//! Every pattern creates one output piece, and the remaining lines become the
//! final piece. `-f` sets the prefix (default `xx`), `-n` the numeric suffix
//! width (default 2, accepted range 1..=9), and `-s` suppresses the byte count
//! that is otherwise printed for each piece. Piece `i` is named
//! `PREFIX` + `i` zero-padded to `N` digits; existing files are truncated.
//!
//! Not implemented: `%RE%`, offsets (`/RE/+N`), the repeat suffix `{N}`, and
//! regex alternation/groups — this is the deliberately basic subset named in
//! the catalog. A pattern that matches no line is a data error (exit 1); a
//! malformed pattern is a usage error (exit 2).
//!
//! Exit: 0 success, 1 read/write/no-match/line-range error, 2 usage.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::File;

use super::bre::Pattern;
use crate::util;

const USAGE: &str = "usage: csplit [-s] [-f PREFIX] [-n N] FILE PATTERN...";

enum Split {
    Line(usize),
    Regex { source: String, pattern: Pattern },
}

pub fn run(args: &[String]) -> i32 {
    let mut quiet = false;
    let mut prefix: &str = "xx";
    let mut digits = 2usize;
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
        if parse_options && (arg == "-s" || arg == "--quiet" || arg == "--silent") {
            quiet = true;
            continue;
        }
        if parse_options && arg == "-f" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            prefix = value;
            continue;
        }
        if parse_options && arg == "-n" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match parse_width(value) {
                Some(parsed) => digits = parsed,
                None => {
                    eprintln!("csplit: invalid suffix width: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("-f") {
                if !value.is_empty() {
                    prefix = value;
                    continue;
                }
            }
            if let Some(value) = arg.strip_prefix("-n") {
                if !value.is_empty() {
                    match parse_width(value) {
                        Some(parsed) => digits = parsed,
                        None => {
                            eprintln!("csplit: invalid suffix width: '{value}'");
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
            eprintln!("csplit: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if prefix.is_empty() || operands.len() < 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let source = operands[0];
    let pattern_args = &operands[1..];

    let mut patterns: Vec<Split> = Vec::new();
    for text in pattern_args {
        match parse_pattern(text) {
            Ok(pattern) => {
                if util::push(&mut patterns, pattern).is_err() {
                    eprintln!("csplit: out of memory");
                    return util::EXIT_FAILURE;
                }
            }
            Err(message) => {
                eprintln!("csplit: {message}");
                return util::EXIT_USAGE;
            }
        }
    }

    let data = match util::read_input(source) {
        Ok(data) => data,
        Err(err) => {
            util::report("csplit", util::display_name(source), err);
            return util::EXIT_FAILURE;
        }
    };
    let lines: Vec<&[u8]> = util::lines(&data).collect();
    let ends_with_newline = data.ends_with(b"\n");

    let mut cursor = 0usize;
    let mut piece = 0usize;
    for split in &patterns {
        let target = match split {
            Split::Line(number) => {
                let target = *number - 1;
                if target < cursor || target > lines.len() {
                    eprintln!("csplit: {number}: line number out of range");
                    return util::EXIT_FAILURE;
                }
                target
            }
            Split::Regex { source, pattern } => {
                let mut found = None;
                for (line_number, line) in lines.iter().enumerate().skip(cursor) {
                    if pattern.is_match(line) {
                        found = Some(line_number);
                        break;
                    }
                }
                match found {
                    Some(target) => target,
                    None => {
                        eprintln!("csplit: '{source}': match not found");
                        return util::EXIT_FAILURE;
                    }
                }
            }
        };

        if let Err(code) = emit_piece(
            prefix,
            digits,
            piece,
            &lines,
            cursor,
            target,
            lines.len(),
            ends_with_newline,
            quiet,
        ) {
            return code;
        }
        cursor = target;
        piece += 1;
    }

    if let Err(code) = emit_piece(
        prefix,
        digits,
        piece,
        &lines,
        cursor,
        lines.len(),
        lines.len(),
        ends_with_newline,
        quiet,
    ) {
        return code;
    }
    util::EXIT_OK
}

#[allow(clippy::too_many_arguments)]
fn emit_piece(
    prefix: &str,
    digits: usize,
    number: usize,
    lines: &[&[u8]],
    start: usize,
    end: usize,
    total: usize,
    ends_with_newline: bool,
    quiet: bool,
) -> Result<(), i32> {
    let name = piece_name(prefix, digits, number);
    let mut contents: Vec<u8> = Vec::new();
    for (offset, line) in lines[start..end].iter().enumerate() {
        let absolute = start + offset;
        if contents.try_reserve(line.len() + 1).is_err() {
            eprintln!("csplit: out of memory");
            return Err(util::EXIT_FAILURE);
        }
        contents.extend_from_slice(line);
        if absolute + 1 < total || ends_with_newline {
            contents.push(b'\n');
        }
    }

    let file = match File::create(&name) {
        Ok(file) => file,
        Err(err) => {
            util::report("csplit", &name, err);
            return Err(util::EXIT_FAILURE);
        }
    };
    if let Err(err) = util::write_all(file.as_raw_fd(), &contents) {
        util::report("csplit", &name, err);
        return Err(util::EXIT_FAILURE);
    }

    if !quiet {
        let line = format!("{}\n", contents.len());
        if let Err(err) = util::write_all(1, line.as_bytes()) {
            eprintln!("csplit: write error: {err}");
            return Err(util::EXIT_FAILURE);
        }
    }
    Ok(())
}

fn piece_name(prefix: &str, digits: usize, number: usize) -> String {
    let decimal = format!("{number}");
    let mut name = String::from(prefix);
    for _ in decimal.len()..digits {
        name.push('0');
    }
    name.push_str(&decimal);
    name
}

fn parse_width(text: &str) -> Option<usize> {
    let width = util::parse_usize(text)?;
    if (1..=9).contains(&width) {
        Some(width)
    } else {
        None
    }
}

fn parse_pattern(text: &str) -> Result<Split, String> {
    if text.starts_with('/') {
        if !text.ends_with('/') || text.len() < 2 {
            return Err(format!("invalid pattern: '{text}'"));
        }
        let inner = &text[1..text.len() - 1];
        let pattern = Pattern::compile(inner.as_bytes())
            .ok_or_else(|| format!("invalid regular expression: '{inner}'"))?;
        return Ok(Split::Regex {
            source: String::from(text),
            pattern,
        });
    }
    let number = util::parse_usize(text)
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("invalid pattern: '{text}'"))?;
    Ok(Split::Line(number))
}
