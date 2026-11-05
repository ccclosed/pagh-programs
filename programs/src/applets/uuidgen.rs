//! `uuidgen` — print RFC 4122 *version-4-shaped* UUIDs.
//!
//! ```text
//! uuidgen [-r] [-n N]
//! ```
//!
//! The default (and only) mode is `-r`: 16 random bytes rendered as
//! `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`, with the version nibble forced to 4
//! and the variant bits to `10`. `-n N` prints N UUIDs, one per line; `-n 0`
//! prints nothing and exits 0.
//!
//! ## There is no entropy source
//!
//! The ABI has no `getrandom`, no `/dev/urandom` and no hardware RNG. This
//! applet seeds a xorshift64* generator (the same constants as `shuf`) from
//! `clock_gettime(CLOCK_MONOTONIC)` seconds and nanoseconds and from `getpid`,
//! then derives every UUID from that stream. The seed is deterministic for a
//! given clock reading and pid, so:
//!
//! * the output is **not cryptographically random**;
//! * two processes seeded in the same clock tick can produce the same UUID;
//! * uniqueness and unpredictability are **not guaranteed**.
//!
//! It is a shape-compatible identifier generator for a single-user hobby OS,
//! not a replacement for a kernel CSPRNG. `-r` is accepted (and is the only
//! mode) for command-line compatibility.
//!
//! Exit: 0 success; 2 usage or an invalid `-n`; 1 on a write error.

use alloc::string::String;

use libpagh::{eprintln, sys, Timespec};

use crate::util;

const USAGE: &str = "usage: uuidgen [-r] [-n N]";
const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

pub fn run(args: &[String]) -> i32 {
    let mut count = 1usize;
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && (arg == "-r" || arg == "--random") {
            continue;
        }
        if parse_options && (arg == "-n" || arg == "--count") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match util::parse_usize(value) {
                Some(parsed) => count = parsed,
                None => {
                    eprintln!("uuidgen: invalid count: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-n") {
            match util::parse_usize(&arg[2..]) {
                Some(parsed) => count = parsed,
                None => {
                    eprintln!("uuidgen: invalid count: '{}'", &arg[2..]);
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let mut state = seed();
    for _ in 0..count {
        let mut bytes = [0u8; 16];
        let high = next_random(&mut state).to_be_bytes();
        let low = next_random(&mut state).to_be_bytes();
        bytes[..8].copy_from_slice(&high);
        bytes[8..].copy_from_slice(&low);
        bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
        bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant

        let text = format_uuid(&bytes);
        if let Err(err) = util::write_line(1, text.as_bytes()) {
            eprintln!("uuidgen: write error: {err}");
            return util::EXIT_FAILURE;
        }
    }
    util::EXIT_OK
}

/// Build the generator state from the monotonic clock and the pid.
///
/// A missing clock or pid is tolerated (the remaining source still seeds the
/// stream); a zero state is replaced because xorshift64* would stay zero.
fn seed() -> u64 {
    let mut state = SEED;
    let mut now = Timespec::default();
    if sys::clock_gettime(sys::CLOCK_MONOTONIC, &mut now).is_ok() {
        state ^= (now.seconds as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        state ^= (now.nanos as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    }
    if let Ok(pid) = sys::getpid() {
        state ^= (pid as u64).wrapping_mul(0xD1B5_4A32_D192_ED03);
    }
    if state == 0 {
        state = SEED;
    }
    state
}

/// xorshift64*: shifts 12/25/27 and the `0x2545F4914F6CDD1D` multiplier.
fn next_random(state: &mut u64) -> u64 {
    let mut value = *state;
    value ^= value >> 12;
    value ^= value << 25;
    value ^= value >> 27;
    *state = value;
    value.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

fn format_uuid(bytes: &[u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push(HEX[((*byte >> 4) & 0x0f) as usize] as char);
        out.push(HEX[(*byte & 0x0f) as usize] as char);
    }
    out
}