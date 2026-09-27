# Garmin USB/MTP Reference

Everything verified about Garmin's MTP responder, with the device and
firmware that established each fact.

## Device identification

- **USB Vendor ID:** `0x091E`
- **Music-capable watches are MTP-only**, never USB Mass Storage.
- **PIDs we've handled in person:**
  - Forerunner 165 Music: `0x5151` (FW 2506, May 2026)
- **PIDs known from `libmtp` device table** (vendor 0x091E, all flagged `DEVICE_FLAGS_ANDROID_BUGS`):
  - FR 645 Music `0x4b48`, FR 245 Music `0x4c05`, FR 945 `0x4c29`, Venu `0x4c9a`, FR 255 `0x4f98`, FR 265 `0x50a1`, FR 965 `0x50db`, plus Fenix 7/8 family and Edge 1040/1050.
  - Source: `src/music-players.h` in https://github.com/libmtp/libmtp
  - **FR 165 Music (`0x5151`) is not yet in libmtp**: we should file an upstream entry.

## Vendor-extension descriptor

- Garmin's `DeviceInfo.vendor_extension_desc` reports `microsoft.com: 1.0`.
- This is the **MS Media Transfer Protocol extensions** identifier (MS-DRMND).
- Implication: Garmin reuses an Android-flavored MTP responder (libmtp tags every Garmin device with `DEVICE_FLAGS_ANDROID_BUGS`). Quirks from that lineage apply: split-header transfers, short-data-phase reads, etc.

## Storage layout (FR165 Music verified)

- One storage: ID `0x00020001`, description **"Internal Storage"**, ~3.45 GiB.
- Root contains four writable folders, the only places music-class content can live:
  - `GARMIN/` (FW assets / activity files / Connect IQ apps)
  - `Music/` (songs)
  - `Audiobooks/`
  - `Podcasts/`
- The watch's **music app** is understood to scan `Music/` and to filter to
  entries carrying `title` + `artist`; untagged files stay on disk (visible via
  MTP) and do not appear in the music-app UI.

  **Provenance: weaker than the rest of this file.** The claim entered the repo
  on 2026-05-03 in a bulk documentation commit (`98326b8`) with no firmware
  version, no date of observation and no record of what was measured, unlike
  §6, §7 and §8, which each name a device, a firmware and a run. It is
  consistent with everything seen since and nothing contradicts the tag half of
  it, but no one wrote down the test. Treat it as the working assumption it is.

  It is also known to be **incomplete as a description of listing**: §8 records
  the music app listing 22 entries whose objects had been deleted from
  `Music/`. Whatever the app lists is therefore not purely a function of what
  `Music/` contains at the moment you look. Product copy must not harden this
  into a rule about what the watch will and will not show.

## Format codes

mtp-rs's `ObjectFormatCode` enum covers the standard codes; vendor-extension format codes (e.g. `0xBA05`) need `ObjectFormatCode::Unknown(0xBA05)`.

| Hex      | Constant                          | Used for                                  |
|----------|-----------------------------------|-------------------------------------------|
| `0x3000` | `Undefined`                       | Generic / unknown; accepted for raw       |
| `0x3001` | `Association`                     | Folder                                    |
| `0x3004` | `Text`                            | Plain text; *rejected* for playlists      |
| `0x3009` | `Mp3`                             | MP3 audio                                 |
| `0xB901` | `WmaAudio`                        | WMA                                       |
| `0xB902` | `OggAudio`                        | Ogg                                       |
| `0xB903` | `AacAudio`                        | AAC                                       |
| `0xB984` | `M4aAudio`                        | M4A / M4B                                 |
| `0xBA05` | (vendor) AbstractAvPlaylist       | Garmin playlists (all writes rejected on the FR165) |
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
[`references/go-mtpfs-pr1.md`](references/go-mtpfs-pr1.md), where the same
quirk is described independently for the Go MTP stack.

### 2. **Filename length cap ≈ 56 chars** for `/Music`

