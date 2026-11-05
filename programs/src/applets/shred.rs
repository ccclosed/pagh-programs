//! `shred` — overwrite a file before removing it.
//!
//! Usage: `shred [-n N] [-u] [-z] [-s SIZE] FILE...`. Each file is opened
//! read-write and overwritten `N` times (default 3) from offset zero with a
//! deterministic xorshift64 pattern; `-z` adds a final zero pass and `-u`
//! unlinks the file afterwards. `-s SIZE` overrides the number of bytes
//! written (default: the current file length). Writing past the old end grows
//! the file; `-s` does not truncate a longer file.
//!
//! The pattern is mapped to printable ASCII (`0x20..=0x7e`) so that a `cat`
//! after a shred is safe on the serial console while still showing that the
//! bytes changed. This is a best-effort overwrite, **not** a secure erase: the
//! ABI has no `fsync`, the ext2 driver may keep blocks cached, and the disk
//! may remap sectors. The kernel's `fchmodat`-style per-run metadata does not
//! affect this; `shred` writes data blocks through `write(2)`.
//!
//! Exit: 0 success, 1 a file could not be opened/written/removed, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::fs::File;
use libpagh::io::{Seek, SeekFrom, Write};
use libpagh::{eprintln, fs, sys};

use crate::util;

const USAGE: &str = "usage: shred [-n N] [-u] [-z] [-s SIZE] FILE...";
const CHUNK: usize = 4096;

pub fn run(args: &[String]) -> i32 {
    let mut passes: usize = 3;
    let mut remove = false;
    let mut zero = false;
    let mut size: Option<u64> = None;
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0usize;
    let mut parse_options = true;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && (arg == "-n" || arg == "--iterations") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match util::parse_usize(value) {
                Some(parsed) => passes = parsed,
                None => {
                    eprintln!("shred: invalid pass count: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && (arg == "-s" || arg == "--size") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match parse_size(value) {
                Some(parsed) => size = Some(parsed),
                None => {
                    eprintln!("shred: invalid size: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && (arg == "-u" || arg == "--remove") {
            remove = true;
            continue;
        }
        if parse_options && (arg == "-z" || arg == "--zero") {
            zero = true;
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("--iterations=") {
                match util::parse_usize(value) {
                    Some(parsed) => {
                        passes = parsed;
                        continue;
                    }
                    None => {
                        eprintln!("shred: invalid pass count: '{value}'");
                        return util::EXIT_USAGE;
                    }
                }
            }
            if let Some(value) = arg.strip_prefix("--size=") {
                match parse_size(value) {
                    Some(parsed) => {
                        size = Some(parsed);
                        continue;
                    }
                    None => {
                        eprintln!("shred: invalid size: '{value}'");
                        return util::EXIT_USAGE;
                    }
                }
            }
            if let Some(value) = arg.strip_prefix("-n") {
                if !value.is_empty() {
                    match util::parse_usize(value) {
                        Some(parsed) => {
                            passes = parsed;
                            continue;
                        }
                        None => {
                            eprintln!("shred: invalid pass count: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                }
            }
            if let Some(value) = arg.strip_prefix("-s") {
                if !value.is_empty() {
                    match parse_size(value) {
                        Some(parsed) => {
                            size = Some(parsed);
                            continue;
                        }
                        None => {
                            eprintln!("shred: invalid size: '{value}'");
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
            eprintln!("shred: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut status = util::EXIT_OK;
    for (file_index, name) in operands.iter().enumerate() {
        let stat = match fs::stat(name) {
            Ok(stat) => stat,
            Err(err) => {
                util::report("shred", name, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        if !stat.is_file() {
            util::report("shred", name, libpagh::Errno::EISDIR);
            status = util::EXIT_FAILURE;
            continue;
        }
        let target = size.unwrap_or(stat.size);
        let mut file = match File::open_with(name, sys::O_RDWR, 0) {
            Ok(file) => file,
            Err(err) => {
                util::report("shred", name, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };

        let mut failed = false;
        for pass in 0..passes {
            if let Err(err) = overwrite(&mut file, target, seed(pass, file_index)) {
                util::report("shred", name, err);
                status = util::EXIT_FAILURE;
                failed = true;
                break;
            }
        }
        if !failed && zero {
            if let Err(err) = write_zeros(&mut file, target) {
                util::report("shred", name, err);
                status = util::EXIT_FAILURE;
                failed = true;
            }
        }
        drop(file);
        if !failed && remove {
            if let Err(err) = fs::remove_file(name) {
                util::report("shred", name, err);
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}

/// One deterministic pass over the first `size` bytes.
fn overwrite(file: &mut File, size: u64, mut state: u64) -> libpagh::errno::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut buffer = [0u8; CHUNK];
    let mut remaining = size;
    while remaining > 0 {
        let take = remaining.min(CHUNK as u64) as usize;
        fill_pattern(&mut buffer[..take], &mut state);
        file.write_all(&buffer[..take])?;
        remaining -= take as u64;
    }
    Ok(())
}

/// The `-z` pass.
fn write_zeros(file: &mut File, size: u64) -> libpagh::errno::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let zeros = [0u8; CHUNK];
    let mut remaining = size;
    while remaining > 0 {
        let take = remaining.min(CHUNK as u64) as usize;
        file.write_all(&zeros[..take])?;
        remaining -= take as u64;
    }
    Ok(())
}

/// xorshift64 bytes mapped onto printable ASCII so the console stays sane.
fn fill_pattern(buffer: &mut [u8], state: &mut u64) {
    for byte in buffer.iter_mut() {
        let mut value = *state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        *state = value;
        *byte = b' ' + ((value >> 24) % 95) as u8;
    }
}

/// Pass/file-dependent seed; the same file and pass always produce the same
/// bytes, which makes the live transcript reproducible.
fn seed(pass: usize, file_index: usize) -> u64 {
    0x9E37_79B9_7F4A_7C15u64
        ^ (pass as u64).wrapping_mul(0x0000_0100_0000_01b3)
        ^ (file_index as u64).wrapping_mul(0xD1B5_4A32_D192_ED03)
}

fn parse_size(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u64>().ok()
}