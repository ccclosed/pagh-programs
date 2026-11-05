//! `comm` — compare two sorted files line by line.
//!
//! The classical three-column output is produced: lines unique to the first
//! file, lines unique to the second, and lines present in both. Columns are
//! separated by a tab; `-1`, `-2`, `-3` suppress the corresponding column.
//! Suppression removes that column's text but keeps the tab positions of the
//! columns that remain, which is what GNU `comm` does (`comm -1` still prints
//! one leading tab on a common line).
//!
//! Both inputs must be sorted in the same byte order; this applet merges, it
//! does not sort. Exactly two operands; `-` means stdin. There is no `--output`,
//! no `--total`, and no locale collation.
//!
//! Exit: 0 success, 1 open/read/write error (a missing file makes the
//! comparison impossible), 2 usage error.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: comm [-1] [-2] [-3] FILE1 FILE2";

pub fn run(args: &[String]) -> i32 {
    let mut suppress = [false; 3];
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            for option in arg[1..].chars() {
                match option {
                    '1' => suppress[0] = true,
                    '2' => suppress[1] = true,
                    '3' => suppress[2] = true,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        operands.push(arg.as_str());
    }

    if operands.len() != 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let first = match util::read_input(operands[0]) {
        Ok(data) => data,
        Err(err) => {
            util::report("comm", util::display_name(operands[0]), err);
            return util::EXIT_FAILURE;
        }
    };
    let second = match util::read_input(operands[1]) {
        Ok(data) => data,
        Err(err) => {
            util::report("comm", util::display_name(operands[1]), err);
            return util::EXIT_FAILURE;
        }
    };

    let mut left: Vec<&[u8]> = Vec::new();
    for line in util::lines(&first) {
        if left.try_reserve(1).is_err() {
            eprintln!("comm: out of memory");
            return util::EXIT_FAILURE;
        }
        left.push(line);
    }
    let mut right: Vec<&[u8]> = Vec::new();
    for line in util::lines(&second) {
        if right.try_reserve(1).is_err() {
            eprintln!("comm: out of memory");
            return util::EXIT_FAILURE;
        }
        right.push(line);
    }

    let mut i = 0usize;
    let mut j = 0usize;
    while i < left.len() || j < right.len() {
        let order = match (left.get(i), right.get(j)) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => core::cmp::Ordering::Less,
            (None, Some(_)) => core::cmp::Ordering::Greater,
            (None, None) => break,
        };
        let result = match order {
            core::cmp::Ordering::Less => {
                let result = emit(0, left[i], &suppress);
                i += 1;
                result
            }
            core::cmp::Ordering::Greater => {
                let result = emit(1, right[j], &suppress);
                j += 1;
                result
            }
            core::cmp::Ordering::Equal => {
                let result = emit(2, left[i], &suppress);
                i += 1;
                j += 1;
                result
            }
        };
        if let Err(err) = result {
            eprintln!("comm: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }

    util::EXIT_OK
}

fn emit(column: usize, line: &[u8], suppress: &[bool; 3]) -> Result<(), Errno> {
    if suppress[column] {
        return Ok(());
    }

    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(line.len() + 2).map_err(|_| Errno::ENOMEM)?;
    // The tabs before a column are positional. A line unique to FILE2 still
    // gets the tab that would have separated it from column 1, and a common
    // line gets one tab per preceding column that was not suppressed.
    if column >= 1 && !suppress[0] {
        out.push(b'\t');
    }
    if column >= 2 && !suppress[1] {
        out.push(b'\t');
    }
    out.extend_from_slice(line);
    util::write_line(1, &out)
}
