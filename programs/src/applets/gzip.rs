//! `gzip` — a real gzip container with DEFLATE **stored** blocks.
//!
//! This is an honest, deliberately small gzip implementation: the output is a
//! valid `1f 8b 08` member with a correct CRC-32 and ISIZE trailer, but every
//! DEFLATE block uses BTYPE=00 (stored), so the payload is not compressed. It
//! exists so the bundle can produce and consume the format the rest of the
//! world uses without pulling in a compressor; the size of a `.gz` written
//! here is the input size plus 18 header/trailer bytes and 5 bytes per 64 KiB
//! block.
//!
//! Supported: `-d`/`--decompress`, `-c`/`--stdout`, `-k`/`--keep`,
//! `-f`/`--force`, `-1`..`-9` (accepted, no effect — stored blocks are fixed),
//! optional gzip header fields (FEXTRA/FNAME/FCOMMENT/FHCRC are skipped),
//! concatenated members, and the naming rules `FILE` -> `FILE.gz` /
//! `FILE.gz` -> `FILE`. Decompression rejects compressed DEFLATE blocks
//! (BTYPE 1/2), a bad CRC/ISIZE, truncated input and a reserved FLG bit.
//!
//! `-` is stdin/stdout; no operand also means stdin/stdout. Without `-c`, a
//! named input is replaced by its output and removed unless `-k` is given;
//! an existing output is refused unless `-f` is given. Exit: 0 success,
//! 1 data/I/O error, 2 usage.
//!
//! This is not GNU gzip: no `-l`, `-t`, `-r`, `-S`, timestamps (MTIME is 0),
//! compression levels or seekable output.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;

use crate::util;

const USAGE: &str = "usage: gzip [-d] [-c] [-k] [-f] [FILE...]";

pub fn run(args: &[String]) -> i32 {
    run_with("gzip", args, false)
}

