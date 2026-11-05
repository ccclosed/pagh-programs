//! A tiny byte-oriented BRE matcher shared by `sed` and `awk`.
//!
//! This is the same deliberately small subset `grep` documents, extracted so
//! the two new text tools do not carry slightly different dialects:
//!
//! * `.` matches any byte;
//! * `*` repeats the previous single-byte atom (literal, `.` or class);
//! * `[abc]`, `[a-z]`, `[^abc]` are byte classes (as in `grep`);
//! * `^` anchors at the start and `$` at the end, only in those positions;
//! * `\` makes the next byte literal.
//!
//! There is no alternation, grouping, `\+`/`\?`, back-reference or interval
//! expression. The matcher is iterative-ish: `find_at` tries the possible end
//! positions from longest to shortest and `match_from` walks the atoms with a
//! bounded backtracking recursion. Patterns are capped at [`MAX_PATTERN`]
//! bytes, so the recursion depth is bounded and a pathological script cannot
//! exhaust the user stack. No input can panic: every index is checked.

use alloc::vec::Vec;

/// Longest accepted pattern; longer input is a usage error, not a crash.
pub const MAX_PATTERN: usize = 4096;

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
    fn matches(&self, byte: u8) -> bool {
        let hit = self.set[byte as usize];
        if self.negated {
            !hit
        } else {
            hit
        }
    }
}

/// A compiled pattern.
pub struct Pattern {
    atoms: Vec<Atom>,
    stars: Vec<bool>,
}

impl Pattern {
    /// Compile `pattern`; `None` is an invalid pattern or one that is too long.
    pub fn compile(pattern: &[u8]) -> Option<Pattern> {
        if pattern.len() > MAX_PATTERN {
            return None;
        }
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
                b'\\' => match pattern.get(index + 1) {
                    Some(byte) => {
                        push_atom(&mut atoms, &mut stars, Atom::Literal(*byte))?;
                        index += 2;
                    }
                    None => {
                        push_atom(&mut atoms, &mut stars, Atom::Literal(b'\\'))?;
                        index += 1;
                    }
                },
                byte => {
                    push_atom(&mut atoms, &mut stars, Atom::Literal(byte))?;
                    index += 1;
                }
            }
        }

        Some(Pattern { atoms, stars })
    }

    /// Is there a match anywhere in `text`?
    pub fn is_match(&self, text: &[u8]) -> bool {
        (0..=text.len()).any(|start| self.find_at(text, start).is_some())
    }

    /// Longest match beginning exactly at `start`, as an exclusive end offset.
    ///
    /// Trying ends from the line end downwards gives leftmost-longest
    /// semantics, which is what `sed` and `grep` promise for this subset.
    pub fn find_at(&self, text: &[u8], start: usize) -> Option<usize> {
        if start > text.len() {
            return None;
        }
        let mut end = text.len();
        loop {
            if self.match_from(0, text, start, end) {
                return Some(end);
            }
            if end == start {
                return None;
            }
            end -= 1;
        }
    }

    fn match_from(&self, atom_index: usize, text: &[u8], pos: usize, end: usize) -> bool {
        let Some(atom) = self.atoms.get(atom_index) else {
            return pos == end;
        };

        if *self.stars.get(atom_index).unwrap_or(&false) {
            // Starred atoms are only literals, `.` or classes (see
            // `quantifiable`), so the counted loop is a plain byte walk.
            let mut max = pos;
            while max < end && atom_matches(atom, text[max]) {
                max += 1;
            }
            // Greedy with bounded backtracking: at most one atom of length.
            let mut take = max - pos;
            loop {
                if self.match_from(atom_index + 1, text, pos + take, end) {
                    return true;
                }
                if take == 0 {
                    return false;
                }
                take -= 1;
            }
        }

        match atom {
            Atom::Start => pos == 0 && self.match_from(atom_index + 1, text, pos, end),
            Atom::End => pos == text.len() && self.match_from(atom_index + 1, text, pos, end),
            _ => {
                pos < end
                    && atom_matches(atom, text[pos])
                    && self.match_from(atom_index + 1, text, pos + 1, end)
            }
        }
    }
}

fn atom_matches(atom: &Atom, byte: u8) -> bool {
    match atom {
        Atom::Literal(expected) => *expected == byte,
        Atom::Any => true,
        Atom::Class(class) => class.matches(byte),
        Atom::Start | Atom::End => false,
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