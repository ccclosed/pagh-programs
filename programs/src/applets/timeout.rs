//! `timeout` — run a command with a wall-clock deadline.
//!
//! Usage: `timeout [-s SIG] DURATION COMMAND [ARG...]`. `DURATION` is the
//! `sleep` grammar (integer seconds or `N.M`); `SIG` is a name such as `TERM`
//! or a number, defaulting to `TERM`. The command is resolved with the fixed
//! pagh search (`/mnt/bin`, then `PWD`, then a literal path) and started with
//! `fork`/`execve`.
//!
//! The parent polls `wait4(WNOHANG)` every 10 ms. A child that exits first
//! supplies the exit status; at the deadline the applet exits **124**
//! (`timeout`'s conventional status). The signal option is parsed and
//! validated, but 0.16.1 cannot deliver it to a child — see the limitation
//! below.
//!
//! # Kernel 0.16.1 limitation: no cross-process kill
//!
//! `kill(2)` accepts a positive number as a **thread id in the caller's run**
//! (the only exception is the caller's own process id). A forked child has a
//! new process id and its main thread id is not that pid; passing the child pid
//! can therefore address an unrelated thread of the parent — observed as the
//! timeout process killing itself. There is no `wait4` timeout, no `alarm`, and
//! no process-group signal, so a child that outlives the deadline cannot be
//! stopped from inside the applet.
//!
//! The port therefore returns 124 at the deadline **without** calling `kill`
//! and without reaping the live child. When `timeout` is the foreground run,
//! the kernel's `cleanup_orphans` terminates that child before the shell prompt
//! returns. `-s SIG` is still parsed and validated (names or numbers) so the
//! interface is stable for a kernel that grows cross-process delivery, but it
//! has no observable effect today. This is a documented kernel gap, not a
//! hidden no-op.
//!
//! A missing command exits 127; a missing/invalid option or duration is 2.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::errno::Errno;
use libpagh::{eprintln, sys, Timespec};

use crate::util;

const USAGE: &str = "usage: timeout [-s SIG] DURATION COMMAND [ARG...]";
const TIMED_OUT: i32 = 124;
const POLL_INTERVAL: Timespec = Timespec {
    seconds: 0,
    nanos: 10_000_000,
};

pub fn run(args: &[String]) -> i32 {
    let Some((signal, duration_text, command, command_args)) = parse(args) else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };
    let Some(duration) = util::parse_duration(duration_text) else {
        eprintln!("timeout: invalid time interval: '{duration_text}'");
        return util::EXIT_USAGE;
    };

    let pid = match util::spawn(command, &command_args) {
        Ok(pid) => pid,
        Err(Errno::ENOENT) => {
            eprintln!("timeout: {command}: not found");
            return util::EXIT_NOAPPLET;
        }
        Err(err) => {
            util::report("timeout", command, err);
            return util::EXIT_FAILURE;
        }
    };

    let mut start = Timespec::default();
    if let Err(err) = sys::clock_gettime(sys::CLOCK_MONOTONIC, &mut start) {
        util::report("timeout", "clock_gettime", err);
        return util::EXIT_FAILURE;
    }

    loop {
        match sys::wait4(pid, sys::WNOHANG) {
            Ok((0, _)) => {}
            Ok((_child, status)) => return util::wait_status_code(status),
            Err(err) => {
                util::report("timeout", "wait4", err);
                return util::EXIT_FAILURE;
            }
        }

        let mut now = Timespec::default();
        if let Err(err) = sys::clock_gettime(sys::CLOCK_MONOTONIC, &mut now) {
            util::report("timeout", "clock_gettime", err);
            return util::EXIT_FAILURE;
        }
        if elapsed(&start, &now, &duration) {
            // `signal` and `pid` cannot be used together safely on 0.16.1:
            // kill(child_pid) resolves the number in the caller's thread
            // table (see the module docs). Return 124 and let the runner's
            // orphan cleanup stop the child instead of risking self-signal.
            let _ = (signal, pid);
            return TIMED_OUT;
        }

        if let Err(err) = sys::nanosleep(&POLL_INTERVAL) {
            util::report("timeout", "nanosleep", err);
            return util::EXIT_FAILURE;
        }
    }
}

/// Parse the option prefix; returns `(signal, duration, command, args)`.
fn parse<'a>(args: &'a [String]) -> Option<(i32, &'a str, &'a str, Vec<&'a str>)> {
    let mut signal = sys::SIGTERM;
    let mut index = 0usize;
    let mut parse_options = true;

    while index < args.len() {
        let arg = args[index].as_str();
        if parse_options && arg == "--" {
            parse_options = false;
            index += 1;
            continue;
        }
        if parse_options && (arg == "-s" || arg == "--signal") {
            let value = args.get(index + 1)?.as_str();
            signal = util::parse_signal(value)?;
            index += 2;
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("--signal=") {
                signal = util::parse_signal(value)?;
                index += 1;
                continue;
            }
            if let Some(value) = arg.strip_prefix("-s") {
                if !value.is_empty() {
                    signal = util::parse_signal(value)?;
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

    let duration = args.get(index)?.as_str();
    let command = args.get(index + 1)?.as_str();
    let command_args: Vec<&str> = args[index + 2..].iter().map(String::as_str).collect();
    Some((signal, duration, command, command_args))
}

/// True when `now - start` has reached `duration`.
fn elapsed(start: &Timespec, now: &Timespec, duration: &Timespec) -> bool {
    let mut seconds = now.seconds - start.seconds;
    let mut nanos = now.nanos - start.nanos;
    if nanos < 0 {
        nanos += 1_000_000_000;
        seconds -= 1;
    }
    seconds > duration.seconds || (seconds == duration.seconds && nanos >= duration.nanos)
}