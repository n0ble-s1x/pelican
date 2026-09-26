# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Tauri 2 (Rust core + system webview), Linux-first. The frontend is **plain
HTML/CSS/JS — no npm, no framework, no bundler** (reconfirmed by the owner
2026-09-26). A JS package manager would add a second supply chain that
`SECURITY.md`'s `cargo deny` / `cargo audit` commitment does not reach.
Distribution target: Flatpak (GNOME runtime supplies webkit2gtk) plus AUR.
The frontend talks to `pelican-core` through Tauri commands and the core's
serializable `Progress` event stream.

## Users

**Primary: the maintainer.** Senior engineer, Linux daily driver, music
kept on a NAS (`/mnt/nas/Music`, NFS), whole albums and soundtracks — often
untagged WAV/FLAC — sent to a Forerunner 165 Music before runs. The interface
is tuned to that workflow first.

**Secondary: the public.** Pelican is open source; a Garmin music-watch owner
who refuses Garmin Express / a vendor account must still be able to succeed
on first run without understanding MTP, tags or transcoding.

## Product Purpose

Put music on a Garmin watch from Linux, over a USB cable, with no account and
no cloud — and **prove every file arrived intact**. Pelican transcodes to one
profile the watch reliably plays, sends each file under a name the watch has
never seen, reads it back and compares hashes.

Success: plug in, choose music, see exactly what will happen, send, and watch
every file verify — without ever producing a broken entry on the watch.

## Positioning

Every generic Linux MTP tool loses or corrupts files on these watches, and a
same-name write poisons the name forever (libmtp#307). Pelican's mechanism —
one proven profile, a never-reuse ledger, hash read-back — is the only path
verified end to end on hardware (FR165 Music, 2026-09-26: 24/24 verified,
cross-checked by libmtp, played on the watch). No telemetry, no daemon, no
network access.

## Operating Context

At a desk, cable attached, before a run:

1. Plug the watch in.
2. Choose music — usually whole albums from the NAS.
3. Review what will be sent: tags, size, the space left.
4. Send; each file transcodes, sends, and verifies (~2 s per track).
5. Unplug, go running.

Sessions are short and occasional. The watch holds ~3.5 GB and 500 tracks.

**Session shape (owner's call):** a **guided flow** is the default — connect →
choose → review → send — because it teaches the watch's limits as it goes.
A **library view** (browse the NAS by artist/album, tick what goes) is
available as an alternative way into "choose".

## Capabilities and Constraints

**Proven (the rebuilt core, hardware-verified 2026-09-26):**

- Any ffmpeg-decodable source → CBR 192 kbps 44.1 kHz stereo MP3, ID3v2.3.
  No passthrough, no options: one profile.
- Tags: title, artist, album artist, album, track, year, genre. Resolved from
  the file, else from the folder layout; run-wide overrides for artist, album,
  genre, year. A file with no resolvable title is refused.
- Never-reused remote names from a per-watch append-only ledger; a name is
  burned before its upload starts.
- Read-back SHA-256 per file; one retry under a fresh name on mismatch.
- Capacity check (free space + 2 MiB, 500 audio objects) before any write.
- Skip tracks already verified on this watch (override: resend).
- Watch status: model, serial, free/total space, `/Music` count, ledger totals.
- Read-only view of `/Music`: each entry is `ledger` (Pelican sent it),
  `foreign`, or `stub` (broken).
- The ledger itself, viewable.

**Planned, owner-requested — "Mixes" (playlist as album):** the watch rejects
MTP playlists, so a user-ordered mix is sent as an **album**: album tag = mix
name, track numbers = mix order, each song keeping its own artist. It appears
on the watch under Albums and plays in order. A song in two mixes is two
files. Needs a small core addition (per-file track override, album-artist
distinct from artist). Not yet built or hardware-tested.

**Hard constraints the design must respect:**

- **Permanence.** A track sent to the watch stays in its music library until a
  factory reset. MTP delete frees space but leaves a dead library entry, so
  Pelican has **no delete**. This must be felt *before* sending, not footnoted.
- **No playlists on the watch.** Real MTP playlist writes fail on the FR165
  and each attempt leaves an undeletable stub. Never imply otherwise.
- One MTP connection at a time; a desktop auto-mount (gvfs) blocks Pelican.
- `/Music` is flat; no folders on the watch. Cover art is not sent.
- No cancel mid-file; a run can stop between files (core work pending).

## Brand Commitments

- Name **Pelican**, a **Krypteia** project. MIT OR Apache-2.0.
- **Zero telemetry, no network access, no background daemon** — the core
  promise; it binds the UI too (no webfonts from the network, no remote
  images).
- **Aesthetic pinned by the owner, 2026-09-26: the modern Bond register — 007
  First Light, Skyfall, Spectre. Classy, elegant, simple.** Craft and restraint
  in the manner of those films' title design and the First Light identity, not
  a spy-gadget or MI6-terminal pastiche. Pelican is public, so it carries **no
  Bond or Omega marks** — no 007 logo, gun barrel, or franchise type.
- Rejected: the old "UNSC tactical" theme; any generic file-manager feel; a UI
  full of knobs (there is one profile — no bitrate pickers or advanced toggles).

## Evidence on Hand

- Real hardware: Forerunner 165 Music, FW 2506. Hardware acceptance record in
  `docs/status.md` and `docs/garmin-library-persistence.md` § Results.
- Real content for design: the owner's NAS albums — *Master and Commander:
  The Far Side of the World* (FLAC, tagged), *Sea of Thieves* (25 untagged
  WAVs, path-derived tags), *Windrose* (33 WAVs).
- No user research, testimonials or install numbers exist — never invent them.
- Only one watch model tested; others must not be claimed.

## Product Principles

1. **Proof over promise.** Every file ends verified or visibly failed; the
   interface shows the proof, not a spinner and a hope.
2. **Permanence is a first-class fact.** What goes on the watch stays. The
   interface makes that weight clear at the moment of choosing — calmly, not
   as a scare.
3. **Never lie about the device.** No delete, no playlists, no folders — and no
   interface that pretends otherwise.
4. **Simple to the point of quiet.** One profile, few choices, the right
   default everywhere. The firmware's quirks are Pelican's problem.
5. **Nothing leaves the machine** — in the code and visibly in the interface.

## Accessibility & Inclusion

WCAG 2.1 AA as the floor (achievable in a webview). Colour never signals
alone: every state also says its name in words.
