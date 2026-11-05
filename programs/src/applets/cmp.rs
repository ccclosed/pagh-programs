//! `cmp` — compare two files byte by byte.
//!
//! With no options the first difference is reported as
//! `FILE1 FILE2 differ: byte N, line L, offset 0xHH`, where byte and line are
//! 1-based and the offset is the zero-based hexadecimal byte position. `-l`
//! lists every differing byte as `N O1 O2` (byte number, then both values in
//! three-digit octal). `-s` suppresses all output and only sets the exit status.
//!
//! One operand is compared with stdin, so `cmp file` and `cmp - file` read the
//! same bytes; an explicit `-` is stdin. A shorter file produces
//! `cmp: EOF on NAME after byte N, line L` (suppressed by `-s`).
//!
//! Exit: 0 identical, 1 different or a read error, 2 usage. Unlike GNU cmp,
//! a read error is 1 (the assignment's "data error" rule), not 2.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: cmp [-s] [-l] FILE1 [FILE2]";

#[derive(Default)]
struct Options {
    silent: bool,
    list: bool,
}

pub fn run(args: &[String]) -> i32 {
    let mut options = Options::default();
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
                    's' => options.silent = true,
                    'l' => options.list = true,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        parse_options = false;
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("cmp: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() || operands.len() > 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let names: [&str; 2] = if operands.len() == 2 {
        [operands[0], operands[1]]
    } else {
        [operands[0], "-"]
    };

    let mut data: [Vec<u8>; 2] = [Vec::new(), Vec::new()];
    for slot in 0..2 {
        match util::read_input(names[slot]) {
            Ok(bytes) => data[slot] = bytes,
            Err(err) => {
                if !options.silent {
                    util::report("cmp", util::display_name(names[slot]), err);
                }
                return util::EXIT_FAILURE;
            }
        }
    }

    let common = data[0].len().min(data[1].len());
    let mut differences = 0usize;
    for offset in 0..common {
        if data[0][offset] == data[1][offset] {
            continue;
        }
        if options.silent {
            return util::EXIT_FAILURE;
        }
        if options.list {
            let line = alloc::format!(
                "{} {:03o} {:03o}",
                offset + 1,
                data[0][offset],
                data[1][offset]
            );
            if let Err(err) = util::write_line(1, line.as_bytes()) {
                eprintln!("cmp: write error: {err}");
                return util::EXIT_FAILURE;
            }
            differences += 1;
        } else {
            let line_number = 1 + count_newlines(&data[0][..offset]);
            let text = alloc::format!(
                "{} {} differ: byte {}, line {}, offset 0x{:x}",
                names[0],
                names[1],
                offset + 1,
                line_number,
                offset
            );
            if let Err(err) = util::write_line(1, text.as_bytes()) {
                eprintln!("cmp: write error: {err}");
                return util::EXIT_FAILURE;
            }
            return util::EXIT_FAILURE;
        }
    }

    if data[0].len() != data[1].len() {
        if !options.silent {
            let shorter = if data[0].len() < data[1].len() { 0 } else { 1 };
            let bytes = data[shorter].len();
            let line_number = lines_through(&data[shorter]);
            let text = alloc::format!(
                "cmp: EOF on {} after byte {}, line {}",
                names[shorter], bytes, line_number
            );
            let _ = util::write_line(2, text.as_bytes());
        }
        return util::EXIT_FAILURE;
    }

    if differences > 0 {
        util::EXIT_FAILURE
    } else {
        util::EXIT_OK
    }
}

fn count_newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|byte| **byte == b'\n').count()
}

/// Human line number for "EOF after byte N": the line containing the last byte,
/// not the line after a final newline (matching cmp's wording).
fn lines_through(bytes: &[u8]) -> usize {
    let mut lines = count_newlines(bytes);
    if bytes.last() != Some(&b'\n') {
        lines += 1;
    }
    lines.max(1)
}