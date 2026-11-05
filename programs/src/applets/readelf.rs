//! `readelf` — inspect the header, sections, program headers and symbols of a
//! 64-bit little-endian ELF file.
//!
//! ```text
//! readelf [-h] [-S] [-s] [-l] FILE
//! ```
//!
//! With no option it prints the file header (`-h`); the options can be
//! combined and are printed in a fixed order: header, program headers,
//! sections, symbols. `-` reads stdin. The parser is shared with [`super::nm`]
//! ([`super::elf`]), so `-s` prints the same lines `nm` would and a file that
//! `nm` rejects is rejected here too.
//!
//! Output is a compact, greppable subset of GNU `readelf`, not a byte-for-byte
//! copy: the header fields are `type`, `machine`, `entry`, `phoff`, `shoff`,
//! `flags`, `ehsize`, `phentsize`, `phnum`, `shentsize`, `shnum`; each section
//! is one line with `[index] name type addr off size`; each symbol is
//! `value type name` in table order; each program header is one line with
//! type, offset, vaddr/paddr, file/memory size, flags and alignment.
//!
//! Only ELF64 little-endian is supported (the pagh target), `-S`/`-s`/`-l`
//! need a section table, and there is no DWARF/line-table reader (that is why
//! the bundle has no `addr2line`). Errors — unreadable file, bad magic,
//! truncated table — exit 1; a usage error exits 2.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::errno::Errno;

use super::elf::{Elf, Header};
use crate::util;

const USAGE: &str = "usage: readelf [-h] [-S] [-s] [-l] FILE";

/// A printing failure: a bad write, or a malformed table found while walking.
enum Failure {
    Io(Errno),
    Data(&'static str),
}

impl From<Errno> for Failure {
    fn from(err: Errno) -> Failure {
        Failure::Io(err)
    }
}

pub fn run(args: &[String]) -> i32 {
    let Some((wanted, operands)) = parse(args) else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };
    if operands.len() != 1 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let name = operands[0];

    let data = match util::read_input(name) {
        Ok(data) => data,
        Err(err) => {
            util::report("readelf", util::display_name(name), err);
            return util::EXIT_FAILURE;
        }
    };

    let mut wanted = wanted;
    if !wanted.header && !wanted.sections && !wanted.symbols && !wanted.programs {
        wanted.header = true;
    }

    let header = match Header::parse(&data) {
        Ok(header) => header,
        Err(message) => {
            eprintln!("readelf: {}: {message}", util::display_name(name));
            return util::EXIT_FAILURE;
        }
    };

