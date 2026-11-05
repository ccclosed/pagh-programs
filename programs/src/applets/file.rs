//! `file` — classify each operand by content.
//!
//! The classes are pagh's own small set, not libmagic's database:
//!
//! * `empty` — a regular file of size 0;
//! * `ELF executable` — the first four bytes are `\x7fELF`;
//! * `directory`, `symbolic link to '...'`, `other`;
//! * `text` — every byte in the first 4096 is printable ASCII (plus tab, LF
//!   and CR);
//! * `binary` — anything else (a NUL or a non-printable byte).
//!
//! The link check comes first (`read_link` looks at the link itself), so a
//! broken symlink is still reported as a link. A regular file is read up to
//! 4096 bytes; the classification is a content heuristic, not a promise about
//! the rest of a larger file. Multiple operands are reported in order; a
//! missing/unreadable operand is reported on fd 2 and the others are still
//! inspected, with exit status 1. Options are absent (`--` is accepted), and
//! output always goes to fd 1 — the shell has no pipes/redirection.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::{self, File};
use libpagh::Errno;

use crate::util;

const USAGE: &str = "usage: file FILE...";
const PEEK: usize = 4096;

pub fn run(args: &[String]) -> i32 {
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("file: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        match describe(name) {
            Ok(description) => {
                let line = format!("{name}: {description}");
                if let Err(err) = util::write_line(1, line.as_bytes()) {
                    eprintln!("file: write error: {err}");
                    return util::EXIT_FAILURE;
                }
            }
            Err(err) => {
                util::report("file", name, err);
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}

fn describe(name: &str) -> Result<String, Errno> {
    match fs::read_link(name) {
        Ok(target) => return Ok(format!("symbolic link to '{target}'")),
        Err(Errno::EINVAL) => {}
        Err(err) => return Err(err),
    }

    let stat = fs::stat(name)?;
    match stat.entry_kind() {
        libpagh::EntryKind::Dir => Ok(String::from("directory")),
        libpagh::EntryKind::Symlink => Ok(String::from("symbolic link")),
        libpagh::EntryKind::Other => Ok(String::from("other")),
        libpagh::EntryKind::File => {
            if stat.size == 0 {
                return Ok(String::from("empty"));
            }
            let mut data: Vec<u8> = Vec::new();
            let want = usize::try_from(stat.size).unwrap_or(PEEK).min(PEEK);
            data.try_reserve(want).map_err(|_| Errno::ENOMEM)?;

            let file = File::open(name)?;
            let mut buffer = [0u8; 1024];
            while data.len() < want {
                let take = (want - data.len()).min(buffer.len());
                match util::read_retry(file.as_raw_fd(), &mut buffer[..take]) {
                    Ok(0) => break,
                    Ok(read) => {
                        data.try_reserve(read).map_err(|_| Errno::ENOMEM)?;
                        data.extend_from_slice(&buffer[..read]);
                    }
                    Err(err) => return Err(err),
                }
            }

            if data.is_empty() {
                return Ok(String::from("empty"));
            }
            if data.len() >= 4 && &data[..4] == b"\x7fELF" {
                return Ok(String::from("ELF executable"));
            }
            if data
                .iter()
                .all(|byte| matches!(*byte, 0x09 | 0x0a | 0x0d | 0x20..=0x7e))
            {
                Ok(String::from("text"))
            } else {
                Ok(String::from("binary"))
            }
        }
    }
}