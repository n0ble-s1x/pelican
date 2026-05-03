# Garmin USB/MTP Reference

Everything we've verified about Garmin's MTP responder, with the device and
firmware that established each fact.

## Device identification

- **USB Vendor ID:** `0x091E`
- **Music-capable watches are MTP-only**, never USB Mass Storage.
- **PIDs we've handled in person:**
  - Forerunner 165 Music — `0x5151` (FW 2506, May 2026)
- **PIDs known from `libmtp` device table** (vendor 0x091E, all flagged `DEVICE_FLAGS_ANDROID_BUGS`):
  - FR 645 Music `0x4b48`, FR 245 Music `0x4c05`, FR 945 `0x4c29`, Venu `0x4c9a`, FR 255 `0x4f98`, FR 265 `0x50a1`, FR 965 `0x50db`, plus Fenix 7/8 family and Edge 1040/1050.
  - Source: `src/music-players.h` in https://github.com/libmtp/libmtp
  - **FR 165 Music (`0x5151`) is not yet in libmtp** — we should file an upstream entry.

## Vendor-extension descriptor

- Garmin's `DeviceInfo.vendor_extension_desc` reports `microsoft.com: 1.0`.
- This is the **MS Media Transfer Protocol extensions** identifier (MS-DRMND).
- Implication: Garmin reuses an Android-flavored MTP responder (libmtp tags every Garmin device with `DEVICE_FLAGS_ANDROID_BUGS`). Quirks from that lineage apply: split-header transfers, short-data-phase reads, etc.

## Storage layout (FR165 Music verified)

- One storage: ID `0x00020001`, description **"Internal Storage"**, ~3.45 GiB.
- Root contains four writable folders — these are the only places music-class content can live:
  - `GARMIN/` (FW assets / activity files / Connect IQ apps)
  - `Music/` (songs)
  - `Audiobooks/`
  - `Podcasts/`
- The watch's **music app** scans `Music/` only, and filters to entries with valid ID3 `title` + `artist`. Untagged files are still on disk (visible via MTP) but invisible in the music-app UI.

## Format codes

mtp-rs's `ObjectFormatCode` enum covers the standard codes; vendor-extension format codes (e.g. `0xBA05`) need `ObjectFormatCode::Unknown(0xBA05)`.

| Hex      | Constant                          | Used for                                  |
|----------|-----------------------------------|-------------------------------------------|
| `0x3000` | `Undefined`                       | Generic / unknown — accepted for raw      |
| `0x3001` | `Association`                     | Folder                                    |
| `0x3004` | `Text`                            | Plain text — *rejected* for playlists     |
| `0x3009` | `Mp3`                             | MP3 audio                                 |
| `0xB901` | `WmaAudio`                        | WMA                                       |
| `0xB902` | `OggAudio`                        | Ogg                                       |
| `0xB903` | `AacAudio`                        | AAC                                       |
| `0xB984` | `M4aAudio`                        | M4A / M4B                                 |
| `0xBA05` | (vendor) AbstractAvPlaylist       | **Garmin playlists — required**           |
| `0xBA10` | (vendor) AbstractAudioPlaylist    | Theoretically valid; not yet tested       |
| `0xBA11` | (vendor) WPL Playlist             | Garmin Express writes WPL; not yet tested |
| `0xBA0F` | (vendor) PLS Playlist             | Listed in Garmin support doc              |

Garmin support page lists accepted music + playlist formats:
https://support.garmin.com/en-US/?faq=JyNEOTsZaR3KMXqej3oQp5
> AAC, ADTS, M3U, M3U8, M4A, M4B, MP3, PLS, WAV, WPL, ZPL.

## Hard-won protocol facts

### 1. `set_split_header_data(true)` is **mandatory**

mtp-rs's default combined-bulk transfer hangs Garmin's responder. Always call:

```rust
let device = MtpDevice::open_first().await?;
device.session().set_split_header_data(true);
```

Symptom if forgotten: `send_object_stream` hangs to 30s timeout, leaves device in a wedged `OpenSession GeneralError` state until USB reset / replug.

Verified 2026-05-02 against FR165 Music FW 2506. Documented in
[`references/go-mtpfs-pr1.md`](references/go-mtpfs-pr1.md) — same quirk
described independently for the Go MTP stack.

