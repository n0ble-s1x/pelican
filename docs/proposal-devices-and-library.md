# Pelican after v0.2.0: several named devices, and seeing what is on them

Design research, 2026-09-27. Scope: two features the owner wants after v0.2.0.

- **A.** Register and name each device, keep its ledger with it, support several devices and several kinds (Garmin watches over MTP, HiFi Walker style DAPs over USB mass storage), each handled with its own output profile.
- **B.** Filter and sort the local library and what is on a device by song, album, artist and playlist, and show clearly what is on the device and what is not.

Grounding: this report was written against `README.md`, `docs/status.md`, `docs/garmin-mtp.md`, `crates/pelican-core/src/{garmin,watch,ledger,mtp}.rs`, `transcode/tags.rs`, `library.rs`, `crates/pelican-shell/src/{commands,device,config}.rs` and `ui/app.js`. No file in the repo was changed, and no USB device was opened. Facts marked **unverified** need a read-only check on hardware before they are relied on.

---

## A. Register and name devices; several devices and kinds

### A.0 Recommendation in one paragraph

Keep one **device registry** per machine as an append-only JSONL log (`$XDG_DATA_HOME/pelican/devices.jsonl`). Each device gets a random Pelican id, a user-given name, a kind, a profile and a pointer to its ledger. The existing `ledger-<serial>.jsonl` files are **adopted where they are, not renamed or rewritten**. Identify Garmin watches by `(USB VID 0x091E, USB iSerial)`, cross-checked against the MTP DeviceInfo serial. Identify mass-storage players mainly by a **marker file Pelican writes once on the volume** (`/.pelican/volume.json`, a random UUID and nothing personal), with USB serial, filesystem UUID and capacity as hints only, because cheap DAPs report fixed or empty serials and FAT UUIDs change on every format. In `pelican-core`, add a `FsBackend` next to the MTP one behind the existing `Backend` trait (still no delete), and move every Garmin-specific constant (MP3 192k, no art, flat `/Music`, `pl{counter}` names, 500-object cap, playlist-as-album) into a `DeviceProfile` value, so a DAP profile can say "FLAC up to 24/192, embedded baseline JPEG art, `Artist/Album/NN Title.flac`, real `.m3u8` playlists". First plug of an unknown device shows a one-screen "New device" card: detected model, kind, a name field prefilled with the model, the profile, and one Register button.

### A.1 Identifying a device stably and uniquely

#### What each identifier is, and how far it can be trusted

