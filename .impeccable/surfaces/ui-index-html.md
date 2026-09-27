---
version: 1
slug: "ui-index-html"
primary_target: "ui/index.html"
related_targets: []
---

## Scope

The Pelican desktop window (Tauri 2, Linux-first) — the only surface. Replaces
the removed Fjord UI entirely. Views: the guided flow (Watch → Choose →
Review → Send), a Library way into Choose (browse a music root such as the
NAS), and two quiet secondary views: On the watch (read-only `/Music`
listing: ledger / foreign / stub) and Ledger.

## Visitor mode

Operate. The owner is completing a task: get chosen music onto the watch,
proven, in one short sitting.

## Audience and job

Primary: the owner — NAS music, whole albums and soundtracks, often untagged
WAV/FLAC. Secondary: a public first-run user. Job: plug in, choose, see what
will happen (tags, size, room left, permanence), send, watch every file verify.

## Constraints

Only proven capabilities (PRODUCT.md). No delete, no playlists, no playback,
no knobs. Mixes (a playlist sent as an album) ships as "Send as a mix" on
Review, over the core's `--mix` path (82fc311); not yet hardware-tested. No network: every font and asset vendored. No Bond or
Omega marks. Plain HTML/CSS/JS, no npm.

## Direction contract

THESIS: Every step of the send is a title card from a modern Bond opening —
black ink, one silhouette, one line of widely tracked capitals — and the send
run is the credits, tracks rising as each is proven. It refuses the
category's sidebar-plus-track-table file manager.

OWN-WORLD: Ink black ground (#050506) with a faintly raised ink (#0E0E10);
bone ink text (#EDE8DE, dimmed #A7A29A); hairlines at bone 10%. Champagne
(#C9A45C) is law: it lights only what is proven — a verified track, the send
action, the room left — never decoration. Blood red (#8E1B1B, legible tint
for text) appears only on failure, always with the word. One self-hosted thin
wide-capital display face for title lines only; a vendored workhorse sans for
everything operable; tabular figures for sizes and hashes. Every state is a
mark plus a word. No cards, no glass, no gradients as surfaces; ink moves, UI
chrome does not.

STORY: The owner sees at once which watch is connected and how much room it
has; chooses albums by drop or from the library; reads a calm review that
says exactly what will go, how it will be tagged, whether it fits, and that
what goes on stays on; presses Send; watches the credits roll up as each
track is proven by hash; unplugs and runs.

FIRST VIEWPORT: 1280×800. Left ~55%: the ink field, a round-watch silhouette
(authored SVG, generic, no brand) sitting in slowly settled ink at optical
centre, ~300px. Right column, vertically centred: the title line
"FORERUNNER 165 MUSIC" in the display face (~34px, tracking ~0.28em), beneath
it one quiet line of room — "2.3 GB free · room for about 470 tracks" — and
one action, "Choose music". Top-right, small tracked links: On the watch ·
Ledger. Bottom edge: the four-step index as words (Watch · Choose · Review ·
Send), the current one in bone, the rest dim. No watch: the silhouette is
empty ink, title "CONNECT YOUR WATCH", and the fix (cable, udev rule, gvfs)
in one plain line.

FORM: Title Sequence — my list position 1 (Impeccable's pick; the roll
assigned position 5, Shanghai Glass, as `direction-payload.json` records).
The roll's seed key was not persisted with its output, so none is claimed
here. Signature
interaction: the credits roll — during Send, each track line rises into
place and resolves to "Verified" in champagne with its hash prefix, and a
thread of champagne ink blooms in the field once per verified track; at rest
the ink is still. Motion grammar: 150–250 ms exponential ease-out for UI,
ink is a bounded canvas effect that never runs when idle and respects
prefers-reduced-motion.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## Decisions after the build

- The dial carries no counter. The verified count lives in the credits'
  meta line; the watch stays a silhouette with only the champagne room arc.
- Failed credits keep full contrast for as long as they are in the roll; the
  roll fades only at its very edges, never over a row's reading position.
- A failure sends one blood-red thread through the ink, beside the credit
  that names it — the only place red enters the field.

- The title is 34px at 0.28em, as the contract says, and holds one line
  at 1280 by narrowing the card field there rather than the type: the card
  field is clamp(40%, 100% − 702px, 52%). Measured in headless Brave at
  1280×800: "FORERUNNER 165 MUSIC" is 609px at 34px / 0.28em (516px at the
  interim 30px / 0.24em); the field is 578px (45%), the case 286px, and the
  title box 620px on one line. From about 1460px up the field is 52% again.
  Rendered side by side with the 30px / 0.24em build, the contract size
  reads as the title card; the smaller case is the lesser loss.
- The watch is one opaque silhouette. The strap no longer fades: it runs
  off the field's top and bottom edges, and the SVG is painted over the ink
  canvas, so no ink (the red failure thread included) crosses case or strap.
- No credit is ever half shown. The pinned failure stands on an opaque ink
  ground (the dissolve shadow is gone), and a credit that begins to pass
  under it, or out of the roll's top edge, is hidden whole until it clears.
- The no-watch states offer "Install the USB rule" (a pkexec path in the
  shell, one fixed command, rule compiled in) when the rule is missing or
  outdated. Demo state: `#permission`.

## Unresolved

- Mixes and stop-between-files are built (82fc311) and shipped in the window;
  neither is hardware-tested yet.
