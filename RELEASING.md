# Releasing

This is the operational runbook for **pagh-programs** (the `paghbox` applet
bundle). The branch model is in `BRANCHING.md`; the version/feature rules are in
`README.md` and `catalog.md`. Nothing here releases automatically except the
tag-driven `release` workflow in `.github/workflows/release.yml`.

## Version sources

| Repository | Version source |
|---|---|
| Pagh-OS, pagh-riscv, pagh-python, pagh-rust | `Cargo.toml` (`[package].version`; workspace members as documented below) |
| pagh-libc | `VERSION` |
| pagh-api | `libpagh/Cargo.toml` |
| pagh-programs | `programs/Cargo.toml` |
| pagh-repo | index manifest version |

**This repository:** the version source is `programs/Cargo.toml`
(`[package].version` of the `paghbox` crate). It is quoted in
`README.md` (`Пакет `paghbox VERSION` содержит N команд`), `catalog.md` and
`docs/package-format.md`, and `tools/check_docs.py` fails if those figures drift
from the code (`NAMES` in `programs/src/applets/mod.rs` plus this file). CI runs
that check, and the release workflow runs it again before publishing. The
`.ppkg`/`repo/index` built by `tools/build_repo.py` carry the same version;
`python3 tools/build_repo.py --check` is the package-level gate.

`dev` carries the **next release** version. The first feature after a release
bumps MINOR, the first fix bumps PATCH. `main` keeps the last **released**
version. A stable tag `vX.Y.Z` must equal the version in the source file on
`main`.

## Pre-release (release candidate) — from `dev` only

1. Freeze `dev`: after `vX.Y.Z-rc.1`, only fixes for this release are merged
   until the release lands. Everything else waits.
2. Pre-flight on the exact `dev` commit:
   - CI green (job `build`): `cargo build --release --target
     riscv64gc-unknown-none-elf` against the sibling `pagh-api`, `python3
     tools/build_repo.py --check` and `python3 tools/check_docs.py`;
   - live evidence when applet behaviour changed: the matching
     `tools/live_test_*.py` scenario, with its transcript under `docs/` (see
     `docs/live-testing.md`); for a docs/version-only release the CI checks
     above are the evidence;
   - release notes/tag message written (this repository keeps no CHANGELOG;
     GitHub generates notes from the merged PRs);
   - version source equals `X.Y.Z` (no `-rc` in `programs/Cargo.toml`);
   - no open blocker, no known regression in the release scope.
3. Tag and push `vX.Y.Z-rc.N`:
   ```sh
   git checkout dev && git pull --ff-only
   git tag -a vX.Y.Z-rc.1 -m "paghbox X.Y.Z-rc.1"
   git push origin vX.Y.Z-rc.1
   ```
4. Watch the `release` workflow: it refuses the tag unless it is reachable
   from `dev` **and not** from `main`, checks the base version against
   `programs/Cargo.toml`, runs `tools/check_docs.py`, and publishes a GitHub
   **Pre-release** with the source archive and its `.sha256`.
   A rejected tag stays in the repository but produces **no** release entry:
   delete it (`git push origin :refs/tags/<tag>`) before retrying.
5. Test the pre-release source archive (build the applet bundle from it and run
   the relevant live scenario). Bugs: fix on `dev`, tag `-rc.2` (never move the
   `-rc.1` tag). **Do not publish a pre-release to pagh-repo** — see below.

## Stable release — from `main`

1. Merge `dev` into `main` (merge commit; no code changes at this step):
   ```sh
   git checkout main && git pull --ff-only
   git merge --no-ff dev
   git push origin main
   ```
2. Wait for CI on `main` to be green.
3. Tag and push `vX.Y.Z` on the merge commit; the workflow checks that the
   commit is reachable from `main` and that `programs/Cargo.toml` says `X.Y.Z`.
4. Verify in **Releases**: the new entry is a full release with
   `<repo>-vX.Y.Z.tar.gz` and matching `.sha256`. GitHub marks the newest stable
   as **Latest**. pagh-programs publishes source only; there is no ELF asset.
