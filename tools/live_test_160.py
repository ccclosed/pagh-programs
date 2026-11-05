#!/usr/bin/env python3
"""Live QEMU check for the paghbox 1.6.0 batch (107 applets, 13 new).

Builds a 64 MiB ext2 image with the release `paghbox`, `/mnt/bin` symlinks for
the commands that the process applets spawn, and small fixtures (including a
synthetic ELF with `.symtab` and `.dynsym`). Each case boots the kernel's own
`tools/run.py` with 2-3 commands and compares stdout/stderr plus the shell's
`run: ... exited with code N` line.

The ELF fixture is generated in `build_symbols_elf()`; its symbol table is
deliberately tiny so `nm` output is stable and readable on the serial line.

Usage:

    python3 tools/live_test_160.py
    python3 tools/live_test_160.py --kernel PATH --runner PATH --transcript PATH
"""

from __future__ import annotations

import argparse
import pathlib
import re
import shutil
import struct
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
PAGHBOX = ROOT / "target" / "riscv64gc-unknown-none-elf" / "release" / "paghbox"
DEFAULT_KERNEL = pathlib.Path(
    "/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/debug/pagh"
)
DEFAULT_RUNNER = pathlib.Path("/home/oleg/pagh-riscv/tools/run.py")
DEFAULT_TRANSCRIPT = ROOT / "docs" / "live-1.6.0-transcript.txt"
PROMPT = "pagh:rv64>"
RUN = "run /mnt/paghbox"


# ─── Fixtures ─────────────────────────────────────────────────────────────


def build_symbols_elf() -> bytes:
    """A valid ELF64 LE RISC-V image with .symtab and .dynsym.

    Symbols: a local object `counter` in .data, a global function `main` in
    .text, an undefined global `puts`, and one dynamic function `dynfunc`.
    Both string tables start with the empty-name NUL.
    """
    sht_null, sht_progbits, sht_strtab, sht_symtab, sht_dynsym = 0, 1, 3, 2, 11
    shf_write, shf_alloc, shf_exec = 1, 2, 4
    shstr = b"\0.text\0.data\0.dynsym\0.dynstr\0.symtab\0.strtab\0.shstrtab\0"
    shoff = 64
    shentsize = 64
    shnum = 8
    dynsym_off = shoff + shentsize * shnum
    dynstr = b"\0dynfunc\0"
    dynstr_off = dynsym_off + 48
    symtab_off = dynstr_off + len(dynstr)
    strtab = b"\0counter\0main\0puts\0"
    strtab_off = symtab_off + 96
    shstr_off = strtab_off + len(strtab)
    total = (shstr_off + len(shstr) + 7) // 8 * 8
    out = bytearray(total)
    out[0:4] = b"\x7fELF"
    out[4] = 2  # ELFCLASS64
    out[5] = 1  # ELFDATA2LSB
    out[6] = 1  # EV_CURRENT
    struct.pack_into(
        "<HHIQQQIHHHHHH", out, 16,
        2, 243, 1, 0, 0, shoff, 0, 64, 0, 0, shentsize, shnum, 7,
    )

    def section(index, name, kind, flags, offset, size, link, info, align, entsize):
        struct.pack_into(
            "<IIQQQQIIQQ", out, shoff + index * shentsize,
            name, kind, flags, 0, offset, size, link, info, align, entsize,
        )

    section(0, 0, sht_null, 0, 0, 0, 0, 0, 0, 0)
    section(1, 1, sht_progbits, shf_alloc | shf_exec, 0, 0, 0, 0, 1, 0)
    section(2, 7, sht_progbits, shf_alloc | shf_write, 0, 0, 0, 0, 1, 0)
    section(3, 13, sht_dynsym, 0, dynsym_off, 48, 4, 1, 8, 24)
    section(4, 21, sht_strtab, 0, dynstr_off, len(dynstr), 0, 0, 1, 0)
    section(5, 29, sht_symtab, 0, symtab_off, 96, 6, 2, 8, 24)
    section(6, 37, sht_strtab, 0, strtab_off, len(strtab), 0, 0, 1, 0)
    section(7, 45, sht_strtab, 0, shstr_off, len(shstr), 0, 0, 1, 0)

    def symbol(base, index, name, info, shndx, value, size):
        struct.pack_into("<IBBHQQ", out, base + index * 24, name, info, 0, shndx, value, size)

    symbol(dynsym_off, 1, 1, 0x12, 1, 0x2000, 4)
    symbol(symtab_off, 1, 1, 0x01, 2, 0x40, 4)
    symbol(symtab_off, 2, 9, 0x12, 1, 0x1234, 8)
    symbol(symtab_off, 3, 14, 0x12, 0, 0, 0)
    out[dynstr_off:dynstr_off + len(dynstr)] = dynstr
    out[strtab_off:strtab_off + len(strtab)] = strtab
    out[shstr_off:shstr_off + len(shstr)] = shstr
    return bytes(out)


