//! `watch` — run a command repeatedly and show its output.
//!
//! Usage: `watch [-n SEC] [-t] [-c N] COMMAND [ARG...]`. The default interval
//! is 2 seconds (`-n` accepts the `sleep` grammar), and the screen is cleared
//! before each run with the ANSI sequence `ESC[2J ESC[H`. `-t` suppresses the
//! `Every <interval>: <command>` header. `-c N` is a port extension that stops
//! after `N` iterations; without it the loop runs until the process is killed
//! (Ctrl-C on the console terminates it).
//!
//! # No TTY detection
//!
//! The ABI has no `isatty`, so the ANSI clear is always written to fd 1, even
//! if it were redirected (the pagh shell has no redirection anyway). The raw
//! escape bytes are part of the transcript. Every iteration is a fresh
//! `fork`/`execve` of the resolved command, so state does not leak between
//! runs.
//!
//! A command that cannot be resolved is a runtime error (1); a bad option or
//! interval is a usage error (2). A non-zero child exit does not stop the
//! loop, matching `watch`'s "keep observing" behaviour.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::errno::Errno;
use libpagh::{eprintln, sys, Timespec};

use crate::util;

const USAGE: &str = "usage: watch [-n SEC] [-t] [-c N] COMMAND [ARG...]";

pub fn run(args: &[String]) -> i32 {
    let Some((interval, no_header, count, command, command_args)) = parse(args) else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };

    if count == Some(0) {
        return util::EXIT_OK;
    }

    let header = if no_header {
        None
    } else {
        Some(format!(
            "Every {}: {}",
            format_duration(&interval),
            command_line(command, &command_args)
        ))
    };

    let mut iterations = 0usize;
    loop {
        if let Err(err) = util::write_all(1, b"\x1b[2J\x1b[H") {
            eprintln!("watch: write error: {err}");
            return util::EXIT_FAILURE;
        }
        if let Some(header) = &header {
            if let Err(err) = util::write_line(1, header.as_bytes()) {
                eprintln!("watch: write error: {err}");
                return util::EXIT_FAILURE;
            }
        }

        match util::spawn_wait(command, &command_args) {
            Ok(_status) => {}
            Err(Errno::ENOENT) => {
                eprintln!("watch: {command}: not found");
                return util::EXIT_FAILURE;
            }
            Err(err) => {
                util::report("watch", command, err);
                return util::EXIT_FAILURE;
            }
        }

        iterations += 1;
        if let Some(limit) = count {
            if iterations >= limit {
                return util::EXIT_OK;
            }
        }
        if let Err(err) = sys::nanosleep(&interval) {
            util::report("watch", "nanosleep", err);
            return util::EXIT_FAILURE;
        }
    }
}

/// Parse options; returns `(interval, no_header, count, command, args)`.
fn parse<'a>(
    args: &'a [String],
) -> Option<(Timespec, bool, Option<usize>, &'a str, Vec<&'a str>)> {
    let mut interval = Timespec {
        seconds: 2,
        nanos: 0,
    };
    let mut no_header = false;
    let mut count: Option<usize> = None;
    let mut index = 0usize;
    let mut parse_options = true;

    while index < args.len() {
        let arg = args[index].as_str();
        if parse_options && arg == "--" {
            parse_options = false;
            index += 1;
            continue;
        }
        if parse_options && (arg == "-n" || arg == "--interval") {
            let value = args.get(index + 1)?.as_str();
            interval = util::parse_duration(value)?;
            index += 2;
            continue;
        }
        if parse_options && (arg == "-c" || arg == "--count") {
            let value = args.get(index + 1)?.as_str();
            count = Some(value.parse::<usize>().ok()?);
            index += 2;
            continue;
        }
        if parse_options && (arg == "-t" || arg == "--no-title") {
            no_header = true;
            index += 1;
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("--interval=") {
                interval = util::parse_duration(value)?;
                index += 1;
                continue;
            }
            if let Some(value) = arg.strip_prefix("--count=") {
                count = Some(value.parse::<usize>().ok()?);
                index += 1;
                continue;
            }
            if let Some(value) = arg.strip_prefix("-n") {
                if !value.is_empty() {
                    interval = util::parse_duration(value)?;
                    index += 1;
                    continue;
                }
            }
            if let Some(value) = arg.strip_prefix("-c") {
                if !value.is_empty() {
                    count = Some(value.parse::<usize>().ok()?);
                    index += 1;
                    continue;
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            return None;
        }
        break;
    }

    let command = args.get(index)?.as_str();
    let command_args: Vec<&str> = args[index + 1..].iter().map(String::as_str).collect();
    Some((interval, no_header, count, command, command_args))
}

/// Join the command and its arguments with single spaces, for the header.
fn command_line(command: &str, args: &[&str]) -> String {
    let mut line = String::from(command);
    for arg in args {
        line.push(' ');
        line.push_str(arg);
    }
    line
}

/// `2s` for a whole interval, otherwise `0.500s` with trailing zeroes removed.
fn format_duration(duration: &Timespec) -> String {
    let mut fraction = format!("{:09}", duration.nanos.max(0));
    while fraction.ends_with('0') {
        fraction.pop();
    }
    if fraction.is_empty() {
        format!("{}s", duration.seconds)
    } else {
        format!("{}.{}s", duration.seconds, fraction)
    }
}