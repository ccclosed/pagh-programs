//! Shared ELF64 little-endian reader for `nm` and `readelf`.
//!
//! Only the subset the two applets need is modelled: the ELF header, the
//! section table, the program header table and the symbol tables. Every access
//! is bounds-checked against the file image, so a truncated header, a section
//! table past EOF, a bad string-table link or a short symbol entry becomes a
//! `&'static str` diagnostic rather than a panic. Extended section numbering
//! (`e_shnum == 0`, `SHN_XINDEX`) and sections after the first 65 535 are not
//! supported; both are absent from the pagh toolchain's output.
//!
//! The parser is shared so `readelf -s` and `nm` cannot disagree about which
//! symbols a file has or which type letter each one gets.

use alloc::string::String;
use alloc::vec::Vec;

/// Size of the ELF64 file header, in bytes.
pub const HEADER_SIZE: u64 = 64;
/// Size of an ELF64 section header, in bytes.
pub const SECTION_HEADER_SIZE: u64 = 64;
/// Size of an ELF64 program header, in bytes.
pub const PROGRAM_HEADER_SIZE: u64 = 56;
/// Size of an ELF64 symbol table entry, in bytes.
pub const SYMBOL_SIZE: u64 = 24;

/// The ELF64 file header fields this port uses.
#[derive(Clone, Copy)]
pub struct Header {
    pub e_type: u16,
    pub e_machine: u16,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

impl Header {
    /// Parse and validate the 64-byte header.
    pub fn parse(data: &[u8]) -> Result<Header, &'static str> {
        if data.len() < HEADER_SIZE as usize {
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
        Ok(Header {
            e_type: u16_at(data, 0x10).ok_or("truncated ELF header")?,
            e_machine: u16_at(data, 0x12).ok_or("truncated ELF header")?,
            e_entry: u64_at(data, 0x18).ok_or("truncated ELF header")?,
            e_phoff: u64_at(data, 0x20).ok_or("truncated ELF header")?,
            e_shoff: u64_at(data, 0x28).ok_or("truncated ELF header")?,
            e_flags: u32_at(data, 0x30).ok_or("truncated ELF header")?,
            e_ehsize: u16_at(data, 0x34).ok_or("truncated ELF header")?,
            e_phentsize: u16_at(data, 0x36).ok_or("truncated ELF header")?,
            e_phnum: u16_at(data, 0x38).ok_or("truncated ELF header")?,
            e_shentsize: u16_at(data, 0x3a).ok_or("truncated ELF header")?,
            e_shnum: u16_at(data, 0x3c).ok_or("truncated ELF header")?,
            e_shstrndx: u16_at(data, 0x3e).ok_or("truncated ELF header")?,
        })
    }
}

/// One entry of the section header table.
#[derive(Clone, Copy)]
pub struct Section {
    pub sh_name: u32,
    pub sh_type: u32,
    pub sh_flags: u64,
    pub sh_addr: u64,
    pub sh_offset: u64,
    pub sh_size: u64,
    pub sh_link: u32,
    pub sh_entsize: u64,
}

/// One entry of the program header table.
#[derive(Clone, Copy)]
pub struct ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

/// A symbol as `nm` and `readelf -s` print it: value, type letter and name.
pub struct Symbol {
    pub value: u64,
    pub kind: u8,
    pub name: String,
}

/// A parsed ELF image: the header plus the section table.
pub struct Elf<'a> {
    pub data: &'a [u8],
    pub header: Header,
    pub sections: Vec<Section>,
}

