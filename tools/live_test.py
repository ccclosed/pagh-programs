#!/usr/bin/env python3
"""Boot the real pagh-riscv kernel in QEMU and verify the 1.3.0 applets.

This is the live half of the release check. It builds a 64 MiB ext2 image with
`mke2fs -d`, copies the release `paghbox` into it, then boots the kernel's own
`tools/run.py` once per group of 2-4 commands. Keeping the groups short matters:
the serial driver loses long typed lines, so a boot with five commands can end
with the shell never seeing the last one.

For every command the runner asserts the exact stdout (for the deterministic
tools), the exit code, and for error paths a diagnostic substring. Any mismatch
is retried once on a freshly rebuilt image; the successful boots are written to
`docs/live-1.3.0-transcript.txt` and a PASS line is printed per command.

Usage:

    python3 tools/live_test.py
    python3 tools/live_test.py --kernel PATH --runner PATH --transcript PATH
"""

from __future__ import annotations

import argparse
import base64
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
DEFAULT_TRANSCRIPT = ROOT / "docs" / "live-1.3.0-transcript.txt"
PROMPT = "pagh:rv64>"
RUN = "run /mnt/paghbox"
FIXTURES = {
    "hello.txt": b"hello\n",
    "lines.txt": b"one\ntwo\nthree\n",
    "sed.txt": b"alpha\nbeta42\nGamma\ndelta\n",
    "awk.txt": b"apple:10\nbanana:25\ncherry:5\ndate:30\n",
    "shuf.txt": b"red\ngreen\nblue\nyellow\nmagenta\n",
    "diff1.txt": b"a\nb\nc\nd\n",
    "diff2.txt": b"a\nB\nc\ne\n",
    "diff_ins.txt": b"a\nc\n",
    "diff_del.txt": b"a\nb\nc\n",
    "cmp1.txt": b"abc\ndef\n",
    "cmp2.txt": b"abc\nxef\n",
    "eof1.txt": b"abc\n",
    "eof2.txt": b"abc\nzzz\n",
    "same1.txt": b"same\n",
    "same2.txt": b"same\n",
    "base.txt": b"hello world\n",
    "invalid.b64": b"!!!!\n",
    "invalid.b32": b"!!!\n",
}
FIXTURES["hello.b64"] = base64.b64encode(FIXTURES["hello.txt"]) + b"\n"
FIXTURES["hello.b32"] = base64.b32encode(FIXTURES["hello.txt"]) + b"\n"
FIXTURES["base.b64"] = base64.b64encode(FIXTURES["base.txt"])
FIXTURES["base.b32"] = base64.b32encode(FIXTURES["base.txt"])


class Check:
    def __init__(self, command: str, output: str | None = None, code: int = 0,
                 contains: str | None = None, label: str = ""):
        self.command = command
        self.output = output
        self.code = code
        self.contains = contains
        self.label = label


def c(label: str, args: str, **kwargs) -> Check:
    return Check(f"{RUN} {args}", label=label, **kwargs)


