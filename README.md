# Krypteia · Pelican

**Sync your music to a Garmin watch on Linux. Without renting it from anyone.**

A small Rust tool — single static binary, no webview, no telemetry — that drops music files onto a Garmin watch over USB/MTP. It exists because Garmin Express is Windows/Mac only, the Linux MTP stack is fragile, and we shouldn't have to choose between owning our media and using the platform we want.

A [Krypteia](https://krypteia.example) project.

---

## Why this exists

Krypteia is a privacy and security consulting practice. Our mission is to help people take back their digital sovereignty — to understand the systems they live inside, and to choose tools that don't make them the product.

This tool is part of that. We believe:

- **Privacy is a prerequisite for freedom.** What you listen to, when, and where — that's nobody's business but yours.
- **Own your data. Own your media.** Buy it once, keep it forever, no subscription tax, no licensing terms that change under you. Music is yours; treat it that way.
- **Open source is a public good.** Useful software should be inspectable, forkable, and improvable by anyone. We give back.
- **Linux deserves first-class tools.** Big Tech's desktop strategy treats Linux as "too small to bother with." We disagree.

If you're tired of Spotify/Apple Music telling you what you can listen to and when — and watching them pull tracks out of "your" library — there's a way out. Buy your music DRM-free (we'd suggest [Qobuz](https://www.qobuz.com/) for hi-res FLAC, [Bandcamp](https://bandcamp.com/) for direct artist support, [7digital](https://www.7digital.com/) for catalogue) and put it on devices that play files, not licenses.

This tool is one small piece of that. Drop a folder of MP3s or FLACs onto your watch and run.

---

## Why a Garmin

We like Garmin watches because **you don't have to give them a phone.** Most modern fitness watches are useless without pairing to a smartphone running a vendor app that ingests your activity data, your heart rate, your sleep, your location — and uploads it to a cloud you don't control. Garmin watches still work standalone. You can buy one, never sign into Garmin Connect, never install the phone app, and the watch still tracks your runs, paces you, and plays the music you put on it.

That's a real privacy win. The watch is a tool you own, not a sensor harvesting your life. Combined with this app — which puts music on the watch over a USB cable, no account required — you have a complete loop: fitness data stays on the watch, your music stays on your computer, neither leaves unless you decide.

If you do want to pair to a phone for some features (LTE, music streaming, smart notifications), the **Tactix** and high-end **Forerunner** lines are excellent. The **Instinct** line is great if you want a rugged minimalist watch — though note that as of this writing **Instinct models don't have on-watch music**, so this tool won't help you there. Check the spec page before buying for music sync.

For the record: we're not affiliated with Garmin. We just appreciate that they ship a product that respects "I don't want your cloud."

---

## What it does

- Drag a folder of music onto the **WATCH** pane → upload to your Garmin's `/Music` or `/Audiobooks`
- Auto-transcode FLAC, OGG, Opus, WMA, AIFF, ALAC, M4A → CBR 192 kbps MP3 (via `ffmpeg`)
- Strip non-standard ID3 frames that confuse Garmin's music indexer
- Truncate filenames to the 56-char cap Garmin firmware quietly enforces
- Local **playlists** — group tracks together, "Send →" mass-uploads them
- Right-click delete on watch entries (handles broken stubs from prior failed syncs)
- Per-device upload journal (persists across runs)

---

## What it does **not** do (yet)

- **Real on-watch playlists.** Garmin Express historically synced iTunes/WMP playlists via vendor-specific MTP operations (`0x9000-0x900B`). We haven't reverse-engineered them yet. Local-only playlists are a stopgap. Help wanted.
- **Pull-from-watch / library browsing.** Garmin firmware doesn't expose its indexed music database over MTP. Only the staging folder is browseable.
- **Windows.** No plans. Use Garmin Express, or — and we say this with affection — consider that Linux is right there. It's free, it's better, and it doesn't ask permission to update itself in the middle of a 5K.
- **macOS.** Possible eventually. PRs welcome.
- **Models we haven't tested.** See compatibility below.

---

## Compatibility

**Tested and verified working:**

| Model | Firmware | Notes |
|---|---|---|
| Forerunner 165 Music | 2506 | All features verified end-to-end |

**Untested, but no reason it shouldn't work:** any music-capable Garmin that exposes itself as MTP — Forerunner 245/255/265/645/745/945/955/965 Music, Fenix 5 Plus / 6 / 7 / 8 (music variants), Venu 2/3, Epix Gen 2, Tactix Delta/7. The firmware quirks we work around (filename length cap, tag-frame allowlist, split-header MTP) appear consistent across the line.

**Want a model added to the tested matrix?** We'll happily make it work — but we need hardware. Send a PR with model-specific quirks if you find any, or [reach out](https://krypteia.example) if you want to ship us a unit. Free tactix would be cool. 😉

---

## Install

### From source (any distro)

```sh
# Prerequisites: Rust 1.78+, ffmpeg, libudev
git clone https://github.com/n0ble-s1x/pelican
cd pelican
cargo build --release

# Install the udev rule so you don't need root to talk to the watch
sudo install -m 644 udev/99-garmin-music.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

### Arch Linux (AUR — coming with first release)

```sh
yay -S pelican
```

### Flatpak / AppImage / `.deb`

Planned for v0.2. PRs welcome.

---

## Use

Plug your watch in. Put it in MTP USB mode. Run:

```sh
pelican
```

Drag a folder of music onto the **WATCH** pane. Watch the green dots. Done.

Or headless:

```sh
# Upload an album
pelican --headless --copy ~/Music/Album

# Delete files by remote path
pelican --headless --delete "Music/foo.mp3"

# List playlists already on the watch
pelican --headless --list-playlists
```

---

## How it works (the short version)

The watch's `/Music` folder is the only writable music-target over MTP. Garmin's
firmware silently rejects writes that don't match a narrow profile, so we:

1. **Normalize audio** through `ffmpeg`: CBR 192 kbps MP3, 44.1 kHz stereo,
   strip embedded album art (`-vn`), no ID3v1, ID3v2.3 only with a strict tag
   allowlist (`title`, `artist`, `album`, `track`, `date`, `genre` —
   non-standard frames trigger silent reject)
2. **Sanitize filenames** to ≤ 56 ASCII chars (Garmin firmware quietly drops
   writes whose name exceeds ~60 chars)
3. **Set `split_header_data(true)`** on the PTP session — without it, uploads
   time out at 30 s
4. **List with `get_object_handles` + per-handle `get_object_info`** — broken
   stubs from prior partial uploads stay visible (and deletable) instead of
   blanking the listing

Long-form notes for anyone hacking on this live in [docs/garmin-mtp-notes.md](docs/garmin-mtp-notes.md).

---

## Privacy / security stance

- **Zero telemetry.** This binary phones home to nothing. Ever.
- **No internet access required.** Everything runs locally.
- **Per-device journal lives on your disk** at `~/.local/share/pelican/`.
  Nothing leaves the machine.
- **Dependency surface kept narrow** and audited via `cargo audit` in CI.
- **Reproducible builds** via `Cargo.lock` checked in.

If you find a security issue, please follow [SECURITY.md](SECURITY.md).

---

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). PRs welcome. We do code review before
merge — for security and quality, not gatekeeping. New device support, packaging
help, and reverse-engineering of Garmin's vendor MTP ops are all wanted.

---

## License

Dual-licensed under [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE) at your
option. Pick whichever fits your project.

---

## Acknowledgements

- Pattern inspiration from [Pop!_OS COSMIC Files](https://github.com/pop-os/cosmic-files) (GPL-3.0). No code borrowed.
- MTP protocol work standing on the shoulders of [`mtp-rs`](https://crates.io/crates/mtp-rs) and `nusb`.
- And everyone who keeps fighting for the open web. Keep going.

— *Krypteia, 2026*
