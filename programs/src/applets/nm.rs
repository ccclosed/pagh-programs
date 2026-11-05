//! `nm` — list the symbol table of a 64-bit little-endian ELF file.
//!
//! Usage: `nm [-D] [-u] [-n] FILE...`. Each line is
//! `0000000000001234 T name`: the 16-hex-digit value, one type letter and the
//! symbol name. `-D` reads `.dynsym` instead of `.symtab`; `-u` keeps only
//! undefined symbols; `-n` sorts numerically by value (ties by name). Without
//! `-D` a `.symtab` wins when present, otherwise `.dynsym` is used, so a
//! static binary is not listed twice.
//!
//! Type letters follow the familiar table: `T`/`t` text, `D`/`d` writable
//! data, `B`/`b` `.bss`, `R`/`r` read-only data, `A`/`a` absolute, `C`/`c`
//! common and `U` undefined; lowercase means a local binding. `STT_SECTION`
//! and `STT_FILE` entries are skipped, as GNU `nm` does by default. There is
//! **no demangling** (no C++ or Rust names) and no `-C`, `-S`, `-l` or `-f`.
//!
//! The parser lives in [`super::elf`] and is shared with `readelf -s`, so the
//! two commands report the same symbols and the same type letters. It is
//! deliberately bounds-checked: a truncated header, a section table past EOF,
//! a bad string-table link or a short symbol entry is a data error (1), never
//! a panic. Usage errors are 2.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use super::elf;
use crate::util;

const USAGE: &str = "usage: nm [-D] [-u] [-n] FILE...";

pub fn run(args: &[String]) -> i32 {
    let Some((dyn_only, undef_only, numeric, operands)) = parse(args) else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };
    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("nm", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        let parsed = match elf::Elf::parse(&data) {
            Ok(parsed) => parsed,
            Err(message) => {
                eprintln!("nm: {}: {message}", util::display_name(name));
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        let (mut symbols, had_table) = match parsed.symbols(dyn_only, undef_only) {
            Ok(result) => result,
            Err(message) => {
                eprintln!("nm: {}: {message}", util::display_name(name));
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        if symbols.is_empty() {
            // A table that the filter emptied is not an error; a file with no
            // matching table at all gets the conventional diagnostic but still
            // exits 0 (GNU nm behaviour).
            if !had_table {
                eprintln!("nm: {}: no symbols", util::display_name(name));
            }
            continue;
        }
        if numeric {
            symbols.sort_by(|left, right| {
                left.value
                    .cmp(&right.value)
                    .then_with(|| left.name.cmp(&right.name))
            });
        }
        for symbol in symbols {
            let line = format!("{:016x} {} {}", symbol.value, symbol.kind as char, symbol.name);
            if let Err(err) = util::write_line(1, line.as_bytes()) {
                eprintln!("nm: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
    }
    status
}

/// Parse options; `None` is a usage error.
fn parse<'a>(args: &'a [String]) -> Option<(bool, bool, bool, Vec<&'a str>)> {
    let mut dyn_only = false;
    let mut undef_only = false;
    let mut numeric = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        let text = arg.as_str();
        if parse_options && text == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && text.len() > 1 && text.starts_with('-') {
            for option in text[1..].bytes() {
                match option {
                    b'D' => dyn_only = true,
                    b'u' => undef_only = true,
                    b'n' => numeric = true,
                    _ => return None,
                }
            }
            continue;
        }
        if util::push(&mut operands, text).is_err() {
            return None;
        }
    }
    Some((dyn_only, undef_only, numeric, operands))
}