Files written into `/Music` whose `remote_name` exceeds ~60 chars are silently
discarded by post-write validation. Both `send_object_info` and
`send_object_stream` return `Ok` at the protocol layer, but no audio data
persists; only a 32 KB metadata-only "broken stub" handle remains.

Mitigation: Pelican's remote names are `pl{counter:05}-{slug}.mp3` with the
stem capped at 56 chars; the slug comes from the title with exotic characters
replaced. The audio's ID3 tags carry the real
title for the screen, so the on-disk filename is just an identifier.

### 3. **Newly-created subfolders inside `/Music` are unreliable**

`storage.list_objects(handle)` on a freshly-created `/Music/<album>/` folder
returns `Protocol GeneralError`. Pelican therefore writes every file
directly into `/Music/`. The watch builds its library view from ID3 tags, so
the flat layout is invisible to the user.

### 4. Encoding profile

Every source is transcoded with this invocation (`rebuild-plan.md`, R1):

```
ffmpeg -hide_banner -loglevel error -nostdin -i SRC -map 0:a:0 -vn \
  -map_metadata -1 -c:a libmp3lame -b:a 192k -ar 44100 -ac 2 \
  -id3v2_version 3 -write_id3v1 0 -metadata k=v… DST.mp3
```

- **`-vn`** and **`-map 0:a:0`**: no embedded art. Large art (a FLAC's
  picture block becomes an oversized ID3v2 frame) triggers silent rejection.
- **`-map_metadata -1`** plus explicit `-metadata`: only the tags Pelican
  resolved are written.
- **`-b:a 192k`**: CBR, not VBR.
- **`-id3v2_version 3`**: v2.3 only. With v2.4, whether tags appear in the
  music app is unreliable.
- **`-write_id3v1 0`**: a single tag version.

Verified on FR165 Music FW 2506, 2026-09-26: files in this profile appear in
the music app and play (`garmin-library-persistence.md` § Results).

### 5. Library grouping

The watch's library view groups by `ARTIST + ALBUM`. For multi-composer,
soundtrack or classical material, Pelican writes the album artist as the
`ARTIST` tag so the album view stays in one piece, and also writes it as
`album_artist` (TPE2). See `crates/pelican-core/src/transcode/tags.rs`.

### 6. "Broken stub" failure mode

When post-write validation rejects an upload (bad audio profile, oversized
art, filename too long, etc.) the MTP layer reports success but only a
metadata-only stub persists. These stubs:

- Have unreadable handles (GetObjectInfo returns `Protocol GeneralError`)
- Block whole-folder listings if you `collect()` instead of streaming
- Are eventually garbage-collected by the watch on its own
- **Have refused every deletion we have asked for**: every `DeleteObject`
  issued against a broken-stub handle has returned `Protocol GeneralError`.
  We have never seen one succeed. This is an observed record on one device
  and one firmware (FR165 Music, FW 2506), not a proven property of the
  protocol, so it is stated as a record rather than as "cannot". Cleanup
  that does happen is watch-side and asynchronous.

Pelican lists stubs as `‹unreadable #N›` rows (marked `stub` in `pelican
ls`), counts each one as a taken name, and never tries to delete one.

### 7. Filename-collision failure mode

Verified 2026-05-03 on FR165 Music FW 2506: when an upload's `remote_name`
matches an existing readable file in `/Music`, the watch firmware **does
not cleanly overwrite**. Instead, both files become unreadable broken
stubs (the existing file is corrupted, and the new upload also lands as
a stub).

