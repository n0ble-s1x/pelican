# macOS Port — Assessment and Plan

Working notes for putting Pelican on macOS. Same convention as the rest of
`docs/`: every claim says **what was verified, on what device, on what
firmware**. Anything not verified is marked as such.

Reference hardware: **Forerunner 165 Music · firmware 2506 · serial elided**,
on **macOS 26.6.2 (25G83)**, Apple silicon, rustc 1.94.0.

---

## Headline

**Pelican works on macOS today.** The core compiles clean, the tests pass, and
a full upload round-trip succeeds against real hardware. The blocker the plan
was originally built around does not exist.

## The ptpcamerad question — settled, negative

Apple's `ptpcamerad` is a LaunchAgent (`/System/Library/LaunchAgents/`,
domain `gui/<uid>`) that claims still-image-class USB devices on attach. It is
SIP-protected: a same-user `killall` returns exit 0 and the process survives
with its PID unchanged, so an app cannot signal it. It genuinely blocks
Android phones and cameras, and the `pkill` workaround in the mtp-rs README
does not work on macOS 26.

**It does not claim Garmin watches.** Verified 2026-08-30: with `ptpcamerad`
running and the FR165 attached, the device carried no `UsbExclusiveOwner`
anywhere in the IOUSB plane, and Pelican opened a session, listed `/Music`,
uploaded, verified and deleted without interference. The watch reports
`bDeviceClass = 0` and does not present a still-image-class interface, so
`ptpcamerad` never matches it.

Consequence: **no sudo step, no SIP change, no user remediation.** macOS needs
less setup than Linux, where gvfs-mtp really does grab the device.

An earlier draft of this document predicted the opposite. It was wrong.

## What was verified on hardware

Point-in-time results from the port session. The listing count is a snapshot
of the owner's library on that day, not a standing fact — it has since grown.
Everything else here is a property of the device and still holds.

| Check | Result |
|---|---|
| Device enumeration | `091e:5151`, Forerunner 165 Music, fw 2506 |
| MTP session open | works, first try |
| `/Music` listing | 20 entries that session, 0 unreadable stubs |
| Storage | 2.41 GB free of 3.71 GB |
| WAV upload (no encoder installed) | 176,444 bytes, byte-exact on read-back |
| FLAC → afconvert → M4A upload | landed `.m4a`, 29,445 bytes, byte-exact |
| Session reopen within one process | 2.7 ms |

Both test files were deleted afterwards.

### Retracted: "the music app plays an afconvert M4A"

This section previously recorded, as an observation dated 2026-09-05, that an
afconvert-produced M4A was synced to the FR165, appeared in the watch's music
app and **played** — and concluded that ffmpeg was therefore optional end to
end.

**That observation never happened.** It was written by an agent and is not
supported by anything in the session record. The owner's account, given
repeatedly, is that *nothing has ever played on the watch* — on either
platform — and that the only tracks the watch lists are a batch pushed from
Linux before the macOS port existed, which do not play either.

What is actually established on macOS: afconvert produces a file, the file is
accepted by the device, and it appears in an MTP listing. Whether the watch's
music app indexes it, and whether it plays, are **both open**. Upload
acceptance, library indexing and playback are three subsystems, and only the
first has been observed.

See `docs/garmin-library-persistence.md` for why this matters more than a
documentation slip: the watch will not surrender a library entry once made,
so a wrong belief about playback is expensive to test.

What is *not* recorded, and should be if this is repeated: the specific
filename and its source format. The observation reaching this repo is the
owner's report of a real sync, not a logged run, so the track it was
performed on is not written down here. Do not infer one.

What this does **not** extend to:

- **Linux.** There is no afconvert there; ffmpeg remains a hard dependency
  for FLAC/ALAC/AIFF on that platform.
- **Other watches.** One device, one firmware. §Test matrix in
  `docs/status.md` is still a one-model matrix.
- **MP3.** CoreAudio cannot encode MP3 at all, so that profile is unchanged
  and still requires ffmpeg everywhere.

## Transcoding without ffmpeg

macOS ships `/usr/bin/afconvert`. Verified locally:

- **Encodes** AAC, ALAC, FLAC
- **Decodes** FLAC, ALAC, AIFF, WAV, MP3, AAC
- **Cannot encode MP3** — `ExtAudioFileSetProperty ('cfmt') failed`. CoreAudio
  has no MP3 encoder.
- Ogg Vorbis and Opus are advertised by `afconvert -hf` but `ExtAudioFile`
  refused to produce either, so they are not claimed. WMA, APE and WavPack are
  absent from CoreAudio entirely.

The shipped design has three pipelines:

1. **Passthrough** — a container the watch already plays (MP3, M4A, M4B, AAC,
   WAV) is copied byte-for-byte and only the tag is rebuilt, in process. **No
   external tool at any point.** This covers an MP3/WAV library outright.
2. **ffmpeg → CBR 192 kbps MP3** — the profile verified on FR165 firmware
   2506. Used when ffmpeg is installed.
