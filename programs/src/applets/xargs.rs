//! `xargs` — build command lines from whitespace-separated arguments.
//!
//! Usage: `xargs [-a FILE] [-n N] [-I{}] [-r] [COMMAND [ARG...]]`. The default
//! command is `echo`. Arguments are read from stdin unless `-a FILE` supplies a
//! file; `-a` is a port extension because the console fd 0 has **no EOF** (see
//! the README), so a stdin `xargs` would wait forever. An empty file with `-r`
//! runs nothing; without `-r` the command runs once with no arguments.
//!
//! `-n N` limits the arguments per invocation (default: all of them). `-I{}`
//! (the placeholder can be any non-empty string) replaces every occurrence of
//! the placeholder in the command's *arguments* and runs once per input token;
//! the command name itself is not rewritten. Input is split on ASCII
//! whitespace, with no shell-style quoting: the kernel shell has no quoting
//! either, so this keeps the two layers consistent.
//!
//! Each invocation goes through `util::spawn_wait` (fork/execve/wait4). Exit:
//! `0` all succeeded, `123` an invocation failed, `124` the command could not
//! run at all (not found, fork failure, or child `execve` failure 127), `2`
//! usage. `-a FILE` that cannot be read is 1.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::errno::Errno;
use libpagh::eprintln;

use crate::util;

pub fn run(args: &[String]) -> i32 {
    run_as("xargs", args, None)
}

/// Shared body used by `parallel` with `force_max = Some(1)`.
pub(crate) fn run_as(program: &str, args: &[String], force_max: Option<usize>) -> i32 {
    let usage = format!(
        "usage: {program} [-a FILE] [-n N] [-I{{}}] [-r] [COMMAND [ARG...]]"
    );
    let Some((input, max_args, replace, no_run_empty, command)) = parse(args) else {
        eprintln!("{usage}");
        return util::EXIT_USAGE;
    };
    let max_args = force_max.or(max_args);

    let data = match input {
        Some(name) => match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report(program, util::display_name(name), err);
                return util::EXIT_FAILURE;
            }
        },
        None => match util::read_all(0) {
            Ok(data) => data,
            Err(err) => {
                eprintln!("{program}: standard input: {err}");
                return util::EXIT_FAILURE;
            }
        },
    };

    let mut tokens: Vec<String> = Vec::new();
    for part in data.split(|byte| byte.is_ascii_whitespace()) {
        if part.is_empty() {
            continue;
        }
        let token = String::from_utf8_lossy(part).into_owned();
        if util::push(&mut tokens, token).is_err() {
            eprintln!("{program}: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if tokens.is_empty() && no_run_empty {
        return util::EXIT_OK;
    }

    // Missing command: report once instead of once per invocation.
    let resolved = match util::which(&command[0]) {
        Ok(path) => path,
        Err(Errno::ENOENT) => {
            eprintln!("{program}: {}: not found", command[0]);
            return 124;
        }
        Err(err) => {
            util::report(program, &command[0], err);
            return 124;
        }
    };

    if tokens.is_empty() {
        // The ordinary xargs default: one invocation with no arguments.
        return merge_status(util::EXIT_OK, invoke(program, &resolved, &[]));
    }

    let mut status = util::EXIT_OK;
    if let Some(placeholder) = replace {
        for token in &tokens {
            let expanded: Vec<String> = command[1..]
                .iter()
                .map(|template| template.replace(placeholder, token))
                .collect();
            let args: Vec<&str> = expanded.iter().map(String::as_str).collect();
            status = merge_status(status, invoke(program, &resolved, &args));
        }
        return status;
    }

    let chunk_size = max_args.unwrap_or(tokens.len().max(1));
    let mut start = 0usize;
    while start < tokens.len() {
        let end = (start + chunk_size).min(tokens.len());
        let args: Vec<&str> = tokens[start..end].iter().map(String::as_str).collect();
        status = merge_status(status, invoke(program, &resolved, &args));
        start = end;
    }
    status
}

/// One invocation, classified for the xargs exit convention.
fn invoke(program: &str, resolved: &str, args: &[&str]) -> i32 {
    match util::spawn_wait(resolved, args) {
        Ok(0) => util::EXIT_OK,
        Ok(127) => 124,
        Ok(_) => 123,
        Err(err) => {
            util::report(program, resolved, err);
            124
        }
    }
}

/// `124` (cannot run) dominates `123` (a command failed), which dominates 0.
fn merge_status(current: i32, new: i32) -> i32 {
    if new == 124 || current == 124 {
        124
    } else if new != 0 || current != 0 {
        123
    } else {
        util::EXIT_OK
    }
}

/// Parse options; returns `(input, max_args, replace, no_run_empty, command)`.
fn parse<'a>(
    args: &'a [String],
) -> Option<(
    Option<&'a str>,
    Option<usize>,
    Option<&'a str>,
    bool,
    Vec<String>,
)> {
    let mut input: Option<&str> = None;
    let mut max_args: Option<usize> = None;
    let mut replace: Option<&str> = None;
    let mut no_run_empty = false;
    let mut command: Vec<String> = Vec::new();
    let mut index = 0usize;
    let mut parse_options = true;

    while index < args.len() {
        let arg = args[index].as_str();
        if parse_options && arg == "--" {
            parse_options = false;
            index += 1;
            continue;
        }
        if parse_options && (arg == "-a" || arg == "--arg-file") {
            let value = args.get(index + 1)?.as_str();
            if value.is_empty() {
                return None;
            }
            input = Some(value);
            index += 2;
            continue;
        }
        if parse_options && (arg == "-n" || arg == "--max-args") {
            let value = args.get(index + 1)?.as_str();
            max_args = Some(parse_positive(value)?);
            index += 2;
            continue;
        }
        if parse_options && arg == "-I" {
            let value = args.get(index + 1)?.as_str();
            if value.is_empty() {
                return None;
            }
            replace = Some(value);
            index += 2;
            continue;
        }
        if parse_options && (arg == "-r" || arg == "--no-run-if-empty") {
            no_run_empty = true;
            index += 1;
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("--arg-file=") {
                if value.is_empty() {
                    return None;
                }
                input = Some(value);
                index += 1;
                continue;
            }
            if let Some(value) = arg.strip_prefix("--max-args=") {
                max_args = Some(parse_positive(value)?);
                index += 1;
                continue;
            }
            if let Some(value) = arg.strip_prefix("-a") {
                if !value.is_empty() {
                    input = Some(value);
                    index += 1;
                    continue;
                }
            }
            if let Some(value) = arg.strip_prefix("-n") {
                if !value.is_empty() {
                    max_args = Some(parse_positive(value)?);
                    index += 1;
                    continue;
                }
            }
            if let Some(value) = arg.strip_prefix("-I") {
                if !value.is_empty() {
                    replace = Some(value);
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

    while index < args.len() {
        if util::push(&mut command, String::from(args[index].as_str())).is_err() {
            return None;
        }
        index += 1;
    }
    if command.is_empty() {
        if util::push(&mut command, String::from("echo")).is_err() {
            return None;
        }
    }
    Some((input, max_args, replace, no_run_empty, command))
}

fn parse_positive(text: &str) -> Option<usize> {
    let value = util::parse_usize(text)?;
    if value == 0 {
        None
    } else {
        Some(value)
    }
}