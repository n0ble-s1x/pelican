# Security Policy

Krypteia takes security seriously. This document explains how to report
vulnerabilities and what we do to keep this project safe.

## Reporting a vulnerability

**Please do not file a public GitHub issue for security problems.** Instead:

- Email **krypteia.annex447@8alias.com** with the details
- We'll acknowledge within **3 business days**
- We aim to ship a fix or mitigation within **30 days** for high-severity
  issues; lower severity may take longer

If the issue is in an upstream dependency rather than this project's code,
let us know anyway — we'll help shepherd the report.

## Scope

In scope:
- Code in this repository
- Build configuration, CI workflows
- Anything our binary writes to disk or sends over USB

Out of scope:
- Vulnerabilities in upstream dependencies (report those upstream; we'll
  coordinate)
- Vulnerabilities in Garmin firmware (report to Garmin's security team)
- Brute-forcing user-provided file paths or filesystem permissions

## What we do on our side

- **`cargo audit`** runs in CI on every PR and main-branch push; advisories
  break the build
- **`cargo deny`** enforces an allowlist of approved licenses and forbids
  yanked / vulnerable crates
- **Dependabot** opens PRs weekly for outdated dependencies
- **No telemetry, no network access** — the binary doesn't connect to anything
- **Reproducible builds** via committed `Cargo.lock`
- **CI builds on a pinned toolchain** to avoid silent compiler/std-lib changes
- **All releases are signed** (SSH-key signed git tags; releases include
  SHA-256 of binaries)
- **Branch protection on `main`**: required PR review, required passing CI,
  no force pushes, no direct admin merges of unreviewed code
- **Minimum-required permissions** on the udev rule we ship
  (`MODE="0660" TAG+="uaccess"` — grants ACL to the active session user only;
  no `root:root` daemon, no setuid binary)

## What you, the user, should know

- Krypteia · Pelican runs in **userspace** with no elevated privileges
- It writes only to: `~/.local/share/pelican/` (per-device journal)
  and `$TMPDIR/pelican/` (transcoded MP3 cache, swept on startup)
- It opens USB devices through `nusb`, requires the udev rule for non-root
  access
- It shells out to `ffmpeg` for audio normalization. The path it shells out
  to is your system `ffmpeg` from `$PATH`. If your `$PATH` is compromised,
  so is this. (Standard for any tool that uses `ffmpeg`.)
- It does **not** open network sockets, write outside its data dir, or
  modify system files

## Acknowledgements

We'll publicly credit reporters in release notes (with permission). If you
prefer anonymous reporting that's fine too.
