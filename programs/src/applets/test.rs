//! `test` / `[` — file and integer/string predicate evaluation.
//!
//! Unary predicates: `-e -f -d -s -L -r -w -x -z -n`. Binary predicates:
//! `=` `!=` (string equality) and `-eq -ne -lt -le -gt -ge` (decimal `i64`
//! comparison). `!` negates, and `(` `)` group — when the kernel shell passes
//! them as separate words, as in `test ! ( -f a )` (the shell has no
//! redirection, so plain `(`/`)` are ordinary arguments).
//!
//! `[` is the same evaluator and requires the final argument to be `]`; a
//! missing bracket is a usage error (2). Exit status is 0 for true, 1 for
//! false, 2 for a syntax error (including a non-numeric operand of a numeric
//! comparison). `test` with no arguments is false.
//!
//! File metadata comes from `fs::stat`, which follows a final symlink and
//! needs read permission on the target; `-L` is `read_link` (the link itself),
//! and `-e` accepts a broken link as existing because the link entry does. The
//! ABI has no `lstat`, no `-nt`/`-ot` timestamps and no access(2); `-r/-w/-x`
//! therefore inspect the low mode bits rather than performing an access check.
//! `-a`/`-o` and `-t`/`-p`/`-S` are not implemented. No pipes/redirection.

use alloc::string::String;

use libpagh::eprintln;
use libpagh::fs;

use crate::util;

const TEST_USAGE: &str = "usage: test EXPRESSION";
const BRACKET_USAGE: &str = "usage: [ EXPRESSION ]";

pub fn run(args: &[String], bracket: bool) -> i32 {
    let program = if bracket { "[" } else { "test" };

    let tokens: &[String] = if bracket {
        match args.last().map(String::as_str) {
            Some("]") => &args[..args.len() - 1],
            _ => {
                eprintln!("[: missing ']'");
                eprintln!("{BRACKET_USAGE}");
                return util::EXIT_USAGE;
            }
        }
    } else {
        args
    };

    match evaluate(tokens) {
        Ok(true) => util::EXIT_OK,
        Ok(false) => util::EXIT_FAILURE,
        Err(()) => {
            eprintln!("{program}: syntax error");
            eprintln!("{}", if bracket { BRACKET_USAGE } else { TEST_USAGE });
            util::EXIT_USAGE
        }
    }
}

fn evaluate(tokens: &[String]) -> Result<bool, ()> {
    if tokens.is_empty() {
        return Ok(false);
    }
    if tokens[0] == "!" {
        return Ok(!evaluate(&tokens[1..])?);
    }
    if tokens.len() >= 2 && tokens[0] == "(" && tokens[tokens.len() - 1] == ")" {
        return evaluate(&tokens[1..tokens.len() - 1]);
    }

    match tokens.len() {
        1 => Ok(!tokens[0].is_empty()),
        2 => {
            if is_unary(&tokens[0]) {
                Ok(unary(&tokens[0], &tokens[1]))
            } else {
                Err(())
            }
        }
        3 => {
            if is_binary(&tokens[1]) {
                binary(&tokens[1], &tokens[0], &tokens[2])
            } else {
                Err(())
            }
        }
        _ => Err(()),
    }
}

fn is_unary(operator: &str) -> bool {
    matches!(
        operator,
        "-e" | "-f" | "-d" | "-s" | "-L" | "-h" | "-r" | "-w" | "-x" | "-z" | "-n"
    )
}

fn unary(operator: &str, operand: &str) -> bool {
    match operator {
        "-e" => exists(operand),
        "-f" => fs::stat(operand).map(|stat| stat.is_file()).unwrap_or(false),
        "-d" => fs::stat(operand).map(|stat| stat.is_dir()).unwrap_or(false),
        "-s" => fs::stat(operand)
            .map(|stat| stat.size > 0)
            .unwrap_or(false),
        "-L" | "-h" => fs::read_link(operand).is_ok(),
        "-r" => mode(operand).map(|bits| bits & 0o444 != 0).unwrap_or(false),
        "-w" => mode(operand).map(|bits| bits & 0o222 != 0).unwrap_or(false),
        "-x" => mode(operand).map(|bits| bits & 0o111 != 0).unwrap_or(false),
        "-z" => operand.is_empty(),
        "-n" => !operand.is_empty(),
        _ => false,
    }
}

fn exists(path: &str) -> bool {
    // A broken symlink has no stat-able target but does exist as a directory
    // entry; read_link is the only ABI call that sees it.
    fs::read_link(path).is_ok() || fs::stat(path).is_ok()
}

fn mode(path: &str) -> Option<u32> {
    fs::stat(path).ok().map(|stat| stat.mode)
}

fn is_binary(operator: &str) -> bool {
    matches!(
        operator,
        "=" | "!=" | "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge"
    )
}

fn binary(operator: &str, left: &str, right: &str) -> Result<bool, ()> {
    match operator {
        "=" => Ok(left == right),
        "!=" => Ok(left != right),
        _ => {
            let left = left.parse::<i64>().map_err(|_| ())?;
            let right = right.parse::<i64>().map_err(|_| ())?;
            Ok(match operator {
                "-eq" => left == right,
                "-ne" => left != right,
                "-lt" => left < right,
                "-le" => left <= right,
                "-gt" => left > right,
                "-ge" => left >= right,
                _ => return Err(()),
            })
        }
    }
}