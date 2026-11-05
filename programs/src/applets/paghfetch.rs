//! `paghfetch` — a small, honest system banner.
//!
//! ```text
//!      +------+
//!      | pagh |    paghfetch
//!      +------+    sysname:  pagh
//!                  nodename: pagh-riscv
//!                  release:  0.10.0
//!                  machine:  riscv64
//!                  uptime:   42s
//!                  nproc:    1
//!                  pwd:      /mnt
//! ```
//!
//! Every value comes from the kernel or the process environment: `sysname`,
//! `nodename`, `release` and `machine` from `uname`, `uptime` from
//! `clock_gettime(CLOCK_MONOTONIC)`, `nproc` from the platform's one-hart
//! policy (there is no affinity syscall) and `pwd` from `PWD`. There is no
//! distribution name, CPU model, memory figure or package count to report, so
//! none is invented. The box is drawn from the actual `sysname`, not a
//! hard-coded string; the uptime is `?s` if the clock call fails.
//!
//! `uname` must succeed (exit 1 otherwise); an operand is a usage error (2).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::{env, eprintln, sys, Timespec, Uname};

use crate::util;

const USAGE: &str = "usage: paghfetch";
const LABEL_WIDTH: usize = 10;

pub fn run(args: &[String]) -> i32 {
    if !args.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut info = Uname::default();
    if let Err(err) = sys::uname(&mut info) {
        util::report("paghfetch", "uname", err);
        return util::EXIT_FAILURE;
    }

    let mut now = Timespec::default();
    let uptime = match sys::clock_gettime(sys::CLOCK_MONOTONIC, &mut now) {
        Ok(()) => format!("{}s", now.seconds.max(0)),
        Err(_) => String::from("?s"),
    };
    let pwd = env::current_dir();

    let fields = [
        ("sysname:", info.sysname()),
        ("nodename:", info.nodename()),
        ("release:", info.release()),
        ("machine:", info.machine()),
        ("uptime:", uptime.as_str()),
        ("nproc:", "1"),
        ("pwd:", pwd.as_str()),
    ];

    let mut lines: Vec<String> = Vec::new();
    let mut push = |line: String| -> bool { util::push(&mut lines, line).is_ok() };
    if !push(String::from("paghfetch")) {
        eprintln!("paghfetch: out of memory");
        return util::EXIT_FAILURE;
    }
    for (label, value) in fields {
        if !push(format!("{label:<LABEL_WIDTH$} {value}")) {
            eprintln!("paghfetch: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let art = banner(info.sysname());
    let art_width = art.first().map_or(0, String::len);
    let rows = lines.len().max(art.len());
    for row in 0..rows {
        let left = art.get(row).map(String::as_str).unwrap_or("");
        let right = lines.get(row).map(String::as_str).unwrap_or("");
        let line = if right.is_empty() {
            String::from(left)
        } else if left.is_empty() {
            format!("{}{right}", " ".repeat(art_width + 3))
        } else {
            format!("{left}   {right}")
        };
        if let Err(err) = util::write_line(1, line.as_bytes()) {
            eprintln!("paghfetch: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    util::EXIT_OK
}

/// Draw the three-line box around the real `sysname`.
fn banner(sysname: &str) -> Vec<String> {
    let field = sysname.len().max(4) + 2;
    let border = format!("      +{}+", "-".repeat(field));
    let padding = field.saturating_sub(sysname.len());
    let left = padding / 2;
    let right = padding - left;
    let middle = format!(
        "      |{}{}{}|",
        " ".repeat(left),
        sysname,
        " ".repeat(right)
    );
    let mut rows = Vec::new();
    if util::push(&mut rows, border.clone()).is_err() {
        return rows;
    }
    if util::push(&mut rows, middle).is_err() {
        return rows;
    }
    let _ = util::push(&mut rows, border);
    rows
}