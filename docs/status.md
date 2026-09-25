# Status — What Works, What Doesn't

Reference scope: Pelican v0.1.0 series, verified against **Forerunner 165
Music · firmware 2506** unless otherwise noted. Other Garmin music watches
*should* behave similarly (same `microsoft.com: 1.0` MTP responder family)
but have not been individually tested.

## ✅ Works

| Feature                                                          | Notes                                                                                  |
|------------------------------------------------------------------|----------------------------------------------------------------------------------------|
| Music upload — MP3, M4A, M4B, AAC, WAV (direct)                  | Pure-Rust mtp-rs; per-file MTP session; verified large multi-file batches              |
| Music upload — FLAC, ALAC, AIFF                                  | No installed software needed **on macOS**: `/usr/bin/afconvert` → CBR 192 kbps AAC in M4A. With ffmpeg present, either platform → CBR 192 kbps MP3, ID3v2.3, strict tag allowlist. Whether either output *plays* is unresolved — see the caveat table |
| Music upload — OGG, Opus, WMA, APE, WV                           | **Requires `ffmpeg`** on either platform → CBR 192 kbps MP3. `AFCONVERT_DECODES` does not cover these, so a Mac without ffmpeg refuses them per file with a message naming the format and the fix |
| Native formats copied, never re-encoded                          | MP3/M4A/M4B/AAC/WAV are copied byte-for-byte; only the tag is rebuilt, with no tool     |
| Album-artist tag rewriting                                       | `album_artist` becomes the `ARTIST` tag — multi-composer albums group as one           |
| Filename sanitization (56-char cap, FAT-hostile chars stripped)  | Applied in **both** transcode AND `--no-transcode` paths                               |
| `set_split_header_data(true)` for the MTP transport              | Required by Garmin firmware; auto-applied                                              |
| Listing `/Music` with broken-stub surfacing                      | Surfaced as `‹unreadable #N›` rows; carries the handle so a delete can be *attempted*  |
| Removing a file from `/Music`                                    | Per-handle, batched in the shell and `--delete Music/foo.mp3 --delete Music/bar.mp3 …` in the CLI. This deletes the **object**; see the caveat table for what it does not do |
| GVFS-mount detection (Linux)                                     | Warns if a GVFS MTP mount is holding the device, with the `gio mount -u` fix. A warning, not a refusal — the CLI proceeds |
| macOS `ptpcamerad` detection                                     | Names the holder from the IORegistry. **Cannot** self-fix — see `docs/macos-port.md`   |
| GUI (eframe/egui) — three-pane file browser, drag-drop           | The **Linux** build (`crates/pelican`, binary `pelican`); window title "Krypteia · Pelican" |
| macOS shell — Tauri 2 over the `ui/` frontend                    | `crates/pelican-shell`, binary `pelican-shell`. Watch wall, capacity gauge, live transfer report, select-and-delete behind a confirmation |
| macOS — local library browsing and **in-app playback**           | Pelican is a local player that also syncs (PRODUCT.md). The system webview decodes MP3, AAC, ALAC, FLAC and WAV; auditioning before committing to ~3.7 GB is the point |
| macOS — albums and artists views, playback queue, media keys     | Grouping comes from Pelican's own upload journal and the UI says so |
| macOS — cover art at play time                                   | Read out of the user's own file when a track starts. Nothing is fetched |
| Name-collision guard on the upload path                          | Plan-time, against the listing the shell already takes, plus a refusal in the open session immediately before the write. **Not yet exercised against hardware.** See the caveat table |
| Local playlists (history-stored, not pushed to watch)            | Per-device-serial JSON — `$XDG_DATA_HOME/pelican/uploads-<serial>.json` on Linux, `~/Library/Application Support/com.krypteia.pelican/` on macOS |
| Streaming upload from disk (no full-file buffer)                 | Memory peak now ~CHUNK (256 KB), not 2× file size                                      |

## ⚠ Works with caveats

