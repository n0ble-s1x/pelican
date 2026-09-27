# Requirements (R1-R12)

The requirements the core was rebuilt to on 2026-09-26, each a pass/fail
check. The product promise, proven on hardware the same day
(`garmin-library-persistence.md` § Results): **Pelican transcodes your music
to one known-good profile, sends it to the watch under a name that has never
been used, and proves each file landed intact.** It does not delete. Once a
track is on the watch it stays in the watch's music library until a factory
reset.

The rebuild removed the egui GUI, the macOS Tauri shell and its interface,
afconvert and passthrough, the delete and playlist code, and every hardware
example that writes or deletes. The Tauri shell was then rebuilt for Linux
over this core, with a new interface; the requirements below cover the core
and the CLI.

## Requirements (each one is a pass/fail check)

### R1: One output profile
Every source, whatever its format (FLAC, WAV, ALAC/AAC m4a, MP3, OGG, Opus, AIFF, WMA…),
is re-encoded by ffmpeg:
`-hide_banner -loglevel error -nostdin -i SRC -map 0:a:0 -vn -map_metadata -1 -c:a libmp3lame -b:a 192k -ar 44100 -ac 2 -id3v2_version 3 -write_id3v1 0 -metadata k=v… DST.mp3`.
No passthrough. Missing ffmpeg → a clear error naming the fix, before any
device work.

### R2: Tags, always present
Written tags: `title`, `artist`, `album_artist`, `album`, `track`, `date`, `genre`.
Resolution order per field: CLI override → source tag (lofty) → path fallback.
- title: tag → filename stem with a leading track number and separator
  (`01 - `, `01_`, `01.`, `1 `) stripped.
- track: tag → leading digits of the filename (one to three of them), and
  only when one of the separators above follows (the same prefix the title
  rule strips). So `01Intro` and `1999` get no track, and `2001 A Space
  Odyssey` keeps its year as part of the title. *(Amended 2026-09-26 to
  match the tested behavior.)*
- album: tag → parent directory name.
- artist: tag (`album_artist` preferred, as today) → grandparent directory name
  **if** the source was reached by walking a directory given on the command line
  and the grandparent is inside that root; otherwise the album name.
- date/genre: tag → omitted (or CLI override).
Values pass the existing `sanitize_tag_value`. A file whose resolved title is
empty is refused. `--artist/--album/--genre/--year` apply to the whole run.
Proof case: `Sea of Thieves/02 - Maiden Voyage.wav` (no usable tags) →
title "Maiden Voyage", track 2, album "Sea of Thieves", artist "Sea of Thieves".

### R3: Remote names are never reused
Remote name = `pl{counter:05}-{slug}.mp3`, slug = the existing
`sanitize_filename_stem` of the title, total stem ≤ 56 chars.
The counter comes from the ledger (R4) and is strictly monotonic per device.
Before the first write of a run, `/Music` is listed once and **every** name on
the device (and every stub handle) is treated as taken, as is every name in
the ledger, whatever its status. Comparison is case-insensitive and consistent
(one fold function used everywhere). Nothing ever writes a name that is taken.
There is no overwrite path and no "rename on collision" branch; a fresh
counter is always free.

### R4: The ledger
`$XDG_DATA_HOME/pelican/ledger-<serial>.jsonl`, append-only JSON Lines, never
pruned, never rewritten. One line per event:
`{"v":1,"at":RFC3339,"event":"reserve"|"verified"|"failed","counter":N,"remote":"…","source":"abs path","source_sha256":"…","upload_sha256":"…"?,"bytes":N?,"title":…,"artist":…,"album":…,"reason":…?}`.
- A `reserve` line is appended **and fsync'd before** `SendObjectInfo`, so a crash
  mid-write still burns the name.
- Exclusive lock (`std::fs::File::lock`, Rust ≥ 1.89, so `rust-version` is 1.89) for
  the duration of a run; a second run on the same device fails fast.
- Any unparsable line → hard error naming the file and line. Never
  "start empty", never overwrite.
