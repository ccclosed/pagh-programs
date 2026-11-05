//! `tar` — create, list and extract **ustar** archives.
//!
//! The format is POSIX ustar (512-byte headers, `ustar\0` magic, the 100-byte
//! name / 155-byte prefix split, octal fields and the standard checksum).
//! Entries are regular files, directories and symbolic links; names are stored
//! relative (a leading `/` is stripped) and `..` components are refused both
//! when creating and when extracting.
//!
//! ```text
//! tar -c [-v] -f ARCHIVE [-C DIR] FILE...
//! tar -t [-v] -f ARCHIVE [MEMBER...]
//! tar -x [-v] -f ARCHIVE [-C DIR] [MEMBER...]
//! ```
//!
//! `-f -` uses stdin/stdout; `-C DIR` selects the directory following operands
//! are relative to when creating, and the destination root when extracting
//! (the last `-C` wins for extraction). `-v` prints entry names to stderr while
//! creating/extracting; `-t` always prints the listing to stdout.
//! Without `-f`, the archive is stdin/stdout. There is no compression, no
//! hard-link detection, no GNU long-name/PAX extension, no sparse-file support
//! and no permission restoration (the ABI's chmod is not persistent); the
//! ustar name/prefix and size limits are enforced with an error instead of
//! being silently truncated. Exit: 0 success, 1 data/I/O error, 2 usage.
//!
//! [`super::untar`] is the same extractor under its historical name.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::{self, Dir};
use libpagh::{path, sys, EntryKind, Errno};

use crate::util;

const USAGE: &str = "usage: tar -c|-t|-x [-v] [-f FILE] [-C DIR] [FILE...]";
const BLOCK: usize = 512;
const MAX_SIZE: u64 = 8_589_934_591; // 11 octal digits

#[derive(Clone, Copy, Eq, PartialEq)]
enum Mode {
    Create,
    List,
    Extract,
}

#[derive(Clone)]
struct Operand {
    name: String,
    /// Directory from the most recent `-C` at the point the operand appeared.
    base: String,
}

pub fn run(args: &[String]) -> i32 {
    run_with("tar", args, None)
}

/// `untar`: extraction only, but the same options and archive reader.
pub fn run_extract(args: &[String]) -> i32 {
    run_with("untar", args, Some(Mode::Extract))
}

