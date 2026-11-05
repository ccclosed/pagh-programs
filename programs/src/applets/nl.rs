//! `nl` — number lines.
//!
//! By default only non-empty lines are numbered (`-b t`); `-b a` numbers every
//! line, including empty ones. Numbering starts at 1 and increments once per
//! numbered line (skipped empty lines do not consume a number). The familiar
//! shape is used: the number right-aligned in six columns, then a tab.
//!
//! One input operand or stdin; no operands means stdin. `-w`, `-s`, `-v`, `-i`
//! and section delimiters are not implemented — this is the minimal subset.
//!
//! Exit: 0 success, 1 open/read/write error, 2 usage error.

use alloc::format;
use alloc::string::String;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: nl [-b a|t] [FILE]";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Body {
    All,
    NonEmpty,
}

pub fn run(args: &[String]) -> i32 {
    let mut body = Body::NonEmpty;
    let mut operand: Option<&str> = None;
    let mut parse_options = true;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        if parse_options && arg == "--" {
            parse_options = false;
            index += 1;
            continue;
        }
        if parse_options && arg == "-b" {
            index += 1;
            match args.get(index) {
                Some(value) => match parse_body(value) {
                    Some(parsed) => body = parsed,
                    None => {
                        eprintln!("{USAGE}");
                        return util::EXIT_USAGE;
                    }
                },
                None => {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-b") {
            match parse_body(&arg[2..]) {
                Some(parsed) => body = parsed,
                None => {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                }
            }
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        if operand.is_some() {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        operand = Some(arg.as_str());
        index += 1;
    }

    let name = operand.unwrap_or("-");
    let data = match util::read_input(name) {
        Ok(data) => data,
        Err(err) => {
            util::report("nl", util::display_name(name), err);
            return util::EXIT_FAILURE;
        }
    };

    let mut number: u64 = 1;
    for line in util::lines(&data) {
        let numbered = body == Body::All || !line.is_empty();
        if numbered {
            let prefix = format!("{number:>6}\t");
            if let Err(err) = util::write_all(1, prefix.as_bytes()) {
                eprintln!("nl: write error: {err}");
                return util::EXIT_FAILURE;
            }
            number = number.saturating_add(1);
        }
        if let Err(err) = util::write_line(1, line) {
            eprintln!("nl: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    util::EXIT_OK
}

fn parse_body(text: &str) -> Option<Body> {
    match text {
        "a" => Some(Body::All),
        "t" => Some(Body::NonEmpty),
        _ => None,
    }
}