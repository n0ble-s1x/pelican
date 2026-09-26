# Status — what the rebuilt core does, and what it does not

As of the `rebuild` branch, 2026-09-26. Requirement IDs (R1–R12) are
`docs/rebuild-plan.md`'s.

**Pelican itself has not yet run against a watch.** Every claim below about
the push is proven against the fake MTP backend (`mtp::fake`) and, for the
encode, against a real ffmpeg. The hardware facts the design rests on — a
clean upload of this exact MP3 profile appears and plays, and a libmtp
write reads back byte-identical — were established by hand with
`mtp-sendfile`/`mtp-getfile`, not with Pelican's code
(`docs/garmin-library-persistence.md` § Results — Linux, 2026-09-26).
Whether mtp-rs writes as cleanly as libmtp did is what the hardware
acceptance run (end of `rebuild-plan.md`) settles. Until that run this page
claims nothing about Pelican on the device.

Reference hardware for that run: Forerunner 165 Music · FW 2506, on Linux.
macOS is out of scope for the rebuild.

## What it does

| Capability | How | Req |
|---|---|---|
| One output profile | Every source, whatever its format, is re-encoded by ffmpeg to CBR 192 kbps, 44.1 kHz, stereo MP3 with an ID3v2.3 tag and no ID3v1; art and source metadata are dropped. No passthrough. A missing ffmpeg is refused before any device work, with the fix named. | R1 |
| Tags that are always there | title, artist, album_artist, album, track, date, genre. Each resolves override → source tag → path; a file whose title resolves empty is refused, not sent to be invisible. | R2 |
| Names never reused | `pl{counter:05}-{slug}.mp3`, counter strictly monotonic per watch. Every name on the device, every stub, and every name in the ledger is taken; one case fold (`mtp::fold_name`) is used for every comparison. Each stub moves the counter one further, since its name cannot be seen. No overwrite path, no rename-on-collision branch. | R3 |
| The ledger | `$XDG_DATA_HOME/pelican/ledger-<serial>.jsonl`, append-only, never pruned or rewritten. The `reserve` line is fsync'd before the upload starts. Exclusive lock per run; a second run fails fast. An unparsable line is a hard error naming file and line. | R4 |
| Skip what is there | A source whose SHA-256 the ledger has as `verified` is skipped as "already on watch"; `--resend` sends it again under a new name. A repeat of the same audio within one run is sent once. | R5 |
| Upload, then prove it | One MTP session per run, split-header transfers, streamed upload into flat `/Music`. Each upload is read back and hashed against the local transcode. Mismatch or error → `failed` in the ledger, the name stays burned, one retry under a fresh name (`--retries N`). | R6 |
| Capacity before any write | Refused before the first write if planned bytes + 2 MiB exceed free space, or audio objects on `/Music` + planned files exceed 500. A retry is checked the same way, with the room promised to files still queued held back. | R7 |
| Temp files never outlive the run | Everything is transcoded into `$XDG_CACHE_HOME/pelican/staging/<run-id>/` before the session opens; the dir is removed on success, failure and error, and a dir left by Ctrl-C is swept at next start. Sources are only read. | R8 |
| CLI | `pelican push [--dry-run] [--resend] [--retries N] [--artist/--album/--genre/--year] [--serial S] PATHS…`, `pelican status`, `pelican ls` (rows marked `ledger`/`foreign`/`stub`), `pelican ledger`. A dry run prints the plan through the same decision code the run uses, and touches no device. | R9 |
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
- **Cancel mid-run from the API.** The CLI is stopped with Ctrl-C (staging
  is swept at next start; the reserved name stays burned). `transfer::push`
  has no cancel hook yet.
- **A UI.** The egui GUI and the macOS Tauri shell were removed; the UI is
  to be rebuilt against the API below.

## The API a front-end calls

`pelican-core` is the whole engine; the CLI is a thin printer over it.

- `source::expand(paths)` → `transfer::plan(sources, &overrides)` →
  `transfer::preview(&entries, ledger, resend)` gives a `Verdict` per file
  (`send` / `skip` / `refused`) without touching the device or transcoding.
- `transfer::push(entries, &mut ledger, open, options, env)` runs it.
  `env.progress` receives owned, `Serialize` `Progress` events —
  `transcoding`, `connecting`, `sending` (per attempt, with the burned
  name), `uploading` (bytes), `attempt_failed`, `done` (exactly one per
  planned file) — and the call returns a `Report` with a `Tally`.
- `watch::read(&mut backend, &device)` is one read-only query in an open
  session: model, serial, free/capacity, and `/Music`. `Snapshot::counts()`
  and `Snapshot::rows(ledger)` (each row `ledger` / `foreign` / `stub`) are
  what `status` and `ls` print.
- `garmin::list_devices` / `pick_device`, `ledger::Ledger::{open, read}`,
  and `platform::detect` for the gvfs warning.

## Tests

`cargo test --workspace --all-features` on Linux, 2026-09-26: **132 pass**.

| Target | Count |
|---|---|
| `pelican-core` unit tests (`src/`) | 92 |
| `crates/pelican-core/tests/transfer.rs` — the run end to end on the fake watch | 21 |
| `crates/pelican-core/tests/ffmpeg_profile.rs` — real ffmpeg; skipped with a message if absent | 5 |
| `crates/pelican-core/tests/sanitization.rs` | 3 |
| `crates/pelican-core/tests/tag_notice_repro.rs` | 1 |
| `pelican` CLI unit tests (`src/`) | 10 |

The fake backend records every call, injects upload/download failures and
silent corruption, and models stubs and free space. What it cannot test:

- `mtp.rs`'s mtp-rs backend — needs the watch. The read-only examples
  (`diagnose`, `verify_roundtrip`, `dump_file`, `usb_inspect`,
  `probe_objprops`) and the hardware acceptance run cover it.
- `garmin::pick_device` — wraps nusb enumeration.
- `platform::gvfs::detect` — reads the live gvfs mount directory.
- Whether the watch's music app indexes and plays what Pelican sends. That
  is the acceptance run's last step, and a person does it.

## Known unknowns

- Whether mtp-rs's write is as clean as libmtp's. The read-back hash will
  say so on the first file of the acceptance run.
- Whether a read-back through mtp-rs can pass while the object on flash is
  still wrong (a device-side cache). The libmtp run on 2026-09-26 found no
  such effect; it has not been checked through mtp-rs.
- Whether the firmware treats names differing only in extension as one
  file. Irrelevant to Pelican's own writes (always `.mp3`, always a fresh
  counter); relevant only to foreign objects.