5. **Publish the stable build to pagh-repo** (and only a stable build):
   ```sh
   # in pagh-programs, on the stable tag
   git checkout vX.Y.Z
   cargo build --release --target riscv64gc-unknown-none-elf
   python3 tools/build_repo.py           # writes repo/index and repo/pkg/*.ppkg
   python3 tools/build_repo.py --check
   # in pagh-repo, on dev
   python3 tools/publish.py ../pagh-programs/repo
   python3 tools/verify.py
   git add -A && git commit -m "repo: publish paghbox X.Y.Z"
   ```
   Open a PR to `pagh-repo` `dev`; the package repository releases `index`/`pkg`
   from its own `main` (see pagh-repo's `RELEASING.md`). A package built from
   `dev` or from an `-rc.N` tag must never be committed there.
6. `dev` continues with the next version (first change bumps it and updates the
   applet figures through `tools/check_docs.py`).

## Hotfix — straight to `main`, no RC

Hotfixes are limited to **critical/high verified security findings** or a
release-blocking regression in a released version. Everything else rides `dev`.

1. Branch from `main`: `hotfix/<short-name>`.
2. PR to `main` (never to `dev`), full CI plus a targeted regression test/PoC
   for the bug, and the bumped PATCH version in `programs/Cargo.toml` with the
   README/catalog/package-format figures updated (`tools/check_docs.py`). Get it
   reviewed.
3. Merge to `main`, then tag the PATCH release `vX.Y.Z+1` directly (no RC),
   verify the release entry, and publish the `.ppkg` to pagh-repo as in the
   stable flow.
4. Merge `main` back into `dev` with `--no-ff`:
   ```sh
   git checkout dev && git pull --ff-only
   git merge --no-ff main
   ```
   Resolve version conflicts in favour of the larger version (dev stays ahead).
5. CI on `dev` must be green after the merge-back.

## Cross-repository ABI change order

When a change touches the C/ABI boundary, release in dependency order and only
then update dependants:

1. `pagh-api` / `pagh-libc` (whichever changed; if both, `pagh-api` first when
   the SDK pins the libc header).
2. `pagh-rust` / `pagh-python` (they consume `pagh-api`/`pagh-libc`).
3. `pagh-programs` (apps built against the new SDK), `pagh-repo` (published
   packages), and finally Pagh-OS / pagh-riscv images that ship the new
   runtime/ABI.
4. Update the pinned sibling versions (`Cargo.toml` path/registry deps,
   `Cargo.lock`) in the same release train, not later.

Never tag a dependant before its dependency is tagged; the release workflow
only checks branch/version, not cross-repo compatibility.

## Backfill and "Latest"

GitHub marks **Latest** by release creation time, not by version number. When
backfilling already-released versions, create tags in ascending version order
(oldest first), so the newest version is created last and becomes Latest.

## Rollback / mistakes

- Wrong tag on the wrong commit: delete the release (`gh release delete <tag>
  --yes`) and the tag (`git push origin :refs/tags/<tag>`), fix, retag.
- Workflow rejected the tag: delete the tag; no release exists to clean up.
- Bad stable release: prefer a PATCH hotfix over moving/deleting a published
  tag; if the release is minutes old and nothing depends on it, delete
  release + tag, then re-tag the fixed commit.
- A wrong `.ppkg` reached pagh-repo: revert the publish commit there (the
  previous `pkg/` files stay as history), run `python3 tools/verify.py`, and
  only then re-publish the corrected stable build. Never move the pagh-repo tag.
- Never move an existing tag; immutable tags are enforced by the tag ruleset.

## Release checklist (paste into the release PR / notes)

- [ ] Version source (`programs/Cargo.toml`) updated and equal to the tag base
- [ ] README/catalog/package-format figures match the code (`tools/check_docs.py`)
- [ ] Release notes/tag message written
- [ ] CI green on the exact commit
- [ ] Live evidence run and linked when applet behaviour changed
- [ ] Freeze respected (no feature merges after `-rc.1`)
- [ ] Source archive + `.sha256` downloaded and built once
- [ ] Stable `.ppkg` published to pagh-repo (`build_repo.py --check`, `verify.py`)
- [ ] `main`/`dev` synchronized per the flow above
- [ ] Release entry has the right type (Pre-release vs Release)