//! `patch` — apply a unified diff (`---`/`+++`/`@@`) to one file.
//!
//! The applet implements real hunk application, not a byte-copy:
//!
//! * the target comes from the `+++` header (or is `/dev/null`-created from a
//!   `--- /dev/null` source); `-p N` strips N leading components;
//! * each `@@ -a,b +c,d @@` hunk is matched against the source line by line;
//!   every context (` `) and deletion (`-`) line must equal the source text,
//!   otherwise the hunk fails (no fuzz, no offset search);
//! * additions (`+`) are inserted, deletions removed, and both `old_count` and
//!   `new_count` are cross-checked against the body;
//! * `\ No newline at end of file` is honoured for context and added lines.
//!
//! `-p N` (default 0) strips leading path components, `-o OUT` writes the
//! result elsewhere (`-` is stdout) and otherwise the target is patched in
//! place after a `.orig` backup is written. The patch is read from `-i FILE`,
//! a single operand, or stdin. Only one file per patch is supported; git
//! binary patches, C-style context diffs, renames and metadata lines are not.
//! Exit: 0 success, 1 malformed/failed/read/write error, 2 usage.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;

use crate::util;

const USAGE: &str = "usage: patch [-p N] [-o OUT] [-i FILE] [PATCHFILE]";

struct SourceLine<'a> {
    data: &'a [u8],
    newline: bool,
}

struct HunkLine<'a> {
    kind: u8,
    data: &'a [u8],
    newline: bool,
}

struct Hunk<'a> {
    old_start: usize,
    old_count: usize,
    lines: Vec<HunkLine<'a>>,
}