| Feature                                                          | Caveat                                                                                                    |
|------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------|
| Playing anything Pelican put on the watch                         | **Never confirmed, on either platform.** Files are accepted and appear in an MTP listing; whether the watch's music app indexes and plays them is open. The tracks the device currently lists came from a pre-macOS Linux batch and do not play. Community reports suggest MTP writes from Linux/macOS can land empty or truncated, which would explain it and has never been checked here — a `download_file` round trip and a `sha256` would settle it. `docs/garmin-library-persistence.md` |
| Listing newly-created subfolders inside `/Music`                 | Garmin firmware returns `Protocol GeneralError`. We **flatten by default**; `--no-flatten` for opt-in.    |
| Name collisions with files already on the watch                  | A same-name write destroys **both** copies and the wreckage cannot be deleted (`garmin-mtp.md` §6, §7). Pelican now checks every name the watch can *report* before writing and refuses; the user can opt into a rename. Two blind spots stand: a broken stub's real filename is unreadable, so a collision with one is undetectable by name; and whether the firmware treats `song.mp3` and `song.wav` as colliding is unverified, so the comparison is stem-level and over-reports. **Neither guard layer has run against a watch yet** — both are covered by fake-backend tests only. |
| Files with no ID3 title+artist                                   | Land on disk but invisible in the music app. Default warns + uploads; `--require-tags` strict-rejects.    |
| Deleting broken stubs left by failed prior writes                | Watch has refused `DeleteObject` for its own broken handles every time asked (`Protocol GeneralError`); never once seen to succeed. Auto-GC'd eventually. See `docs/garmin-mtp.md` §6. |
| Delete as the user understands it — taking a track off the watch | **A successful delete may not remove the track from the watch's music app.** Observed once, FR165 FW 2506, 2026-09-05: 22 files deleted, every `DeleteObject` returned `Ok`, `/Music` fell to one entry and free space rose 78.5 MB — and the watch's music app still listed all 22. The mechanism is **not established**; `docs/garmin-mtp.md` §8 records the two models that both fit and the test that would separate them. Pelican has no operation that reaches the watch's library, so a user who fills the watch may not be able to reclaim the space with Pelican. The UI says this before the send and again before the delete. **Corroborated externally:** owners of the FR245/265/645/945/955, fēnix 6 and fēnix 8 report the same ghost entries, with a factory reset the only confirmed cure and Garmin Express not reliably clearing them either — `docs/garmin-library-persistence.md`. |

## ❌ Doesn't work / Blocked

| Attempt                                                          | Outcome                                                                                                  |
|------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------|
| Playlist write via MTP `SendObjectInfo` + `SendObject`           | Silently rejected on FR165 across all 6 path/format variants. See `docs/playlists.md`.                   |
| Playlist write with format codes 0xBA05 / 0xBA10 / 0xBA11        | Same outcome — format code is not the discriminator on FR165.                                            |
| Removing a track from the watch's **music library**              | No operation in Pelican reaches it. Standard MTP exposes `/Music` and nothing above it. If a library op exists it is in the undocumented vendor block — see `docs/vendor-ops.md` and the **not-yet-run** plan in `docs/vendor-op-probe-proposal.md`. |
| Vendor-op probe (`examples/probe_vendor_ops`)                    | Wedges device session; requires physical replug (USB reset alone insufficient). A safer redesign is proposed, unrun, in `docs/vendor-op-probe-proposal.md`. |
| Combined-bulk MTP transfers (mtp-rs default w/o split-header)    | Hangs `send_object_stream` to 30s timeout; wedges session.                                               |
| Sending FLAC with embedded album art via SendObject              | Watch silently rejects (oversized APIC frame). Mitigated by `-vn` in transcode pipeline.                 |

## Hardware-firmware test matrix

|                              | FR165 Music · 2506 | FR945 / FR255 / Venu | FR645 Music             |
|------------------------------|--------------------|----------------------|--------------------------|
| Music upload                 | ✅ verified        | (presumed ✅)        | (presumed ✅)            |
| MTP playlist write           | ❌ rejected        | ✅ per better-sync   | ✅ per better-sync       |
| Vendor opcodes 0x9000-0x900B | declared           | declared (FR945)     | `0x9000-0x9006` declared |

We **do not have second-watch hardware to confirm** whether FR165's playlist
rejection is a model-specific firmware regression or a wider issue. Adding
a borrowed FR945 / FR255 to the test matrix would resolve the ambiguity in
~30 minutes; see `docs/playlists.md` for the recipe to validate.

## Test coverage (measured 2026-09-06 on macOS 26.6.2, `cargo test --workspace`)

**122 tests pass.** This run was macOS only; the figure has not been re-taken
on Linux since the workspace split, so it is stated for one platform rather
than claimed for both.

| Target | Count |
|---|---|
| `pelican-core` unit tests (`src/`) | 76 |
| `crates/pelican-core/tests/collision.rs` | 11 |
| `crates/pelican-core/tests/pipeline.rs` | 5 |
| `crates/pelican-core/tests/sanitization.rs` | 5 |
| `crates/pelican-core/tests/tag_notice_repro.rs` | 1 |
| `pelican-shell` unit tests (`src/`) | 24 |

