#!/usr/bin/env python3
"""Live QEMU check for paghbox 1.11.0: the `nc` and `dd` applets plus the
1.8.0/1.10.0 regression suites on the current kernel.

This is the evidence harness behind `docs/live-1.11.0-transcript.txt`. 1.11.0
adds two applets that the ABI can support exactly:

* `dd` — block copy between regular files (and stdin/stdout) with `bs`, `count`,
  `skip`, `seek`, `conv=notrunc` and `status=`;
* `nc` — one TCP connection as a client or as a single-connection listener, with
  a `poll(2)` relay between the peer and stdin/stdout, and DNS resolution of a
  host name through the same one-`A` resolver `curl`/`wget` use.

The `ps`/`free`/`pgrep`/`df` applets are **not** here on purpose: the pagh-riscv
ABI (0.21.x) has no `/proc`, no `/proc/meminfo` and no `statfs`, so those tools
could only be faked. The PR body records the triage; this harness verifies what
shipped.

The suite reuses `tools/live_test_180.py` (1.8.0 fixtures and cases) and the
network cases of `tools/live_test_1100.py`, then adds:

* `dd` — copy with summary, block size/count/skip/status, and error paths;
* `nc` — usage, a TCP client against a host server on `10.0.2.2`, and a
  single-connection listener reached through QEMU's `hostfwd` (the host dialer
  retries until the guest listener is up).

Network cases need QEMU's user network and a host TCP/HTTP server; TLS cases are
not repeated here (1.11.0 changes no ABI path and the 1.10.0 transcript already
covers them). Everything else is local.

Usage::

    python3 tools/live_test_1110.py
    python3 tools/live_test_1110.py --new-only
    python3 tools/live_test_1110.py --kernel PATH --runner PATH --transcript PATH
"""

from __future__ import annotations

import argparse
import pathlib
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time

TOOLS = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))
import live_test_180 as base  # noqa: E402  (fixtures + the 1.8.0 cases)
import live_test_1100 as v1100  # noqa: E402  (1.10.0 cases + host helpers)

ROOT = TOOLS.parent
PAGHBOX = ROOT / "target" / "riscv64gc-unknown-none-elf" / "release" / "paghbox"
DEFAULT_KERNEL = v1100.DEFAULT_KERNEL
DEFAULT_RUNNER = v1100.DEFAULT_RUNNER
DEFAULT_TRANSCRIPT = ROOT / "docs" / "live-1.11.0-transcript.txt"
PROMPT = base.PROMPT
RUN = base.RUN

Check = v1100.Check


# ─── Host-side TCP peers ──────────────────────────────────────────────────


class HostTcpSender:
    """A host TCP server that answers exactly one connection with `payload`."""

    def __init__(self, payload: bytes) -> None:
        self.payload = payload
        self.port = v1100.free_port()
        self.sock: socket.socket | None = None
        self.thread: threading.Thread | None = None

    def __enter__(self):
        self.sock = socket.socket()
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        deadline = time.time() + 10
        while True:
            try:
                self.sock.bind(("127.0.0.1", self.port))
                break
            except OSError:
                if time.time() > deadline:
                    raise
                time.sleep(0.2)
        self.sock.listen(1)
        self.thread = threading.Thread(target=self._serve, daemon=True)
        self.thread.start()
        return self

    def _serve(self) -> None:
        assert self.sock is not None
        try:
            connection, _peer = self.sock.accept()
            with connection:
                # A peer that answers and closes in the same instant the
                # handshake completes is mishandled by the kernel's minimal TCP
                # client under QEMU user-net (`connect()` reports ETIMEDOUT even
                # though the host accepted; `curl` reproduces it, so it is not
                # this applet). A short pause before the payload/close is what a
                # real server does and keeps the case about `nc`, not about that
                # race.
                time.sleep(0.75)
                connection.sendall(self.payload)
        except OSError:
            pass

    def __exit__(self, *exc_info) -> None:
        if self.sock is not None:
            self.sock.close()
        if self.thread is not None:
            self.thread.join(timeout=2)