def shred_pattern(size: int, pass_number: int, file_index: int = 0) -> bytes:
    """Mirror `shred.rs`: xorshift64 bytes mapped to printable ASCII."""
    mask = (1 << 64) - 1
    state = (
        0x9E3779B97F4A7C15
        ^ ((pass_number * 0x00000100000001B3) & mask)
        ^ ((file_index * 0xD1B54A32D192ED03) & mask)
    ) & mask
    out = bytearray()
    for _ in range(size):
        value = state
        value ^= (value << 13) & mask
        value ^= value >> 7
        value ^= (value << 17) & mask
        state = value & mask
        out.append(0x20 + ((value >> 24) % 95))
    return bytes(out)


FIXTURES: dict[str, bytes] = {
    "tokens.txt": b"alpha beta gamma delta\n",
    "empty.txt": b"",
    "install_src.txt": b"install me\n",
    "shred.txt": b"SECRETDATA\n",
    "shred_del.txt": b"REMOVEME\n",
    "more.txt": b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n",
    "notelf.txt": b"not an elf\n",
    "symbols.elf": build_symbols_elf(),
}
SYMLINKS = ("echo", "sleep", "true", "false", "cat")


# ─── Cases ────────────────────────────────────────────────────────────────


class Check:
    def __init__(
        self,
        label: str,
        args: str,
        *,
        output: str | None = None,
        code: int = 0,
        contains: tuple[str, ...] = (),
    ):
        self.command = f"{RUN} {args}"
        self.label = label
        self.output = output
        self.code = code
        self.contains = contains


def c(label: str, args: str, **kwargs) -> Check:
    return Check(label, args, **kwargs)


MORE_3 = "\n".join(
    ["1", "2", "3", "--More--", "4", "5", "6", "--More--", "7", "8", "9", "--More--", "10"]
)
MORE_D_5 = "\n".join(
    [
        "1", "2", "3", "4", "5",
        "--More-- (Press space to continue, 'q' to quit.)",
        "6", "7", "8", "9", "10",
    ]
)
NM_ALL = (
    "0000000000000040 d counter\n"
    "0000000000001234 T main\n"
    "0000000000000000 U puts"
)
NM_SORTED = (
    "0000000000000000 U puts\n"
    "0000000000000040 d counter\n"
    "0000000000001234 T main\n"
)
# xxd renders 11 bytes: ten zero bytes plus the original trailing newline.
XXD_ZERO = "00000000: 0000 0000 0000 0000 0000 0a              ..........."

