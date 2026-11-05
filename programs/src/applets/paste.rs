//! `paste` — merge files line by line.
//!
//! With one file, each line is printed as is. With several files, line N is
//! formed by joining line N of every file with a delimiter; a file that has no
//! line N contributes an empty field (so a short last line still gets its
//! delimiter). The delimiter list is cycled **within each output line** and
//! restarts at the first delimiter on the next line, matching GNU `paste`.
//!
//! `-d LIST` sets the delimiters. LIST is byte-oriented: `\t` (default), `\n`,
//! `\\` and `\0` (an empty delimiter) are recognized; any other backslash
//! escape is the escaped byte. An empty LIST means no delimiter at all. No
//! `-s` (serial) mode and no output file: the kernel shell has no redirection.
//!
//! No operands means stdin. A missing file is reported and the remaining files
//! are still pasted, but the exit status is 1.
//!
//! Exit: 0 success, 1 open/read/write error, 2 usage error.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: paste [-d LIST] [FILE...]";

struct FileLines {
    data: Vec<u8>,
    ranges: Vec<(usize, usize)>,
}

pub fn run(args: &[String]) -> i32 {
    let mut delimiters: Vec<Option<u8>> = alloc::vec![Some(b'\t')];
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
            match parse_delimiters(value) {
                Some(parsed) => delimiters = parsed,
                None => {
                    eprintln!("paste: invalid delimiter list: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-d") {
            match parse_delimiters(&arg[2..]) {
                Some(parsed) => delimiters = parsed,
                None => {
                    eprintln!("paste: invalid delimiter list: '{}'", &arg[2..]);
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

    if operands.is_empty() {
        operands.push("-");
    }

    let mut status = util::EXIT_OK;
    let mut files: Vec<FileLines> = Vec::new();
    for name in operands {
        match read_lines(name) {
            Ok(file) => {
                if files.try_reserve(1).is_err() {
                    eprintln!("paste: out of memory");
                    return util::EXIT_FAILURE;
                }
                files.push(file);
            }
            Err(err) => {
                util::report("paste", util::display_name(name), err);
                status = util::EXIT_FAILURE;
            }
        }
    }

    let rows = files.iter().map(|file| file.ranges.len()).max().unwrap_or(0);
    for row in 0..rows {
        let capacity = files
            .iter()
            .map(|file| match file.ranges.get(row) {
                Some(&(start, end)) => end - start + 1,
                None => 1,
            })
            .sum();
        let mut out: Vec<u8> = Vec::new();
        if out.try_reserve(capacity).is_err() {
            eprintln!("paste: out of memory");
            return util::EXIT_FAILURE;
        }

        let mut delimiter = 0usize;
        for (file_index, file) in files.iter().enumerate() {
            if let Some(&(start, end)) = file.ranges.get(row) {
                out.extend_from_slice(&file.data[start..end]);
            }
            if file_index + 1 < files.len() && !delimiters.is_empty() {
                if let Some(byte) = delimiters[delimiter % delimiters.len()] {
                    out.push(byte);
                }
                delimiter += 1;
            }
        }

        if let Err(err) = util::write_line(1, &out) {
            eprintln!("paste: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }

    status
}

fn read_lines(name: &str) -> Result<FileLines, Errno> {
    let data = util::read_input(name)?;
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    while start < data.len() {
        let (end, next) = match data[start..].iter().position(|byte| *byte == b'\n') {
            Some(offset) => (start + offset, start + offset + 1),
            None => (data.len(), data.len()),
        };
        ranges.try_reserve(1).map_err(|_| Errno::ENOMEM)?;
        ranges.push((start, end));
        start = next;
    }
    Ok(FileLines { data, ranges })
}

fn parse_delimiters(text: &str) -> Option<Vec<Option<u8>>> {
    let bytes = text.as_bytes();
    let mut out: Vec<Option<u8>> = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        let slot = if bytes[index] == b'\\' && index + 1 < bytes.len() {
            let next = bytes[index + 1];
            index += 2;
            match next {
                b'n' => Some(b'\n'),
                b't' => Some(b'\t'),
                b'\\' => Some(b'\\'),
                b'0' => None,
                other => Some(other),
            }
        } else {
            let byte = bytes[index];
            index += 1;
            Some(byte)
        };
        out.try_reserve(1).ok()?;
        out.push(slot);
    }

    Some(out)
}