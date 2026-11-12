#!/usr/bin/env python3
"""Live QEMU check for paghbox 1.13.0: `ps`, `free`, `df` on the procfs/statfs
kernel plus the 1.8.0–1.12.0 regression suites.

This is the evidence harness behind `docs/live-1.13.0-transcript.txt`. The three
applets added in 1.13.0 were honestly impossible in wave 1 (no `/proc`, no
meminfo, no statfs in the then-current ABI). Now they read real kernel state:

* `ps` — scans `/proc` for numeric pid directories, reads
  `/proc/<pid>/{stat,cmdline}`; TTY is `?` on purpose (no tty model in the ABI);
  `ps --uptime` prints `/proc/uptime` (port extension).
* `free` — formats `/proc/meminfo` (`MemTotal`/`MemFree`/`MemAvailable`);
  `shared`/`buff/cache`/`Swap` are honest zeros (no such counters/subsystem).
* `df` — `/proc/mounts` + the native `statfs(43)` syscall (96-byte `Statfs`,
  issued raw because libpagh 0.6.1 does not wrap it yet); `-h` human sizes.

The test kernel is **not** main: it is built from the `feat/statfs` worktree
(`/home/oleg/pagh-riscv-feat`, branch head `1e137d8`, version 0.24.0) which
stacks PR #9 (`/proc`), #10 (per-pid `/proc`) and #11 (`statfs`). Build it
read-only in a scratch directory:

    rsync -a --exclude=target --exclude=.git /home/oleg/pagh-riscv-feat/ /tmp/riscv-feat-src/
    (cd /tmp/riscv-feat-src && cargo build --locked --release)

`--legacy-kernel` (default: the last release kernel in `/home/oleg/pagh-riscv`)
runs one extra boot on the older 0.21.x kernel and checks the honest failure
messages: the applets must say "the kernel needs the procfs ABI" / "not
implemented" instead of pretending.

Usage::

    python3 tools/live_test_1130.py
    python3 tools/live_test_1130.py --new-only
    python3 tools/live_test_1130.py --kernel PATH --legacy-kernel PATH
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
import live_test_1120 as v1120  # noqa: E402

ROOT = TOOLS.parent
PAGHBOX = ROOT / "target" / "riscv64gc-unknown-none-elf" / "release" / "paghbox"
DEFAULT_KERNEL = pathlib.Path(
    "/tmp/pagh-night2/riscv-feat-src/target/riscv64gc-unknown-none-elf/release/pagh"
)
DEFAULT_LEGACY_KERNEL = pathlib.Path(
    "/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/release/pagh"
)
DEFAULT_RUNNER = v1100.DEFAULT_RUNNER
DEFAULT_TRANSCRIPT = ROOT / "docs" / "live-1.13.0-transcript.txt"
PROMPT = base.PROMPT

Check = v1100.Check


# ─── Validators for the tabular output ────────────────────────────────────


def _columns(line: str) -> list[str]:
    return line.split()


def validate_free(output: str) -> str | None:
    """`total`, `used`, `free`, `available` must be consistent for a 256 MiB VM."""
    lines = [line for line in output.splitlines() if line.split()]
    if len(lines) < 2:
        return f"free output has no Mem row: {output!r}"
    header, mem = lines[0], lines[1]
    values = _columns(mem)
    if len(values) != 7 or values[0] != "Mem:":
        return f"free Mem row is not 'Mem: total used free shared buff/cache available': {mem!r}"
    numbers = []
    for text in values[1:]:
        try:
            numbers.append(int(text))
        except ValueError:
            return f"free Mem value is not an integer: {text!r}"
    total, used, free, shared, cached, available = numbers
    if total != 262144:
        return f"MemTotal {total} != 262144 kB (256 MiB VM)"
    if used + free != total:
        return f"used({used}) + free({free}) != total({total})"
    if shared != 0 or cached != 0:
        return "shared/buff-cache must be 0: the kernel has no such counters"
    if available != free:
        return f"available({available}) != free({free}) (MemAvailable == MemFree)"
    return None


def validate_free_m(output: str) -> str | None:
    values = _columns([l for l in output.splitlines() if l.split()][1])
    if len(values) < 2 or values[1] != "256":
        return f"free -m MemTotal is not 256 MiB: {values!r}"
    return None


def validate_df(output: str) -> str | None:
    """Both mounts are listed: procfs with an honest zero capacity, ext2 64 MiB."""
    rows = [line.split() for line in output.splitlines()[1:] if line.split()]
    procfs = [row for row in rows if row and row[0] == "procfs"]
    ext2 = [row for row in rows if row and row[0] == "ext2"]
    if not procfs:
        return f"df has no procfs row: {output!r}"
    if not ext2:
        return f"df has no ext2 row (disk should be mounted): {output!r}"
    if procfs[0][1] != "0" or procfs[0][-1] != "/proc":
        return f"procfs row is not the honest zero row on /proc: {procfs[0]!r}"
    if ext2[0][1] != "65536":
        return f"ext2 1K-blocks {ext2[0][1]} != 65536 (64 MiB image)"
    if ext2[0][-1] != "/mnt":
        return f"ext2 row is not mounted on /mnt: {ext2[0]!r}"
    return None


def validate_ps_two_rows(output: str) -> str | None:
    """`timeout 5 paghbox ps` leaves two live processes for the scanner."""
    rows = [line.split() for line in output.splitlines()[1:] if line.split()]
    if len(rows) < 2:
        return f"ps shows fewer than two processes: {output!r}"
    commands = [" ".join(row[3:]) for row in rows]
    if not any("timeout 5" in command for command in commands):
        return f"ps does not show the parent timeout process: {commands!r}"
    if not any(command.endswith("paghbox ps") for command in commands):
        return f"ps does not show itself: {commands!r}"
    return None


# ─── New cases ────────────────────────────────────────────────────────────


def proc_cases() -> list[tuple[str, list[Check]]]:
    """The 1.13.0 cases: ps, free, df."""
    return [
        (
            "ps table",
            [
                Check("ps table", "ps",
                      contains=("  PID TTY      STAT COMMAND", "/mnt/paghbox ps")),
                Check("ps uptime", "ps --uptime",
                      pattern=r"uptime: \d+\.\d{2}s"),
            ],
        ),
        (
            "ps sees live processes",
            [
                Check("ps via timeout", "timeout 5 /mnt/paghbox ps",
                      validator=validate_ps_two_rows),
            ],
        ),
        (
            "ps usage",
            [
                Check("ps bad option", "ps -x", code=2, contains=("usage: ps",)),
            ],
        ),
        (
            "free table",
            [
                Check("free default", "free", validator=validate_free,
                      contains=("Mem:", "Swap:")),
                Check("free -m", "free -m", validator=validate_free_m),
                Check("free -h", "free -h", contains=("256.0M",)),
            ],
        ),
        (
            "free usage",
            [
                Check("free bad unit", "free -z", code=2,
                      contains=("usage: free",)),
            ],
        ),
        (
            "df mounts",
            [
                Check("df all", "df", validator=validate_df,
                      contains=("Filesystem", "Mounted on")),
                Check("df -h", "df -h", contains=("64.0M",)),
            ],
        ),
        (
            "df path and errors",
            [
                Check("df /proc", "df /proc", contains=("procfs", "/proc")),
                Check("df /mnt", "df /mnt", contains=("ext2", "65536", "/mnt")),
            ],
        ),
        (
            "df refusals",
            [
                Check("df synthetic root", "df /", code=1,
                      contains=("df: /: Operation not supported",)),
                Check("df missing path", "df /nope", code=1,
                      contains=("df: /nope: No such file or directory",)),
                Check("df bad option", "df -x", code=2, contains=("usage: df",)),
            ],
        ),
    ]


# ─── Runner (same shape as the 1.12.0 driver) ────────────────────────────


def run_cases(args, work: pathlib.Path, cases, special) -> int:
    disk = work / "disk.img"
    rootfs = work / "rootfs"
    transcript = [
        f"# paghbox 1.13.0 live transcript ({len(cases)} boots + 1 legacy boot)",
        "# kernel (procfs/statfs): " + str(args.kernel),
        "# legacy kernel (0.21.x, for the honest-refusal boot): " + str(args.legacy_kernel),
        "# driver: python3 tools/live_test_1130.py",
        "",
    ]
    passed = 0
    total = 0
    for index, (name, checks) in enumerate(cases):
        total += len(checks)
        last_log = ""
        failures: list[str] = []
        for attempt in range(3):
            v1120.build_image(disk, rootfs)
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

    if getattr(args, "no_legacy", False) or not args.legacy_kernel.is_file():
        print("note: legacy-kernel boot skipped", file=sys.stderr)
        args.transcript.parent.mkdir(parents=True, exist_ok=True)
        args.transcript.write_text("\n".join(transcript) + "\n", encoding="utf-8")
        print(f"LIVE SUMMARY: {passed}/{total} PASS")
        print(f"transcript: {args.transcript}")
        return 0

    # One boot on the legacy kernel: the three applets must refuse honestly
    # instead of printing a fabricated table.
    legacy_checks = [
        Check("ps without procfs", "ps", code=1,
              contains=("ps: /proc: No such file or directory",)),
        Check("free without meminfo", "free", code=1,
              contains=("free: /proc/meminfo: No such file or directory",)),
        Check("df without statfs", "df /mnt", code=1,
              contains=("df: /mnt: Function not implemented",)),
    ]
    total += len(legacy_checks)
    v1120.build_image(disk, rootfs)
    last_log = v1110.boot(
        args.runner, args.legacy_kernel, disk, legacy_checks, work, 0, len(cases),
    )
    failures = v1100.verify(last_log, legacy_checks)
    if failures:
        print("FAIL legacy-kernel refusals:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        print(last_log[-6000:], file=sys.stderr)
        return 1
    transcript.append(f"===== boot {len(cases) + 1}: legacy kernel refusals =====")
    found = v1100.records(last_log)
    for check in legacy_checks:
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
    parser.add_argument("--legacy-kernel", type=pathlib.Path, default=DEFAULT_LEGACY_KERNEL)
    parser.add_argument("--runner", type=pathlib.Path, default=DEFAULT_RUNNER)
    parser.add_argument("--transcript", type=pathlib.Path, default=DEFAULT_TRANSCRIPT)
    parser.add_argument("--new-only", action="store_true",
                        help="run only the 1.13.0 cases (fast iteration)")
    parser.add_argument("--no-legacy", action="store_true",
                        help="skip the legacy-kernel refusal boot")
    args = parser.parse_args()

    if not PAGHBOX.is_file():
        print(f"live_test: missing {PAGHBOX}; run cargo build first", file=sys.stderr)
        return 2
    if not args.kernel.is_file() or not args.runner.is_file():
        print("live_test: kernel or run.py missing", file=sys.stderr)
        return 2

    work = pathlib.Path(tempfile.mkdtemp(prefix="pagh-live-1130-"))
    try:
        http_root = work / "http-root"
        http_root.mkdir()
        for name, data in v1100.HTTP_FIXTURES.items():
            (http_root / name).write_bytes(data)
        server = v1100.HostServer(http_root)

        tcp_sender = v1110.HostTcpSender(b"hello from host\n")
        listener_host_port = v1100.free_port()
        listener_guest_port = 7001
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
            cases += v1120.text_cases()
        cases += proc_cases()

        listener_payload = b"hello from host\n"
        special: dict[str, tuple[object, str | None]] = {
            "nc TCP client": (tcp_sender, None),
            "nc TCP listener (hostfwd)": (
                v1110.HostTcpDialer(listener_host_port, listener_payload),
                hostfwd,
            ),
        }

        with server:
            result = run_cases(args, work, cases, special)
        return result
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())