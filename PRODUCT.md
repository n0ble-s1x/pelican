# Product

> **History.** This is the design brief for the GUI that the `rebuild`
> branch removed (`docs/rebuild-plan.md`). Pelican is now the `pelican`
> command-line tool: `push`, `status`, `ls` and `ledger`. The brief is kept
> for its reasoning; where it describes the interface, playback, playlists
> or removing files, it describes something Pelican no longer has.
> "Works today" and the session constraint below are current.

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Tauri (Rust core + system webview), with the frontend as **plain HTML/CSS/JS —
no npm, no framework, no bundler**. Confirmed by the owner 2026-09-01 after
weighing SwiftUI against Tauri.

The npm-free constraint is a product requirement, not a preference: Pelican's
`SECURITY.md` commits to a narrow, audited dependency surface, and `cargo deny`
/ `cargo audit` cover the whole graph today. Adding a JS package manager would
introduce a second supply chain that commitment does not reach.

macOS is the first target; Linux follows when hardware is available. The
existing egui GUI stays as the Linux build until then.

## Users

**Primary:** anyone who owns a Garmin music watch and does not want to use
Garmin Express or a vendor cloud account. Public open-source product — the
design target is a stranger on first run, not the maintainer.

They typically own their music outright (DRM-free purchases from Qobuz,
Bandcamp, 7digital) and keep it on local disk or a NAS. Many chose Garmin
specifically because the watch works standalone without pairing a phone.

Technical literacy is **not** assumed. The interface must not require
understanding MTP, USB, tags, or transcoding to succeed.

## Product Purpose

Put music from your computer onto your Garmin watch over a USB cable, with no
account, no cloud, and no vendor software — and let you **listen to it first**.

Scope expanded 2026-09-01: Pelican is a small local music player that also
syncs. You browse a library on local disk or a NAS, play tracks to audition
them, build playlists, and push a chosen set to the watch. Auditioning matters
because the watch holds ~3.7 GB: committing a track is a real decision, and
choosing blind is the thing that makes the current tool tedious.

Playback is close to free in this stack — the system webview decodes MP3, AAC,
ALAC, FLAC and WAV natively — which is a direct dividend of choosing Tauri over
SwiftUI.

Success is a first-time user going from "watch plugged in" to "music on the
watch" without reading documentation, and without being asked to understand
anything about the protocol underneath.

## Positioning

Garmin Express is Windows and macOS only, and routes through a vendor account.
The Linux MTP stack is fragile. Pelican is a single local tool that talks to
the watch directly over USB — **no telemetry, no daemon, no network access at
all.** Nothing about what you listen to leaves your machine.

The differentiating mechanism is hard-won firmware knowledge, not UI: strict
tag rewriting, filename sanitisation, per-file MTP sessions, and broken-stub
recovery are what make transfers actually land on Garmin hardware. Competing
generic MTP file managers fail on this device family for exactly these reasons.

## Operating Context

The whole session happens at a desk with a cable attached:

1. Plug the watch in over USB.
2. Choose music from local disk or a mounted NAS.
3. Wait through a transfer of a few files to a few hundred.
4. Unplug and go running.

Sessions are short, occasional, and interruptible. The watch holds roughly
3.7 GB, so users are managing a small curated subset of a much larger library —
deciding what comes *off* matters as much as what goes on.

## Capabilities and Constraints

**Works today (the rebuilt CLI; the hardware acceptance run is pending):**

- Take MP3, M4A, AAC, WAV, FLAC, ALAC, AIFF and more; every file is
  transcoded by ffmpeg to one MP3 profile before the watch is opened.
- Send each file under a name never used on that watch, read it back, and
  compare it byte for byte before calling it done.
- Report free space and device identity.
- A per-device ledger of every name used and what became of it.

**Constraints the design must respect:**

- **One MTP session at a time.** Any concurrent operation fails.
- **One session per run.** The rebuild lists `/Music`, checks capacity and
  sends every file in a single session, proving each by read-back
  (`docs/rebuild-plan.md` R6). The old GUI opened a fresh session per file;
  the rebuild does not, and the hardware acceptance run is what will
  confirm the watch accepts it.
- **Filenames are capped at 56 characters** and FAT-hostile punctuation is
  stripped; the tag carries the real title.
- **Files without title+artist tags are invisible** on the watch even though
  they transfer successfully.
