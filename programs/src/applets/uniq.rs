//! `uniq` — collapse consecutive duplicate lines.
//!
//! Options: `-c` prefix each group with its count (right-aligned to seven
//! columns, then a space), `-d` print only groups that repeat, `-u` print only
//! groups that do not. `-d` and `-u` together print nothing, matching GNU.
//! One input operand (a file or `-`); no operand means stdin. Only *consecutive*
//! duplicates collapse — sort the input first if you want global uniqueness.
//!
//! There is no output-file operand: the kernel shell has no redirection, and
//! writing a result file is outside this batch's read-only scope. Output is raw
//! bytes with a final newline per line; input is never decoded as UTF-8.
//!
//! Exit: 0 success, 1 open/read/write error, 2 usage error.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: uniq [-c] [-d] [-u] [FILE]";

#[derive(Clone, Copy, Default)]
struct Options {
    count: bool,
    duplicates: bool,
    unique: bool,
}

pub fn run(args: &[String]) -> i32 {
    let mut options = Options {
        duplicates: true,
        unique: true,
        ..Options::default()
    };
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
                    'c' => options.count = true,
                    'd' => options.unique = false,
                    'u' => options.duplicates = false,
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

    if operands.len() > 1 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let name = operands.first().copied().unwrap_or("-");

    let data = match util::read_input(name) {
        Ok(data) => data,
        Err(err) => {
            util::report("uniq", util::display_name(name), err);
            return util::EXIT_FAILURE;
        }
    };

    let mut status = util::EXIT_OK;
    let mut group: Option<&[u8]> = None;
    let mut count: u64 = 0;

    for line in util::lines(&data) {
        match group {
            Some(current) if current == line => {
                count = count.saturating_add(1);
            }
            Some(current) => {
                if emit(current, count, &options).is_err() {
                    status = util::EXIT_FAILURE;
                    break;
                }
                group = Some(line);
                count = 1;
            }
            None => {
                group = Some(line);
                count = 1;
            }
        }
    }
    if let Some(current) = group {
        if emit(current, count, &options).is_err() {
            status = util::EXIT_FAILURE;
        }
    }

    status
}

fn emit(line: &[u8], count: u64, options: &Options) -> Result<(), Errno> {
    let repeated = count > 1;
    if (repeated && !options.duplicates) || (!repeated && !options.unique) {
        return Ok(());
    }
    if options.count {
        let prefix = format!("{count:>7} ");
        util::write_all(1, prefix.as_bytes())?;
    }
    util::write_line(1, line)
}