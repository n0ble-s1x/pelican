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
  [`.cargo/audit.toml`](.cargo/audit.toml) — that path, not the repo root,
  is the one `cargo-audit` actually reads. Every entry there is
  *informational* (`unmaintained` / `unsound`) and argued crate by crate;
  no vulnerability is allowlisted.
- **`cargo deny`** enforces an SPDX license allowlist, forbids yanked crates,
  restricts crate sources to crates.io, and bans wildcard versions.
- **Dependabot** opens PRs for outdated dependencies on a **48-hour cooldown**
  — no PR is created for a release younger than 2 days, giving the world time
  to flag a poisoned release before it surfaces here.
- **Never auto-merge. Anything.** Not Dependabot PRs, not security PRs, not
  one-line typo fixes. Every merge is a deliberate human decision.
- **No telemetry, no network access.** The binary opens no sockets and
  compiles no HTTP/TLS code. `cargo deny check sources` audits where crates
  come from; `scripts/check.sh` additionally asserts that `reqwest`, `hyper`
  and `tower-http` are not in the compiled graph, graphical shell included.
- **Reproducible builds** via committed `Cargo.lock`.
- **`unsafe_code = "deny"`** in `Cargo.toml` — every `unsafe` block in our
  code requires an explicit `#[allow(unsafe_code)]` with a SAFETY comment.
  Currently there is exactly one (a `geteuid` syscall in `crates/pelican-core/src/platform/gvfs.rs`).
- **Branch protection on `main`**: PRs are required (no direct push to main),
  no force-pushes, no branch deletion, conversation must be resolved before
  merge, admins are subject to all rules. The maintainer is the sole
  collaborator with merge access; external contributors PR from forks.
- **Signed commits enforced on `main`.** Every commit that lands on `main`
  must carry a valid SSH signature from a key registered to the maintainer's
  GitHub account. GitHub displays a green "Verified" badge on each commit;
  unsigned commits are rejected at push time. The maintainer signs from a
  dedicated, passphrase-protected ed25519 key kept separate from any
  authentication key — a leak of one does not compromise the other.
- **Squash-only merges.** Web UI does not offer rebase or merge-commit;
  every PR collapses into a single signed commit on `main`, keeping a
  linear, attributable history.
- **Web commit sign-off required** — any commit authored through GitHub's
  web UI must include a DCO sign-off line.
- **Minimum-required permissions** on the udev rule we ship
  (`udev/70-garmin-mtp.rules`: `TAG+="uaccess"` only — an ACL for the active
  seat user on Garmin devices; no mode change, no group, no `root:root`
  daemon, no setuid binary).

## The graphical shell

`crates/pelican-shell` builds `pelican-app`, a Tauri 2 window over
`pelican-core` (Linux-first, system webkit2gtk). What keeps it small:

- **No JS supply chain.** `ui/` is plain HTML, CSS and JS, checked in and
  compiled into the binary. No npm, no bundler, no `node_modules` — so the
  whole dependency graph is the one `cargo deny` and `cargo audit` read.
- **Tauri with default features off** (`wry` only: no asset compression,
  no runtime-mutable ACL) and **no Tauri plugins** — no fs, shell, dialog,
  http, updater or opener. No asset protocol is enabled.
- **The ACL is the boundary.** `build.rs` registers an app manifest, so
  every command is ACL-gated, and `capabilities/main.json` grants exactly
  the eleven commands the UI invokes plus `core:event:allow-listen` /
  `allow-unlisten` (for run progress and file drag-and-drop) — no
  `core:default`. `scripts/check.sh` fails if the gated, registered and
  granted lists differ or anything else is granted.
- **CSP**: `default-src 'self'`; scripts and styles from `'self'` only (no
  `'unsafe-inline'`, no `'unsafe-eval'`); `img-src 'self' data:`; IPC as the
  only `connect-src`; no objects, frames, workers or form targets; no remote
  origin anywhere — also asserted by `scripts/check.sh`. `freezePrototype`
  is on. `withGlobalTauri` exposes `window.__TAURI__` to the page's own
  scripts, which the CSP limits to the bundled files.
