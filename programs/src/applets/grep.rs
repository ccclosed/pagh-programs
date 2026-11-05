//! `grep` — search lines for a literal string or a small BRE-style pattern.
//!
//! The regex subset is intentionally small and documented:
//!
//! * `.` matches any byte (lines contain no LF);
//! * `*` repeats the previous single-byte atom (literal, `.` or a class);
//! * `[abc]`, `[a-z]`, `[^abc]` are byte classes, with `]` first and `-` at
//!   either end treated literally, and `\` escaping inside;
//! * `^` anchors at the start of the line and `$` at the end, but only in those
//!   positions (elsewhere they are literals, as in BRE);
//! * `\` makes the next byte literal (`\*`, `\.`, `\\`, `\$`, ...).
//!
//! There is no alternation, no grouping, no `\+`/`\?`, no back-references, no
//! interval expressions and no POSIX named classes. `-F` selects literal
//! substring matching and bypasses the matcher entirely.
//!
//! Options: `-i` (ASCII case-insensitive), `-v` (invert), `-n` (line number),
//! `-c` (count selected lines per file), `-l` (print only names of files with a
//! selected line; takes precedence over `-c`), `-F` (fixed string). The pattern
//! is the first operand; the remaining operands are files, and `-` is stdin.
//! With more than one file, normal output and `-c` are prefixed with the file
//! name; `-n` adds `line:` (or `file:line:`) in front of the text.
//!
//! Exit: 0 if at least one line was selected, 1 if none, 2 for a usage error,
//! an invalid pattern, or any file that could not be opened/read.
//!
//! The matcher is an NFA simulation over the pattern atoms: it is iterative (no
//! recursion), bounded by `O(line length * pattern length)`, and never panics
//! on a malformed pattern or arbitrary bytes.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: grep [-i] [-v] [-n] [-c] [-l] [-F] PATTERN [FILE...]";

#[derive(Clone, Copy, Default)]
struct Options {
    ignore_case: bool,
    invert: bool,
    numbers: bool,
    count: bool,
    list: bool,
    fixed: bool,
}

enum Engine {
    Literal { needle: Vec<u8>, ignore_case: bool },
    Regex(Matcher),
}

impl Engine {
    fn matches(&mut self, line: &[u8]) -> bool {
        match self {
            Engine::Literal {
                needle,
                ignore_case,
            } => contains_literal(line, needle, *ignore_case),
            Engine::Regex(matcher) => matcher.matches(line),
        }
    }
}

enum Atom {
    Literal(u8),
    Any,
    Class(Class),
    Start,
    End,
}

struct Class {
    negated: bool,
    set: [bool; 256],
}

impl Class {
    fn matches(&self, byte: u8, ignore_case: bool) -> bool {
        let mut hit = self.set[byte as usize];
        if ignore_case {
            hit |= self.set[byte.to_ascii_lowercase() as usize];
            hit |= self.set[byte.to_ascii_uppercase() as usize];
        }
        if self.negated {
            !hit
        } else {
            hit
        }
    }
}

struct Matcher {
    atoms: Vec<Atom>,
    stars: Vec<bool>,
    active: Vec<bool>,
    next: Vec<bool>,
    ignore_case: bool,
}

impl Matcher {
    fn compile(pattern: &[u8], ignore_case: bool) -> Option<Matcher> {
        let mut atoms: Vec<Atom> = Vec::new();
        let mut stars: Vec<bool> = Vec::new();
        let mut index = 0usize;

        while index < pattern.len() {
            match pattern[index] {
                b'^' if index == 0 => {
                    push_atom(&mut atoms, &mut stars, Atom::Start)?;
                    index += 1;
                }
                b'$' if index + 1 == pattern.len() => {
                    push_atom(&mut atoms, &mut stars, Atom::End)?;
                    index += 1;
                }
                b'.' => {
                    push_atom(&mut atoms, &mut stars, Atom::Any)?;
                    index += 1;
                }
                b'[' => {
                    let (class, next) = parse_class(pattern, index)?;
                    push_atom(&mut atoms, &mut stars, Atom::Class(class))?;
                    index = next;
                }
                b'*' => {
                    let mut quantified = false;
                    if let Some(last) = atoms.len().checked_sub(1) {
                        if !stars[last] && quantifiable(&atoms[last]) {
                            stars[last] = true;
                            index += 1;
                            quantified = true;
                        }
                    }
                    if !quantified {
                        push_atom(&mut atoms, &mut stars, Atom::Literal(b'*'))?;
                        index += 1;
                    }
                }
                b'\\' => {
                    let (literal, next) = read_escaped(pattern, index)?;
                    push_atom(&mut atoms, &mut stars, Atom::Literal(literal))?;
                    index = next;
                }
                byte => {
                    push_atom(&mut atoms, &mut stars, Atom::Literal(byte))?;
                    index += 1;
                }
            }
        }

        let size = atoms.len() + 1;
        let mut active: Vec<bool> = Vec::new();
        active.try_reserve(size).ok()?;
        active.resize(size, false);
        let mut next: Vec<bool> = Vec::new();
        next.try_reserve(size).ok()?;
        next.resize(size, false);

        Some(Matcher {
            atoms,
            stars,
            active,
            next,
            ignore_case,
        })
    }