CASES: list[tuple[str, list[Check]]] = [
    (
        "timeout success",
        [
            c("timeout short", "timeout 1 sleep 0.2", output=""),
            c("timeout absolute", "timeout 2 /mnt/paghbox echo timed-ok",
              output="timed-ok"),
        ],
    ),
    (
        "timeout deadline",
        [
            c("timeout TERM", "timeout 0.1 sleep 2", code=124),
            c("timeout KILL", "timeout -s KILL 0.1 sleep 2", code=124),
            c("timeout numeric", "timeout -s 9 0.1 sleep 2", code=124),
        ],
    ),
    (
        "timeout errors",
        [
            c("timeout missing", "timeout 1 /mnt/nope", code=127,
              contains=("not found",)),
            c("timeout bad duration", "timeout bogus /mnt/bin/echo hi", code=2,
              contains=("invalid time interval",)),
            c("timeout bad signal", "timeout -s BOGUS 1 /mnt/bin/echo hi", code=2,
              contains=("usage: timeout",)),
        ],
    ),
    (
        "xargs batching",
        [
            c("xargs -n 2", "xargs -a /mnt/tokens.txt -n 2 echo",
              output="alpha beta\ngamma delta"),
            c("xargs -I", "xargs -a /mnt/tokens.txt -I{} echo pre-{}-post",
              output="pre-alpha-post\npre-beta-post\npre-gamma-post\npre-delta-post"),
            c("xargs empty -r", "xargs -a /mnt/empty.txt -r echo", output=""),
        ],
    ),
    (
        "xargs default and errors",
        [
            c("xargs default", "xargs -a /mnt/tokens.txt",
              output="alpha beta gamma delta"),
            c("xargs missing command", "xargs -a /mnt/tokens.txt /mnt/nope", code=124,
              contains=("not found",)),
            c("xargs failure", "xargs -a /mnt/tokens.txt -n 1 /mnt/bin/false", code=123),
        ],
    ),
    (
        "parallel sequential",
        [
            c("parallel -j 2", "parallel -j 2 -a /mnt/tokens.txt /mnt/bin/echo",
              output="alpha\nbeta\ngamma\ndelta"),
            c("parallel default echo", "parallel -a /mnt/tokens.txt",
              output="alpha\nbeta\ngamma\ndelta"),
            c("parallel bad jobs", "parallel -j 0 -a /mnt/tokens.txt echo", code=2,
              contains=("invalid job count",)),
        ],
    ),
    (
        "install create",
        [
            c("install -D -m", "install -D -m 600 /mnt/install_src.txt /mnt/inst/new/copy.txt"),
            c("install content", "cat /mnt/inst/new/copy.txt", output="install me"),
            c("install exists", "install -m 600 /mnt/install_src.txt /mnt/inst/new/copy.txt",
              code=1, contains=("File exists",)),
        ],
    ),
    (
        "install force",
        [
            c("install first", "install -m 644 /mnt/install_src.txt /mnt/ow.txt"),
            c("install -f", "install -f -m 644 /mnt/install_src.txt /mnt/ow.txt"),
            c("install force content", "cat /mnt/ow.txt", output="install me"),
        ],
    ),
    (
        "install usage",
        [
            c("install bad mode", "install -m 999 /mnt/install_src.txt /mnt/x",
              code=2, contains=("invalid mode",)),
            c("install one operand", "install /mnt/install_src.txt", code=2,
              contains=("usage: install",)),
            c("install bad option", "install -z /mnt/install_src.txt /mnt/x", code=2,
              contains=("usage: install",)),
        ],
    ),
    (
        "shred pattern and remove",
        [
            c("shred -n 1 -s 10", "shred -n 1 -s 10 /mnt/shred.txt"),
            c("shred cat after", "cat /mnt/shred.txt",
              output=shred_pattern(10, 0).decode("ascii")),
            c("shred -u", "shred -u -n 1 /mnt/shred_del.txt"),
        ],
    ),
    (
        "shred removal",
        [
            c("shred remove", "shred -u -n 1 /mnt/shred_del.txt"),
            c("shred gone", "cat /mnt/shred_del.txt", code=1,
              contains=("No such file or directory",)),
            c("shred zero", "shred -n 1 -s 10 -z /mnt/shred.txt"),
        ],
    ),
    (
        "shred zero dump",
        [
            c("shred zero run", "shred -n 1 -s 10 -z /mnt/shred.txt"),
            c("shred zero xxd", "xxd /mnt/shred.txt", output=XXD_ZERO),
            c("shred bad passes", "shred -n x /mnt/shred.txt", code=2,
              contains=("invalid pass count",)),
        ],
    ),
    (
        "command modes",
        [
            c("command -v", "command -v sleep", output="/mnt/bin/sleep"),
            c("command -V", "command -V echo", output="echo is /mnt/bin/echo"),
            c("command run", "command echo direct", output="direct"),
        ],
    ),
    (
        "command nested and errors",
        [
            c("command nested", "command /mnt/paghbox echo nested", output="nested"),
            c("command missing", "command /mnt/nope", code=127,
              contains=("not found",)),
            c("command -v missing", "command -v /mnt/nope", code=1, output=""),
        ],
    ),
    (
        "command usage",
        [
            c("command no operand", "command", code=2, contains=("usage: command",)),
            c("command bad option", "command -z", code=2, contains=("usage: command",)),
            c("command -v no operand", "command -v", code=2,
              contains=("usage: command",)),
        ],
    ),
    (
        "type alias",
        [
            c("type default", "type sleep", output="sleep is /mnt/bin/sleep"),
            c("type -v", "type -v echo", output="/mnt/bin/echo"),
            c("type missing", "type -V missing", code=1, contains=("not found",)),
        ],
    ),
    (
        "time measurements",
        [
            c("time echo", "time /mnt/bin/echo timed", code=0,
              contains=("timed", "real ", "user 0.000s", "sys 0.000s")),
            c("time false", "time /mnt/bin/false", code=1, contains=("real ",)),
            c("time missing", "time /mnt/nope", code=127, contains=("not found",)),
        ],
    ),
    (
        "nice no-op",
        [
            c("nice -n 5", "nice -n 5 /mnt/bin/echo nice", output="nice"),
            c("nice false", "nice /mnt/bin/false", code=1),
            c("nice bad", "nice -n bad echo hi", code=2,
              contains=("invalid adjustment",)),
        ],
    ),
    (
        "exit codes",
        [
            c("exit 7", "exit 7", code=7, output=""),
            c("exit default", "exit", code=0, output=""),
            c("exit invalid", "exit 300", code=2, contains=("invalid status",)),
        ],
    ),
    (
        "more paging",
        [
            c("more -n 3", "more -n 3 /mnt/more.txt", output=MORE_3),
            c("more -d", "more -d -n 5 /mnt/more.txt", output=MORE_D_5),
            c("more bad lines", "more -n 0 /mnt/more.txt", code=2,
              contains=("invalid line count",)),
        ],
    ),
    (
        "nm tables",
        [
            c("nm symtab", "nm /mnt/symbols.elf", output=NM_ALL),
            c("nm dynsym", "nm -D /mnt/symbols.elf",
              output="0000000000002000 T dynfunc"),
            c("nm undefined", "nm -u /mnt/symbols.elf",
              output="0000000000000000 U puts"),
        ],
    ),
    (
        "nm sort and errors",
        [
            c("nm numeric", "nm -n /mnt/symbols.elf", output=NM_SORTED.rstrip("\n")),
            c("nm not elf", "nm /mnt/notelf.txt", code=1,
              contains=("too short",)),
            c("nm bad option", "nm -z /mnt/symbols.elf", code=2,
              contains=("usage: nm",)),
        ],
    ),
    (
        "watch finite",
        [
            c("watch -t -c 2", "watch -t -c 2 -n 0 /mnt/bin/echo hi",
              output="\x1b[2J\x1b[Hhi\n\x1b[2J\x1b[Hhi"),
            c("watch header", "watch -c 1 -n 0 /mnt/bin/echo hi",
              contains=("Every 0s: /mnt/bin/echo hi",)),
        ],
    ),
]


