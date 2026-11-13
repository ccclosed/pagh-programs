#!/usr/bin/env python3
"""Live QEMU check for paghbox 1.12.0: `dos2unix`, `unix2dos`, `iconv`, `printenv`
plus the 1.8.0/1.10.0/1.11.0 regression suites on the current kernel.

This is the evidence harness behind `docs/live-1.12.0-transcript.txt`. 1.12.0
adds four applets that the ABI can support exactly — all of them pure
file/character/argument work, no `/proc` needed:

* `dos2unix` / `unix2dos` — in-place CRLF <-> LF conversion (whole-file buffer;
  a lone CR is preserved, `unix2dos` is idempotent on an existing CRLF file);
* `iconv` — a documented fail-closed subset of character-set conversion:
  UTF-8, ASCII, ISO-8859-1, UTF-16LE, UTF-16BE; invalid input, unpaired
  surrogates and unrepresentable characters are errors, never substitutions;
* `printenv` — `envp` listing and `printenv NAME` with GNU's exit 1 for a
  missing name (the environment is exactly PWD on this kernel, no guesswork).

The suite reuses `tools/live_test_180.py` (1.8.0), the network cases of
`tools/live_test_1100.py` and the `dd`/`nc` cases of `tools/live_test_1110.py`,
then adds the four new applets. Network cases need QEMU's user network and host
TCP/HTTP servers; TLS cases are not repeated (1.12.0 changes no ABI path and the
1.10.0 transcript covers them).

Usage::

    python3 tools/live_test_1120.py
    python3 tools/live_test_1120.py --new-only
    python3 tools/live_test_1120.py --kernel PATH --runner PATH --transcript PATH
"""

from __future__ import annotations

import argparse
import pathlib
import shutil
import subprocess
import sys
import tempfile

TOOLS = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))
import live_test_180 as base  # noqa: E402
import live_test_1100 as v1100  # noqa: E402
import live_test_1110 as v1110  # noqa: E402

ROOT = TOOLS.parent
PAGHBOX = ROOT / "target" / "riscv64gc-unknown-none-elf" / "release" / "paghbox"
DEFAULT_KERNEL = v1100.DEFAULT_KERNEL
DEFAULT_RUNNER = v1100.DEFAULT_RUNNER
DEFAULT_TRANSCRIPT = ROOT / "docs" / "live-1.12.0-transcript.txt"
PROMPT = base.PROMPT

Check = v1100.Check

# Fixtures for the 1.12.0 applets. The expectations are computed on the host
# from the same bytes, so the guest conversion is compared with an independent
# result (`cmp`), not with a hand-typed string.
CRLF_TEXT = b"one\r\ntwo\r\nthree\r\n"
LONE_CR_TEXT = b"a\rb\r\n"
LONE_CR_EXPECTED = b"a\rb\n"
LF_TEXT = b"a\nb\n"
LF_DOS_EXPECTED = b"a\r\nb\r\n"
UTF8_CAFE = "caf\u00e9\n".encode("utf-8")
LATIN1_CAFE = "caf\u00e9\n".encode("iso-8859-1")
UTF16LE_CAFE = "caf\u00e9\n".encode("utf-16-le")
UTF16BE_CAFE = "caf\u00e9\n".encode("utf-16-be")

TEXT_FIXTURES: dict[str, bytes] = {
    "crlf.txt": CRLF_TEXT,
    "lonecr.txt": LONE_CR_TEXT,
    "lonecr.expected": LONE_CR_EXPECTED,
    "lf.txt": LF_TEXT,
    "lf.dos.expected": LF_DOS_EXPECTED,
    "latin1.txt": LATIN1_CAFE,
    "utf8.txt": UTF8_CAFE,
    "utf16le.bin": UTF16LE_CAFE,
    "utf16be.bin": UTF16BE_CAFE,
}


def build_image(disk: pathlib.Path, rootfs: pathlib.Path) -> None:
    """The shared 1.8.0/1.10.0 fixtures plus the 1.12.0 conversion fixtures."""
    if rootfs.exists():
        shutil.rmtree(rootfs)
    rootfs.mkdir(parents=True)
    shutil.copy2(PAGHBOX, rootfs / "paghbox")
    for fixtures in (base.FIXTURES, v1100.HTTP_FIXTURES, TEXT_FIXTURES):
        for name, data in fixtures.items():
            (rootfs / name).write_bytes(data)
    if disk.exists():
        disk.unlink()
    subprocess.run(
        ["mke2fs", "-q", "-t", "ext2", "-F", "-b", "4096", "-d", str(rootfs),
         str(disk), "64M"],
        check=True,
    )