    if wanted.header {
        if let Err(err) = print_header(&header) {
            eprintln!("readelf: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }

    if !(wanted.sections || wanted.symbols || wanted.programs) {
        return util::EXIT_OK;
    }

    let image = match Elf::parse(&data) {
        Ok(image) => image,
        Err(message) => {
            eprintln!("readelf: {}: {message}", util::display_name(name));
            return util::EXIT_FAILURE;
        }
    };

    let mut printed = Ok(());
    if wanted.programs {
        printed = print_program_headers(&image);
    }
    if printed.is_ok() && wanted.sections {
        printed = print_sections(&image);
    }
    if printed.is_ok() && wanted.symbols {
        printed = print_symbols(&image);
    }

    match printed {
        Ok(()) => util::EXIT_OK,
        Err(Failure::Io(err)) => {
            eprintln!("readelf: write error: {err}");
            util::EXIT_FAILURE
        }
        Err(Failure::Data(message)) => {
            eprintln!("readelf: {}: {message}", util::display_name(name));
            util::EXIT_FAILURE
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Wanted {
    header: bool,
    sections: bool,
    symbols: bool,
    programs: bool,
}

/// Parse the option letters; `None` is a usage error.
fn parse<'a>(args: &'a [String]) -> Option<(Wanted, Vec<&'a str>)> {
    let mut wanted = Wanted::default();
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
                    b'h' => wanted.header = true,
                    b'S' => wanted.sections = true,
                    b's' => wanted.symbols = true,
                    b'l' => wanted.programs = true,
                    _ => return None,
                }
            }
            continue;
        }
        parse_options = false;
        if util::push(&mut operands, text).is_err() {
            return None;
        }
    }
    Some((wanted, operands))
}

fn print_header(header: &Header) -> Result<(), Errno> {
    let lines = [
        String::from("ELF header:"),
        format!(
            "  type:      {} ({})",
            elf_type_name(header.e_type),
            header.e_type
        ),
        format!(
            "  machine:   {} ({})",
            machine_name(header.e_machine),
            header.e_machine
        ),
        format!("  entry:     {:#018x}", header.e_entry),
        format!("  phoff:     {}", header.e_phoff),
        format!("  shoff:     {}", header.e_shoff),
        format!("  flags:     {:#x}", header.e_flags),
        format!("  ehsize:    {}", header.e_ehsize),
        format!("  phentsize: {}", header.e_phentsize),
        format!("  phnum:     {}", header.e_phnum),
        format!("  shentsize: {}", header.e_shentsize),
        format!("  shnum:     {}", header.e_shnum),
    ];
    for line in lines {
        util::write_line(1, line.as_bytes())?;
    }
    Ok(())
}

fn print_sections(image: &Elf<'_>) -> Result<(), Failure> {
    util::write_line(1, b"Sections:")?;
    for (index, section) in image.sections.iter().enumerate() {
        let name = image.section_name(section).map_err(Failure::Data)?;
        let line = format!(
            "  [{index:>2}] {name:<20} {:<16} {:#018x} {:#010x} {:#08x}",
            section_type_name(section.sh_type),
            section.sh_addr,
            section.sh_offset,
            section.sh_size,
        );
        util::write_line(1, line.as_bytes())?;
    }
    Ok(())
}

fn print_symbols(image: &Elf<'_>) -> Result<(), Failure> {
    let (symbols, had_table) = image
        .symbols(false, false)
        .map_err(Failure::Data)?;
    if symbols.is_empty() {
        let text = if had_table {
            "Symbols: none"
        } else {
            "Symbols: no symbol table"
        };
        util::write_line(1, text.as_bytes())?;
        return Ok(());
    }
    util::write_line(1, b"Symbols:")?;
    for symbol in symbols {
        let line = format!(
            "{:016x} {} {}",
            symbol.value,
            symbol.kind as char,
            symbol.name
        );
        util::write_line(1, line.as_bytes())?;
    }
    Ok(())
}

fn print_program_headers(image: &Elf<'_>) -> Result<(), Failure> {
    let headers = image.program_headers().map_err(Failure::Data)?;
    if headers.is_empty() {
        util::write_line(1, b"Program headers: none")?;
        return Ok(());
    }
    util::write_line(1, b"Program headers:")?;
    for (index, header) in headers.iter().enumerate() {
        let line = format!(
            "  [{index:>2}] {:<16} off={:#010x} vaddr={:#018x} paddr={:#018x} filesz={:#x} memsz={:#x} flags={:#x} align={:#x}",
            program_type_name(header.p_type),
            header.p_offset,
            header.p_vaddr,
            header.p_paddr,
            header.p_filesz,
            header.p_memsz,
            header.p_flags,
            header.p_align,
        );
        util::write_line(1, line.as_bytes())?;
    }
    Ok(())
}

fn elf_type_name(kind: u16) -> &'static str {
    match kind {
        0 => "NONE",
        1 => "REL",
        2 => "EXEC",
        3 => "DYN",
        4 => "CORE",
        _ => "UNKNOWN",
    }
}

fn machine_name(machine: u16) -> &'static str {
    match machine {
        2 => "SPARC",
        3 => "x86",
        8 => "MIPS",
        20 => "PowerPC",
        21 => "PowerPC64",
        40 => "ARM",
        62 => "x86-64",
        183 => "AArch64",
        243 => "RISC-V",
        _ => "UNKNOWN",
    }
}

fn section_type_name(kind: u32) -> &'static str {
    match kind {
        0 => "NULL",
        1 => "PROGBITS",
        2 => "SYMTAB",
        3 => "STRTAB",
        4 => "RELA",
        5 => "HASH",
        6 => "DYNAMIC",
        7 => "NOTE",
        8 => "NOBITS",
        9 => "REL",
        10 => "SHLIB",
        11 => "DYNSYM",
        14 => "INIT_ARRAY",
        15 => "FINI_ARRAY",
        16 => "PREINIT_ARRAY",
        17 => "GROUP",
        18 => "SYMTAB_SHNDX",
        0x6fff_fff6 => "GNU_HASH",
        0x6fff_fffe => "VERNEED",
        0x6fff_ffff => "VERSYM",
        _ => "UNKNOWN",
    }
}

fn program_type_name(kind: u32) -> &'static str {
    match kind {
        0 => "NULL",
        1 => "LOAD",
        2 => "DYNAMIC",
        3 => "INTERP",
        4 => "NOTE",
        5 => "SHLIB",
        6 => "PHDR",
        7 => "TLS",
        0x6474_e550 => "GNU_EH_FRAME",
        0x6474_e551 => "GNU_STACK",
        0x6474_e552 => "GNU_RELRO",
        0x7000_0003 => "RISCV_ATTRIBUTES",
        _ => "UNKNOWN",
    }
}