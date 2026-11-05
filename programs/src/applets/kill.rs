//! `kill` — send a signal to one or more processes.
//!
//! Usage: `kill [-SIGNAL] PID...` (also `kill -s SIGNAL PID...`). The default
//! is `TERM`; named signals are `HUP`, `INT`, `QUIT`, `KILL`, `USR1`, `USR2`,
//! `ALRM`, `TERM`, `CHLD`, `CONT` and `STOP` (with or without a `SIG` prefix),
//! plus numeric signals `0..=64`. `-0` performs the existence/permission check
//! without delivering anything.
//!
//! The kernel's `kill(pid, sig)` accepts a positive thread id, `0` for the
//! calling thread group and `-1` for every thread of the run; this applet
//! accepts `0` and positive pids, and rejects negative pids (with `--` they
//! could otherwise be mistaken for options). Every operand is attempted even
//! after a failure; each failure prints `kill: PID: <errno>` and the applet
//! exits 1. `ESRCH` is therefore `No such process`, as in the rest of the
//! bundle.
//!
//! The kernel shell has no `$$`, so the safe live probe is `kill -0 0`; a bad
//! pid checks the ESRCH path. Exit: 0 all sent, 1 any failure, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::errno::Errno;
use libpagh::sys;

use crate::util;

const USAGE: &str = "usage: kill [-SIGNAL] PID...";

pub fn run(args: &[String]) -> i32 {
    let mut signal = sys::SIGTERM;
    let mut pids: Vec<&str> = Vec::new();
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            let spec = &arg[1..];
            if spec == "s" {
                let Some(value) = args.get(index).map(String::as_str) else {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                };
                index += 1;
                match util::parse_signal(value) {
                    Some(parsed) => signal = parsed,
                    None => {
                        eprintln!("kill: invalid signal: '{value}'");
                        return util::EXIT_USAGE;
                    }
                }
            } else {
                match util::parse_signal(spec) {
                    Some(parsed) => signal = parsed,
                    None => {
                        eprintln!("kill: invalid signal: '{spec}'");
                        return util::EXIT_USAGE;
                    }
                }
            }
            continue;
        }
        parse_options = false;
        if util::push(&mut pids, arg).is_err() {
            eprintln!("kill: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if pids.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut parsed: Vec<(i32, &str)> = Vec::new();
    for text in pids {
        match text.parse::<i32>() {
            Ok(pid) if pid >= 0 => {
                if util::push(&mut parsed, (pid, text)).is_err() {
                    eprintln!("kill: out of memory");
                    return util::EXIT_FAILURE;
                }
            }
            _ => {
                eprintln!("kill: invalid pid: '{text}'");
                return util::EXIT_USAGE;
            }
        }
    }

    let mut status = util::EXIT_OK;
    for (pid, text) in parsed {
        match sys::kill(pid, signal) {
            Ok(()) => {}
            // The frozen errno table has no ESRCH (raw 3), so libpagh maps the
            // kernel's "no such task" answer to `Unknown`. For kill that is
            // the ESRCH path; print the conventional message instead of the
            // generic "outside the pagh ABI" diagnostic.
            Err(Errno::Unknown) => {
                eprintln!("kill: {text}: No such process");
                status = util::EXIT_FAILURE;
            }
            Err(err) => {
                util::report("kill", text, err);
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}
