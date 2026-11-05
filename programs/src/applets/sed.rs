//! `sed` — a small stream editor.
//!
//! The accepted script is deliberately a subset, documented here rather than
//! guessed at run time:
//!
//! * an address is `N` or `N,M` (1-based decimal line numbers);
//! * `s/RE/REP/[g]` substitutes, with any byte as the delimiter (`s|a|b|`);
//! * `p` prints the pattern space, `d` deletes it and starts the next line;
//! * commands may be separated by `;` or a newline;
//! * `-n` suppresses the automatic print at the end of each cycle.
//!
//! `RE` is the BRE-lite subset from [`super::bre`] (`^ $ . * []` and `\`); it
//! has no groups, alternation or back-references. In `REP`, `&` expands to the
//! whole match, `\&` is a literal `&`, `\\` a backslash, and `\n`/`\t` are a
//! newline/tab. A backslash before a digit is rejected as a usage error since
//! there are no capture groups to refer to.
//!
//! There is no `-i` (in-place), no `-e`/`-f`, no `y`, `a`, `i`, `c`, `r`, `w`,
//! no `$`/regex addresses and no branch commands. Files are read in order as
//! one stream; `-` is stdin. Exit: 0 success, 1 read/write error, 2 usage or
//! invalid script.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use super::bre::Pattern;
use crate::util;

const USAGE: &str = "usage: sed [-n] SCRIPT [FILE...]";

struct Command {
    /// `None` means every line; otherwise an inclusive 1-based range.
    address: Option<(usize, usize)>,
    kind: Kind,
}

enum Kind {
    Substitute {
        pattern: Pattern,
        replacement: Vec<u8>,
        global: bool,
    },
    Print,
    Delete,
}

impl Command {
    fn applies(&self, line: usize) -> bool {
        match self.address {
            None => true,
            Some((first, last)) => line >= first && line <= last,
        }
    }
}

pub fn run(args: &[String]) -> i32 {
    let mut suppress = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg == "-n" {
            suppress = true;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("sed: out of memory");
            return util::EXIT_FAILURE;
        }
        parse_options = false;
    }

    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let script = operands[0];
    let mut files: Vec<&str> = Vec::new();
    for name in &operands[1..] {
        if util::push(&mut files, *name).is_err() {
            eprintln!("sed: out of memory");
            return util::EXIT_USAGE;
        }
    }
    if files.is_empty() {
        files.push("-");
    }

    let commands = match parse_script(script) {
        Some(commands) => commands,
        None => {
            eprintln!("sed: invalid script: '{script}'");
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
    };

    let mut status = util::EXIT_OK;
    let mut line_number = 0usize;

    for name in files {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("sed", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };

        for line in util::lines(&data) {
            line_number = line_number.saturating_add(1);
            let mut text: Vec<u8> = Vec::new();
            if text.try_reserve(line.len()).is_err() {
                eprintln!("sed: out of memory");
                return util::EXIT_FAILURE;
            }
            text.extend_from_slice(line);

            let mut deleted = false;
            for command in &commands {
                if !command.applies(line_number) {
                    continue;
                }
                match &command.kind {
                    Kind::Print => {
                        if let Err(err) = util::write_line(1, &text) {
                            eprintln!("sed: write error: {err}");
                            return util::EXIT_FAILURE;
                        }
                    }
                    Kind::Delete => {
                        deleted = true;
                        break;
                    }
                    Kind::Substitute {
                        pattern,
                        replacement,
                        global,
                    } => match substitute(pattern, &text, replacement, *global) {
                        Some(replaced) => text = replaced,
                        None => {
                            eprintln!("sed: out of memory");
                            return util::EXIT_FAILURE;
                        }
                    },
                }
            }

            if !deleted && !suppress {
                if let Err(err) = util::write_line(1, &text) {
                    eprintln!("sed: write error: {err}");
                    return util::EXIT_FAILURE;
                }
            }
        }
    }

    status
}

