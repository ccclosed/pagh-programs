//! Shared plumbing for the applets: console reads that honour `EAGAIN`, whole
//! output writes, input opening (`-` means stdin) and small option helpers.
//!
//! Everything returns [`Errno`]; applets decide the diagnostics and the exit
//! code. Nothing here panics on bad input.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::env;
use libpagh::errno::{Errno, Result};
use libpagh::fs;
use libpagh::fs::File;
use libpagh::sys;
use libpagh::EntryKind;
use libpagh::Timespec;

/// Exit code for a normal result.
pub const EXIT_OK: i32 = 0;
/// Exit code for a data/runtime failure (a file could not be read, a number
/// was invalid, writing failed).
pub const EXIT_FAILURE: i32 = 1;
/// Exit code for a command-line syntax error.
pub const EXIT_USAGE: i32 = 2;
/// Exit code for an unknown applet name (busybox convention).
pub const EXIT_NOAPPLET: i32 = 127;

/// Read from `fd`, retrying `EAGAIN` by yielding the CPU.
///
/// `0` means EOF. This is the ABI's console-input contract: fd 0 returns
/// `-EAGAIN` until a byte is queued, and there is no `poll` to wait on.
pub fn read_retry(fd: i32, buf: &mut [u8]) -> Result<usize> {
    loop {
        match sys::read(fd, buf) {
            Ok(n) if n <= buf.len() => return Ok(n),
            // A kernel that claims more than the buffer is broken; never slice
            // past it (that would panic, and this process has no unwinding).
            Ok(_) => return Err(Errno::EIO),
            Err(Errno::EAGAIN) => {
                // The only voluntary switch available; ignore a yield failure
                // and retry the read (a broken yield must not hang output).
                let _ = sys::sched_yield();
            }
            other => return other,
        }
    }
}

/// Write all of `buf` to `fd`, retrying `EAGAIN` with `sched_yield`.
pub fn write_all(fd: i32, mut buf: &[u8]) -> Result<()> {
    while !buf.is_empty() {
        match sys::write(fd, buf) {
            Ok(0) => return Err(Errno::EIO),
            Ok(n) if n <= buf.len() => buf = &buf[n..],
            Ok(_) => return Err(Errno::EIO),
            Err(Errno::EAGAIN) => {
                let _ = sys::sched_yield();
            }
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// Read `fd` to EOF into a `Vec`. Used by `tail` and the whole-file readers.
///
/// `try_reserve` turns memory exhaustion into `ENOMEM` instead of an abort.
pub fn read_all(fd: i32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = read_retry(fd, &mut buf)?;
        if n == 0 {
            return Ok(out);
        }
        if n > buf.len() {
            return Err(Errno::EIO);
        }
        out.try_reserve(n).map_err(|_| Errno::ENOMEM)?;
        out.extend_from_slice(&buf[..n]);
    }
}

/// An input operand: a file, or stdin for the conventional `-`.
///
/// The `File` is owned (and closed on drop); stdin is not closed.
pub struct Input {
    file: Option<File>,
}

impl Input {
    /// Open an operand; `"-"` selects fd 0.
    pub fn open(name: &str) -> Result<Input> {
        if name == "-" {
            Ok(Input { file: None })
        } else {
            Ok(Input {
                file: Some(File::open(name)?),
            })
        }
    }

    /// The descriptor to read from.
    pub fn fd(&self) -> i32 {
        match &self.file {
            Some(file) => file.as_raw_fd(),
            None => 0,
        }
    }
}

/// The name shown in headers for an operand.
pub fn display_name(name: &str) -> &str {
    if name == "-" {
        "standard input"
    } else {
        name
    }
}

/// Read a whole operand into memory (`-` is stdin).
///
/// The line-oriented applets use this instead of a hand-written reader: the
/// input has to be sorted/grouped/cut anyway, and [`read_all`] already retries
/// `EAGAIN` and uses `try_reserve`.
pub fn read_input(name: &str) -> Result<Vec<u8>> {
    let input = Input::open(name)?;
    read_all(input.fd())
}

/// Write `line` and a terminating newline to `fd`.
///
/// Output errors are returned rather than ignored, so an applet can print the
/// conventional `program: write error: ...` diagnostic and exit 1.
pub fn write_line(fd: i32, line: &[u8]) -> Result<()> {
    write_all(fd, line)?;
    write_all(fd, b"\n")
}

/// Split a byte buffer into lines, without the trailing `\n`.
///
/// The rules are the usual text-tool ones:
///
/// * `"a\nb\n"` yields `"a"`, `"b"`;
/// * `"a\nb"` yields `"a"`, `"b"` (the last line needs no newline);
/// * `"\n"` yields one empty line; `""` yields no lines.
///
/// Splitting is on ASCII LF only; a CR, if present, stays in the line. The
/// iterator borrows the buffer, so a whole file costs one allocation plus the
/// line offsets the caller stores.
pub fn lines(data: &[u8]) -> Lines<'_> {
    Lines {
        data,
        start: 0,
        done: false,
    }
}

/// Iterator returned by [`lines`].
pub struct Lines<'a> {
    data: &'a [u8],
    start: usize,
    done: bool,
}

