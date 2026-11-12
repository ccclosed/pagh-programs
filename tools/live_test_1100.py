#!/usr/bin/env python3
"""Live QEMU check for paghbox 1.10.0: ABI 0.21.0 applets plus the 1.8.0 suite.

This is the evidence harness behind `docs/live-1.10.0-transcript.txt`. It reuses
the 1.8.0 batch fixtures and cases (`tools/live_test_180.py`) as a regression
suite on the current kernel, then adds the applets that need the newer ABI:

* `threads` — the 1.9.0 demo, never part of the 1.8 transcript;
* `uuidgen` — version-4 UUIDs from the kernel entropy pool (`getrandom`);
* `dns` — one `A` lookup over the ABI's UDP socket (QEMU resolver 10.0.2.3);
* `curl`/`wget` — `https://` through kernel-verified TLS 1.3, plain `http://`
  against a host server on 10.0.2.2, DNS names, and the fail-closed paths
  (unresolvable name, `ftp://` URL, local self-signed cert rejected).

Network cases need QEMU's user network (`tools/run.py --net`) and a host HTTP
server; the TLS cases need host internet access. Everything else is local. The
guest frames are checked exactly like the earlier suites: stdout/stderr, the
`run: ... exited with code N` marker and, where the data is fixed, exact text.

Usage::

    python3 tools/live_test_1100.py
    python3 tools/live_test_1100.py --kernel PATH --runner PATH --transcript PATH
"""

from __future__ import annotations

import argparse
import functools
import http.server
import pathlib
import re
import shutil
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time

TOOLS = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))
import live_test_180 as base  # noqa: E402  (fixtures + the 1.8.0 case list)

ROOT = TOOLS.parent
PAGHBOX = ROOT / "target" / "riscv64gc-unknown-none-elf" / "release" / "paghbox"
DEFAULT_KERNEL = pathlib.Path(
    "/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/release/pagh"
)
DEFAULT_RUNNER = pathlib.Path("/home/oleg/pagh-riscv/tools/run.py")
DEFAULT_TRANSCRIPT = ROOT / "docs" / "live-1.10.0-transcript.txt"
PROMPT = base.PROMPT
RUN = base.RUN
UUID = base.UUID

HTTP_FIXTURES = {
    # Short names keep one boot's typed input under the kernel console's
    # receive buffer; the groups below are sized for that limit.
    "h": b"hello\n",
    "fox.txt": base.FOX,
}


# ─── Checks ───────────────────────────────────────────────────────────────


class Check(base.Check):
    """A 1.8.0-style check plus an optional Python-side validator."""

    def __init__(self, label: str, args: str, *, validator=None, **kwargs):
        super().__init__(label, args, **kwargs)
        self.validator = validator


def distinct_uuids(output: str) -> str | None:
    lines = output.splitlines()
    if len(lines) != len(set(lines)):
        return f"repeated UUID in {lines!r}"
    return None


def same_multiset(expected: list[str]):
    def validate(output: str) -> str | None:
        got = output.splitlines()
        if sorted(got) != sorted(expected):
            return f"lines {got!r} are not a permutation of {expected!r}"
        return None

    return validate


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


class HostHandler(http.server.SimpleHTTPRequestHandler):
    """Static files plus `/chunked`, a deliberate HTTP/1.1 chunked response.

    The applet sends HTTP/1.0, where the body is normally close-delimited; the
    client also decodes `Transfer-Encoding: chunked` because a peer may ignore
    the version. This endpoint is the only way to exercise that decoder without
    relying on an external server's framing.
    """

    def do_GET(self) -> None:
        if self.path.split("?")[0] != "/chunked":
            super().do_GET()
            return
        body = b"3\r\nhel\r\n3\r\nlo\n\r\n0\r\n\r\n"
        self.protocol_version = "HTTP/1.1"
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()
        self.wfile.write(body)
        self.close_connection = True

    def log_message(self, *args, **kwargs) -> None:
        pass


class HostServer:
    """A plain HTTP server on 127.0.0.1, reachable from the guest as 10.0.2.2."""

    def __init__(self, directory: pathlib.Path) -> None:
        self.port = free_port()
        handler = functools.partial(HostHandler, directory=str(directory))
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", self.port), handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *exc_info) -> None:
        self.httpd.shutdown()
        self.httpd.server_close()


