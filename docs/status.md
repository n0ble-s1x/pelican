# Status: what Pelican does, and what it does not

As of `main`, 2026-09-27. Requirement IDs (R1-R12) are
`docs/rebuild-plan.md`'s. The core (`pelican-core`) does the work; the
command-line tool (`pelican`) and the desktop app (`pelican-app`, in
`crates/pelican-shell`, with the interface in `ui/`) are front-ends over it.

**Hardware acceptance passed, 2026-09-26.** `pelican push` of 24 tracks from
the Sea of Thieves album (WAV, no usable tags, on the NAS) to a Forerunner
165 Music · FW 2506 on Linux: **24 verified, 0 failed** in 57 s including
transcode. Staging dir empty afterwards; ledger holds 24 `reserve` + 24
`verified`; `pelican ls` shows 24 `ledger` rows and no new stubs. An
independent read-back through libmtp (`mtp-getfile`, separate stack, fresh
session) of `pl00013-Spectral Sails.mp3` matched the ledger's
`upload_sha256`, decoded cleanly and carried its tags. After a replug the
album appears in the watch's music app with the path-derived tags and the
owner's spot check of playback (including titles with apostrophes and
dashes) is green. mtp-rs writes as cleanly as libmtp.

Track 02 was deliberately left out: it was already on the watch as
`pl0001.mp3` from the hand-run libmtp test, and a second push would have
left a permanent duplicate library entry. The counter correctly started
at 2 because `pl0001` was on the device.

Reference hardware for that run: Forerunner 165 Music · FW 2506, on Linux.
macOS is out of scope for the rebuild.

**Second hardware pass, 2026-09-27**, same watch and firmware:

- **Backup**: all 329 files under `GARMIN` (7.6 MiB) copied in 4.5 s, the
  empty `Debug/err_log.txt` included. The first two attempts wedged the
  watch at file 47: a GetObject for a 0-byte file comes back "Empty
  response" and every later call in that session fails. Empty files are
  now written locally from the listing without a download.
- **Mix**: `--mix test1` of four songs from three folders (WAV and FLAC,
  one already on the watch): 4 verified; an independent libmtp read-back
  matched each ledger hash and carried album `test1`, album artist
  "Various Artists" and tracks 1 to 4. The album appears in the watch's
  music app and plays.
- **Desktop app**: a five-song playlist from two folders sent through
  `pelican-app`: 5 verified, libmtp read-back matched, tracks 1 to 5.
- **Start over**: `reset-ledger` against a watch with 27 audio objects
  refused and left the ledger byte-identical. The other half (a `reset`
  line after a real factory reset) is not yet hardware-tested.

## What it does