This is distinct from the per-handle silent-rejection mode: the watch
*acknowledges* the upload and seems to start the overwrite, then leaves
both ends in a half-applied state. The same behavior was reported
independently on an FR645 in
[libmtp#307](https://github.com/libmtp/libmtp/issues/307), where the name
stayed unusable for every later write, from any operating system, until a
factory reset.

Because §6 records that no `DeleteObject` against a stub has ever succeeded,
a collision destroys a file the user already had **and** leaves wreckage that
cannot be removed. It is not a cosmetic duplicate.

#### How Pelican avoids it

Pelican never writes a name that has been used before, rather than checking
whether a name is free at the moment of writing (`rebuild-plan.md`, R3 and
R4; `crates/pelican-core/src/naming.rs`):

- Every remote name is `pl{counter:05}-{slug}.mp3`. The counter comes from
  the per-watch ledger and only rises.
- Before the first write of a run, `/Music` is listed once. Every name on the
  device, every name in the ledger, and every stub counts as taken. Each stub
  moves the counter one further, since its real name cannot be read.
- One case fold (`mtp::fold_name`) is used for every comparison, because
  `/Music` is FAT-derived and case-insensitive.
- A name is written to the ledger and fsync'd before its upload starts, so a
  crash mid-transfer still burns it.
- There is no overwrite path, no rename-on-collision branch and no delete.

Known limits (`status.md`, "What it does not do"): a stub whose hidden
counter sits beyond a gap in the numbering cannot be covered by name, and a
second machine's ledger is invisible (its names still count as `foreign`
objects). Whether the firmware treats `song.mp3` and `song.wav` as one name
is unverified; Pelican's own writes are always `.mp3` under a fresh counter,
so it matters only for foreign files.

### 8. A delete that succeeds does not clear the watch's music app

**Observation, not a conclusion.** FR165 Music · FW 2506 · 2026-09-05.

What was done: the owner selected all 22 files in the watch view of the
macOS port (which still had a delete; see `archive/macos-port.md`), pressed
Delete, and approved the confirmation.

What Pelican measured:

- Every `DeleteObject` returned `Ok`. No `DeleteFailed` event was emitted for
  any of the 22 paths, so no handle refused (contrast §6, where stub handles
  have refused every time).
- A later listing of `/Music` returned **one** entry.
- Free space had risen **78.5 MB** across the delete.

So at the MTP layer the delete did what it says: the objects are gone from
`/Music` and the storage reflects it.

What the watch showed: **the watch's own music app still listed the deleted
tracks.** The owner reports there is no control on the watch for clearing
them, and believes that is a Garmin Express function. That second part is
his report, not something this repo has verified.

#### What this already settles about *listing*

Whatever the mechanism turns out to be, one thing follows from the observation
alone: **what the music app lists is not a function of what `Music/` holds at
the moment you look.** Twenty-two entries were listed while their objects were
gone from `Music/` and the storage figures agreed they were gone.

