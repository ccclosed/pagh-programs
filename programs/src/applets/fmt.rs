//! `fmt` — reflow text paragraphs to a target width.
//!
//! Usage: `fmt [-s] [-w N] [FILE...]`. Each paragraph (a run of non-blank
//! lines) is joined into words and wrapped at `N` bytes (default 75), with a
//! single space between words. Blank lines are preserved. `-s` is GNU's
//! split-only mode: line breaks are kept and only lines longer than the width
//! are split. Without `-s`, short lines are joined into paragraphs.
//!
//! This is the basic text-reflow subset: no sentence-spacing heuristic, no
//! indentation preservation for reflowed text (a short line kept by `-s` is
//! passed through unchanged), no `-u`/`-p`/`-g`, and width is counted in
//! bytes, not display columns. A word longer than the width is emitted on its
//! own longer line rather than split.
//!
//! Several operands are formatted one after another; `-` is stdin. Exit: 0
//! success, 1 open/read/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: fmt [-s] [-w N] [FILE...]";

pub fn run(args: &[String]) -> i32 {
    let mut split_only = false;
    let mut width = 75usize;
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
        if parse_options && arg == "-s" {
            split_only = true;
            continue;
        }
        if parse_options && arg == "-w" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match parse_width(value) {
                Some(parsed) => width = parsed,
                None => {
                    eprintln!("fmt: invalid width: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("-w") {
                if !value.is_empty() {
                    match parse_width(value) {
                        Some(parsed) => {
                            width = parsed;
                            continue;
                        }
                        None => {
                            eprintln!("fmt: invalid width: '{value}'");
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
            eprintln!("fmt: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        operands.push("-");
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("fmt", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        if let Err(err) = format_data(&data, width, split_only) {
            eprintln!("fmt: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    status
}

fn parse_width(text: &str) -> Option<usize> {
    let width = util::parse_usize(text)?;
    if width == 0 {
        None
    } else {
        Some(width)
    }
}

fn format_data(data: &[u8], width: usize, split_only: bool) -> libpagh::Result<()> {
    let lines: Vec<&[u8]> = util::lines(data).collect();
    let mut index = 0usize;
    while index < lines.len() {
        if is_blank(lines[index]) {
            util::write_all(1, b"\n")?;
            index += 1;
            continue;
        }
        let start = index;
        while index < lines.len() && !is_blank(lines[index]) {
            index += 1;
        }
        if split_only {
            for line in &lines[start..index] {
                if line.len() <= width {
                    util::write_all(1, line)?;
                    util::write_all(1, b"\n")?;
                } else {
                    wrap_words(&words(line), width)?;
                }
            }
        } else {
            let mut all_words: Vec<&[u8]> = Vec::new();
            for line in &lines[start..index] {
                for word in words(line) {
                    if util::push(&mut all_words, word).is_err() {
                        return Err(libpagh::Errno::ENOMEM);
                    }
                }
            }
            wrap_words(&all_words, width)?;
        }
    }
    Ok(())
}

fn is_blank(line: &[u8]) -> bool {
    line.iter().all(|byte| byte.is_ascii_whitespace())
}

fn words(line: &[u8]) -> Vec<&[u8]> {
    line.split(|byte| byte.is_ascii_whitespace())
        .filter(|word| !word.is_empty())
        .collect()
}

fn wrap_words(words: &[&[u8]], width: usize) -> libpagh::Result<()> {
    let mut column = 0usize;
    let mut line: Vec<u8> = Vec::new();
    for word in words {
        let extra = if line.is_empty() { 0 } else { 1 };
        if !line.is_empty() && column + extra + word.len() > width {
            util::write_all(1, &line)?;
            util::write_all(1, b"\n")?;
            line.clear();
            column = 0;
        }
        if !line.is_empty() {
            line.push(b' ');
            column += 1;
        }
        if line.try_reserve(word.len()).is_err() {
            return Err(libpagh::Errno::ENOMEM);
        }
        line.extend_from_slice(word);
        column += word.len();
    }
    if !line.is_empty() {
        util::write_all(1, &line)?;
        util::write_all(1, b"\n")?;
    }
    Ok(())
}