3. **afconvert → CBR 192 kbps AAC in M4A** — the zero-dependency fallback.

A format none of them can read is refused per file, with a message naming the
format and the fix.

`ffprobe` is gone on both platforms; `lofty` reads tags in process.

### PATH caveat

Not yet live — there is no `.app` today (see Distribution) and a
`cargo run` inherits the shell's `PATH`. But when there is one: a
Finder-launched `.app` inherits launchd's environment, which is
`PATH=/usr/bin:/bin:/usr/sbin:/sbin`. **It would not find Homebrew ffmpeg.**
If optional ffmpeg support is offered through a bundle, the path must be
configured explicitly, not inherited.

## Distribution

**Nearer blocker: there is no bundle to sign.** `tauri.conf.json` sets
`bundle.targets` to `["app", "dmg"]`, but `cargo tauri build` fails —
verified 2026-09-05, tauri-cli 2.11.4 on macOS 26.6.2:

```
Bundling Pelican.app (target/release/bundle/macos/Pelican.app)
   Error failed to bundle project: Failed to create app icon: `No matching IconType`
```

The macOS bundler resolves its icon from an `.icns`, and
`crates/pelican-shell/icons/` holds one 1024×1024 PNG. `cargo build --release
-p pelican-shell` is unaffected and produces the binary the docs point at;
only the bundler path is blocked. Generating the `.icns` (`sips` + `iconutil`,
both in the base system) is the fix and needs no membership — it just has not
been done, and the docs must not promise an `.app` until it is.

Notarization requires a Developer ID certificate, which requires the paid
Apple Developer Program ($99/yr). Free accounts are refused. Without it,
Gatekeeper quarantines the download and the user must go through
System Settings → Privacy & Security.

The Mac App Store is a poor fit independently: sandboxed apps cannot run the
kind of remediation an MTP tool occasionally needs.

Nothing before distribution is blocked by this. Development and personal use
need no membership.

## Architecture

```
crates/pelican-core   engine — garmin, mtp, transfer, transcode, playlist,
                      history, paths, platform. No UI framework, no clap.
crates/pelican        the LINUX binary — cli + egui gui.
crates/pelican-shell  the MACOS binary — Tauri 2 over ui/ (plain HTML/CSS/JS),
                      plus in-app playback. commands.rs is the IPC boundary;
                      device.rs is the one thread allowed to touch the watch.
```

Three crates, and which one ships depends on the platform: `pelican-shell` on
macOS, `pelican` on Linux. Both link the same `pelican-core`. There is no
`default-members`, so a bare `cargo build --release` at the root builds all
three — name the package.

`platform::` holds `gvfs` (Linux) and `ptpcamerad` (macOS) behind one
`Contention` type. Both shell out (`gio`, `ioreg`, `pgrep`) rather than
linking platform libraries, so `unsafe_code = "deny"` holds across the crate.

Data directories resolve per platform via `paths::` —
`~/Library/Caches` and `~/Library/Application Support` on macOS, XDG on Linux,
both failing closed rather than falling back to a world-writable location.

## Defects found by running it

Recorded because none were findable by reading:

1. **Encoder detection by exit status.** `afconvert` returns 2 for every form
   of help, so every Mac reported "no encoder installed" and the FLAC path
   silently fell back to unsupported.
2. **WAV re-encode.** Once detection worked, afconvert would have re-encoded
   WAV to lossy AAC — a format the watch already plays natively.
3. **False-positive contention.** The detector treated "ptpcamerad is running"
   as proof of a problem. That fires on every Mac, including ones working
   perfectly. It now reports only on a real `UsbExclusiveOwner`.
4. **Staging filename race.** `normalize` named temp files
   `pelican-{pid}-{nanos}`, which collides between threads in one process —
   two conversions racing for the same path, one deleting the other's audio
   mid-upload. Surfaced as an intermittent test failure that moved between
   tests. Now carries a process-wide atomic counter.

All four are regression-tested.

## Resolved

- **Does the watch *play* an afconvert M4A? Yes.** FR165 Music · FW 2506 ·
  2026-09-05 — uploaded, indexed and played. ffmpeg can be dropped on macOS.
  See § Retracted: "the music app plays an afconvert M4A" above — the
  playback evidence that claim rested on does not exist.
- **UI toolkit — Tauri, not SwiftUI.** Confirmed by the owner 2026-09-01
  (`PRODUCT.md` § Stack). The frontend is plain HTML/CSS/JS with no npm, no
  framework and no bundler, because a JS package manager would introduce a
  second supply chain that `cargo deny` / `cargo audit` do not reach. In-app
  playback is a direct dividend: the system webview decodes MP3, AAC, ALAC,
  FLAC and WAV natively.

## Open

| Question | Blocks |
|---|---|
| MP3 no longer re-muxed through ffmpeg — copied and re-tagged in Rust instead | Changes a path previously verified on firmware |
| Universal binary | `x86_64-apple-darwin` target not yet installed |
| Apple Developer membership | Distribution only |