def text_cases() -> list[tuple[str, list[Check]]]:
    """The 1.12.0 cases: the four new applets.

    Conversion results are compared **byte-for-byte** with host-computed
    fixtures (`cmp`), never printed: the console transcript is decoded chunk by
    chunk by the runner, so a multi-byte UTF-8 character split across two reads
    can become U+FFFD in the log even though the guest wrote correct bytes. The
    only visible iconv output here is ASCII (`hello.txt` -> `hello`).
    """
    return [
        (
            "printenv listing",
            [
                Check("printenv all", "printenv", output="PWD=/mnt"),
                Check("printenv one", "printenv PWD", output="/mnt"),
                Check("printenv missing", "printenv NOPE", output="", code=1),
            ],
        ),
        (
            "printenv operands",
            [
                Check("printenv found and missing", "printenv PWD NOPE",
                      output="/mnt", code=1),
                Check("printenv --", "printenv -- PWD", output="/mnt"),
                Check("printenv bad option", "printenv -x", code=2,
                      contains=("usage: printenv",)),
            ],
        ),
        (
            "dos2unix in place",
            [
                Check("dos2unix converts", "dos2unix /mnt/crlf.txt", output=""),
                Check("dos2unix content", "cat /mnt/crlf.txt",
                      output="one\ntwo\nthree"),
            ],
        ),
        (
            "dos2unix keeps a lone CR",
            [
                Check("dos2unix lone cr", "dos2unix /mnt/lonecr.txt", output=""),
                Check("dos2unix lone cr content",
                      "cmp /mnt/lonecr.txt /mnt/lonecr.expected", output=""),
            ],
        ),
        (
            "unix2dos in place",
            [
                Check("unix2dos converts", "unix2dos /mnt/lf.txt", output=""),
                Check("unix2dos idempotent", "unix2dos /mnt/lf.txt", output=""),
                Check("unix2dos content", "cmp /mnt/lf.txt /mnt/lf.dos.expected",
                      output=""),
            ],
        ),
        (
            "iconv ascii and latin1",
            [
                Check("iconv ascii utf8",
                      "iconv -f ascii -t utf8 /mnt/hello.txt", output="hello"),
                Check("iconv latin1 utf8 to file",
                      "iconv -f latin1 -t utf8 -o /mnt/u8out.txt /mnt/latin1.txt",
                      output=""),
            ],
        ),
        (
            "iconv latin1 round trip",
            [
                Check("iconv latin1 utf8 to file",
                      "iconv -f latin1 -t utf8 -o /mnt/u8out.txt /mnt/latin1.txt",
                      output=""),
                Check("iconv latin1 bytes",
                      "cmp /mnt/u8out.txt /mnt/utf8.txt", output=""),
            ],
        ),
        (
            "iconv utf8 to latin1",
            [
                Check("iconv utf8 to latin1",
                      "iconv -f utf8 -t latin1 -o /mnt/lat2.txt /mnt/utf8.txt",
                      output=""),
                Check("iconv latin1 bytes back",
                      "cmp /mnt/latin1.txt /mnt/lat2.txt", output=""),
            ],
        ),
        (
            "iconv utf8 to utf16le",
            [
                Check("iconv utf8 to utf16le",
                      "iconv -f utf8 -t utf16le -o /mnt/u8le.txt /mnt/utf8.txt",
                      output=""),
                Check("iconv utf16le bytes",
                      "cmp /mnt/u8le.txt /mnt/utf16le.bin", output=""),
            ],
        ),
        (
            "iconv utf16be to utf8",
            [
                Check("iconv utf16be to utf8",
                      "iconv -f utf16be -t utf8 -o /mnt/u8be.txt /mnt/utf16be.bin",
                      output=""),
                Check("iconv utf16be bytes",
                      "cmp /mnt/u8be.txt /mnt/utf8.txt", output=""),
            ],
        ),
        (
            "iconv utf16le to utf8",
            [
                Check("iconv utf16le to utf8",
                      "iconv -f utf16le -t utf8 -o /mnt/u8le2.txt /mnt/utf16le.bin",
                      output=""),
                Check("iconv utf16le bytes back",
                      "cmp /mnt/u8le2.txt /mnt/utf8.txt", output=""),
            ],
        ),
        (
            "iconv fail-closed",
            [
                Check("iconv invalid utf8", "iconv -f utf8 -t ascii /mnt/latin1.txt",
                      code=1, contains=("iconv: invalid input for UTF-8 at byte 3",)),
                Check("iconv unrepresentable",
                      "iconv -f utf8 -t ascii /mnt/utf8.txt", code=1,
                      contains=("iconv: cannot convert U+00E9 to ASCII",)),
            ],
        ),
        (
            "iconv usage and list",
            [
                Check("iconv no options", "iconv", code=2,
                      contains=("iconv: -f FROM and -t TO are required",)),
                Check("iconv bad charset",
                      "iconv -f klingon -t utf8 /mnt/utf8.txt", code=2,
                      contains=("iconv: unsupported charset 'klingon'",)),
                Check("iconv -l", "iconv -l",
                      output="UTF-8\nASCII\nISO-8859-1\nUTF-16LE\nUTF-16BE"),
            ],
        ),
    ]