pub fn run(args: &[String]) -> i32 {
    let mut strip = 0usize;
    let mut output: Option<String> = None;
    let mut input: Option<String> = None;
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
            if let Some(value) = arg.strip_prefix("--strip=") {
                let Some(parsed) = value.parse::<usize>().ok() else {
                    eprintln!("patch: invalid strip count: '{value}'");
                    return util::EXIT_USAGE;
                };
                strip = parsed;
            } else if arg == "--strip" {
                let Some(value) = args.get(index).map(String::as_str) else {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                };
                index += 1;
                let Some(parsed) = value.parse::<usize>().ok() else {
                    eprintln!("patch: invalid strip count: '{value}'");
                    return util::EXIT_USAGE;
                };
                strip = parsed;
            } else if let Some(value) = arg.strip_prefix("--output=") {
                output = Some(String::from(value));
            } else if arg == "--output" {
                let Some(value) = args.get(index) else {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                };
                index += 1;
                output = Some(value.clone());
            } else if let Some(value) = arg.strip_prefix("--input=") {
                input = Some(String::from(value));
            } else if arg == "--input" {
                let Some(value) = args.get(index) else {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                };
                index += 1;
                input = Some(value.clone());
            } else if arg == "--force" || arg == "--batch" {
                // Accepted for compatibility: this port always overwrites the
                // `.orig` backup and never asks questions.
            } else {
                eprintln!("patch: unrecognized option '{arg}'");
                return usage();
            }
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            let body = &arg[1..];
            let Some(option) = body.chars().next() else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            let rest = &body[option.len_utf8()..];
            if option == 'f' {
                if !rest.is_empty() {
                    eprintln!("patch: invalid option -- 'f'");
                    return usage();
                }
                // No-op compatibility flag; see the long form above.
                continue;
            }
            let value = if rest.is_empty() {
                let Some(value) = args.get(index).map(String::as_str) else {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                };
                index += 1;
                value
            } else {
                rest
            };
            match option {
                'p' => match value.parse::<usize>() {
                    Ok(parsed) => strip = parsed,
                    Err(_) => {
                        eprintln!("patch: invalid strip count: '{value}'");
                        return util::EXIT_USAGE;
                    }
                },
                'o' => output = Some(String::from(value)),
                'i' => input = Some(String::from(value)),
                _ => {
                    eprintln!("patch: invalid option -- '{option}'");
                    return usage();
                }
            }
            continue;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("patch: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.len() > 1 || (input.is_some() && !operands.is_empty()) {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let patch_name = input
        .as_deref()
        .or_else(|| operands.first().copied())
        .unwrap_or("-");
    let patch = match util::read_input(patch_name) {
        Ok(data) => data,
        Err(err) => {
            util::report("patch", util::display_name(patch_name), err);
            return util::EXIT_FAILURE;
        }
    };

    let parsed = match parse_patch(&patch, strip) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("patch: {message}");
            return util::EXIT_FAILURE;
        }
    };
    if parsed.hunks.is_empty() {
        eprintln!("patch: no hunks found");
        return util::EXIT_FAILURE;
    }
    if parsed.new_path == "/dev/null" {
        eprintln!("patch: deleting a file is not supported");
        return util::EXIT_FAILURE;
    }
    if parsed.new_path.is_empty() {
        eprintln!("patch: empty target file name");
        return util::EXIT_FAILURE;
    }

    let source = if parsed.old_path == "/dev/null" {
        Vec::new()
    } else {
        match util::read_input(&parsed.new_path) {
            Ok(data) => data,
            Err(err) => {
                util::report("patch", &parsed.new_path, err);
                return util::EXIT_FAILURE;
            }
        }
    };

    let source_lines = match split_source(&source) {
        Some(lines) => lines,
        None => {
            eprintln!("patch: out of memory");
            return util::EXIT_FAILURE;
        }
    };
    let patched = match apply_hunks(&source_lines, &parsed.hunks) {
        Ok(data) => data,
        Err(message) => {
            eprintln!("patch: {message}");
            return util::EXIT_FAILURE;
        }
    };

    match output.as_deref() {
        Some("-") => {
            if let Err(err) = util::write_all(1, &patched) {
                eprintln!("patch: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
        Some(name) => {
            if let Err(err) = fs::write(name, &patched) {
                util::report("patch", name, err);
                return util::EXIT_FAILURE;
            }
        }
        None => {
            let backup = format!("{}.orig", parsed.new_path);
            if let Err(err) = fs::write(&backup, &source) {
                util::report("patch", &backup, err);
                return util::EXIT_FAILURE;
            }
            if let Err(err) = fs::write(&parsed.new_path, &patched) {
                util::report("patch", &parsed.new_path, err);
                return util::EXIT_FAILURE;
            }
        }
    }
    util::EXIT_OK
}

fn usage() -> i32 {
    eprintln!("{USAGE}");
    util::EXIT_USAGE
}

struct ParsedPatch<'a> {
    old_path: String,
    new_path: String,
    hunks: Vec<Hunk<'a>>,
}

fn parse_patch(data: &[u8], strip: usize) -> Result<ParsedPatch<'_>, String> {
    let mut lines: Vec<&[u8]> = Vec::new();
    lines.try_reserve(data.len() / 16 + 1).map_err(|_| "out of memory")?;
    for line in util::lines(data) {
        lines.push(line);
    }

    let mut index = 0usize;
    while index < lines.len() && !lines[index].starts_with(b"--- ") {
        index += 1;
    }
    if index >= lines.len() {
        return Err(String::from("missing '---' header"));
    }
    let old_raw = header_path(lines[index], b"--- ").ok_or("malformed '---' header")?;
    index += 1;
    if index >= lines.len() || !lines[index].starts_with(b"+++ ") {
        return Err(String::from("missing '+++' header"));
    }
    let new_raw = header_path(lines[index], b"+++ ").ok_or("malformed '+++' header")?;
    index += 1;

    // `/dev/null` is the unified-diff marker for "no source file", not a real
    // path component; do not run it through -p stripping.
    let old_path = if old_raw == "/dev/null" {
        String::from("/dev/null")
    } else {
        strip_path(old_raw, strip)?
    };
    let new_path = if new_raw == "/dev/null" {
        String::from("/dev/null")
    } else {
        strip_path(new_raw, strip)?
    };
    let mut hunks: Vec<Hunk<'_>> = Vec::new();

    while index < lines.len() {
        if lines[index].is_empty() {
            index += 1;
            continue;
        }
        if lines[index].starts_with(b"--- ") || lines[index].starts_with(b"diff ") {
            return Err(String::from("multi-file patches are not supported"));
        }
        if !lines[index].starts_with(b"@@ ") {
            return Err(format!(
                "unexpected line {} in patch: '{}'",
                index + 1,
                printable(lines[index])
            ));
        }
        let (old_start, old_count, _new_start, new_count) =
            parse_hunk_header(lines[index]).ok_or_else(|| {
                format!(
                    "malformed hunk header at line {}: '{}'",
                    index + 1,
                    printable(lines[index])
                )
            })?;
        if old_count > 0 && old_start == 0 {
            return Err(format!("invalid old range at line {}", index + 1));
        }
        index += 1;

        let mut hunk_lines: Vec<HunkLine<'_>> = Vec::new();
        let mut old_seen = 0usize;
        let mut new_seen = 0usize;
        let mut previous: Option<usize> = None;

        // A hunk body is exactly old_count old lines and new_count new lines;
        // once both are consumed the next line is a header, a marker or the
        // end of the patch. Prefix checks alone would confuse a deleted line
        // whose text starts with "-- " with the next file's "--- " header.
        while old_seen < old_count || new_seen < new_count {
            if index >= lines.len() {
                return Err(format!(
                    "hunk at line {} is truncated (old {old_seen}/{old_count}, new {new_seen}/{new_count})",
                    index.saturating_sub(1),
                ));
            }
            let line = lines[index];
            if line.is_empty() {
                return Err(format!("blank line inside hunk at line {}", index + 1));
            }
            match line[0] {
                b' ' => {
                    hunk_lines.try_reserve(1).map_err(|_| "out of memory")?;
                    previous = Some(hunk_lines.len());
                    hunk_lines.push(HunkLine {
                        kind: b' ',
                        data: &line[1..],
                        newline: true,
                    });
                    old_seen += 1;
                    new_seen += 1;
                }
                b'-' => {
                    hunk_lines.try_reserve(1).map_err(|_| "out of memory")?;
                    previous = Some(hunk_lines.len());
                    hunk_lines.push(HunkLine {
                        kind: b'-',
                        data: &line[1..],
                        newline: true,
                    });
                    old_seen += 1;
                }
                b'+' => {
                    hunk_lines.try_reserve(1).map_err(|_| "out of memory")?;
                    previous = Some(hunk_lines.len());
                    hunk_lines.push(HunkLine {
                        kind: b'+',
                        data: &line[1..],
                        newline: true,
                    });
                    new_seen += 1;
                }
                b'\\' => {
                    let Some(last) = previous else {
                        return Err(format!("stray '\\' marker at line {}", index + 1));
                    };
                    hunk_lines[last].newline = false;
                }
                _ => {
                    return Err(format!(
                        "invalid hunk line {}: '{}'",
                        index + 1,
                        printable(line)
                    ));
                }
            }
            index += 1;
        }

        // "\ No newline at end of file" markers belong to the hunk even
        // though they do not count toward either range.
        while index < lines.len() && lines[index].starts_with(b"\\") {
            let Some(last) = previous else {
                return Err(format!("stray '\\' marker at line {}", index + 1));
            };
            hunk_lines[last].newline = false;
            index += 1;
        }

        if old_seen != old_count {
            return Err(format!(
                "hunk at line {}: old count {old_seen} != header {old_count}",
                index
            ));
        }
        if new_seen != new_count {
            return Err(format!(
                "hunk at line {}: new count {new_seen} != header {new_count}",
                index
            ));
        }
        hunks.try_reserve(1).map_err(|_| "out of memory")?;
        hunks.push(Hunk {
            old_start,
            old_count,
            lines: hunk_lines,
        });
    }

    Ok(ParsedPatch {
        old_path,
        new_path,
        hunks,
    })
}