`tag_notice_repro.rs` pins one thing: a blank value in the primary tag frame
must not shadow a real value in another tag on the same file. It is written
against a synthesised WAV whose ID3v2 (the primary for WAV) carries `"   "`
for title and artist while its RIFF INFO tag carries the real ones. Before
the fix, `Tags::read` returned `None` for both and the UI told the owner the
file was untagged.

- **76 unit tests** across `crates/pelican-core/src/` — playlist, transcode,
  transfer, mtp, paths, and the platform contention detectors
  - Filename stem sanitizer: cap-at-56, replace unsafe chars, collapse dashes, trim outer, never empty
  - Tag value sanitizer: strips ©®™℗ + control bytes; preserves accented letters
  - File extension classification (audio vs not, supported-by-Garmin vs needs-transcode)
  - `expand_inputs_with` flatten + non-flatten directory tree behavior
  - The **write-time collision guard**, driven through a fake `Backend`: a
    stale plan is caught before `SendObjectInfo`; a listing that fails with no
    plan-time evidence refuses to write; the same failure *with* plan-time
    evidence proceeds; a run cannot collide with itself; a write whose bytes
    drained but was never confirmed still reserves the name; Stop is honoured
    between files and never inside one; and the cost budget — **one folder
    walk per directory per run**, pinned by a twenty-file plan that takes
    exactly one listing
  - `mtp::same_file` case-insensitivity — `/Music` is FAT-derived, so a
    byte-equal name comparison under-reports
  - `playlist::parse` edge cases: CRLF, blank lines, multiple comment-line variants, empty
  - `playlist::serialize_for_device` for both `PathStyle` variants
  - `is_playlist` extension matching (case-insensitive, edge cases)
- **11 integration tests** in `crates/pelican-core/tests/collision.rs` — the
  plan-time half of the collision guard: a name already on the watch is not
  planned over, collisions survive truncation and case-folding, a stem match
  counts across extensions, a rename lands somewhere free on both sides and
  inside the 56-char budget, and a device-side name is matched **raw** so an
  accented filename Garmin Express wrote cannot raise a false alarm
- **5 integration tests** in `crates/pelican-core/tests/sanitization.rs` — invariants tied to
  documented Garmin firmware quirks, bound to the real API rather than to
  constants re-declared in the test file
- **5 end-to-end tests** in `crates/pelican-core/tests/pipeline.rs` — real audio through the real
  encoder selection and tag rebuild, no hardware needed
- **24 unit tests** in `crates/pelican-shell/src/` — the shell's own logic,
  none of which needs a webview:
  - `dto::classify_skip` / `classify_fail` against the engine's exact wording,
    including the collision refusal and the drained-but-unconfirmed upload the
    shell reconciles on
  - `device::journal_stem` — the journal must key on the name the *watch* will
    report, including when the collision resolver renamed the job
  - `device::project_before` — one `/Music` listing, two pictures, and the
    difference between them (a broken stub blocks a name but does not count as
    "already there" for reconciliation)
  - `device::Tally` counter semantics — a skip or a failure advances "3 of 12"
  - `scan` encode planning and format labels, including the afconvert fallback
    and the per-file refusal when no installed encoder reads a format
  - `commands` base64 and URL encoding for the cover-art data URI

What we **do not** unit-test (and why):

- `mtp.rs` MTP backend — needs hardware. Covered by `examples/diagnose`,
  `examples/probe_playlist`, `examples/probe_platform`,
  `examples/probe_objprops`, and manual smoke tests. The transfer loop above
  it is now testable because `transfer::run_with` takes an injected opener.
- `app.rs` GUI — needs an event loop. Manual end-to-end testing only. The
  transfer loop it used to own is now `transfer::run`, which is covered.
- `ui/` — no npm and therefore no JS test runner, by design. Manual.
- `device.rs`'s device loop — its backend is opened by `mtp::open` with no
  injection seam, so the "one listing per transfer" budget is structural
  (`start_sync` calls `music_before` once) rather than asserted by a test.
- `garmin::pick_device` — wraps nusb enumeration. Manual.
- `gvfs::warn_if_holding_garmin` — POSIX-side IPC. Manual.
- `history::record` write atomicity — best-effort JSON writes; no concurrency in practice.
