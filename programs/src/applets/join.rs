//! `join` — join lines of two sorted files on a common field.
//!
//! Usage: `join [-1 N] [-2 N] [-t CHAR] FILE1 FILE2`. The key is field 1 in
//! both files unless `-1`/`-2` select another 1-based field. Fields are runs
//! of ASCII whitespace (the default) or the single literal byte given with
//! `-t`. Output is the key followed by the remaining fields of the first line
//! and then of the second, separated by a space or by `-t`'s byte:
//!
//! ```text
//! $ join a b
//! a 1 A
//! ```
//!
//! Both inputs must be sorted by key in byte order; lines with equal keys are
//! paired (a cartesian product for duplicate keys), and lines without a match
//! are dropped. This is the basic join subset: no `-a`, `-v`, `-o`, `-e`,
//! `-i`, `-j` or newline separator. There is no `getline`; each file is read
//! whole, and `-` means stdin.
//!
//! Exit: 0 success, 1 open/read/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: join [-1 N] [-2 N] [-t CHAR] FILE1 FILE2";

pub fn run(args: &[String]) -> i32 {
    let mut key1 = 1usize;
    let mut key2 = 1usize;
    let mut separator: Option<u8> = None;
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
        if parse_options && (arg == "-1" || arg == "-2" || arg == "-t") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match arg {
                "-1" => match parse_field(value) {
                    Some(field) => key1 = field,
                    None => {
                        eprintln!("join: invalid field number: '{value}'");
                        return util::EXIT_USAGE;
                    }
                },
                "-2" => match parse_field(value) {
                    Some(field) => key2 = field,
                    None => {
                        eprintln!("join: invalid field number: '{value}'");
                        return util::EXIT_USAGE;
                    }
                },
                _ => match parse_separator(value) {
                    Some(byte) => separator = Some(byte),
                    None => {
                        eprintln!("join: separator must be one byte");
                        return util::EXIT_USAGE;
                    }
                },
            }
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("-1") {
                if !value.is_empty() {
                    match parse_field(value) {
                        Some(field) => {
                            key1 = field;
                            continue;
                        }
                        None => {
                            eprintln!("join: invalid field number: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                }
            }
            if let Some(value) = arg.strip_prefix("-2") {
                if !value.is_empty() {
                    match parse_field(value) {
                        Some(field) => {
                            key2 = field;
                            continue;
                        }
                        None => {
                            eprintln!("join: invalid field number: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                }
            }
            if let Some(value) = arg.strip_prefix("-t") {
                if !value.is_empty() {
                    match parse_separator(value) {
                        Some(byte) => {
                            separator = Some(byte);
                            continue;
                        }
                        None => {
                            eprintln!("join: separator must be one byte");
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
            eprintln!("join: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.len() != 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let first_name = operands[0];
    let second_name = operands[1];

    let first_data = match util::read_input(first_name) {
        Ok(data) => data,
        Err(err) => {
            util::report("join", util::display_name(first_name), err);
            return util::EXIT_FAILURE;
        }
    };
    let second_data = match util::read_input(second_name) {
        Ok(data) => data,
        Err(err) => {
            util::report("join", util::display_name(second_name), err);
            return util::EXIT_FAILURE;
        }
    };

    let first: Vec<Vec<&[u8]>> = util::lines(&first_data)
        .map(|line| fields(line, separator))
        .collect();
    let second: Vec<Vec<&[u8]>> = util::lines(&second_data)
        .map(|line| fields(line, separator))
        .collect();

    let separator_byte = separator.unwrap_or(b' ');
    let mut i = 0usize;
    let mut j = 0usize;
    while i < first.len() && j < second.len() {
        let a_key = key(&first[i], key1);
        let b_key = key(&second[j], key2);
        match a_key.cmp(b_key) {
            core::cmp::Ordering::Less => i += 1,
            core::cmp::Ordering::Greater => j += 1,
            core::cmp::Ordering::Equal => {
                let i_end = run_end(&first, i, key1);
                let j_end = run_end(&second, j, key2);
                for a_fields in &first[i..i_end] {
                    for b_fields in &second[j..j_end] {
                        if let Err(err) = emit(a_fields, b_fields, key1, key2, separator_byte) {
                            eprintln!("join: write error: {err}");
                            return util::EXIT_FAILURE;
                        }
                    }
                }
                i = i_end;
                j = j_end;
            }
        }
    }
    util::EXIT_OK
}

fn parse_field(text: &str) -> Option<usize> {
    let field = util::parse_usize(text)?;
    if field == 0 {
        None
    } else {
        Some(field)
    }
}

fn parse_separator(text: &str) -> Option<u8> {
    let bytes = text.as_bytes();
    if bytes.len() == 1 {
        Some(bytes[0])
    } else {
        None
    }
}

fn fields(line: &[u8], separator: Option<u8>) -> Vec<&[u8]> {
    match separator {
        Some(sep) => line.split(|byte| *byte == sep).collect(),
        None => line
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|field| !field.is_empty())
            .collect(),
    }
}

fn key<'a>(fields: &'a [&'a [u8]], number: usize) -> &'a [u8] {
    fields.get(number - 1).copied().unwrap_or(b"")
}

fn run_end(rows: &[Vec<&[u8]>], start: usize, key_number: usize) -> usize {
    let wanted = key(&rows[start], key_number);
    let mut end = start + 1;
    while end < rows.len() && key(&rows[end], key_number) == wanted {
        end += 1;
    }
    end
}

fn emit(
    first: &[&[u8]],
    second: &[&[u8]],
    key1: usize,
    key2: usize,
    separator: u8,
) -> libpagh::Result<()> {
    let key_field = key(first, key1);
    util::write_all(1, key_field)?;
    // GNU join prints every non-key field, including fields that precede the
    // key (the -1/-2 key need not be the first column).
    for (position, field) in first.iter().enumerate() {
        if position == key1 - 1 {
            continue;
        }
        util::write_all(1, &[separator])?;
        util::write_all(1, field)?;
    }
    for (position, field) in second.iter().enumerate() {
        if position == key2 - 1 {
            continue;
        }
        util::write_all(1, &[separator])?;
        util::write_all(1, field)?;
    }
    util::write_all(1, b"\n")
}