fn run_with(program: &str, args: &[String], forced_mode: Option<Mode>) -> i32 {
    let mut requested: Option<Mode> = None;
    let mut archive_name: Option<String> = None;
    let mut verbose = false;
    let mut base = String::new();
    let mut operands: Vec<Operand> = Vec::new();
    let mut parse_options = true;

    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.starts_with("--") {
            if arg == "--create" {
                if !set_mode(&mut requested, Mode::Create) {
                    return usage(program);
                }
            } else if arg == "--extract" || arg == "--get" {
                if !set_mode(&mut requested, Mode::Extract) {
                    return usage(program);
                }
            } else if arg == "--list" {
                if !set_mode(&mut requested, Mode::List) {
                    return usage(program);
                }
            } else if arg == "--verbose" {
                verbose = true;
            } else if let Some(value) = arg.strip_prefix("--file=") {
                archive_name = Some(String::from(value));
            } else if arg == "--file" {
                let Some(value) = args.get(index) else {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                };
                index += 1;
                archive_name = Some(value.clone());
            } else if let Some(value) = arg.strip_prefix("--directory=") {
                match change_base(&mut base, value) {
                    Ok(()) => {}
                    Err(message) => {
                        eprintln!("{program}: {message}");
                        return util::EXIT_FAILURE;
                    }
                }
            } else if arg == "--directory" {
                let Some(value) = args.get(index) else {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                };
                index += 1;
                match change_base(&mut base, value) {
                    Ok(()) => {}
                    Err(message) => {
                        eprintln!("{program}: {message}");
                        return util::EXIT_FAILURE;
                    }
                }
            } else {
                eprintln!("{program}: unrecognized option '{arg}'");
                return usage(program);
            }
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            let bytes = arg.as_bytes();
            let mut position = 1usize;
            let mut stop = false;
            while position < bytes.len() && !stop {
                match bytes[position] {
                    b'c' => {
                        if !set_mode(&mut requested, Mode::Create) {
                            return usage(program);
                        }
                        position += 1;
                    }
                    b'x' => {
                        if !set_mode(&mut requested, Mode::Extract) {
                            return usage(program);
                        }
                        position += 1;
                    }
                    b't' => {
                        if !set_mode(&mut requested, Mode::List) {
                            return usage(program);
                        }
                        position += 1;
                    }
                    b'v' => {
                        verbose = true;
                        position += 1;
                    }
                    b'f' | b'C' => {
                        let option = bytes[position];
                        let value = if position + 1 < bytes.len() {
                            &arg[position + 1..]
                        } else {
                            let Some(value) = args.get(index) else {
                                eprintln!("{USAGE}");
                                return util::EXIT_USAGE;
                            };
                            index += 1;
                            value.as_str()
                        };
                        if option == b'f' {
                            archive_name = Some(String::from(value));
                        } else {
                            match change_base(&mut base, value) {
                                Ok(()) => {}
                                Err(message) => {
                                    eprintln!("{program}: {message}");
                                    return util::EXIT_FAILURE;
                                }
                            }
                        }
                        stop = true;
                    }
                    other => {
                        eprintln!("{program}: invalid option -- '{}'", other as char);
                        return usage(program);
                    }
                }
            }
            continue;
        }
        parse_options = false;
        let operand = Operand {
            name: String::from(arg),
            base: base.clone(),
        };
        if util::push(&mut operands, operand).is_err() {
            eprintln!("{program}: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let mode = match (requested, forced_mode) {
        (Some(mode), Some(forced)) if mode != forced => {
            eprintln!("{program}: conflicting mode option");
            return usage(program);
        }
        (_, Some(forced)) => forced,
        (Some(mode), None) => mode,
        (None, None) => {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
    };

    let archive_name = archive_name.unwrap_or_else(|| String::from("-"));
    match mode {
        Mode::Create => create_archive(program, &archive_name, &operands, verbose),
        Mode::List => list_archive(program, &archive_name, &operands),
        Mode::Extract => extract_archive(program, &archive_name, &operands, &base, verbose),
    }
}

fn usage(program: &str) -> i32 {
    if program == "untar" {
        eprintln!("usage: untar [-v] [-f FILE] [-C DIR] [MEMBER...]");
    } else {
        eprintln!("{USAGE}");
    }
    util::EXIT_USAGE
}

fn set_mode(slot: &mut Option<Mode>, mode: Mode) -> bool {
    match *slot {
        Some(existing) if existing != mode => {
            eprintln!("tar: only one of -c, -t and -x may be given");
            false
        }
        _ => {
            *slot = Some(mode);
            true
        }
    }
}

fn change_base(base: &mut String, directory: &str) -> Result<(), String> {
    let stat = fs::stat(directory).map_err(|err| format!("{directory}: {err}"))?;
    if stat.entry_kind() != EntryKind::Dir {
        return Err(format!("{directory}: Not a directory"));
    }
    *base = path::resolve(directory);
    Ok(())
}

// ─── Create ───────────────────────────────────────────────────────────────

fn create_archive(program: &str, archive_name: &str, operands: &[Operand], verbose: bool) -> i32 {
    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut archive: Vec<u8> = Vec::new();
    let mut status = util::EXIT_OK;
    let mut now = libpagh::Timespec::default();
    let timestamp = match sys::clock_gettime(sys::CLOCK_REALTIME, &mut now) {
        Ok(()) if now.seconds >= 0 => now.seconds as u64,
        _ => 0,
    };

    for operand in operands {
        let full = if operand.name.starts_with('/') || operand.base.is_empty() {
            operand.name.clone()
        } else {
            util::join_path(&operand.base, &operand.name)
        };
        let Some(name) = normalize_member(&operand.name) else {
            eprintln!("{program}: {}: unsafe or empty archive name", operand.name);
            status = util::EXIT_FAILURE;
            continue;
        };
        if let Err(message) = add_path(&mut archive, &full, &name, timestamp, verbose) {
            eprintln!("{program}: {message}");
            status = util::EXIT_FAILURE;
        }
    }

    if archive.try_reserve(1024).is_err() {
        eprintln!("{program}: out of memory");
        return util::EXIT_FAILURE;
    }
    archive.extend_from_slice(&[0u8; 1024]);

    let write_result = if archive_name == "-" {
        util::write_all(1, &archive)
    } else {
        fs::write(archive_name, &archive)
    };
    if let Err(err) = write_result {
        util::report(program, archive_name, err);
        return util::EXIT_FAILURE;
    }
    status
}

fn add_path(
    archive: &mut Vec<u8>,
    full: &str,
    name: &str,
    timestamp: u64,
    verbose: bool,
) -> Result<(), String> {
    let kind = util::path_kind(full).map_err(|err| format!("{full}: {err}"))?;
    match kind {
        EntryKind::Symlink => {
            let target = fs::read_link(full).map_err(|err| format!("{full}: {err}"))?;
            let header = build_header(name, b'2', 0, 0o777, timestamp, Some(&target))?;
            push_header(archive, &header)?;
            if verbose {
                eprintln!("{name}");
            }
        }
        EntryKind::Dir => {
            let stat = fs::stat(full).map_err(|err| format!("{full}: {err}"))?;
            let mut dir_name = String::from(name);
            if !dir_name.ends_with('/') {
                dir_name.push('/');
            }
            let header = build_header(
                &dir_name,
                b'5',
                0,
                (stat.mode & 0o7777) as u32,
                timestamp,
                None,
            )?;
            push_header(archive, &header)?;
            if verbose {
                eprintln!("{dir_name}");
            }

            let dir = Dir::open(full).map_err(|err| format!("{full}: {err}"))?;
            let mut children: Vec<String> = Vec::new();
            for entry in dir {
                let entry = entry.map_err(|err| format!("{full}: {err}"))?;
                if util::push(&mut children, entry.name().to_string()).is_err() {
                    return Err(String::from("out of memory"));
                }
            }
            children.sort();
            for child in children {
                if child == "." || child == ".." {
                    continue;
                }
                let child_full = util::join_path(full, &child);
                let child_name = format!("{name}/{child}");
                add_path(archive, &child_full, &child_name, timestamp, verbose)?;
            }
        }
        EntryKind::File => {
            let stat = fs::stat(full).map_err(|err| format!("{full}: {err}"))?;
            let data = match util::read_input(full) {
                Ok(data) => data,
                Err(err) => return Err(format!("{full}: {err}")),
            };
            let size = data.len() as u64;
            if size > MAX_SIZE {
                return Err(format!("{full}: file too large for ustar"));
            }
            let header = build_header(
                name,
                b'0',
                size,
                (stat.mode & 0o7777) as u32,
                timestamp,
                None,
            )?;
            push_header(archive, &header)?;
            if archive.try_reserve(data.len()).is_err() {
                return Err(String::from("out of memory"));
            }
            archive.extend_from_slice(&data);
            let padding = (BLOCK - data.len() % BLOCK) % BLOCK;
            if archive.try_reserve(padding).is_err() {
                return Err(String::from("out of memory"));
            }
            archive.resize(archive.len() + padding, 0);
            if verbose {
                eprintln!("{name}");
            }
        }
        EntryKind::Other => {
            return Err(format!("{full}: unsupported file type"));
        }
    }
    Ok(())
}

fn build_header(
    name: &str,
    typeflag: u8,
    size: u64,
    mode: u32,
    timestamp: u64,
    link: Option<&str>,
) -> Result<[u8; BLOCK], String> {
    if name.as_bytes().contains(&0) {
        return Err(String::from("NUL byte in file name"));
    }
    let (prefix, short_name) = split_name(name)?;
    let mut header = [0u8; BLOCK];
    copy_field(&mut header, 0, 100, short_name.as_bytes());
    copy_field(&mut header, 345, 155, prefix.as_bytes());
    write_octal(&mut header, 100, 8, mode as u64, 0o7777)?;
    write_octal(&mut header, 108, 8, 0, u64::MAX)?;
    write_octal(&mut header, 116, 8, 0, u64::MAX)?;
    write_octal(&mut header, 124, 12, size, MAX_SIZE)?;
    write_octal(&mut header, 136, 12, timestamp, u64::MAX)?;
    header[156] = typeflag;
    if let Some(target) = link {
        if target.as_bytes().contains(&0) || target.len() > 100 {
            return Err(String::from("link target too long for ustar"));
        }
        copy_field(&mut header, 157, 100, target.as_bytes());
    }
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    copy_field(&mut header, 265, 32, b"root");
    copy_field(&mut header, 297, 32, b"root");
    write_octal(&mut header, 329, 8, 0, u64::MAX)?;
    write_octal(&mut header, 337, 8, 0, u64::MAX)?;

    for byte in &mut header[148..156] {
        *byte = b' ';
    }
    let mut checksum = 0u32;
    for byte in &header {
        checksum = checksum.wrapping_add(*byte as u32);
    }
    for (index, digit) in header[148..154].iter_mut().enumerate() {
        *digit = b'0' + ((checksum >> (3 * (5 - index))) & 7) as u8;
    }
    header[154] = 0;
    header[155] = b' ';
    Ok(header)
}

fn split_name(name: &str) -> Result<(&str, &str), String> {
    if name.is_empty() {
        return Err(String::from("empty name"));
    }
    if name.len() <= 100 {
        return Ok(("", name));
    }
    if name.len() > 256 {
        return Err(format!("{name}: name too long for ustar"));
    }
    let bytes = name.as_bytes();
    for position in (1..bytes.len()).rev() {
        if bytes[position] == b'/' {
            let prefix = &name[..position];
            let rest = &name[position + 1..];
            if !rest.is_empty() && prefix.len() <= 155 && rest.len() <= 100 {
                return Ok((prefix, rest));
            }
        }
    }
    Err(format!("{name}: name too long for ustar"))
}

fn copy_field(header: &mut [u8; BLOCK], start: usize, length: usize, value: &[u8]) {
    let end = start + length;
    if end > header.len() || value.len() > length {
        return;
    }
    header[start..start + value.len()].copy_from_slice(value);
}

fn write_octal(
    header: &mut [u8; BLOCK],
    start: usize,
    total: usize,
    value: u64,
    maximum: u64,
) -> Result<(), String> {
    if value > maximum || total < 2 || start + total > header.len() {
        return Err(String::from("numeric field overflow in tar header"));
    }
    let digits = total - 1;
    let mut remaining = value;
    for index in (0..digits).rev() {
        header[start + index] = b'0' + (remaining & 7) as u8;
        remaining >>= 3;
    }
    if remaining != 0 {
        return Err(String::from("numeric field overflow in tar header"));
    }
    header[start + digits] = 0;
    Ok(())
}

fn push_header(archive: &mut Vec<u8>, header: &[u8; BLOCK]) -> Result<(), String> {
    if archive.try_reserve(BLOCK).is_err() {
        return Err(String::from("out of memory"));
    }
    archive.extend_from_slice(header);
    Ok(())
}

/// Lexically normalise an archive name: strip leading `/` and `./`, collapse
/// `//`, and refuse `..` and empty names. The result has no trailing slash.
fn normalize_member(raw: &str) -> Option<String> {
    let mut out = String::new();
    for component in raw.split('/') {
        match component {
            "" | "." => {}
            ".." => return None,
            other => {
                if other.as_bytes().contains(&0) {
                    return None;
                }
                if !out.is_empty() {
                    out.push('/');
                }
                out.push_str(other);
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

// ─── Read / list / extract ────────────────────────────────────────────────

struct Member {
    name: String,
    typeflag: u8,
    size: usize,
    linkname: String,
    data_start: usize,
    next: usize,
}

fn list_archive(program: &str, archive_name: &str, operands: &[Operand]) -> i32 {
    let data = match util::read_input(archive_name) {
        Ok(data) => data,
        Err(err) => {
            util::report(program, util::display_name(archive_name), err);
            return util::EXIT_FAILURE;
        }
    };
    let selections: Vec<&str> = operands.iter().map(|operand| operand.name.as_str()).collect();
    let mut offset = 0usize;
    let mut saw_member = false;
    loop {
        let member = match next_member(&data, offset) {
            Ok(Some(member)) => member,
            Ok(None) => break,
            Err(message) => {
                eprintln!("{program}: {message}");
                return util::EXIT_FAILURE;
            }
        };
        saw_member = true;
        if selected(&member.name, &selections) {
            if let Err(err) = util::write_line(1, member.name.as_bytes()) {
                eprintln!("{program}: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
        offset = member.next;
    }
    if !saw_member && !data.is_empty() && !data.iter().all(|byte| *byte == 0) {
        eprintln!("{program}: {archive_name}: no tar members found");
        return util::EXIT_FAILURE;
    }
    util::EXIT_OK
}

fn extract_archive(
    program: &str,
    archive_name: &str,
    operands: &[Operand],
    base: &str,
    verbose: bool,
) -> i32 {
    let data = match util::read_input(archive_name) {
        Ok(data) => data,
        Err(err) => {
            util::report(program, util::display_name(archive_name), err);
            return util::EXIT_FAILURE;
        }
    };
    let selections: Vec<&str> = operands.iter().map(|operand| operand.name.as_str()).collect();
    let mut offset = 0usize;
    let mut status = util::EXIT_OK;
    loop {
        let member = match next_member(&data, offset) {
            Ok(Some(member)) => member,
            Ok(None) => break,
            Err(message) => {
                eprintln!("{program}: {message}");
                return util::EXIT_FAILURE;
            }
        };
        offset = member.next;
        if !selected(&member.name, &selections) {
            continue;
        }
        if member.name.starts_with('/') {
            eprintln!("{program}: {}: absolute archive member", member.name);
            status = util::EXIT_FAILURE;
            continue;
        }
        let Some(clean_name) = normalize_member(member.name.trim_end_matches('/')) else {
            eprintln!("{program}: {}: unsafe archive member", member.name);
            status = util::EXIT_FAILURE;
            continue;
        };
        let target = if base.is_empty() {
            clean_name.clone()
        } else {
            util::join_path(base, &clean_name)
        };

        let result = match member.typeflag {
            b'0' | 0 => extract_regular(&data, &member, &target),
            b'5' => ensure_dir_all(&target).map_err(|err| format!("{target}: {err}")),
            b'2' => extract_symlink(&member, &target),
            other => Err(format!(
                "{}: unsupported tar entry type '{}'",
                member.name,
                other as char
            )),
        };
        match result {
            Ok(()) => {
                if verbose {
                    eprintln!("{}", member.name);
                }
            }
            Err(message) => {
                eprintln!("{program}: {message}");
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}

fn extract_regular(data: &[u8], member: &Member, target: &str) -> Result<(), String> {
    if member.size > 0 && data.len().saturating_sub(member.data_start) < member.size {
        return Err(format!("{}: truncated member data", member.name));
    }
    create_parents(target).map_err(|err| format!("{target}: {err}"))?;
    let end = member.data_start.saturating_add(member.size);
    if end > data.len() {
        return Err(format!("{}: truncated member data", member.name));
    }
    fs::write(target, &data[member.data_start..end]).map_err(|err| format!("{target}: {err}"))
}

fn extract_symlink(member: &Member, target: &str) -> Result<(), String> {
    // Do not create a link that points outside the extraction root: a later
    // member could otherwise write through it. Relative, downward links (the
    // common `link -> file` case) are fine.
    if member.linkname.starts_with('/')
        || member
            .linkname
            .split('/')
            .any(|component| component == "..")
    {
        return Err(format!(
            "{}: unsafe symbolic-link target '{}'",
            member.name, member.linkname
        ));
    }
    create_parents(target).map_err(|err| format!("{target}: {err}"))?;
    match fs::remove_file(target) {
        Ok(()) => {}
        Err(Errno::ENOENT) => {}
        Err(err) => {
            return Err(format!("{target}: {err}"));
        }
    }
    fs::symlink(&member.linkname, target).map_err(|err| format!("{target}: {err}"))
}

fn create_parents(path: &str) -> Result<(), Errno> {
    if let Some(position) = path.rfind('/') {
        if position > 0 {
            ensure_dir_all(&path[..position])?;
        }
    }
    Ok(())
}

/// `mkdir -p`, without assuming that `mkdirat` on an existing directory is
/// `EEXIST`.
///
/// The 0.16.1 VFS answers `ENOTSUP` for `mkdirat("/mnt")` (the mount root) even
/// though the directory exists, which makes `libpagh::fs::create_dir_all` fail
/// for every absolute path below `/mnt`. Checking `stat` first is both more
/// portable and avoids that dead end; it also gives the usual "file in the
/// way" answer for an existing non-directory.
fn ensure_dir_all(path: &str) -> Result<(), Errno> {
    if path.is_empty() || path == "/" {
        return Ok(());
    }
    match fs::stat(path) {
        Ok(stat) if stat.entry_kind() == EntryKind::Dir => return Ok(()),
        Ok(_) => return Err(Errno::EEXIST),
        Err(Errno::ENOENT) => {}
        Err(error) => return Err(error),
    }
    if let Some(position) = path.rfind('/') {
        if position > 0 {
            ensure_dir_all(&path[..position])?;
        }
    }
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(Errno::EEXIST) => match fs::stat(path) {
            Ok(stat) if stat.entry_kind() == EntryKind::Dir => Ok(()),
            Ok(_) => Err(Errno::EEXIST),
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    }
}

fn selected(name: &str, selections: &[&str]) -> bool {
    if selections.is_empty() {
        return true;
    }
    let member = name.trim_end_matches('/');
    for selection in selections {
        let wanted = selection.trim_end_matches('/');
        if member == wanted {
            return true;
        }
        if let Some(rest) = member.strip_prefix(wanted) {
            if rest.starts_with('/') {
                return true;
            }
        }
    }
    false
}

fn next_member(data: &[u8], offset: usize) -> Result<Option<Member>, String> {
    if offset >= data.len() {
        return Ok(None);
    }
    if data.len().saturating_sub(offset) < BLOCK {
        return Err(String::from("truncated tar header"));
    }
    let header = &data[offset..offset + BLOCK];
    if header.iter().all(|byte| *byte == 0) {
        return Ok(None);
    }
    if &header[257..262] != b"ustar" {
        return Err(String::from("not a ustar archive"));
    }
    let stored = parse_octal(&header[148..156])
        .ok_or_else(|| String::from("invalid tar header checksum field"))?;
    let mut computed = 0u32;
    for (index, byte) in header.iter().enumerate() {
        let value = if (148..156).contains(&index) {
            b' ' as u32
        } else {
            *byte as u32
        };
        computed = computed.wrapping_add(value);
    }
    if computed != stored as u32 {
        return Err(String::from("tar header checksum mismatch"));
    }

    let name_field = c_string(&header[0..100]);
    let prefix = c_string(&header[345..500]);
    let name = if prefix.is_empty() {
        String::from(name_field)
    } else {
        format!("{prefix}/{name_field}")
    };
    if name.is_empty() {
        return Err(String::from("tar member with empty name"));
    }
    let size = parse_octal(&header[124..136])
        .ok_or_else(|| String::from("invalid tar size field"))?;
    let size = usize::try_from(size).map_err(|_| String::from("tar member too large"))?;
    let typeflag = match header[156] {
        0 => b'0',
        other => other,
    };
    let linkname = String::from(c_string(&header[157..257]));
    let data_start = offset + BLOCK;
    let padded = size
        .checked_add(BLOCK - 1)
        .map(|value| value / BLOCK * BLOCK)
        .ok_or_else(|| String::from("tar member size overflow"))?;
    let next = data_start
        .checked_add(padded)
        .ok_or_else(|| String::from("tar member size overflow"))?;
    if next > data.len() {
        return Err(String::from("truncated tar member data"));
    }
    Ok(Some(Member {
        name,
        typeflag,
        size,
        linkname,
        data_start,
        next,
    }))
}

fn c_string(field: &[u8]) -> &str {
    let end = field.iter().position(|byte| *byte == 0).unwrap_or(field.len());
    core::str::from_utf8(&field[..end]).unwrap_or("")
}

fn parse_octal(field: &[u8]) -> Option<u64> {
    let mut value = 0u64;
    let mut seen = false;
    for byte in field {
        match *byte {
            0 | b' ' => {
                if seen {
                    break;
                }
            }
            b'0'..=b'7' => {
                value = value.checked_mul(8)?.checked_add((*byte - b'0') as u64)?;
                seen = true;
            }
            _ => return None,
        }
    }
    if seen {
        Some(value)
    } else {
        None
    }
}