| Capability | How | Req |
|---|---|---|
| One output profile | Every source, whatever its format, is re-encoded by ffmpeg to CBR 192 kbps, 44.1 kHz, stereo MP3 with an ID3v2.3 tag and no ID3v1; art and source metadata are dropped. No passthrough. A missing ffmpeg is refused before any device work, with the fix named. | R1 |
| Tags that are always there | title, artist, album_artist, album, track, date, genre. Each resolves override → source tag → path; a file whose title resolves empty is refused, not sent to be invisible. | R2 |
| Names never reused | `pl{counter:05}-{slug}.mp3`, counter strictly monotonic per watch. Every name on the device, every stub, and every name in the ledger is taken; one case fold (`mtp::fold_name`) is used for every comparison. Each stub moves the counter one further, since its name cannot be seen. No overwrite path, no rename-on-collision branch. | R3 |
| The ledger | `$XDG_DATA_HOME/pelican/ledger-<serial>.jsonl`, append-only, never pruned or rewritten. A `reset` line (see below) closes an epoch; lookups answer from the current one. The `reserve` line is fsync'd before the upload starts. Exclusive lock per run; a second run fails fast. An unparsable line is a hard error naming file and line. | R4 |
| Skip what is there | A source whose SHA-256 the ledger has as `verified` **in the same resolved album** is skipped as "already on watch"; `--resend` sends it again under a new name. The album is part of the key because a song on its own album and the same song inside a mix are two library entries. A repeat of the same audio within one run is sent once. | R5 |
| Mixes (playlist as album) | **Hardware-tested 2026-09-27.** The watch rejects MTP playlists, so `--mix NAME` sends the files in the order given as one album: album = NAME, album artist "Various Artists", track = 1..n in that order, each song keeping its own title and artist; year and genre only from `--year` / `--genre`. A song in two mixes is two files. |  |
| Stop between files | A `transfer::Stop` handle passed in `Env` is checked before each transcode and before each send, never inside a file (a retry of the file in flight still runs). Unsent files end as `skipped` (`stopped`), the tally is flagged `stopped`, staging is removed, and no stopped file has a ledger line. The CLI does not expose it; Ctrl-C still ends a CLI run. |  |
| Upload, then prove it | One MTP session per run, split-header transfers, streamed upload into flat `/Music`. Each upload is read back and hashed against the local transcode. Mismatch or error → `failed` in the ledger, the name stays burned, one retry under a fresh name (`--retries N`). | R6 |
| Capacity before any write | Refused before the first write if planned bytes + 2 MiB exceed free space, or audio objects on `/Music` + planned files exceed 500. A retry is checked the same way, with the room promised to files still queued held back. | R7 |
| Temp files never outlive the run | Everything is transcoded into `$XDG_CACHE_HOME/pelican/staging/<run-id>/` before the session opens; the dir is removed on success, failure and error, and a dir left by Ctrl-C is swept at next start. Sources are only read. | R8 |
| CLI | `pelican push [--dry-run] [--resend] [--retries N] [--artist/--album/--genre/--year] [--mix NAME] [--serial S] PATHS…`, `pelican status`, `pelican ls` (rows marked `ledger`/`foreign`/`stub`), `pelican ledger`, `pelican backup [DEST]`, `pelican reset-ledger`. A dry run prints the plan through the same decision code the run uses, and touches no device. | R9 |
| Send again | Besides the run-wide `--resend`, a front-end can name single files to send again (`Options::resend_sources`, `transfer::Resend`): each goes under a fresh name (the counter never repeats) as another copy in the library. Adding a song already on the watch to a playlist needs no flag at all: the skip key is audio + album, so it is sent as a new copy automatically. | R5 |
| A watch that stops answering | After a reboot the FR165 can come back wedged: seen 2026-09-26 enumerating as `091e:0003` "Garmin GPS usb/tty converter", every MTP open then timing out after 30 s until a physical replug. `error::classify` sorts any device error into `not_found` / `permission` / `busy` / `gvfs` / `wedged` / `other`; a timeout (mtp-rs `Error::Timeout`, an I/O `TimedOut`, or the text) is `wedged` and carries `error::REPLUG`: "The watch isn't answering. Unplug it, wait five seconds, plug it back in." `garmin::pick_device` reports a watch seen only as `091e:0003` the same way. A push whose upload or read-back times out records the attempt as `failed`, does not retry, and ends the run with that instruction instead of waiting out a timeout per file. The CLI prints the instruction on its own line. |  |
| Back up the watch | `backup::backup` walks the watch's `GARMIN` folder with `list_dir` and copies every file with `download_file` into `DEST/GARMIN/…`. Those are the only two calls it makes, so it cannot write to the watch. Local files are created new, never replaced; device names that could climb out of DEST are refused. Stoppable between files; a file that will not copy is reported and the rest are still copied; a wedge ends it with the replug instruction. `backup::default_dest(model)` is `~/Documents/Pelican/<model> backup <YYYY-MM-DD>` (XDG documents dir, UTC date, ` (2)` if taken). CLI: `pelican backup [DEST]`. |  |
| Start over after a factory reset | `reset::reset_ledger` re-reads `/Music` itself and, only if it holds no audio objects (stubs count), appends a `reset` line to the ledger. After it, names and verified sources from before no longer count; `max_counter` still spans everything, so the numbering keeps rising. Refuses with an explanation while audio remains. CLI: `pelican reset-ledger`. | R4 |
| Places | `places::places(library_root)`: Home, the XDG Music dir, the configured library, mounted network shares (nfs/nfs4/cifs/smb3/smbfs/sshfs from `/proc/self/mounts`, plus an `autofs` trigger that `/etc/fstab` says mounts a network filesystem, as with a systemd automount NAS), and drives under `/run/media/$USER` or `/media/$USER`. Mount points are not `stat`ed (an NFS mount with its server gone can hang); the fixed folders are. Parsing is pure and tested on fixture text. |  |
| gvfs warning | Every device-touching command warns, with the `gio mount -u` fix, if gvfs-mtp holds the watch. | R9 |
| udev | `udev/70-garmin-mtp.rules` sorts before `73-seat-late.rules`, so `uaccess` actually grants the ACL. | R10 |

