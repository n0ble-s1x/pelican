<h1 align="center">Krypteia · Pelican</h1>

<p align="center">
  <strong>Your music on your Garmin watch. No account, no cloud, no vendor app.</strong>
</p>

<p align="center">
  <a href="https://github.com/n0ble-s1x/pelican"><img src="https://img.shields.io/badge/github-n0ble--s1x%2Fpelican-0b0b0b?style=flat-square&logo=github" alt="GitHub repo" /></a>
  <img src="https://img.shields.io/badge/rust-1.89%2B-0b0b0b?style=flat-square&logo=rust" alt="Rust 1.89+" />
  <img src="https://img.shields.io/badge/platform-Linux-0b0b0b?style=flat-square" alt="Platform: Linux" />
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-0b0b0b?style=flat-square" alt="License: MIT OR Apache-2.0" /></a>
  <img src="https://img.shields.io/badge/telemetry-zero-16a34a?style=flat-square" alt="Zero telemetry" />
</p>

Pelican takes the music on your computer, converts it to a format Garmin
watches reliably play, sends it to the watch over USB, and proves every file
arrived intact. **No account, no daemon, no telemetry, and no network access.**

It exists because every path Garmin offers runs through a Garmin account.
Garmin Express is Windows and macOS only and wants you signed in; on Linux
there is no vendor tool at all, and the generic MTP tools lose or corrupt
files on these watches. Owning your music should not mean renting the cable.

