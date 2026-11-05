#!/usr/bin/env python3
"""Live QEMU check for the paghbox 1.5.0 batch (94 applets, 12 new).

The suite is the batch-6 companion to `tools/live_test_140.py`. It builds a
64 MiB ext2 image with the release `paghbox` and fixtures, boots the kernel's
own `tools/run.py` once per group of 2-3 commands, and compares each command's
stdout/stderr and the shell's `run: ... exited with code N` line with the
expected values.

The last boot creates a gzip member and an ustar archive inside the guest; the
host then dumps them out of the disk image with `debugfs` and checks them with
Python's `gzip` and `tarfile`, so the compatibility is verified in both
directions (a host-produced stored-block `.gz`/ustar `.tar` is consumed in the
guest by earlier cases).

Usage:

    python3 tools/live_test_150.py
    python3 tools/live_test_150.py --kernel PATH --runner PATH --transcript PATH
"""

from __future__ import annotations

import argparse
import calendar
import gzip
import hashlib
import io
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
PAGHBOX = ROOT / "target" / "riscv64gc-unknown-none-elf" / "release" / "paghbox"
DEFAULT_KERNEL = pathlib.Path(
    "/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/debug/pagh"
)
DEFAULT_RUNNER = pathlib.Path("/home/oleg/pagh-riscv/tools/run.py")
DEFAULT_TRANSCRIPT = ROOT / "docs" / "live-1.5.0-transcript.txt"
PROMPT = "pagh:rv64>"
RUN = "run /mnt/paghbox"

MONTHS = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
]
LONG_DIR = "long" + "d" * 50
LONG_FILE = "file" + "f" * 60


def build_ext_tar() -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        directory = tarfile.TarInfo("extdir")
        directory.type = tarfile.DIRTYPE
        directory.mode = 0o755
        archive.addfile(directory)

        data = b"host made\n"
        entry = tarfile.TarInfo("extdir/one.txt")
        entry.size = len(data)
        entry.mode = 0o644
        archive.addfile(entry, io.BytesIO(data))

        link = tarfile.TarInfo("extdir/link")
        link.type = tarfile.SYMTYPE
        link.linkname = "one.txt"
        link.mode = 0o777
        archive.addfile(link)

        long_directory = tarfile.TarInfo(LONG_DIR)
        long_directory.type = tarfile.DIRTYPE
        long_directory.mode = 0o755
        archive.addfile(long_directory)

        long_data = b"long host file\n"
        long_entry = tarfile.TarInfo(f"{LONG_DIR}/{LONG_FILE}")
        long_entry.size = len(long_data)
        long_entry.mode = 0o644
        archive.addfile(long_entry, io.BytesIO(long_data))
    return buffer.getvalue()


def build_long_tar() -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        directory = tarfile.TarInfo(LONG_DIR)
        directory.type = tarfile.DIRTYPE
        directory.mode = 0o755
        archive.addfile(directory)
        data = b"long host file\n"
        entry = tarfile.TarInfo(f"{LONG_DIR}/{LONG_FILE}")
        entry.size = len(data)
        entry.mode = 0o644
        archive.addfile(entry, io.BytesIO(data))
    return buffer.getvalue()


