# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- **macOS support, verified on hardware.** Forerunner 165 Music · FW 2506 on
  macOS 26.6.2 (Apple silicon): session open, `/Music` listing, upload and
  read-back all succeed. `docs/macos-port.md` has the measured table. Apple's
  `ptpcamerad` does **not** claim Garmin watches — the watch presents
  `bDeviceClass = 0` — so macOS needs no udev equivalent, no sudo step and no
  SIP change.
- **Workspace split into three crates** (`54a9f72`): `pelican-core` (engine,
  no UI framework, no clap), `pelican` (the Linux cli + egui binary),
  `pelican-shell` (the macOS binary). Note there is no `default-members`, so
  a bare `cargo build --release` builds all three — name the package.
- **`pelican-shell`, the macOS Tauri 2 shell** (`dc7b6a8`) and the **`ui/`
  frontend** (`55dd47e`) — plain HTML/CSS/JS, no npm, no framework, no
  bundler. Watch wall, capacity gauge, live transfer report.
- **Pelican is now a local player that also syncs.** In-app playback through
  the system webview, cover art read out of the user's own file at play time,
  albums and artists views, a playback queue, media keys and Control Center
  transport (`71fe68e`, `dd1f5ea`, `bb4a73d`). Auditioning matters because the
  watch holds about 3.7 GB and committing a track is a real decision.
- **Third encoder path: `afconvert`** → CBR 192 kbps AAC in M4A, the
  zero-dependency macOS fallback. Three pipelines now, picked per file:
  passthrough (native containers copied byte-for-byte, tag rebuilt in
  process, no external tool), ffmpeg → CBR 192 kbps MP3, afconvert → M4A.
  `ffmpeg` is no longer an unconditional requirement — but `AFCONVERT_DECODES`
  does not cover OGG, Opus, WMA, APE or WV, so those still need it.
- **Name-collision guard on the upload path.** A same-name write destroys
  *both* copies and the wreckage cannot be deleted (`docs/garmin-mtp.md` §6,
  §7). Two refusal-first layers: `transfer::resolve_conflicts` compares the
  plan against the listing the caller already took, and `transfer::run` lists
  the target folder in the same open session immediately before the upload.
  A rename only ever happens when the user asks for one after being shown the
  collision. The write-time layer reads each target directory **once per run**
  and keeps what it knows current from the names it writes, so its cost does
  not grow with the plan; a name is reserved when the write lands and also
  when the bytes drained without a confirmation, because that file may be
  aboard. Neither layer has been exercised against hardware. Closes
  `docs/audit-2026-05-03.md` finding #4.
- `examples/probe_objprops.rs` — the probe behind the object-property finding
  below. `examples/probe_platform.rs` for the contention detectors.
- `examples/probe_playlist.rs` — automated playlist write-format probe
  (six variants: path styles × format codes).
- `examples/probe_vendor_ops.rs` — sweep Garmin vendor opcodes
  `0x9000-0x900B` + `0x9810`/`0x9811`. Logs to `target/probe_vendor_ops.log`.
- `examples/wipe_stubs.rs` — find and delete unreadable handles in `/Music`.
- `examples/dump_file.rs` — hex-dump a single file pulled from the watch.
- `docs/` directory — protocol reference, vendor-op probe results,
  playlist failure log, code-audit findings, dated research log,
  references snapshots from `better-sync` / `go-mtpfs`.
- `scripts/check.sh` — local QA gate (fmt, clippy, build, test, audit, deny).
  It also enforces two invariants a type checker cannot: no `async fn` in
  `commands.rs`, and a 1:1 match between the granted capabilities and the
  `invoke()` calls in `ui/app.js`.
- Flatpak packaging recipe in `packaging/flatpak/`.
- `unsafe_code = "deny"` lint at crate root (one documented exception, a
  `geteuid` syscall, now at `crates/pelican-core/src/platform/gvfs.rs`).
- Test count is now **122** (`cargo test --workspace`, macOS,
  2026-09-06) — see `docs/status.md` for the per-target split. Includes 25 new
  sanitizer/classification/playlist tests from the pre-split work, plus the
  collision guard's own suite driven through a fake `Backend`.