CASES: list[tuple[str, list[Check]]] = [
    ("kernel version", [
        c("uname", "uname -a", output=None, contains="0.16."),
    ]),
    ("rmdir", [
        c("rmdir -p", "rmdir -p p1/p2/p3", output=""),
        c("rmdir multiple", "rmdir /mnt/d_empty /mnt/d_nonempty", code=1,
          contains="Directory not empty"),
    ]),
    ("tac sum cksum", [
        c("tac", "tac /mnt/lines.txt", output="three\ntwo\none"),
        c("sum", "sum /mnt/hello.txt", output="36979     1 /mnt/hello.txt"),
        c("sum -s", "sum -s /mnt/hello.txt", output="542 1 /mnt/hello.txt"),
        c("cksum", "cksum /mnt/hello.txt", output="3015617425 6 /mnt/hello.txt"),
    ]),
    ("base64/base32 encode", [
        c("base64", "base64 /mnt/hello.txt", output="aGVsbG8K"),
        c("base64 -w 4", "base64 -w 4 /mnt/hello.txt", output="aGVs\nbG8K"),
        c("base32", "base32 /mnt/hello.txt", output="NBSWY3DPBI======"),
        c("base32 -w 8", "base32 -w 8 /mnt/base.txt",
          output="NBSWY3DP\nEB3W64TM\nMQFA===="),
    ]),
    ("decode + diff", [
        c("base64 -d", "base64 -d /mnt/hello.b64", output="hello"),
        c("base32 -d", "base32 -d /mnt/hello.b32", output="hello"),
        c("diff equal", "diff /mnt/same1.txt /mnt/same2.txt", output=""),
        c("diff change", "diff /mnt/diff1.txt /mnt/diff2.txt", code=1,
          output="2c2\n< b\n---\n> B\n4c4\n< d\n---\n> e"),
    ]),
    ("cmp report", [
        c("cmp first", "cmp /mnt/cmp1.txt /mnt/cmp2.txt", code=1,
          output="/mnt/cmp1.txt /mnt/cmp2.txt differ: byte 5, line 2, offset 0x4"),
        c("cmp -l", "cmp -l /mnt/cmp1.txt /mnt/cmp2.txt", code=1,
          output="5 144 170"),
    ]),
    ("cmp silent/eof", [
        c("cmp -s", "cmp -s /mnt/cmp1.txt /mnt/cmp2.txt", code=1, output=""),
        c("cmp equal", "cmp /mnt/same1.txt /mnt/same2.txt", output=""),
        c("cmp eof", "cmp /mnt/eof1.txt /mnt/eof2.txt", code=1,
          output="cmp: EOF on /mnt/eof1.txt after byte 4, line 1"),
    ]),
    ("sed", [
        c("sed address p", "sed -n 2p /mnt/sed.txt", output="beta42"),
        c("sed substitute", "sed s/a/X/g /mnt/sed.txt",
          output="XlphX\nbetX42\nGXmmX\ndeltX"),
        c("sed delete range", "sed 2,3d /mnt/sed.txt", output="alpha\ndelta"),
        c("sed two commands", "sed -n 1p;2p /mnt/sed.txt", output="alpha\nbeta42"),
    ]),
    ("sed empty-global", [
        c("sed a* global", "sed s/a*/X/g /mnt/sed.txt",
          output="XlXpXhX\nXbXeXtX4X2X\nXGXmXmX\nXdXeXlXtX"),
    ]),
    ("awk comparisons", [
        c("awk compare", "awk -F: $2>20{print$1,$2} /mnt/awk.txt",
          output="banana 25\ndate 30"),
        c("awk string", "awk -F: $1==banana{print$2} /mnt/awk.txt", output="25"),
    ]),
    ("awk regex/NR", [
        c("awk regex/NR", "awk /a/{print$1,NR} /mnt/awk.txt",
          output="apple:10 1\nbanana:25 2\ndate:30 4"),
        c("awk NR", "awk -F: NR==2{print$2} /mnt/awk.txt", output="25"),
    ]),
    ("shuf", [
        c("shuf -n", "shuf -n 3 /mnt/shuf.txt", output="magenta\nblue\ngreen"),
        c("shuf -e", "shuf -e a b c", output="c\nb\na"),
        c("shuf -i", "shuf -i 1-5", output="5\n3\n2\n4\n1"),
    ]),
    ("numfmt core", [
        c("numfmt si", "numfmt --to=si 1000 1500 1000000",
          output="1.0k\n1.5k\n1.0M"),
        c("numfmt iec", "numfmt --to=iec 1024 1536", output="1.0K\n1.5K"),
        c("numfmt from", "numfmt --from=si 1.5K", output="1500"),
        c("numfmt pad/suffix", "numfmt --to=si --suffix=B --padding=8 1000",
          output="   1.0kB"),
    ]),
    ("numfmt extras", [
        c("numfmt auto", "numfmt --to=auto 1024 1500 1000000",
          output="1.0K\n1.5k\n1.0M"),
        c("numfmt unit", "numfmt --to=si --to-unit=1000 5000", output="5"),
        c("numfmt from iec", "numfmt --from=iec 1Ki 2M", output="1024\n2097152"),
        c("numfmt invalid", "numfmt --to=si abc", code=1,
          output="numfmt: invalid number: 'abc'"),
    ]),
    ("numfmt rounding", [
        c("numfmt from-zero", "numfmt --to=si 1234567 999999 1100001",
          output="1.3M\n1.0M\n1.2M"),
    ]),
    ("diff add/delete", [
        c("diff insert", "diff /mnt/diff_ins.txt /mnt/diff1.txt", code=1,
          output="1a2\n> b\n2a4\n> d"),
        c("diff delete", "diff /mnt/diff1.txt /mnt/diff_del.txt", code=1,
          output="4d3\n< d"),
    ]),
    ("errors", [
        c("base64 invalid", "base64 -d /mnt/invalid.b64", code=1,
          output="base64: invalid input"),
        c("rmdir usage", "rmdir", code=2, output="usage: rmdir [-p] DIR..."),
        c("sed usage", "sed", code=2, output="usage: sed [-n] SCRIPT [FILE...]"),
        c("awk usage", "awk", code=2,
          output="usage: awk [-F SEP] 'PROGRAM' [FILE...]"),
    ]),
    ("options usage", [
        c("base64 option", "base64 -z", code=2,
          output="usage: base64 [-d] [-w N] [FILE]"),
        c("tac missing", "tac /mnt/nope", code=1,
          contains="No such file or directory"),
        c("sum missing", "sum /mnt/nope", code=1,
          contains="No such file or directory"),
        c("shuf bad range", "shuf -i 5-1", code=2,
          contains="invalid range"),
    ]),
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
        (rootfs / name).write_bytes(data)
    for directory in ("d_empty", "d_nonempty", "p1/p2/p3"):
        (rootfs / directory).mkdir(parents=True, exist_ok=True)
    (rootfs / "d_nonempty" / "file.txt").write_bytes(b"x\n")
    if disk.exists():
        disk.unlink()
    subprocess.run(
        ["mke2fs", "-q", "-t", "ext2", "-F", "-b", "4096", "-d", str(rootfs), str(disk), "64M"],
        check=True,
    )


def boot(runner: pathlib.Path, kernel: pathlib.Path, disk: pathlib.Path,
         checks: list[Check], work: pathlib.Path, attempt: int, index: int) -> str:
    sock = work / f"serial-{index}-{attempt}.sock"
    monitor = work / f"monitor-{index}-{attempt}.sock"
    log = work / f"boot-{index}-{attempt}.log"
    keys = "\n".join(check.command for check in checks)
    if index == 1:
        keys = "cd /mnt\n" + keys
    keys += "\n"
    command = [
        sys.executable, str(runner),
        "--kernel", str(kernel),
        "--disk", str(disk),
        "--keys", keys,
        "--settle", "2.5",
        "--serial", str(sock),
        "--monitor", str(monitor),
        "--serial-log", str(log),
        "--shot", str(work / f"shot-{index}-{attempt}.png"),
    ]
    subprocess.run(command, check=False, stdout=subprocess.DEVNULL,
                   stderr=subprocess.DEVNULL, timeout=150)
    if not log.exists():
        return ""
    return log.read_text(encoding="utf-8", errors="replace")


def verify(log: str, checks: list[Check]) -> list[str]:
    records = {}
    for match in RECORD.finditer(log):
        records[match.group(1).strip()] = (match.group(2), int(match.group(3)))
    failures = []
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
        if check.contains is not None and check.contains not in output:
            failures.append(f"{check.command}: {check.contains!r} not in {output!r}")
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

    work = pathlib.Path(tempfile.mkdtemp(prefix="pagh-live-"))
    disk = work / "disk.img"
    rootfs = work / "rootfs"
    transcript = [f"# paghbox 1.3.0 live transcript ({len(CASES)} boots)", ""]
    passed = 0
    total = 0
    try:
        for index, (name, checks) in enumerate(CASES):
            total += len(checks)
            last_log = ""
            failures: list[str] = []
            for attempt in range(2):
                build_image(disk, rootfs)
                last_log = boot(args.runner, args.kernel, disk, checks, work, attempt, index)
                failures = verify(last_log, checks)
                if not failures:
                    break
            if failures:
                print(f"FAIL {name}:", file=sys.stderr)
                for failure in failures:
                    print(f"  {failure}", file=sys.stderr)
                print(last_log[-4000:], file=sys.stderr)
                return 1

            transcript.append(f"===== boot {index + 1}: {name} =====")
            for check in checks:
                match = next(
                    (m for m in RECORD.finditer(last_log)
                     if m.group(1).strip() == check.command),
                    None,
                )
                if match is not None:
                    output = match.group(2).replace("\r", "")
                    if output.endswith("\n"):
                        output = output[:-1]
                    transcript.append(f"{PROMPT} {check.command}")
                    if output:
                        transcript.append(output)
                    transcript.append(f"run: /mnt/paghbox exited with code {match.group(3)}")
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