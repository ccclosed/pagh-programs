//! `command` — run a command directly, or identify how it resolves.
//!
//! Usage: `command [-v|-V] COMMAND [ARG...]`. With no option the command is
//! resolved with the fixed pagh search and executed through
//! `fork`/`execve`/`wait4`; the applet exits with the child's code, or 127 when
//! the name is not found. `-v` prints the resolved absolute path (one per
//! operand, no output for a miss) and `-V` prints `name is /path` plus a
//! diagnostic for a miss. Both query modes exit 1 if any operand is not found.
//!
//! There are no shell builtins to special-case: `paghbox` is a single ELF and
//! the kernel shell has no functions or aliases. `--` ends option parsing, so a
//! command whose name begins with `-` can still be named explicitly.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::errno::Errno;
use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: command [-v|-V] COMMAND [ARG...]";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Run,
    Terse,
    Verbose,
}

pub fn run(args: &[String]) -> i32 {
    run_named("command", args, Mode::Run, USAGE)
}

/// Shared implementation for `command` (default: run) and `type` (default:
/// describe). The label is used in diagnostics and the usage text.
pub(crate) fn run_named(label: &str, args: &[String], default_mode: Mode, usage: &str) -> i32 {
    let mut mode = default_mode;
    let mut index = 0usize;
    let mut parse_options = true;
    let mut flag_seen = false;

    while index < args.len() {
        let arg = args[index].as_str();
        if parse_options && arg == "--" {
            parse_options = false;
            index += 1;
            continue;
        }
        if parse_options && arg == "-v" {
            mode = Mode::Terse;
            flag_seen = true;
            index += 1;
            continue;
        }
        if parse_options && arg == "-V" {
            mode = Mode::Verbose;
            flag_seen = true;
            index += 1;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            // Accept combined short forms such as `-vV`, but nothing else.
            let mut combined = true;
            let mut last = None;
            for byte in arg[1..].bytes() {
                match byte {
                    b'v' => last = Some(Mode::Terse),
                    b'V' => last = Some(Mode::Verbose),
                    _ => {
                        combined = false;
                        break;
                    }
                }
            }
            if combined {
                if let Some(parsed) = last {
                    mode = parsed;
                    flag_seen = true;
                }
                index += 1;
                continue;
            }
            eprintln!("{usage}");
            return util::EXIT_USAGE;
        }
        break;
    }

    let operands = &args[index..];
    let query = flag_seen || default_mode != Mode::Run;
    if query {
        if operands.is_empty() {
            eprintln!("{usage}");
            return util::EXIT_USAGE;
        }
        return query_mode(label, mode, operands);
    }

    let Some(command) = operands.first().map(String::as_str) else {
        eprintln!("{usage}");
        return util::EXIT_USAGE;
    };
    let command_args: Vec<&str> = operands[1..].iter().map(String::as_str).collect();
    match util::spawn_wait(command, &command_args) {
        Ok(code) => code,
        Err(Errno::ENOENT) => {
            eprintln!("{label}: {command}: not found");
            util::EXIT_NOAPPLET
        }
        Err(err) => {
            util::report(label, command, err);
            util::EXIT_FAILURE
        }
    }
}

fn query_mode(label: &str, mode: Mode, operands: &[String]) -> i32 {
    let mut status = util::EXIT_OK;
    for name in operands {
        match util::which(name) {
            Ok(path) => {
                let line = match mode {
                    Mode::Terse => path,
                    Mode::Verbose => format!("{name} is {path}"),
                    Mode::Run => path,
                };
                if let Err(err) = util::write_line(1, line.as_bytes()) {
                    eprintln!("{label}: write error: {err}");
                    return util::EXIT_FAILURE;
                }
            }
            Err(Errno::ENOENT) => {
                if mode == Mode::Verbose {
                    eprintln!("{label}: {name}: not found");
                }
                status = util::EXIT_FAILURE;
            }
            Err(err) => {
                util::report(label, name, err);
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}