impl<'a> Iterator for Lines<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        if self.done || self.start >= self.data.len() {
            self.done = true;
            return None;
        }
        let rest = &self.data[self.start..];
        match rest.iter().position(|byte| *byte == b'\n') {
            Some(offset) => {
                self.start += offset + 1;
                Some(&rest[..offset])
            }
            None => {
                self.done = true;
                Some(rest)
            }
        }
    }
}

/// A human-readable name for an ABI [`EntryKind`], as used by `stat`.
pub fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "file",
        EntryKind::Dir => "directory",
        EntryKind::Symlink => "symlink",
        EntryKind::Other => "other",
    }
}

/// Print the conventional `program: operand: error` line to fd 2.
pub fn report(program: &str, operand: &str, err: Errno) {
    libpagh::eprintln!("{program}: {operand}: {err}");
}

/// Parse a signed decimal integer without panicking.
pub fn parse_i64(text: &str) -> Option<i64> {
    text.parse::<i64>().ok()
}

/// Parse an unsigned decimal integer without panicking.
pub fn parse_usize(text: &str) -> Option<usize> {
    text.parse::<usize>().ok()
}

/// Parse `SECONDS` or `SECONDS.FRACTION` (up to nine fractional digits).
///
/// This is the `sleep` grammar, shared by `sleep`, `timeout` and `watch`:
/// integer seconds or a decimal fraction, no suffixes and no sign. Zero is
/// valid. The caller decides whether a missing or malformed value is a usage
/// error; this function never panics.
pub fn parse_duration(text: &str) -> Option<Timespec> {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let has_whole = !whole.is_empty();
    let has_fraction = fraction.map_or(false, |part| !part.is_empty());
    if !has_whole && !has_fraction {
        return None;
    }

    let seconds: i64 = if whole.is_empty() {
        0
    } else {
        if !whole.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        whole.parse::<i64>().ok()?
    };

    let mut nanos: i64 = 0;
    if let Some(fraction) = fraction {
        if fraction.len() > 9 {
            return None;
        }
        let mut scale: i64 = 100_000_000;
        for byte in fraction.bytes() {
            if !byte.is_ascii_digit() {
                return None;
            }
            nanos += i64::from(byte - b'0') * scale;
            scale /= 10;
        }
    }

    Some(Timespec { seconds, nanos })
}

