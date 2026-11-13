//! Raw `statfs(43)` wrapper — the one call `df` needs that `libpagh` 0.6.1
//! does not wrap yet.
//!
//! pagh-riscv 0.24.0 (PR #11) adds `statfs(43)`/`fstatfs(44)` on the generic
//! RISC-V numbers with a **native 96-byte record**, not Linux's 120-byte one:
//! ten little-endian `u64` fields followed by a 16-byte NUL-padded type name.
//! `libpagh` in `pagh-api` main (`b203ea6`) predates that ABI, so this module
//! issues the `ecall` itself, the same way `libpagh::sys` wraps the calls it
//! does cover, and keeps the contract in one documented place instead of
//! half-guessing it inside the applet.
//!
//! Calling convention: `a7` = 43, `a0` = NUL-terminated **absolute** path
//! pointer, `a1` = record pointer, result in `a0` (`0` or `-errno`). The kernel
//! validates both ranges; a relative path is `-EINVAL`, the synthetic root is
//! `-ENOTSUP`, and a kernel without this ABI answers `-ENOSYS`.
//!
//! `fstatfs(44)` is deliberately not wrapped: nothing in the applet bundle
//! needs to stat the filesystem behind an already-open descriptor, and an
//! unused wrapper would be dead code pretending to be tested.

use alloc::vec::Vec;

use libpagh::errno::Errno;
use libpagh::path;

/// `statfs(path, *Statfs)`.
pub const SYS_STATFS: usize = 43;

/// The native `Statfs` record: 10 little-endian `u64`s and a 16-byte type name.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Statfs {
    /// Filesystem magic (`0xEF53` for ext2, `0` when there is none).
    pub magic: u64,
    /// Bytes per allocation unit.
    pub block_size: u64,
    /// Total blocks.
    pub blocks_total: u64,
    /// Free blocks.
    pub blocks_free: u64,
    /// Blocks available to an unprivileged writer (`== blocks_free` here).
    pub blocks_available: u64,
    /// Total inodes (`0` for a synthetic filesystem).
    pub files_total: u64,
    /// Free inodes.
    pub files_free: u64,
    /// First 8 bytes of the ext2 UUID, LE (`0` when the image has none).
    pub fsid: u64,
    /// VFS name limit (255).
    pub name_len: u64,
    /// No flags are defined; always 0.
    pub flags: u64,
    /// NUL-padded ASCII type name: `ext2`, `procfs`, ...
    pub type_name: [u8; 16],
}

const _: () = assert!(core::mem::size_of::<Statfs>() == 96);

impl Statfs {
    /// The filesystem type name as text (`"ext2"`, `"procfs"`, ...).
    ///
    /// The kernel writes NUL-padded ASCII; a record that is not ASCII is
    /// reported as `"?"` instead of inventing a name.
    pub fn fs_type(&self) -> &str {
        let end = self
            .type_name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(self.type_name.len());
        core::str::from_utf8(&self.type_name[..end]).unwrap_or("?")
    }

    /// Blocks in use (`total - free`, saturating).
    pub fn blocks_used(&self) -> u64 {
        self.blocks_total.saturating_sub(self.blocks_free)
    }

    /// `block_size * blocks`, or 0 when the record is zeroed.
    pub fn bytes_in(&self, blocks: u64) -> u64 {
        blocks.saturating_mul(self.block_size)
    }
}

/// Ask the kernel about the filesystem at `path`.
///
/// `path` may be relative (it is resolved against `PWD` first) but the syscall
/// itself receives the absolute form, which is what the ABI accepts.
pub fn statfs(path: &str) -> Result<Statfs, Errno> {
    let absolute = path::resolve(path);
    if absolute.as_bytes().contains(&0) {
        return Err(Errno::EINVAL);
    }
    let mut c_path: Vec<u8> = Vec::new();
    c_path
        .try_reserve(absolute.len() + 1)
        .map_err(|_| Errno::ENOMEM)?;
    c_path.extend_from_slice(absolute.as_bytes());
    c_path.push(0);

    let mut record = Statfs::default();
    // SAFETY: numbers and pointers follow the ABI contract above; `c_path` is
    // NUL-terminated and `record` is a live 96-byte `#[repr(C)]` struct, and
    // the kernel validates both user ranges before writing/reading (`-EFAULT`
    // otherwise), so no invalid access can fault this process.
    let ret = unsafe {
        syscall2(
            SYS_STATFS,
            c_path.as_ptr() as usize,
            (&mut record as *mut Statfs) as usize,
        )
    };
    check(ret).map(|_| record)
}

/// One two-argument syscall: `a7` = number, result in `a0`.
///
/// The assembly deliberately omits the `nomem`/`nostack` options: the kernel
/// writes through `a1` into the caller's `Statfs`, so the compiler must assume
/// memory is clobbered. This is the same shape as `libpagh::sys::syscall6`.
#[inline]
unsafe fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    let result: isize;
    // SAFETY: the caller guarantees the pointer arguments; `ecall` is the ABI's
    // single door into the kernel and treats every register as untrusted.
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") number,
            inlateout("a0") a0 => result,
            in("a1") a1,
        )
    };
    result
}

/// Turn the kernel's `0`/`-errno` answer into `Result`.
fn check(ret: isize) -> Result<(), Errno> {
    if ret < 0 {
        Err(Errno::from_raw(ret.wrapping_neg() as i32))
    } else {
        Ok(())
    }
}
