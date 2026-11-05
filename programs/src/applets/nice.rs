//! `nice` — run a command with a (ignored) scheduling adjustment.
//!
//! Usage: `nice [-n N] COMMAND [ARG...]`. `N` is parsed as a signed integer so
//! the usual `-n 10` / `-n -5` spellings are accepted, then deliberately
//! ignored: the pagh ABI has **no process priorities** (`setpriority`/
//! `getpriority` do not exist), and the scheduler has a single run queue. The
//! catalog calls this a no-op, and the module says so instead of pretending to
//! change anything.
//!
//! The command is started through the shared `fork`/`execve` helper and its
//! exit code is returned. A missing command is 127, a missing operand or bad
//! adjustment is usage error 2. `--` ends option parsing.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::errno::Errno;
use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: nice [-n N] COMMAND [ARG...]";

pub fn run(args: &[String]) -> i32 {
    let mut index = 0usize;
    let mut parse_options = true;
    let mut adjustment: i32 = 10;

    while index < args.len() {
        let arg = args[index].as_str();
        if parse_options && arg == "--" {
            parse_options = false;
            index += 1;
            continue;
        }
        if parse_options && (arg == "-n" || arg == "--adjustment") {
            let Some(value) = args.get(index + 1).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            match value.parse::<i32>() {
                Ok(parsed) => adjustment = parsed,
                Err(_) => {
                    eprintln!("nice: invalid adjustment: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            index += 2;
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("--adjustment=") {
                match value.parse::<i32>() {
                    Ok(parsed) => {
                        adjustment = parsed;
                        index += 1;
                        continue;
                    }
                    Err(_) => {
                        eprintln!("nice: invalid adjustment: '{value}'");
                        return util::EXIT_USAGE;
                    }
                }
            }
            if let Some(value) = arg.strip_prefix("-n") {
                if !value.is_empty() {
                    match value.parse::<i32>() {
                        Ok(parsed) => {
                            adjustment = parsed;
                            index += 1;
                            continue;
                        }
                        Err(_) => {
                            eprintln!("nice: invalid adjustment: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        break;
    }

    let Some(command) = args.get(index).map(String::as_str) else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };
    let command_args: Vec<&str> = args[index + 1..].iter().map(String::as_str).collect();

    // The value is intentionally unused: there is no setpriority in the ABI.
    let _ = adjustment;

    match util::spawn_wait(command, &command_args) {
        Ok(code) => code,
        Err(Errno::ENOENT) => {
            eprintln!("nice: {command}: not found");
            util::EXIT_NOAPPLET
        }
        Err(err) => {
            util::report("nice", command, err);
            util::EXIT_FAILURE
        }
    }
}