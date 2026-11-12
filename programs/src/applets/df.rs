//! `df` — report filesystem capacity through the native `statfs(43)` ABI.
//!
//! ```text
//! df [-h] [PATH...]
//! ```
//!
//! `pagh-riscv` 0.24.0 (PR #11) added `statfs(43)` with a native 96-byte
//! `Statfs`; this applet is its first consumer. `libpagh` 0.6.1 does not wrap
//! the call yet, so [`crate::applets::statfs_sys`] issues the `ecall` and
//! documents the record; see that module for the contract.
//!
//! With no operands `df` reads `/proc/mounts` (`pagh-riscv` 0.22.0, PR #9) and
//! reports every mount point, including `procfs`, whose capacity is honestly
//! zero. The kernel's `/proc/mounts` device field is the mount point (there are
//! no device nodes in this ABI), so the `Filesystem` column shows the
//! filesystem **type** from `Statfs.type_name` (`ext2`, `procfs`) and the last
//! column carries the path: a device name would have to be invented. With
//! operands each path is resolved and `statfs`-ed directly, so `df` still works
//! on a kernel without `/proc/mounts` as long as the caller names the path.
//!
//! `1K-blocks` is `blocks_total * block_size / 1024`; `Used` and `Available`
//! use `blocks_free`/`blocks_available`. `Use%` is integer arithmetic rounded
//! up (a filesystem that is not empty is never reported as 0% — the same policy
//! `df` follows); a zero-capacity filesystem shows `-`.
//!
//! Options: `-h` human-readable sizes, `-k` 1K blocks (default; accepted so a
//! script written for busybox `df -k` runs). No `-i`, `-T`, `-a`, `-P`.
//!
//! Errors: a missing path is `ENOENT`, the synthetic root is `ENOTSUP`; both
//! are reported per operand and the exit status is 1. A kernel without the new
//! syscall answers `ENOSYS` and says so instead of printing a fabricated table.
//!
//! Exit: 0 success; 1 statfs/read error; 2 usage.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs;
use libpagh::path;

use super::statfs_sys::{self, Statfs};
use crate::util;

const USAGE: &str = "usage: df [-h] [PATH...]";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SizeFormat {
    Blocks1K,
    Human,
}