- **The watch does not expose its indexed library *listing* over MTP.** No
  MTP call enumerates it. How an entry in that library relates to a file in
  `/Music` is **not established** — see `docs/garmin-mtp.md` §8 for the two
  models that both fit what has been measured.
- **It does, however, answer per-object property queries.** Verified
  2026-09-02 on FR165 / FW 2506 via `examples/probe_objprops.rs`: the watch
  declares operations `0x9801`–`0x9805` and answers `GetObjectPropValue` with
  Name, Artist, AlbumName, AlbumArtist, Duration and Track — for any handle in
  `/Music`, including files Pelican never uploaded. These are separate facts
  and the earlier flat claim was wrong; `docs/garmin-mtp.md` § Object
  properties has the probe output.
- **The journal remains the source of *provenance*.** What Pelican put there
  is a question the device cannot answer, which is why grouping keys off the
  journal — not because the tags are unreadable.
- **Subfolders inside `/Music` are unreliable**, so uploads are flattened.
- Transfers are slow and cannot be parallelised.

**Open product decision — a network source (Navidrome).** Reading a
self-hosted Subsonic/Navidrome library was raised as a future possibility. It
is genuinely appealing and it is also **the first thing that would break the
product's stated promise**: the README's first paragraph says no network, and
`SECURITY.md` and the Flatpak manifest are built on that. A LAN-only server the
user runs themselves is not "phoning home", but it is still network access, and
the claim would have to be rewritten honestly rather than quietly widened.
**Not in scope now.** If it is ever taken up, it needs an explicit, visible
opt-in and a rewritten promise — never a silent capability.

**Open product decision — playlists.** The owner wants playlists and
organisation to be first-class. Today the FR165 **silently rejects MTP
playlist writes** across every path and format code tried (`docs/playlists.md`),
so Pelican's playlists are local-only groupings that queue their tracks for
upload — the watch itself sees loose files. `better-sync` reports real playlist
writes succeeding on FR945/FR255/Venu, so this may be a model-specific
firmware regression rather than a universal limit. **Unresolved:** whether to
attempt real playlist writes and degrade gracefully, or present local grouping
honestly. Design must not imply the watch has playlists when it does not.

**Deferred:** the transmission log is cut from the default interface. It stays
diagnostically valuable when firmware misbehaves, so it needs somewhere to
live that is not the main view.

## Brand Commitments

- Name: **Pelican**, a **Krypteia** project.
- Licence MIT OR Apache-2.0; reproducible builds; checked-in lockfile.
- **Zero telemetry, no network access, no background daemon.** This is the
  product's core promise and is stated in the first paragraph of the README.
- Existing "UNSC tactical" dark theme is **explicitly rejected** by the owner
  and is anti-reference, not a constraint.

## Evidence on Hand

- Real hardware: Forerunner 165 Music, firmware 2506, verified end-to-end.
- Extensive protocol research in `docs/` — `garmin-mtp.md`, `playlists.md`,
  `status.md`, `vendor-ops.md`, `research-log.md`.
- No user research, no testimonials, no analytics, no install numbers. **None
  exist — future work must not invent them.**
- Only one watch model has been tested. Behaviour on other Garmin music
  watches is inferred, not verified, and must not be claimed as tested.

## Product Principles

0. **Hear it before you commit it.** 3.7 GB is a real budget. Auditioning a
   track, reordering a playlist, and pushing a set are one continuous motion,
   not three tools.
1. **Simple to the point of magic.** The user's stated bar. Plug in, drop
   music, done. Every protocol quirk the firmware imposes is Pelican's problem
   to absorb, never the user's to learn.
2. **Never lie about the device.** If the watch cannot do something —
   playlists, subfolders, showing its library — say so plainly rather than
   simulating it. Trust is the product.
3. **Nothing leaves the machine.** No network, no accounts, no telemetry, in
   the code and visibly in the interface.
4. **Curation, not just transfer.** 3.7 GB means choosing. Taking music off
   deserves as much care as putting it on.
5. **Failure is expected and recoverable.** Firmware rejects things, cables get
   pulled, stubs get orphaned. The interface should make every failure legible
   and fixable without a terminal.

## Accessibility & Inclusion

No product-specific standard has been established yet. Two facts constrain it:
the current egui interface has **no accessibility support at all** (`eframe`'s
`accesskit` feature is disabled), so this is a net improvement in any case; and
a webview frontend inherits real accessibility primitives, so meeting
WCAG 2.1 AA is achievable rather than aspirational.
