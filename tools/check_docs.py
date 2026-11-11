#!/usr/bin/env python3
"""Check that the documentation figures match the code and the package index.

    python3 tools/check_docs.py

This is the guard against the class of drift that 1.9.0 shipped: the README
still described 121 applets and did not list `threads` even though the code and
the package manifest had it. The checker reads `NAMES` from the dispatch module
(the same single source of truth `build_repo.py` uses) and asserts:

* the README applet-table heading says `(len(NAMES))` and every name appears in
  the table (aliases such as `test` / `[` share one row, so a name counts when
  its backticked spelling occurs in the section);
* the README package sentence says `Пакет `paghbox VERSION` содержит N команд`,
  matching `programs/Cargo.toml` and `NAMES`;
* `catalog.md` mentions the same package version and that all N names are in
  `commands=`;
* `docs/package-format.md` says `N commands in paghbox VERSION`.

Package payload size/SHA-256 are deliberately **not** checked here: the ELF
depends on the unpinned `libpagh` path dependency, so CI (which builds against
pagh-api's default branch) can legitimately build a different payload than the
release figure quoted in the README. Sizes/hashes are the job of
`build_repo.py --check` for the package itself.

Only the Python standard library is used.
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))

from build_repo import applet_names, package_version  # noqa: E402

README = ROOT / "README.md"
CATALOG = ROOT / "catalog.md"
PACKAGE_FORMAT = ROOT / "docs" / "package-format.md"


class DocError(Exception):
    """Documentation and code disagree."""


def flattened(path: pathlib.Path) -> str:
    """The file's text with all whitespace runs collapsed to one space."""
    return " ".join(path.read_text(encoding="utf-8").split())


def check_readme(names: list[str], version: str) -> list[str]:
    text = README.read_text(encoding="utf-8")
    failures: list[str] = []

    match = re.search(r"^## Applet'ы \((\d+)\)", text, re.M)
    if match is None:
        failures.append("README: no '## Applet'ы (N)' heading")
        count = None
    else:
        count = int(match.group(1))
        if count != len(names):
            failures.append(
                f"README: applet heading says {count}, NAMES has {len(names)}"
            )

    heading = text.find("## Applet'ы (")
    end = text.find("`cat/wc/head/tail`", heading if heading >= 0 else 0)
    section = text[heading : end if end >= 0 else len(text)]
    missing = [name for name in names if f"`{name}`" not in section]
    if missing:
        failures.append(f"README: applet table has no row for: {', '.join(missing)}")

    sentence = f"`paghbox {version}` содержит {len(names)}"
    if sentence not in text:
        failures.append(
            f"README: expected the package sentence {sentence!r} (from "
            "programs/Cargo.toml and NAMES)"
        )
    if not re.search(rf"{len(names)} модул", text):
        failures.append(f"README: structure block does not say {len(names)} modules")
    return failures


def check_catalog(names: list[str], version: str) -> list[str]:
    text = flattened(CATALOG)
    failures: list[str] = []
    if f"`paghbox` {version}" not in text and f"paghbox {version}" not in text:
        failures.append(f"catalog.md: does not mention paghbox {version}")
    if f"все {len(names)} входят" not in text:
        failures.append(f"catalog.md: does not say all {len(names)} names are in commands=")
    return failures


def check_package_format(names: list[str], version: str) -> list[str]:
    text = flattened(PACKAGE_FORMAT)
    expected = f"{len(names)} commands in paghbox {version}"
    if expected not in text:
        return [f"docs/package-format.md: expected {expected!r}"]
    return []


def main() -> int:
    try:
        names = applet_names()
        version = package_version()
    except Exception as error:  # BuildError is defined in build_repo
        print(f"check_docs: cannot read the source of truth: {error}", file=sys.stderr)
        return 2

    failures: list[str] = []
    for check in (check_readme, check_catalog, check_package_format):
        failures.extend(check(names, version))

    if failures:
        for failure in failures:
            print(f"check_docs: {failure}", file=sys.stderr)
        return 1
    print(
        f"OK: docs match the code: {len(names)} applets, paghbox {version}, "
        "README/catalog/package-format consistent"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())