That bounds the claim in *Storage layout* above ("scans `Music/` only, and
filters to entries with `title` + `artist`"), which describes the tag filter,
not the lifetime of an entry. The tag half is untouched by this. The word
"only" is not a rule this repo has earned, and no UI string may state it as
one.

#### The mechanism is not established

Two models both fit what was measured. **Neither is settled and nothing in
this repo distinguishes them:**

- **(a) Index-in-place with dangling entries.** The music app indexes files
  where they lie and the index was not invalidated by the MTP delete, so the
  listed entries point at objects that no longer exist. They would list and
  fail to play.
- **(b) Ingest into storage MTP cannot see.** The watch copies a file out of
  `/Music` into its own store on acceptance, so `/Music` is a drop box and
  deleting from it removes only the drop-box copy. The entries would still
  play and the space they use would stay consumed.

Reports from the session support parts of each. The one test that would
separate them was not run: **play a track the app still lists after its file
was deleted.** Under (a) it fails; under (b) it plays. Do that before writing
either model down as fact.

#### Storage figures, and what they do not show

Measured on the same device after the delete:

| Quantity | Value |
|---|---|
| Storage total | 3706.2 MB |
| Free | 2488.8 MB |
| Used (difference) | 1217.4 MB |
| Held by `/Music` + `/Audiobooks` at that moment | 16.7 MB |

**Do not read the 1200.7 MB gap as hidden music.** The owner's entire music
library is about 78 MB, so music cannot account for more than a small slice of
it. Firmware, maps, watch faces and Connect IQ apps are the ordinary
occupants of that space on this model, and none of them are visible through
the four writable folders MTP exposes. The gap is *unaccounted for by this
measurement*, which is a different statement from *occupied by files the
watch is hiding*.

#### What this means for the product

Deleting an object frees its space and nothing more, and no operation over
standard MTP is known to touch the watch's music library. Pelican therefore
has no delete (`status.md`). Once a track is on the watch, it stays in the
library until a factory reset.

If a library-management operation exists at all, the undocumented vendor
opcodes are where it would be: see `vendor-ops.md`, and the unrun probe plan
in `archive/vendor-op-probe-proposal.md`.


## Vendor opcodes

See [`vendor-ops.md`](vendor-ops.md). Probe results from FR165 Music FW 2506
are transcribed in [`archive/research-log.md`](archive/research-log.md)
§ 2026-05-03. The raw `target/probe_vendor_ops.log` the probe wrote is not in
this tree (`target/` is not committed), so the transcribed table is the
surviving record. Do not re-run the probe to regenerate it: it wedges the
session until a physical replug. See
[`archive/vendor-op-probe-proposal.md`](archive/vendor-op-probe-proposal.md).

## What does *not* work

| Attempt                                                 | Outcome                                                                   |
|---------------------------------------------------------|---------------------------------------------------------------------------|
| Send playlist as `ObjectFormatCode::Text` (0x3004)      | Silently rejected. No file appears.                                       |
| Create `/Music/<album>/` then list it                   | `Protocol GeneralError` on the new handle.                                |
| Combined-bulk MTP transfers (mtp-rs default)            | Hangs, wedges session.                                                    |
| Filenames > ~60 chars in `/Music`                       | Silent discard, broken stub left behind.                                  |
| FLAC files containing embedded art, sent via SendObject | Silent discard on watches; broken stub.                                   |
| `simple-mtpfs` writes (per Garmin forums)               | Often produces zero-byte files. Avoid this codepath for any future work.  |

## Object properties: the watch can report its own tags

Verified 2026-09-02 on **Forerunner 165 Music, firmware 2506**, read-only via
`examples/probe_objprops.rs`.

`GetDeviceInfo` reports 37 supported operations, including the full MTP
object-property set:

| Opcode | Operation | Supported |
|--------|-----------|-----------|
| `0x9801` | GetObjectPropsSupported | yes |
| `0x9802` | GetObjectPropDesc | yes |
| `0x9803` | GetObjectPropValue | yes |
| `0x9804` | SetObjectPropValue | yes |
| `0x9805` | GetObjectPropList | yes |

`GetObjectPropValue` answers for real handles in `/Music`, returning
length-prefixed UTF-16LE strings:

```
album-track-02-normalized.mp3
  Name (0xDC44)        "Ghost of Time Tognetti: Into the Fog"
  Artist (0xDC46)      "Iva Davies"
  AlbumName (0xDC9A)   "Master and Commander: The Far Side of the World
                        (Music from the Motion Picture)"
  AlbumArtist (0xDC9B) "Iva Davies"
  Duration (0xDC89)    132362 ms
  Track (0xDC8B)       2
```

This holds for files Pelican never uploaded, so it is the device's own
index and not an echo of anything we wrote.

**Why it matters.** The ledger is the source of *provenance* (what Pelican
put there), but a file Pelican did not send is not nameless: its artist and
album can be asked for directly.

**Artwork was never asked about.** `probe_objprops.rs` requests 0xDC46,
0xDC9A, 0xDC9B, 0xDC89 and 0xDC8B and nothing else. It does not call `GetThumb`
(0x100A) or ask for `RepresentativeSampleData` (0xDC81), so this repo has no
evidence either way about whether the FR165 can return a picture for an object
in `/Music`.

**Not yet done.** Nothing reads these properties at runtime. `mtp-rs` exposes
`session().get_object_prop_value()`, and `ObjectPropertyCode` carries an
`Unknown(u16)` catch-all for the audio codes above, which is how the probe
reaches them. Cost per file is one round trip per property, so a listing that
wants tags should prefer `GetObjectPropList` (`0x9805`) over N calls of
`0x9803`.
