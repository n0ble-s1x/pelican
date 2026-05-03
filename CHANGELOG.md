# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed
- `--no-transcode` uploads now share the 56-char filename cap with the
  transcode path. Previously, direct uploads of MP3/M4A/AAC/WAV with long
  source filenames silently became broken stubs on the watch.
- Headless dispatch in `main.rs` now triggers when any of `--delete`,
  `--list-playlists`, `--create-playlist` is set. Previously these flags
  silently launched the GUI and the requested operation never ran.

### Changed
- `MtpRsBackend::upload` streams chunks lazily from disk via
  `futures::stream::poll_fn` instead of buffering the entire file plus a
  parallel chunked Vec. Memory peak ≈ 256 KB instead of 2× file size.
- `transcode::sanitize_filename_stem` is now public so the filename rules
  can be reused by both upload paths.
- `playlist::serialize_for_device` takes a `PathStyle` enum (variants
  `BareCasePreserved` and `UppercaseWithPrefix`) — documented seam for
  future playlist-protocol work.

### Added
- `examples/probe_playlist.rs` — automated playlist write-format probe
  (six variants: path styles × format codes).
- `examples/probe_vendor_ops.rs` — sweep Garmin vendor opcodes
  `0x9000-0x900B` + `0x9810`/`0x9811`. Logs to `target/probe_vendor_ops.log`.
- `examples/wipe_stubs.rs` — find and delete unreadable handles in `/Music`.
- `examples/dump_file.rs` — hex-dump a single file pulled from the watch.
- `docs/` directory — protocol reference, vendor-op probe results,
  playlist failure log, code-audit findings, dated research log,
  references snapshots from `better-sync` / `go-mtpfs`.
- 25 new unit tests covering filename + tag sanitizers, audio
  classification, directory-tree expansion, playlist parse edge cases,
  and `serialize_for_device` for both `PathStyle` variants.

### Verified hardware behavior (Forerunner 165 Music · FW 2506)
- MTP playlist write rejected across **all six** tried variants
  (`docs/playlists.md`). Working hypothesis: FR165 firmware does not
  expose the MTP playlist code path at all. Pending confirmation against
  a borrowed older watch.
- Vendor opcode probe to completion **wedges** the device's MTP session;
  `usb_reset` does not clear it; physical replug is required.
- Filename collisions during upload corrupt **both** files (existing +
  new) instead of cleanly overwriting. Pre-write collision check is
  tracked as a follow-up.

### Security
- Dropped `egui_extras` (unused; pulled in `ureq` + `rustls` + `ring` +
  `webpki-roots` + ~40 transitive crates — a full HTTP/TLS stack in a tool
  with no network surface). Dependency count: 497 → 449.
- `transcode::cache_dir()` now writes to `$XDG_CACHE_HOME/pelican` (fallback
  `$HOME/.cache/pelican`) instead of `$TMPDIR/pelican`. The previous form
  ignored `create_dir_all` errors and could be subverted by a hostile symlink
  pre-planted in `/tmp` on a shared multi-user host.
- `cargo audit` allowlist for RUSTSEC-2024-0436 (`paste`, unmaintained,
  transitive via `eframe → wgpu → metal`) moved to `audit.toml` with rationale.
- Tightened SPDX license allowlist in `deny.toml` (removed unused
  `CC0-1.0`, `OpenSSL`, `Unicode-DFS-2016`, `MPL-2.0`).

### Changed
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

### Added
- `scripts/check.sh` — local QA gate (fmt, clippy, build, test, audit, deny).
- Flatpak packaging recipe in `packaging/flatpak/`.
- `unsafe_code = "deny"` lint at crate root (one documented exception in
  `src/gvfs.rs` for a `geteuid` syscall).

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
  staging folder is browseable
