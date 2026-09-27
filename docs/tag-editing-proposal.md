# Proposal: editing tags in the user's own files

**Status: proposal. Not implemented, not scheduled, nothing in the build does
any of this.** Written 2026-09-06, when the dead `Add tags…` button was
removed from the missing-tag notice in `ui/index.html`.

## Why there is a document instead of a button

The notice that warns "3 tracks have no artist" carried a `<button disabled>Add
tags…</button>` next to it. It had no id, no handler, no class anything
selected, and there was no tag-writing command in
`crates/pelican-shell/src/commands.rs`. Nothing in the codebase could ever have
enabled it. A greyed control reads as *available under some condition you have
not met*; this one meant *never implemented*, which is a different sentence and
the screen was not saying it.

It was removed rather than wired up, because wiring it up is not a small job.

## What Pelican does today, which is the thing that would change

Pelican has never written to a file the user gave it. `normalize` copies the
source into the cache directory and `retag_in_place` only ever writes that
staged copy — `crates/pelican-core/src/transcode/mod.rs`. The source tree is
read-only to this program, and that is currently a property a user can rely on
without being told.

Tag editing ends that. It is the first destructive operation Pelican would
perform on the library rather than on the watch, and it deserves the same
scrutiny the delete path got.

## What a design has to answer first

1. **Which formats.** The six-field allowlist is written to the *staged* copy
   today, so the writer only ever had to handle the encoder's output container.
   Writing to sources means MP3 (ID3v2, and which version), FLAC (Vorbis
   comments, plus files that also carry ID3v2), M4A, and every combination
   lofty will hand back.

2. **The WAV case, which has no good answer.** The owner's own library is the
   example: 58 WAVs carrying no tag of any kind — a RIFF chunk walk shows only
   `fmt `, `bext`, `junk` and `data`. Pelican could write RIFF INFO or ID3v2
   into them, but neither is evidence the watch would then index them, and
   `docs/garmin-mtp.md` does not record a tagged-WAV observation. Writing a tag
   that does not fix the problem, into a file that was fine before, is worse
   than doing nothing. Do not ship the WAV path on a guess; get an observation
   first.

3. **Partial understanding.** A file the parser only half understands is
   exactly the file a writer can destroy. `readable: false` files must be
   refused outright — the two truncated FLACs in the owner's library, missing
   their `fLaC` marker, are the live example.

4. **Backup and undo.** Copy-then-write-then-swap at minimum. "It rewrote my
   library and I cannot get back" is unrecoverable in a way nothing else in
   this app is.

5. **Where the values come from.** Typed by hand, or read from somewhere? See
   below.

## The sidecar, which may be the better feature

The owner's folders carry Jellyfin-style `album.nfo` sidecars holding
`<title>` and a per-track title list. His media server shows titles from those;
the audio bytes carry none. That is a real and common shape for a self-hosted
library, and reading it is *not* destructive.

If it is done, the UI must label it as coming from the sidecar and not from the
file, because the two are different facts and the watch only reads one of them —
a title Pelican shows from an `.nfo` is still a title the watch will not see. It
does not fix the music-app problem on its own; combined with tag writing it
would supply the values, which is the safer half of a risky feature.

Scope it separately from the writer.

## What was done instead

The notice now names the field that is actually missing and names the remedy in
prose — add the tag in whatever tagger you use, then choose the folder again.
That is what the disabled button was standing in for, it costs no pixels, and
it promises nothing Pelican cannot do.