fn header_path<'a>(line: &'a [u8], prefix: &[u8]) -> Option<&'a str> {
    let rest = line.strip_prefix(prefix)?;
    let rest = if let Some(tab) = rest.iter().position(|byte| *byte == b'\t') {
        &rest[..tab]
    } else {
        rest
    };
    let text = core::str::from_utf8(rest).ok()?;
    let text = text.trim_end_matches('\r').trim_end();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn strip_path(path: &str, count: usize) -> Result<String, String> {
    if count == 0 {
        return Ok(String::from(path));
    }
    let mut slashes = 0usize;
    for (position, byte) in path.bytes().enumerate() {
        if byte == b'/' {
            slashes += 1;
            if slashes == count {
                let rest = &path[position + 1..];
                if rest.is_empty() {
                    return Err(format!("cannot strip {count} components from '{path}'"));
                }
                return Ok(String::from(rest));
            }
        }
    }
    Err(format!("cannot strip {count} components from '{path}'"))
}

fn parse_hunk_header(line: &[u8]) -> Option<(usize, usize, usize, usize)> {
    let text = core::str::from_utf8(line).ok()?;
    let rest = text.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(' ')?;
    let rest = rest.strip_prefix('+')?;
    let (new, rest) = rest.split_once(' ')?;
    if !rest.starts_with("@@") {
        return None;
    }
    let (old_start, old_count) = parse_range(old)?;
    let (new_start, new_count) = parse_range(new)?;
    Some((old_start, old_count, new_start, new_count))
}