### Fixed
- **The watch *can* report its own tags** (`ec9cd3d`). The UI and DESIGN.md
  both claimed the opposite. Verified 2026-09-02 on FR165 / FW 2506 via
  `examples/probe_objprops.rs`: the watch declares operations `0x9801`–`0x9805`
  and answers `GetObjectPropValue` with Name, Artist, AlbumName, AlbumArtist,
  Duration and Track — including for files Pelican never uploaded. Grouping
  still keys off the local journal, which is correct for *provenance*; the
  copy now says that rather than denying a capability the device has.
- `mtp::remote_size` compared filenames byte-for-byte. `/Music` is
  FAT-derived and case-insensitive, so `Track.mp3` and `track.mp3` are one
  file to the firmware — the probe silently reported "landed" for a file it
  had failed to find. Now compares with full `to_lowercase`.
- `--no-transcode` uploads now share the 56-char filename cap with the
  transcode path. Previously, direct uploads of MP3/M4A/AAC/WAV with long
  source filenames silently became broken stubs on the watch.
- Headless dispatch in `main.rs` now triggers when any of `--delete`,
  `--list-playlists`, `--create-playlist` is set. Previously these flags
  silently launched the GUI and the requested operation never ran.
- Four defects found only by running the port against hardware: encoder
  detection by exit status (`afconvert` returns 2 for every form of help, so
  every Mac reported "no encoder"), WAV re-encoded to lossy AAC when the watch
  plays WAV natively, false-positive contention on every Mac running
  `ptpcamerad`, and a staging-filename race between threads in one process.
  All four are regression-tested. `docs/macos-port.md`.

### Changed
- `ffprobe` is gone on both platforms; `lofty` reads tags and audio
  properties in process.
- `transfer::run` is now a two-line wrapper over `transfer::run_with`, which
  takes the backend opener as an argument. The transfer loop — including the
  write-time collision refusal — is testable without a watch.
- `transfer::run_cancellable` polls a caller-supplied predicate between files,
  and `pelican-shell` now hands the engine the **whole plan in one call**
  instead of one call per file. A call per file re-read `/Music` for every
  track and started each file's memory of what the run had written empty.
  Stop keeps the granularity it had: between files, never inside a data phase
  that cannot be aborted without leaving an undeletable object.
- `MtpRsBackend::upload` streams chunks lazily from disk via
  `futures::stream::poll_fn` instead of buffering the entire file plus a
  parallel chunked Vec. Memory peak ≈ 256 KB instead of 2× file size.
- `transcode::sanitize_filename_stem` is now public so the filename rules
  can be reused by both upload paths.
- `playlist::serialize_for_device` takes a `PathStyle` enum (variants
  `BareCasePreserved` and `UppercaseWithPrefix`) — documented seam for
  future playlist-protocol work.
- MSRV bumped from 1.78 → 1.85 (transitive deps require `edition2024`).
- Dependabot cooldown set to 48 h: no PR opens for any release younger than
  two days, so a poisoned upstream has time to surface before it reaches us.
- Repository hardening (server-side, not in source diff):
  - GitHub Actions disabled at repo level — local-CI contract via
    `scripts/check.sh`.
  - Branch protection on `main`: PRs required, no force-push, no deletion,
    conversation resolution required, admins enforced.
  - Auto-merge disabled. Squash-only merges. Web commit sign-off required.
  - Secret scanning + push protection + Dependabot security updates enabled.
  - **Signed commits enforced on `main`** — `required_signatures=true` in
    branch protection. Maintainer signs from a dedicated, passphrase-
    protected ssh-ed25519 key (separate from any authentication key);
    GitHub displays a green "Verified" badge on each commit.
- `SECURITY.md`, `CONTRIBUTING.md`, and PR template rewritten around the
  local-CI contract and explicit ban on adding network-capable dependencies
  without prior discussion.

### Verified hardware behavior (Forerunner 165 Music · FW 2506)
- MTP playlist write rejected across **all six** tried variants
  (`docs/playlists.md`). Working hypothesis: FR165 firmware does not
  expose the MTP playlist code path at all. Pending confirmation against
  a borrowed older watch.
- Vendor opcode probe to completion **wedges** the device's MTP session;
  `usb_reset` does not clear it; physical replug is required.
- Filename collisions during upload corrupt **both** files (existing +
  new) instead of cleanly overwriting. The guard listed under Added is the
  response; it has not itself been run against a watch.
- The watch answers per-object property queries — see Fixed, above.

### Security
- **The macOS shell puts the system WKWebView in front of the engine.** That
  is new attack surface and `SECURITY.md` § The macOS shell documents it in
  full: `withGlobalTauri: true` exposes the whole `window.__TAURI__` namespace
  to the page, mitigated by a CSP with no `unsafe-inline`, no remote origin
  and no `connect-src` that leaves the machine.