/// Apply one substitution. `None` is an allocation failure.
fn substitute(
    pattern: &Pattern,
    text: &[u8],
    replacement: &[u8],
    global: bool,
) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    if out.try_reserve(text.len()).is_err() {
        return None;
    }
    let mut pos = 0usize;
    // GNU sed suppresses an empty match immediately after a non-empty one (`s/a*/X/g`
    // on `aa` is `X`, not `XX`). Track the end offset of that match.
    let mut after_nonempty: Option<usize> = None;

    loop {
        let mut found: Option<(usize, usize)> = None;
        for start in pos..=text.len() {
            if let Some(end) = pattern.find_at(text, start) {
                found = Some((start, end));
                break;
            }
        }
        let Some((start, end)) = found else {
            append(&mut out, &text[pos..])?;
            break;
        };

        if end == start && after_nonempty == Some(start) {
            // Suppress the empty match at the spot where the previous
            // non-empty match ended; keep the byte that follows instead.
            if start < text.len() {
                append(&mut out, &text[start..start + 1])?;
                pos = start + 1;
                after_nonempty = None;
                continue;
            }
            break;
        }

        append(&mut out, &text[pos..start])?;
        append_replacement(&mut out, replacement, &text[start..end])?;
        if !global {
            append(&mut out, &text[end..])?;
            break;
        }
        if end > start {
            pos = end;
            after_nonempty = Some(end);
        } else if start < text.len() {
            // An empty match advances by one byte; GNU sed's `s/x*/-/g`
            // therefore inserts a replacement between every pair of bytes.
            append(&mut out, &text[start..start + 1])?;
            pos = start + 1;
            after_nonempty = None;
        } else {
            // Empty match at end of line: one replacement, then stop, or the
            // loop would never make progress.
            break;
        }
    }

    Some(out)
}

fn append(out: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    out.try_reserve(bytes.len()).ok()?;
    out.extend_from_slice(bytes);
    Some(())
}

fn append_replacement(out: &mut Vec<u8>, replacement: &[u8], matched: &[u8]) -> Option<()> {
    let mut index = 0usize;
    while index < replacement.len() {
        match replacement[index] {
            b'&' => {
                append(out, matched)?;
                index += 1;
            }
            b'\\' => match replacement.get(index + 1) {
                Some(b'&') => {
                    if out.try_reserve(1).is_err() {
                        return None;
                    }
                    out.push(b'&');
                    index += 2;
                }
                Some(b'\\') => {
                    if out.try_reserve(1).is_err() {
                        return None;
                    }
                    out.push(b'\\');
                    index += 2;
                }
                Some(b'n') => {
                    if out.try_reserve(1).is_err() {
                        return None;
                    }
                    out.push(b'\n');
                    index += 2;
                }
                Some(b't') => {
                    if out.try_reserve(1).is_err() {
                        return None;
                    }
                    out.push(b'\t');
                    index += 2;
                }
                Some(other) => {
                    // Other escapes quote the next byte, which is the least
                    // surprising behaviour for a subset without backrefs.
                    if out.try_reserve(1).is_err() {
                        return None;
                    }
                    out.push(*other);
                    index += 2;
                }
                None => {
                    if out.try_reserve(1).is_err() {
                        return None;
                    }
                    out.push(b'\\');
                    index += 1;
                }
            },
            byte => {
                if out.try_reserve(1).is_err() {
                    return None;
                }
                out.push(byte);
                index += 1;
            }
        }
    }
    Some(())
}

// ─── script parser ─────────────────────────────────────────────────────────