class HostTcpDialer:
    """A host TCP client that retries until a `hostfwd`-forwarded port answers.

    QEMU's user network accepts `hostfwd` connections only once the guest is up
    and the listener exists; before that the attempt is refused or reset, so the
    dialer retries instead of treating the first failure as a test failure.
    """

    def __init__(self, port: int, payload: bytes) -> None:
        self.port = port
        self.payload = payload
        self.thread: threading.Thread | None = None

    def __enter__(self):
        self.thread = threading.Thread(target=self._dial, daemon=True)
        self.thread.start()
        return self

    def _dial(self) -> None:
        deadline = time.time() + 120
        while time.time() < deadline:
            try:
                with socket.create_connection(("127.0.0.1", self.port), timeout=2) as conn:
                    conn.sendall(self.payload)
                    return
            except OSError:
                time.sleep(0.25)

    def __exit__(self, *exc_info) -> None:
        if self.thread is not None:
            self.thread.join(timeout=5)


# ─── New cases ────────────────────────────────────────────────────────────


def new_cases(host_tcp_port: int) -> list[tuple[str, list[Check]]]:
    """The 1.11.0 cases (host-dependent ones only when `host_tcp_port` is set)."""
    cases: list[tuple[str, list[Check]]] = [
        (
            "dd copy and summary",
            [
                Check("dd copy", "dd if=/mnt/abc.txt of=/mnt/ddcopy.txt",
                      output="0+1 records in\n0+1 records out"),
                Check("dd copy content", "cat /mnt/ddcopy.txt", output="abc"),
            ],
        ),
        (
            "dd block size",
            [
                Check("dd bs round trip", "dd if=/mnt/fox.txt of=/mnt/fox.copy bs=8",
                      output="5+1 records in\n5+1 records out"),
                Check("dd bs content", "cmp /mnt/fox.txt /mnt/fox.copy", output=""),
            ],
        ),
        (
            "dd notrunc",
            [
                Check("dd notrunc existing", "dd if=/mnt/abc.txt of=/mnt/fox.copy "
                      "bs=4 conv=notrunc status=none", output=""),
                Check("dd notrunc leaves tail", "cmp /mnt/fox.txt /mnt/fox.copy",
                      code=1, contains=("differ: byte 1",)),
            ],
        ),
        (
            "dd count skip and silent",
            [
                Check("dd count block", "dd if=/mnt/abc.txt bs=2 count=1 status=none",
                      output="ab"),
                Check("dd skip byte", "dd if=/mnt/abc.txt bs=1 skip=1 status=none",
                      output="bc"),
            ],
        ),
        (
            "dd errors",
            [
                Check("dd bad size", "dd bs=abc", code=2,
                      contains=("dd: invalid block size 'abc'",)),
                Check("dd missing input", "dd if=/mnt/nope", code=1,
                      contains=("dd: /mnt/nope:")),
                Check("dd unsupported conv", "dd conv=fsync", code=2,
                      contains=("conv=fsync: not supported by this ABI",)),
            ],
        ),
        (
            "nc usage",
            [
                Check("nc no args", "nc", code=2, contains=("usage: nc",)),
                Check("nc unsupported -u", "nc -u 10.0.2.2 80", code=2,
                      contains=("unsupported option '-u'",)),
                Check("nc client -p", "nc -p 1234 10.0.2.2 80", code=2,
                      contains=("usage: nc",)),
            ],
        ),
    ]
    if host_tcp_port is not None:
        cases.append(
            (
                "nc TCP client",
                [
                    Check("nc client", f"nc 10.0.2.2 {host_tcp_port}",
                          output="hello from host"),
                ],
            )
        )
    return cases


# ─── Runner (same shape as the 1.10.0 driver) ────────────────────────────


