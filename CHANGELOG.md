# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Initial public release.

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