pub fn run(args: &[String]) -> i32 {
    let mut size_format = SizeFormat::Blocks1K;
    let mut operands: Vec<&str> = Vec::new();
    let mut parse_options = true;

    for arg in args {
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        match arg.as_str() {
            "-h" if parse_options => size_format = SizeFormat::Human,
            "-k" if parse_options => size_format = SizeFormat::Blocks1K,
            _ if parse_options && arg.len() > 1 && arg.starts_with('-') => {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            }
            _ => {
                parse_options = false;
                if util::push(&mut operands, arg.as_str()).is_err() {
                    eprintln!("df: out of memory");
                    return util::EXIT_FAILURE;
                }
            }
        }
    }

    // One row per path. `(mount_point, record, error)` so transport errors and
    // honest rows can be reported in the order the kernel listed the mounts.
    let mut rows: Vec<(String, Result<Statfs, libpagh::Errno>)> = Vec::new();
    let mut status = util::EXIT_OK;

    if operands.is_empty() {
        match mounts() {
            Ok(mounts) => {
                for mount_point in mounts {
                    let record = statfs_sys::statfs(&mount_point);
                    if record.is_err() {
                        status = util::EXIT_FAILURE;
                    }
                    if util::push(&mut rows, (mount_point, record)).is_err() {
                        eprintln!("df: out of memory");
                        return util::EXIT_FAILURE;
                    }
                }
            }
            Err(err) => {
                util::report("df", "/proc/mounts", err);
                eprintln!("df: name a path (df PATH) on a kernel without /proc");
                return util::EXIT_FAILURE;
            }
        }
    } else {
        for operand in operands {
            let resolved = path::resolve(operand);
            let record = statfs_sys::statfs(&resolved);
            if record.is_err() {
                status = util::EXIT_FAILURE;
            }
            if util::push(&mut rows, (resolved, record)).is_err() {
                eprintln!("df: out of memory");
                return util::EXIT_FAILURE;
            }
        }
    }

    let mut out = String::new();
    match size_format {
        SizeFormat::Blocks1K => out.push_str(&format!(
            "{:<12} {:>10} {:>9} {:>9} {:>4} {}\n",
            "Filesystem", "1K-blocks", "Used", "Available", "Use%", "Mounted on"
        )),
        SizeFormat::Human => out.push_str(&format!(
            "{:<12} {:>10} {:>9} {:>9} {:>4} {}\n",
            "Filesystem", "Size", "Used", "Available", "Use%", "Mounted on"
        )),
    }

    for (mount_point, record) in &rows {
        let record = match record {
            Ok(record) => record,
            Err(err) => {
                util::report("df", mount_point, *err);
                continue;
            }
        };
        let total = record.bytes_in(record.blocks_total);
        let used = record.bytes_in(record.blocks_used());
        let available = record.bytes_in(record.blocks_available);
        // `blocks` are the kernel's units; the percentage must not depend on
        // the display unit, so it is computed from blocks.
        let percent = use_percent(record);
        let (total_text, used_text, available_text) = match size_format {
            SizeFormat::Blocks1K => (
                alloc::format!("{}", total / 1024),
                alloc::format!("{}", used / 1024),
                alloc::format!("{}", available / 1024),
            ),
            SizeFormat::Human => (human(total), human(used), human(available)),
        };
        out.push_str(&format!(
            "{:<12} {:>10} {:>9} {:>9} {:>4} {}\n",
            record.fs_type(),
            total_text,
            used_text,
            available_text,
            percent,
            mount_point
        ));
    }

    match util::write_all(1, out.as_bytes()) {
        Ok(()) => status,
        Err(err) => {
            eprintln!("df: write error: {err}");
            util::EXIT_FAILURE
        }
    }
}

/// The mount points from `/proc/mounts`, in kernel order.
fn mounts() -> Result<Vec<String>, libpagh::Errno> {
    let text = fs::read_to_string("/proc/mounts")?;
    let mut mounts: Vec<String> = Vec::new();
    for line in util::lines(text.as_bytes()) {
        let Ok(line) = core::str::from_utf8(line) else {
            continue;
        };
        let mut fields = line.split_whitespace();
        let Some(_device) = fields.next() else {
            continue;
        };
        let Some(mount_point) = fields.next() else {
            continue;
        };
        if !mount_point.starts_with('/') {
            continue;
        }
        if util::push(&mut mounts, String::from(mount_point)).is_err() {
            return Err(libpagh::Errno::ENOMEM);
        }
    }
    Ok(mounts)
}

/// `Use%` = ceil(used * 100 / total), or `-` for a zero-capacity filesystem.
fn use_percent(record: &Statfs) -> String {
    if record.blocks_total == 0 {
        return String::from("-");
    }
    let used = record.blocks_used();
    let scaled = used.saturating_mul(100);
    let percent =
        scaled / record.blocks_total + u64::from(!scaled.is_multiple_of(record.blocks_total));
    alloc::format!("{percent}%")
}

/// Human-readable byte count with one decimal, integer arithmetic only.
fn human(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        tenths(bytes, 1024 * 1024 * 1024, "G")
    } else if bytes >= 1024 * 1024 {
        tenths(bytes, 1024 * 1024, "M")
    } else if bytes >= 1024 {
        tenths(bytes, 1024, "K")
    } else {
        alloc::format!("{bytes}B")
    }
}

/// `{whole}.{tenth}{suffix}` without floating point.
fn tenths(value: u64, divisor: u64, suffix: &str) -> String {
    let scaled = value.saturating_mul(10) / divisor;
    alloc::format!("{}.{}{suffix}", scaled / 10, scaled % 10)
}
