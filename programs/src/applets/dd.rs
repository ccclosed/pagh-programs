//! `dd` — copy blocks with an explicit block size, count, skip and seek.
//!
//! ```text
//! dd [if=FILE] [of=FILE] [bs=N] [count=N] [skip=N] [seek=N]
//!    [conv=notrunc] [status=none|noxfer]
//! ```
//!
//! `if` defaults to stdin and `of` to stdout, like busybox `dd`; `bs` is 512
//! bytes, `count` is unlimited, `skip`/`seek` are 0. Numbers are decimal with
//! the usual `dd` suffix: `c`=1, `w`=2, `b`=512, `k`/`K`=1024, `M`=1024²,
//! `G`=1024³. `skip` and `seek` are in blocks, as in `dd`.
//!
//! The output is opened **truncated** unless `conv=notrunc`, which is exactly
//! what this ABI supports: `openat` has `O_TRUNC` and there is no separate
//! "truncate at the final offset" call that would be visible to a reader in
//! between. `seek` moves the output offset before the copy (`lseek`); the
//! ext2 write path zero-fills a hole, so a seek past EOF leaves zeros, not
//! stale bytes. There is no `conv=fsync` because the ABI has no `fsync`.
//!
//! `skip` on a regular input file is one `lseek`; on stdin (which is not
//! seekable) the skipped bytes are read and discarded. `status=none` silences
//! the two summary lines (`N+M records in/out`, written to fd 2); `noxfer` is
//! accepted because the summary has no byte-count line to suppress.
//!
//! Exit: 0 success; 1 read/write/seek error; 2 unrecognised operand or value.
//! Like `dd` everywhere, this is a block copier, not a line tool: `count`
//! counts blocks, and a short final read is the `+1` in the summary.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::File;
use libpagh::sys;

use crate::util;

const USAGE: &str = "usage: dd [if=FILE] [of=FILE] [bs=N] [count=N] [skip=N] \
                     [seek=N] [conv=notrunc] [status=none|noxfer]";

/// `dd`'s default block size.
const DEFAULT_BLOCK: u64 = 512;

/// Parsed command line.
struct Options {
    input: Option<String>,
    output: Option<String>,
    block: usize,
    count: Option<u64>,
    skip: u64,
    seek: u64,
    notrunc: bool,
    quiet: bool,
}

pub fn run(args: &[String]) -> i32 {
    let options = match parse(args) {
        Ok(options) => options,
        Err(code) => return code,
    };

    // Input first: a missing `if=` must fail before an `of=` is truncated.
    // The `File`s are kept alive in the tuple so their descriptors stay open
    // (dropping one closes it) until the applet returns.
    let (in_fd, _input_file): (i32, Option<File>) = match &options.input {
        Some(path) => match File::open(path) {
            Ok(file) => (file.as_raw_fd(), Some(file)),
            Err(err) => {
                util::report("dd", path, err);
                return util::EXIT_FAILURE;
            }
        },
        None => (0, None),
    };
    let input_is_file = _input_file.is_some();

    let (out_fd, _output_file): (i32, Option<File>) = match &options.output {
        Some(path) => {
            let flags = if options.notrunc {
                sys::O_WRONLY | sys::O_CREAT
            } else {
                sys::O_WRONLY | sys::O_CREAT | sys::O_TRUNC
            };
            match File::open_with(path, flags, 0o666) {
                Ok(file) => (file.as_raw_fd(), Some(file)),
                Err(err) => {
                    util::report("dd", path, err);
                    return util::EXIT_FAILURE;
                }
            }
        }
        None => (1, None),
    };

    // `skip` before the first read, so a skipped block is never copied.
    if options.skip != 0 {
        let bytes = match block_bytes(options.skip, options.block) {
            Ok(bytes) => bytes,
            Err(()) => {
                eprintln!("dd: skip is too large");
                return util::EXIT_USAGE;
            }
        };
        if let Err(err) = skip_input(in_fd, input_is_file, bytes) {
            eprintln!("dd: skip: {err}");
            return util::EXIT_FAILURE;
        }
    }

    // `seek` before the first write. On stdout this is the ABI's `EINVAL`: the
    // console is not seekable, which is the honest answer rather than silently
    // ignoring the operand.
    if options.seek != 0 {
        let bytes = match block_bytes(options.seek, options.block) {
            Ok(bytes) => bytes,
            Err(()) => {
                eprintln!("dd: seek is too large");
                return util::EXIT_USAGE;
            }
        };
        if bytes > i64::MAX as u64 {
            eprintln!("dd: seek is too large");
            return util::EXIT_USAGE;
        }
        if let Err(err) = sys::lseek(out_fd, bytes as i64, sys::SEEK_SET) {
            eprintln!("dd: seek: {err}");
            return util::EXIT_FAILURE;
        }
    }

    let mut buffer: Vec<u8> = Vec::new();
    if buffer.try_reserve_exact(options.block).is_err() {
        eprintln!("dd: out of memory");
        return util::EXIT_FAILURE;
    }
    buffer.resize(options.block, 0);

    let mut in_full = 0u64;
    let mut in_partial = 0u64;
    let mut out_full = 0u64;
    let mut out_partial = 0u64;
    let mut blocks = 0u64;

    loop {
        if let Some(count) = options.count {
            if blocks >= count {
                break;
            }
        }
        let read = match util::read_retry(in_fd, &mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) => {
                eprintln!("dd: read: {err}");
                return util::EXIT_FAILURE;
            }
        };
        if let Err(err) = util::write_all(out_fd, &buffer[..read]) {
            eprintln!("dd: write: {err}");
            return util::EXIT_FAILURE;
        }
        blocks += 1;
        if read == options.block {
            in_full += 1;
            out_full += 1;
        } else {
            in_partial += 1;
            out_partial += 1;
        }
    }

    if !options.quiet {
        eprintln!("{in_full}+{in_partial} records in");
        eprintln!("{out_full}+{out_partial} records out");
    }
    util::EXIT_OK
}

