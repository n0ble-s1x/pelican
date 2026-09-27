---
version: 1
slug: "ui-index-html"
primary_target: "ui/index.html"
related_targets: []
---

## Scope

The Pelican desktop window (Tauri 2, Linux-first) — the only surface. Replaces
the removed Fjord UI entirely. Views: the guided flow (Watch → Choose →
Review → Send), where Choose is the Library explorer (places + folder tree,
ticks on folders and songs) and a Playlist is named on its own title card;
two quiet secondary views: On the watch (read-only `/Music` listing: ledger /
foreign / stub) and Ledger; and Start over, a four-card factory-reset
walkthrough reached from On the watch.

## Visitor mode

Operate. The owner is completing a task: get chosen music onto the watch,
proven, in one short sitting.

## Audience and job

Primary: the owner — NAS music, whole albums and soundtracks, often untagged
WAV/FLAC. Secondary: a public first-run user. Job: plug in, choose, see what
will happen (tags, size, room left, permanence), send, watch every file verify.

## Constraints

Only proven capabilities (PRODUCT.md). No delete, no MTP playlists, no
playback, no knobs. A Playlist is sent as an album over the core's `mix`
path; not yet hardware-tested. Pelican never resets the watch; it copies the
watch's files read-only and resets only its own ledger after reading `/Music`
empty. No network: every font and asset vendored. No Bond or Omega marks: the
dive-watch silhouette is a silhouette only. Plain HTML/CSS/JS, no npm.

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
Seed key a89ed0a8. The owner chose this card explicitly on the
decision page on 2026-09-26 (serve-question key 60909434, ANSWER
`{"optionId":"model-pick","buildPath":"code"}`), over the rolled Shanghai
Glass; the pick was logged back to the seed with `--kind pick`. Signature
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