    fn matches(&mut self, text: &[u8]) -> bool {
        let count = self.atoms.len();
        for slot in self.active.iter_mut() {
            *slot = false;
        }
        self.active[0] = true;
        closure(
            &mut self.active,
            &self.atoms,
            &self.stars,
            text,
            0,
        );
        if self.active[count] {
            return true;
        }

        for position in 0..text.len() {
            for slot in self.next.iter_mut() {
                *slot = false;
            }
            for state in 0..count {
                if !self.active[state] {
                    continue;
                }
                if atom_matches(&self.atoms[state], text[position], self.ignore_case) {
                    let target = if self.stars[state] { state } else { state + 1 };
                    self.next[target] = true;
                }
            }
            // A match may start at any position, not only at the beginning.
            self.next[0] = true;
            closure(&mut self.next, &self.atoms, &self.stars, text, position + 1);
            core::mem::swap(&mut self.active, &mut self.next);
            if self.active[count] {
                return true;
            }
        }

        false
    }
}

fn push_atom(atoms: &mut Vec<Atom>, stars: &mut Vec<bool>, atom: Atom) -> Option<()> {
    atoms.try_reserve(1).ok()?;
    stars.try_reserve(1).ok()?;
    atoms.push(atom);
    stars.push(false);
    Some(())
}

fn quantifiable(atom: &Atom) -> bool {
    !matches!(atom, Atom::Start | Atom::End)
}

/// Add the epsilon transitions of every active state. Only forward edges are
/// ever added, so one left-to-right pass reaches the fixpoint.
fn closure(set: &mut [bool], atoms: &[Atom], stars: &[bool], text: &[u8], position: usize) {
    let count = atoms.len();
    let mut index = 0usize;
    while index < count {
        if !set[index] {
            index += 1;
            continue;
        }
        let zero_width = match atoms[index] {
            Atom::Start => position == 0,
            Atom::End => position == text.len(),
            _ => false,
        };
        if zero_width || stars[index] {
            set[index + 1] = true;
        }
        index += 1;
    }
}

fn atom_matches(atom: &Atom, byte: u8, ignore_case: bool) -> bool {
    match atom {
        Atom::Literal(expected) => bytes_equal(*expected, byte, ignore_case),
        Atom::Any => true,
        Atom::Class(class) => class.matches(byte, ignore_case),
        Atom::Start | Atom::End => false,
    }
}

fn bytes_equal(left: u8, right: u8, ignore_case: bool) -> bool {
    if ignore_case {
        left.to_ascii_lowercase() == right.to_ascii_lowercase()
    } else {
        left == right
    }
}

fn read_escaped(pattern: &[u8], index: usize) -> Option<(u8, usize)> {
    match pattern.get(index + 1) {
        // In BRE a backslash before an ordinary byte is that byte (`\n` is
        // `n`, not a newline); only `\\`, `\*`, `\.` and friends matter here.
        Some(byte) => Some((*byte, index + 2)),
        None => Some((b'\\', index + 1)),
    }
}

fn read_class_byte(pattern: &[u8], index: usize) -> Option<(u8, usize)> {
    let byte = *pattern.get(index)?;
    if byte == b'\\' {
        let next = *pattern.get(index + 1)?;
        Some((next, index + 2))
    } else {
        Some((byte, index + 1))
    }
}

