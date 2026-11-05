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
//! The parser is deliberately bounds-checked: a truncated header, a section
//! table past EOF, a bad string-table link or a short symbol entry is a data
//! error (1), never a panic. Usage errors are 2.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: nm [-D] [-u] [-n] FILE...";

#[derive(Clone, Copy)]
struct Section {
    sh_type: u32,
    sh_flags: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_entsize: u64,
}

struct Symbol {
    value: u64,
    kind: u8,
    name: String,
}

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
        let (mut symbols, had_table) = match parse_elf(&data, dyn_only, undef_only) {
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

/// Bounds-checked symbol extraction.
fn parse_elf(
    data: &[u8],
    dyn_only: bool,
    undef_only: bool,
) -> Result<(Vec<Symbol>, bool), &'static str> {
    if data.len() < 64 {
        return Err("file is too short for an ELF64 header");
    }
    if &data[..4] != b"\x7fELF" {
        return Err("not an ELF file");
    }
    if data[4] != 2 {
        return Err("not a 64-bit ELF (ELFCLASS64 required)");
    }
    if data[5] != 1 {
        return Err("not a little-endian ELF (ELFDATA2LSB required)");
    }

    let shoff = u64_at(data, 0x28).ok_or("truncated ELF header")?;
    let shentsize = u16_at(data, 0x3a).ok_or("truncated ELF header")? as u64;
    let shnum = u16_at(data, 0x3c).ok_or("truncated ELF header")? as u64;
    if shnum == 0 || shentsize < 64 {
        return Err("no section headers");
    }

    let table_size = shnum
        .checked_mul(shentsize)
        .ok_or("section table size overflows")?;
    if slice(data, shoff, table_size).is_none() {
        return Err("section table extends past end of file");
    }

    let mut sections: Vec<Section> = Vec::new();
    sections
        .try_reserve(shnum as usize)
        .map_err(|_| "out of memory")?;
    for index in 0..shnum {
        let offset = shoff
            .checked_add(index.checked_mul(shentsize).ok_or("section table overflows")?)
            .ok_or("section table overflows")?;
        let section = read_section(data, offset).ok_or("truncated section header")?;
        sections.push(section);
    }

    let mut chosen: Vec<usize> = Vec::new();
    let mut has_symtab = false;
    for (index, section) in sections.iter().enumerate() {
        if section.sh_type == 2 {
            has_symtab = true;
            if !dyn_only {
                chosen.push(index);
            }
        }
    }
    if dyn_only || !has_symtab {
        chosen.clear();
        for (index, section) in sections.iter().enumerate() {
            if section.sh_type == 11 {
                chosen.push(index);
            }
        }
    }

    let had_table = !chosen.is_empty();
    let mut symbols: Vec<Symbol> = Vec::new();
    for index in chosen {
        let section = sections[index];
        let entries = symbol_table(data, &sections, section, undef_only)?;
        for symbol in entries {
            symbols.try_reserve(1).map_err(|_| "out of memory")?;
            symbols.push(symbol);
        }
    }
    Ok((symbols, had_table))
}

fn read_section(data: &[u8], offset: u64) -> Option<Section> {
    Some(Section {
        sh_type: u32_at(data, offset.checked_add(4)?)?,
        sh_flags: u64_at(data, offset.checked_add(8)?)?,
        sh_offset: u64_at(data, offset.checked_add(0x18)?)?,
        sh_size: u64_at(data, offset.checked_add(0x20)?)?,
        sh_link: u32_at(data, offset.checked_add(0x28)?)?,
        sh_entsize: u64_at(data, offset.checked_add(0x38)?)?,
    })
}