- Counter for a run starts at `max(ledger counters, pl-NNNNN names on device) + 1`.
- Added after the rebuild: a `reset` event, written only after a factory
  reset is confirmed by reading `/Music` (`status.md`, "Ledger format
  change").

### R5: Skip what is already there
A source whose `source_sha256` has a `verified` event in the ledger is skipped
and reported as "already on watch", unless `--resend` (which gets a new name).
Amended after the rebuild: the skip key is the source hash plus the resolved
album, so the same song inside a playlist sent as an album is a new copy.

### R6: Upload, then prove it
One MTP session per run (not per file), `set_split_header_data(true)`, streamed
upload into `/Music` (flat; the watch rejects subfolders). After each upload:
`download_file` the new handle, SHA-256 it, compare with the local transcode.
- match → `verified` event.
- mismatch / download error / upload error → `failed` event with reason; the
  name stays burned. Retry **once** under a fresh counter (`--retries N`,
  default 1). Never delete, never re-send to the same name.
- No user-facing text anywhere may tell the user to delete something.

### R7: Capacity check before any write
Refuse the run (before the first write) if the planned MP3 bytes + 2 MiB margin
exceed device free space, or if the audio-object count on `/Music` + planned
files would exceed 500 (Garmin FAQ limit). Planned size is known exactly
because transcodes happen before the session opens (R8).

### R8: Temp files never outlive the run
Transcode all planned files first into `$XDG_CACHE_HOME/pelican/staging/<run-id>/`,
then open the session and push. Staging dir is removed on success, failure,
error and Ctrl-C (drop guard + signal-safe cleanup on next start via the
existing `sweep`). Sources are only ever read.

### R9: The CLI
- `pelican push [--dry-run] [--resend] [--retries N] [--artist A] [--album B] [--genre G] [--year Y] [--serial S] PATHS…`
  Paths are files or directories (recursive; hidden files and non-audio skipped).
  `--dry-run` transcodes nothing and touches no device: it prints the plan:
  source → resolved tags → (remote name assigned at run time).
  Real runs print one line per file and a final tally: verified / skipped / failed.
  Exit code non-zero if any file failed.
- `pelican status`: model, serial, free space, `/Music` object count, ledger totals.
- `pelican ls`: read-only listing of `/Music`, each row marked `ledger`/`foreign`/`stub`.
- `pelican ledger`: prints the ledger for the device.
- **No delete, no playlist, no tag-on-device command exists.** The `Backend`
  trait has no delete method at all, so the capability does not exist in the type.
- Every device-touching command warns if gvfs/kio holds the device (existing gvfs module).
- Added after the rebuild: `push --mix NAME`, `pelican backup [DEST]` and
  `pelican reset-ledger` (`status.md`).

### R10: udev
`udev/70-garmin-mtp.rules`: `SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="091e", TAG+="uaccess"`
with the comment explaining why it must sort before `73-seat-late.rules` and
why `ID_MTP_DEVICE` is deliberately not set. Remove the 99- rule; update any
doc or packaging file that references it.

### R11: Known bugs fixed
1. `platform::gvfs::tests::shell_quote_neutralises_embedded_quotes` failed: fix the code or the test, whichever is wrong.
2. Folder resolution compares byte-exact while files compare case-insensitive: use one fold function.
3. `to_ascii_lowercase` vs `to_lowercase` mismatch: gone with (2).
4. Size-only / zero-counts-as-success verification: replaced by R6.
5. "Delete it before retrying" messages: removed.

### R12: Gates (all must pass)
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`: including new tests for R2 (proof
  case + each fallback), R3 (taken names from device, ledger and stubs; case
  fold), R4 (reserve-before-write ordering via FakeBackend; corrupt line → error;
  lock contention), R5, R6 (mismatch → failed + retry under new name + no
  delete), R7, R8 (staging gone after success and after a failure), and one
  ffmpeg end-to-end test producing a CBR 192k 44.1k stereo MP3 with ID3v2.3
  tags (skipped with a message if ffmpeg is absent).
- `cargo deny check` and `cargo audit` (report, don't block on network failure).
- `unsafe_code = "deny"` still holds (the one documented gvfs exception may stay).

### Out of scope here
Hardware testing (done by hand after the build; agents never touch the
watch), README rewrite, the UI, packaging, macOS.

## Hardware acceptance (after the build, by hand)
Push the Sea of Thieves album (25 WAVs) from the NAS with `pelican push`.
Every file `verified`; staging dir empty; ledger has 25 reserve + 25 verified
lines; the album appears on the watch and plays. That result also settles
whether mtp-rs writes as cleanly as libmtp did.

Passed 2026-09-26 with 24 of the 25 tracks (track 02 was already on the
watch from the hand-run libmtp test). The record is in `status.md`.