- **No delete command exists**, as in the core. `library_list` and
  `preview` only read local files; `set_library_root` writes one file,
  `$XDG_CONFIG_HOME/pelican/config.json`.
- **One device session at a time.** `status`, `watch_list`, `push`,
  `backup_watch`, `reset_check` and `reset_ledger` share one lock and a second caller is refused as busy; device work runs on its
  own OS thread, never the UI thread.
- Run it with `cargo run -p pelican-shell`, not `cargo tauri dev`: a dev
  server is served without the CSP.

**The one privileged path.** `install_udev_rule` (the window's "Install
the USB rule" button) runs exactly one fixed command through polkit:
`/usr/bin/pkexec /bin/sh -c 'install -m 644 /dev/stdin
/etc/udev/rules.d/70-garmin-mtp.rules && udevadm control --reload &&
udevadm trigger --action=add --subsystem-match=usb
--attr-match=idVendor=091e && udevadm settle'`. The command is a constant
in `crates/pelican-shell/src/udev.rs`; the rule is compiled in with
`include_str!` from `udev/70-garmin-mtp.rules` and written to the child's
stdin; the command takes no arguments from the webview and nothing is
interpolated into it. pkexec is called by absolute path, and the button
is refused when it is missing or when the app runs inside a Flatpak
(`FLATPAK_ID` set or `/.flatpak-info` present), where the window prints
the host command instead. It runs only when you press the button and
authenticate; `udev_rule_status` only reads the two rule paths.

The GTK3 stack Tauri uses on Linux brings two argued advisory exceptions
(`glib` 0.18, `proc-macro-error`); the reasoning is in `.cargo/audit.toml`.

## What you, the user, should know

- Krypteia · Pelican runs in **userspace** with no elevated privileges.
- It writes only to `~/.local/share/pelican/` (the per-watch ledger),
  `~/.cache/pelican/staging/` (per-run transcodes, removed when the run ends
  and swept on the next start if a run was killed) and, for the window only,
  `~/.config/pelican/config.json` (the music folder). Per-user dirs, never
  `/tmp` or another shared location.
- It reads your source files and never writes to them.
- It opens USB devices through `nusb`, which needs the udev rule for non-root
  access (`udev/70-garmin-mtp.rules`).
- It runs `ffmpeg` from your `$PATH` to transcode. If your `$PATH` is
  compromised, so is this — standard for any tool that uses `ffmpeg`. Source
  paths are passed as absolute paths, never through a shell.
- On the watch it only creates files in `/Music`, under names it has never
  used. It has no code path that deletes or overwrites anything there.
- The watch backup is read-only on the watch (it lists and downloads
  `GARMIN/`) and writes only into the folder you pick, default
  `~/Documents/Pelican/`, creating files and never replacing one.
- A watch's ledger is reset only after Pelican re-reads `/Music` itself and
  finds no audio left; the reset is an appended line, not an erasure.
- It does **not** open network sockets, write outside its data dirs, or
  modify system files — with one exception you trigger yourself: the
  window's "Install the USB rule" button writes
  `/etc/udev/rules.d/70-garmin-mtp.rules` through polkit, after asking for
  your password (see The graphical shell).

## Future hardening (tracked, not yet shipped)

- `cargo-vet` for an explicit per-dependency audit ledger — every transitive
  crate signed off in `supply-chain/` instead of trusting the SPDX-license
  allowlist alone. The graph is now **624 external crates** (627
  `[[package]]` blocks in `Cargo.lock` at `ec9cd3d`, less the three workspace
  members). The community-audit coverage split quoted here previously — 93 of
  449 covered by Mozilla / Bytecode Alliance / Google / Embark / ISRG / Zcash
  / Fermyon — was measured against the pre-Tauri, Linux-only graph and is
  **not** valid for the current one. It has to be re-derived against the
  real 624 before any of it is republished; `cargo vet` needs network access
  to fetch those audit sets, so it has not been re-run here.
- Reproducible-build verification (deterministic `cargo build --release`
  output across machines).

## Acknowledgements

We'll publicly credit reporters in release notes (with permission). If you
prefer anonymous reporting that's fine too.
