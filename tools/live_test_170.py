#!/usr/bin/env python3
"""Live QEMU check for the paghbox 1.7.0 batch (114 applets, 7 new).

Builds a 64 MiB ext2 image with the release `paghbox`, `/mnt/bin` symlinks for
the commands used by the shell and fixtures for the seven new applets. Each
case boots the kernel's own `tools/run.py` with 2-3 commands and compares
stdout/stderr plus the shell's `run: ... exited with code N` line.

The fixtures include host-computed SHA-224/SHA-384 digests, a CRC-32 check
file, Bitcoin base58 vectors, spaced hex, and a gzip stream built with
`gzip.compress(compresslevel=0)` (DEFLATE stored blocks), which is exactly the
subset `gzip`/`zcat` implement.

Usage:

    python3 tools/live_test_170.py
    python3 tools/live_test_170.py --kernel PATH --runner PATH --transcript PATH
"""

from __future__ import annotations

import argparse
import gzip
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
PAGHBOX = ROOT / "target" / "riscv64gc-unknown-none-elf" / "release" / "paghbox"
DEFAULT_KERNEL = pathlib.Path(
    "/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/debug/pagh"
)
DEFAULT_RUNNER = pathlib.Path("/home/oleg/pagh-riscv/tools/run.py")
DEFAULT_TRANSCRIPT = ROOT / "docs" / "live-1.7.0-transcript.txt"
PROMPT = "pagh:rv64>"
RUN = "run /mnt/paghbox"


# ─── Fixtures ─────────────────────────────────────────────────────────────


SHA224 = {
    "empty": "d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f",
    "abc": "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7",
    "fox": "730e109bd7a8a32b1cb9d9a09aa2325d2430587ddbc0c38bad911525",
}
SHA384 = {
    "empty": (
        "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da"
        "274edebfe76f65fbd51ad2f14898b95b"
    ),
    "abc": (
        "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed"
        "8086072ba1e7cc2358baeca134c825a7"
    ),
    "fox": (
        "ca737f1014a48f4c0b6dd43cb177b0afd9e5169367544c494011e3317dbf9a50"
        "9cb1e5dc1e85a941bbee3d7f2afbc9b1"
    ),
}
FILES = {
    "empty": b"",
    "abc": b"abc",
    "fox": b"The quick brown fox jumps over the lazy dog",
}


def sum_lines(algorithm: dict[str, str], names: tuple[str, ...]) -> str:
    return "\n".join(f"{algorithm[name]}  /mnt/{name}.txt" for name in names)


STORED = gzip.compress(b"zcat stored payload\n", compresslevel=0)
STORED_SECOND = gzip.compress(b"second member\n", compresslevel=0)
COMPRESSED = gzip.compress(b"deflate compressed payload\n", compresslevel=9)