- **The Tauri ACL is the boundary and it is committed.** `build.rs` declares
  an app manifest naming every command; `capabilities/main.json` grants
  exactly the commands `ui/app.js` calls and **no `core:default`** — no
  window, fs, shell, http or event permission. `scripts/check.sh` diffs the
  two lists in both directions.
- **The asset-protocol grant is real.** In-webview playback needs the webview
  to read audio files, so the folder the user picks is added to the
  asset-protocol scope, recursively, for the process lifetime. The scope is
  rebuilt at startup from that one path rather than persisted, `tauri.conf.json`
  ships an empty scope, and every command that accepts a path re-checks it
  against a granted root.
- **Media decoding is OS surface `cargo-deny` cannot reach.** Playback hands a
  path to WKWebView, which decodes through AVFoundation and CoreAudio —
  Apple's parsers, not in the dependency graph, not auditable by us.
- **+254 lockfile entries** for the port: 373 → 627 `[[package]]` blocks
  between `dc7b6a8^` and `ec9cd3d`, 256 added and 2 removed. Tauri, wry and
  the `objc2` family. `reqwest`, `hyper` and `tower-http` are in the lockfile
  for features this build does not enable and are **not compiled in**;
  `scripts/check.sh` fails if `cargo tree -e normal -i` ever finds them.
- Dropped `egui_extras` (unused; pulled in `ureq` + `rustls` + `ring` +
  `webpki-roots` + ~40 transitive crates — a full HTTP/TLS stack in a tool
  with no network surface). Dependency count at the time: 497 → 449.
- `transcode::cache_dir()` now writes to `$XDG_CACHE_HOME/pelican` (fallback
  `$HOME/.cache/pelican`; `~/Library/Caches/com.krypteia.pelican` on macOS)
  instead of `$TMPDIR/pelican`. The previous form ignored `create_dir_all`
  errors and could be subverted by a hostile symlink pre-planted in `/tmp` on
  a shared multi-user host.
- `cargo audit` allowlist for RUSTSEC-2024-0436 (`paste`, unmaintained,
  transitive via `eframe → wgpu → metal`) moved to `audit.toml` with rationale.
- Tightened SPDX license allowlist in `deny.toml` (removed unused
  `CC0-1.0`, `OpenSSL`, `Unicode-DFS-2016`, `MPL-2.0`).

## [0.1.0] - 2026-05-02

### Added
- GUI: three-pane file browser (LOCAL · actions · WATCH) with drag-and-drop
  from the OS file manager, intra-app row drag, and right-click context menus
- Headless CLI: `--copy`, `--delete`, `--list-playlists`, `--create-playlist`,
  `--track`, `--require-tags`, `--no-transcode`
- ffmpeg-based audio normalization (CBR 192 kbps MP3, ID3v2.3 strict tag
  allowlist, sanitized 56-char filename, embedded album art stripped)
- Garmin-firmware workarounds: `split_header_data(true)`, per-handle listing
  to surface broken stubs, local free-space delta tracking
- Per-device upload journal in `~/.local/share/pelican/`
- Local playlists (saved track groups, batch send to watch)
- udev rule for non-root USB access
- Onboarding panel with troubleshooting hints
- Verified end-to-end on Forerunner 165 Music (firmware 2506)

### Known limitations
- MTP playlist writes silently rejected by Garmin firmware regardless of
  format code (vendor-specific path not yet reverse-engineered)
- Broken-stub deletion via `DeleteObject` returns GeneralError; watch
  GCs them on power-cycle
- Garmin's indexed music library is not exposed via MTP — only the
  staging folder is browseable[^objprops]

[^objprops]: Half of this is now known to be wrong, and per `docs/README.md`'s
convention the shipped entry is left standing with the correction attached.
Verified 2026-09-02 on FR165 / FW 2506 (`examples/probe_objprops.rs`): there
is indeed no MTP call that *enumerates* the indexed library, but the watch
declares operations `0x9801`–`0x9805` and answers `GetObjectPropValue` for any
handle in `/Music` — Name, Artist, AlbumName, AlbumArtist, Duration and Track,
including for files Pelican never uploaded. See `docs/garmin-mtp.md`
"Object properties". The local journal remains the source of *provenance*,
which is a different question from whether the tags are readable.
