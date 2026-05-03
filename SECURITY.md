# Security Policy

Krypteia takes security seriously. This document explains how to report
vulnerabilities and the supply-chain posture we maintain on this project.

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
- Build configuration, packaging recipes
- Anything our binary writes to disk or sends over USB

Out of scope:
- Vulnerabilities in upstream dependencies (report those upstream; we'll
  coordinate)
- Vulnerabilities in Garmin firmware (report to Garmin's security team)
- Brute-forcing user-provided file paths or filesystem permissions

## What we do on our side

- **No third-party CI.** We do not use GitHub Actions or any hosted CI. The
  build / lint / test / audit pipeline lives in [`scripts/check.sh`](scripts/check.sh)
  and runs entirely on the maintainer's machine, where every command is
  inspectable. Eliminates a major supply-chain surface (compromised Action,
  leaked CI tokens, malicious workflow on PR from fork).
- **Local-CI contract.** Contributors run `./scripts/check.sh --full` before
  opening a PR. The maintainer pulls the PR branch and re-runs the same script
  locally before merge.
- **`cargo audit`** runs as part of `scripts/check.sh --full`; new RUSTSEC
  advisories break the build. Allowlisted advisories (with rationale) live in
  [`deny.toml`](deny.toml).
- **`cargo deny`** enforces an SPDX license allowlist, forbids yanked crates,
  restricts crate sources to crates.io, and bans wildcard versions.
- **Dependabot** opens PRs for outdated dependencies on a **48-hour cooldown**
  — no PR is created for a release younger than 2 days, giving the world time
  to flag a poisoned release before it surfaces here.
- **Never auto-merge. Anything.** Not Dependabot PRs, not security PRs, not
  one-line typo fixes. Every merge is a deliberate human decision.
- **No telemetry, no network access.** The binary opens no sockets and bundles
  no HTTP/TLS code. The dep tree is audited (via `cargo deny check sources`)
  to keep this true; any new network-capable transitive dep would surface in
  the supply-chain check.
- **Reproducible builds** via committed `Cargo.lock`.
- **`unsafe_code = "deny"`** in `Cargo.toml` — every `unsafe` block in our
  code requires an explicit `#[allow(unsafe_code)]` with a SAFETY comment.
  Currently there is exactly one (a `geteuid` syscall in `src/gvfs.rs`).
- **Branch protection on `main`**: PRs are required (no direct push to main),
  no force-pushes, no branch deletion, conversation must be resolved before
  merge, admins are subject to all rules. The maintainer is the sole
  collaborator with merge access; external contributors PR from forks.
- **Minimum-required permissions** on the udev rule we ship
  (`MODE="0660" TAG+="uaccess"` — grants ACL to the active session user only;
  no `root:root` daemon, no setuid binary).

## What you, the user, should know

- Krypteia · Pelican runs in **userspace** with no elevated privileges
- It writes only to: `~/.local/share/pelican/` (per-device journal)
  and `$XDG_CACHE_HOME/pelican/` (transcoded MP3 cache, swept on startup) —
  per-user dirs, never `/tmp` or another shared location
- It opens USB devices through `nusb`, requires the udev rule for non-root
  access
- It shells out to `ffmpeg` for audio normalization. The path it shells out
  to is your system `ffmpeg` from `$PATH`. If your `$PATH` is compromised,
  so is this. (Standard for any tool that uses `ffmpeg`.)
- It does **not** open network sockets, write outside its data dir, or
  modify system files

## Future hardening (tracked, not yet shipped)

- Signed commits + signed git tags (currently unsigned; will enable when a
  long-lived signing key is provisioned for the maintainer identity)
- `cargo-vet` for an explicit per-dependency audit ledger — every transitive
  crate signed off in `supply-chain/` instead of trusting the SPDX-license
  allowlist alone
- Reproducible-build verification (deterministic `cargo build --release`
  output across machines)

## Acknowledgements

We'll publicly credit reporters in release notes (with permission). If you
prefer anonymous reporting that's fine too.