FIXTURES: dict[str, bytes] = {
    "hello.txt": b"hello\n",
    "empty.txt": b"",
    "abc.txt": b"abc",
    "fox.txt": b"The quick brown fox jumps over the lazy dog",
    "plain.txt": b"hello\n",
    "weird.txt": b"??>?\n",
    "b64.txt": b"aGVsbG8K\n",
    "b32.txt": b"NBSWY3DPBI======\n",
    "b32hex.txt": b"D1IMOR3F18======\n",
    "b16.txt": b"68656C6C6F0A\n",
    "weird64.txt": b"Pz8+Pwo=\n",
    "weird64url.txt": b"Pz8-Pwo=\n",
    "bad64.txt": b"!!!!\n",
    "sums.md5": (
        hashlib.md5(b"abc").hexdigest().encode() + b"  /mnt/abc.txt\n"
    ),
    "sums.sha256": (
        hashlib.sha256(b"abc").hexdigest().encode() + b"  /mnt/abc.txt\n"
    ),
    "sums.bad": b"00000000000000000000000000000000  /mnt/abc.txt\n",
    "gzip_src.txt": b"stored gzip round trip\n",
    "ext.gz": gzip.compress(b"external stored gzip\n", compresslevel=0),
    "tree/a.txt": b"tree file\n",
    "tree/sub/b.txt": b"nested\n",
    "patch_me.txt": b"alpha\nbeta\ngamma\n",
    "fix.patch": (
        b"--- /mnt/patch_me.txt\n"
        b"+++ /mnt/patch_me.txt\n"
        b"@@ -1,3 +1,4 @@\n"
        b" alpha\n"
        b"-beta\n"
        b"+beta changed\n"
        b" gamma\n"
        b"+delta\n"
    ),
    "multi.txt": b"one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n",
    "multi.patch": (
        b"--- /mnt/multi.txt\n"
        b"+++ /mnt/multi.txt\n"
        b"@@ -1,3 +1,4 @@\n"
        b" one\n"
        b"-two\n"
        b"+TWO\n"
        b"+two-and-a-half\n"
        b" three\n"
        b"@@ -6,3 +7,3 @@\n"
        b" six\n"
        b"-seven\n"
        b"+SEVEN\n"
        b" eight\n"
    ),
    "multi2.txt": b"red\ngreen\nblue\n",
    "p1.patch": (
        b"--- a/multi2.txt\n"
        b"+++ b/multi2.txt\n"
        b"@@ -1,3 +1,3 @@\n"
        b" red\n"
        b"-green\n"
        b"+GREEN\n"
        b" blue\n"
    ),
    "fix2_target.txt": b"keep me\nchange me\n",
    "fix2.patch": (
        b"--- /mnt/fix2_target.txt\n"
        b"+++ /mnt/fix2_target.txt\n"
        b"@@ -1,2 +1,2 @@\n"
        b" keep me\n"
        b"-change me\n"
        b"+changed\n"
    ),
    "nonl.txt": b"alpha\nbeta",
    "nonl.patch": (
        b"--- /mnt/nonl.txt\n"
        b"+++ /mnt/nonl.txt\n"
        b"@@ -1,2 +1,2 @@\n"
        b" alpha\n"
        b"-beta\n"
        b"\\ No newline at end of file\n"
        b"+beta!\n"
        b"\\ No newline at end of file\n"
    ),
    "bad_target.txt": b"alpha\nbeta\n",
    "bad.patch": (
        b"--- /mnt/bad_target.txt\n"
        b"+++ /mnt/bad_target.txt\n"
        b"@@ -1,2 +1,2 @@\n"
        b" not the first line\n"
        b"-whatever\n"
        b"+changed\n"
    ),
    "newfile.patch": (
        b"--- /dev/null\n"
        b"+++ b/created.txt\n"
        b"@@ -0,0 +1,2 @@\n"
        b"+first\n"
        b"+second\n"
    ),
    "badcount.patch": (
        b"--- /mnt/bad_target.txt\n"
        b"+++ /mnt/bad_target.txt\n"
        b"@@ -1,5 +1,2 @@\n"
        b" alpha\n"
        b"-beta\n"
        b"+changed\n"
    ),
    f"{LONG_DIR}/{LONG_FILE}": b"long guest file\n",
    "ext.tar": build_ext_tar(),
    "long.tar": build_long_tar(),
}


# ─── Calendar reference model ─────────────────────────────────────────────