/// Parse a signal name (`TERM`, `SIGTERM`, any ASCII case) or a number
/// `0..=64`.
///
/// The same table is used by `kill` and `timeout`, so a signal that one
/// accepts is not rejected by the other.
pub fn parse_signal(text: &str) -> Option<i32> {
    if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) {
        let number = text.parse::<i32>().ok()?;
        if (0..=64).contains(&number) {
            return Some(number);
        }
        return None;
    }
    let upper = text.to_ascii_uppercase();
    let name = upper.strip_prefix("SIG").unwrap_or(&upper);
    match name {
        "HUP" => Some(sys::SIGHUP),
        "INT" => Some(sys::SIGINT),
        "QUIT" => Some(sys::SIGQUIT),
        "KILL" => Some(sys::SIGKILL),
        "USR1" => Some(sys::SIGUSR1),
        "USR2" => Some(sys::SIGUSR2),
        "ALRM" => Some(sys::SIGALRM),
        "TERM" => Some(sys::SIGTERM),
        "CHLD" => Some(sys::SIGCHLD),
        "CONT" => Some(sys::SIGCONT),
        "STOP" => Some(sys::SIGSTOP),
        _ => None,
    }
}

/// True for a path that `fs::stat` says is a regular file.
fn is_file(path: &str) -> bool {
    fs::stat(path).map(|stat| stat.is_file()).unwrap_or(false)
}

/// Resolve a command name the way this port's shell does.
///
/// The kernel does not set `PATH`, so the search is deliberately fixed:
///
/// 1. `/mnt/bin/<name>` (where `pkg install` puts the symlinks);
/// 2. `<PWD>/<name>`;
/// 3. a literal absolute path, or a relative path resolved against `PWD`.
///
/// Returns the absolute path as a `String`, or `ENOENT`. A directory is not a
/// command and the executable bit is not inspected (`test -x` does that).
pub fn which(program: &str) -> Result<String> {
    if program.is_empty() {
        return Err(Errno::ENOENT);
    }
    if program.contains('/') {
        let candidate = if program.starts_with('/') {
            String::from(program)
        } else {
            join_path(&env::current_dir(), program)
        };
        return if is_file(&candidate) {
            Ok(candidate)
        } else {
            Err(Errno::ENOENT)
        };
    }

    let installed = format!("/mnt/bin/{program}");
    if is_file(&installed) {
        return Ok(installed);
    }
    let local = join_path(&env::current_dir(), program);
    if is_file(&local) {
        return Ok(local);
    }
    Err(Errno::ENOENT)
}

/// Decode the raw `wait4` status into an exit code.
///
/// The kernel stores `code << 8` for a normal exit and the signal number in
/// the low seven bits for a signal/fault death; the shell convention
/// `128 + signal` is returned for the latter. There is no core-dump bit.
pub fn wait_status_code(status: i32) -> i32 {
    let signal = status & 0x7f;
    if signal != 0 {
        128 + signal
    } else {
        (status >> 8) & 0xff
    }
}

/// Fork a child and `execve` `program` with `args`.
///
/// The child's `argv[0]` is the resolved absolute path, as busybox does; the
/// child inherits stdout/stderr and a single-entry environment (`PWD`), so a
/// relative-path applet still resolves against the caller's directory. On an
/// `execve` failure the child prints the diagnostic on fd 2 and exits 127;
/// the parent gets the pid. The caller owns the `wait4`.
pub fn spawn(program: &str, args: &[&str]) -> Result<i32> {
    let path = which(program)?;
    spawn_path(&path, args)
}

/// [`spawn`] with an already resolved absolute `path` (no search).
pub fn spawn_path(path: &str, args: &[&str]) -> Result<i32> {
    let mut argv: Vec<&str> = Vec::new();
    argv.try_reserve(args.len() + 1)
        .map_err(|_| Errno::ENOMEM)?;
    argv.push(path);
    argv.extend_from_slice(args);

    let pwd = env::current_dir();
    let pwd_entry = format!("PWD={pwd}");
    let envp = [pwd_entry.as_str()];

    let pid = sys::fork()?;
    if pid == 0 {
        // A failed execve must not fall back into the parent's code: report it
        // and make the child's exit status the conventional 127.
        if let Err(err) = sys::execve(path, &argv, &envp) {
            libpagh::eprintln!("{path}: {err}");
        }
        sys::exit(127);
    }
    Ok(pid)
}

