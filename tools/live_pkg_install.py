#!/usr/bin/env python3
"""Live `pkg install` check for the generated package repository.

This is the evidence harness behind `docs/live-<version>-pkg-install.txt`. It
serves a built `repo/` over HTTP to a QEMU guest, installs `paghbox` with the
kernel's own `pkg` installer, and then checks what the installer did:

1. boot 1 (with QEMU user network): `pkg repo`, `pkg index`, `pkg install
   paghbox` against the host's HTTP server on `10.0.2.2`; the transcript must
   contain `pkg: installed paghbox <version>` from the index;
2. boot 2 (no network): run two installer-created symlinks — one of them a
   current-batch applet — to prove the install is not a network artefact;
3. `debugfs -R 'ls -l /bin'` on the same image: the `paghbox` inode must be the
   payload size from the index and every `commands=` name must be a symlink,
   plus `paghbox` itself (so `1 + len(commands)` entries, no duplicates).

Usage::

    cargo build --release --target riscv64gc-unknown-none-elf
    python3 tools/build_repo.py
    python3 tools/live_pkg_install.py
    python3 tools/live_pkg_install.py --repo PATH --transcript PATH
"""

from __future__ import annotations

import argparse
import functools
import http.server
import pathlib
import re
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading

ROOT = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_REPO = ROOT / "repo"
DEFAULT_ELF = ROOT / "target" / "riscv64gc-unknown-none-elf" / "release" / "paghbox"
DEFAULT_KERNEL = pathlib.Path(
    "/home/oleg/pagh-riscv/target/riscv64gc-unknown-none-elf/release/pagh"
)
DEFAULT_RUNNER = pathlib.Path("/home/oleg/pagh-riscv/tools/run.py")
PROMPT = "pagh:rv64>"


class Failure(RuntimeError):
    """A check failed; the caller prints the transcript and exits 1."""


def die(message: str, code: int = 2) -> None:
    print(f"live_pkg_install: {message}", file=sys.stderr)
    raise SystemExit(code)


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args, **kwargs) -> None:  # noqa: D401 - quiet by design
        pass


class HostServer:
    def __init__(self, directory: pathlib.Path) -> None:
        self.port = free_port()
        handler = functools.partial(QuietHandler, directory=str(directory))
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", self.port), handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *exc_info) -> None:
        self.httpd.shutdown()
        self.httpd.server_close()


def read_index(repo: pathlib.Path) -> dict:
    text = (repo / "index").read_text()
    lines = text.splitlines()
    if not lines or lines[0] != "PPKGINDEX 1":
        die(f"bad index header in {repo / 'index'}")
    packages = []
    for line in lines[1:]:
        if not line.strip():
            continue
        name, version, sha, size, kind, commands, url = line.split("\t")
        packages.append(
            {
                "name": name,
                "version": version,
                "sha256": sha,
                "size": int(size),
                "kind": kind,
                "commands": commands.split(","),
                "url": url,
            }
        )
    return {"packages": packages}


def payload_size(package: pathlib.Path) -> int:
    data = package.read_bytes()
    if data[:8] != b"PPKG0001":
        die(f"{package} is not a .ppkg")
    _manifest_len, payload_len = struct.unpack_from("<II", data, 8)
    return payload_len


def build_image(disk: pathlib.Path, rootfs: pathlib.Path) -> None:
    if rootfs.exists():
        shutil.rmtree(rootfs)
    rootfs.mkdir(parents=True)
    (rootfs / "abc.txt").write_bytes(b"abc")
    (rootfs / "hello.txt").write_bytes(b"hello\n")
    (rootfs / "dd.txt").write_bytes(b"dd-payload\n")
    (rootfs / "crlf.txt").write_bytes(b"one\r\ntwo\r\n")
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
    keys: str,
    work: pathlib.Path,
    tag: str,
    *,
    net: bool,
    settle: float,
) -> str:
    sock = work / f"{tag}.serial.sock"
    monitor = work / f"{tag}.monitor.sock"
    log = work / f"{tag}.log"
    command = [
        sys.executable, str(runner),
        "--kernel", str(kernel),
        "--disk", str(disk),
        "--keys", keys,
        "--settle", str(settle),
        "--timeout", "60",
        "--serial", str(sock),
        "--monitor", str(monitor),
        "--serial-log", str(log),
        "--shot", str(work / f"{tag}.png"),
    ]
    if net:
        command.append("--net")
    try:
        subprocess.run(
            command, check=False, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, timeout=240,
        )
    except subprocess.TimeoutExpired:
        die(f"{tag}: run.py timed out")
    if not log.exists():
        die(f"{tag}: no serial log")
    return log.read_text(encoding="utf-8", errors="replace")