FIXTURES: dict[str, bytes] = {
    "empty.txt": FILES["empty"],
    "abc.txt": FILES["abc"],
    "fox.txt": FILES["fox"],
    "numbers.txt": b"123456789",
    "hello.txt": b"hello\n",
    "hello.hex": b"68 65 6c 6c\n6f 0a\n",
    "hexspaced.txt": b"61 62\n63\t0a\n",
    "badhex.txt": b"61 6\n",
    "badhexchar.txt": b"61 6g\n",
    # Bitcoin base58 of hello.txt, abc.txt and the 0x00 0x01 leading-zero case.
    "b58hello.txt": b"tzCkV5DK\n",
    "b58abc.txt": b"3VNr4q\n",
    "b58bad.txt": b"0OIl\n",
    "zeros.bin": b"\x00\x01",
    "sums.sha224": (
        SHA224["abc"].encode() + b"  /mnt/abc.txt\n"
    ),
    "sums.sha224.bad": b"0" * 56 + b"  /mnt/abc.txt\n",
    "sums.sha384": (
        SHA384["abc"].encode() + b"  /mnt/abc.txt\n"
    ),
    "sums.sha384.bad": b"0" * 96 + b"  /mnt/abc.txt\n",
    "crc32.good": b"cbf43926 9 /mnt/numbers.txt\n",
    "crc32.bad": b"00000000 9 /mnt/numbers.txt\n",
    "crc32.malformed": b"this is not a checksum line\n",
    "stored.gz": STORED,
    "stored2.gz": STORED + STORED_SECOND,
    "compressed.gz": COMPRESSED,
    "bad.gz": b"not gzip at all\n",
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


CASES: list[tuple[str, list[Check]]] = [
    (
        "sha224 known vectors",
        [
            c("sha224 empty", "sha224sum /mnt/empty.txt",
              output=f"{SHA224['empty']}  /mnt/empty.txt"),
            c("sha224 abc", "sha224sum /mnt/abc.txt",
              output=f"{SHA224['abc']}  /mnt/abc.txt"),
            c("sha224 fox", "sha224sum /mnt/fox.txt",
              output=f"{SHA224['fox']}  /mnt/fox.txt"),
        ],
    ),
    (
        "sha224 options",
        [
            c("sha224 -b", "sha224sum -b /mnt/abc.txt",
              output=f"{SHA224['abc']} */mnt/abc.txt"),
            c("sha224 check", "sha224sum -c /mnt/sums.sha224",
              output="/mnt/abc.txt: OK"),
            c("sha224 bad check", "sha224sum -c /mnt/sums.sha224.bad", code=1,
              contains=("/mnt/abc.txt: FAILED", "did NOT match")),
        ],
    ),
    (
        "sha384 known vectors",
        [
            c("sha384 empty", "sha384sum /mnt/empty.txt",
              output=f"{SHA384['empty']}  /mnt/empty.txt"),
            c("sha384 abc", "sha384sum /mnt/abc.txt",
              output=f"{SHA384['abc']}  /mnt/abc.txt"),
            c("sha384 fox", "sha384sum /mnt/fox.txt",
              output=f"{SHA384['fox']}  /mnt/fox.txt"),
        ],
    ),
    (
        "sha384 options",
        [
            c("sha384 -b", "sha384sum -b /mnt/abc.txt",
              output=f"{SHA384['abc']} */mnt/abc.txt"),
            c("sha384 check", "sha384sum -c /mnt/sums.sha384",
              output="/mnt/abc.txt: OK"),
            c("sha384 bad check", "sha384sum -c /mnt/sums.sha384.bad", code=1,
              contains=("/mnt/abc.txt: FAILED", "did NOT match")),
        ],
    ),
    (
        "hash status and usage",
        [
            c("sha224 status ok", "sha224sum --status -c /mnt/sums.sha224", output=""),
            c("sha384 status bad", "sha384sum --status -c /mnt/sums.sha384.bad",
              code=1, output=""),
            c("sha224 bad option", "sha224sum -z", code=2,
              contains=("invalid option", "usage: sha224sum")),
        ],
    ),
    (
        "crc32 vectors",
        [
            c("crc32 check value", "crc32 /mnt/numbers.txt",
              output="cbf43926 9 /mnt/numbers.txt"),
            c("crc32 abc", "crc32 /mnt/abc.txt",
              output="352441c2 3 /mnt/abc.txt"),
            c("crc32 empty", "crc32 /mnt/empty.txt",
              output="00000000 0 /mnt/empty.txt"),
        ],
    ),
    (
        "crc32 check mode",
        [
            c("crc32 check ok", "crc32 -c /mnt/crc32.good",
              output="/mnt/numbers.txt: OK"),
            c("crc32 check bad", "crc32 -c /mnt/crc32.bad", code=1,
              contains=("/mnt/numbers.txt: FAILED", "did NOT match")),
            c("crc32 malformed", "crc32 -c /mnt/crc32.malformed", code=1,
              contains=("improperly formatted",)),
        ],
    ),
    (
        "base58 encode",
        [
            c("base58 hello", "base58 /mnt/hello.txt",
              output="tzCkV5DK"),
            c("base58 leading zero", "base58 /mnt/zeros.bin", output="12"),
            c("base58 usage", "base58 /mnt/hello.txt /mnt/abc.txt", code=2,
              contains=("usage: base58",)),
        ],
    ),
    (
        "base58 decode",
        [
            c("base58 decode hello", "base58 -d /mnt/b58hello.txt",
              output="hello"),
            c("base58 decode abc", "base58 -d /mnt/b58abc.txt", output="abc"),
            c("base58 invalid", "base58 -d /mnt/b58bad.txt", code=1,
              contains=("invalid input",)),
        ],
    ),
    (
        "hex2bin decode",
        [
            c("hex2bin write", "hex2bin -o /mnt/decoded.bin /mnt/hexspaced.txt"),
            c("hex2bin round trip", "bin2hex /mnt/decoded.bin",
              output="6162630a"),
            c("hex2bin invalid", "hex2bin /mnt/badhexchar.txt", code=1,
              contains=("invalid hex input",)),
        ],
    ),
    (
        "bin2hex encode",
        [
            c("bin2hex lower", "bin2hex /mnt/hello.txt",
              output="68656c6c6f0a"),
            c("bin2hex upper", "bin2hex -u /mnt/hello.txt",
              output="68656C6C6F0A"),
            c("bin2hex wrap", "bin2hex -w 4 /mnt/hello.txt",
              output="6865\n6c6c\n6f0a"),
        ],
    ),
    (
        "bin2hex bad input",
        [
            c("bin2hex bad width", "bin2hex -w x /mnt/hello.txt", code=2,
              contains=("invalid wrap width",)),
            c("bin2hex missing", "bin2hex /mnt/nope.bin", code=1,
              contains=("No such file or directory",)),
            c("bin2hex stdout hex2bin", "hex2bin -o - /mnt/hello.hex",
              output="hello"),
        ],
    ),
    (
        "zcat stored gzip",
        [
            c("zcat stored", "zcat /mnt/stored.gz", output="zcat stored payload"),
            c("zcat concatenated", "zcat /mnt/stored2.gz",
              output="zcat stored payload\nsecond member"),
            c("zcat compressed", "zcat /mnt/compressed.gz", code=1,
              contains=("compressed DEFLATE blocks are not supported",)),
        ],
    ),
    (
        "zcat errors",
        [
            c("zcat bad magic", "zcat /mnt/bad.gz", code=1,
              contains=("bad gzip magic",)),
            c("zcat bad option", "zcat -c /mnt/stored.gz", code=2,
              contains=("usage: zcat",)),
            c("zcat missing", "zcat /mnt/nope.gz", code=1,
              contains=("No such file or directory",)),
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

    work = pathlib.Path(tempfile.mkdtemp(prefix="pagh-live-170-"))
    disk = work / "disk.img"
    rootfs = work / "rootfs"
    transcript = [f"# paghbox 1.7.0 live transcript ({len(CASES)} boots)", ""]
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