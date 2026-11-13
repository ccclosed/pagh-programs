//! `ps` — list processes, read from `/proc`.
//!
//! The kernel's procfs (`pagh-riscv` 0.22–0.23, PRs #9/#10) exposes one
//! directory per live process: `/proc/<pid>/{status,stat,cmdline}`. This applet
//! scans `/proc` for numeric entries and prints the busybox-simplified table
//! `PID TTY STAT COMMAND`:
//!
//! * `PID` is the directory name (the same id `getpid()` returns);
//! * `TTY` is always `?`: this ABI has no tty, no controlling terminal and no
//!   process groups, so there is nothing true to print — `?` is what `ps`
//!   prints when it does not know either;
//! * `STAT` is the aggregate state character from `/proc/<pid>/stat`
//!   (`R`/`S`/`T`);
//! * `COMMAND` is the argv snapshot from `/proc/<pid>/cmdline` (NUL-separated)
//!   joined with spaces, or `[comm]` when the snapshot is empty.
//!
//! Kernel threads (`/proc/kthreads/<tid>`) are **not** processes: they have no
//! pid, so they are not in the table. `/proc/self` is a symbolic link and is
//! skipped; a pid that disappears between `readdir` and `open` (a normal race)
//! is skipped silently, never reported as an error.
//!
//! `--uptime` is a port extension: it prints the kernel's `/proc/uptime` line
//! (seconds since boot, centisecond field) and exits — the applet equivalent of
//! a system-wide "how long has this been up" probe, without inventing a TIME
//! column (`/proc/<pid>/stat` has no start time, so a per-process TIME would be
//! a lie).
//!
//! Exit: 0 success; 1 `/proc` missing or a file that exists but cannot be read;
//! 2 usage.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;
use libpagh::fs::{Dir, EntryKind};

use crate::util;

const USAGE: &str = "usage: ps [--uptime]";

struct Process {
    pid: u32,
    state: char,
    command: String,
}

pub fn run(args: &[String]) -> i32 {
    let mut uptime = false;
    for arg in args {
        match arg.as_str() {
            "--uptime" => uptime = true,
            _ => {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            }
        }
    }

    if uptime {
        return print_uptime();
    }

    let directory = match Dir::open("/proc") {
        Ok(directory) => directory,
        Err(err) => {
            eprintln!("ps: /proc: {err} (the kernel needs the procfs ABI)");
            return util::EXIT_FAILURE;
        }
    };

    let mut pids: Vec<u32> = Vec::new();
    for entry in directory {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("ps: /proc: {err}");
                return util::EXIT_FAILURE;
            }
        };
        if !matches!(entry.kind(), EntryKind::Dir) {
            continue;
        }
        // Numeric directories are processes; `kthreads`/`net` are not pids and
        // `/proc/self` is a symlink (filtered by the kind check already).
        if let Ok(pid) = entry.name().parse::<u32>() {
            if util::push(&mut pids, pid).is_err() {
                eprintln!("ps: out of memory");
                return util::EXIT_FAILURE;
            }
        }
    }
    pids.sort_unstable();

    let mut processes: Vec<Process> = Vec::new();
    let mut status = util::EXIT_OK;
    for pid in pids {
        let path = format!("/proc/{pid}/stat");
        let line = match fs::read_to_string(&path) {
            Ok(line) => line,
            // The process exited between readdir and open: a race, not an error.
            Err(libpagh::Errno::ENOENT) => continue,
            Err(err) => {
                util::report("ps", &path, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        let Some((_pid, comm, state)) = parse_stat(&line) else {
            continue;
        };
        let command = match read_command(pid) {
            Some(command) if !command.is_empty() => command,
            _ => format!("[{comm}]"),
        };
        if util::push(
            &mut processes,
            Process {
                pid,
                state,
                command,
            },
        )
        .is_err()
        {
            eprintln!("ps: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let mut out = String::new();
    out.push_str("  PID TTY      STAT COMMAND\n");
    for process in &processes {
        out.push_str(&format!(
            "{:>5} {:<8} {:<4} ",
            process.pid, "?", process.state
        ));
        out.push_str(&process.command);
        out.push('\n');
    }
    if let Err(err) = util::write_all(1, out.as_bytes()) {
        eprintln!("ps: write error: {err}");
        return util::EXIT_FAILURE;
    }
    status
}

/// Print the kernel's `/proc/uptime` as `uptime: <seconds>s`.
fn print_uptime() -> i32 {
    let text = match fs::read_to_string("/proc/uptime") {
        Ok(text) => text,
        Err(err) => {
            util::report("ps", "/proc/uptime", err);
            return util::EXIT_FAILURE;
        }
    };
    let seconds = text.split_whitespace().next().unwrap_or("");
    if seconds.is_empty() {
        eprintln!("ps: /proc/uptime: empty");
        return util::EXIT_FAILURE;
    }
    let line = format!("uptime: {seconds}s\n");
    match util::write_all(1, line.as_bytes()) {
        Ok(()) => util::EXIT_OK,
        Err(err) => {
            eprintln!("ps: write error: {err}");
            util::EXIT_FAILURE
        }
    }
}

/// The argv snapshot at `/proc/<pid>/cmdline`, NUL-separated per the kernel's
/// Linux-shaped contract; `None` when the file cannot be read.
fn read_command(pid: u32) -> Option<String> {
    let path = format!("/proc/{pid}/cmdline");
    let bytes = fs::read_to_end(&path).ok()?;
    let mut command = String::new();
    let mut first = true;
    for argument in bytes.split(|byte| *byte == 0) {
        if argument.is_empty() {
            continue;
        }
        if !first {
            command.push(' ');
        }
        command.push_str(&String::from_utf8_lossy(argument));
        first = false;
    }
    Some(command)
}

/// Parse the native one-line `/proc/<pid>/stat`:
/// `id (comm) state ppid threads switches work stack_kib satp`.
///
/// `comm` is parenthesised and the kernel replaces `(`/`)`/whitespace in it
/// with `_`, so the first `(` and the last `)` delimit it unambiguously.
fn parse_stat(line: &str) -> Option<(u32, String, char)> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close <= open {
        return None;
    }
    let pid: u32 = line[..open].trim().parse().ok()?;
    let comm = line[open + 1..close].to_string();
    let state = line[close + 1..].split_whitespace().next()?;
    let state = state.chars().next()?;
    Some((pid, comm, state))
}
