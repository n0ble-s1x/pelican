# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Tauri 2 (Rust core + system webview), Linux-first. The frontend is **plain
HTML/CSS/JS, with no npm, framework or bundler** (reconfirmed by the owner
2026-09-26). A JS package manager would add a second supply chain that
`SECURITY.md`'s `cargo deny` / `cargo audit` commitment does not reach.
Distribution target: Flatpak (the GNOME runtime supplies webkit2gtk) and AUR.
The frontend talks to `pelican-core` through Tauri commands and the core's
serializable `Progress` event stream.

## Users

**Primary: the maintainer.** Senior engineer, Linux daily driver, music
kept on a NAS (`/mnt/nas/Music`, NFS), whole albums and soundtracks, often
untagged WAV/FLAC, sent to a Forerunner 165 Music before runs. The interface
is tuned to that workflow first.

**Secondary: the public.** Pelican is open source; a Garmin music-watch owner
who will not use Garmin Express or a vendor account must still be able to succeed
on first run without understanding MTP, tags or transcoding.

## Product Purpose

Put music on a Garmin watch from Linux, over a USB cable, with no account and
no cloud, and **prove every file arrived intact**. Pelican transcodes to one
profile the watch reliably plays, sends each file under a name the watch has
never seen, reads it back and compares hashes.

Success: plug in, choose music, see exactly what will happen, send, and watch
every file verify, without ever producing a broken entry on the watch.

## Positioning

