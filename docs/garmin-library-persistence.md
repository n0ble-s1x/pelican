# The library that will not let go

**Outcome (settled 2026-09-26).** A clean upload under a new name plays on
the watch, and mtp-rs writes as cleanly as libmtp. The results are at the
end of this file. They are why Pelican never reuses a name, reads every
file back, and has no delete.

The rest of this file is the question as it stood on 2026-09-25, before the
work moved to Linux: what was unknown, what the community had established,
and the experiments that settled it.

## The problem, stated honestly

On the reference **Forerunner 165 Music, firmware 2506**:

1. A batch of tracks pushed from Linux (before the macOS port existed) appears
   in the watch's music app and **does not play**. The owner reports one track
   once produced audio at its start, and nothing since.
2. `DeleteObject` over MTP removes files from `/Music` and reclaims space
   (78.5 MB verified), but the watch's music app still lists the tracks.
3. Those entries survive a USB replug and a full watch reboot.
4. Nothing Pelican has produced, on either platform, has ever been confirmed
   to play on the watch. See `archive/macos-port.md` § Retracted.

So there are two distinct unknowns, and they have been repeatedly conflated:

| Unknown | Status |
|---|---|
| Can a library entry be removed? | No known way over MTP |
| Can a file Pelican writes be played at all? | **Never tested on a clean device** |

The second has never had a fair trial, because every attempt was made on a
device already carrying the wreckage from the first.

## What the community has established

This is not a Pelican bug and not a macOS bug. It is a Garmin firmware
behavior, reported for years across the FR245, FR265, FR645, FR945, FR955,
fēnix 6 and fēnix 8.

