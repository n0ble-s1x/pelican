# Research Log

Dated entries — what we looked at, what we learned, why we made each
decision. New entries at the bottom.

---

## 2026-05-02 · Initial scaffold

- Decision: pure-Rust stack on `mtp-rs` + `nusb`; UI on `eframe`/`egui`.
- Found `set_split_header_data(true)` is required against Garmin. Without
  it, sessions hang. Verified on FR165 Music FW 2506.
- Found newly-created `/Music/<album>/` subfolders are unreliable to list —
  switched default behavior to flatten uploads into `/Music/` directly.
- Established the MP3 transcoding profile that lands cleanly:
  `-vn -ac 2 -ar 44100 -b:a 192k -map_metadata 0 -id3v2_version 3 -write_id3v1 0`.
  Earlier attempts with VBR / kept album art / id3v2.4 produced "broken
  stubs" (32 KB metadata-only handles).
- Diagnosed the **filename-length cap** as the actual root cause of most
  upload failures. Files in `/Music` whose `remote_name` is >~60 chars are
  silently discarded by the watch's post-write validator, regardless of
  audio profile. Truncate stems to 56 chars + `.mp3`.
- Tagged the album-fragmentation issue: emit `album_artist` as both
  `ARTIST` and `TPE2` so multi-composer albums group as one.
- Tried sending an M3U8 playlist with `ObjectFormatCode::Text`: silently
  rejected. Logged as open question for v0.3.

## 2026-05-03 · Vendor opcode probe + playlist research

### Probe results

`examples/probe_vendor_ops.rs` against FR165 Music FW 2506 (log at
`target/probe_vendor_ops.log`):

| Op       | Result          |
|----------|-----------------|
| `0x9000` | DATA phase      |
| `0x9001` | SHORT response  |
| `0x9002`–`0x900B` | TIMEOUT (5s) |
| `0x9810`–`0x9811` | TIMEOUT (5s) |

Implication: 0x9000 and 0x9001 are real ops with no-arg signatures; the
others need parameters we don't yet know. Vendor ops are not blocking
playlist work — proceed with standard ops first.

### Playlist breakthrough — `better-sync`

Discovered https://github.com/Schachte/better-sync — Go CLI that does what
Pelican does, has working playlist support on FR family + Venu since 2024.
Key recipe (snapshotted in `references/better-sync-playlist.go.txt`):

- ObjectFormat = **`0xBA05`** (MTP_FORMAT_ABSTRACT_AV_PLAYLIST), not Text.
- Filename = `<sanitized>.m3u8`, allowed chars `[A-Za-z0-9 !_\-&()+.']`,
  ≤64 chars.
- Parent = the `Music` folder handle (case-insensitive find).
- Body = `#EXTM3U\n` + per-track `0:/MUSIC/<TRACK>.MP3\n` (uppercase, with
  `0:/` device-path prefix).

### Other useful refs

- **libmtp `music-players.h`** — every Garmin device tagged with
  `DEVICE_FLAGS_ANDROID_BUGS`. FR165 Music (`0x091E:0x5151`) is **not yet
  in the table**; we should file an upstream entry.
- **go-mtpfs PR #1 (CodyJung)** — documents Garmin's split-header /
  short-data-phase quirks. We already handle the split-header side via
  `set_split_header_data(true)`.
- **Garmin support page** lists accepted formats:
  AAC, ADTS, M3U, M3U8, M4A, M4B, MP3, PLS, WAV, **WPL, ZPL** included.
  WPL (`0xBA11`) is what Garmin Express writes when using WMP.

### Decisions

1. v0.3 playlist work proceeds via **standard MTP `SendObjectInfo` +
   `SendObject` with format `0xBA05`** — mirror better-sync.
2. Vendor opcode reverse-engineering deferred — not on the playlist path.
3. Add **uppercase Garmin-style track paths** to our `playlist::serialize`.
4. Documentation directory `docs/` created; future protocol work (v0.4
   transcoding, v0.5 Win/Mac, anything beyond music) lands here.

### Source snapshots for offline reference

Saved under `docs/references/`:
- `better-sync-playlist.go.txt` — the canonical SendObjectInfo+SendObject pair
- `better-sync-sanitize.go.txt` — filename + path-style logic
- `go-mtpfs-pr1.md` — summary of the split-header / short-read fix

## 2026-05-03 (later) · FR165 playlist probe — hypothesis revised

Built `examples/probe_playlist.rs` that runs six write variants (path styles
× format codes) against the watch in one session. **All six were silently
rejected** — left broken stubs in `/Music`, none surfaced as readable
playlists. Detail in `docs/playlists.md`.

Revised hypothesis: **FR165 Music firmware likely does not accept MTP
playlist writes at all.** It's a post-Connect-IQ-2.0 watch (released Mar
2024); Garmin's modern playlist sync path is BLE + Garmin Connect cloud,
not MTP. better-sync's success was on older generations (FR945, FR255,
Venu, FR645).

Pivot for v0.3:
- Stop iterating MTP playlist-format variants on FR165 — no clear way
  forward without a wire-level reference.
- Capture Garmin Express writing a playlist (Windows VM + USBPcap) to get
  ground truth on what the protocol actually looks like.
- Test on a borrowed FR945 / FR255 if possible — if it works there and
  not on FR165, hypothesis confirmed and we should document it as a
  hardware constraint rather than a bug.
- Investigate whether Garmin Connect Mobile uses BLE for playlist sync.
  If yes, that path is gigantic in scope vs MTP and likely out-of-band
  for v0.3.

The `playlist::serialize_for_device` API (with `PathStyle` variants) is
left in place so we can flip styles cheaply once we have ground truth.

### Stub debris from this session

`/Music` accumulated 6 broken stubs from the probe. Need to clean
post-replug via `examples/wipe_music.rs` (selective wipe by name, after
a USB session reset).
