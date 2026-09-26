# Status — what the rebuilt core does, and what it does not

As of the `rebuild` branch, 2026-09-26. Requirement IDs (R1–R12) are
`docs/rebuild-plan.md`'s.

**Hardware acceptance passed, 2026-09-26.** `pelican push` of 24 tracks from
the Sea of Thieves album (WAV, no usable tags, on the NAS) to a Forerunner
165 Music · FW 2506 on Linux: **24 verified, 0 failed** in 57 s including
transcode. Staging dir empty afterwards; ledger holds 24 `reserve` + 24
`verified`; `pelican ls` shows 24 `ledger` rows and no new stubs. An
independent read-back through libmtp (`mtp-getfile`, separate stack, fresh
session) of `pl00013-Spectral Sails.mp3` matched the ledger's
`upload_sha256`, decoded cleanly and carried its tags. After a replug the
album appears in the watch's music app with the path-derived tags and the
owner's spot check of playback — including titles with apostrophes and
dashes — is green. mtp-rs writes as cleanly as libmtp.

Track 02 was deliberately left out: it was already on the watch as
`pl0001.mp3` from the hand-run libmtp test, and a second push would have
left a permanent duplicate library entry. The counter correctly started
at 2 because `pl0001` was on the device.

Reference hardware for that run: Forerunner 165 Music · FW 2506, on Linux.
macOS is out of scope for the rebuild.

## What it does

| Capability | How | Req |
|---|---|---|
| One output profile | Every source, whatever its format, is re-encoded by ffmpeg to CBR 192 kbps, 44.1 kHz, stereo MP3 with an ID3v2.3 tag and no ID3v1; art and source metadata are dropped. No passthrough. A missing ffmpeg is refused before any device work, with the fix named. | R1 |
| Tags that are always there | title, artist, album_artist, album, track, date, genre. Each resolves override → source tag → path; a file whose title resolves empty is refused, not sent to be invisible. | R2 |
| Names never reused | `pl{counter:05}-{slug}.mp3`, counter strictly monotonic per watch. Every name on the device, every stub, and every name in the ledger is taken; one case fold (`mtp::fold_name`) is used for every comparison. Each stub moves the counter one further, since its name cannot be seen. No overwrite path, no rename-on-collision branch. | R3 |
| The ledger | `$XDG_DATA_HOME/pelican/ledger-<serial>.jsonl`, append-only, never pruned or rewritten. The `reserve` line is fsync'd before the upload starts. Exclusive lock per run; a second run fails fast. An unparsable line is a hard error naming file and line. | R4 |
| Skip what is there | A source whose SHA-256 the ledger has as `verified` **in the same resolved album** is skipped as "already on watch"; `--resend` sends it again under a new name. The album is part of the key because a song on its own album and the same song inside a mix are two library entries. A repeat of the same audio within one run is sent once. | R5 |
| Mixes (playlist as album) | **Implemented, not yet hardware-tested.** The watch rejects MTP playlists, so `--mix NAME` sends the files in the order given as one album: album = NAME, album artist "Various Artists", track = 1..n in that order, each song keeping its own title and artist; year and genre only from `--year` / `--genre`. A song in two mixes is two files. | — |
| Stop between files | A `transfer::Stop` handle passed in `Env` is checked before each transcode and before each send, never inside a file (a retry of the file in flight still runs). Unsent files end as `skipped` (`stopped`), the tally is flagged `stopped`, staging is removed, and no stopped file has a ledger line. The CLI does not expose it; Ctrl-C still ends a CLI run. | — |
| Upload, then prove it | One MTP session per run, split-header transfers, streamed upload into flat `/Music`. Each upload is read back and hashed against the local transcode. Mismatch or error → `failed` in the ledger, the name stays burned, one retry under a fresh name (`--retries N`). | R6 |
| Capacity before any write | Refused before the first write if planned bytes + 2 MiB exceed free space, or audio objects on `/Music` + planned files exceed 500. A retry is checked the same way, with the room promised to files still queued held back. | R7 |
| Temp files never outlive the run | Everything is transcoded into `$XDG_CACHE_HOME/pelican/staging/<run-id>/` before the session opens; the dir is removed on success, failure and error, and a dir left by Ctrl-C is swept at next start. Sources are only read. | R8 |
| CLI | `pelican push [--dry-run] [--resend] [--retries N] [--artist/--album/--genre/--year] [--mix NAME] [--serial S] PATHS…`, `pelican status`, `pelican ls` (rows marked `ledger`/`foreign`/`stub`), `pelican ledger`. A dry run prints the plan through the same decision code the run uses, and touches no device. | R9 |
| gvfs warning | Every device-touching command warns, with the `gio mount -u` fix, if gvfs-mtp holds the watch. | R9 |
| udev | `udev/70-garmin-mtp.rules` — sorts before `73-seat-late.rules`, so `uaccess` actually grants the ACL. | R10 |

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
  `foreign` objects — which still keeps them from being reused.
- **Cancel mid-file.** A run can be stopped between files only; the file
  in flight is finished and proven (or failed) first. The CLI is stopped
  with Ctrl-C (staging is swept at next start; a reserved name stays
  burned).
- **A UI.** The egui GUI and the macOS Tauri shell were removed; the UI is
  to be rebuilt against the API below.

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
  the file's plan `index` —
  `transcoding`, `connecting`, `sending` (per attempt, with the burned
  name), `uploading` (bytes), `attempt_failed`, `done` (exactly one per
  planned file; a verified one carries the proven SHA-256), then
  `finished` (the `Tally`, with `stopped`) — and the call returns a
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

## Tests

`cargo test --workspace --all-features` on Linux, 2026-09-26: **150 pass**.

| Target | Count |
|---|---|
| `pelican-core` unit tests (`src/`) | 102 |
| `crates/pelican-core/tests/transfer.rs` — the run end to end on the fake watch | 27 |
| `crates/pelican-core/tests/ffmpeg_profile.rs` — real ffmpeg; skipped with a message if absent | 6 |
| `crates/pelican-core/tests/sanitization.rs` | 3 |
| `crates/pelican-core/tests/tag_notice_repro.rs` | 1 |
| `pelican` CLI unit tests (`src/`) | 11 |

The fake backend records every call, injects upload/download failures and
silent corruption, and models stubs and free space. What it cannot test:

- `mtp.rs`'s mtp-rs backend — needs the watch. The read-only examples
  (`diagnose`, `verify_roundtrip`, `dump_file`, `usb_inspect`,
  `probe_objprops`) and the hardware acceptance run cover it.
- `garmin::pick_device` — wraps nusb enumeration.
- `platform::gvfs::detect` — reads the live gvfs mount directory.
- Whether the watch's music app indexes and plays what Pelican sends —
  checked by a person on the watch (green, 2026-09-26), not by any test.

## Known unknowns

- Whether a read-back can pass while the object on flash is still wrong (a
  device-side cache). A cross-stack read-back (libmtp, fresh session) after
  the acceptance run matched, and the tracks play, so no such effect has
  been seen; it is not proven impossible.
- Whether the firmware treats names differing only in extension as one
  file. Irrelevant to Pelican's own writes (always `.mp3`, always a fresh
  counter); relevant only to foreign objects.