/// Fork, `execve` and block in `wait4`; returns the decoded child status.
///
/// A missing command is `Err(ENOENT)` from [`which`]; an `execve` failure in
/// the child is reported by the child and surfaces as status 127.
pub fn spawn_wait(program: &str, args: &[&str]) -> Result<i32> {
    let pid = spawn(program, args)?;
    let (_child, status) = sys::wait4(pid, 0)?;
    Ok(wait_status_code(status))
}

/// Format a number into a decimal digit count (for `wc`'s column width).
pub fn digits(mut value: u64) -> usize {
    let mut width = 1;
    while value >= 10 {
        value /= 10;
        width += 1;
    }
    width
}

/// Push onto a `Vec`, turning allocator failure into `ENOMEM`.
///
/// The ABI's allocator aborts on an allocation failure, which would turn a
/// recoverable "out of memory" into exit 102. The applets that grow a
/// collection call this instead of `Vec::push`; the caller reports the error
/// with [`report`] and exits 1.
pub fn push<T>(vec: &mut Vec<T>, value: T) -> Result<()> {
    vec.try_reserve(1).map_err(|_| Errno::ENOMEM)?;
    vec.push(value);
    Ok(())
}

/// `parent` + `/` + `name`, without doubling a trailing slash.
///
/// The VFS accepts absolute paths only, so `parent` is normally already
/// absolute (the applets pass the operand's spelling or a resolved path).
pub fn join_path(parent: &str, name: &str) -> String {
    let mut out = String::with_capacity(parent.len() + name.len() + 1);
    out.push_str(parent);
    if !out.ends_with('/') {
        out.push('/');
    }
    out.push_str(name);
    out
}

/// Create `path` and any missing parents, skipping components that exist.
///
/// `libpagh::fs::create_dir_all` starts at `/` and calls `mkdir` on each
/// component; on 0.16.1 the mount point `/mnt` is a synthetic node whose
/// `create_dir` answers `ENOTSUP`, so that helper fails for every path below a
/// mount even though `/mnt` already exists. This version stats each component
/// first and only calls `mkdir` for a missing one, which is the same
/// stat-before-mkdir workaround `tar` uses. An existing non-directory is
/// `EEXIST`, not silently skipped.
pub fn create_dir_all(path: &str) -> Result<()> {
    let absolute = libpagh::path::resolve(path);
    if absolute == "/" {
        return Ok(());
    }
    let mut prefix = String::new();
    for component in absolute.split('/').filter(|component| !component.is_empty()) {
        prefix.push('/');
        prefix.push_str(component);
        match fs::stat(&prefix) {
            Ok(stat) => {
                if !stat.is_dir() {
                    return Err(Errno::EEXIST);
                }
                continue;
            }
            Err(Errno::ENOENT) => {}
            Err(err) => return Err(err),
        }
        match fs::create_dir(&prefix) {
            Ok(()) => {}
            Err(Errno::EEXIST) => {
                if !fs::stat(&prefix)?.is_dir() {
                    return Err(Errno::EEXIST);
                }
            }
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// The kind of `path` **without following a final symbolic link**.
///
/// The ABI has no `lstat`: `fs::stat` is `openat` + `fstat` and therefore
/// follows the final component. `readlinkat` is the one call that looks at the
/// link itself, so a successful `fs::read_link` is how an applet detects a
/// symlink; anything else is classified by `fs::stat` (which follows it).
///
/// A non-link answers `EINVAL`; that is the expected branch, not an error.
pub fn path_kind(path: &str) -> Result<EntryKind> {
    match libpagh::fs::read_link(path) {
        Ok(_) => Ok(EntryKind::Symlink),
        Err(Errno::EINVAL) => Ok(libpagh::fs::stat(path)?.entry_kind()),
        Err(err) => Err(err),
    }
}