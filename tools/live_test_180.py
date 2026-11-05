#!/usr/bin/env python3
"""Live QEMU check for the paghbox 1.8.0 batch (120 applets, 6 new).

Builds a 64 MiB ext2 image with the release `paghbox` and fixtures for the six
new applets: SHA3-256 known vectors and check files, base85 vectors (bare and
Adobe-delimited), an invalid base85 file, a synthetic ELF with `.symtab`,
`.dynsym` and section names, and a non-ELF file. `calc`, `uuidgen` and
`paghfetch` need no fixtures: their inputs are arguments and the kernel itself.

Each case boots the kernel's own `tools/run.py` with 2-3 commands and compares
stdout/stderr, the exit code from `run: ... exited with code N`, and — for the
UUID output, which is random by design — a regular expression.

Usage:

    python3 tools/live_test_180.py
    python3 tools/live_test_180.py --kernel PATH --runner PATH --transcript PATH
"""

from __future__ import annotations

import argparse
import base64
import hashlib
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
DEFAULT_TRANSCRIPT = ROOT / "docs" / "live-1.8.0-transcript.txt"
PROMPT = "pagh:rv64>"
RUN = "run /mnt/paghbox"


# ─── Fixtures ─────────────────────────────────────────────────────────────


def build_symbols_elf() -> bytes:
    """A valid ELF64 LE RISC-V image with .symtab and .dynsym.

    Symbols: a local object `counter` in .data, a global function `main` in
    .text and an undefined global `puts`; `.dynsym` holds `dynfunc`. Section
    names come from `.shstrtab`, e_shstrndx = 7.
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


HELLO = b"hello\n"
ABC = b"abc"
FOX = b"The quick brown fox jumps over the lazy dog"
SHA3 = {
    "empty": hashlib.sha3_256(b"").hexdigest(),
    "abc": hashlib.sha3_256(ABC).hexdigest(),
    "fox": hashlib.sha3_256(FOX).hexdigest(),
}
assert SHA3["empty"] == "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a"
assert SHA3["abc"] == "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
assert SHA3["fox"] == "69070dda01975c8c120c3aada1b282394e7f032fa9cf32f4cb2259a0897dfc04"

A85_HELLO = base64.a85encode(HELLO)
A85_ABC = base64.a85encode(ABC)
A85_HELLO_ADOBE = base64.a85encode(HELLO, adobe=True)

FIXTURES: dict[str, bytes] = {
    "empty.txt": b"",
    "abc.txt": ABC,
    "fox.txt": FOX,
    "hello.txt": HELLO,
    "zeros.bin": b"\x00\x00\x00\x00",
    "notelf.txt": b"not an ELF file\n" + b"\0" * 49,
    "symbols.elf": build_symbols_elf(),
    # base85 vectors are built by the host encoder; the guest must agree.
    "b85hello.txt": A85_HELLO + b"\n",
    "b85adobe.txt": A85_HELLO_ADOBE + b"\n",
    "b85bad.txt": b"<~v~>\n",
    "sums.sha3": SHA3["abc"].encode() + b"  /mnt/abc.txt\n",
    "sums.sha3.bad": b"0" * 64 + b"  /mnt/abc.txt\n",
}
SYMLINKS = ("echo", "true", "false")


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
        pattern: str | None = None,
    ):
        self.command = f"{RUN} {args}"
        self.label = label
        self.output = output
        self.code = code
        self.contains = contains
        self.pattern = pattern


def c(label: str, args: str, **kwargs) -> Check:
    return Check(label, args, **kwargs)


UUID = r"[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}"
ELF_HEADER = (
    "ELF header:\n"
    "  type:      EXEC (2)\n"
    "  machine:   RISC-V (243)\n"
    "  entry:     0x0000000000000000\n"
    "  phoff:     0\n"
    "  shoff:     64\n"
    "  flags:     0x0\n"
    "  ehsize:    64\n"
    "  phentsize: 0\n"
    "  phnum:     0\n"
    "  shentsize: 64\n"
    "  shnum:     8"
)
ELF_SYMBOLS = (
    "Symbols:\n"
    "0000000000000040 d counter\n"
    "0000000000001234 T main\n"
    "0000000000000000 U puts"
)

CASES: list[tuple[str, list[Check]]] = [
    (
        "sha3 known vectors",
        [
            c("sha3 empty", "sha3sum /mnt/empty.txt",
              output=f"{SHA3['empty']}  /mnt/empty.txt"),
            c("sha3 abc", "sha3sum /mnt/abc.txt",
              output=f"{SHA3['abc']}  /mnt/abc.txt"),
            c("sha3 fox", "sha3sum /mnt/fox.txt",
              output=f"{SHA3['fox']}  /mnt/fox.txt"),
        ],
    ),
    (
        "sha3 options",
        [
            c("sha3 -b", "sha3sum -b /mnt/abc.txt",
              output=f"{SHA3['abc']} */mnt/abc.txt"),
            c("sha3 check", "sha3sum -c /mnt/sums.sha3",
              output="/mnt/abc.txt: OK"),
            c("sha3 bad check", "sha3sum -c /mnt/sums.sha3.bad", code=1,
              contains=("/mnt/abc.txt: FAILED", "did NOT match")),
        ],
    ),
    (
        "sha3 status and usage",
        [
            c("sha3 status ok", "sha3sum --status -c /mnt/sums.sha3", output=""),
            c("sha3 status bad", "sha3sum --status -c /mnt/sums.sha3.bad",
              code=1, output=""),
            c("sha3 bad option", "sha3sum -z", code=2,
              contains=("invalid option", "usage: sha3sum")),
        ],
    ),
    (
        "calc arithmetic",
        [
            c("calc precedence", "calc 2+3*4", output="14"),
            c("calc parentheses", "calc (2+3)*4", output="20"),
            c("calc unary minus", "calc -5+2", output="-3"),
        ],
    ),
    (
        "calc errors",
        [
            c("calc divide by zero", "calc 1/0", code=1,
              contains=("division by zero",)),
            c("calc syntax", "calc 2+", code=2, contains=("syntax error",)),
            c("calc modulo zero", "calc 7%0", code=1,
              contains=("division by zero",)),
        ],
    ),
    (
        "calc multiple and -e",
        [
            c("calc two expressions", "calc 1+1 2*3", output="2\n6"),
            c("calc eval", "calc -e 10%3", output="1"),
        ],
    ),
    (
        "base85 encode",
        [
            c("base85 hello", "base85 /mnt/hello.txt", output=A85_HELLO.decode()),
            c("base85 zero group", "base85 /mnt/zeros.bin", output="z"),
            c("base85 adobe", "base85 -a /mnt/abc.txt",
              output=f"<~{A85_ABC.decode()}~>"),
        ],
    ),
    (
        "base85 decode",
        [
            c("base85 decode hello", "base85 -d /mnt/b85hello.txt",
              output="hello"),
            c("base85 decode adobe", "base85 -d /mnt/b85adobe.txt",
              output="hello"),
            c("base85 invalid", "base85 -d /mnt/b85bad.txt", code=1,
              contains=("invalid input",)),
        ],
    ),
    (
        "readelf header",
        [
            c("readelf header", "readelf /mnt/symbols.elf", output=ELF_HEADER),
            c("readelf not elf", "readelf -h /mnt/notelf.txt", code=1,
              contains=("not an ELF file",)),
            c("readelf no operand", "readelf", code=2,
              contains=("usage: readelf",)),
        ],
    ),
    (
        "readelf sections and symbols",
        [
            c("readelf sections", "readelf -S /mnt/symbols.elf",
              contains=("Sections:", "[ 1] .text", "PROGBITS",
                        "[ 5] .symtab", "SYMTAB", "[ 7] .shstrtab", "STRTAB")),
            c("readelf symbols", "readelf -s /mnt/symbols.elf", output=ELF_SYMBOLS),
        ],
    ),
    (
        "readelf program headers and errors",
        [
            c("readelf program headers", "readelf -l /mnt/symbols.elf",
              output="Program headers: none"),
            c("readelf bad section file", "readelf -S /mnt/notelf.txt", code=1,
              contains=("not an ELF file",)),
            c("readelf bad option", "readelf -z /mnt/symbols.elf", code=2,
              contains=("usage: readelf",)),
        ],
    ),
    (
        "uuidgen",
        [
            c("uuidgen default", "uuidgen", pattern=UUID),
            c("uuidgen count", "uuidgen -n 3",
              pattern=rf"{UUID}\n{UUID}\n{UUID}"),
            c("uuidgen bad count", "uuidgen -n x", code=2,
              contains=("invalid count",)),
        ],
    ),
    (
        "paghfetch",
        [
            c("paghfetch", "paghfetch",
              contains=("paghfetch", "sysname:", "nodename:", "machine:",
                        "uptime:", "nproc:", "pwd:  ", "/mnt")),
            c("paghfetch usage", "paghfetch x", code=2,
              contains=("usage: paghfetch",)),
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
        if check.pattern is not None and re.fullmatch(check.pattern, output) is None:
            failures.append(
                f"{check.command}: output {output!r} does not match {check.pattern!r}"
            )
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

    work = pathlib.Path(tempfile.mkdtemp(prefix="pagh-live-180-"))
    disk = work / "disk.img"
    rootfs = work / "rootfs"
    transcript = [f"# paghbox 1.8.0 live transcript ({len(CASES)} boots)", ""]
    passed = 0
    total = 0
    try:
        for index, (name, checks) in enumerate(CASES):
            total += len(checks)
            last_log = ""
            failures: list[str] = []
            for attempt in range(3):
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