fn parse_script(script: &str) -> Option<Vec<Command>> {
    let bytes = script.as_bytes();
    let mut index = 0usize;
    let mut commands: Vec<Command> = Vec::new();

    loop {
        skip_separators(bytes, &mut index);
        if index >= bytes.len() {
            break;
        }

        let address = parse_address(bytes, &mut index)?;
        let operator = *bytes.get(index)?;
        index += 1;

        let kind = match operator {
            b's' => parse_substitute(bytes, &mut index)?,
            b'p' => Kind::Print,
            b'd' => Kind::Delete,
            _ => return None,
        };
        commands.try_reserve(1).ok()?;
        commands.push(Command { address, kind });

        // After a command only whitespace or a command separator may follow.
        while index < bytes.len() && (bytes[index] == b' ' || bytes[index] == b'\t') {
            index += 1;
        }
        if index < bytes.len() && bytes[index] != b';' && bytes[index] != b'\n' {
            return None;
        }
    }

    if commands.is_empty() {
        None
    } else {
        Some(commands)
    }
}

fn skip_separators(bytes: &[u8], index: &mut usize) {
    while *index < bytes.len() {
        match bytes[*index] {
            b' ' | b'\t' | b'\n' | b';' => *index += 1,
            _ => break,
        }
    }
}

fn parse_address(bytes: &[u8], index: &mut usize) -> Option<Option<(usize, usize)>> {
    let has_digits = matches!(bytes.get(*index), Some(byte) if byte.is_ascii_digit());
    if !has_digits {
        return Some(None);
    }
    let first = parse_number(bytes, index)?;
    if matches!(bytes.get(*index), Some(b',')) {
        *index += 1;
        let last = parse_number(bytes, index)?;
        // `5,2` behaves like the single address `5` rather than matching
        // nothing; this is the useful reading of an accidental reverse range.
        Some(Some((first, last.max(first))))
    } else {
        Some(Some((first, first)))
    }
}

fn parse_number(bytes: &[u8], index: &mut usize) -> Option<usize> {
    let start = *index;
    let mut value = 0usize;
    while let Some(byte) = bytes.get(*index) {
        if !byte.is_ascii_digit() {
            break;
        }
        value = value
            .checked_mul(10)?
            .checked_add((*byte - b'0') as usize)?;
        *index += 1;
    }
    if *index == start {
        None
    } else {
        Some(value)
    }
}

fn parse_substitute(bytes: &[u8], index: &mut usize) -> Option<Kind> {
    let delimiter = *bytes.get(*index)?;
    if delimiter == b'\\' || delimiter == b'\n' {
        return None;
    }
    *index += 1;

    let pattern_bytes = read_delimited(bytes, index, delimiter)?;
    let replacement = read_delimited(bytes, index, delimiter)?;

    let mut global = false;
    while let Some(byte) = bytes.get(*index) {
        match byte {
            b'g' => {
                global = true;
                *index += 1;
            }
            b' ' | b'\t' | b';' | b'\n' => break,
            _ => return None,
        }
    }

    // Reject `\1` and friends: this subset has no capture groups, and silently
    // producing a digit would be a lie about what the script means.
    let mut at = 0usize;
    while at < replacement.len() {
        if replacement[at] == b'\\' {
            if matches!(replacement.get(at + 1), Some(byte) if byte.is_ascii_digit()) {
                return None;
            }
            at += 2;
        } else {
            at += 1;
        }
    }

    if pattern_bytes.is_empty() {
        // GNU sed's empty RE reuses the previous one; this subset has no such
        // state, so refuse rather than silently matching the empty string.
        return None;
    }
    let pattern = Pattern::compile(&pattern_bytes)?;
    Some(Kind::Substitute {
        pattern,
        replacement,
        global,
    })
}

/// Read bytes up to an unescaped `delimiter`, keeping backslash escapes in the
/// returned bytes for the caller. The delimiter itself is not included.
fn read_delimited(bytes: &[u8], index: &mut usize, delimiter: u8) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    loop {
        let byte = *bytes.get(*index)?;
        if byte == delimiter {
            *index += 1;
            return Some(out);
        }
        if byte == b'\\' {
            let next = *bytes.get(*index + 1)?;
            out.try_reserve(2).ok()?;
            out.push(byte);
            out.push(next);
            *index += 2;
            continue;
        }
        out.try_reserve(1).ok()?;
        out.push(byte);
        *index += 1;
    }
}