Every generic Linux MTP tool loses or corrupts files on these watches, and a
same-name write poisons the name forever (libmtp#307). Pelican's mechanism
(one proven profile, a never-reuse ledger, hash read-back) is the only path
verified end to end on hardware (FR165 Music, 2026-09-26: 24/24 verified,
cross-checked by libmtp, played on the watch). No telemetry, no daemon, no
network access.

## Operating Context

At a desk, cable attached, before a run:

1. Plug the watch in.
2. Choose music, usually whole albums from the NAS.
3. Review what will be sent: tags, size, the space left.
4. Send; each file transcodes, sends, and verifies (~2 s per track).
5. Unplug, go running.

Sessions are short and occasional. The watch holds about 3.7 GB and 500 tracks.

**Session shape (owner's call):** a **guided flow** is the default (connect →
choose → review → send), because it teaches the watch's limits as it goes.
"Choose" is a **library explorer** (owner, 2026-09-26: "don't make folks type
paths"): places down the side (Home, Music, the library, mounted network
shares, removable drives, the whole computer), a folder tree that opens in
place, and a tick on every folder and every song. No path is ever typed.
Dropping folders on the window still works.

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
- Skip tracks already verified on this watch. **Send again** (owner,
  2026-09-26): per song from Review, or for every skipped song at once. Each
  goes as another copy under a fresh name (the core never reuses one).
- Watch status: model, serial, free/total space, `/Music` count, ledger totals.
- Read-only view of `/Music`: each entry is `ledger` (Pelican sent it),
  `foreign`, or `stub` (broken).
- The ledger itself, viewable.
- **A watch that stops answering** (seen on hardware 2026-09-26 after a watch
  reboot) is named as such, with the fix in plain words: "The watch isn't
  answering. Unplug it, wait five seconds, plug it back in." No docs needed.
- **Back up the watch** (read-only): copies the watch's whole `GARMIN` folder
  (activities, sleep, health monitoring, records, settings) to
  `~/Documents/Pelican/<model> backup <date>`. Never writes to the watch.
- **Start over** after a factory reset: a four-step walkthrough (what a reset
  erases → back up first → the reset steps on the watch → plug back in).
  Pelican reads `/Music` itself and resets its ledger for that watch only if
  no audio remains; otherwise it refuses and says why. Pelican never resets
  the watch.

**Owner-requested: playlists, sent as albums.** Built in the core and the
window, and **not yet hardware-tested**. The watch rejects MTP playlists, so a
user-ordered **Playlist** is sent as an **album**: album tag = playlist name,
album artist "Various Artists", track numbers = playlist order, each song
keeping its own artist. In the window: tick songs across any folders, "Make a
playlist", name it, Enter → Review (reorder there) → Send, with one honest
line: "On the watch it appears under Albums, in this order." A song already
on the watch goes again as a new copy inside the playlist (the skip key is
audio + album). The IPC keeps the core's name, `mix`.

**Hard constraints the design must respect:**

- **Permanence.** A track sent to the watch stays in its music library until a
  factory reset. MTP delete frees space but leaves a dead library entry, so
  Pelican has **no delete**. This must be felt *before* sending, not footnoted.
  Said the same way everywhere (Review, On the watch, Start over): nothing can
  be deleted one song at a time; only a factory reset clears the watch, and it
  erases **everything**: activities, health data, settings, Garmin Pay and
  music.
- **No playlists on the watch.** Real MTP playlist writes fail on the FR165
  and each attempt leaves an undeletable stub. Never imply otherwise.
- One MTP connection at a time; a desktop auto-mount (gvfs) blocks Pelican.
- `/Music` is flat; no folders on the watch. Cover art is not sent.
- No cancel mid-file. A run can stop between files: the window offers "Stop
  after this song", and the file in flight is finished and proven first.

## Brand Commitments

- Name **Pelican**, a **Krypteia** project. MIT OR Apache-2.0.
- **Zero telemetry, no network access, no background daemon.** This is the
  core promise, and it binds the UI too (no webfonts from the network, no remote
  images).
- **Aesthetic pinned by the owner, 2026-09-26: the modern Bond register (007
  First Light, Skyfall, Spectre). Classy, elegant, simple.** Craft and restraint
  in the manner of those films' title design and the First Light identity, not
  a spy-gadget or MI6-terminal pastiche. Pelican is public, so it carries **no
  Bond or Omega marks**: no 007 logo, gun barrel or franchise type.
- **The watch silhouette** (owner, 2026-09-26): a classic dive watch in the
  Seamaster manner, with a scalloped unidirectional bezel, twisted lyre lugs,
  guarded crown at 3, helium-valve crown at 10, bracelet. **Silhouette only**:
  no name, logo, dial text or any other mark.
- **Motion** (owner, 2026-09-26: "some dynamic motion and animations would be
  fun"): title-card transitions, the bezel clicking round per proven track,
  the room arc, the credits roll, ink blooms. Classy, never busy.
- Rejected: any generic file-manager feel, and a UI full of knobs (there is
  one profile, so no bitrate pickers or advanced toggles).

## Evidence on Hand

- Real hardware: Forerunner 165 Music, FW 2506. Hardware acceptance record in
  `docs/status.md` and `docs/garmin-library-persistence.md` § Results.
- Real content for design: the owner's NAS albums, *Master and Commander:
  The Far Side of the World* (FLAC, tagged), *Sea of Thieves* (25 untagged
  WAVs, path-derived tags), *Windrose* (33 WAVs).
- No user research, testimonials or install numbers exist. Never invent them.
- Only one watch model tested; others must not be claimed.

## Product Principles

1. **Proof over promise.** Every file ends verified or visibly failed; the
   interface shows the proof, not a spinner and a hope.
2. **Permanence is a first-class fact.** What goes on the watch stays. The
   interface makes that weight clear at the moment of choosing, calmly and not
   as a scare.
3. **Never lie about the device.** No delete, no playlists, no folders, and no
   interface that pretends otherwise.
4. **Simple to the point of quiet.** One profile, few choices, the right
   default everywhere. The firmware's quirks are Pelican's problem.
5. **Nothing leaves the machine.** The promise is kept in the code, in
   SECURITY.md and in the `check.sh` invariants, not in window copy.

## Accessibility & Inclusion

WCAG 2.1 AA as the floor (achievable in a webview). Color never signals
alone: every state also says its name in words.