class UntrustedTlsServer:
    """`openssl s_server` with a self-signed certificate for example.com.

    A socket the guest can reach, a name the certificate claims, and no chain to
    the kernel's committed CA bundle: the handshake must be refused. This is the
    applet-level negative twin of the kernel's chain gate test.
    """

    def __init__(self, work: pathlib.Path) -> None:
        self.work = work
        self.port = free_port()
        self.cert = work / "selfsigned.pem"
        self.key = work / "selfsigned.key"
        self.process: subprocess.Popen | None = None

    def __enter__(self):
        subprocess.run(
            [
                "openssl", "req", "-x509", "-newkey", "ec",
                "-pkeyopt", "ec_paramgen_curve:prime256v1",
                "-keyout", str(self.key), "-out", str(self.cert),
                "-days", "2", "-nodes",
                "-subj", "/CN=example.com",
                "-addext", "subjectAltName=DNS:example.com",
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self.process = subprocess.Popen(
            [
                "openssl", "s_server", "-quiet",
                "-cert", str(self.cert), "-key", str(self.key),
                "-accept", f"127.0.0.1:{self.port}", "-naccept", "2",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        deadline = time.time() + 10
        while time.time() < deadline:
            try:
                with socket.create_connection(("127.0.0.1", self.port), timeout=0.5):
                    return self
            except OSError:
                time.sleep(0.1)
        raise RuntimeError("openssl s_server did not start listening")

    def __exit__(self, *exc_info) -> None:
        if self.process is not None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)


def new_cases(http_port: int | None, tls_port: int | None):
    """The 1.10.0 cases; network parts are skipped when the host is offline.

    A boot types at most ~150 bytes of commands: the kernel console's receive
    buffer is small, and a longer queue makes `run.py`'s serial write block and
    time out (the earlier suites split sessions for the same reason). The
    harness asserts that limit in `boot()`.
    """
    cases: list[tuple[str, list[Check]]] = [
        (
            "threads demo",
            [
                Check("threads counter", "threads", output="counter=1000"),
                Check("threads usage", "threads -x", code=2,
                      contains=("usage: threads",)),
            ],
        ),
        (
            "entropy applets",
            [
                Check("uuidgen entropy", "uuidgen -n 4",
                      pattern="\n".join([UUID] * 4), validator=distinct_uuids),
                Check("shuf entropy", "shuf -e a b c d e",
                      validator=same_multiset(["a", "b", "c", "d", "e"])),
            ],
        ),
        (
            "kill signals",
            [
                Check("kill -0 group", "kill -0 0", output=""),
                Check("kill no such pid", "kill 99999", code=1,
                      contains=("kill: 99999: No such process",)),
                Check("kill bad pid", "kill abc", code=2,
                      contains=("kill: invalid pid: 'abc'",)),
            ],
        ),
        (
            "dns literal and usage",
            [
                Check("dns literal", "dns 10.0.2.2", output="10.0.2.2 -> 10.0.2.2"),
                Check("dns usage", "dns -z", code=2, contains=("usage: dns",)),
                Check("dns missing host", "dns", code=2, contains=("usage: dns",)),
            ],
        ),
    ]
    if http_port is not None:
        host = f"10.0.2.2:{http_port}"
        cases += [
            (
                "plain HTTP against the host",
                [
                    Check("curl http", f"curl http://{host}/h", output="hello"),
                    Check("curl http -i", f"curl -i http://{host}/h",
                          contains=("HTTP/1.0 200 OK", "Content-Length: 6")),
                ],
            ),
            (
                "plain HTTP download",
                [
                    Check("wget http", f"wget -O h.out http://{host}/h",
                          contains=("wget: saved h.out (6 bytes)",)),
                    Check("cat wget output", "cat h.out", output="hello"),
                ],
            ),
            (
                "chunked response",
                [
                    Check("curl chunked", f"curl http://{host}/chunked",
                          output="hello"),
                    Check("curl chunked -i", f"curl -i http://{host}/chunked",
                          contains=("HTTP/1.1 200 OK",
                                    "Transfer-Encoding: chunked",
                                    "hello")),
                ],
            ),
        ]
    if tls_port is not None:
        cases += [
            (
                "dns resolver",
                [
                    Check("dns example.com", "dns example.com",
                          pattern=r"example\.com -> \d{1,3}(?:\.\d{1,3}){3}"),
                    Check("dns -s example.com", "dns -s 10.0.2.3 example.com",
                          pattern=r"example\.com -> \d{1,3}(?:\.\d{1,3}){3}"),
                    Check("dns nxdomain", "dns example.invalid", code=1,
                          contains=("dns: example.invalid:",)),
                ],
            ),
            (
                "HTTPS GET",
                [
                    Check("curl https", "curl https://example.com/",
                          contains=("Example Domain", "This domain is for use")),
                    Check("curl https -i", "curl -i https://example.com/",
                          contains=("HTTP/1.1 200 OK", "Content-Type: text/html")),
                ],
            ),
            (
                "HTTPS GET to a file",
                [
                    Check("curl https -o", "curl -o out.html https://example.com/"),
                    Check("cat https output", "cat out.html",
                          contains=("Example Domain",)),
                ],
            ),
            (
                "HTTPS wget",
                [
                    Check("wget https", "wget -O w.html https://example.com/",
                          contains=("wget: saved w.html (",)),
                    Check("wget https body", "cat w.html",
                          contains=("Example Domain",)),
                ],
            ),
            (
                "HTTPS wget to stdout",
                [
                    Check("wget https stdout", "wget -O - https://example.com/",
                          contains=("Example Domain",)),
                ],
            ),
            (
                "HTTPS fail-closed",
                [
                    Check("curl nxdomain", "curl https://example.invalid/", code=1,
                          contains=("curl: resolve:",)),
                    Check("curl bad scheme", "curl ftp://example.com/", code=2,
                          contains=("unsupported URL",)),
                ],
            ),
            (
                "TLS chain gate",
                [
                    Check("curl bad tls", f"curl https://10.0.2.2:{tls_port}/",
                          code=1, contains=("curl: tls:",)),
                ],
            ),
        ]
    return cases


# ─── Runner ───────────────────────────────────────────────────────────────


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
    for name, data in {**base.FIXTURES, **HTTP_FIXTURES}.items():
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


def records(log: str) -> dict[str, tuple[str, int]]:
    found: dict[str, tuple[str, int]] = {}
    for match in RECORD.finditer(log):
        found[match.group(1).strip()] = (match.group(2), int(match.group(3)))
    return found


def verify(log: str, checks: list[Check]) -> list[str]:
    found = records(log)
    failures: list[str] = []
    for check in checks:
        record = found.get(check.command)
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
        validator = getattr(check, "validator", None)
        if validator is not None:
            problem = validator(output)
            if problem is not None:
                failures.append(f"{check.command}: {problem}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--kernel", type=pathlib.Path, default=DEFAULT_KERNEL)
    parser.add_argument("--runner", type=pathlib.Path, default=DEFAULT_RUNNER)
    parser.add_argument("--transcript", type=pathlib.Path, default=DEFAULT_TRANSCRIPT)
    parser.add_argument("--http", action="store_true",
                        help="run the plain-HTTP host-server cases")
    parser.add_argument("--tls", action="store_true",
                        help="run the DNS/HTTPS cases (needs host internet)")
    args = parser.parse_args()
    # The full run is the default; --http/--tls exist so a developer can run a
    # subset while iterating.
    if not args.http and not args.tls:
        args.http = args.tls = True

    if not PAGHBOX.is_file():
        print(f"live_test: missing {PAGHBOX}; run cargo build first", file=sys.stderr)
        return 2
    if not args.kernel.is_file() or not args.runner.is_file():
        print("live_test: kernel or run.py missing", file=sys.stderr)
        return 2

    work = pathlib.Path(tempfile.mkdtemp(prefix="pagh-live-1100-"))
    try:
        http_root = work / "http-root"
        http_root.mkdir()
        for name, data in HTTP_FIXTURES.items():
            (http_root / name).write_bytes(data)
        server = HostServer(http_root) if args.http else None
        tls_server = UntrustedTlsServer(work) if args.tls else None
        try:
            if server is not None:
                server.__enter__()
            if tls_server is not None:
                tls_server.__enter__()
            cases = base.CASES + new_cases(
                server.port if server is not None else None,
                tls_server.port if tls_server is not None else None,
            )
            return run_cases(args, work, cases)
        finally:
            if tls_server is not None:
                tls_server.__exit__(None, None, None)
            if server is not None:
                server.__exit__(None, None, None)
    finally:
        shutil.rmtree(work, ignore_errors=True)


def run_cases(args, work: pathlib.Path, cases) -> int:
    disk = work / "disk.img"
    rootfs = work / "rootfs"
    transcript = [
        f"# paghbox 1.10.0 live transcript ({len(cases)} boots)",
        "# kernel: " + str(args.kernel),
        f"# driver: python3 tools/live_test_1100.py",
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
            print(last_log[-6000:], file=sys.stderr)
            return 1

        transcript.append(f"===== boot {index + 1}: {name} =====")
        found = records(last_log)
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
    raise SystemExit(main())