def center(text: str, width: int) -> str:
    return " " * ((width - len(text)) // 2) + text


def month_block(year: int, month: int) -> list[str]:
    lines = [center(f"{MONTHS[month - 1]} {year}", 20), "Su Mo Tu We Th Fr Sa"]
    for week in calendar.Calendar(firstweekday=6).monthdayscalendar(year, month):
        lines.append(" ".join(f"{day:2d}" if day else "  " for day in week).rstrip())
    while len(lines) < 8:
        lines.append("")
    return lines


def month_lines(year: int, month: int) -> list[str]:
    lines = month_block(year, month)
    while len(lines) > 2 and lines[-1] == "":
        lines.pop()
    return lines


def year_lines(year: int) -> list[str]:
    lines = [center(str(year), 64), ""]
    for quarter in range(4):
        blocks = [month_block(year, quarter * 3 + offset + 1) for offset in range(3)]
        for row in range(8):
            line = ""
            for index, block in enumerate(blocks):
                if index:
                    line += "  "
                line += block[row].ljust(20)
            lines.append(line.rstrip())
        if quarter < 3:
            lines.append("")
    return lines


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


MD5 = {
    "empty": "d41d8cd98f00b204e9800998ecf8427e",
    "abc": "900150983cd24fb0d6963f7d28e17f72",
    "fox": "9e107d9d372bb6826bd81d3542a419d6",
}
SHA1 = {
    "empty": "da39a3ee5e6b4b0d3255bfef95601890afd80709",
    "abc": "a9993e364706816aba3e25717850c26c9cd0d89d",
    "fox": "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12",
}
SHA256 = {
    "empty": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "abc": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    "fox": "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592",
}
SHA512 = {
    "empty": (
        "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce"
        "47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
    ),
    "abc": (
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a"
        "2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    ),
    "fox": (
        "07e547d9586f6a73f73fbac0435ed76951218fb7d0c8d788a309d785436bbb64"
        "2e93a252a954f23912547d1e8a3b5ed6e1bfd7097821233fa0538f3db854fee6"
    ),
}
B2 = {
    "empty": (
        "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419"
        "d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce"
    ),
    "abc": (
        "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d"
        "17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"
    ),
    "fox": (
        "a8add4bdddfd93e4877d2746e62817b116364a1fa7bc148d95090bc7333b3673"
        "f82401cf7aa2e4cb1ecd90296e3f14cb5413f8ed77be73045b13914cdcd6a918"
    ),
}


def sum_lines(algorithm: dict[str, str], names: tuple[str, ...]) -> str:
    return "\n".join(f"{algorithm[name]}  /mnt/{name}.txt" for name in names)


def checksum_case(name: str, args: str, names: tuple[str, ...]) -> tuple:
    algorithm = {"md5sum": MD5, "sha1sum": SHA1, "sha256sum": SHA256,
                 "sha512sum": SHA512, "b2sum": B2}[name]
    return (
        f"{name} vectors",
        [
            c(name, args, output=sum_lines(algorithm, names)),
        ],
        False,
    )


CASES: list[tuple[str, list[Check], bool]] = [
    checksum_case("md5sum", "md5sum /mnt/empty.txt /mnt/abc.txt /mnt/fox.txt",
                  ("empty", "abc", "fox")),
    checksum_case("sha1sum", "sha1sum /mnt/empty.txt /mnt/abc.txt /mnt/fox.txt",
                  ("empty", "abc", "fox")),
    checksum_case("sha256sum", "sha256sum /mnt/empty.txt /mnt/abc.txt /mnt/fox.txt",
                  ("empty", "abc", "fox")),
    checksum_case("sha512sum", "sha512sum /mnt/empty.txt /mnt/abc.txt /mnt/fox.txt",
                  ("empty", "abc", "fox")),
    checksum_case("b2sum", "b2sum /mnt/empty.txt /mnt/abc.txt /mnt/fox.txt",
                  ("empty", "abc", "fox")),
    (
        "hash options",
        [
            c("md5sum -b", "md5sum -b /mnt/abc.txt",
              output=f"{MD5['abc']} */mnt/abc.txt"),
            c("md5sum check", "md5sum -c /mnt/sums.md5",
              output="/mnt/abc.txt: OK"),
            c("sha256 check", "sha256sum -c /mnt/sums.sha256",
              output="/mnt/abc.txt: OK"),
        ],
        False,
    ),
    (
        "hash check failure",
        [
            c("md5sum bad check", "md5sum -c /mnt/sums.bad", code=1,
              contains=("/mnt/abc.txt: FAILED", "WARNING: 1 computed checksum")),
            c("md5sum --status", "md5sum -c --status /mnt/sums.md5", output=""),
            c("md5sum unknown option", "md5sum -z", code=2,
              contains=("usage: md5sum",)),
        ],
        False,
    ),
    (
        "basenc base64",
        [
            c("basenc base64 encode", "basenc --base64 /mnt/plain.txt",
              output="aGVsbG8K"),
            c("basenc base64 decode", "basenc --base64 -d /mnt/b64.txt",
              output="hello"),
            c("basenc base64url decode", "basenc --base64url -d /mnt/weird64url.txt",
              output="??>?"),
        ],
        False,
    ),
    (
        "basenc base32",
        [
            c("basenc base32 encode", "basenc --base32 /mnt/plain.txt",
              output="NBSWY3DPBI======"),
            c("basenc base32 decode", "basenc --base32 -d /mnt/b32.txt",
              output="hello"),
            c("basenc base32hex decode", "basenc --base32hex -d /mnt/b32hex.txt",
              output="hello"),
        ],
        False,
    ),
    (
        "basenc base16 and wrap",
        [
            c("basenc base16 encode", "basenc --base16 /mnt/plain.txt",
              output="68656C6C6F0A"),
            c("basenc base16 decode", "basenc --base16 -d /mnt/b16.txt",
              output="hello"),
            c("basenc -w 4", "basenc --base64 -w 4 /mnt/plain.txt",
              output="aGVs\nbG8K"),
        ],
        False,
    ),
    (
        "basenc errors",
        [
            c("basenc missing type", "basenc /mnt/plain.txt", code=2,
              contains=("missing encoding type",)),
            c("basenc invalid data", "basenc --base64 -d /mnt/bad64.txt", code=1,
              output="basenc: invalid input"),
            c("basenc bad option", "basenc -z", code=2, contains=("usage: basenc",)),
        ],
        False,
    ),
    (
        "gzip round trip",
        [
            c("gzip keep", "gzip -k /mnt/gzip_src.txt"),
            c("gunzip force", "gunzip -f /mnt/gzip_src.txt.gz"),
            c("gzip content", "cat /mnt/gzip_src.txt",
              output="stored gzip round trip"),
        ],
        False,
    ),
    (
        "gzip external member",
        [
            c("gzip -dc host stream", "gzip -d -c /mnt/ext.gz",
              output="external stored gzip"),
            c("gunzip -c host stream", "gunzip -c /mnt/ext.gz",
              output="external stored gzip"),
        ],
        False,
    ),
    (
        "gzip naming",
        [
            c("gzip first", "gzip -k /mnt/plain.txt"),
            c("gzip exists", "gzip --keep /mnt/plain.txt", code=1,
              contains=("already exists",)),
            c("gzip force", "gzip -fk /mnt/plain.txt"),
        ],
        False,
    ),
    (
        "gzip errors",
        [
            c("gzip unknown suffix", "gzip -d /mnt/plain.txt", code=1,
              contains=("unknown suffix",)),
            c("gzip bad option", "gzip -z", code=2, contains=("usage: gzip",)),
            c("gunzip bad option", "gunzip -z", code=2, contains=("usage: gunzip",)),
        ],
        False,
    ),
    (
        "tar create and list",
        [
            c("tar create", "tar -cf /mnt/tree.tar -C /mnt tree"),
            c("tar list", "tar -tf /mnt/tree.tar",
              output="tree/\ntree/a.txt\ntree/link\ntree/sub/\ntree/sub/b.txt"),
        ],
        False,
    ),
    (
        "tar extract",
        [
            c("tar create again", "tar -cf /mnt/tree.tar -C /mnt tree"),
            c("tar extract", "tar -xf /mnt/tree.tar -C /mnt/out"),
            c("tar cat", "cat /mnt/out/tree/a.txt", output="tree file"),
        ],
        False,
    ),
    (
        "tar host archive",
        [
            c("tar list host", "tar -tf /mnt/ext.tar",
              output=(
                  "extdir/\nextdir/one.txt\nextdir/link\n"
                  f"{LONG_DIR}/\n{LONG_DIR}/{LONG_FILE}"
              )),
        ],
        False,
    ),
    (
        "tar host extract",
        [
            c("tar extract host", "tar -xf /mnt/ext.tar -C /mnt/out2"),
            c("tar readlink host", "readlink /mnt/out2/extdir/link",
              output="one.txt"),
        ],
        False,
    ),
    (
        "untar alias",
        [
            c("untar extract", "untar -f /mnt/ext.tar -C /mnt/out2"),
            c("untar cat", "cat /mnt/out2/extdir/one.txt", output="host made"),
        ],
        False,
    ),
    (
        "tar long prefix",
        [
            c("tar list long", "tar -tf /mnt/long.tar",
              output=f"{LONG_DIR}/\n{LONG_DIR}/{LONG_FILE}"),
        ],
        False,
    ),
    (
        "tar usage",
        [
            c("tar no mode", "tar -f /mnt/ext.tar", code=2,
              contains=("usage: tar",)),
            c("tar bad option", "tar -z", code=2, contains=("usage: tar",)),
            c("untar conflict", "untar -c -f /mnt/ext.tar", code=2,
              contains=("conflicting mode",)),
        ],
        False,
    ),
    (
        "patch in place",
        [
            c("patch apply", "patch -i /mnt/fix.patch"),
            c("patch result", "cat /mnt/patch_me.txt",
              output="alpha\nbeta changed\ngamma\ndelta"),
            c("patch backup", "cat /mnt/patch_me.txt.orig",
              output="alpha\nbeta\ngamma"),
        ],
        False,
    ),
    (
        "patch multiple hunks",
        [
            c("patch multi", "patch -i /mnt/multi.patch"),
            c("patch multi result", "cat /mnt/multi.txt",
              output=(
                  "one\nTWO\ntwo-and-a-half\nthree\nfour\nfive\nsix\nSEVEN\neight"
              )),
        ],
        False,
    ),
    (
        "patch -p1",
        [
            c("patch p1", "patch -p1 -i /mnt/p1.patch"),
            c("patch p1 result", "cat /mnt/multi2.txt", output="red\nGREEN\nblue"),
        ],
        False,
    ),
    (
        "patch -o",
        [
            c("patch output", "patch -o /mnt/patched.out -i /mnt/fix2.patch"),
            c("patch original intact", "cat /mnt/fix2_target.txt",
              output="keep me\nchange me"),
            c("patch output content", "cat /mnt/patched.out",
              output="keep me\nchanged"),
        ],
        False,
    ),
    (
        "patch no newline",
        [
            c("patch no newline", "patch -i /mnt/nonl.patch"),
            c("patch no newline xxd", "xxd /mnt/nonl.txt",
              output="00000000: 616c 7068 610a 6265 7461 21              alpha.beta!"),
        ],
        False,
    ),
    (
        "patch new file",
        [
            c("patch create file", "patch -p1 -i /mnt/newfile.patch"),
            c("patch created content", "cat /mnt/created.txt",
              output="first\nsecond"),
        ],
        False,
    ),
    (
        "patch failure and usage",
        [
            c("patch context fail", "patch -i /mnt/bad.patch", code=1,
              contains=("hunk #1 failed at source line 1",)),
            c("patch count fail", "patch -i /mnt/badcount.patch", code=1,
              contains=("truncated",)),
            c("patch missing arg", "patch -p", code=2, contains=("usage: patch",)),
        ],
        False,
    ),
    (
        "cal months",
        [
            c("cal feb 2024", "cal 2 2024", output="\n".join(month_lines(2024, 2))),
            c("cal jan 1970", "cal 1 1970", output="\n".join(month_lines(1970, 1))),
            c("cal sep 1752", "cal 9 1752", output="\n".join(month_lines(1752, 9))),
        ],
        False,
    ),
    (
        "cal year grid",
        [
            c("cal -y 2024", "cal -y 2024", output="\n".join(year_lines(2024))),
        ],
        False,
    ),
    (
        "cal current and y",
        [
            c("cal current", "cal", output="\n".join(month_lines(1970, 1))),
            c("cal 2024 grid", "cal 2024", output="\n".join(year_lines(2024))),
        ],
        False,
    ),
    (
        "cal usage",
        [
            c("cal month 13", "cal 13 2024", code=2, contains=("month must be 1..12",)),
            c("cal year 0", "cal 2 0", code=2, contains=("invalid year",)),
            c("cal bad option", "cal -z", code=2, contains=("usage: cal",)),
        ],
        False,
    ),
    (
        "host artifacts",
        [
            c("host gzip create", "gzip -k /mnt/plain.txt"),
            c("host tar create", f"tar -cf /mnt/tree.tar -C /mnt tree {LONG_DIR}"),
        ],
        False,
    ),
]

RECORD = re.compile(
    rf"{re.escape(PROMPT)} ({re.escape(RUN)}[^\r\n]*)\r?\n(.*?)"
    r"run: /mnt/paghbox exited with code (\d+)",
    re.S,
)


def build_image(disk: pathlib.Path, rootfs: pathlib.Path) -> None:
    if rootfs.exists():
        shutil.rmtree(rootfs)
    rootfs.mkdir(parents=True)
    shutil.copy2(PAGHBOX, rootfs / "paghbox")
    for name, data in FIXTURES.items():
        target = rootfs / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    for directory in ("out", "out2"):
        (rootfs / directory).mkdir(parents=True, exist_ok=True)
    (rootfs / "tree" / "link").symlink_to("a.txt")
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
        sys.executable,
        str(runner),
        "--kernel",
        str(kernel),
        "--disk",
        str(disk),
        "--keys",
        keys,
        "--settle",
        "3.0",
        "--serial",
        str(sock),
        "--monitor",
        str(monitor),
        "--serial-log",
        str(log),
        "--shot",
        str(work / f"shot-{index}-{attempt}.png"),
    ]
    subprocess.run(
        command,
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=180,
    )
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


def check_host_artifacts(disk: pathlib.Path, work: pathlib.Path) -> list[str]:
    """Dump guest-created files and validate them with host Python."""
    failures: list[str] = []
    gz_path = work / "guest-plain.txt.gz"
    tar_path = work / "guest-tree.tar"
    for inner, outer in (("/plain.txt.gz", gz_path), ("/tree.tar", tar_path)):
        if outer.exists():
            outer.unlink()
        result = subprocess.run(
            ["debugfs", "-R", f"dump {inner} {outer}", str(disk)],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if result.returncode != 0 or not outer.is_file():
            failures.append(f"debugfs could not dump {inner}")
            continue
    if not failures:
        try:
            data = gzip.decompress(gz_path.read_bytes())
            if data != b"hello\n":
                failures.append(f"host gzip payload {data!r} != b'hello\\n'")
        except OSError as error:
            failures.append(f"host gzip decompress failed: {error}")
    if not failures:
        try:
            with tarfile.open(tar_path, "r:") as archive:
                names = archive.getnames()
                member = archive.extractfile("tree/a.txt")
                content = member.read() if member is not None else b""
            expected = {
                "tree",
                "tree/a.txt",
                "tree/link",
                "tree/sub",
                "tree/sub/b.txt",
                LONG_DIR,
                f"{LONG_DIR}/{LONG_FILE}",
            }
            if set(names) != expected:
                failures.append(f"host tar names {sorted(names)!r} != {sorted(expected)!r}")
            if content != b"tree file\n":
                failures.append(f"host tar tree/a.txt content {content!r}")
            if f"{LONG_DIR}/{LONG_FILE}" not in names:
                failures.append("host tar did not store the long name")
        except (OSError, tarfile.TarError) as error:
            failures.append(f"host tar read failed: {error}")
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
    if shutil.which("debugfs") is None:
        print("live_test: debugfs is required for the host round-trip check", file=sys.stderr)
        return 2

    work = pathlib.Path(tempfile.mkdtemp(prefix="pagh-live-150-"))
    disk = work / "disk.img"
    rootfs = work / "rootfs"
    transcript = [f"# paghbox 1.5.0 live transcript ({len(CASES)} boots)", ""]
    passed = 0
    total = 0
    try:
        for index, (name, checks, _net) in enumerate(CASES):
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
                        m
                        for m in RECORD.finditer(last_log)
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

        # The last boot left the guest-created gzip/tar in `disk`.
        total += 2
        host_failures = check_host_artifacts(disk, work)
        transcript.append("===== host compatibility =====")
        transcript.append("$ debugfs dump /plain.txt.gz; python3 gzip.decompress")
        transcript.append("$ debugfs dump /tree.tar; python3 tarfile")
        if host_failures:
            print("FAIL host round-trip:", file=sys.stderr)
            for failure in host_failures:
                print(f"  {failure}", file=sys.stderr)
            transcript.append("FAIL host gzip/tar round-trip")
            args.transcript.parent.mkdir(parents=True, exist_ok=True)
            args.transcript.write_text("\n".join(transcript) + "\n", encoding="utf-8")
            return 1
        transcript.append("PASS host gzip: gzip.decompress(guest /plain.txt.gz) == b'hello\\n'")
        transcript.append("PASS host tar: tarfile read guest /tree.tar (tree + long name)")
        print("PASS host gzip: gzip.decompress(guest /plain.txt.gz)")
        print("PASS host tar: tarfile read guest /tree.tar")
        passed += 2
    finally:
        shutil.rmtree(work, ignore_errors=True)

    args.transcript.parent.mkdir(parents=True, exist_ok=True)
    args.transcript.write_text("\n".join(transcript) + "\n", encoding="utf-8")
    print(f"LIVE SUMMARY: {passed}/{total} PASS")
    print(f"transcript: {args.transcript}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())