### 2. **Filename length cap ≈ 56 chars** for `/Music`

Files written into `/Music` whose `remote_name` exceeds ~60 chars are silently
discarded by post-write validation. Both `send_object_info` and
`send_object_stream` return `Ok` at the protocol layer, but no audio data
persists — only a 32 KB metadata-only "broken stub" handle remains.

Mitigation: `transcode::sanitize_filename_stem` truncates the stem to 56
chars and replaces exotic characters. The audio's ID3 tags carry the real
title for the screen, so the on-disk filename is just an identifier.

### 3. **Newly-created subfolders inside `/Music` are unreliable**

`storage.list_objects(handle)` on a freshly-created `/Music/<album>/` folder
returns `Protocol GeneralError`. Workaround: **flatten by default** — every
file lands directly in `/Music/`. The watch builds its library view from ID3
tags, so flat-on-disk is invisible to the user. `--no-flatten` exists for
opt-in.

### 4. Encoding profile for transcoded MP3

The exact ffmpeg invocation that lands properly on FR165 Music:

```
ffmpeg -i <in> -vn -ac 2 -ar 44100 -b:a 192k \
  -map_metadata 0 -id3v2_version 3 -write_id3v1 0 <out.mp3>
```

- **`-vn`** — strip embedded album art. FLAC's APIC frame becomes oversized ID3v2 art and triggers silent rejection.
- **`-b:a 192k`** — CBR (not VBR `-qscale:a 2`).
- **`-id3v2_version 3`** — v2.3 only. v2.4 is a coin-flip on whether tags appear in the music app.
- **`-write_id3v1 0`** — single tag version.

### 5. Library grouping

The watch's library view groups by `ARTIST + ALBUM`. For multi-composer or
soundtrack/classical material, prefer `ALBUM_ARTIST` for the `ARTIST` tag we
emit so the album view stays unified. Implemented in
`transcode::extract_safe_tags` (also writes `TPE2 (album_artist)` as a
duplicate).

### 6. "Broken stub" failure mode

When post-write validation rejects an upload (bad audio profile, oversized
art, filename too long, etc.) the MTP layer reports success but only a
metadata-only stub persists. These stubs:

- Have unreadable handles (GetObjectInfo returns `Protocol GeneralError`)
- Block whole-folder listings if you `collect()` instead of streaming
- Are eventually garbage-collected by the watch on its own
- **Cannot be deleted via MTP** — `DeleteObject` returns `Protocol GeneralError`
  on a broken-stub handle. We surface them in the UI as `‹unreadable #N›`
  rows with `is_broken=true` so the user knows they exist; cleanup is
  watch-side and asynchronous.

### 7. Filename-collision failure mode

Verified 2026-05-03 on FR165 Music FW 2506: when an upload's `remote_name`
matches an existing readable file in `/Music`, the watch firmware **does
not cleanly overwrite**. Instead, both files become unreadable broken
stubs (the existing file is corrupted, and the new upload also lands as
a stub).

This is distinct from the per-handle silent-rejection mode — the watch
*acknowledges* the upload and seems to start the overwrite, then leaves
both ends in a half-applied state.

Mitigation: Pelican should detect collisions before `SendObjectInfo` and
either skip or rename with a numeric suffix. Not implemented as of
v0.1.0; tracked in `docs/audit-2026-05-03.md` finding #4.

## Vendor opcodes

See [`vendor-ops.md`](vendor-ops.md). Probe results from FR165 Music FW 2506 logged at `target/probe_vendor_ops.log`.

## What does *not* work

| Attempt                                                 | Outcome                                                                   |
|---------------------------------------------------------|---------------------------------------------------------------------------|
| Send playlist as `ObjectFormatCode::Text` (0x3004)      | Silently rejected. No file appears.                                       |
| Create `/Music/<album>/` then list it                   | `Protocol GeneralError` on the new handle.                                |
| Combined-bulk MTP transfers (mtp-rs default)            | Hangs, wedges session.                                                    |
| Filenames > ~60 chars in `/Music`                       | Silent discard, broken stub left behind.                                  |
| FLAC files containing embedded art, sent via SendObject | Silent discard on watches; broken stub.                                   |
| `simple-mtpfs` writes (per Garmin forums)               | Often produces zero-byte files. Avoid this codepath for any future work.  |