**Ghost library entries are real, and a factory reset is the only confirmed
cure.** An FR265 owner working from a Mac with libmtp describes a device-side
database that permanently remembers any filename that has ever been on the
watch, which MTP deletion does not touch; their conclusion is *"The only fix:
factory reset the watch."*
([PSA thread](https://forums.garmin.com/apps-software/mac-windows-software/f/garmin-express/440708/psa-why-your-mp3s-disappear-on-garmin-watches-via-mtp-and-how-to-actually-fix-it))
An FR645 owner with an undeletable track that stalled on play resolved it only
by hard reset
([thread](https://forums.garmin.com/sports-fitness/sports-fitness/f/forerunner-645-645-m/147141/hard-reset-question-trying-to-get-rid-of-a-pesky-song)).
On a fēnix 6 the playlists outlived their files and a restart did not help
([thread](https://forums.garmin.com/outdoor-recreation/outdoor-recreation/f/fenix-6-series/213228/my-music-playlists-remain-after-deleting-corresponding-folder-files)).

**Garmin Express does not reliably clear it either.** An FR955 owner deleted
through Express and reported the track info stayed on the watch
([thread](https://forums.garmin.com/apps-software/mac-windows-software/f/garmin-express/381908/issue-with-deleting-music-and-audiobooks-from-garmin-watch)).
Express is therefore not a guaranteed escape hatch, and not a reason to keep
Pelican alive on its own.

**Files written from Linux and macOS can arrive empty or truncated.** An FR955
owner found *"the files I can copy to my 955 are in fact empty (from linux)"*
([thread](https://forums.garmin.com/sports-fitness/running-multisport/f/forerunner-955-series/402291/how-to-copy-music-under-linux)).
Others report roughly a 30% success rate, 0-byte objects that vanish on
replug, and better results after inserting a delay between files. **This is
the most likely explanation for listed-but-unplayable tracks**, and it is
testable without touching the library at all.

**What people say works:** plain MP3 carrying basic ID3 (title, artist, album,
year); short unique filenames that are never reused, so a new file cannot
collide with a remembered name; no MTP track-metadata objects, letting the
watch read tags from the file itself; and verifying afterwards that the object
really is on the device at full size rather than trusting the success return.

Pelican's Linux path already produces exactly the format those reports favor:
ffmpeg, CBR 192 kbps MP3, ID3v2.3, strict tag allowlist. The macOS path
produces AAC in M4A, which several reports specifically advise against.

## The experiments

1. **Byte-level round trip.** `Backend::download_file` exists and works. Pull
   a track back off the watch and `sha256` it against the source. If Pelican's
   uploads are truncated or empty, this finds it in one command and explains
   everything downstream. *Nothing in this repo has ever checked this.*
2. **A clean-device playback test.** One tagged MP3, a short unique name, onto
   a watch with no ghost entries, then play it.
3. **A second, independent MTP stack.** `mtp-sendtr -q` from libmtp, and
   `gio copy` over gvfs, the method the FR955 owner had working. If those
   land whole files and `mtp-rs` does not, the defect is ours and fixable.
4. **Watching Garmin Express on the wire.** `usbmon` plus Wireshark is easy on
   Linux, but Express does not run there; it needs a Windows guest with USB
   passthrough. Worth it only if a re-index command is the last thing missing.

## Why Linux is the right place to try

- `usbmon` gives real USB capture with no special tooling.
- libmtp and gvfs are both first-class, giving two independent stacks to
  cross-check `mtp-rs` against.
- ffmpeg is present, so the MP3 profile the community reports success with is
  the default rather than the fallback.
- `udev` rules and `gio mount -u` make contention explicit and fixable, which
  on macOS it is not.

## The decision rule

Run experiment 1 first: it is read-only, costs nothing, and needs no reset.

- **Uploads are truncated** → a real bug in our transport, fix it and retest.
- **Uploads are byte-perfect and still do not play** → the format or the
  ghost-collision is the cause. Factory reset (owner's call: it erases
  on-watch history; anything already in Garmin Connect survives), then
  experiment 2 with a short unique filename.
- **A clean device plays a fresh MP3** → Pelican needs three changes: never
  reuse a remote filename, verify every upload by reading it back, and state
  plainly that delete cannot clear the library.
- **A clean device still refuses to play** → the watch gates playback on
  something outside MTP's reach. Archive the project; the read-side work
  survives in agoge for pulling FIT files.

## Results: Linux, 2026-09-26

Run on the maintainer's workstation (CachyOS, kernel 7.2.7, libmtp 1.1.23, systemd 262) against the
reference FR165 Music, FW 2506. Experiments 1 and 2 above, and the first half of 3.

**Access.** libmtp does not list `091e:5151`; it still connects via the
vendor-class interface probe ("UNKNOWN in libmtp", cosmetic). The node had no
user ACL: the former `99-garmin-music.rules` could never work on systemd, because
`uaccess` is applied by `RUN{builtin}+="uaccess"` in `73-seat-late.rules` and a
`TAG` added at 99 arrives after it. A rule at `70-` fixes it.

**Experiment 1: read-back of what is on the watch.** Three audio objects
exist (`/Music` m4a + wav, `/Audiobooks/probe.mp3`), all Mac-era. Each came
back at full listed size and decodes end to end with zero ffmpeg errors. The
earlier Linux batch is no longer present as objects, so *its* integrity cannot
be rechecked.

**Playback on the watch** (owner, same day): the m4a (AAC 192k) plays;
`probe.mp3` (CBR 192k, ID3v2.3) plays from Audiobooks; the WAV is not listed
(no ID3 title/artist, only BWF/REAPER tags). Every other entry in the music
app is a ghost with no object behind it, including duplicate "versions" of the
same track and an entry named "6. 2". **The watch plays every intact file it
has; the failures are ghosts and damaged writes.**

**Experiment 2: fresh upload.** A NAS WAV (an album never before on the
watch) → ffmpeg `-map_metadata -1 -c:a libmp3lame -b:a 192k -ar 44100 -ac 2
-id3v2_version 3`, explicit title/artist/album_artist/album/track/date/genre →
`mtp-sendfile pl0001.mp3 Music/pl0001.mp3` (plain object, no MTP track
metadata) → `mtp-getfile` read-back: **SHA-256 identical**
(`490020e8…6eaaefb4`, 3 807 097 B). Owner confirms it **appears and plays
through**.

**Upstream corroboration found today.** libmtp issue #307 (2025-06, FR645):
sending to a path that already exists turns both objects into stubs,
DeleteObject on them errors, and the path stays poisoned for every later send,
Windows included; factory reset only. That is this repo's "collision destroys
both files" and "stubs cannot be deleted", observed independently.

**Verdict under the decision rule:** a clean upload plays, even on a device
still carrying ghosts. Pelican continues, with three changes: never reuse a
remote filename (persistent per-serial ledger), verify every upload by
read-back hash, and state plainly that delete cannot clear the library.
Whether mtp-rs writes as cleanly as libmtp did was settled by the rebuild's
first hardware run the same day: 24 of 24 files verified by read-back hash
and confirmed by an independent libmtp read-back (`status.md`).