RECORD = re.compile(
    rf"{re.escape(PROMPT)} ({re.escape(RUN)}[^\r\n]*)\r?\n(.*?)"
    r"run: /mnt/paghbox exited with code (\d+)",
    re.S,
)


# ─── Runner ───────────────────────────────────────────────────────────────


def build_image(disk: pathlib.Path, rootfs: pathlib.Path) -> None:
    if rootfs.exists():
        shutil.rmtree(rootfs)
    rootfs.mkdir(parents=True)
    shutil.copy2(PAGHBOX, rootfs / "paghbox")
    bindir = rootfs / "bin"
    bindir.mkdir()
    for name in SYMLINKS:
        (bindir / name).symlink_to("/mnt/paghbox")
    for name, data in FIXTURES.items():
        target = rootfs / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    if disk.exists():
        disk.unlink()
    subprocess.run(
        ["mke2fs", "-q", "-t", "ext2", "-F", "-b", "4096", "-d", str(rootfs),
         str(disk), "64M"],
        check=True,
    )


def boot(
    runner: pathlib.Path,
    kernel: pathlib.Path,
    disk: pathlib.Path,
    checks: list[Check],
    work: pathlib.Path,
    attempt: int,
    index: int,
) -> str:
    sock = work / f"serial-{index}-{attempt}.sock"
    monitor = work / f"monitor-{index}-{attempt}.sock"
    log = work / f"boot-{index}-{attempt}.log"
    commands = ["cd /mnt"] + [check.command for check in checks]
    keys = "\n".join(commands) + "\n"
    command = [
        sys.executable, str(runner),
        "--kernel", str(kernel),
        "--disk", str(disk),
        "--keys", keys,
        "--settle", "3.0",
        "--serial", str(sock),
        "--monitor", str(monitor),
        "--serial-log", str(log),
        "--shot", str(work / f"shot-{index}-{attempt}.png"),
    ]
    try:
        subprocess.run(
            command, check=False, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, timeout=180,
        )
    except subprocess.TimeoutExpired:
        return ""
    if not log.exists():
        return ""
    return log.read_text(encoding="utf-8", errors="replace")


