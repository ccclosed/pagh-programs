#!/usr/bin/env python3
"""Live QEMU check for the paghbox 1.4.0 batch (82 applets).

This is the batch-5 companion to `tools/live_test.py`. It builds a 64 MiB ext2
image with the release `paghbox` and the fixtures, then boots the kernel's own
`tools/run.py` once per group of 2-4 commands. Each command's stdout/stderr and
the shell's `run: ... exited with code N` line are compared with the expected
values. Network groups pass `--net` and use a host `python3 -m http.server`
reached by the guest as `10.0.2.2:<port>`.

Usage:

    python3 tools/live_test_140.py
    python3 tools/live_test_140.py --kernel PATH --runner PATH --transcript PATH
"""

from __future__ import annotations

import argparse
import pathlib
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
PAGHBOX = ROOT / "target" / "riscv64gc-unknown-none-elf" / "release" / "paghbox"
DEFAULT_KERNEL = pathlib.Path(
    "/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/debug/pagh"
)
DEFAULT_RUNNER = pathlib.Path("/home/oleg/pagh-riscv/tools/run.py")
DEFAULT_TRANSCRIPT = ROOT / "docs" / "live-1.4.0-transcript.txt"
PROMPT = "pagh:rv64>"
RUN = "run /mnt/paghbox"
PORT_PLACEHOLDER = "{PORT}"

FIXTURES = {
    "hello.txt": b"hello\n",
    "split.txt": b"one\ntwo\nthree\nfour\nfive\n",
    "bytes.bin": b"abcdefghij\n",
    "csplit.txt": b"alpha\nbeta\nSECTION\ngamma\ndelta\n",
    "j1": b"a 1\nb 2\nd 4\n",
    "j2": b"a A\nc C\nd D\ne E\n",
    "j3": b"x:1\nb:2\nd:4\n",
    "j4": b"1:p\n3:q\n4:r\n",
    "fmt.txt": (
        b"The quick brown fox jumps over the lazy dog and keeps on running.\n"
        b"A short second line here.\n\n"
        b"A separate paragraph follows with a few words.\n"
    ),
    "tee_in.txt": b"tee input line\n",
    "tee_append.txt": b"prefix\n",
    "hello.hex": b"00000000: 6865 6c6c 6f0a                           hello.\n",
    "demo.txt": b"network ok\n",
}
SERVER_FILES = {
    "demo.txt": b"network ok\n",
    "index.html": b"index body\n",
}


def uuencode(data: bytes) -> bytes:
    """The same classic encoding as programs/src/applets/uu.rs."""
    lines: list[str] = []
    for start in range(0, len(data), 45):
        chunk = data[start : start + 45]
        if not chunk:
            continue
        line = chr(0x20 + len(chunk))
        for offset in range(0, len(chunk), 3):
            first = chunk[offset]
            second = chunk[offset + 1] if offset + 1 < len(chunk) else 0
            third = chunk[offset + 2] if offset + 2 < len(chunk) else 0
            value = (first << 16) | (second << 8) | third
            for shift in (18, 12, 6, 0):
                six = (value >> shift) & 0x3F
                line += "`" if six == 0 else chr(six + 0x20)
        lines.append(line)
    return ("\n".join(lines) + "\n" if lines else "").encode()


FIXTURES["hello.uu"] = (
    b"begin 644 hello.txt\n" + uuencode(FIXTURES["hello.txt"]) + b"end\n"
)


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