## What it does not do

- **Delete.** The `Backend` trait has no delete method, so the capability
  does not exist in the type. Deleting an object does not take a track out
  of the watch's music library (`docs/garmin-library-persistence.md`), and a
  delete-then-write is how a name gets reused. Once a track is on the watch
  it stays in the library until a factory reset.
- **Playlists, or tags on the device.** No playlist write and no
  tag-on-device command. Playlist writes are silently rejected on the FR165
  anyway (`docs/playlists.md`).
- **Subfolders in `/Music`.** The watch rejects them; everything goes in flat.
- **Tidy up after a failed write.** A failed attempt leaves whatever it
  left on the watch. The ledger records it; nothing tries to remove it.
- **See a stub's real name.** A stub is counted and moves the counter, but
  a stub whose hidden counter sits beyond a gap in the numbering cannot be
  covered by name. Such a stub comes only from another machine's ledger or
  a lost one, and `push` warns when the ledger cannot explain the stubs it
  sees.
- **Share a ledger between machines.** The ledger is per machine. A second
  machine pushing to the same watch sees the first one's names only as
  `foreign` objects, which still keeps them from being reused.
- **Cancel mid-file.** A run can be stopped between files only; the file
  in flight is finished and proven (or failed) first. The CLI is stopped
  with Ctrl-C (staging is swept at next start; a reserved name stays
  burned).
- **Reset the ledger on someone's word.** `reset-ledger` reads `/Music`
  itself; "the watch is clean" is never taken on trust.

## The API a front-end calls

`pelican-core` is the whole engine; the CLI is a thin printer over it.

- `source::expand(paths)` → `transfer::plan_with(sources, &overrides,
  mix)` (`plan` without a mix) → `transfer::preview(&entries, ledger,
  resend)` gives a `Verdict` per file (`send` / `skip` / `refused`) without
  touching the device or transcoding. `preview::build(&entries, ledger,
  resend, room)` wraps that for a UI: per-file tags, source size and an
  **estimated** size (duration × 192 kbps + 4 KiB, or a size ratio when the
  duration is unknown), totals, and whether it fits.
- `transfer::push(entries, &mut ledger, open, options, env)` runs it.
  `env.progress` receives owned, `Serialize` `Progress` events, each with
  the file's plan `index`:
  `transcoding`, `connecting`, `sending` (per attempt, with the burned
  name), `uploading` (bytes), `attempt_failed`, `done` (exactly one per
  planned file; a verified one carries the proven SHA-256), then
  `finished` (the `Tally`, with `stopped`). The call returns a
  `Report`. `env.stop` is the stop-between-files handle.
- `library::list(dir)` browses the music on disk one folder at a time,
  read-only, with the same audio-extension and hidden-entry rules as
  `source::expand`.
- `watch::read(&mut backend, &device)` is one read-only query in an open
  session: model, serial, free/capacity, and `/Music`. `Snapshot::counts()`
  and `Snapshot::rows(ledger)` (each row `ledger` / `foreign` / `stub`) are
  what `status` and `ls` print.
- `garmin::list_devices` / `pick_device`, `ledger::Ledger::{open, read}`,
  and `platform::detect` for the gvfs warning.
