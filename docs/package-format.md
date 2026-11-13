# .ppkg and repository index format

A pagh package (`.ppkg`) is deliberately simple: a fixed header, a UTF-8 manifest,
and one ELF payload. There is no tar/ar inside, no compression and no package
signatures yet (the kernel gained userland TLS sessions in 0.21.0, but the `pkg`
installer still mirrors the repository over plain HTTP; the OpenPGP signature
step is the next milestone, and until it lands the index carries only SHA-256 and
size).

## File layout

```
offset  size  field
0       8     magic "PPKG0001"
8       4     manifest_len   (u32 LE)
12      4     payload_len    (u32 LE)
16      N     manifest (UTF-8, key=value lines, LF separated)
16+N    M     payload: one static RV64 ELF, exactly payload_len bytes
```

Manifest keys (all required unless noted):

```
name=hello
version=1.0.0
arch=riscv64
kind=app                  # app | applet-bundle
commands=hello           # comma-separated command names the package provides
entry=hello              # command the shell should use as argv[0] default (optional)
description=one-line text
sha256=<hex of payload>   # must match the index entry
```

`kind=applet-bundle` means the payload is one busybox-style ELF: the installer
installs `paghbox` once and creates one symlink per `commands` entry; each symlink
dispatches on `argv[0]`.

The format itself does not cap the number of commands. The kernel installer's
current limit is `MAX_COMMANDS = 160` (raised from 32 in 0.10.2), which covers the
120-name target plus the `threads`, `dns`, `nc` and `dd` extras (124 commands in
paghbox 1.11.0); a package above it would be downloaded and hash-checked, then
refused with `pkg: install: package manifest is invalid`. That is an
implementation limit, not a manifest error.

## Repository index (`index`)

One line per package, LF-terminated:

```
PPKGINDEX 1
name<TAB>version<TAB>sha256<TAB>size<TAB>kind<TAB>commands<TAB>url
```

`url` is relative to the repository root (`pkg/hello-1.0.0.ppkg`). The `sha256`
and `size` fields describe the **whole `.ppkg` file** — the bytes the installer
streams and hashes while downloading. (The manifest's own `sha256` describes only
the ELF payload and is checked after the download gate.) The installer fetches
`<repo>/index`, then `<repo>/<url>`, verifies size and SHA-256 of the downloaded
bytes **before** parsing `PPKG0001`, and only then installs. An unsigned repository is a valid test mirror; a mirror whose index
or package fails the digest is refused and nothing is installed.

## Installer (`pkg` in the kernel shell)

```
pkg repo [URL]        show or set the repository base URL (default http://10.0.2.2:8000/)
pkg index             fetch and cache the index (RAM); prints package count
pkg list              list installable packages and installed state
pkg install <name>    fetch, verify, install under /mnt/bin, create symlinks
pkg remove <name>     remove the installed package's files/symlinks
pkg installed         list installed packages
```

The installer is **not apt**: no Debian metadata, no `.deb`, no OpenPGP (yet),
no dependency solver. It is the native package path for the ports in this repo.
An installed package writes `/mnt/pkg/<name>.manifest`; `pkg remove` reads it.