/// Shared entry point for `gzip` and `gunzip` (`force_decompress`).
///
/// `gunzip` is a separate applet name, but its semantics are exactly
/// `gzip -d`, including the diagnostics prefix.
pub fn run_with(program: &str, args: &[String], force_decompress: bool) -> i32 {
    let mut decompress = force_decompress;
    let mut to_stdout = false;
    let mut keep = false;
    let mut force = false;
    let mut operands: Vec<&str> = Vec::new();
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
            match arg {
                "--decompress" | "--uncompress" => decompress = true,
                "--stdout" | "--to-stdout" => to_stdout = true,
                "--keep" => keep = true,
                "--force" => force = true,
                "--fast" | "--best" => {}
                _ => {
                    eprintln!("{program}: unrecognized option '{arg}'");
                    return usage(program);
                }
            }
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            let mut valid = true;
            for option in arg[1..].chars() {
                match option {
                    'd' => decompress = true,
                    'c' => to_stdout = true,
                    'k' => keep = true,
                    'f' => force = true,
                    '1'..='9' => {}
                    _ => valid = false,
                }
            }
            if !valid {
                eprintln!("{program}: invalid option -- '{}'", &arg[1..]);
                return usage(program);
            }
            continue;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("{program}: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.is_empty() {
        operands.push("-");
    }

    let mut status = util::EXIT_OK;
    for name in operands {
        if name == "-" {
            if let Err(err) = process_stdin(program, decompress) {
                eprintln!("{program}: standard input: {err}");
                status = util::EXIT_FAILURE;
            }
            continue;
        }
        if !to_stdout && decompress {
            let Some(stripped) = name.strip_suffix(".gz") else {
                eprintln!("{program}: {name}: unknown suffix -- ignored");
                status = util::EXIT_FAILURE;
                continue;
            };
            if stripped.is_empty() {
                eprintln!("{program}: {name}: unknown suffix -- ignored");
                status = util::EXIT_FAILURE;
                continue;
            }
        }
        if !to_stdout && !decompress && name.ends_with(".gz") {
            eprintln!("{program}: {name} already has .gz suffix -- unchanged");
            status = util::EXIT_FAILURE;
            continue;
        }

        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report(program, name, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };

        let result = if decompress {
            decode_stream(&data)
        } else {
            encode_stream(&data)
        };
        let output = match result {
            Ok(output) => output,
            Err(message) => {
                eprintln!("{program}: {name}: {message}");
                status = util::EXIT_FAILURE;
                continue;
            }
        };

        if to_stdout {
            if let Err(err) = util::write_all(1, &output) {
                eprintln!("{program}: write error: {err}");
                return util::EXIT_FAILURE;
            }
            continue;
        }

        let output_name = if decompress {
            match name.strip_suffix(".gz") {
                Some(stripped) if !stripped.is_empty() => stripped,
                _ => {
                    eprintln!("{program}: {name}: unknown suffix -- ignored");
                    status = util::EXIT_FAILURE;
                    continue;
                }
            }
        } else {
            // The owned name has to outlive the write; build it here.
            ""
        };
        let owned_name;
        let target = if decompress {
            output_name
        } else {
            owned_name = alloc::format!("{name}.gz");
            owned_name.as_str()
        };

        if !force && fs::stat(target).is_ok() {
            eprintln!("{program}: {target} already exists; not overwritten");
            status = util::EXIT_FAILURE;
            continue;
        }
        if let Err(err) = fs::write(target, &output) {
            util::report(program, target, err);
            status = util::EXIT_FAILURE;
            continue;
        }
        if !keep {
            if let Err(err) = fs::remove_file(name) {
                util::report(program, name, err);
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}

fn usage(program: &str) -> i32 {
    if program == "gunzip" {
        eprintln!("usage: gunzip [-c] [-k] [-f] [FILE...]");
    } else {
        eprintln!("{USAGE}");
    }
    util::EXIT_USAGE
}

fn process_stdin(program: &str, decompress: bool) -> Result<(), String> {
    let data = util::read_input("-").map_err(|err| alloc::format!("{err}"))?;
    let output = if decompress {
        decode_stream(&data)
    } else {
        encode_stream(&data)
    }
    .map_err(String::from)?;
    util::write_all(1, &output)
        .map_err(|err| alloc::format!("write error: {err}"))?;
    let _ = program;
    Ok(())
}

fn encode_stream(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    let block_count = data.len() / 65535 + 1;
    let capacity = 10usize
        .saturating_add(8)
        .saturating_add(data.len())
        .saturating_add(block_count.saturating_mul(5));
    let mut out: Vec<u8> = Vec::new();
    out.try_reserve(capacity).map_err(|_| "out of memory")?;

    // A fixed header: no name/comment, MTIME=0 (reproducible), OS=Unix.
    out.extend_from_slice(&[0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0x03]);

    let mut offset = 0usize;
    loop {
        let remaining = data.len().saturating_sub(offset);
        let chunk_len = remaining.min(65535);
        let final_block = offset + chunk_len >= data.len();
        out.push(if final_block { 0x01 } else { 0x00 });
        let length = chunk_len as u16;
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&(!length).to_le_bytes());
        out.extend_from_slice(&data[offset..offset + chunk_len]);
        offset += chunk_len;
        if final_block {
            break;
        }
    }

    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    Ok(out)
}

fn decode_stream(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    if data.is_empty() {
        return Err("empty input");
    }
    let mut out: Vec<u8> = Vec::new();
    let mut offset = 0usize;
    let mut members = 0usize;
    while offset < data.len() {
        // Tolerate zero padding after the last member.
        if data[offset..].iter().all(|byte| *byte == 0) {
            break;
        }
        let (member, next) = decode_member(data, offset)?;
        if out.try_reserve(member.len()).is_err() {
            return Err("out of memory");
        }
        out.extend_from_slice(&member);
        members += 1;
        offset = next;
    }
    if members == 0 {
        return Err("no gzip member");
    }
    Ok(out)
}

fn decode_member(data: &[u8], start: usize) -> Result<(Vec<u8>, usize), &'static str> {
    let mut position = start;
    if data.len().saturating_sub(position) < 10 {
        return Err("truncated gzip header");
    }
    if data[position] != 0x1f || data[position + 1] != 0x8b {
        return Err("bad gzip magic");
    }
    if data[position + 2] != 0x08 {
        return Err("unsupported compression method");
    }
    let flags = data[position + 3];
    if flags & 0xE0 != 0 {
        return Err("reserved gzip header flag set");
    }
    position += 10;

    if flags & 0x04 != 0 {
        if data.len().saturating_sub(position) < 2 {
            return Err("truncated gzip extra field");
        }
        let extra = u16::from_le_bytes([data[position], data[position + 1]]) as usize;
        position += 2;
        if data.len().saturating_sub(position) < extra {
            return Err("truncated gzip extra field");
        }
        position += extra;
    }
    if flags & 0x08 != 0 {
        position = skip_c_string(data, position)?;
    }
    if flags & 0x10 != 0 {
        position = skip_c_string(data, position)?;
    }
    if flags & 0x02 != 0 {
        if data.len().saturating_sub(position) < 2 {
            return Err("truncated gzip header CRC");
        }
        position += 2;
    }

    let mut out: Vec<u8> = Vec::new();
    loop {
        let header = *data.get(position).ok_or("truncated DEFLATE block header")?;
        position += 1;
        let final_block = header & 0x01 != 0;
        let block_type = (header >> 1) & 0x03;
        if block_type != 0 {
            return Err("compressed DEFLATE blocks are not supported");
        }
        if data.len().saturating_sub(position) < 4 {
            return Err("truncated stored block header");
        }
        let length = u16::from_le_bytes([data[position], data[position + 1]]) as usize;
        let complement = u16::from_le_bytes([data[position + 2], data[position + 3]]);
        position += 4;
        if complement != !(length as u16) {
            return Err("bad stored block length complement");
        }
        if data.len().saturating_sub(position) < length {
            return Err("truncated stored block");
        }
        if out.try_reserve(length).is_err() {
            return Err("out of memory");
        }
        out.extend_from_slice(&data[position..position + length]);
        position += length;
        if final_block {
            break;
        }
    }

    if data.len().saturating_sub(position) < 8 {
        return Err("truncated gzip trailer");
    }
    let expected_crc = u32::from_le_bytes([
        data[position],
        data[position + 1],
        data[position + 2],
        data[position + 3],
    ]);
    let expected_size = u32::from_le_bytes([
        data[position + 4],
        data[position + 5],
        data[position + 6],
        data[position + 7],
    ]);
    position += 8;
    if crc32(&out) != expected_crc {
        return Err("CRC-32 mismatch");
    }
    if (out.len() as u32) != expected_size {
        return Err("ISIZE mismatch");
    }
    Ok((out, position))
}

fn skip_c_string(data: &[u8], start: usize) -> Result<usize, &'static str> {
    let mut position = start;
    while position < data.len() {
        if data[position] == 0 {
            return Ok(position + 1);
        }
        position += 1;
    }
    Err("unterminated gzip header string")
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}