- `error::classify(&err)` / `classify_with(&err, gvfs_holds_it)` →
  `DeviceErrorKind` (serializes `not_found` … `wedged`); `error::REPLUG` is
  the wedge instruction, `error::Wedged` the marker in the chain.
- `transfer::preview_with(&entries, ledger, &Resend)` and
  `preview::build_with(&entries, ledger, &Resend, room)` take a per-file
  send-again; `Options { resend, resend_sources, retries }` does the same
  for `push` (`Options` is `Clone`).
- `backup::backup(&mut backend, dest, &stop, &mut progress)` →
  `Summary`; progress is `backup::Progress` (`listing` / `file` /
  `finished`, serde-tagged `kind`). `backup::default_dest(model)`.
- `reset::check(&mut backend)` → `{ audio_objects, clean }`;
  `reset::reset_ledger(&mut backend, &mut ledger)` → `{ clean, reset,
  audio_objects, message }` (the ledger opened with `Ledger::open`).
- `places::places(library_root)` → `[Place { label, path, kind }]`.

## Ledger format change: the `reset` event (2026-09-26)

Still `"v":1`; every existing ledger reads exactly as before. One new
event kind:

```json
{"v":1,"at":"…","event":"reset","counter":<highest counter so far>,"remote":"","source":"","source_sha256":"","title":"","artist":null,"album":null,"reason":"factory reset confirmed: /Music read back with 0 audio objects"}
```

- Written only by `reset::reset_ledger`, after a fresh listing of `/Music`
  shows no audio objects; appended and fsync'd like every line. Nothing
  before it is rewritten.
- After it: `names`, `has_name`, `verified`, `verified_in`,
  `unproven_names` and `totals` see only later events (`Ledger::current`);
  `Totals.resets` counts the reset lines. `max_counter` spans every event,
  so a counter is never handed out twice.
- **Older Pelican builds refuse a ledger that holds a `reset` line**: the
  unknown event is reported as damage at that line and nothing runs. That
  is the safe direction (fail closed; no name can be reused), but it means
  a machine must not go back to a pre-reset build after using
  `reset-ledger`.

## Tests

`cargo test --workspace --all-features` runs these targets. Run it for the
current count.

| Target | What it covers |
|---|---|
| `pelican-core` unit tests (`src/`) | Each module of the core |
| `crates/pelican-core/tests/transfer.rs` | A run end to end on the fake watch |
| `crates/pelican-core/tests/ffmpeg_profile.rs` | The output profile with a real ffmpeg; skipped with a message if ffmpeg is absent |
| `crates/pelican-core/tests/sanitization.rs` | Filename and tag sanitizing |
| `crates/pelican-core/tests/tag_notice_repro.rs` | A tag-resolution regression |
| `pelican` unit tests (`src/`) | The command line |
| `pelican-shell` unit tests (`src/`) | The app's Rust side: IPC types, config, the udev rule, environment handling |

The fake backend records every call, injects upload/download failures,
silent corruption and a wedged watch (every call times out), and models
stubs, nested folders and free space. What it cannot test:

- `mtp.rs`'s mtp-rs backend: needs the watch. The read-only examples
  (`diagnose`, `verify_roundtrip`, `dump_file`, `usb_inspect`,
  `probe_objprops`) and the hardware acceptance run cover it.
- `garmin::pick_device`: wraps nusb enumeration.
- `platform::gvfs::detect`: reads the live gvfs mount directory.
- Whether the watch's music app indexes and plays what Pelican sends:
  checked by a person on the watch (green, 2026-09-26), not by any test.

## Known unknowns

- Whether a read-back can pass while the object on flash is still wrong (a
  device-side cache). A cross-stack read-back (libmtp, fresh session) after
  the acceptance run matched, and the tracks play, so no such effect has
  been seen; it is not proven impossible.
- Whether the firmware treats names differing only in extension as one
  file. Irrelevant to Pelican's own writes (always `.mp3`, always a fresh
  counter); relevant only to foreign objects.
