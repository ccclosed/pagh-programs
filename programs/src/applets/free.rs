//! `free` — report memory usage from `/proc/meminfo`.
//!
//! The kernel's procfs (`pagh-riscv` 0.22.0, PR #9) renders `/proc/meminfo`
//! from the PMM, the heap and the Sv39 counters. This applet only formats it:
//!
//! ```text
//!               total        used        free      shared  buff/cache   available
//! Mem:         262144       12400      249744           0           0      249744
//! Swap:             0           0           0
//! ```
//!
//! `used = total - free`, and `available = MemAvailable`, which the kernel sets
//! equal to `MemFree` because there is no page cache and nothing to reclaim
//! (documented in `src/vfs/proc.rs`). The `shared` and `buff/cache` columns are
//! **0 because the kernel tracks no such counters** — `meminfo` has no such
//! rows, and inventing a number would be worse than printing the honest zero.
//! `Swap:` is the same story: there is no swap subsystem, so its three values
//! are zero.
//!
//! Options: `-b` bytes, `-k` kB (default), `-m` MiB, `-g` GiB, `-h` human
//! (integer decimal, no floating point — the port's text tools do not depend on
//! the FPU). There is no `-t` total row, no `-w`, no `-s` sampling.
//!
//! Exit: 0 success; 1 `/proc/meminfo` unreadable or missing `Mem*` rows;
//! 2 usage.

use alloc::format;
use alloc::string::String;

use libpagh::eprintln;
use libpagh::fs;

use crate::util;

const USAGE: &str = "usage: free [-b|-k|-m|-g|-h]";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Bytes,
    Kilo,
    Mega,
    Giga,
    Human,
}

pub fn run(args: &[String]) -> i32 {
    let mut unit = Unit::Kilo;
    for arg in args {
        match arg.as_str() {
            "-b" => unit = Unit::Bytes,
            "-k" => unit = Unit::Kilo,
            "-m" => unit = Unit::Mega,
            "-g" => unit = Unit::Giga,
            "-h" => unit = Unit::Human,
            _ => {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            }
        }
    }

    let text = match fs::read_to_string("/proc/meminfo") {
        Ok(text) => text,
        Err(err) => {
            util::report("free", "/proc/meminfo", err);
            return util::EXIT_FAILURE;
        }
    };

    let mut total_kb: Option<u64> = None;
    let mut free_kb: Option<u64> = None;
    let mut available_kb: Option<u64> = None;
    for line in util::lines(text.as_bytes()) {
        let Ok(line) = core::str::from_utf8(line) else {
            continue;
        };
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let mut fields = rest.split_whitespace();
        let Some(value) = fields.next().and_then(|value| value.parse::<u64>().ok()) else {
            continue;
        };
        let value_kb = match (fields.next(), key.trim()) {
            // The kernel writes `kB` for the Mem rows; a byte unit is not
            // produced today but is accepted so the parser is not unit-blind.
            (Some("B"), _) => value / 1024,
            _ => value,
        };
        match key.trim() {
            "MemTotal" => total_kb = Some(value_kb),
            "MemFree" => free_kb = Some(value_kb),
            "MemAvailable" => available_kb = Some(value_kb),
            _ => {}
        }
    }

    let (Some(total_kb), Some(free_kb)) = (total_kb, free_kb) else {
        eprintln!("free: /proc/meminfo: MemTotal/MemFree missing");
        return util::EXIT_FAILURE;
    };
    let available_kb = available_kb.unwrap_or(free_kb);
    let used_kb = total_kb.saturating_sub(free_kb);

    let mut out = String::new();
    out.push_str(&format!(
        "{:<6}{:>12}{:>12}{:>12}{:>12}{:>12}{:>12}\n",
        "", "total", "used", "free", "shared", "buff/cache", "available"
    ));
    out.push_str(&format!(
        "{:<6}{:>12}{:>12}{:>12}{:>12}{:>12}{:>12}\n",
        "Mem:",
        scale(total_kb, unit),
        scale(used_kb, unit),
        scale(free_kb, unit),
        0,
        0,
        scale(available_kb, unit),
    ));
    out.push_str(&format!("{:<6}{:>12}{:>12}{:>12}\n", "Swap:", 0, 0, 0));

    match util::write_all(1, out.as_bytes()) {
        Ok(()) => util::EXIT_OK,
        Err(err) => {
            eprintln!("free: write error: {err}");
            util::EXIT_FAILURE
        }
    }
}

/// Render one kB value in the requested unit.
///
/// Scaling is integer arithmetic deliberately: the port's text tools never
/// depend on the FPU, so `-h` prints tenths computed as `value*10/divisor`.
fn scale(kb: u64, unit: Unit) -> String {
    match unit {
        Unit::Kilo => alloc::format!("{kb}"),
        Unit::Bytes => alloc::format!("{}", kb.saturating_mul(1024)),
        Unit::Mega => alloc::format!("{}", kb / 1024),
        Unit::Giga => alloc::format!("{}", kb / (1024 * 1024)),
        Unit::Human => human(kb.saturating_mul(1024)),
    }
}

/// Human-readable byte count with one decimal: `256.0M`, `3.0K`, `12B`.
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

/// `{whole}.{tenth}{suffix}` computed without floating point.
fn tenths(value: u64, divisor: u64, suffix: &str) -> String {
    let scaled = value.saturating_mul(10) / divisor;
    alloc::format!("{}.{}{suffix}", scaled / 10, scaled % 10)
}
