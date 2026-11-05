//! `time` — measure how long a command took.
//!
//! Usage: `time COMMAND [ARG...]`. The command is started with the shared
//! `fork`/`execve` helper and `clock_gettime(CLOCK_MONOTONIC)` is read before
//! and after. Three lines go to fd 2 (GNU `time` also writes its report to
//! stderr, so a command's stdout stays clean):
//!
//! ```text
//! real 0.031s
//! user 0.000s
//! sys 0.000s
//! ```
//!
//! The `real` value is rounded to milliseconds with integer arithmetic. The
//! ABI has no per-process CPU accounting (`times`/`getrusage` are absent), so
//! `user` and `sys` are honest zeros rather than invented numbers. The child's
//! exit code is returned; a missing command is 127, and a missing operand is
//! usage error 2.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::errno::Errno;
use libpagh::{eprintln, sys, Timespec};

use crate::util;

const USAGE: &str = "usage: time COMMAND [ARG...]";

pub fn run(args: &[String]) -> i32 {
    let mut index = 0usize;
    if args.first().map(String::as_str) == Some("--") {
        index = 1;
    }
    let Some(command) = args.get(index).map(String::as_str) else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };
    let command_args: Vec<&str> = args[index + 1..].iter().map(String::as_str).collect();

    let mut start = Timespec::default();
    if let Err(err) = sys::clock_gettime(sys::CLOCK_MONOTONIC, &mut start) {
        util::report("time", "clock_gettime", err);
        return util::EXIT_FAILURE;
    }

    let result = util::spawn_wait(command, &command_args);

    let mut end = start;
    if let Err(err) = sys::clock_gettime(sys::CLOCK_MONOTONIC, &mut end) {
        util::report("time", "clock_gettime", err);
        end = start;
    }
    print_times(&start, &end);

    match result {
        Ok(code) => code,
        Err(Errno::ENOENT) => {
            eprintln!("time: {command}: not found");
            util::EXIT_NOAPPLET
        }
        Err(err) => {
            util::report("time", command, err);
            util::EXIT_FAILURE
        }
    }
}

fn print_times(start: &Timespec, end: &Timespec) {
    let mut seconds = end.seconds - start.seconds;
    let mut nanos = end.nanos - start.nanos;
    if nanos < 0 {
        nanos += 1_000_000_000;
        seconds -= 1;
    }
    if seconds < 0 {
        seconds = 0;
        nanos = 0;
    }
    let mut millis = (nanos + 500_000) / 1_000_000;
    if millis >= 1000 {
        millis -= 1000;
        seconds += 1;
    }
    eprintln!("real {seconds}.{millis:03}s");
    eprintln!("user 0.000s");
    eprintln!("sys 0.000s");
}