| Identifier | Where it comes from | Stable across | Unique? | Notes for Pelican |
|---|---|---|---|---|
| USB VID:PID | Device descriptor, via `nusb` (already read in `garmin::list_devices`) | Everything | No: identifies a model, not a unit | Garmin VID `0x091E`. PID is per model (FR165 Music `0x5151`; fenix 8 `0x51b6`/`0x51b8`/`0x51b5`, FR970 `0x51d5` in the [libmtp device table](https://github.com/libmtp/libmtp/blob/master/src/music-players.h)). Useful for kind detection, never for identity. |
| USB iSerial | String descriptor `iSerialNumber`, `nusb::DeviceInfo::serial_number()` | Reboots, factory reset (expected), other computers | On Garmin: yes in practice. On cheap devices: often not | This is today's ledger key. Read as a hexadecimal number, the owner's serial gives a 10-digit decimal, the shape of a Garmin **Unit ID**. **Unverified hypothesis**: the watch's iSerial is its Unit ID in zero-padded hex. Check under Settings, System, About on the watch. If true, the serial is printed on the device and survives factory reset, which is ideal. |
| USB iProduct | String descriptor | Everything | No | The FR165 sets `iProduct = 0` (see the comment on `Backend::model` in `mtp.rs`), so the model name only exists over MTP. |
| MTP DeviceInfo `Manufacturer`, `Model`, `DeviceVersion`, `SerialNumber` | `GetDeviceInfo`, cached by mtp-rs at open (`MtpDevice::device_info()`, fields in `mtp-rs 0.13.2 src/ptp/types/device.rs`) | Model yes; DeviceVersion changes with firmware; SerialNumber should be stable | SerialNumber: should be, per spec | Pelican reads `model` today and prints `serial_number` only in `examples/diagnose.rs`. Nothing records whether Garmin's MTP SerialNumber equals the USB iSerial. **Unverified.** Record both at registration and cross-check at every open. `DeviceVersion` is the firmware string, worth storing per session (for the compatibility table), never as identity. |
| MTP device property `DeviceFriendlyName` (`0xD402`) | `GetDevicePropValue`, if `0xD402` is in `DeviceInfo.device_properties_supported` | Until the owner renames it | No | Defined in the MTP spec and used by Windows ([NXP MTP Responder Development Guide](https://www.nxp.com/docs/en/supporting-information/MTP_RESP_DEV_GUIDE.pdf)); libmtp reads it only after checking support (`LIBMTP_Get_Friendlyname` in [libmtp.c](https://github.com/libmtp/libmtp/blob/master/src/libmtp.c)). mtp-rs has no named constant for it but `DevicePropertyCode::Unknown(0xD402)` with `session().get_device_prop_value()` reaches it. **Whether Garmin firmware lists `0xD402` is not recorded anywhere in the repo.** The check is free: `device_properties_supported` is already in the cached DeviceInfo, so a read-only line in `examples/diagnose.rs` answers it the next time the owner runs it. Recommendation: at most show it as a suggested name. **Never write it** (`SetDevicePropValue` is a device write outside the ledger, for no gain; the name belongs on this machine). |
| MTP `SynchronizationPartner` (`0xD401`) | Same | | | Windows Media Player's sync-partner string. Not useful here; do not write it. |
| USB port path (`bus-port.port`) | sysfs, `nusb` `port_chain`; mtp-rs `location_id` | Only while the cable stays in the same port | Per port | Useful only to tell two identical devices apart *during one session* (for example to open the right one). Never stored as identity. udev maintainers make the same point about identical hardware ([LKML: stable identification of identical USB hardware](https://lkml.iu.edu/hypermail/linux/kernel/0706.2/0867.html)). |
| Filesystem UUID (FAT volume serial) | `/dev/disk/by-uuid`, udev `ID_FS_UUID` | Until the card is reformatted | Random 32 bits, fine among a few devices | FAT32's "UUID" is the volume serial in the boot sector and is **regenerated on every format** ([minio/directpv#602](https://github.com/minio/directpv/issues/602); [kirsle.net on FAT serials](https://www.kirsle.net/get-fat-drive-serial-numbers-in-unix)). Good hint, bad key. |
| Filesystem label, capacity | udev `ID_FS_LABEL`, `statvfs` | Label until renamed; capacity until card swap | No | Hints. |
| Pelican marker file on the volume | Written by Pelican once, at registration | Until the card is erased | Yes (random UUID v4) | The strongest identity a mass-storage player can have. See below. |

#### Precedent: how Strawberry (the Clementine fork) does it

Strawberry keys a udisks2 device as `Udisks2/<serial>/<vendor>/<model>/<capacity>/<uuid>` ([udisks2lister.cpp, `PartitionData::unique_id`](https://github.com/strawberrymusicplayer/strawberry/blob/master/src/device/udisks2lister.cpp)) and a GIO volume by filesystem UUID, falling back to filesystem size ([giolister.cpp](https://github.com/strawberrymusicplayer/strawberry/blob/master/src/device/giolister.cpp)). It stores `unique_id, friendly_name, icon, transcode_mode, transcode_format` per device in a `devices` table and keeps a separate song table per device ([devicedatabasebackend.cpp](https://github.com/strawberrymusicplayer/strawberry/blob/master/src/device/devicedatabasebackend.cpp)). Disconnected devices stay listed as "Remembered" until the user chooses Forget ([devicemanager.cpp](https://github.com/strawberrymusicplayer/strawberry/blob/master/src/device/devicemanager.cpp)). That is almost exactly the shape the owner is asking for. Its weakness is the one Pelican must avoid: because the key includes the FAT UUID and capacity, reformatting or swapping the card silently creates a "new" device. For Pelican that would mean a new empty ledger, which is harmless on mass storage (no name-reuse hazard) but confusing, so Pelican should ask instead of silently forking.

#### Recommended identity rules

**Garmin (MTP):**

- Primary key: `usb:091e:<iSerial>`. This is what the ledger is already keyed by, so migration is free.
- At registration also record: PID, MTP `Model`, MTP `SerialNumber`, `DeviceVersion`, and whether `0xD402` is supported.
- At every open: if the MTP SerialNumber disagrees with the recorded one for the same iSerial, stop with an explanation rather than guess. (Cheap to add, since DeviceInfo is already cached.)
- A Garmin with **no** iSerial stays refused, as today (`serial_of` in `pelican-shell/src/commands.rs`). A marker file is not an option on Garmin: a stray file on the watch is a name the ledger does not own, and the watch's writable folders are all special.
- Two Garmin watches with the same serial have never been reported and would violate Garmin's own Unit ID scheme; treat it as an error ("two devices claim to be X").

**Mass-storage DAP:**

- Primary key: the UUID in `/.pelican/volume.json` on the volume root, written with `create_new` at registration, fsync'd with its directory. Content: `{"v":1,"volume":"<uuid v4>","created":"<rfc3339>"}`. No name, no hostname, no user data: whoever finds the card learns only that Pelican used it.
- Hints stored in the registry: USB VID, PID, iSerial (may be empty or shared), FAT UUID, FS label, capacity.
- Matching, in order:
  1. Marker present and known here: that device. Hints that differ (new FAT UUID impossible with a marker; a different USB serial means the card moved to another player) are updated with a line in the registry and shown once ("this card is now in a different player").
  2. Marker present, unknown here: the card was registered on another computer. Offer to adopt it under the same id (see A.4).
  3. No marker, hints match one registered device: "Looks like Mav's HiFi Walker mini, but its card has no Pelican marker. Was it erased or replaced?" (see A.4).
  4. No marker, no match: new device.
- Empty or duplicated iSerial is simply ignored as a discriminator. Cheap devices share or omit serials routinely ([udisks#490, two devices with identical serial](https://github.com/storaged-project/udisks/issues/490); [hbrobotics thread on identical profiles](https://groups.google.com/g/hbrobotics/c/QgJ84KHnrYs)). A heuristic blacklist (all zeros, `0123456789ABCDEF`, strings equal to another attached device's) marks a serial as "not unique" in the registry so it is never used for matching.

Important consequence for the HiFi Walker family: the H2 and H2 mini have **no internal storage**; the library lives on the microSD card ([H2 mini product page](https://hifiwalker.com/products/hifi-walker-h2-mini): "Internal: None"; H2 transfer guide: without a card "your computer may not recognize the player as a storage drive", [hifiwalker.com](https://hifiwalker.com/blogs/hifi-walker-h2/transfer-music-to-hifi-walker-h2)). A marker on the card means Pelican follows **the card**, which is the right thing: the ledger describes what is on that card. The UI should say so once, in the registration card for this kind.

#### Where to find mass-storage players on Linux without new daemons

`places.rs` already reads `/proc/self/mounts` and knows `/run/media/$USER` and `/media/$USER`. From a mount point, the block device's sysfs path leads up to the USB interface, where `idVendor`, `idProduct` and `serial` are plain files; `/dev/disk/by-uuid` gives the FS UUID. All read-only, no udisks D-Bus dependency, no `unsafe`. Pelican does not mount anything itself; the desktop's automount does, as it does today for drives in Places.

### A.2 What the devices actually present

#### HiFi Walker H2 (original)

Sources: the official manual PDFs ([manual 1](https://images-na.ssl-images-amazon.com/images/I/91HvuworOAL.pdf), [manual 2](https://m.media-amazon.com/images/I/91TUP7ZOpzL.pdf)) and the [official transfer guide](https://hifiwalker.com/blogs/hifi-walker-h2/transfer-music-to-hifi-walker-h2).

- **USB:** USB Mass Storage. "System Setting, USB Mode, USB (not DAC)". The manual's FAQ: "This functions more like an external hard drive... you simply drag and drop your music." Not MTP.
- **Formats (manual table):** DSF/DFF DSD64 (2.8 MHz), WAV 192 kHz, APE 192/24, FLAC 192/24, AIFF 384/32, WMA 192/24, MP3 48/16, OGG 48/16, AAC 48/16. "DTS coded WAV files are not supported"; APE "insane" compression is not supported.
- **Library:** "Music Scan: scan the entire Micro SD card and build your music library", Automatic (on every card insert) or Manual. Category views Songs, Album, Genre, Artist, each marked "(ID3 tag info required)". Folder browsing via Explorer works without tags.
- **Playlists:** `.m3u` and `.m3u8`. "Only *.m3u/*.m3u8 file needs to be at the root folder of the SD card. The folder that contains your target music doesn't have to be at the root folder." They appear under Explorer, not Category. The vendor recommends foobar2000 to make them. **Path rules are not documented** (relative vs absolute, `/` vs `\`, encoding). Must be tested on the device.
- **Album art: contradictory.** The manual has an "Album cover: press Play/Pause to turn on" setting, while the same manual's FAQ says "The player does not support any album art." A manuals.plus copy of an H2 manual (403 to fetch directly, seen only as a search snippet) says the cover must be a JPG **with the same file name as the track** (`testsong111.mp3` next to `testsong111.jpg`). A 2022 review says "album art generally looks clean and vibrant" ([Audiophile Heaven](https://www.audiophile-heaven.com/2022/02/hifi-walker-h2-music-player-retro-chinese-dap.html)). Likely firmware-dependent. **Unverified; test on the owner's unit.**
- **Rockbox:** the H2 is a rebadged AIGO Eros Q and runs Rockbox's `erosqnative` build, dual-boot, FAT32 card required ([HiFi Walker Rockbox guide](https://hifiwalker.com/blogs/hifi-walker-h2/hifi-walker-h2-rockbox-guide); [Rockbox forum thread](https://forums.rockbox.org/index.php?topic=54764.0)).

#### HiFi Walker H2 mini ("Mav's HiFi Walker mini" is probably this)

Sources: [product page](https://hifiwalker.com/products/hifi-walker-h2-mini), [transfer guide](https://hifiwalker.com/blogs/dap-guides-tips/download-music-to-h2-mini-2026-step-by-step-guide-for-flac-dsd-mp3-files), [format guide](https://hifiwalker.com/blogs/dap-guides-tips/h2-mini-audio-format-guide-flac-dsd-pcm-optimization-2026).

- **USB:** mass storage ("choose the USB mode to 'Storage'"; appears as "a removable drive"). microSD only, up to 512 GB, 64 GB card included. 1.54 inch 240x240 touch screen.
- **Formats:** product page lists DSF/DFF, FLAC, WAV, ALAC, APE, MP3; "PCM up to 384 kHz / 32 bit", "Native DSD256". The vendor's own format guide contradicts it: "Native FLAC up to 24-bit/192kHz", DSD64/DSD128 via DoP, "files encoded at 32-bit/384kHz will be downsampled". Treat **24/192 FLAC** as the safe ceiling and DSD as copy-only.
- **Library:** "Settings, Library, Update Library to force rescan". Tags matter: "album art appears in the player's grid view" once files are tagged; the guide recommends embedding metadata (Mp3tag) before transfer. It recommends `/Music/Artist/Album/` and warns against nesting deeper than 4 levels. It suggests libraries up to about 10,000 tracks.
- **Playlists:** not documented for the mini. Assume the H2's `.m3u8` at card root until tested.
- **Album art:** embedded art is implied by the "grid view" remark; no statement on `cover.jpg`/`folder.jpg`. **Unverified.**
- Caveat: these hifiwalker.com blog pages read as marketing copy and disagree with each other. Every profile limit below is a starting value to be confirmed on the owner's unit.

#### Rockbox (if the owner ever dual-boots the H2)

From the Rockbox manual sources on GitHub:

- **Album art** ([album_art_info.tex](https://github.com/Rockbox/rockbox/blob/master/manual/appendix/album_art_info.tex)): search order is embedded JPEG (ID3v2 or MP4 only), `./<filename>.jpg`, `./<albumtitle>.jpg`, `./cover.jpg`, `./folder.jpg`, `/.rockbox/albumart/<albumartist>-<albumtitle>.jpg`, then `../<albumtitle>.jpg`, `../cover.jpg`. **No progressive or multi-scan JPEG**, only baseline. Embedded art in FLAC is not in the list, so for FLAC a `cover.jpg` beside the tracks is what works.
- **Database** ([tagcache.tex](https://github.com/Rockbox/rockbox/blob/master/manual/rockbox_interface/tagcache.tex)): initial scan in the background; "Auto Update" on boot; "Update Now" detects new and deleted files; `database.ignore`/`database.unignore` per folder.
- **Playlists** ([apps/playlist.c](https://github.com/Rockbox/rockbox/blob/master/apps/playlist.c)): `.m3u8` is UTF-8, relative paths are resolved against the playlist's directory.

So one "DAP lossless" profile that writes embedded baseline JPEG **and** a `cover.jpg` per album folder covers the native firmware (likely) and Rockbox (certain).

#### Garmin fenix / epix / Forerunner Music families vs the FR165

- **Same responder family.** Every Garmin entry in libmtp, from the FR645 Music to the fenix 8, FR970 and Venu 4, carries `DEVICE_FLAGS_ANDROID_BUGS` ([music-players.h](https://github.com/libmtp/libmtp/blob/master/src/music-players.h)). The FR165 (`0x5151`) is still not in that table; the fenix 9 is too new to be. Music watches are MTP-only (`docs/garmin-mtp.md`).
- **Storage:** FR165 Music about 3.45 GiB usable (repo measurement). FR965 32 GB; epix Gen 2 16 GB base, 32 GB sapphire ([DC Rainmaker](https://www.dcrainmaker.com/2023/03/forerunner-differences-detailed.html)). fenix 9 series: **64 GB** across the board ([DC Rainmaker fenix 9 review summary](https://www.dcrainmaker.com/2026/09/garmin-fenix9-series-pro-inreach-solar-in-depth-review.html), announced 2026-08-25, [Garmin newsroom](https://www.garmin.com/en-US/newsroom/press-release/outdoor/garmin-expands-its-flagship-performance-smartwatch-lineup-with-fenix9-and-fenix9-pro/)). DCR says music on the fenix 9 is "all identical" to earlier fenix models.
- **Formats:** Garmin's FAQ for music watches lists AAC, ADTS, M3U, M3U8, M4A, M4B, MP3, PLS, WAV, WPL, ZPL ([Garmin support](https://support.garmin.com/en-US/?faq=JyNEOTsZaR3KMXqej3oQp5)); the fenix 8 and 7 manuals say "such as .mp3 and .m4a" ([fenix 8 manual](https://www8.garmin.com/manuals/webhelp/GUID-EECCAC99-90D6-4AB1-9A3A-EC433D3365E2/EN-US/GUID-CD4439DF-46FF-4279-A8D5-8DA61C87A4EB.html)). No lossless beyond WAV, no FLAC. Pelican's MP3 192k profile stays right for the whole family.
- **What is not known for other models:** the 500-track ceiling (`watch::MAX_OBJECTS`) is FR165/FR645-era; the FR645 Music is documented at 500 songs ([Wikipedia, Garmin Forerunner](https://en.wikipedia.org/wiki/Garmin_Forerunner)). No source found for a fenix 8/9 personal-music track limit. The 56-character name cap, the subfolder failure, the collision-stub behavior, and playlist rejection are all FR165 FW 2506 facts. The fenix 9 speaker models are a new playback path but not a new transfer path.
- **Recommendation:** one `garmin-mp3` profile for the whole family, with `max_objects` and `name_max` as profile fields (500 and 56 by default) and a per-model "verified" flag that the UI shows ("Unverified model: Pelican uses the Forerunner 165's rules"), matching the Compatibility table in the README.

### A.3 Data model

#### The registry: `$XDG_DATA_HOME/pelican/devices.jsonl`

Append-only, one JSON object per line, fsync'd, mode 0600, same parser discipline as the ledger (an unparsable line is a hard error naming the line). Mutable state is derived by folding the lines, so nothing is ever rewritten. A JSONL log over `devices.json`/TOML because:

- it matches the ledger's rules and code (lock, fsync, corruption is fatal);
- rename and profile changes keep their history, which answers "what was this called when I sent that?";
- no atomic-rewrite code path to get wrong.

Events (all with `v`, `at`, `device`):

```json
{"v":1,"at":"2026-10-02T18:00:00Z","event":"register","device":"d_7f3c…","name":"Mav's Garmin 165","kind":"garmin_mtp","profile":"garmin-mp3-192","ledger":"ledger-0000abcd1234.jsonl","identity":{"usb_vid":"091e","usb_pid":"5151","usb_serial":"0000abcd1234","mtp_serial":"…","mtp_model":"Forerunner 165 Music"},"adopted":true}
{"v":1,"at":"…","event":"register","device":"d_91aa…","name":"Mav's HiFi Walker mini","kind":"mass_storage","profile":"dap-lossless","ledger":"ledger-d_91aa….jsonl","identity":{"volume":"3b2e…(marker uuid)"},"hints":{"usb_vid":"…","usb_pid":"…","usb_serial":null,"fs_uuid":"1A2B-3C4D","capacity":63864569856}}
{"v":1,"at":"…","event":"rename","device":"d_7f3c…","name":"Mav's FR165"}
{"v":1,"at":"…","event":"profile","device":"d_91aa…","profile":"rockbox-lossless"}
{"v":1,"at":"…","event":"hints","device":"d_91aa…","hints":{"fs_uuid":"5E6F-7A8B"},"reason":"card re-marked after erase"}
{"v":1,"at":"…","event":"retire","device":"d_7f3c…","reason":"sold"}
```

- `device` is a random Pelican id (`d_` plus 128 random bits in hex). It is never derived from device-controlled text, so it is always a safe filename.
- `name`: user text, control characters stripped (`garmin::strip_control`), trimmed, length cap (say 64 chars), not unique-constrained but the UI warns on a duplicate. Names never become paths.
- `ledger`: a file name relative to the data dir. New devices get `ledger-<device id>.jsonl`. Adopted Garmin ledgers keep `ledger-<serial>.jsonl`.
- **"Last seen" is not stored in the registry**, to keep the log from growing with every plug. It is derived: the newest ledger event, plus an in-memory "seen this session". If the owner wants a durable last-seen, put it in a small cache file (`$XDG_CACHE_HOME/pelican/seen.json`) that may be lost without harm.
- `retire` hides a device from pickers; its ledger stays, and a retired device plugged in again is offered "Bring back Mav's FR165?".

#### Migration from `ledger-<serial>.jsonl`, with no data loss

1. On start, if `devices.jsonl` does not exist, list `ledger-*.jsonl` in the data dir.
2. For each, append one `register` line with `"adopted": true`, `kind: garmin_mtp` (every existing ledger is Garmin), `profile: garmin-mp3-192`, `ledger` pointing at the **existing file name**, `identity.usb_serial` from the file name, and `name: null`.
3. Take the ledger's shared lock while reading its header for the model name if present (it is not today; the name prompt fills it later). Do not touch its bytes.
4. A device with `name: null` is shown as its model plus "Name this watch" the next time it is plugged in.

Properties: the ledger file is never renamed, moved or rewritten, so an older Pelican still finds it by serial (rollback-safe, unlike the `reset` event change in `status.md`); a crash mid-migration leaves a registry that the next start completes (step 1 becomes "any `ledger-*.jsonl` not referenced by a `register` line"); running it twice registers nothing twice.

Ledger line additions (optional fields, `v` stays 1, older builds ignore unknown fields because `Event` has no `deny_unknown_fields`):

- `album_artist`, `track`, `disc`, `genre`, `year`: what was written, so feature B can group without re-reading sources.
- `mix`: the playlist name when the track was sent as part of a playlist (today a mix is only recognizable as `album` plus the fact that album artist was "Various Artists", which the ledger does not record).
- `profile`: the profile id used, so a DAP ledger can tell FLAC passthrough from a transcode.
- `path`: for mass storage, the relative path on the volume (for Garmin `remote` already is the name in `/Music`).

Any new **event kind** (for example `adopt`) would be refused by older builds, as `reset` is. Prefer fields to kinds.

#### Kind and profile abstraction in `pelican-core`

Today the Garmin rules are spread across `garmin.rs` (VID, `MUSIC_FOLDER`), `watch.rs` (`MAX_OBJECTS`), `naming.rs` (counter names, 56-char cap), `transcode` (the one ffmpeg invocation), `tags.rs` (strict seven-field ID3v2.3 allowlist, no art) and `transfer.rs`. The proposal gathers them into data:

```rust
pub enum Kind { GarminMtp, MassStorage }

pub struct DeviceProfile {
    pub id: &'static str,              // "garmin-mp3-192", "dap-lossless", "rockbox-lossless"
    pub audio: AudioPolicy,
    pub art: ArtPolicy,
    pub tags: TagPolicy,
    pub layout: Layout,
    pub playlists: PlaylistPolicy,
    pub limits: Limits,
    pub after_send: &'static str,      // "Unplug; the watch indexes on its own" / "On the player: Settings, Library, Update Library"
}

pub enum AudioPolicy {
    /// Everything to one lossy format (today's R1).
    TranscodeAll { codec: Mp3Cbr { kbps: 192 }, rate: 44_100, channels: 2 },
    /// Lossless stays lossless up to a ceiling; lossy is copied; unsupported lossy is transcoded.
    Lossless { max_rate: 192_000, max_bits: 24, copy_dsd: bool, passthrough: &'static [Ext], lossy_fallback: Mp3Cbr { kbps: 320 } },
}
pub enum ArtPolicy { None, Embedded { max_px: u32, baseline_jpeg: bool, also_cover_jpg: bool } }
pub enum TagPolicy { StrictId3v23Allowlist, NativeAllowlist /* Vorbis for FLAC, ID3v2.3 for MP3 */ }
pub enum Layout { FlatCounter { folder: &'static str, name_max: usize }, ArtistAlbum { root: &'static str } }
pub enum PlaylistPolicy { AsAlbum, M3u8AtRoot { relative: bool, separator: char } }
pub struct Limits { pub max_objects: Option<usize>, pub reserve_bytes: u64 }
```

Backends:

- Keep `mtp::Backend` as the device API. It already has exactly the needed surface (`list_dir`, `upload`, `free_space`, `download_file`, `ensure_folder`, `model`) and, deliberately, no delete. Rename it `Backend` in a `device` module if convenient.
- Add `fs::FsBackend { root: PathBuf }` implementing it:
  - `upload`: `OpenOptions::create_new(true)` into the final path (never overwrite), stream, `sync_all` the file and its directory. If the name exists, the planner picks another (`… (2).flac`); the backend never replaces.
  - `download_file`: the proof step needs the bytes **from the card, not the page cache**. After `sync_all`, drop the file's cached pages with `posix_fadvise(DONTNEED)` via `rustix::fs::fadvise` (a safe wrapper, so the workspace's `unsafe` ban holds), then read. Document the same residual unknown as for MTP: a device-side controller cache could still answer.
  - `free_space`: `statvfs` through `rustix`.
  - Path safety: every remote path is built by Pelican from sanitized components under `root`; reject `..`, absolute paths, and symlinks when resolving; FAT rules (no `"*/:<>?\|`, no trailing dot or space, 255 UTF-16 units per component, case-insensitive compare with the existing `fold_name`).
- `naming.rs` becomes per-layout. `FlatCounter` is today's code, unchanged. `ArtistAlbum` builds `Music/<Album Artist>/<Album>/<disc>-<track> <Title>.<ext>` and is still recorded with a `reserve` line before the write, so the ledger stays the complete record.
- The planner (`transfer::plan_with`) takes a `&DeviceProfile`; the transcode step picks `copy`, `ffmpeg flac` or `ffmpeg mp3` per file; the art step (new) extracts, scales and re-encodes cover art to baseline JPEG with ffmpeg (`-vf scale=…` and mjpeg, which writes baseline) and embeds it, plus `cover.jpg` once per album folder when asked.
- Playlists on a DAP become **real playlists**: an `.m3u8` at the card root referencing files already on the card, so a song is not duplicated per playlist (unlike Garmin's playlist-as-album). The playlist file is a new file each time (`<name>.m3u8`, then `<name> (2).m3u8` on a resend), never an overwrite; its hash is proven like any file and it has its own ledger line.
- Rules kept intact: nothing leaves the machine (no network, the marker has no personal data); append-only ledger and registry; no delete in either backend; the reserve-before-write rule applies to both.

Proposed initial profiles:

| Profile | For | Audio | Art | Layout | Playlists |
|---|---|---|---|---|---|
| `garmin-mp3-192` | All Garmin music watches | Today's R1, unchanged | None | Flat `/Music`, `pl{counter:05}-slug.mp3`, 56-char stem | As album, "Various Artists" |
| `dap-lossless` | HiFi Walker H2 / H2 mini native firmware | FLAC copied if at most 24/192 and tags clean, else ffmpeg FLAC resampled to the cap; WAV/AIFF/ALAC/APE to FLAC; MP3/AAC copied; Opus (not in the H2 list) to MP3 320; DSD copied only if the profile allows | Embedded baseline JPEG at most 500 px, plus `cover.jpg` | `Music/Artist/Album/NN Title.ext` (4 levels, the vendor's advice) | `.m3u8` at root, relative paths |
| `rockbox-lossless` | H2 on Rockbox, other Rockbox players | As above | `cover.jpg` baseline (Rockbox ignores FLAC-embedded art) | Same | `.m3u8`, relative |
| `generic-mp3` | Unknown mass-storage player the owner opts in | MP3 V0 or 320 | Embedded baseline JPEG | Same | `.m3u8` |

### A.4 First-plug registration flow and edge cases

#### The normal path

1. `status` sees a device. Pelican computes its identity and looks it up in the registry.
2. **Known device:** the header shows its name ("Mav's Garmin 165") with the model underneath in small type. Nothing else changes from today's flow. Every send button names the target: "Send 14 tracks to Mav's Garmin 165".
3. **New device:** a single "New device" card instead of the watch screen:
   - What Pelican found: model ("Forerunner 165 Music" or "USB drive, 59.5 GB, label H2MINI"), kind in plain words ("Garmin watch" or "Music player (USB drive)"), and the last four characters of the serial so two identical watches can be told apart.
   - **Name** field prefilled with the model; the owner types "Mav's Garmin 165". Enter registers.
   - **Profile**, preselected by kind with one sentence of consequence: "MP3 at 192 kbps, no album art: what Garmin watches play reliably" or "Lossless FLAC up to 24-bit/192 kHz with album art". A "Change" link reveals the other profiles.
   - For mass storage only: "Pelican will add a small hidden folder `.pelican` to the card so it can recognize it again. It holds a random number, nothing else."
   - One primary button: **Register**. A secondary "Not now" still allows read-only views (what is on it), but **sending requires registration**, because the ledger needs a home.
4. **Adopted legacy watch** (migration left `name: null`): the card says "You have sent 33 songs to this watch from this computer. Give it a name." Same form.

#### Edge cases

| Case | What Pelican sees | Behavior |
|---|---|---|
| Two of the same model (two FR165s) | Same PID, different iSerial | Two registrations. The registration card shows the serial tail and, if both are plugged in, asks the owner to unplug one and register them one at a time. Ambiguity at send time is impossible because the send is bound to the registry id of the device the last `status` saw (today's `Last.serial` rule, generalized). |
| Two identical DAPs, empty or identical serials | No marker on either | The first one registered gets a marker; the second is then unambiguously "new". If both are plugged in unregistered, register one at a time. |
| Garmin factory reset | Same iSerial (expected; **verify** once after a real reset), `/Music` empty | Same registry entry and name. The existing Start over flow (`reset::reset_ledger`) closes the ledger epoch. Nothing new needed. |
| DAP card erased or replaced | No marker; USB hints (and maybe capacity) match a registered device | Ask: "This looks like Mav's HiFi Walker mini, but its card is new or was erased. Is it the same player?" **Yes:** Pelican checks that none of the ledger's current-epoch paths exist on the card (the analog of `reset_ledger`'s check), appends a `reset` line to that ledger, writes a new marker and a `hints` registry line. If files from the ledger are still there (the card was merely re-marked or the marker deleted by hand), it re-marks without a reset. **No:** register as a new device. |
| Card moved to a different player | Marker known, USB hints differ | Same device (the card is the library). Update hints with a registry line; say so once. |
| Same Garmin on a second computer | Unknown iSerial there | Registered fresh on that machine; its ledger is empty, and the first machine's names still count as taken because they are on the device as `foreign` objects (`status.md`). Offer **"Import from another computer…"**: a file picker for a copied `devices.jsonl` and ledger file (moved by the owner on a USB stick; Pelican never networks). Import appends a `register` line and copies the ledger only if no local ledger exists for that device; if both exist, the foreign ledger is attached **read-only** as an extra source of taken names and a higher `max_counter` floor, never merged into the local file. |
| Same DAP on a second computer | Marker present, unknown id | Offer to adopt the id from the marker so both machines agree on identity. Optional (owner's choice, open question): mirror the ledger onto the card as `/.pelican/ledger-<machine>.jsonl`, append-only, so each computer can read the others' history. On mass storage there is no name-reuse hazard, so a missing mirror is harmless; it only improves "what is on it". |
| Unknown MTP device (a phone) | MTP, VID not Garmin | Not offered as a target: "Pelican sends to Garmin watches and USB music players. This device is not supported." No session is opened. |
| Unknown Garmin model | VID `0x091E`, PID not in Pelican's table | Registered as `garmin_mtp` with the default profile and an "Unverified model" badge linking to "Verifying a new watch" in the README. |
| Arbitrary USB drive | Mass storage, no marker, no match | **Never offered automatically** (Pelican must not treat every thumb drive as a player). A Places entry for a removable drive gets a "Use as a music player…" action, which starts registration with a profile choice. Known DAP VID:PIDs can be offered proactively once the owner's H2 mini's IDs are recorded. |
| Garmin stuck in serial mode (`091e:0003`) | Same serial, wrong PID | Today's wedge message, now with the device's name: "Mav's Garmin 165 isn't answering. Unplug it…". |
| Device without a usable Garmin serial | No iSerial | Refused as today. |
| Renamed or retired | | Rename from the device header menu (appends `rename`). "Forget this device" appends `retire`, keeps the ledger, and is undone automatically by the "Bring back?" prompt on next plug. |

---

## B. Organize and understand music: what is there and what is not

### B.0 Recommendation in one paragraph

Add one **Music** view with a scope switch (**Library**, **On Mav's Garmin 165**, **Compare**), four groupings as tabs (**Albums**, **Artists**, **Songs**, **Playlists**), a presence filter (**All**, **On device**, **Not on device**, **Partly**), a single search box that accepts plain words and a few `field:value` tokens, and a sort menu per grouping. Every album row carries a text-first presence mark ("12 of 12", "7 of 12", "none") with a small segmented meter. Selecting what is missing and pressing **Send** hands the selection to the existing Review step. Pelican is additive, not a mirror: the view never offers "remove from device", and its copy says so. The index is built and queried in Rust (a cache of tags keyed by path, size and mtime, plus the ledger and the last device listing), and the page renders only the visible rows.

### B.1 How comparable tools present this

- **iTunes / Apple Devices sync.** Choose "Entire music library" or "Selected artists, albums, genres, and playlists", then tick items in lists organized by Artists, Albums, Genres or Playlists ([Apple support](https://support.apple.com/en-kz/guide/devices-windows/mchlbf6a1fab/windows)). The device summary shows a capacity bar split by content type. Lesson: the four groupings the owner named are the canonical set; ticking at group level is the natural unit. Unlike Pelican, iTunes sync is a mirror: unticking removes from the device.
- **MusicBee.** Devices appear in a Devices node in the left sidebar (MTP, USB drives, SD cards); "files from the selected locations are compared to the equivalent files on the device. If the file is not already on the device it is copied" ([MusicBee wiki, Devices](https://musicbee.fandom.com/wiki/Devices)). Per-device settings cover format conversion and folder naming. Lesson: per-device profile plus a compare step is the expected mental model.
- **foobar2000.** A text query language over the library: `artist IS x AND title HAS y`, uppercase keywords, case-insensitive values ([Hydrogenaudio: Query syntax](https://wiki.hydrogenaudio.org/index.php?title=Foobar2000%3AQuery_syntax)); its Facets component gives column filters (Album Artist, Album). Lesson: power users like a few field tokens, but plain words must just work.
- **Clementine / Strawberry.** Devices tab on the left; double-click opens a device library that is browsed with the same grouping as the local collection ("group your library by artist, album, year, or genre, or a combination"); "Copy to device" from any list; device properties set the conversion format ([Clementine wiki, Portable Devices](https://github.com/clementine-player/Clementine/wiki/Portable-Devices)). Remembered devices remain listed when unplugged (Strawberry `DeviceManager`, see A.1). Lesson: the device view should be the **same component** as the library view, just scoped, and should work for a disconnected device from its stored state.
- **gPodder.** Filesystem or MTP devices, optional per-podcast folders, optional `.m3u` playlists written on the device, and a local record of which episodes were synced ([gPodder user manual](https://gpodder.github.io/docs/user-manual.html)). Lesson: "on device" is best answered from a local record (Pelican's ledger), with the device listing as confirmation.
- **Rockbox database.** Tag-driven navigation Artist, Album, Track, plus Genre and Year, and search; built from the tags of files on the device, updated on demand ([tagcache.tex](https://github.com/Rockbox/rockbox/blob/master/manual/rockbox_interface/tagcache.tex)). The H2's native menus are the same shape: Songs, Album, Genre, Artist, each requiring tags (H2 manual). Lesson: what the player will show is a function of **tags**, so Pelican's device view should group by the tags Pelican wrote, not by file paths.

### B.2 Proposed UX for Pelican's plain HTML/JS UI

#### Placement

A new top-level view, **Music**, reachable from the watch screen and from Choose. It replaces nothing: Choose (the folder explorer) stays the way to pick by folder, "On the watch" becomes the **On <device name>** scope of this view, and Ledger stays as the audit log. Selecting tracks in Music and pressing Send feeds the same `preview`/Review step that Choose feeds today.

#### Layout

```
Music                                      [ Library | On Mav's Garmin 165 | Compare ]
[ search: artist:"iva davies" missing            ]   Albums  Artists  Songs  Playlists
Show: (All) (On device) (Not on device) (Partly)       Sort: Album artist, A to Z  v
-------------------------------------------------------------------------------------
[ ] Master and Commander (Music from...)   Iva Davies     2003   12 of 12  [##########]
[ ] Sea of Thieves                         Sea of Thieves 2018   24 of 25  [######### ]  1 missing
[ ] Blue Train                             John Coltrane  1958    none     [          ]
-------------------------------------------------------------------------------------
3 albums selected, 38 songs not on the device, 212 MB           [ Review and send ]
```

- **Scope switch.**
  - *Library*: the local music index (configured `library_root` and anything else the owner added), each item marked with presence on the currently selected device.
  - *On <device>*: what the device holds, from the ledger's current epoch joined with the last listing, including `foreign` items and `stub` rows, with the ledger-sent items linked back to their sources.
  - *Compare*: the union, with presence as a first-class column; this is the "what is there and what is not" answer. With several registered devices, a device picker sits next to the switch, and presence can be shown for a disconnected device from its ledger (labeled "as of <date of last listing>").
- **Groupings (tabs).**
  - *Albums*: album artist, album, year, presence, size. Expand to tracks.
  - *Artists*: artist with album count and presence rollup; expand to albums.
  - *Songs*: flat table: title, artist, album, track, duration, format, presence.
  - *Playlists*: on Garmin, the playlists-as-albums Pelican sent (ledger `mix` field, album artist "Various Artists"), each with its songs in order; on a DAP, the `.m3u8` files. In Library scope, the owner's local `.m3u`/`.m3u8` files if any (read-only), which is the natural source for "send this playlist".
- **Presence filter chips:** All, On device, Not on device, Partly (albums and artists only). Counts on each chip ("Not on device 1,204").
- **Sort keys:**
  - Albums: album artist, album title, year, date added locally (mtime), date sent (ledger), presence (partly first), size.
  - Artists: name, album count, presence.
  - Songs: title, artist, album then track, duration, date sent, format.
  - Playlists: name, date sent, song count.
  - Sorting ignores a leading "The", folds case and accents (`fold_name` plus Unicode NFKD), and places blank values last.
- **Search.** Plain words match title, artist, album artist and album (all words must match, case and accent insensitive, like foobar2000's HAS). Tokens: `artist:`, `album:`, `title:`, `genre:`, `year:`, `is:on`, `is:missing`, `is:partly`, `is:foreign`, `is:playlist`. Quoted phrases. No boolean operators in v1. Parsing lives in Rust so the CLI can share it (`pelican ls --missing "album:sea of thieves"`).
- **Presence marks (text first, color second, per `DESIGN.md`'s ink style):**
  - Song: **On device** (verified in the ledger and present in the last listing), **Sent, not seen** (verified in the ledger, absent from the last listing: deleted elsewhere, or the listing is stale), **Not on device**, **Changed since sent** (same source path, different hash), **Only on device** (foreign or orphaned: no local source found).
  - Album: "N of M", with M the local album's tracks, a 10-cell meter, and "all", "none" or "N missing". Songs that are on the device **only inside a playlist** do not count toward the album (they are a different library entry on the watch, per `Ledger::verified_in`); show them as a hint: "+3 in playlists".
  - Garmin honesty line in this view, reusing `NO_DELETE` wording: what Pelican shows is what it sent and what `/Music` lists, not what the watch's music app displays (the app can list entries whose files are gone, `garmin-mtp.md` section 8).
- **Actions.** Checkbox selection at any level; "Select all missing" in the footer; **Review and send** opens Review with the selection. For a Garmin, the footer also shows the object budget ("38 of 467 remaining slots"); for a DAP, bytes only. No remove action anywhere.
- **Big libraries.** Virtualize the list: render only the rows in view plus a margin (a fixed row height makes this about 60 lines of vanilla JS), or page by group. Queries go to Rust with `offset`/`limit`; the page never holds 20,000 rows of DOM.
- **Keyboard and a11y.** `/` focuses search; arrow keys move; Space toggles; presence is always in text; the meter has an `aria-label` ("7 of 12 songs on the device").

### B.3 Data it needs

| Data | Source today | Gap | Proposal |
|---|---|---|---|
| Local tags (title, artist, album artist, album, track, disc, year, genre, duration) | `transcode::tags::Tags::read` per file (lofty, cover art skipped); `library::list` has names and sizes only | No persistent index; reading a NAS library on every open is slow | `$XDG_CACHE_HOME/pelican/index.jsonl`, one line per file: path, size, mtime, tags, duration, format, sample rate and bit depth (for the DAP profile), `sha256` when known. It is a **cache**: safe to delete, rebuilt incrementally (re-read only files whose size or mtime changed), built in a background thread with progress and stop, like `backup`. Album artist must be read separately from artist (today `Tags::read` folds album artist into `artist`). |
| Source hash | Computed during a push (`source_sha256`) | Hashing a whole library is expensive | Match in tiers: 1) ledger `source` path equals the file's path and size and mtime unchanged since the ledger line: presume same, mark as such; 2) otherwise hash lazily only the candidates whose tags match a ledger line; 3) store hashes in the index once computed. |
| What Pelican sent | Ledger: `remote`, `source`, `source_sha256`, `title`, `artist`, `album`, status, epoch | No album artist, track, mix name | Add the optional fields in A.3 (`album_artist`, `track`, `disc`, `mix`, `profile`, `path`). For existing lines, fall back: a `verified` line whose source path still resolves can be re-read for missing tags. |
| What the device holds | `watch::read` gives `/Music` names, sizes, stubs; `Snapshot::rows(ledger)` marks origin | Tags for `foreign` objects | Garmin: object properties `0xDC44/46/9A/9B/8B/89` are readable (`garmin-mtp.md`, verified FW 2506). mtp-rs 0.13.2 exposes `get_object_prop_value` but not `GetObjectPropList` (`0x9805`), so it is one round trip per property per object; read them only for `foreign` rows (usually few), cache by handle plus size, or contribute `GetObjectPropList` upstream. DAP: read tags with lofty straight from the mounted card, cached like the local index. |
| Last listing for a disconnected device | Not kept | Needed for "as of" presence | Cache the last listing per device id in `$XDG_CACHE_HOME/pelican/devices/<id>/listing.json`, stamped with time. A cache, not a record. |
| Local playlists | Not read | Needed for the Playlists tab in Library scope | Parse `.m3u`/`.m3u8` found under the library root (read-only), resolving relative paths against the playlist's folder (Rockbox's rule). |

Rust side: one new command, `music_query { scope, device, group, presence, search, sort, offset, limit }`, returning rows plus counts per chip, with the index and joins held in memory in the shell (`Shell` state), refreshed after each push and each `status`. The CLI gets the same queries through `pelican ls` flags.

### B.4 Presence rules, precisely

For device D (current ledger epoch E) and local track T with hash h (or presumed hash via tier 1):

- **On device** if E has a `verified` event with `source_sha256 == h` and `album == T's resolved album` (the existing `verified_in` key), and its `remote` is in D's last listing (case-folded, `fold_name`).
- **Sent, not seen** if that event exists and the listing does not contain it, or no listing has been taken since the event.
- **In a playlist on device** if a verified event has `source_sha256 == h` and a `mix` (or album equal to a mix name with "Various Artists").
- **Not on device** otherwise.
- **Only on device** for device rows with no local match: `foreign` objects, and ledger entries whose source path and hash no longer resolve locally.
- Album state: count On device among the album's local tracks: all (M of M), none (0), else Partly.

Unresolved `reserve` lines (a run that died mid-upload) are never "on device"; they show under the song as "an earlier send did not finish" in the On device scope, which matches `Totals.unresolved`.

---

## Open questions for the owner

**Devices and identity**

1. Does the watch's Unit ID (Settings, System, About) equal its USB serial read as a hexadecimal number? If so, the registration card can show the Unit ID the owner recognizes.
2. May `examples/diagnose.rs` gain two read-only lines (MTP `SerialNumber` vs USB iSerial, and whether `0xD402` is in `device_properties_supported`), to be run by the owner on the FR165 at a convenient time?
3. Which exact "HiFi Walker mini" is it (H2 mini, or another model)? Please share `lsusb -v` for it in Storage mode and the output of `lsblk -o NAME,FSTYPE,UUID,LABEL,SIZE` so its VID:PID, serial behavior and filesystem can be recorded.
4. Is writing the hidden `/.pelican/volume.json` marker to a DAP's card acceptable? It is the only reliable identity for cheap players. Alternative: identity by FS UUID only, with the re-register prompt after every reformat.
5. Should a DAP's ledger also be mirrored onto its card (append-only, per machine) so a second computer sees the history, or stay strictly on this machine?
6. For "BabyCow's Garmin Fenix 9": will it be sent to from the owner's computer (so it needs registering here), and can someone run the README's "Verifying a new watch" test on it? Its 500-track cap, name-length cap and playlist behavior are all unverified.

**Profiles**

7. DAP audio: keep FLAC up to 24/192 and resample anything above, or allow the H2 mini's claimed 384 kHz after a test? Should DSD be copied or skipped?
8. DAP lossy sources: copy MP3/AAC as they are (recommended, no generational loss) or normalize everything to one format?
9. Album art size: 500 px baseline JPEG embedded plus `cover.jpg` per folder, or smaller for a 240x240 screen to save space? And for the original H2, is a per-track `<track>.jpg` sidecar wanted if the test shows that is what its firmware reads?
10. DAP playlists: real `.m3u8` files at the card root (recommended) or keep the Garmin "playlist as album" behavior for consistency across devices?
11. Should a DAP's folder layout be `Music/Album Artist/Album/NN Title.ext`, or the owner's own local layout mirrored?

**Registration flow**

12. Must a device be registered before anything can be sent (recommended), or may "Not now" still send under the model name?
13. For the legacy FR165 ledger: register silently with the name prompt at next plug (recommended), or ask on first launch of the new version?

**Library view**

14. Which folders make up "the library" for the index: only `library_root`, or also NAS mounts and other places the owner ticks?
15. Is a background index build of the whole library acceptable (it reads every file's tags once, hashes only on demand), and should it run on app start or only when the Music view is opened?
16. Should the Compare scope be able to show several devices at once (a column per device), or one device at a time (recommended for v1)?
17. Should songs that are on the watch only inside a playlist count toward an album being "on device"? The recommendation is no, shown separately as "+N in playlists", because the watch lists them as a different album.