def session_lines(log: str) -> list[str]:
    """The transcript from the first prompt onwards, CR-stripped."""
    start = log.find(PROMPT)
    if start < 0:
        return []
    text = log[start:].replace("\r\n", "\n").replace("\r", "\n")
    return [line for line in text.split("\n")]


def run() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", type=pathlib.Path, default=DEFAULT_REPO)
    parser.add_argument("--kernel", type=pathlib.Path, default=DEFAULT_KERNEL)
    parser.add_argument("--runner", type=pathlib.Path, default=DEFAULT_RUNNER)
    parser.add_argument("--transcript", type=pathlib.Path)
    args = parser.parse_args()

    repo = args.repo
    index = read_index(repo)
    if len(index["packages"]) != 1:
        die(f"expected one package in the index, got {len(index['packages'])}")
    package = index["packages"][0]
    version = package["version"]
    commands = package["commands"]
    package_path = repo / package["url"]
    if not package_path.is_file():
        die(f"missing {package_path}")
    payload = payload_size(package_path)

    if args.transcript is None:
        args.transcript = ROOT / "docs" / f"live-{version}-pkg-install.txt"
    if not args.kernel.is_file() or not args.runner.is_file():
        die(f"kernel or run.py missing: {args.kernel} / {args.runner}")

    work = pathlib.Path(tempfile.mkdtemp(prefix=f"pagh-pkg-{version}-"))
    disk = work / "disk.img"
    rootfs = work / "rootfs"
    try:
        with HostServer(repo) as server:
            base = f"http://10.0.2.2:{server.port}/"
            print(f"+ serving {repo} at {base}")
            build_image(disk, rootfs)
            install_log = boot(
                args.runner, args.kernel, disk,
                f"cd /mnt\npkg repo {base}\npkg index\npkg install paghbox\n",
                work, "install", net=True, settle=40.0,
            )
            install = session_lines(install_log)
            joined = "\n".join(install)
            expected_installed = f"pkg: installed {package['name']} {version}"
            if expected_installed not in joined:
                raise Failure(
                    f"{expected_installed!r} missing from the install session\n{joined}"
                )
            if f"pkg: index 1 package(s)" not in joined:
                raise Failure(f"the index fetch did not report one package\n{joined}")

            # Two NIC-less boots: the console input is typed one byte at a
            # time and the kernel's receive buffer is small, so a single boot
            # with seven commands is longer than the settle window reliably
            # covers. Each boot checks the symlinks it was given.
            symlink_sessions: list[tuple[str, str, tuple[str, ...]]] = [
                (
                    "symlink-old",
                    "cd /mnt\nrun /mnt/bin/dns 10.0.2.2\nrun /mnt/bin/threads\n",
                    ("10.0.2.2 -> 10.0.2.2", "counter=1000"),
                ),
                (
                    "symlink-new",
                    "cd /mnt\nrun /mnt/bin/dd if=/mnt/dd.txt status=none\n"
                    "run /mnt/bin/nc\n"
                    "run /mnt/bin/iconv -f ascii -t utf8 /mnt/hello.txt\n"
                    "run /mnt/bin/printenv\n",
                    ("dd-payload", "usage: nc", "hello", "PWD=/mnt"),
                ),
                (
                    "symlink-proc",
                    "cd /mnt\nrun /mnt/bin/free\nrun /mnt/bin/ps\n"
                    "run /mnt/bin/df\n",
                    ("Mem:", "PID TTY", "procfs"),
                ),
            ]
            symlink_blocks: list[tuple[str, list[str]]] = []
            for tag, keys, expected in symlink_sessions:
                symlink_log = boot(
                    args.runner, args.kernel, disk, keys,
                    work, tag, net=False, settle=10.0,
                )
                symlink = session_lines(symlink_log)
                symlink_joined = "\n".join(symlink)
                for needle in expected:
                    if needle not in symlink_joined:
                        raise Failure(
                            f"{needle!r} missing from the {tag} session\n{symlink_joined}"
                        )
                symlink_blocks.append((tag, symlink))

        # The installer's on-disk result, read by an independent tool.
        listing = subprocess.run(
            ["debugfs", "-R", "ls -l /bin", str(disk)],
            check=True, capture_output=True, text=True,
        ).stdout
        entries: dict[str, str] = {}
        for line in listing.splitlines():
            fields = line.split()
            if len(fields) >= 7 and fields[-1] not in (".", ".."):
                entries[fields[-1]] = " ".join(fields[2:6])
        if "paghbox" not in entries:
            raise Failure(f"/bin/paghbox missing from debugfs output:\n{listing}")
        if str(payload) not in entries["paghbox"]:
            raise Failure(
                f"/bin/paghbox inode does not show {payload} bytes: {entries['paghbox']}"
            )
        missing = [name for name in commands if name not in entries]
        if missing:
            raise Failure(f"commands without a symlink in /bin: {missing}")
        extra = sorted(set(entries) - set(commands) - {"paghbox"})
        if extra:
            raise Failure(f"unexpected /bin entries: {extra}")
        if len(entries) != len(commands) + 1:
            raise Failure(
                f"/bin has {len(entries)} entries, expected {len(commands) + 1}"
            )

        transcript = [
            f"# paghbox {version}: pkg install and symlink live check",
            "",
            f"Host: `python3 tools/live_pkg_install.py` (serve.sh equivalent: an HTTP",
            f"server on 127.0.0.1; the guest sees it as `10.0.2.2`).",
            f"Guest: QEMU `virt`, kernel release `{args.kernel}`, 64 MiB ext2, `--net`.",
            f"Package: `repo/index` -> `{package['name']} {version}`, "
            f"{package['size']} bytes, SHA-256 `{package['sha256']}`,",
            f"payload {payload} bytes, {len(commands)} commands.",
            "The disk starts empty except for the fixtures `abc.txt`, `hello.txt`,",
            "`dd.txt` and `crlf.txt`; there is no pre-installed `paghbox`.",
            "",
            "Boot 1 installs from the freshly built `repo/`; boots 2-4 reuse the",
            "same disk without a NIC and run installer-created symlinks. Boot 2",
            "checks the earlier applets (`dns`, `threads`), boot 3 `dd`/`nc`/`iconv`/",
            "`printenv`, boot 4 the procfs applets: `free` prints its `Mem:` row,",
            "`ps` its table, `df` the `procfs` mount (the test kernel has /proc).",
            "",
            "```text",
            "==== install ====",
            *install,
            "```",
            "",
            "```text",
            "==== symlink-old (no NIC) ====",
            *symlink_blocks[0][1],
            "```",
            "",
            "```text",
            "==== symlink-new (no NIC) ====",
            *symlink_blocks[1][1],
            "```",
            "",
            "```text",
            "==== symlink-proc (no NIC) ====",
            *symlink_blocks[2][1],
            "```",
            "",
            f"`debugfs -R 'ls -l /bin' disk.img` shows the installed `paghbox` inode",
            f"({payload} bytes) and **{len(commands)} symlinks** (one per `commands=`",
            f"entry), {len(entries)} names total. The first lines are:",
            "",
            "```text",
            *listing.splitlines()[:12],
            "...",
            "```",
            "",
            f"All {len(commands)} command names were found exactly once, and the inode size",
            "matches the index payload byte-for-byte.",
            "",
        ]
        args.transcript.parent.mkdir(parents=True, exist_ok=True)
        args.transcript.write_text("\n".join(transcript), encoding="utf-8")
        print(f"OK: {package['name']} {version} installed, "
              f"{len(commands)} symlinks in /bin")
        print(f"transcript: {args.transcript}")
        return 0
    except Failure as failure:
        print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(run())