fn parse_class(pattern: &[u8], start: usize) -> Option<(Class, usize)> {
    let mut index = start + 1;
    let mut negated = false;
    if index < pattern.len() && pattern[index] == b'^' {
        negated = true;
        index += 1;
    }

    let mut set = [false; 256];
    let mut first = true;
    loop {
        if index >= pattern.len() {
            return None;
        }
        if pattern[index] == b']' && !first {
            index += 1;
            break;
        }
        first = false;

        let (low, next) = read_class_byte(pattern, index)?;
        index = next;
        if index < pattern.len()
            && pattern[index] == b'-'
            && index + 1 < pattern.len()
            && pattern[index + 1] != b']'
        {
            let (high, next) = read_class_byte(pattern, index + 1)?;
            if low > high {
                return None;
            }
            for value in low..=high {
                set[value as usize] = true;
            }
            index = next;
        } else {
            set[low as usize] = true;
        }
    }

    Some((Class { negated, set }, index))
}

fn contains_literal(haystack: &[u8], needle: &[u8], ignore_case: bool) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    for start in 0..=haystack.len() - needle.len() {
        let mut matched = true;
        for offset in 0..needle.len() {
            if !bytes_equal(haystack[start + offset], needle[offset], ignore_case) {
                matched = false;
                break;
            }
        }
        if matched {
            return true;
        }
    }
    false
}

pub fn run(args: &[String]) -> i32 {
    let mut options = Options::default();
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            for option in arg[1..].chars() {
                match option {
                    'i' => options.ignore_case = true,
                    'v' => options.invert = true,
                    'n' => options.numbers = true,
                    'c' => options.count = true,
                    'l' => options.list = true,
                    'F' => options.fixed = true,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        parse_options = false;
        operands.push(arg.as_str());
    }

    if operands.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let pattern = operands[0];
    let mut files: Vec<&str> = Vec::new();
    for name in &operands[1..] {
        if files.try_reserve(1).is_err() {
            eprintln!("grep: out of memory");
            return util::EXIT_USAGE;
        }
        files.push(*name);
    }
    if files.is_empty() {
        files.push("-");
    }

    let mut engine = if options.fixed {
        let mut needle: Vec<u8> = Vec::new();
        if needle.try_reserve(pattern.len()).is_err() {
            eprintln!("grep: out of memory");
            return util::EXIT_USAGE;
        }
        needle.extend_from_slice(pattern.as_bytes());
        Engine::Literal {
            needle,
            ignore_case: options.ignore_case,
        }
    } else {
        match Matcher::compile(pattern.as_bytes(), options.ignore_case) {
            Some(matcher) => Engine::Regex(matcher),
            None => {
                eprintln!("grep: invalid pattern: '{pattern}'");
                return util::EXIT_USAGE;
            }
        }
    };

    let named = files.len() > 1;
    let mut any_selected = false;
    let mut read_error = false;

    for name in &files {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("grep", util::display_name(name), err);
                read_error = true;
                continue;
            }
        };

        let label = util::display_name(name);
        let mut count: u64 = 0;
        let mut line_number: u64 = 0;
        let mut file_selected = false;

        for line in util::lines(&data) {
            line_number = line_number.saturating_add(1);
            let matched = engine.matches(line);
            let selected = matched != options.invert;
            if !selected {
                continue;
            }

            count = count.saturating_add(1);
            any_selected = true;
            file_selected = true;

            if options.list {
                break;
            }
            if options.count {
                continue;
            }

            let mut prefix = String::new();
            if named {
                prefix.push_str(label);
                prefix.push(':');
            }
            if options.numbers {
                let _ = write!(prefix, "{line_number}:");
            }
            if !prefix.is_empty() {
                if let Err(err) = util::write_all(1, prefix.as_bytes()) {
                    eprintln!("grep: write error: {err}");
                    return util::EXIT_USAGE;
                }
            }
            if let Err(err) = util::write_line(1, line) {
                eprintln!("grep: write error: {err}");
                return util::EXIT_USAGE;
            }
        }

        if options.list {
            if file_selected {
                if let Err(err) = util::write_line(1, label.as_bytes()) {
                    eprintln!("grep: write error: {err}");
                    return util::EXIT_USAGE;
                }
            }
        } else if options.count {
            let line = if named {
                format!("{label}:{count}")
            } else {
                format!("{count}")
            };
            if let Err(err) = util::write_line(1, line.as_bytes()) {
                eprintln!("grep: write error: {err}");
                return util::EXIT_USAGE;
            }
        }
    }

    if read_error {
        util::EXIT_USAGE
    } else if any_selected {
        util::EXIT_OK
    } else {
        util::EXIT_FAILURE
    }
}