impl<'a> Elf<'a> {
    /// Parse the header and the section table, validating both against EOF.
    pub fn parse(data: &'a [u8]) -> Result<Elf<'a>, &'static str> {
        let header = Header::parse(data)?;
        let shoff = header.e_shoff;
        let shentsize = u64::from(header.e_shentsize);
        let shnum = u64::from(header.e_shnum);
        if shnum == 0 || shentsize < SECTION_HEADER_SIZE {
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
        Ok(Elf {
            data,
            header,
            sections,
        })
    }

    /// Resolve a section's name through `e_shstrndx`.
    ///
    /// `e_shstrndx == SHN_UNDEF` means the file has no section-name table and
    /// every name is empty (the parser prints the index alone). A bad index,
    /// a string table past EOF and an out-of-range name offset are errors.
    pub fn section_name(&self, section: &Section) -> Result<&'a str, &'static str> {
        if self.header.e_shstrndx == 0 {
            return Ok("");
        }
        let strtab = self
            .sections
            .get(self.header.e_shstrndx as usize)
            .ok_or("invalid section string-table index")?;
        let strings = slice(self.data, strtab.sh_offset, strtab.sh_size)
            .ok_or("section string table extends past end of file")?;
        let name = c_string(strings, section.sh_name as usize)
            .ok_or("section name is outside the string table")?;
        core::str::from_utf8(name).map_err(|_| "section name is not valid UTF-8")
    }

    /// Read the program header table; an empty table is `Ok(vec![])`.
    pub fn program_headers(&self) -> Result<Vec<ProgramHeader>, &'static str> {
        let phnum = u64::from(self.header.e_phnum);
        if phnum == 0 {
            return Ok(Vec::new());
        }
        let phentsize = u64::from(self.header.e_phentsize);
        if phentsize < PROGRAM_HEADER_SIZE {
            return Err("program header is shorter than 56 bytes");
        }
        let table_size = phnum
            .checked_mul(phentsize)
            .ok_or("program header table size overflows")?;
        if slice(self.data, self.header.e_phoff, table_size).is_none() {
            return Err("program header table extends past end of file");
        }

        let mut headers: Vec<ProgramHeader> = Vec::new();
        headers
            .try_reserve(phnum as usize)
            .map_err(|_| "out of memory")?;
        for index in 0..phnum {
            let offset = self
                .header
                .e_phoff
                .checked_add(
                    index
                        .checked_mul(phentsize)
                        .ok_or("program header table overflows")?,
                )
                .ok_or("program header table overflows")?;
            let header = read_program_header(self.data, offset)
                .ok_or("truncated program header")?;
            headers.push(header);
        }
        Ok(headers)
    }

    /// Extract symbols from `.symtab` (or `.dynsym` with `dyn_only`).
    ///
    /// The returned bool says whether at least one matching table existed, so
    /// the caller can distinguish "no symbols after filtering" from "the file
    /// has no symbol table".
    pub fn symbols(
        &self,
        dyn_only: bool,
        undef_only: bool,
    ) -> Result<(Vec<Symbol>, bool), &'static str> {
        let mut chosen: Vec<usize> = Vec::new();
        let mut has_symtab = false;
        for (index, section) in self.sections.iter().enumerate() {
            if section.sh_type == 2 {
                has_symtab = true;
                if !dyn_only {
                    chosen.push(index);
                }
            }
        }
        if dyn_only || !has_symtab {
            chosen.clear();
            for (index, section) in self.sections.iter().enumerate() {
                if section.sh_type == 11 {
                    chosen.push(index);
                }
            }
        }

        let had_table = !chosen.is_empty();
        let mut symbols: Vec<Symbol> = Vec::new();
        for index in chosen {
            let section = self.sections[index];
            let entries = symbol_table(self.data, &self.sections, section, undef_only)?;
            for symbol in entries {
                symbols.try_reserve(1).map_err(|_| "out of memory")?;
                symbols.push(symbol);
            }
        }
        Ok((symbols, had_table))
    }
}

fn read_section(data: &[u8], offset: u64) -> Option<Section> {
    Some(Section {
        sh_name: u32_at(data, offset)?,
        sh_type: u32_at(data, offset.checked_add(4)?)?,
        sh_flags: u64_at(data, offset.checked_add(8)?)?,
        sh_addr: u64_at(data, offset.checked_add(0x10)?)?,
        sh_offset: u64_at(data, offset.checked_add(0x18)?)?,
        sh_size: u64_at(data, offset.checked_add(0x20)?)?,
        sh_link: u32_at(data, offset.checked_add(0x28)?)?,
        sh_entsize: u64_at(data, offset.checked_add(0x38)?)?,
    })
}

fn read_program_header(data: &[u8], offset: u64) -> Option<ProgramHeader> {
    Some(ProgramHeader {
        p_type: u32_at(data, offset)?,
        p_flags: u32_at(data, offset.checked_add(4)?)?,
        p_offset: u64_at(data, offset.checked_add(8)?)?,
        p_vaddr: u64_at(data, offset.checked_add(0x10)?)?,
        p_paddr: u64_at(data, offset.checked_add(0x18)?)?,
        p_filesz: u64_at(data, offset.checked_add(0x20)?)?,
        p_memsz: u64_at(data, offset.checked_add(0x28)?)?,
        p_align: u64_at(data, offset.checked_add(0x30)?)?,
    })
}

fn symbol_table(
    data: &[u8],
    sections: &[Section],
    section: Section,
    undef_only: bool,
) -> Result<Vec<Symbol>, &'static str> {
    let entsize = if section.sh_entsize == 0 {
        SYMBOL_SIZE
    } else {
        section.sh_entsize
    };
    if entsize < SYMBOL_SIZE {
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
        let entry = slice(data, offset, SYMBOL_SIZE).ok_or("truncated symbol entry")?;
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