//! `wc` — count lines, words and bytes.
//!
//! Options `-l`, `-w`, `-c` (combinable, default all three). Words are runs of
//! non-whitespace bytes (space, tab, newline, CR, VT, FF). A file that cannot
//! be read is reported and skipped; the applet still prints the others and the
//! total, and exits 1.
//!
//! Formatting follows the familiar shape: selected counts in `l w c` order,
//! right-aligned to the widest value, followed by the name when the user named
//! an operand (`-` included). With no operands and more than one column the
//! width is 7, matching what `wc` does for a pipe. The ABI does not define this
//! formatting; it is a port policy, not a kernel contract.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use libpagh::{eprintln, Errno};

use crate::util;

const USAGE: &str = "usage: wc [-lwc] [FILE...]";

#[derive(Clone, Copy, Default)]
struct Counts {
    lines: u64,
    words: u64,
    bytes: u64,
}

pub fn run(args: &[String]) -> i32 {
    let mut show_lines = false;
    let mut show_words = false;
    let mut show_bytes = false;
    let mut files: Vec<&str> = Vec::new();
    let mut options = true;

    for arg in args {
        if options && arg == "--" {
            options = false;
            continue;
        }
        if options && arg.len() > 1 && arg.starts_with('-') {
            for option in arg[1..].chars() {
                match option {
                    'l' => show_lines = true,
                    'w' => show_words = true,
                    'c' => show_bytes = true,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        files.push(arg.as_str());
    }

    let named = !files.is_empty();
    if files.is_empty() {
        files.push("-");
    }

    if !show_lines && !show_words && !show_bytes {
        show_lines = true;
        show_words = true;
        show_bytes = true;
    }
    let columns = usize::from(show_lines) + usize::from(show_words) + usize::from(show_bytes);

    let mut status = util::EXIT_OK;
    let mut results: Vec<(String, Counts)> = Vec::new();

    for name in &files {
        match count_file(name) {
            Ok(counts) => results.push((String::from(*name), counts)),
            Err(err) => {
                util::report("wc", name, err);
                status = util::EXIT_FAILURE;
            }
        }
    }

    let mut total = Counts::default();
    for (_, counts) in &results {
        total.lines = total.lines.saturating_add(counts.lines);
        total.words = total.words.saturating_add(counts.words);
        total.bytes = total.bytes.saturating_add(counts.bytes);
    }

    let mut max_value = 0u64;
    for (_, counts) in &results {
        max_value = max_value.max(counts.lines).max(counts.words).max(counts.bytes);
    }
    max_value = max_value
        .max(total.lines)
        .max(total.words)
        .max(total.bytes);
    let width = if !named && columns > 1 {
        7
    } else {
        util::digits(max_value)
    };

    for (name, counts) in &results {
        let shown_name = if named { name.as_str() } else { "" };
        if write_line(counts, shown_name, width, show_lines, show_words, show_bytes) {
            return util::EXIT_FAILURE;
        }
    }
    if results.len() > 1 {
        if write_line(&total, "total", width, show_lines, show_words, show_bytes) {
            return util::EXIT_FAILURE;
        }
    }

    status
}

fn count_file(name: &str) -> Result<Counts, Errno> {
    let input = util::Input::open(name)?;
    let mut counts = Counts::default();
    let mut in_word = false;
    let mut buf = [0u8; 8192];

    loop {
        let n = util::read_retry(input.fd(), &mut buf)?;
        if n == 0 {
            return Ok(counts);
        }
        counts.bytes = counts.bytes.saturating_add(n as u64);
        for byte in &buf[..n] {
            if *byte == b'\n' {
                counts.lines = counts.lines.saturating_add(1);
            }
            if is_space(*byte) {
                in_word = false;
            } else if !in_word {
                counts.words = counts.words.saturating_add(1);
                in_word = true;
            }
        }
    }
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn write_line(
    counts: &Counts,
    name: &str,
    width: usize,
    lines: bool,
    words: bool,
    bytes: bool,
) -> bool {
    let mut line = String::new();
    if lines {
        let _ = write!(line, "{:>width$}", counts.lines, width = width);
    }
    if words {
        if !line.is_empty() {
            line.push(' ');
        }
        let _ = write!(line, "{:>width$}", counts.words, width = width);
    }
    if bytes {
        if !line.is_empty() {
            line.push(' ');
        }
        let _ = write!(line, "{:>width$}", counts.bytes, width = width);
    }
    if !name.is_empty() {
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(name);
    }
    line.push('\n');
    match util::write_all(1, line.as_bytes()) {
        Ok(()) => false,
        Err(err) => {
            eprintln!("wc: write error: {err}");
            true
        }
    }
}