A [Krypteia](https://github.com/n0ble-s1x) project.

---

## Why this exists

Krypteia is a privacy and security consulting practice. Our mission is to help people take back their digital sovereignty — to understand the systems they live inside, and to choose tools that don't make them the product.

This tool is part of that. We believe:

- **Privacy is a prerequisite for freedom.** What you listen to, when, and where — that's nobody's business but yours.
- **Own your data. Own your media.** Buy it once, keep it forever, no subscription tax, no licensing terms that change under you.
- **Open source is a public good.** Useful software should be inspectable, forkable, and improvable by anyone. We give back.
- **Linux deserves first-class tools.** Big Tech's desktop strategy treats Linux as "too small to bother with." We disagree.

If you're tired of streaming services telling you what you can listen to and watching them pull tracks out of "your" library — there's a way out. Buy your music DRM-free ([Qobuz](https://www.qobuz.com/) for hi-res FLAC, [Bandcamp](https://bandcamp.com/) for direct artist support, [7digital](https://www.7digital.com/) for catalogue) and put it on devices that play files, not licenses.

---

## Why a Garmin

We like Garmin watches because **you don't have to give them a phone.** Most modern fitness watches are useless without pairing to a smartphone running a vendor app that ingests your activity data, your heart rate, your sleep, your location — and uploads it to a cloud you don't control. Garmin watches still work standalone. Buy one, never sign into Garmin Connect, never install the phone app, and the watch still tracks your runs, paces you, and plays the music you put on it.

Combined with Pelican — which puts music on the watch over a USB cable, no account required — you have a complete loop: fitness data stays on the watch, your music stays on your computer, neither leaves unless you decide.

The **Tactix** and high-end **Forerunner** lines are excellent. The **Instinct** line is great if you want a rugged minimalist watch — though note that as of this writing **Instinct models don't have on-watch music**, so this tool won't help you there. Check the spec page before buying for music sync.

For the record: we're not affiliated with Garmin.

---

## What Pelican does

Pelican puts music on the watch, and proves it got there.

1. **Reads your library.** Files or folders, on local disk or a NAS. FLAC, WAV,
   ALAC, AAC, MP3, OGG, Opus, AIFF, WMA — anything ffmpeg can decode.
2. **Tags every track.** Title, artist, album, track, year, genre. It uses the
   file's own tags where they exist, and falls back to your folder layout
   (`Artist/Album/01 - Title.flac`) where they don't. Untagged WAVs from a game
   soundtrack arrive named and grouped correctly. You can override artist,
   album, genre or year for a whole run.
3. **Transcodes to one proven profile.** Every file, whatever its source,
   becomes a CBR 192 kbps, 44.1 kHz stereo MP3 with an ID3v2.3 tag. There is
   no passthrough: one output format means one thing to get right.
4. **Sends each file under a name the watch has never seen.** See
   [the one rule](#the-one-rule-never-reuse-a-name).
5. **Reads every file back and hashes it.** A file counts as delivered only
   when the bytes on the watch match the bytes Pelican made. A mismatch is
   retried once under a fresh name, and reported.
6. **Checks space first.** It refuses a run that would overfill the watch or
   pass Garmin's 500-track limit, before it writes anything.
7. **Cleans up.** Transcodes live in a per-run staging folder under
   `~/.cache/pelican/` and are gone when the run ends, however it ends. Your
   source files are only ever read.

Verified on a Forerunner 165 Music (firmware 2506) on Linux, 2026-09-26: a
24-track album pushed in under a minute, every file hash-verified, confirmed
again by an independent libmtp read-back, and played on the watch.
Details in [`docs/status.md`](docs/status.md).

---

## What Pelican does not do — read this before you fill your watch

**Once a track is on the watch, it stays in the watch's music library until
you factory-reset the watch.** Pelican has no delete, on purpose.

This is Garmin firmware behaviour, not a Pelican limitation, and it has been
reported for years across the Forerunner 245, 265, 645, 945 and 955 and the
fēnix 6 and 8:

- Deleting a file over MTP removes the file and frees the space — but the
  watch's music library keeps listing the track. The entry survives replug
  and reboot, and no longer plays. We measured it: 22 files deleted, 78.5 MB
  freed, all 22 still listed.
- That library is not reachable over MTP. No one has found the file behind it.
- Garmin Express does not reliably clear these entries either.
- The only confirmed cure is **Settings → System → Reset → Delete Data and
  Reset Settings**, which also wipes on-watch history and Garmin Pay.

A delete button that frees space but leaves a dead track in your library
would be a lie with a trash-can icon, so Pelican does not have one. The
command-line tool and the library have no delete capability at all.

**So choose what you send.** Use `--dry-run` to see exactly what will go
across, and with which tags, before anything touches the watch.

Also not supported:

- **Playlists.** The Forerunner 165 silently rejects playlist files sent over
  MTP ([`docs/playlists.md`](docs/playlists.md)). Browse by album and artist
  on the watch instead.
- **Folders inside `/Music`.** The firmware mishandles them; everything goes
  into `/Music` flat and the watch groups by tag.
- **Cover art.** It is stripped. Large embedded art makes the watch reject files.
- **macOS and Windows.** The rebuild is Linux-only for now. The earlier macOS
  port is in git history ([`docs/macos-port.md`](docs/macos-port.md)).

---

## The one rule: never reuse a name

Sending a file to a name that already exists on a Garmin music watch
corrupts **both** copies into broken objects that cannot be deleted, and that
name then fails for every future write — on Linux, macOS and Windows alike
([libmtp#307](https://github.com/libmtp/libmtp/issues/307); we hit it
independently). The watch also remembers names that no longer appear as files.

Pelican therefore never reuses a name. Every file goes out as
`pl00042-Title.mp3`, where the number comes from a per-watch **ledger** —
`~/.local/share/pelican/ledger-<serial>.jsonl`, an append-only log of every
name ever sent. A name is written to the ledger *before* the upload begins,
so even a crash mid-transfer burns it. Every name the watch currently
reports, including broken ones, is treated as taken too.

The ledger also means a track you already sent is skipped next time
(`--resend` overrides this, under a new name).

**Keep the ledger.** It is the only memory of which names are safe. It lives
in your data directory, not on the watch, so back it up with the rest of your
home folder. If you lose it, Pelican still refuses every name the watch can
report, but it can no longer see names the watch only remembers.

---

## Use

```sh
# What is plugged in, how full it is, how many names this machine has used
pelican status

# Preview: every file, and the tags it will get. Transcodes nothing, touches no device
pelican push --dry-run ~/Music/Master\ and\ Commander

# Send it
pelican push ~/Music/Master\ and\ Commander
#   verified  pl00031-Ghost of Time.mp3  ← …/02 - Ghost of Time.flac
#   …
#   14 verified, 0 skipped, 0 failed

# Override tags for a whole run (e.g. a soundtrack whose folder is the album)
pelican push --artist "Various Artists" --genre Soundtrack "/mnt/nas/Music/Sea of Thieves"

# What is in /Music: each entry marked ledger (sent by Pelican), foreign, or stub (broken)
pelican ls

# Every name this machine has sent to this watch
pelican ledger
```

`push` exits non-zero if any file failed. With more than one watch plugged
in, pick one with `--serial`.

---

## Compatibility

| Model | Firmware | Status |
|---|---|---|
| Forerunner 165 Music | 2506 | ✅ Verified end to end on Linux: transcode, send, hash read-back, plays |
| Other Garmin music watches (Forerunner 245/255/265/645/945/955/965 Music, Venu, fēnix, epix, tactix) | — | 🟡 Untested. Same MTP responder family, and the library and name-reuse behaviour above is reported on many of them |
| Watches without on-watch music | — | ❌ Not applicable |

**Want a model on the verified row?** Run the [test below](#verifying-a-new-watch)
and open an issue with the output.

---

## Install

Linux only. Build from source:

```sh
# Needs: Rust 1.89+, ffmpeg, libudev
git clone https://github.com/n0ble-s1x/pelican
cd pelican
cargo build --release -p pelican
sudo install -m 755 target/release/pelican /usr/local/bin/
```

### One-time USB permission (required)

The watch needs a udev rule so your user can talk to it without root:

```sh
sudo install -m 644 udev/70-garmin-mtp.rules /etc/udev/rules.d/
sudo udevadm control --reload
sudo udevadm trigger --action=add --subsystem-match=usb --attr-match=idVendor=091e
```

It has to be numbered below 73. systemd grants device access in
`73-seat-late.rules`, and a rule sorted after that (older Pelican shipped a
`99-` rule) is silently ignored. The rule also deliberately does not mark the
watch as an MTP device for your desktop. The watch allows one connection at a
time, and a file manager that auto-mounts it will block Pelican. If
`pelican status` warns that gvfs is holding the watch, unmount it
(`gio mount -u …`, the warning gives the command).

### Packages

An AUR `PKGBUILD` ([`packaging/aur/`](packaging/aur/)) and a Debian package
config (`cargo deb -p pelican`) are in the tree; neither is published yet. A
graphical app and a Flatpak are planned — see [Roadmap](#roadmap).

---

## Verifying a new watch

```sh
pelican status                       # does it see the watch?
pelican push --dry-run ONE_ALBUM     # are the tags right?
pelican push ONE_ALBUM               # every line should say "verified"
pelican ls                           # new rows marked "ledger", no new "stub"
```

Then unplug the watch and play a few tracks. Report the model, firmware and
results in an issue.

---

## How it works

A Rust workspace with two crates:

- **`pelican-core`**: the library. It does the source walk and tag
  resolution, drives ffmpeg, keeps the ledger and staging folder, and handles
  the MTP transfer. The transfer uses [`mtp-rs`](https://crates.io/crates/mtp-rs)
  and [`nusb`](https://crates.io/crates/nusb), pure Rust with no libmtp and no
  FFI. One session per run, split-header/data transfers (Garmin's firmware
  hangs without them), streamed uploads, and a read-back hash per file. The
  device interface has no delete method.
- **`pelican`**: the command-line tool.

No daemon, no telemetry, no network access at runtime. `unsafe` is denied
workspace-wide, with one documented exception for the gvfs ownership check.

The protocol notes behind all of this, including what the watch answers,
what wedges it and what we tried that failed, are in [`docs/`](docs/):

| Doc | What's in it |
|---|---|
| [`status.md`](docs/status.md) | What the current build does, and what was verified on hardware |
| [`garmin-library-persistence.md`](docs/garmin-library-persistence.md) | The ghost-library problem, community evidence, and the experiments that settled it |
| [`garmin-mtp.md`](docs/garmin-mtp.md) | Full MTP protocol reference for Garmin music watches |
| [`rebuild-plan.md`](docs/rebuild-plan.md) | The requirements the current build is held to |
| [`playlists.md`](docs/playlists.md) | Why playlist writes fail on the FR165 |

---

## Roadmap

- **A graphical app.** A clean desktop UI over `pelican-core` offering only
  what is proven: pick music, preview tags, check space, send, and watch every
  file verify.
- **Flatpak**, so it runs on any distribution.
- **Cancel mid-run**, between files.
- **More verified watches.** This needs owners willing to run the test above.

Not on the roadmap: delete, playlists, or anything else the watch firmware
has not been shown to support.

---

## Privacy / security

- Zero telemetry, zero network access at runtime.
- Everything Pelican keeps is in `~/.local/share/pelican/` (the ledger) and
  `~/.cache/pelican/` (transient staging). Nothing leaves the machine.
- Dependencies are checked with `cargo deny` and `cargo audit`.
- Vulnerability reports → [`SECURITY.md`](SECURITY.md).

---

## Contributing

Read [`CONTRIBUTING.md`](CONTRIBUTING.md). PRs welcome, and hardware reports
from other watch models most of all.

---

## License

Dual-licensed under [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE) at your option.

---

## Acknowledgements

- [`mtp-rs`](https://crates.io/crates/mtp-rs) and [`nusb`](https://crates.io/crates/nusb) — pure-Rust MTP/USB stack we stand on.
- [`better-sync`](https://github.com/Schachte/better-sync) (Schachte) — Go reference implementation that informed our playlist-format research.
- [`go-mtpfs`](https://github.com/ganeshrvel/go-mtpfs) — the upstream PR documenting Garmin's split-header / short-data-phase USB quirks.
- [`libmtp`](https://github.com/libmtp/libmtp) — device-table research and `DEVICE_FLAGS_ANDROID_BUGS` lineage that explained the firmware family.
- Pattern inspiration from [Pop!_OS COSMIC Files](https://github.com/pop-os/cosmic-files) (GPL-3.0 — pattern only, no code borrowed).
- Everyone who keeps fighting for the open web. Keep going.

— *Krypteia, 2026*
