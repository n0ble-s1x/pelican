# Security policy

## Reporting a vulnerability

Report security problems privately through GitHub's private vulnerability
reporting: open the repository's **Security** tab and choose **Report a
vulnerability**, or go straight to
<https://github.com/n0ble-s1x/pelican/security/advisories/new>. Please do not
open a public issue, pull request or discussion for a security problem.

A useful report says what is affected (file, command or build), how to
reproduce it, and what an attacker gains. A proof of concept helps but is not
required.

Ordinary bugs, including crashes with no security impact, belong in
[public issues](https://github.com/n0ble-s1x/pelican/issues).

## How a report is handled

Pelican follows coordinated disclosure: the details of a vulnerability stay
private until a fix is available, and then everything is published. The code
itself is always public.

1. You submit the report. It becomes a draft security advisory that only you
   and the maintainer can see.
2. The maintainer acknowledges it, usually within a few days, and confirms or
   disputes the issue with you in the advisory thread.
3. The fix is developed privately, in a temporary private fork attached to
   the advisory. You can be invited to review or contribute to it.
4. The fix is merged and released.
5. The advisory is published on GitHub, with a CVE requested through GitHub
   where the issue warrants one. It credits you if you want credit; you can
   also stay anonymous.

If a fix will take longer than about 90 days, the maintainer will tell you
why and agree on a disclosure date with you. If the problem is in an
upstream dependency, say so in the report, and the maintainer will help get
it to the right project.

## Scope

In scope:

- Code in this repository, including `ui/`.
- Build configuration and packaging recipes.
- Anything Pelican writes to disk or sends over USB.
- The udev rule, and the one privileged command the app can run.

Out of scope:

- Vulnerabilities in upstream crates (report them upstream; we will
  coordinate if Pelican is affected).
- Vulnerabilities in Garmin firmware (report them to Garmin).
- Attacks that need someone who already controls your user account or your
  `$PATH`.

## Project practices

- **No hosted CI.** GitHub Actions is disabled. Formatting, lints, build,
  tests, `cargo audit` and `cargo deny` run through
  [`scripts/check.sh`](scripts/check.sh) on the contributor's machine, and
  the maintainer re-runs it before merging. That takes compromised Actions,
  leaked CI tokens and malicious workflows off the attack surface.
- `cargo audit --deny warnings` fails on any RUSTSEC advisory, including
  unmaintained and unsound crates. The exceptions are listed, each with its
  reasoning, in [`.cargo/audit.toml`](.cargo/audit.toml). Every exception is
  informational (unmaintained or unsound); no vulnerability is ignored.
  `check.sh` fails if `deny.toml` ignores a different set.
- `cargo deny` enforces a license allowlist, rejects yanked crates, allows
  only crates.io as a source, and bans wildcard versions.
- Dependabot opens update PRs only for releases at least two days old, so a
  poisoned release has time to be noticed. Nothing is auto-merged.
- No network code. The binaries open no sockets, and `check.sh` fails if
  `reqwest`, `hyper` or `tower-http` is compiled into any of them.
- `Cargo.lock` is committed.
- `unsafe_code = "deny"` across the workspace. The one exception is a
  `geteuid` call in `crates/pelican-core/src/platform/gvfs.rs`, with a SAFETY
  comment.
- `main` is protected: changes land through pull requests, force-pushes and
  branch deletion are blocked, the rules apply to admins, and merges are
  squash-only. Every commit on `main` must carry a verified signature. The
  maintainer signs with a dedicated, passphrase-protected SSH key kept apart
  from any authentication key.
- The udev rule (`udev/70-garmin-mtp.rules`) adds only `TAG+="uaccess"`: an
  ACL on Garmin devices for the user at the active seat. No mode change, no
  group, no daemon, no setuid binary.

## The desktop app

`crates/pelican-shell` builds `pelican-app`, a Tauri 2 window over
`pelican-core` that uses the system webkit2gtk.

- **No JavaScript supply chain.** `ui/` is plain HTML, CSS and JS, checked
  in and compiled into the binary. There is no npm, bundler or
  `node_modules`, so the whole dependency graph is the one `cargo deny` and
  `cargo audit` check.
- **Tauri with default features off** (`wry` only) and no Tauri plugins: no
  fs, shell, dialog, http, updater or opener plugin, and no asset protocol.
- **The ACL is the boundary.** `build.rs` registers an app manifest, so every
  command is ACL-gated. `capabilities/main.json` grants the app's own
  commands plus `core:event:allow-listen` and `allow-unlisten` (for progress
  events and file drops), and no `core:default`. `check.sh` fails if the
  gated, registered and granted lists differ from each other or from the
  commands `ui/app.js` invokes, or if anything else is granted.
- **CSP.** `default-src 'self'`; scripts and styles from `'self'` only, with
  no `'unsafe-inline'` or `'unsafe-eval'`; `img-src 'self' data:`; IPC is the
  only `connect-src`; no objects, frames, workers or form targets; no remote
  origin. `check.sh` checks this too. `freezePrototype` is on.
  `withGlobalTauri` exposes `window.__TAURI__` to the page's own scripts,
  which the CSP limits to the bundled files.
- **No delete command**, as in the core. `library_list` and `preview` only
  read local files. The window reads `$XDG_CONFIG_HOME/pelican/config.json`
  for the library folder and never writes it.
- **One device session at a time.** `status`, `watch_list`, `push`,
  `backup_watch`, `reset_check` and `reset_ledger` share one lock, and a
  second caller is refused as busy. Device work runs on its own thread, never
  the UI thread.
- Run it with `cargo run -p pelican-shell`, not `cargo tauri dev`: a dev
  server is served without the CSP.

**The one privileged action.** The window's **Install the USB rule** button
(`install_udev_rule`) runs one fixed command through polkit:

```sh
/usr/bin/pkexec /bin/sh -c 'install -m 644 /dev/stdin /etc/udev/rules.d/70-garmin-mtp.rules && udevadm control --reload && udevadm trigger --action=add --subsystem-match=usb --attr-match=idVendor=091e && udevadm settle'
```

The command is a constant in `crates/pelican-shell/src/udev.rs`. The rule is
compiled in from `udev/70-garmin-mtp.rules` and written to the command's
stdin. Nothing from the webview is passed to the command or interpolated
into it, and pkexec is called by absolute path. The button is refused when
pkexec is missing or when the app runs inside a Flatpak (`FLATPAK_ID` set or
`/.flatpak-info` present); the window then shows the command to run by hand.
It runs only when you press the button and authenticate. `udev_rule_status`
only reads the two rule paths.

The GTK3 stack Tauri uses on Linux brings two advisory exceptions (`glib`
0.18 and `proc-macro-error`), argued in `.cargo/audit.toml`.

## What Pelican does on your machine

- It runs as your user, with no elevated privileges.
- It writes to:
  - `~/.local/share/pelican/`: the per-watch ledger.
  - `~/.cache/pelican/staging/`: transcodes for the current send, removed
    when the send ends and swept at the next start if a send was killed.
  - `~/Documents/Pelican/<model> backup <YYYY-MM-DD>/`: the watch backup,
    when you run one. `pelican backup DEST` writes into the folder you name
    instead. A backup creates files and never replaces one.

  All of these are per-user directories, never `/tmp` or another shared
  location.
- It reads your source files and never writes to them.
- It opens the watch through `nusb`, which needs the udev rule for access
  without root.
- It runs `ffmpeg` from your `$PATH` to transcode. If your `$PATH` is
  compromised, so is Pelican, as with any tool that runs `ffmpeg`. Source
  paths are passed as arguments, never through a shell.
- On the watch it only creates files in `/Music`, under names it has never
  used. It has no code path that deletes or overwrites anything there. The
  backup only reads from the watch.
- A watch's ledger is reset only after Pelican re-reads `/Music` itself and
  finds no audio. The reset is an appended line; nothing is erased.
- It opens no network sockets and changes no system files, with one
  exception you trigger yourself: the **Install the USB rule** button writes
  `/etc/udev/rules.d/70-garmin-mtp.rules` through polkit, after asking for
  your password.

## Not done yet

- `cargo-vet` is not set up, and audit coverage of the current dependency
  graph has not been measured. To count the graph, run
  `grep -c '^\[\[package\]\]' Cargo.lock` (the count includes the three
  workspace crates).
- Reproducible builds have not been verified across machines.