fn parse(args: &[String]) -> Result<Options, i32> {
    let mut options = Options {
        input: None,
        output: None,
        block: DEFAULT_BLOCK as usize,
        count: None,
        skip: 0,
        seek: 0,
        notrunc: false,
        quiet: false,
    };

    for arg in args {
        let Some((key, value)) = arg.split_once('=') else {
            eprintln!("dd: unrecognised operand '{arg}'");
            eprintln!("{USAGE}");
            return Err(util::EXIT_USAGE);
        };
        match key {
            "if" => {
                if value.is_empty() {
                    eprintln!("dd: empty if=");
                    return Err(util::EXIT_USAGE);
                }
                options.input = Some(String::from(value));
            }
            "of" => {
                if value.is_empty() {
                    eprintln!("dd: empty of=");
                    return Err(util::EXIT_USAGE);
                }
                options.output = Some(String::from(value));
            }
            "bs" => {
                let Some(size) = parse_size(value) else {
                    eprintln!("dd: invalid block size '{value}'");
                    return Err(util::EXIT_USAGE);
                };
                if size == 0 || size > isize::MAX as u64 {
                    eprintln!("dd: invalid block size '{value}'");
                    return Err(util::EXIT_USAGE);
                }
                options.block = size as usize;
            }
            "count" => match parse_size(value) {
                Some(count) => options.count = Some(count),
                None => {
                    eprintln!("dd: invalid count '{value}'");
                    return Err(util::EXIT_USAGE);
                }
            },
            "skip" => match parse_size(value) {
                Some(skip) => options.skip = skip,
                None => {
                    eprintln!("dd: invalid skip '{value}'");
                    return Err(util::EXIT_USAGE);
                }
            },
            "seek" => match parse_size(value) {
                Some(seek) => options.seek = seek,
                None => {
                    eprintln!("dd: invalid seek '{value}'");
                    return Err(util::EXIT_USAGE);
                }
            },
            "conv" => {
                for flag in value.split(',') {
                    match flag {
                        "notrunc" => options.notrunc = true,
                        // Real `dd` conversions, refused with a reason instead
                        // of accepted and ignored: this ABI has no fsync and
                        // no way to make a failed block "not an error".
                        "fsync" | "fdatasync" | "sync" | "noerror" | "sync,noerror" => {
                            eprintln!("dd: conv={flag}: not supported by this ABI");
                            return Err(util::EXIT_USAGE);
                        }
                        _ => {
                            eprintln!("dd: unrecognised conv '{flag}'");
                            return Err(util::EXIT_USAGE);
                        }
                    }
                }
            }
            "status" => match value {
                "none" => options.quiet = true,
                "noxfer" => {
                    // The summary has no byte-count line; the flag is a no-op
                    // and is accepted so a script that passes it still runs.
                }
                _ => {
                    eprintln!("dd: unrecognised status '{value}'");
                    return Err(util::EXIT_USAGE);
                }
            },
            _ => {
                eprintln!("dd: unrecognised operand '{key}='");
                eprintln!("{USAGE}");
                return Err(util::EXIT_USAGE);
            }
        }
    }
    Ok(options)
}

/// Parse a `dd` size: decimal digits plus the single-character suffix.
fn parse_size(text: &str) -> Option<u64> {
    if text.is_empty() {
        return None;
    }
    let (digits, multiplier) = match text.as_bytes().last() {
        Some(b'c') => (&text[..text.len() - 1], 1u64),
        Some(b'w') => (&text[..text.len() - 1], 2u64),
        Some(b'b') => (&text[..text.len() - 1], 512u64),
        Some(b'k') | Some(b'K') => (&text[..text.len() - 1], 1024u64),
        Some(b'M') => (&text[..text.len() - 1], 1024u64 * 1024),
        Some(b'G') => (&text[..text.len() - 1], 1024u64 * 1024 * 1024),
        _ => (text, 1u64),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value: u64 = digits.parse().ok()?;
    value.checked_mul(multiplier)
}

/// `blocks * block`, or `Err(())` when the product does not fit in a `u64`.
fn block_bytes(blocks: u64, block: usize) -> Result<u64, ()> {
    blocks.checked_mul(block as u64).ok_or(())
}

/// Position the input `bytes` bytes in: `lseek` for a file, read-and-discard
/// for stdin (the console is not seekable).
fn skip_input(fd: i32, is_file: bool, bytes: u64) -> Result<(), libpagh::Errno> {
    if is_file {
        if bytes > i64::MAX as u64 {
            return Err(libpagh::Errno::EINVAL);
        }
        sys::lseek(fd, bytes as i64, sys::SEEK_CUR).map(|_| ())
    } else {
        let mut left = bytes;
        let mut scratch = [0u8; 1024];
        while left > 0 {
            let want = core::cmp::min(left, scratch.len() as u64) as usize;
            let read = util::read_retry(fd, &mut scratch[..want])?;
            if read == 0 {
                break;
            }
            left -= read as u64;
        }
        Ok(())
    }
}
