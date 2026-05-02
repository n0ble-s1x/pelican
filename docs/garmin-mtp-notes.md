# Garmin MTP — what we learned the hard way

Notes for anyone hacking on this. Verified on **Forerunner 165 Music · firmware 2506** unless otherwise noted. Garmin firmware behavior across the music-watch line is reasonably consistent but not guaranteed identical.

## Required protocol setup

- **`set_split_header_data(true)` on the PTP session.** Without it,
  `send_object_stream` hangs and times out at the 30 s default. Garmin's
  Microsoft-flavored MTP stack (`vendor_extension_desc = "microsoft.com: 1.0"`)
  doesn't accept the optimized combined-bulk path that mtp-rs uses by default.
- USB Vendor ID: `0x091E`. Music-capable models advertise an MTP interface
  (class `0xFF`/`0xFF`/`0x00`, interface string `"MTP"`) when in MTP USB mode.
  Older non-music Garmins were USB-MSC; the line switched at the Fenix 5 Plus.

## Storage layout

```
/                      (root)
├── GARMIN/            (firmware-internal — activities, settings, apps)
├── Music/             (user music — read by the watch's music app)
├── Audiobooks/        (per-file resume position; firmware bookmarks playback)
└── Podcasts/
```

Only `/Music`, `/Audiobooks`, and `/Podcasts` accept user-uploaded audio.

## Audio profile that works

Verified on FR165:

- **Format:** CBR MP3 at 192 kbps, 44.1 kHz stereo
- **Tags:** ID3v2.3 only, standard frames only — `title`, `artist`, `album`,
  `track`, `date`, `genre`. **Strip everything else.** Non-standard frames
  (`COPYRIGHT` with `℗`, `QBZ:TID`, `DISCTOTAL`, `TRACKTOTAL`, `SUBTITLE`,
  embedded `APIC` cover art) trigger the watch's music indexer to silently
  reject the file post-write.
- **Filename:** ≤ 56 chars in the stem (≤ 60 total with `.mp3`). ASCII letters,
  digits, spaces, dashes, dots, underscores. Anything longer, the watch
  silently drops the write.
- **No ID3v1.** Pass `-write_id3v1 0` to ffmpeg.

ffmpeg invocation that produces a Garmin-clean MP3:

```
ffmpeg -y -i input.flac \
  -vn \
  -ac 2 -ar 44100 \
  -codec:a libmp3lame -b:a 192k \
  -map_metadata -1 \
  -id3v2_version 3 \
  -write_id3v1 0 \
  -metadata title=… \
  -metadata artist=… \
  -metadata album=… \
  -metadata album_artist=… \
  -metadata track=… \
  -metadata date=… \
  -metadata genre=… \
  output.mp3
```

## Album grouping

Garmin's library view groups by **`ARTIST` + `ALBUM`**, not by `ALBUM_ARTIST`.
For multi-artist soundtracks/classical albums where each track has a different
performer credit, you must set `ARTIST` to the album-wide credit (the
`ALBUM_ARTIST` value), or every track shows as its own album.

## Failure modes & their causes

| Symptom | What's actually happening | Fix |
|---|---|---|
| Upload reports OK but file doesn't appear / free space doesn't change | Watch firmware silently dropped the write post-`send_object_info`. Cause is one of: filename too long, oversized embedded album art, non-standard ID3 frames, exotic unicode in tags. The handle persists as a "broken stub" (GetObjectInfo errors). | Use the strict ffmpeg config above. Sanitize filename. |
| `Protocol error: GeneralError during DeleteObject` | Garmin firmware refuses to delete handles whose GetObjectInfo errors. | Power-cycle the watch — its GC clears broken stubs on next boot. |
| `Protocol error: GeneralError during GetObjectInfo` on listing | Folder contains a broken stub. | Use `list_objects_stream` (or per-handle `get_object_info` directly) so individual failures don't kill the whole listing. |
| MTP open times out at 30 s | One of: split-header not set; watch's MTP service is sleeping (screen off); USB mode is "Garmin sync" not MTP; lingering session from a prior crash. | Set `split_header_data(true)`; tap the watch screen to wake it; switch USB mode; replug. |
| `interface is busy (errno 16)` | Another process / GVFS holds the USB. | `gio mount -u …` if GVFS-mounted; kill any other MTP client. |
| Filenames render with weird characters on the watch | Source had non-ASCII; our sanitizer falls back to `-`. | Tag your music with the proper title in ID3 — the watch's screen reads tags, not filenames. |

## Vendor-specific MTP operations (open question)

The watch advertises 12 vendor operations: `0x9000–0x900B`, plus `0x9810`
(GetServiceIDs?) and `0x9811`. Garmin Express historically synced
iTunes/WMP playlists, almost certainly via these. We have not reverse-engineered
them.

A USB packet capture of Garmin Express writing a playlist would crack this
open. PRs welcome.

## Listing reliability

Use `session.get_object_handles` directly, then per-handle `get_object_info`,
catching errors individually. Don't rely on `storage.list_objects` (it's
all-or-nothing — one corrupt handle bails the whole listing). The streaming
variant `list_objects_stream` yields per-handle results but silently skips
broken ones — fine for display but not for cleanup, since you can't get the
broken handles' values to delete them.

## Free-space metric

`storage.info().free_space_bytes` is **firmware-cached** for a long time after
writes — sometimes minutes. Track delta locally if you need real-time. Forcing
`storage.refresh()` doesn't help on FR165.

## Useful diagnostic examples in this repo

- [`examples/diagnose.rs`](../examples/diagnose.rs) — full filesystem walk
- [`examples/test_delete.rs`](../examples/test_delete.rs) — list + delete every readable file
- [`examples/probe_vendor_ops.rs`](../examples/probe_vendor_ops.rs) — call vendor ops with no params, log responses
- [`examples/usb_inspect.rs`](../examples/usb_inspect.rs) — low-level USB probe (descriptor read, interface claim)
- [`examples/usb_reset.rs`](../examples/usb_reset.rs) — issue USB device reset
- [`examples/check_formats.rs`](../examples/check_formats.rs) — list playback_formats and operations_supported
