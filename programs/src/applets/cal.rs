//! `cal` — a Gregorian month or year calendar.
//!
//! ```text
//! cal                 # current month (from clock_gettime, epoch-based)
//! cal MONTH [YEAR]    # a month; one number 1..12 means current year
//! cal YEAR            # the year grid (a number outside 1..12)
//! cal -y [YEAR]       # the year grid explicitly
//! ```
//!
//! The grid is seven columns, Sunday-first, with the correct proleptic
//! Gregorian weekdays and leap years. The "current" date is derived from
//! `clock_gettime(CLOCK_REALTIME)` and treated as seconds since
//! 1970-01-01T00:00:00Z. pagh has no RTC and its realtime clock is the boot
//! tick clock, so a freshly booted machine shows January 1970 plus uptime —
//! the same caveat as the `date` applet. There is no timezone or locale
//! support: month and weekday names are English and the arithmetic is UTC.
//!
//! `-y`/`--year` selects the year grid; unknown options are usage errors (2),
//! as are a month outside 1..12 or a year outside 1..9999. A clock failure is
//! a data error (1), as is a write failure.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::{sys, Timespec};

use crate::util;

const USAGE: &str = "usage: cal [-y] [MONTH] [YEAR]";
const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEK_HEADER: &str = "Su Mo Tu We Th Fr Sa";
const BLOCK_WIDTH: usize = 20;

pub fn run(args: &[String]) -> i32 {
    let mut year_mode = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && (arg == "-y" || arg == "--year") {
            year_mode = true;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("cal: invalid option -- '{}'", &arg[1..]);
            return usage();
        }
        parse_options = false;
        if util::push(&mut operands, arg.as_str()).is_err() {
            eprintln!("cal: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let (current_year, current_month) = match current_date() {
        Ok(date) => date,
        Err(err) => {
            util::report("cal", "clock_gettime", err);
            return util::EXIT_FAILURE;
        }
    };

    let (year, month, year_grid) = if year_mode {
        if operands.len() > 1 {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        let year = match operands.first() {
            Some(text) => match parse_year(text) {
                Some(year) => year,
                None => {
                    eprintln!("cal: invalid year: '{text}'");
                    return util::EXIT_USAGE;
                }
            },
            None => current_year,
        };
        (year, current_month, true)
    } else {
        match operands.len() {
            0 => (current_year, current_month, false),
            1 => match parse_number(operands[0]) {
                Some(value) if (1..=12).contains(&value) => {
                    (current_year, value as i64, false)
                }
                Some(value) if (1..=9999).contains(&value) => {
                    (value as i64, current_month, true)
                }
                _ => {
                    eprintln!("cal: invalid month or year: '{}'", operands[0]);
                    return util::EXIT_USAGE;
                }
            },
            2 => {
                let Some(month) = parse_number(operands[0]) else {
                    eprintln!("cal: invalid month: '{}'", operands[0]);
                    return util::EXIT_USAGE;
                };
                if !(1..=12).contains(&month) {
                    eprintln!("cal: month must be 1..12: '{}'", operands[0]);
                    return util::EXIT_USAGE;
                }
                let Some(year) = parse_year(operands[1]) else {
                    eprintln!("cal: invalid year: '{}'", operands[1]);
                    return util::EXIT_USAGE;
                };
                (year, month as i64, false)
            }
            _ => {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            }
        }
    };

    let lines = if year_grid {
        year_lines(year)
    } else {
        month_lines(year, month)
    };
    for line in &lines {
        if let Err(err) = util::write_line(1, line.as_bytes()) {
            eprintln!("cal: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    util::EXIT_OK
}

fn usage() -> i32 {
    eprintln!("{USAGE}");
    util::EXIT_USAGE
}

fn parse_number(text: &str) -> Option<i64> {
    text.parse::<i64>().ok()
}

fn parse_year(text: &str) -> Option<i64> {
    match parse_number(text) {
        Some(year) if (1..=9999).contains(&year) => Some(year),
        _ => None,
    }
}

fn current_date() -> libpagh::Result<(i64, i64)> {
    let mut now = Timespec::default();
    sys::clock_gettime(sys::CLOCK_REALTIME, &mut now)?;
    let seconds = now.seconds.max(0);
    let days = seconds / 86_400;
    let (year, month, _day) = civil_from_days(days);
    Ok((year, month))
}

fn month_lines(year: i64, month: i64) -> Vec<String> {
    let block = month_block(year, month);
    let mut lines = block;
    // A short month leaves one or two blank week rows. Keep them for the year
    // grid (blocks must align) but do not print trailing blank lines for a
    // single month.
    while lines.len() > 2 && lines.last().map(|line| line.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines
}

fn month_block(year: i64, month: i64) -> Vec<String> {
    let title = format!("{} {}", MONTH_NAMES[(month - 1) as usize], year);
    let mut lines = Vec::new();
    lines.push(center(&title, BLOCK_WIDTH));
    lines.push(String::from(WEEK_HEADER));

    let first = days_from_civil(year, month, 1);
    let weekday = (first + 4).rem_euclid(7) as usize; // 1970-01-01 was Thursday
    let days = days_in_month(year, month);
    let mut cells: Vec<String> = Vec::new();
    for _ in 0..weekday {
        cells.push(String::from("  "));
    }
    for day in 1..=days {
        cells.push(format!("{day:2}"));
    }
    while cells.len() % 7 != 0 {
        cells.push(String::from("  "));
    }
    for week in cells.chunks(7) {
        let mut line = String::new();
        for (index, cell) in week.iter().enumerate() {
            if index > 0 {
                line.push(' ');
            }
            line.push_str(cell);
        }
        while line.ends_with(' ') {
            line.pop();
        }
        lines.push(line);
    }
    while lines.len() < 2 + 6 {
        lines.push(String::new());
    }
    lines
}

fn year_lines(year: i64) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    lines.push(center(&format!("{year}"), 3 * BLOCK_WIDTH + 4));
    lines.push(String::new());
    for quarter in 0..4 {
        let months: Vec<Vec<String>> = (0..3)
            .map(|offset| month_block(year, quarter * 3 + offset + 1))
            .collect();
        for row in 0..8 {
            let mut line = String::new();
            for (index, block) in months.iter().enumerate() {
                if index > 0 {
                    line.push_str("  ");
                }
                let text = block.get(row).map(String::as_str).unwrap_or("");
                line.push_str(text);
                let width = line.chars().count();
                let block_end = width + BLOCK_WIDTH.saturating_sub(text.chars().count());
                while line.chars().count() < block_end {
                    line.push(' ');
                }
            }
            while line.ends_with(' ') {
                line.pop();
            }
            lines.push(line);
        }
        if quarter < 3 {
            lines.push(String::new());
        }
    }
    lines
}

fn center(text: &str, width: usize) -> String {
    let length = text.chars().count();
    if length >= width {
        return String::from(text);
    }
    let mut out = String::new();
    for _ in 0..(width - length) / 2 {
        out.push(' ');
    }
    out.push_str(text);
    out
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`, with Euclidean division expressed explicitly).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era =
        year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Inverse of [`days_from_civil`] for non-negative/negative epoch days.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524
        - day_of_era / 146_096)
        / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}