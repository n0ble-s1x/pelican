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
  and `tower-http` are not in the compiled graph, because the macOS shell
  puts them in `Cargo.lock` without ever building them. See "The macOS
  shell" below.
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
  (`MODE="0660" TAG+="uaccess"` — grants ACL to the active session user only;
  no `root:root` daemon, no setuid binary).

## The macOS shell (`crates/pelican-shell`)

The macOS build puts a WKWebView in front of the same engine. That is a real
change to the threat model and to the numbers an auditor will check first, so
it is written down here rather than discovered.

**The frontend has no package manager.** `ui/` is hand-written HTML, CSS and
JavaScript, served from disk. There is no npm, no bundler, no `node_modules`
and no lockfile that `cargo-deny` cannot read. This is a product requirement:
a JS package manager would add a second supply chain with none of the
auditing above applied to it.

**`reqwest`, `hyper` and `tower-http` appear in `Cargo.lock` and are never
compiled.** Tauri declares them for features this build does not enable
(`default-features = false`; no updater, no HTTP plugin, no CLI dev server).
The lockfile is therefore *not* evidence either way, so `scripts/check.sh`
asserts the thing that actually matters, on every run:

```
$ cargo tree -e normal -i reqwest
warning: nothing to print.
```

Same for `hyper` and `tower-http`. If any of them ever becomes reachable, the
quick check fails and the no-network claim in this file has to be rewritten
before anything ships.

The port adds **+254 lockfile entries** (285 added, 31 removed) — Tauri, wry,
and the `objc2` family. That is a large number for a small app and it is the
price of a native window; nearly all of it is Apple framework bindings that a
Cocoa app links against anyway.

**`withGlobalTauri: true`.** With no bundler there is no way to `import` the
API module, so the whole `window.__TAURI__` namespace is exposed to the page.
That includes `__TAURI__.mocks`, whose `clearMocks()` mutates
`__TAURI_INTERNALS__`. Anything executing in the renderer can reach the nine
commands listed in `crates/pelican-shell/capabilities/main.json`, and can
disturb the IPC internals. The mitigation is that
there is no way to get code into the renderer — the CSP has no `unsafe-inline`,
no remote origin and no `connect-src` that leaves the machine — not that the
namespace is hidden.

**The ACL is the boundary, and it is committed.** `build.rs` declares an app
manifest naming every command, which is what makes our own commands
ACL-gated at all; without it they would be callable regardless of any
capability file. `capabilities/main.json` then grants exactly nine
permissions and **no `core:default`** — the shell needs no window, fs, shell,
http or event permission. Each of the nine maps 1:1 to an `invoke()` call in
`ui/app.js`; a granted-but-uncalled command is live IPC surface that
`removeUnusedCommands` cannot strip, because listing it in a capability is
exactly what marks it as used. The generated grant
(`gen/schemas/capabilities.json`, `permissions/autogenerated/`) is committed
so widening it shows up as a reviewable diff.

**The asset protocol is a real grant.** In-webview playback needs the
webview to be able to read audio files, so when the user picks a library
folder that directory is added to the asset-protocol scope, recursively, for
the process lifetime. Consequences, stated plainly:

- Anything running in the renderer can read any file under that folder.
- The scope is **not** persisted by Tauri and `tauri.conf.json` ships an
  empty scope. A static `$HOME/**` would turn any script injection into
  arbitrary file read; instead the grant is rebuilt at startup from a single
  path the user chose, stored in `shell.json` in the data dir.
- The path is `canonicalize()`d before granting, because `Scope::is_allowed`
  resolves symlinks before matching — a `~/Music/NAS -> /Volumes/...` link
  would otherwise not match its own grant.
- `scan_folder`, `start_sync` and `set_now_playing` each re-check that their
  argument resolves inside a granted root, so the ACL is not the only thing
  standing between a crafted `invoke` and the rest of the disk.

**Media decoding is OS surface `cargo-deny` cannot reach.** Playback hands a
file path to WKWebView, which decodes it through AVFoundation and CoreAudio.
Those are Apple's parsers, not ours and not in the dependency graph; a
malformed FLAC is handled by the same code path as one opened in Safari. We
gain no attack surface a browser does not have, and we can audit none of it.

**No `async fn` commands.** Not a style rule: an async Tauri command runs on
a tokio worker inside an active runtime, and the MTP backend's internal
`block_on` panics there. `scripts/check.sh` greps for it.

## What you, the user, should know

- Krypteia · Pelican runs in **userspace** with no elevated privileges
- It writes only to: `~/.local/share/pelican/` (per-device journal, and on
  macOS `~/Library/Application Support/com.krypteia.pelican/`) and
  `$XDG_CACHE_HOME/pelican/` (transcoded audio cache, swept on startup; on
  macOS `~/Library/Caches/com.krypteia.pelican/`) — per-user dirs, never
  `/tmp` or another shared location
- It opens USB devices through `nusb`, requires the udev rule for non-root
  access
- It shells out to `ffmpeg` for audio normalization. The path it shells out
  to is your system `ffmpeg` from `$PATH`. If your `$PATH` is compromised,
  so is this. (Standard for any tool that uses `ffmpeg`.) On macOS with no
  ffmpeg installed it falls back to `/usr/bin/afconvert`, a fixed absolute
  path that is not looked up in `$PATH`.
- On macOS it reads the folder you choose, and only that folder. Playback
  works by letting the app's own window read files under it — see "The macOS
  shell" above for exactly what that grants.
- It does **not** open network sockets, write outside its data dir, or
  modify system files

## Future hardening (tracked, not yet shipped)

- `cargo-vet` for an explicit per-dependency audit ledger — every transitive
  crate signed off in `supply-chain/` instead of trusting the SPDX-license
  allowlist alone. Surveyed during the security-hardening pass: of 449
  crates, 93 are covered by community audits (Mozilla / Bytecode Alliance /
  Google / Embark / ISRG / Zcash / Fermyon); the remaining 354 would need
  to be exempted at adoption and audited incrementally.
- Reproducible-build verification (deterministic `cargo build --release`
  output across machines).

## Acknowledgements

We'll publicly credit reporters in release notes (with permission). If you
prefer anonymous reporting that's fine too.
