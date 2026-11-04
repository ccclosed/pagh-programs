#!/usr/bin/env python3
"""Build (and verify) the pagh package repository.

    python3 tools/build_repo.py                 # cargo build + write repo/
    python3 tools/build_repo.py --elf PATH      # package an existing ELF
    python3 tools/build_repo.py --check         # verify repo/ only

The output is exactly the format in docs/package-format.md:

    repo/index
    repo/pkg/paghbox-<version>.ppkg

`repo/` is generated, not committed: it is reproducible from the source tree
with the command above and is listed in `.gitignore`.

Index `size` semantics
----------------------

The index `size` and `sha256` describe the **whole `.ppkg` file** — the bytes the
installer actually downloads and hashes while streaming. The manifest's
`sha256` describes only the ELF payload inside the package. The kernel installer
checks the download against the index pair before it parses anything, so an
index that described only the payload would let a malicious header/manifest
through unchecked; total-file semantics is the contract in
`docs/package-format.md`.

Only the Python standard library is used.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUT = ROOT / "repo"
DEFAULT_ELF = ROOT / "target" / "riscv64gc-unknown-none-elf" / "release" / "paghbox"
APPLETS_RS = ROOT / "programs" / "src" / "applets" / "mod.rs"
PROGRAMS_CARGO = ROOT / "programs" / "Cargo.toml"
TARGET = "riscv64gc-unknown-none-elf"

MAGIC = b"PPKG0001"
INDEX_MAGIC = "PPKGINDEX 1"
ARCH = "riscv64"
KIND = "applet-bundle"
ENTRY = "paghbox"
NAME = "paghbox"
DESCRIPTION = "pagh busybox-style bundle: the first 12 native applets"

REQUIRED_MANIFEST = {
    "name",
    "version",
    "arch",
    "kind",
    "commands",
    "description",
    "sha256",
}
OPTIONAL_MANIFEST = {"entry"}
HEX64 = re.compile(r"^[0-9a-f]{64}$")


class BuildError(Exception):
    """A build or verification failure with a human-readable reason."""


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise BuildError(f"cannot read {path}: {error}") from error


def package_version() -> str:
    text = read_text(PROGRAMS_CARGO)
    match = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
    if not match:
        raise BuildError(f"no [package] version in {PROGRAMS_CARGO}")
    return match.group(1)


def applet_names() -> list[str]:
    """Parse `NAMES` from the dispatch module (single source of truth).

    Also checks that every name has a dispatch arm, so the manifest cannot list
    an applet paghbox does not actually run.
    """
    text = read_text(APPLETS_RS)
    match = re.search(r"pub const NAMES:\s*&\[&str\]\s*=\s*&\[(.*?)\];", text, re.S)
    if not match:
        raise BuildError(f"`pub const NAMES` not found in {APPLETS_RS}")
    names = re.findall(r'"([^"]+)"', match.group(1))
    if not names:
        raise BuildError("NAMES is empty")

    seen: set[str] = set()
    for name in names:
        if name in seen:
            raise BuildError(f"duplicate applet name in NAMES: {name}")
        seen.add(name)
        if not re.search(rf'"{re.escape(name)}"\s*=>', text):
            raise BuildError(f"applet {name!r} has no dispatch arm")

    expected = [
        "echo",
        "true",
        "false",
        "yes",
        "pwd",
        "basename",
        "dirname",
        "seq",
        "cat",
        "wc",
        "head",
        "tail",
    ]
    if names != expected:
        raise BuildError(
            "implemented applet list drifted from the first-12 contract:\n"
            f"  expected: {expected}\n  found:    {names}"
        )
    return names


def validate_elf(data: bytes, origin: str) -> None:
    """The payload must be a static ELF64 little-endian RISC-V executable."""
    if len(data) < 64:
        raise BuildError(f"{origin}: shorter than an ELF64 header")
    if data[:4] != b"\x7fELF":
        raise BuildError(f"{origin}: bad ELF magic")
    if data[4] != 2:
        raise BuildError(f"{origin}: not ELFCLASS64")
    if data[5] != 1:
        raise BuildError(f"{origin}: not little-endian")
    e_type, e_machine, _e_version = struct.unpack_from("<HHI", data, 16)
    if e_type not in (2, 3):
        raise BuildError(f"{origin}: e_type={e_type}, expected ET_EXEC/ET_DYN")
    if e_machine != 243:
        raise BuildError(f"{origin}: e_machine={e_machine}, expected RISC-V (243)")
    # The ABI's user window starts at 0x10000; the linked entry must be inside
    # it. This is a layout sanity check, not a substitute for readelf.
    e_entry = struct.unpack_from("<Q", data, 24)[0]
    if e_entry < 0x10000:
        raise BuildError(f"{origin}: entry point {e_entry:#x} is below 0x10000")


def build_package(elf: bytes, version: str, commands: list[str]) -> tuple[bytes, dict]:
    payload_sha = hashlib.sha256(elf).hexdigest()
    command_list = ",".join(commands)
    manifest = (
        f"name={NAME}\n"
        f"version={version}\n"
        f"arch={ARCH}\n"
        f"kind={KIND}\n"
        f"commands={command_list}\n"
        f"entry={ENTRY}\n"
        f"description={DESCRIPTION}\n"
        f"sha256={payload_sha}\n"
    ).encode("utf-8")

    if len(manifest) > 0xFFFF_FFFF:
        raise BuildError("manifest too large for the u32 header")
    header = MAGIC + struct.pack("<II", len(manifest), len(elf))
    package = header + manifest + elf

    meta = {
        "name": NAME,
        "version": version,
        # The index describes the download: the whole .ppkg. The manifest still
        # carries the payload digest, so both are recorded and checked.
        "sha256": hashlib.sha256(package).hexdigest(),
        "size": len(package),
        "payload_sha256": payload_sha,
        "payload_size": len(elf),
        "kind": KIND,
        "commands": command_list,
        "url": f"pkg/{NAME}-{version}.ppkg",
        "manifest": manifest,
        "payload": elf,
        "package": package,
    }
    return package, meta


def write_atomic(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_bytes(data)
    os.replace(temporary, path)


def build(out: Path, elf_path: Path, do_build: bool) -> None:
    commands = applet_names()
    version = package_version()

    if do_build:
        command = ["cargo", "build", "--release", "--target", TARGET]
        print("+ " + " ".join(command))
        environment = dict(os.environ)
        environment.setdefault("CARGO_TERM_COLOR", "never")
        try:
            subprocess.run(command, cwd=ROOT, env=environment, check=True)
        except FileNotFoundError as error:
            raise BuildError(f"cargo not found: {error}") from error
        except subprocess.CalledProcessError as error:
            raise BuildError(f"cargo build failed with status {error.returncode}") from error

    try:
        elf = elf_path.read_bytes()
    except OSError as error:
        raise BuildError(f"cannot read ELF {elf_path}: {error}") from error
    validate_elf(elf, str(elf_path))

    package, meta = build_package(elf, version, commands)

    pkg_dir = out / "pkg"
    pkg_dir.mkdir(parents=True, exist_ok=True)
    # Generated artifacts only: drop stale .ppkg files so the index and the
    # directory cannot disagree. Nothing outside `pkg/*.ppkg` is touched.
    for old in pkg_dir.glob("*.ppkg"):
        old.unlink()

    package_path = out / meta["url"]
    write_atomic(package_path, package)

    index_line = (
        f"{meta['name']}\t{meta['version']}\t{meta['sha256']}\t{meta['size']}\t"
        f"{meta['kind']}\t{meta['commands']}\t{meta['url']}\n"
    )
    index = (INDEX_MAGIC + "\n" + index_line).encode("utf-8")
    write_atomic(out / "index", index)

    print(f"wrote {out / 'index'}")
    print(f"wrote {package_path} ({len(package)} bytes)")
    print(f"  payload: {len(elf)} bytes, sha256 {meta['payload_sha256']}")
    print(f"  package: {len(package)} bytes, sha256 {meta['sha256']}")
    print(f"  commands ({len(commands)}): {', '.join(commands)}")

    verify(out)


def parse_manifest(manifest: bytes, origin: str) -> dict[str, str]:
    try:
        text = manifest.decode("utf-8")
    except UnicodeDecodeError as error:
        raise BuildError(f"{origin}: manifest is not UTF-8: {error}") from error
    values: dict[str, str] = {}
    for number, line in enumerate(text.splitlines(), 1):
        if not line:
            raise BuildError(f"{origin}: manifest line {number} is empty")
        if "=" not in line:
            raise BuildError(f"{origin}: manifest line {number} has no '=': {line!r}")
        key, value = line.split("=", 1)
        if key in values:
            raise BuildError(f"{origin}: duplicate manifest key {key!r}")
        values[key] = value
    missing = REQUIRED_MANIFEST - values.keys()
    if missing:
        raise BuildError(f"{origin}: manifest missing keys: {', '.join(sorted(missing))}")
    unknown = values.keys() - REQUIRED_MANIFEST - OPTIONAL_MANIFEST
    if unknown:
        raise BuildError(f"{origin}: manifest has unknown keys: {', '.join(sorted(unknown))}")
    return values


def parse_package(path: Path) -> tuple[dict[str, str], bytes, bytes]:
    try:
        data = path.read_bytes()
    except OSError as error:
        raise BuildError(f"cannot read {path}: {error}") from error
    if len(data) < 16 or data[:8] != MAGIC:
        raise BuildError(f"{path}: bad or missing PPKG0001 magic")
    manifest_len, payload_len = struct.unpack_from("<II", data, 8)
    expected_length = 16 + manifest_len + payload_len
    if len(data) != expected_length:
        raise BuildError(
            f"{path}: length {len(data)} != 16 + manifest {manifest_len} + payload {payload_len}"
        )
    manifest_bytes = data[16 : 16 + manifest_len]
    payload = data[16 + manifest_len :]
    manifest = parse_manifest(manifest_bytes, str(path))
    validate_elf(payload, str(path))
    return manifest, payload, data


def verify(out: Path) -> None:
    index_path = out / "index"
    if not index_path.is_file():
        raise BuildError(f"{index_path} does not exist; run without --check first")
    try:
        index_text = index_path.read_text(encoding="utf-8")
    except OSError as error:
        raise BuildError(f"cannot read {index_path}: {error}") from error

    lines = index_text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    if not lines or lines[0] != INDEX_MAGIC:
        raise BuildError(f"{index_path}: first line must be {INDEX_MAGIC!r}")

    seen_names: set[str] = set()
    referenced: set[Path] = set()
    packages = 0

    for line_number, line in enumerate(lines[1:], 2):
        fields = line.split("\t")
        if len(fields) != 7:
            raise BuildError(f"{index_path}:{line_number}: expected 7 tab fields, got {len(fields)}")
        name, version, sha, size_text, kind, commands, url = fields
        where = f"{index_path}:{line_number}"

        if not re.fullmatch(r"[a-z0-9][a-z0-9._-]*", name):
            raise BuildError(f"{where}: invalid package name {name!r}")
        if name in seen_names:
            raise BuildError(f"{where}: duplicate package {name!r}")
        seen_names.add(name)
        if not version:
            raise BuildError(f"{where}: empty version")
        if not HEX64.match(sha):
            raise BuildError(f"{where}: sha256 is not 64 lowercase hex digits")
        try:
            size = int(size_text)
        except ValueError as error:
            raise BuildError(f"{where}: size {size_text!r} is not an integer") from error
        if size <= 0:
            raise BuildError(f"{where}: size must be positive")
        if kind not in ("app", "applet-bundle"):
            raise BuildError(f"{where}: kind {kind!r} is not app/applet-bundle")
        if not commands:
            raise BuildError(f"{where}: commands is empty")
        if url.startswith("/") or ".." in url.split("/"):
            raise BuildError(f"{where}: url {url!r} must be relative and stay in the repo")

        package_path = (out / url).resolve()
        try:
            package_path.relative_to(out.resolve())
        except ValueError as error:
            raise BuildError(f"{where}: url {url!r} escapes the repository") from error
        if not package_path.is_file():
            raise BuildError(f"{where}: {package_path} does not exist")
        referenced.add(package_path)

        manifest, payload, _package = parse_package(package_path)
        if manifest["name"] != name:
            raise BuildError(f"{where}: manifest name {manifest['name']!r} != index {name!r}")
        if manifest["version"] != version:
            raise BuildError(f"{where}: manifest version {manifest['version']!r} != {version!r}")
        if manifest["arch"] != ARCH:
            raise BuildError(f"{where}: manifest arch {manifest['arch']!r} != {ARCH!r}")
        if manifest["kind"] != kind:
            raise BuildError(f"{where}: manifest kind {manifest['kind']!r} != {kind!r}")
        if manifest["commands"] != commands:
            raise BuildError(f"{where}: manifest commands differ from the index")
        # The index describes the whole .ppkg (what the installer streams and
        # hashes); the manifest describes only the payload inside it.
        package_bytes = package_path.read_bytes()
        if len(package_bytes) != size:
            raise BuildError(
                f"{where}: index size {size} != package length {len(package_bytes)} "
                "(the index describes the whole .ppkg)"
            )
        digest = hashlib.sha256(package_bytes).hexdigest()
        if digest != sha:
            raise BuildError(f"{where}: package sha256 {digest} != index {sha}")
        payload_digest = hashlib.sha256(payload).hexdigest()
        if manifest["sha256"] != payload_digest:
            raise BuildError(f"{where}: manifest sha256 != payload digest")
        if len(manifest["commands"].split(",")) == 0:
            raise BuildError(f"{where}: commands parse to nothing")
        packages += 1

    pkg_dir = out / "pkg"
    if pkg_dir.is_dir():
        on_disk = {path.resolve() for path in pkg_dir.glob("*.ppkg")}
        orphans = on_disk - referenced
        if orphans:
            listed = ", ".join(sorted(path.name for path in orphans))
            raise BuildError(f"{pkg_dir}: .ppkg not referenced by the index: {listed}")

    print(f"OK: {index_path} lists {packages} package(s); all digests, sizes and manifests agree")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--out",
        type=Path,
        default=DEFAULT_OUT,
        help=f"repository directory to write/verify (default: {DEFAULT_OUT})",
    )
    parser.add_argument(
        "--elf",
        type=Path,
        help=f"package this ELF instead of compiling (default build output: {DEFAULT_ELF})",
    )
    parser.add_argument(
        "--no-build",
        action="store_true",
        help="use the default ELF path without running cargo",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify an existing repository and exit; never builds or writes",
    )
    args = parser.parse_args(argv)

    try:
        if args.check:
            verify(args.out)
            return 0
        if args.elf:
            build(args.out, args.elf, do_build=False)
        elif args.no_build:
            build(args.out, DEFAULT_ELF, do_build=False)
        else:
            build(args.out, DEFAULT_ELF, do_build=True)
    except BuildError as error:
        print(f"build_repo: error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())