def run_cases(args, work: pathlib.Path, cases, special) -> int:
    disk = work / "disk.img"
    rootfs = work / "rootfs"
    transcript = [
        f"# paghbox 1.12.0 live transcript ({len(cases)} boots)",
        "# kernel: " + str(args.kernel),
        "# driver: python3 tools/live_test_1120.py",
        "",
    ]
    passed = 0
    total = 0
    for index, (name, checks) in enumerate(cases):
        total += len(checks)
        last_log = ""
        failures: list[str] = []
        for attempt in range(3):
            build_image(disk, rootfs)
            peer, hostfwd = special.get(name, (None, None))
            if peer is not None:
                peer.__enter__()
            try:
                last_log = v1110.boot(
                    args.runner, args.kernel, disk, checks, work, attempt, index,
                    hostfwd=hostfwd,
                )
            finally:
                if peer is not None:
                    peer.__exit__(None, None, None)
            failures = v1100.verify(last_log, checks)
            if not failures:
                break
        if failures:
            print(f"FAIL {name}:", file=sys.stderr)
            for failure in failures:
                print(f"  {failure}", file=sys.stderr)
            print(last_log[-6000:], file=sys.stderr)
            return 1

        transcript.append(f"===== boot {index + 1}: {name} =====")
        found = v1100.records(last_log)
        for check in checks:
            output, code = found[check.command]
            output = output.replace("\r", "")
            if output.endswith("\n"):
                output = output[:-1]
            transcript.append(f"{PROMPT} {check.command}")
            if output:
                transcript.append(output)
            transcript.append(f"run: /mnt/paghbox exited with code {code}")
            print(f"PASS {check.label}: {check.command}")
            passed += 1
        transcript.append("")

    args.transcript.parent.mkdir(parents=True, exist_ok=True)
    args.transcript.write_text("\n".join(transcript) + "\n", encoding="utf-8")
    print(f"LIVE SUMMARY: {passed}/{total} PASS")
    print(f"transcript: {args.transcript}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--kernel", type=pathlib.Path, default=DEFAULT_KERNEL)
    parser.add_argument("--runner", type=pathlib.Path, default=DEFAULT_RUNNER)
    parser.add_argument("--transcript", type=pathlib.Path, default=DEFAULT_TRANSCRIPT)
    parser.add_argument("--new-only", action="store_true",
                        help="run only the 1.12.0 cases (fast iteration)")
    args = parser.parse_args()

    if not PAGHBOX.is_file():
        print(f"live_test: missing {PAGHBOX}; run cargo build first", file=sys.stderr)
        return 2
    if not args.kernel.is_file() or not args.runner.is_file():
        print("live_test: kernel or run.py missing", file=sys.stderr)
        return 2

    work = pathlib.Path(tempfile.mkdtemp(prefix="pagh-live-1120-"))
    try:
        http_root = work / "http-root"
        http_root.mkdir()
        for name, data in v1100.HTTP_FIXTURES.items():
            (http_root / name).write_bytes(data)
        server = v1100.HostServer(http_root)

        tcp_sender = v1110.HostTcpSender(b"hello from host\n")
        listener_host_port = v1100.free_port()
        listener_guest_port = 7001
        listener_payload = b"hello from host\n"
        hostfwd = f"{listener_host_port}:{listener_guest_port}"

        cases: list[tuple[str, list[Check]]] = []
        if not args.new_only:
            cases += base.CASES
            cases += v1100.new_cases(server.port, None)
            cases += v1110.new_cases(tcp_sender.port)
            cases.append(
                (
                    "nc TCP client",
                    [Check("nc client", f"nc 10.0.2.2 {tcp_sender.port}",
                           output="hello from host")],
                )
            )
            cases.append(
                (
                    "nc TCP listener (hostfwd)",
                    [Check("nc listener", f"nc -l -p {listener_guest_port}",
                           output="hello from host")],
                )
            )
        cases += text_cases()

        # Boots that need a live host peer around them, keyed by case name.
        special: dict[str, tuple[object, str | None]] = {
            "nc TCP client": (tcp_sender, None),
            "nc TCP listener (hostfwd)": (
                v1110.HostTcpDialer(listener_host_port, listener_payload),
                hostfwd,
            ),
        }

        with server:
            return run_cases(args, work, cases, special)
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())