fn parse_range(text: &str) -> Option<(usize, usize)> {
    let (start, count) = match text.split_once(',') {
        Some((start, count)) => (start, Some(count)),
        None => (text, None),
    };
    let start = start.parse::<usize>().ok()?;
    let count = match count {
        Some(value) => value.parse::<usize>().ok()?,
        None => 1,
    };
    Some((start, count))
}

fn split_source(data: &[u8]) -> Option<Vec<SourceLine<'_>>> {
    let mut lines: Vec<SourceLine<'_>> = Vec::new();
    lines.try_reserve(data.len() / 8 + 1).ok()?;
    let mut start = 0usize;
    for (position, byte) in data.iter().enumerate() {
        if *byte == b'\n' {
            lines.push(SourceLine {
                data: &data[start..position],
                newline: true,
            });
            start = position + 1;
        }
    }
    if start < data.len() {
        lines.push(SourceLine {
            data: &data[start..],
            newline: false,
        });
    }
    Some(lines)
}

fn apply_hunks(source: &[SourceLine<'_>], hunks: &[Hunk<'_>]) -> Result<Vec<u8>, String> {
    let mut output: Vec<u8> = Vec::new();
    output
        .try_reserve(source.len() * 8)
        .map_err(|_| "out of memory")?;
    let mut source_position = 0usize;

    for (number, hunk) in hunks.iter().enumerate() {
        let position = if hunk.old_count == 0 {
            hunk.old_start
        } else {
            hunk.old_start - 1
        };
        if position < source_position {
            return Err(format!(
                "hunk #{} overlaps a previous hunk (old_start {})",
                number + 1,
                hunk.old_start
            ));
        }
        if position > source.len() {
            return Err(format!(
                "hunk #{} starts past end of file (old_start {}, {} lines)",
                number + 1,
                hunk.old_start,
                source.len()
            ));
        }
        while source_position < position {
            append_line(&mut output, source[source_position].data, source[source_position].newline)?;
            source_position += 1;
        }

        for line in &hunk.lines {
            match line.kind {
                b' ' => {
                    let Some(actual) = source.get(source_position) else {
                        return Err(format!("hunk #{}: context past end of file", number + 1));
                    };
                    if actual.data != line.data {
                        return Err(format!(
                            "hunk #{} failed at source line {}: expected '{}', found '{}'",
                            number + 1,
                            source_position + 1,
                            printable(line.data),
                            printable(actual.data)
                        ));
                    }
                    append_line(&mut output, actual.data, actual.newline)?;
                    source_position += 1;
                }
                b'-' => {
                    let Some(actual) = source.get(source_position) else {
                        return Err(format!("hunk #{}: deletion past end of file", number + 1));
                    };
                    if actual.data != line.data {
                        return Err(format!(
                            "hunk #{} failed at source line {}: expected '{}', found '{}'",
                            number + 1,
                            source_position + 1,
                            printable(line.data),
                            printable(actual.data)
                        ));
                    }
                    source_position += 1;
                }
                b'+' => {
                    append_line(&mut output, line.data, line.newline)?;
                }
                _ => return Err(format!("hunk #{}: internal bad line kind", number + 1)),
            }
        }
    }
    while source_position < source.len() {
        append_line(&mut output, source[source_position].data, source[source_position].newline)?;
        source_position += 1;
    }
    Ok(output)
}

fn append_line(output: &mut Vec<u8>, data: &[u8], newline: bool) -> Result<(), String> {
    output.try_reserve(data.len() + 1).map_err(|_| "out of memory")?;
    output.extend_from_slice(data);
    if newline {
        output.push(b'\n');
    }
    Ok(())
}

fn printable(data: &[u8]) -> String {
    let mut out = String::new();
    let limit = data.len().min(80);
    for byte in &data[..limit] {
        match *byte {
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(*byte as char),
            other => out.push_str(&format!("\\x{other:02x}")),
        }
    }
    out
}