fn symbol_table(
    data: &[u8],
    sections: &[Section],
    section: Section,
    undef_only: bool,
) -> Result<Vec<Symbol>, &'static str> {
    let entsize = if section.sh_entsize == 0 {
        24
    } else {
        section.sh_entsize
    };
    if entsize < 24 {
        return Err("symbol entry is shorter than 24 bytes");
    }
    let count = section.sh_size / entsize;
    let table_size = count
        .checked_mul(entsize)
        .ok_or("symbol table size overflows")?;
    if slice(data, section.sh_offset, table_size).is_none() {
        return Err("symbol table extends past end of file");
    }

    let strtab = sections
        .get(section.sh_link as usize)
        .ok_or("symbol table has an invalid string-table index")?;
    let strings = slice(data, strtab.sh_offset, strtab.sh_size)
        .ok_or("string table extends past end of file")?;

    let mut symbols: Vec<Symbol> = Vec::new();
    for index in 0..count {
        let offset = section
            .sh_offset
            .checked_add(index.checked_mul(entsize).ok_or("symbol table overflows")?)
            .ok_or("symbol table overflows")?;
        let entry = slice(data, offset, 24).ok_or("truncated symbol entry")?;
        let name_offset = u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]) as usize;
        let info = entry[4];
        let shndx = u16::from_le_bytes([entry[6], entry[7]]);
        let value = u64::from_le_bytes([
            entry[8], entry[9], entry[10], entry[11], entry[12], entry[13], entry[14], entry[15],
        ]);

        if name_offset == 0 {
            // ELF's empty-name sentinel; the first byte of a string table is NUL.
            continue;
        }
        let name = match c_string(strings, name_offset) {
            Some(name) if !name.is_empty() => String::from_utf8_lossy(name).into_owned(),
            _ => continue,
        };
        let kind_type = info & 0x0f;
        if kind_type == 3 || kind_type == 4 {
            // STT_SECTION / STT_FILE: GNU nm omits these by default.
            continue;
        }
        if undef_only && shndx != 0 {
            continue;
        }
        let local = (info >> 4) == 0;
        let kind = symbol_kind(shndx, kind_type, local, sections);
        symbols.try_reserve(1).map_err(|_| "out of memory")?;
        symbols.push(Symbol { value, kind, name });
    }
    Ok(symbols)
}

/// The type letter for one symbol.
fn symbol_kind(shndx: u16, kind_type: u8, local: bool, sections: &[Section]) -> u8 {
    if shndx == 0 {
        return b'U';
    }
    let base = match shndx {
        0xfff1 => b'A',
        0xfff2 => b'C',
        _ => {
            let section = sections.get(shndx as usize);
            match kind_type {
                1 | 6 => {
                    // OBJECT / TLS: .bss is B, writable data D, read-only alloc R.
                    match section {
                        Some(section) if section.sh_type == 8 => b'B',
                        Some(section) if section.sh_flags & 0x1 != 0 => b'D',
                        Some(section) if section.sh_flags & 0x2 != 0 => b'R',
                        _ => b'D',
                    }
                }
                2 | 10 => b'T', // FUNC, GNU_IFUNC
                5 => b'C',      // COMMON
                _ => match section {
                    Some(section) if section.sh_flags & 0x4 != 0 => b'T',
                    Some(section) if section.sh_type == 8 => b'B',
                    Some(section) if section.sh_flags & 0x1 != 0 => b'D',
                    Some(section) if section.sh_flags & 0x2 != 0 => b'R',
                    _ => b'N',
                },
            }
        }
    };
    if local {
        base.to_ascii_lowercase()
    } else {
        base
    }
}

fn c_string(data: &[u8], offset: usize) -> Option<&[u8]> {
    let rest = data.get(offset..)?;
    let end = rest.iter().position(|byte| *byte == 0).unwrap_or(rest.len());
    Some(&rest[..end])
}

fn slice(data: &[u8], offset: u64, length: u64) -> Option<&[u8]> {
    let start = usize::try_from(offset).ok()?;
    let length = usize::try_from(length).ok()?;
    let end = start.checked_add(length)?;
    data.get(start..end)
}

fn u16_at(data: &[u8], offset: u64) -> Option<u16> {
    let bytes = slice(data, offset, 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn u32_at(data: &[u8], offset: u64) -> Option<u32> {
    let bytes = slice(data, offset, 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn u64_at(data: &[u8], offset: u64) -> Option<u64> {
    let bytes = slice(data, offset, 8)?;
    Some(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}