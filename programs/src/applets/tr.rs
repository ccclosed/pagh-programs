//! `tr` — translate, delete or squeeze bytes.
//!
//! Forms:
//!
//! * `tr SET1 SET2` — map each byte of SET1 to the corresponding byte of SET2;
//!   if SET2 is shorter, its last byte is repeated (GNU padding rule);
//! * `tr -d SET1` — delete every byte in SET1;
//! * `tr -s SET1` — squeeze runs of bytes in SET1 to one;
//! * `tr -s SET1 SET2` — translate, then squeeze runs of SET2;
//! * `tr -d -s SET1 SET2` — delete SET1 and squeeze SET2.
//!
//! SETs are byte strings. `a-z` expands to the inclusive ASCII range; a reversed
//! range (`z-a`) is an error. A backslash escapes the next byte and understands
//! `\n`, `\t`, `\r`, `\0`, `\a`, `\b`, `\f`, `\v`, and `\\`; an unknown escape
//! is the escaped byte itself. `-c`/`-C` (complement), `-t` (truncate SET1), and
//! POSIX character classes are **not** implemented.
//!
//! Input is always stdin: there are no file operands in `tr`, and the kernel
//! shell has no pipes, so a file must be fed another way (the current shell
//! cannot do that yet; this is documented, not worked around).
//!
//! Exit: 0 success, 1 read/write error, 2 usage or invalid-set error.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: tr [-d] [-s] SET1 [SET2]";

#[derive(Clone, Copy)]
enum Item {
    Byte(u8),
    Dash,
}

pub fn run(args: &[String]) -> i32 {
    let mut delete = false;
    let mut squeeze = false;
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
                    'd' => delete = true,
                    's' => squeeze = true,
                    _ => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        // The first set ends option parsing: a set may itself start with `-`.
        parse_options = false;
        operands.push(arg.as_str());
    }

    if operands.is_empty() || operands.len() > 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let set1 = match expand_set(operands[0]) {
        Some(set) => set,
        None => {
            eprintln!("tr: invalid set: '{}'", operands[0]);
            return util::EXIT_USAGE;
        }
    };
    let set2 = match operands.get(1) {
        Some(text) => match expand_set(text) {
            Some(set) => Some(set),
            None => {
                eprintln!("tr: invalid set: '{text}'");
                return util::EXIT_USAGE;
            }
        },
        None => None,
    };

    let mut deleted = [false; 256];
    let mut squeezed = [false; 256];
    let mut map = [0u8; 256];
    for (index, slot) in map.iter_mut().enumerate() {
        *slot = index as u8;
    }

    if delete {
        if operands.len() == 2 && !squeeze {
            // The second set has no meaning without -s; refuse rather than
            // silently ignore an operand.
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        for byte in &set1 {
            deleted[*byte as usize] = true;
        }
        if squeeze {
            let squeeze_set = set2.as_ref().unwrap_or(&set1);
            for byte in squeeze_set {
                squeezed[*byte as usize] = true;
            }
        }
    } else if let Some(set2) = set2.as_ref() {
        if set2.is_empty() {
            eprintln!("tr: empty replacement set");
            return util::EXIT_USAGE;
        }
        for (index, byte) in set1.iter().enumerate() {
            let replacement = set2
                .get(index)
                .copied()
                .or_else(|| set2.last().copied())
                .unwrap_or(*byte);
            map[*byte as usize] = replacement;
        }
        if squeeze {
            for byte in set2 {
                squeezed[*byte as usize] = true;
            }
        }
    } else {
        if !squeeze {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        for byte in &set1 {
            squeezed[*byte as usize] = true;
        }
    }

    let mut last: Option<u8> = None;
    let mut input = [0u8; 8192];
    let mut output = [0u8; 8192];

    loop {
        let read = match util::read_retry(0, &mut input) {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) => {
                util::report("tr", "standard input", err);
                return util::EXIT_FAILURE;
            }
        };

        let mut written = 0usize;
        for byte in &input[..read] {
            let candidate = if delete {
                if deleted[*byte as usize] {
                    continue;
                }
                *byte
            } else {
                map[*byte as usize]
            };

            if squeeze && last == Some(candidate) && squeezed[candidate as usize] {
                continue;
            }
            last = Some(candidate);

            if written == output.len() {
                if let Err(err) = util::write_all(1, &output[..written]) {
                    eprintln!("tr: write error: {err}");
                    return util::EXIT_FAILURE;
                }
                written = 0;
            }
            output[written] = candidate;
            written += 1;
        }

        if written > 0 {
            if let Err(err) = util::write_all(1, &output[..written]) {
                eprintln!("tr: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }
    }

    util::EXIT_OK
}

fn parse_items(text: &str) -> Option<Vec<Item>> {
    let bytes = text.as_bytes();
    let mut items: Vec<Item> = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let item = if bytes[index] == b'\\' && index + 1 < bytes.len() {
            index += 2;
            Item::Byte(escaped(bytes[index - 1]))
        } else if bytes[index] == b'-' {
            index += 1;
            Item::Dash
        } else {
            index += 1;
            Item::Byte(bytes[index - 1])
        };
        items.try_reserve(1).ok()?;
        items.push(item);
    }
    Some(items)
}

fn escaped(byte: u8) -> u8 {
    match byte {
        b'n' => b'\n',
        b't' => b'\t',
        b'r' => b'\r',
        b'0' => 0,
        b'a' => 0x07,
        b'b' => 0x08,
        b'f' => 0x0c,
        b'v' => 0x0b,
        other => other,
    }
}

fn expand_set(text: &str) -> Option<Vec<u8>> {
    let items = parse_items(text)?;
    let mut out: Vec<u8> = Vec::new();
    let mut index = 0usize;

    while index < items.len() {
        match items[index] {
            Item::Byte(start) => {
                if index + 2 < items.len() {
                    if let (Item::Dash, Item::Byte(end)) = (items[index + 1], items[index + 2]) {
                        if start > end {
                            return None;
                        }
                        let count = usize::from(end - start) + 1;
                        out.try_reserve(count).ok()?;
                        for value in start..=end {
                            out.push(value);
                        }
                        index += 3;
                        continue;
                    }
                }
                out.try_reserve(1).ok()?;
                out.push(start);
                index += 1;
            }
            Item::Dash => {
                out.try_reserve(1).ok()?;
                out.push(b'-');
                index += 1;
            }
        }
    }
    Some(out)
}