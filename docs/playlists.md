# Playlist Sync (M3U8) — Working Recipe

Verified path for getting an M3U8 playlist onto a Garmin music watch via MTP,
based on `better-sync` (Schachte) — a Go CLI that has been doing this in
production on FR family + Venu since at least 2024.

## The recipe

1. **Find the `Music` folder** at storage root (case-insensitive). Use that
   ObjectHandle as `parent`. *Do not* try to create a subfolder for playlists.
2. Build the playlist body:
   ```
   #EXTM3U
   0:/MUSIC/<TRACK1>.MP3
   0:/MUSIC/<TRACK2>.MP3
   …
   ```
   - Header line `#EXTM3U`, single LF terminator per line.
   - Per-track lines: **uppercase**, `0:/MUSIC/...` style. `better-sync`
     supports five fallback path styles; the default (style 0/4) is the one
     above.
   - One `<track>.<EXT>` per line; no `#EXTINF` is required (better-sync's
     default writer omits them).
3. **Filename**: `<sanitized_name>.m3u8`. Sanitize to `[A-Za-z0-9 !_\-&()+.']`,
   collapse runs of `_`, max 64 chars. Case is preserved.
4. **`SendObjectInfo`** with:
   - `StorageID` = the music storage
   - `ObjectFormat` = **`0xBA05`** (`MTP_FORMAT_ABSTRACT_AV_PLAYLIST`)
   - `ParentObject` = the `Music` folder handle
   - `Filename` = the sanitized name
   - `CompressedSize` = playlist body length in bytes
5. **`SendObject`** with the body bytes.

mtp-rs note: `ObjectFormatCode` is a `num_enum`; use
`ObjectFormatCode::Unknown(0xBA05)` for the playlist format.

## What our previous attempt got wrong

```rust
// src/mtp.rs — old write_raw
let format = if lower.ends_with(".m3u8") || lower.ends_with(".m3u") {
    ObjectFormatCode::Text   // 0x3004
} else {
    ObjectFormatCode::Undefined
};
```

- Used `Text` (0x3004) — Garmin firmware silently rejects playlist writes
  with non-playlist format codes. `0xBA05` is the format the firmware looks
  for.
- Wrote a body of just filenames (`#EXTM3U\n<file>\n...`). Once the format
  code is fixed this still might not be enough; the watch needs full
  Garmin-style paths inside.

## Open questions

- Will the watch resolve `0:/MUSIC/...` paths case-insensitively to actual
  files like `Some Track.mp3`? `better-sync` uppercases everything; this is
  safest. We could try a case-preserving variant next.
- Does Garmin support `WPL` (Windows Media Playlist) format `0xBA11`? Their
  support page lists WPL/ZPL/PLS as accepted; we haven't tested.
- Is `#EXTINF:-1,<title>` lines required for the watch to show track titles
  in the playlist UI, or does it pull title from the referenced file's ID3?
  better-sync's default omits `#EXTINF`.

## 2026-05-03 · FR165 Music probe — all standard MTP variants fail

`examples/probe_playlist.rs` ran six variants in one session:

| Variant | Body                                          | Format    | Result          |
|---------|-----------------------------------------------|-----------|-----------------|
| A       | bare basenames (`repeat-1.mp3`)               | `0xBA05`  | broken stub     |
| B       | uppercase `0:/MUSIC/REPEAT-1.MP3`             | `0xBA05`  | broken stub     |
| C       | case-preserving `0:/Music/repeat-1.mp3`       | `0xBA05`  | broken stub     |
| D       | bare basenames                                | `0xBA10`  | broken stub     |
| E       | backslash-style `Music\repeat-1.mp3`          | `0xBA05`  | broken stub     |
| F       | with `#EXTINF:-1,<title>` lines, bare         | `0xBA05`  | broken stub *   |

\* F was *briefly* visible in `list_objects` as a 46-byte readable entry
(despite a 72-byte write), but downloading the bytes produced a Protocol
GeneralError and wedged the session. The "readable" appearance was almost
certainly a mid-validation race — by the next listing it would have
become a stub like the others.

`MtpDevice` returned `Ok` for all six `SendObjectInfo` + `SendObject`
sequences. The watch's post-write validator silently rejected each.

### Working hypothesis: FR165 firmware may not accept MTP playlist writes

`better-sync` is verified working against FR945 / FR255 / Venu / Forerunner
645 — older models. FR165 Music was released Mar 2024 and ships with a
post-Garmin-Connect-IQ-2.0 firmware family. Garmin's official mobile app
(both iOS + Android) syncs playlists to newer watches via Bluetooth +
Garmin Connect cloud, *not* MTP. The MTP playlist code path may simply
not exist in FR165 firmware.

Evidence for this hypothesis:
- All known playlist format codes silently rejected
- Path-style and body-content variants all fail equivalently
- Music files themselves write fine — only playlists are special-cased

### Next moves to confirm or refute

1. **Capture Garmin Express on Windows** writing a playlist to *any* music
   watch via Wireshark+USBPcap. Compare wire bytes against our attempts.
2. **Test the same `probe_playlist` against a FR945 / FR255** if accessible —
   if those work and FR165 doesn't, hypothesis confirmed.
3. **Inspect Garmin Connect mobile-app traffic** for FR165 to see how it
   delivers playlists (BLE GATT? cloud-side index?).
4. **File a libmtp upstream entry** for FR165 Music and ask the libmtp
   maintainer whether anyone has succeeded with playlists on this generation.

## References

- `better-sync` — `pkg/files/playlist.go` and `pkg/util/sanitize.go`
  (snapshot in [`references/better-sync-playlist.go.txt`](references/better-sync-playlist.go.txt))
- Garmin support: "Audio file types supported on watches"
  https://support.garmin.com/en-US/?faq=JyNEOTsZaR3KMXqej3oQp5
- Garmin support: "Manually creating an MP3 playlist with Windows Media Player"
  https://support.garmin.com/en-US/?faq=4r7FCB0Rk37hG9XrgBHbaA
- FR945 forum: M3U paths on the watch must resolve; bad paths can reboot the
  device. https://forums.garmin.com/sports-fitness/running-multisport/f/forerunner-945/191981/
