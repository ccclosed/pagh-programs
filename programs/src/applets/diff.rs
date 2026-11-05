//! `diff` — compare two files and print the normal (not unified) difference
//! format.
//!
//! Output uses the classic commands `NaM`, `NdM` and `NcM` followed by `<` and
//! `>` lines with a `---` separator between the two sides of a change, for
//! example:
//!
//! ```text
//! 2c2
//! < old line
//! ---
//! > new line
//! ```
//!
//! The line matching is a real longest-common-subsequence diff (Hirschberg's
//! linear-space variant), not a line-by-line guess. Files are compared as byte
//! lines: a missing final newline does not make otherwise equal lines differ.
//! There is no unified/context format, no recursive directory mode and no
//! `-u`.
//!
//! Exit: 0 identical, 1 different, 2 usage, missing file/read error or out of
//! memory. That is the conventional diff contract, and the assignment asks for
//! it explicitly.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: diff FILE1 FILE2";

struct Hunk<'a> {
    /// Number of `a` lines before this hunk (0-based insertion point).
    a_start: usize,
    a_lines: Vec<&'a [u8]>,
    /// Number of `b` lines before this hunk.
    b_start: usize,
    b_lines: Vec<&'a [u8]>,
}

pub fn run(args: &[String]) -> i32 {
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("diff: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.len() != 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut data: [Vec<u8>; 2] = [Vec::new(), Vec::new()];
    for slot in 0..2 {
        match util::read_input(operands[slot]) {
            Ok(bytes) => data[slot] = bytes,
            Err(err) => {
                util::report("diff", util::display_name(operands[slot]), err);
                return util::EXIT_USAGE;
            }
        }
    }

    let left: Vec<&[u8]> = util::lines(&data[0]).collect();
    let right: Vec<&[u8]> = util::lines(&data[1]).collect();

    let mut matches: Vec<(usize, usize)> = Vec::new();
    if lcs_matches(&left, &right, &mut matches).is_none() {
        eprintln!("diff: out of memory");
        return util::EXIT_USAGE;
    }

    let hunks = match build_hunks(&left, &right, &matches) {
        Some(hunks) => hunks,
        None => {
            eprintln!("diff: out of memory");
            return util::EXIT_USAGE;
        }
    };

    for hunk in &hunks {
        if let Err(err) = print_hunk(hunk) {
            eprintln!("diff: write error: {err}");
            return util::EXIT_USAGE;
        }
    }

    if hunks.is_empty() {
        util::EXIT_OK
    } else {
        util::EXIT_FAILURE
    }
}

fn build_hunks<'a>(
    left: &[&'a [u8]],
    right: &[&'a [u8]],
    matches: &[(usize, usize)],
) -> Option<Vec<Hunk<'a>>> {
    let mut hunks: Vec<Hunk<'a>> = Vec::new();
    let mut a_position = 0usize;
    let mut b_position = 0usize;

    for (match_a, match_b) in matches {
        if *match_a > a_position || *match_b > b_position {
            let mut hunk = Hunk {
                a_start: a_position,
                a_lines: Vec::new(),
                b_start: b_position,
                b_lines: Vec::new(),
            };
            hunk.a_lines.try_reserve(*match_a - a_position).ok()?;
            hunk.a_lines.extend_from_slice(&left[a_position..*match_a]);
            hunk.b_lines.try_reserve(*match_b - b_position).ok()?;
            hunk.b_lines.extend_from_slice(&right[b_position..*match_b]);
            hunks.try_reserve(1).ok()?;
            hunks.push(hunk);
        }
        a_position = *match_a + 1;
        b_position = *match_b + 1;
    }

    if a_position < left.len() || b_position < right.len() {
        let mut hunk = Hunk {
            a_start: a_position,
            a_lines: Vec::new(),
            b_start: b_position,
            b_lines: Vec::new(),
        };
        hunk.a_lines.try_reserve(left.len() - a_position).ok()?;
        hunk.a_lines.extend_from_slice(&left[a_position..]);
        hunk.b_lines.try_reserve(right.len() - b_position).ok()?;
        hunk.b_lines.extend_from_slice(&right[b_position..]);
        hunks.try_reserve(1).ok()?;
        hunks.push(hunk);
    }

    Some(hunks)
}

fn print_hunk(hunk: &Hunk<'_>) -> libpagh::Result<()> {
    let a_count = hunk.a_lines.len();
    let b_count = hunk.b_lines.len();
    let header = if a_count > 0 && b_count > 0 {
        alloc::format!(
            "{}c{}",
            range(hunk.a_start + 1, hunk.a_start + a_count),
            range(hunk.b_start + 1, hunk.b_start + b_count)
        )
    } else if a_count > 0 {
        alloc::format!("{}d{}", range(hunk.a_start + 1, hunk.a_start + a_count), hunk.b_start)
    } else {
        alloc::format!("{}a{}", hunk.a_start, range(hunk.b_start + 1, hunk.b_start + b_count))
    };
    util::write_line(1, header.as_bytes())?;

    for line in &hunk.a_lines {
        let mut text: Vec<u8> = Vec::new();
        text.try_reserve(line.len() + 2).map_err(|_| libpagh::Errno::ENOMEM)?;
        text.extend_from_slice(b"< ");
        text.extend_from_slice(line);
        util::write_line(1, &text)?;
    }
    if a_count > 0 && b_count > 0 {
        util::write_line(1, b"---")?;
    }
    for line in &hunk.b_lines {
        let mut text: Vec<u8> = Vec::new();
        text.try_reserve(line.len() + 2).map_err(|_| libpagh::Errno::ENOMEM)?;
        text.extend_from_slice(b"> ");
        text.extend_from_slice(line);
        util::write_line(1, &text)?;
    }
    Ok(())
}

fn range(first: usize, last: usize) -> String {
    if first == last {
        alloc::format!("{first}")
    } else {
        alloc::format!("{first},{last}")
    }
}

// ─── Hirschberg LCS ────────────────────────────────────────────────────────

/// Fill `out` with increasing `(line_in_a, line_in_b)` equal pairs.
fn lcs_matches(a: &[&[u8]], b: &[&[u8]], out: &mut Vec<(usize, usize)>) -> Option<()> {
    if a.is_empty() || b.is_empty() {
        return Some(());
    }
    if a.len() == 1 {
        if let Some(index) = b.iter().position(|line| *line == a[0]) {
            out.try_reserve(1).ok()?;
            out.push((0, index));
        }
        return Some(());
    }
    if b.len() == 1 {
        if let Some(index) = a.iter().position(|line| *line == b[0]) {
            out.try_reserve(1).ok()?;
            out.push((index, 0));
        }
        return Some(());
    }

    let middle = a.len() / 2;
    let left_scores = lcs_lengths(&a[..middle], b)?;
    let right_scores = lcs_lengths_reverse(&a[middle..], b)?;

    let mut split = 0usize;
    let mut best = 0usize;
    for candidate in 0..=b.len() {
        let score = left_scores[candidate].saturating_add(right_scores[candidate]);
        if candidate == 0 || score > best {
            best = score;
            split = candidate;
        }
    }

    lcs_matches(&a[..middle], &b[..split], out)?;
    let before = out.len();
    lcs_matches(&a[middle..], &b[split..], out)?;
    // The recursive calls report indices relative to their own slices.
    for entry in &mut out[before..] {
        entry.0 += middle;
        entry.1 += split;
    }
    Some(())
}

/// `result[j] = LCS(a, b[..j])`.
fn lcs_lengths(a: &[&[u8]], b: &[&[u8]]) -> Option<Vec<usize>> {
    let mut table = Vec::new();
    table.try_reserve(b.len() + 1).ok()?;
    table.resize(b.len() + 1, 0usize);
    for line in a {
        let mut diagonal = 0usize;
        for column in 1..=b.len() {
            let above = table[column];
            if *line == b[column - 1] {
                table[column] = diagonal + 1;
            } else if table[column - 1] > table[column] {
                table[column] = table[column - 1];
            }
            diagonal = above;
        }
    }
    Some(table)
}

/// `result[j] = LCS(a, b[j..])`, computed from the ends.
fn lcs_lengths_reverse(a: &[&[u8]], b: &[&[u8]]) -> Option<Vec<usize>> {
    let mut table = Vec::new();
    table.try_reserve(b.len() + 1).ok()?;
    table.resize(b.len() + 1, 0usize);
    for line in a.iter().rev() {
        let mut diagonal = 0usize;
        for column in (0..b.len()).rev() {
            let below = table[column];
            if *line == b[column] {
                table[column] = diagonal + 1;
            } else if table[column + 1] > table[column] {
                table[column] = table[column + 1];
            }
            diagonal = below;
        }
    }
    Some(table)
}