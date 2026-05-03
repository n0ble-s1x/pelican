# Status — What Works, What Doesn't

Reference scope: Pelican v0.1.0 series, verified against **Forerunner 165
Music · firmware 2506** unless otherwise noted. Other Garmin music watches
*should* behave similarly (same `microsoft.com: 1.0` MTP responder family)
but have not been individually tested.

## ✅ Works

| Feature                                                          | Notes                                                                                  |
|------------------------------------------------------------------|----------------------------------------------------------------------------------------|
| Music upload — MP3, M4A, M4B, AAC, WAV (direct)                  | Pure-Rust mtp-rs; per-file MTP session; verified large multi-file batches              |
| Music upload — FLAC, OGG, Opus, WMA, AIFF, ALAC, APE, WV         | Auto-transcoded to CBR 192 kbps MP3 via ffmpeg, ID3v2.3, strict tag allowlist          |
| Album-artist tag rewriting                                       | `album_artist` becomes the `ARTIST` tag — multi-composer albums group as one           |
| Filename sanitization (56-char cap, FAT-hostile chars stripped)  | Applied in **both** transcode AND `--no-transcode` paths                               |
| `set_split_header_data(true)` for the MTP transport              | Required by Garmin firmware; auto-applied                                              |
| Listing `/Music` with broken-stub surfacing                      | Surfaced as `‹unreadable #N›` rows; carries handle for delete                          |
| Per-file delete, multi-delete in one CLI invocation              | `--delete Music/foo.mp3 --delete Music/bar.mp3 …`                                      |
| GVFS-mount detection                                             | Refuses to start if a GVFS MTP mount is holding the device                             |
| GUI (eframe/egui) — three-pane file browser, drag-drop           | Linux-first; window title "Krypteia · Pelican"                                         |
| Local playlists (history-stored, not pushed to watch)            | Per-device-serial JSON in `$XDG_DATA_HOME/pelican/uploads-<serial>.json`               |
| Streaming upload from disk (no full-file buffer)                 | Memory peak now ~CHUNK (256 KB), not 2× file size                                      |

## ⚠ Works with caveats

| Feature                                                          | Caveat                                                                                                    |
|------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------|
| Listing newly-created subfolders inside `/Music`                 | Garmin firmware returns `Protocol GeneralError`. We **flatten by default**; `--no-flatten` for opt-in.    |
| Filename collision when sanitizer truncates two sources alike    | Watch firmware **corrupts both files** rather than cleanly overwriting. No collision check today — TODO.  |
| Files with no ID3 title+artist                                   | Land on disk but invisible in the music app. Default warns + uploads; `--require-tags` strict-rejects.    |
| Deleting broken stubs left by failed prior writes                | Watch refuses `DeleteObject` for its own broken handles (`Protocol GeneralError`). Auto-GC'd eventually.  |

## ❌ Doesn't work / Blocked

| Attempt                                                          | Outcome                                                                                                  |
|------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------|
| Playlist write via MTP `SendObjectInfo` + `SendObject`           | Silently rejected on FR165 across all 6 path/format variants. See `docs/playlists.md`.                   |
| Playlist write with format codes 0xBA05 / 0xBA10 / 0xBA11        | Same outcome — format code is not the discriminator on FR165.                                            |
| Vendor-op probe (`examples/probe_vendor_ops`)                    | Wedges device session; requires physical replug (USB reset alone insufficient).                          |
| Combined-bulk MTP transfers (mtp-rs default w/o split-header)    | Hangs `send_object_stream` to 30s timeout; wedges session.                                               |
| Sending FLAC with embedded album art via SendObject              | Watch silently rejects (oversized APIC frame). Mitigated by `-vn` in transcode pipeline.                 |

## Hardware-firmware test matrix

|                              | FR165 Music · 2506 | FR945 / FR255 / Venu | FR645 Music             |
|------------------------------|--------------------|----------------------|--------------------------|
| Music upload                 | ✅ verified        | (presumed ✅)        | (presumed ✅)            |
| MTP playlist write           | ❌ rejected        | ✅ per better-sync   | ✅ per better-sync       |
| Vendor opcodes 0x9000-0x900B | declared           | declared (FR945)     | `0x9000-0x9006` declared |

We **do not have second-watch hardware to confirm** whether FR165's playlist
rejection is a model-specific firmware regression or a wider issue. Adding
a borrowed FR945 / FR255 to the test matrix would resolve the ambiguity in
~30 minutes; see `docs/playlists.md` for the recipe to validate.

## Test coverage (as of 2026-05-03)

29 tests pass:

- **25 unit tests** in `src/playlist.rs` (9), `src/transcode.rs` (8), `src/transfer.rs` (8)
  - Filename stem sanitizer: cap-at-56, replace unsafe chars, collapse dashes, trim outer, never empty
  - Tag value sanitizer: strips ©®™℗ + control bytes; preserves accented letters
  - File extension classification (audio vs not, supported-by-Garmin vs needs-transcode)
  - `expand_inputs_with` flatten + non-flatten directory tree behavior
  - `playlist::parse` edge cases: CRLF, blank lines, multiple comment-line variants, empty
  - `playlist::serialize_for_device` for both `PathStyle` variants
  - `is_playlist` extension matching (case-insensitive, edge cases)
- **4 integration tests** in `tests/sanitization.rs` — invariants tied to documented Garmin firmware quirks

What we **do not** unit-test (and why):

- `mtp.rs` MTP backend — needs hardware. Covered by `examples/diagnose`, `examples/probe_playlist`, manual smoke tests.
- `app.rs` GUI — needs an event loop. Manual end-to-end testing only.
- `garmin::pick_device` — wraps nusb enumeration. Manual.
- `gvfs::warn_if_holding_garmin` — POSIX-side IPC. Manual.
- `history::record` write atomicity — best-effort JSON writes; no concurrency in practice.