def verify(log: str, checks: list[Check]) -> list[str]:
    records: dict[str, tuple[str, int]] = {}
    for match in RECORD.finditer(log):
        records[match.group(1).strip()] = (match.group(2), int(match.group(3)))
    failures: list[str] = []
    for check in checks:
        record = records.get(check.command)
        if record is None:
            failures.append(f"no record for {check.command!r}")
            continue
        output, code = record
        output = output.replace("\r", "")
        if output.endswith("\n"):
            output = output[:-1]
        if code != check.code:
            failures.append(f"{check.command}: code {code} != {check.code}")
        if check.output is not None and output != check.output:
            failures.append(f"{check.command}: output {output!r} != {check.output!r}")
        for needle in check.contains:
            if needle not in output:
                failures.append(f"{check.command}: {needle!r} not in {output!r}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--kernel", type=pathlib.Path, default=DEFAULT_KERNEL)
    parser.add_argument("--runner", type=pathlib.Path, default=DEFAULT_RUNNER)
    parser.add_argument("--transcript", type=pathlib.Path, default=DEFAULT_TRANSCRIPT)
    args = parser.parse_args()

    if not PAGHBOX.is_file():
        print(f"live_test: missing {PAGHBOX}; run cargo build first", file=sys.stderr)
        return 2
    if not args.kernel.is_file() or not args.runner.is_file():
        print("live_test: kernel or run.py missing", file=sys.stderr)
        return 2

    work = pathlib.Path(tempfile.mkdtemp(prefix="pagh-live-160-"))
    disk = work / "disk.img"
    rootfs = work / "rootfs"
    transcript = [f"# paghbox 1.6.0 live transcript ({len(CASES)} boots)", ""]
    passed = 0
    total = 0
    try:
        for index, (name, checks) in enumerate(CASES):
            total += len(checks)
            last_log = ""
            failures: list[str] = []
            for attempt in range(2):
                build_image(disk, rootfs)
                last_log = boot(
                    args.runner, args.kernel, disk, checks, work, attempt, index
                )
                failures = verify(last_log, checks)
                if not failures:
                    break
            if failures:
                print(f"FAIL {name}:", file=sys.stderr)
                for failure in failures:
                    print(f"  {failure}", file=sys.stderr)
                print(last_log[-5000:], file=sys.stderr)
                return 1

            transcript.append(f"===== boot {index + 1}: {name} =====")
            for check in checks:
                match = next(
                    (
                        m for m in RECORD.finditer(last_log)
                        if m.group(1).strip() == check.command
                    ),
                    None,
                )
                if match is not None:
                    output = match.group(2).replace("\r", "")
                    if output.endswith("\n"):
                        output = output[:-1]
                    transcript.append(f"{PROMPT} {check.command}")
                    if output:
                        transcript.append(output)
                    transcript.append(
                        f"run: /mnt/paghbox exited with code {match.group(3)}"
                    )
                print(f"PASS {check.label}: {check.command}")
                passed += 1
            transcript.append("")
    finally:
        shutil.rmtree(work, ignore_errors=True)

    args.transcript.parent.mkdir(parents=True, exist_ok=True)
    args.transcript.write_text("\n".join(transcript) + "\n", encoding="utf-8")
    print(f"LIVE SUMMARY: {passed}/{total} PASS")
    print(f"transcript: {args.transcript}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())