CASES: list[tuple[str, list[Check], bool]] = [
    (
        "identity",
        [
            c("whoami", "whoami", output="root"),
            c("id", "id", output="uid=0 gid=0(root) groups=0(root)"),
            c("id -u", "id -u", output="0"),
            c("uname -a", "uname -a", contains=("0.16.1",)),
        ],
        False,
    ),
    (
        "identity options",
        [
            c("id -g", "id -g", output="0"),
            c("id -G", "id -G", output="0"),
            c("id -u -g", "id -u -g", output="0 0"),
        ],
        False,
    ),
    (
        "environment",
        [
            c("env", "env", output="PWD=/mnt"),
            c("env operand", "env extra", code=2, output="usage: env"),
            c("env option", "env -i", code=2, output="usage: env"),
        ],
        False,
    ),
    (
        "tee multiple",
        [
            c("tee files", "tee -i /mnt/tee_in.txt /mnt/tee_out.txt /mnt/tee_out2.txt",
              output="tee input line"),
            c("tee out1", "cat /mnt/tee_out.txt", output="tee input line"),
            c("tee out2", "cat /mnt/tee_out2.txt", output="tee input line"),
        ],
        False,
    ),
    (
        "tee append",
        [
            c("tee -a", "tee -a -i /mnt/tee_in.txt /mnt/tee_append.txt",
              output="tee input line"),
            c("tee -a cat", "cat /mnt/tee_append.txt",
              output="prefix\ntee input line"),
        ],
        False,
    ),
    (
        "split lines",
        [
            c("split -l", "split -l 2 /mnt/split.txt /mnt/sp", output=""),
            c("split aa", "cat /mnt/spaa", output="one\ntwo"),
            c("split ab", "cat /mnt/spab", output="three\nfour"),
            c("split ac", "cat /mnt/spac", output="five"),
        ],
        False,
    ),
    (
        "split bytes",
        [
            c("split -b -d", "split -b 4 -d /mnt/bytes.bin /mnt/b", output=""),
            c("split b00", "xxd /mnt/b00",
              output="00000000: 6162 6364                                abcd"),
            c("split b01", "xxd /mnt/b01",
              output="00000000: 6566 6768                                efgh"),
            c("split b02", "xxd /mnt/b02",
              output="00000000: 696a 0a                                  ij."),
        ],
        False,
    ),
    (
        "csplit regex",
        [
            c("csplit /RE/ -s", "csplit -s -f /mnt/cs /mnt/csplit.txt /SECTION/",
              output=""),
            c("csplit cs00", "cat /mnt/cs00", output="alpha\nbeta"),
            c("csplit cs01", "cat /mnt/cs01", output="SECTION\ngamma\ndelta"),
        ],
        False,
    ),
    (
        "csplit line -n",
        [
            c("csplit 3 -n3", "csplit -f /mnt/cn -n 3 /mnt/csplit.txt 3",
              output="11\n20"),
            c("csplit cn000", "cat /mnt/cn000", output="alpha\nbeta"),
            c("csplit cn001", "cat /mnt/cn001", output="SECTION\ngamma\ndelta"),
        ],
        False,
    ),
    (
        "join",
        [
            c("join default", "join /mnt/j1 /mnt/j2", output="a 1 A\nd 4 D"),
            c("join -t -1 -2", "join -1 2 -2 1 -t : /mnt/j3 /mnt/j4",
              output="1:x:p\n4:d:r"),
            c("join missing", "join /mnt/j1 /mnt/nope", code=1,
              contains=("No such file or directory",)),
        ],
        False,
    ),
    (
        "fmt",
        [
            c("fmt -w20", "fmt -w 20 /mnt/fmt.txt",
              output=(
                  "The quick brown fox\njumps over the lazy\ndog and keeps on\n"
                  "running. A short\nsecond line here.\n\n"
                  "A separate paragraph\nfollows with a few\nwords."
              )),
            c("fmt -s -w20", "fmt -s -w 20 /mnt/fmt.txt",
              output=(
                  "The quick brown fox\njumps over the lazy\ndog and keeps on\n"
                  "running.\nA short second line\nhere.\n\n"
                  "A separate paragraph\nfollows with a few\nwords."
              )),
            c("fmt bad width", "fmt -w 0 /mnt/fmt.txt", code=2,
              output="fmt: invalid width: '0'"),
        ],
        False,
    ),
    (
        "od",
        [
            c("od x x1", "od -A x -t x1 /mnt/hello.txt",
              output="000000 68 65 6c 6c 6f 0a\n000006"),
            c("od d c", "od -A d -t c /mnt/hello.txt",
              output="0000000   h   e   l   l   o  \\n\n0000006"),
            c("od o o1", "od -A o -t o1 /mnt/hello.txt",
              output="0000000 150 145 154 154 157 012\n0000006"),
        ],
        False,
    ),
    (
        "xxd",
        [
            c("xxd dump", "xxd /mnt/hello.txt",
              output="00000000: 6865 6c6c 6f0a                           hello."),
            c("xxd -l -s", "xxd -l 4 -s 1 /mnt/hello.txt",
              output="00000001: 656c 6c6f                                ello"),
            c("xxd -r", "xxd -r /mnt/hello.hex", output="hello"),
        ],
        False,
    ),
    (
        "uuencode/uudecode",
        [
            c("uuencode", "uuencode /mnt/hello.txt hello.txt",
              output="begin 644 hello.txt\n&:&5L;&\\*\nend"),
            c("uudecode -o", "uudecode -o /mnt/udec.txt /mnt/hello.uu", output=""),
            c("udec cat", "cat /mnt/udec.txt", output="hello"),
            c("uudecode stdout", "uudecode -o - /mnt/hello.uu", output="hello"),
        ],
        False,
    ),
    (
        "kill signals",
        [
            c("kill -0 self x2", "kill -0 0 0", output=""),
            c("kill -0 missing", "kill -0 999999", code=1,
              output="kill: 999999: No such process"),
            c("kill bad signal", "kill -BOGUS 0", code=2,
              output="kill: invalid signal: 'BOGUS'"),
            c("kill -TERM missing", "kill -TERM 999999", code=1,
              output="kill: 999999: No such process"),
        ],
        False,
    ),
    (
        "usage identity/files",
        [
            c("whoami usage", "whoami x", code=2, output="usage: whoami"),
            c("id usage", "id x", code=2, output="usage: id [-u] [-g] [-G]"),
            c("tee usage", "tee -z", code=2,
              output="usage: tee [-a] [-i FILE] [FILE...]"),
            c("split usage", "split -z", code=2,
              output="usage: split [-l N] [-b N] [-d] [FILE [PREFIX]]"),
        ],
        False,
    ),
    (
        "usage text",
        [
            c("csplit usage", "csplit", code=2,
              output="usage: csplit [-s] [-f PREFIX] [-n N] FILE PATTERN..."),
            c("join usage", "join", code=2,
              output="usage: join [-1 N] [-2 N] [-t CHAR] FILE1 FILE2"),
            c("od usage", "od -A q", code=2,
              output="od: invalid offset radix: 'q'"),
            c("uuencode usage", "uuencode", code=2,
              output="usage: uuencode [FILE] NAME"),
        ],
        False,
    ),
    (
        "usage codecs/network",
        [
            c("uudecode usage", "uudecode -z", code=2,
              output="usage: uudecode [-o FILE] [FILE]"),
            c("xxd usage", "xxd -z", code=2,
              output="usage: xxd [-r] [-l N] [-s OFF] [FILE]"),
            c("curl bad scheme", "curl https://10.0.2.2/x", code=2,
              output="curl: unsupported URL: 'https://10.0.2.2/x'"),
            c("wget hostname", "wget http://example.com/x", code=2,
              output="wget: unsupported URL: 'http://example.com/x'"),
        ],
        False,
    ),
    (
        "curl network",
        [
            c("curl body", f"curl http://10.0.2.2:{PORT_PLACEHOLDER}/demo.txt",
              output="network ok"),
            c("curl -i", f"curl -i http://10.0.2.2:{PORT_PLACEHOLDER}/demo.txt",
              contains=("HTTP/1.0 200 OK", "Content-type: text/plain",
                        "network ok")),
            c("curl -o", f"curl -o /mnt/c.out http://10.0.2.2:{PORT_PLACEHOLDER}/demo.txt",
              output=""),
            c("curl file", "cat /mnt/c.out", output="network ok"),
        ],
        True,
    ),
    (
        "wget network",
        [
            c("wget default", f"wget http://10.0.2.2:{PORT_PLACEHOLDER}/demo.txt",
              output="wget: saved demo.txt (11 bytes)"),
            c("wget file", "cat /mnt/demo.txt", output="network ok"),
            c("wget -O", f"wget -O /mnt/w.out http://10.0.2.2:{PORT_PLACEHOLDER}/demo.txt",
              output="wget: saved /mnt/w.out (11 bytes)"),
            c("wget -O file", "cat /mnt/w.out", output="network ok"),
        ],
        True,
    ),
    (
        "network errors",
        [
            c("curl 404", f"curl http://10.0.2.2:{PORT_PLACEHOLDER}/missing.txt",
              code=1, output="curl: HTTP 404 File not found"),
            c("wget 404", f"wget http://10.0.2.2:{PORT_PLACEHOLDER}/missing.txt",
              code=1, output="wget: HTTP 404 File not found"),
            c("curl refused", "curl http://10.0.2.2:1/x", code=1,
              output="curl: connect: Connection refused"),
        ],
        True,
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
        (rootfs / name).write_bytes(data)
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
    port: int,
    net: bool,
) -> str:
    sock = work / f"serial-{index}-{attempt}.sock"
    monitor = work / f"monitor-{index}-{attempt}.sock"
    log = work / f"boot-{index}-{attempt}.log"
    commands = [check.command.replace(PORT_PLACEHOLDER, str(port)) for check in checks]
    keys = "\n".join(commands)
    if index == 0:
        keys = "cd /mnt\n" + keys
    else:
        keys = "cd /mnt\n" + keys
    keys += "\n"
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
    if net:
        command.insert(command.index("--disk"), "--net")
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


def verify(log: str, checks: list[Check], port: int) -> list[str]:
    records: dict[str, tuple[str, int]] = {}
    for match in RECORD.finditer(log):
        records[match.group(1).strip()] = (match.group(2), int(match.group(3)))
    failures: list[str] = []
    for check in checks:
        command = check.command.replace(PORT_PLACEHOLDER, str(port))
        record = records.get(command)
        if record is None:
            failures.append(f"no record for {command!r}")
            continue
        output, code = record
        output = output.replace("\r", "")
        if output.endswith("\n"):
            output = output[:-1]
        if code != check.code:
            failures.append(f"{command}: code {code} != {check.code}")
        if check.output is not None and output != check.output:
            failures.append(f"{command}: output {output!r} != {check.output!r}")
        for needle in check.contains:
            if needle not in output:
                failures.append(f"{command}: {needle!r} not in {output!r}")
    return failures


def start_server(work: pathlib.Path) -> tuple[subprocess.Popen, int]:
    www = work / "www"
    www.mkdir()
    for name, data in SERVER_FILES.items():
        (www / name).write_bytes(data)
    probe = socket.socket()
    probe.bind(("127.0.0.1", 0))
    port = probe.getsockname()[1]
    probe.close()
    server = subprocess.Popen(
        [
            sys.executable,
            "-m",
            "http.server",
            str(port),
            "--bind",
            "127.0.0.1",
            "--directory",
            str(www),
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(0.6)
    return server, port


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

    work = pathlib.Path(tempfile.mkdtemp(prefix="pagh-live-140-"))
    disk = work / "disk.img"
    rootfs = work / "rootfs"
    server = None
    port = 0
    network = any(net for _, _, net in CASES)
    if network:
        server, port = start_server(work)

    transcript = [f"# paghbox 1.4.0 live transcript ({len(CASES)} boots)", ""]
    passed = 0
    total = 0
    try:
        for index, (name, checks, net) in enumerate(CASES):
            total += len(checks)
            last_log = ""
            failures: list[str] = []
            for attempt in range(2):
                build_image(disk, rootfs)
                last_log = boot(
                    args.runner, args.kernel, disk, checks, work, attempt, index, port, net
                )
                failures = verify(last_log, checks, port)
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
                command = check.command.replace(PORT_PLACEHOLDER, str(port))
                match = next(
                    (
                        m
                        for m in RECORD.finditer(last_log)
                        if m.group(1).strip() == command
                    ),
                    None,
                )
                if match is not None:
                    output = match.group(2).replace("\r", "")
                    if output.endswith("\n"):
                        output = output[:-1]
                    transcript.append(f"{PROMPT} {command}")
                    if output:
                        transcript.append(output)
                    transcript.append(
                        f"run: /mnt/paghbox exited with code {match.group(3)}"
                    )
                print(f"PASS {check.label}: {command}")
                passed += 1
            transcript.append("")
    finally:
        if server is not None:
            server.terminate()
            server.wait()
        shutil.rmtree(work, ignore_errors=True)

    args.transcript.parent.mkdir(parents=True, exist_ok=True)
    args.transcript.write_text("\n".join(transcript) + "\n", encoding="utf-8")
    print(f"LIVE SUMMARY: {passed}/{total} PASS")
    print(f"transcript: {args.transcript}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())