- Owner round, 2026-09-26 (wedge, footer, silhouette, explorer, playlists,
  send again, permanence + reset, motion):
  - **Wedged watch.** `status.error_kind` drives the no-watch card; "wedged"
    is its own title card, "The watch isn't answering", with the replug
    instruction as the lede and the three moves as a quiet sequence. A send
    that meets it ends on "The watch stopped answering" with the same words
    and says what already arrived. Demo: `#wedged`, `#send-wedged`.
  - **Footer copy** ("No account · no network · nothing leaves this
    computer") is gone from every view; the foot holds only the step index.
  - **Silhouette.** A dive watch in the Seamaster manner, authored SVG:
    30-scallop bezel, twisted lyre lugs (one path, mirrored four ways),
    guarded crown at 3, conical helium-valve crown at 10, a three-link
    bracelet running off both edges. The bracelet's links are bone hairlines
    at 8% on the silhouette, not cuts, so no ink (the red thread included)
    shows through the watch. The silhouette is drawn twice (stroked, then
    filled over) so only its outer edge carries bone. A small pip at 12 on
    the bezel (bone 16%) is the one interior mark; it exists so the bezel's
    turn reads. No name, logo or dial text. The room arc rides the bezel
    ring (r 115 of the 124 bezel).
  - **Explorer.** Places column (Home, Music, Library, network shares,
    drives, plus "Computer" = `/` added by the window so any folder is
    reachable without a picker), a lazily loaded tree (a folder is read by
    `library_list` when first opened), ticks on folders and songs, tri-state
    folders, and a "Chosen" place that lists the send in tick order with
    Leave out. Ticking a folder absorbs anything ticked inside it; unticking
    a song inside a ticked folder splits the folder into its other contents.
    No path field anywhere; there is no "Other folder…" because the webview
    has no native picker in the IPC, and Computer covers it.
  - **Playlist.** Explorer bar: Make a playlist → a title card, "Name the
    playlist", one field, Enter → Review the playlist (reorder with the
    up/down marks; the moved row slides into place). Review keeps a "Send as
    a playlist" toggle with the one honest line.
  - **Send again.** A row skipped as "already on watch" offers Send again;
    once chosen it reads "Send again · another copy, under a new name" with
    "Keep skipped". The run-wide toggle stays ("Send every skipped song
    again").
  - **Permanence.** One statement, two lengths (`NO_DELETE`,
    `NO_DELETE_SHORT` in app.js), used on On the watch, Review and Start
    over step 1: nothing can be deleted one song at a time; only a factory
    reset clears the watch, and it erases everything, not just music.
  - **Start over.** Four title cards with their own step index in the foot
    (Erases · Back up · Reset · Confirm): what a reset erases; back up first
    (sync, the Agoge line, one-click read-only GARMIN backup with live
    progress, optional settings backup on the watch); the six reset steps on
    the watch (FR165 button positions named); plug back in → "The watch is
    clean" calls `reset_check` then `reset_ledger` and reports exactly what
    it found. Refused is its own card with the word Refused in blood-text
    and the likely cause (Reset Default Settings keeps music). A clean
    result is "Verified clean" in champagne (it is proven by read-back) and
    blooms the ink once.
  - **Motion — the at-rest decision.** One loop is allowed at rest: the ink
    breathes (a 14 s transform-only swell of the lit plume behind the case;
    the canvas is never redrawn for it). It runs only while a watch is
    connected and the window is visible, so a live watch reads as alive and
    a missing one as still. Reduced motion and `?still` hold it at rest.
    Nothing else loops at rest. The rest of the motion: title cards resolve
    from wide soft tracking (480 ms) with their lines settling after them
    (≤ 500 ms total); list views bring rows in as a list (220 ms, 16 ms
    stagger, capped at 14); a folder's contents unfold under it; a hairline
    travels under the current step (360 ms); the bezel clicks 6°
    anticlockwise per proven track and rests where the send left it; the
    room arc sweeps in on first read and gives up each proven track's room
    as it lands; credits and blooms as before. Controls stay 180 ms.
    Entrances use the Web Animations API so a re-render never replays them.
  - Demo states: `#wedged`, `#library`, `#chosen`, `#playlist`,
    `#review` (a skipped row with Send again, one set to send again),
    `#review-playlist`, `#send-wedged`, `#reset-1` … `#reset-4` (backup
    running in `#reset-2`), `#reset-refused`, `#reset-done`, plus the
    earlier ones.

- Finish review fixes, 2026-09-26 (disposition: fix):
  - **Silhouette.** Empty states draw one continuous bone hairline edge
    (22%, no dash), a dial ring at r 105 and the lug facets, and the
    bezel's scalloped edge is drawn again over the end links, so the head
    reads round lit or empty. Lugs are slim lyre blades with a twist facet
    and air beside the end link. The bracelet tapers and its links stagger
    (polished centre link, outer links half a pitch off).
  - **Plain-words recovery.** Connect / Release / Busy / Permission each
    say the move as a numbered list (the reset steps' list); the shell's
    sentence, with its file names and commands, is folded under "What
    Pelican saw". There is still no release IPC: the gvfs card tells the
    owner to close the file manager window and eject the watch from its
    side bar.
  - **Backup gate.** While the GARMIN copy runs, Continue is disabled with
    its reason and the one other move is Stop the backup (the shell's
    `stop` covers backups); `resetStep` refuses to pass card 2 mid-copy
    from any control. A stopped or partial copy reads "Continue without a
    full backup".
  - **Title entrance.** clip-path from the centre + blur + opacity, 520 ms;
    no letter-spacing. Height measured constant (one line, 43px) through
    every frame at 1280/1360/1440/1460/1600.
  - **Title band.** Field is clamp(40%, 100% − 716px, 52%): the reserve
    covers the stage inset at its 64px maximum, so 1280–1460 holds one line.
  - **Place marks.** Drives: a drive body with activity dots. Computer: a
    monitor on its stand.
  - **Ceiling notes not taken:** the gun-barrel/aperture device is out
    (PRODUCT.md bans the gun barrel); an ink-wipe between title cards and a
    credits voice for the list views are open for a later round.

## Unresolved

- Playlists (as albums) and stop-between-files are built and shipped in the
  window; neither is hardware-tested yet.
- The window's backup, reset-check and wedge paths are exercised only
  against the in-file mock; the shell side of the new IPC is built
  separately.