def boot(
    runner: pathlib.Path,
    kernel: pathlib.Path,
    disk: pathlib.Path,
    checks: list[Check],
    work: pathlib.Path,
    attempt: int,
    index: int,
    hostfwd: str | None = None,
) -> str:
    sock = work / f"serial-{index}-{attempt}.sock"
    monitor = work / f"monitor-{index}-{attempt}.sock"
    log = work / f"boot-{index}-{attempt}.log"
    commands = ["cd /mnt"] + [check.command for check in checks]
    keys = "\n".join(commands) + "\n"
    if len(keys.encode()) > 150:
        raise ValueError(
            f"boot {index}: {len(keys.encode())} bytes of typed input; the kernel "
            "console buffer needs a shorter group (see new_cases)"
        )
    command = [
        sys.executable, str(runner),
        "--kernel", str(kernel),
        "--disk", str(disk),
        "--net",
        "--keys", keys,
        "--settle", "6.0",
        "--timeout", "60",
        "--serial", str(sock),
        "--monitor", str(monitor),
        "--serial-log", str(log),
        "--shot", str(work / f"shot-{index}-{attempt}.png"),
    ]
    if hostfwd is not None:
        command += ["--hostfwd", hostfwd]
    try:
        subprocess.run(
            command, check=False, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, timeout=240,
        )
    except subprocess.TimeoutExpired:
        return ""
    if not log.exists():
        return ""
    return log.read_text(encoding="utf-8", errors="replace")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--kernel", type=pathlib.Path, default=DEFAULT_KERNEL)
    parser.add_argument("--runner", type=pathlib.Path, default=DEFAULT_RUNNER)
    parser.add_argument("--transcript", type=pathlib.Path, default=DEFAULT_TRANSCRIPT)
    parser.add_argument("--new-only", action="store_true",
                        help="run only the 1.11.0 cases (fast iteration)")
    args = parser.parse_args()

    if not PAGHBOX.is_file():
        print(f"live_test: missing {PAGHBOX}; run cargo build first", file=sys.stderr)
        return 2
    if not args.kernel.is_file() or not args.runner.is_file():
        print("live_test: kernel or run.py missing", file=sys.stderr)
        return 2

    work = pathlib.Path(tempfile.mkdtemp(prefix="pagh-live-1110-"))
    try:
        http_root = work / "http-root"
        http_root.mkdir()
        for name, data in v1100.HTTP_FIXTURES.items():
            (http_root / name).write_bytes(data)
        server = v1100.HostServer(http_root)

        tcp_sender = HostTcpSender(b"hello from host\n")
        listener_host_port = v1100.free_port()
        listener_guest_port = 7001
        listener_payload = b"hello from host\n"
        listener_command = f"nc -l -p {listener_guest_port}"
        hostfwd = f"{listener_host_port}:{listener_guest_port}"

        cases: list[tuple[str, list[Check]]] = []
        if not args.new_only:
            cases += base.CASES
            cases += v1100.new_cases(server.port, None)
        cases += new_cases(tcp_sender.port)
        cases.append(
            (
                "nc TCP listener (hostfwd)",
                [Check("nc listener", listener_command, output="hello from host")],
            )
        )

        # Boots that need a live host peer around them, keyed by case name.
        special: dict[str, tuple[object, str | None]] = {
            "nc TCP client": (tcp_sender, None),
            "nc TCP listener (hostfwd)": (
                HostTcpDialer(listener_host_port, listener_payload),
                hostfwd,
            ),
        }

        with server:
            return run_cases(args, work, cases, special)
    finally:
        shutil.rmtree(work, ignore_errors=True)


def run_cases(args, work: pathlib.Path, cases, special) -> int:
    disk = work / "disk.img"
    rootfs = work / "rootfs"
    transcript = [
        f"# paghbox 1.11.0 live transcript ({len(cases)} boots)",
        "# kernel: " + str(args.kernel),
        "# driver: python3 tools/live_test_1110.py",
        "",
    ]
    passed = 0
    total = 0
    for index, (name, checks) in enumerate(cases):
        total += len(checks)
        last_log = ""
        failures: list[str] = []
        for attempt in range(3):
            v1100.build_image(disk, rootfs)
            peer, hostfwd = special.get(name, (None, None))
            if peer is not None:
                peer.__enter__()
            try:
                last_log = boot(
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


if __name__ == "__main__":
    sys.exit(main())