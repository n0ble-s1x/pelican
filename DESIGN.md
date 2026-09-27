---
name: Pelican
description: Music onto a Garmin watch from Linux, every file proven. The send staged as a title sequence.
colors:
  champagne: "#c9a45c"
  champagne-lift: "#d6b373"
  blood: "#8e1b1b"
  blood-text: "#d46a5f"
  ink: "#050506"
  ink-raised: "#0e0e10"
  ink-high: "#16161a"
  silhouette: "#010102"
  bone: "#ede8de"
  bone-dim: "#a7a29a"
  bone-faint: "#8a857d"
  hair: "rgb(237 232 222 / 0.1)"
  hair-strong: "rgb(237 232 222 / 0.2)"
typography:
  display:
    fontFamily: "Julius Sans One, Hanken Grotesk, sans-serif"
    fontSize: "clamp(1.625rem, 2.66vw, 2.125rem)"
    fontWeight: 400
    lineHeight: 1.25
    letterSpacing: "0.28em"
  headline:
    fontFamily: "Julius Sans One, Hanken Grotesk, sans-serif"
    fontSize: "1.625rem"
    fontWeight: 400
    lineHeight: 1.25
    letterSpacing: "0.28em"
  wordmark:
    fontFamily: "Julius Sans One, Hanken Grotesk, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 400
    letterSpacing: "0.42em"
  lede:
    fontFamily: "Hanken Grotesk, system-ui, sans-serif"
    fontSize: "1.0625rem"
    fontWeight: 400
    lineHeight: 1.5
    fontFeature: "\"tnum\", \"lnum\""
  body:
    fontFamily: "Hanken Grotesk, system-ui, sans-serif"
    fontSize: "0.9375rem"
    fontWeight: 400
    lineHeight: 1.5
    fontFeature: "\"tnum\", \"lnum\""
  body-sm:
    fontFamily: "Hanken Grotesk, system-ui, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 400
    lineHeight: 1.5
    fontFeature: "\"tnum\", \"lnum\""
  label-action:
    fontFamily: "Hanken Grotesk, system-ui, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 500
    letterSpacing: "0.16em"
  label-nav:
    fontFamily: "Hanken Grotesk, system-ui, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 400
    letterSpacing: "0.2em"
  caption:
    fontFamily: "Hanken Grotesk, system-ui, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 400
    lineHeight: 1.5
rounded:
  hairline: "1px"
  code: "2px"
spacing:
  row: "10px"
  gap: "14px"
  step: "18px"
  action-gap: "22px"
  block: "28px"
  actions: "36px"
  gutter: "40px"
  gutter-narrow: "28px"
components:
  button-quiet:
    backgroundColor: "transparent"
    textColor: "{colors.bone}"
    typography: "{typography.label-action}"
    rounded: "{rounded.hairline}"
    padding: "0 22px"
    height: "44px"
  button-quiet-hover:
    backgroundColor: "rgb(237 232 222 / 0.07)"
    textColor: "{colors.bone}"
  button-send:
    backgroundColor: "{colors.champagne}"
    textColor: "{colors.ink}"
    typography: "{typography.label-action}"
    rounded: "{rounded.hairline}"
    padding: "0 22px"
    height: "44px"
  button-send-hover:
    backgroundColor: "{colors.champagne-lift}"
    textColor: "{colors.ink}"
  button-disabled:
    backgroundColor: "transparent"
    textColor: "{colors.bone-faint}"
  link-tracked:
    backgroundColor: "transparent"
    textColor: "{colors.bone-dim}"
    typography: "{typography.label-nav}"
    padding: "6px 2px"
  link-tracked-current:
    textColor: "{colors.bone}"
  text-button:
    backgroundColor: "transparent"
    textColor: "{colors.bone-dim}"
    typography: "{typography.body-sm}"
    padding: "6px 0"
  input-field:
    backgroundColor: "{colors.ink-raised}"
    textColor: "{colors.bone}"
    rounded: "{rounded.hairline}"
    padding: "0 12px"
    height: "38px"
  notice:
    backgroundColor: "{colors.ink-raised}"
    textColor: "{colors.bone}"
    typography: "{typography.body-sm}"
    padding: "10px 14px"
  credit-verified:
    backgroundColor: "transparent"
    textColor: "{colors.champagne}"
    typography: "{typography.body-sm}"
    padding: "9px 0"
  credit-failed:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.blood-text}"
    typography: "{typography.body-sm}"
    padding: "9px 0"
---

# Design System: Pelican

## Overview

**Creative North Star: "The Title Sequence"**

Every step of the send is a title card from a modern Bond opening: black ink, one silhouette, one line of widely tracked capitals. The window is a split frame. On the left, a bounded field of ink holds a dive watch in silhouette (scalloped bezel, lyre lugs, a three-link bracelet), lit from behind. On the right, one title line, one or two quiet lines, and one action. Sending is the credits: track lines rise into place and resolve to a proven state, and a thread of champagne ink blooms in the field once for each verified file.

The register is classy, elegant and simple. That means the craft and restraint of title design, not a spy-gadget or terminal pastiche, and no franchise marks of any kind. Density stays low even in the list views. Rows are separated by hairlines, never boxed, and the ink field narrows to a column so the list has room. Color is almost entirely ink and bone. The two accents are permissions, not decoration. Champagne means proven. Blood means failed, and the word always goes with it.

Motion belongs to the ink. Chrome settles in 150 to 250 ms and then stays still. One loop is allowed at rest: while a watch is connected and the window is visible, the lit plume behind the case breathes (a 14 s transform-only swell; the canvas is not redrawn for it), so a live watch reads as alive and a missing one as still. Beyond that the field runs a bounded bloom only when a track resolves, and it bakes to a still residue under reduced motion.

**Key Characteristics:**
- Ink-black ground with bone text; hairlines at bone 10% and 20%.
- Champagne lights only proven things: verified tracks, the room-left arc, the Send action.
- One thin, wide-capital display face for title lines only; a tabular-figure grotesque for everything operable.
- Every state is a stroked mark plus a word.
- No cards, no glass, no gradient surfaces, no elevation.
- Ink moves; chrome does not.

## Colors

Near-black ink and warm bone, with two accents: champagne (proven) and blood (failure).

### Primary
- **Proof Champagne** (champagne): the color of evidence. It appears on the verified mark and word, a verified credit's hash prefix, the room-left arc on the watch, ledger-sent entries on the watch listing ("Sent by Pelican"), the "Fits" verdict, the filled Send button, "Verified clean" after a reset is read back, and the lit bloom in the ink field. Nothing else is champagne.
- **Champagne Lift** (champagne-lift): the hover state of the Send button, and only that.

### Secondary
- **Blood** (blood): failure as ink. It appears as the notice border on errors and as the one red thread laid into the field beside a failed credit. It is too dark to use for text.
- **Blood, Legible** (blood-text): the failure color for words and marks, including Failed, Refused, Stub, "Does not fit", an invalid field border, and the reason a credit failed. It keeps AA contrast on ink.

### Neutral
- **Ink** (ink): the ground of the window, the field, and the sticky commit block. The canvas base is painted in the same value.
- **Raised Ink** (ink-raised): inputs and the notice band. It is the only raised tone.
- **High Ink** (ink-high): the tint behind inline code (paths, udev rules).
- **Silhouette** (silhouette): the watch case, strap and buttons. It is darker than the ground so the lit ink cuts the shape out.
- **Bone** (bone): primary text, titles, the current step, focus outlines, and the quiet button border at 50%.
- **Dim Bone** (bone-dim): secondary lines, row subtitles, nav links at rest, and pending, skipped and foreign states.
- **Faint Bone** (bone-faint): tertiary text such as step separators, inactive steps, sizes, timestamps, placeholders and disabled labels. It is the dimmest text the system allows.
- **Hairline** (hair) and **Strong Hairline** (hair-strong): row dividers and list tops, and borders for inputs, the notice, disabled buttons and the demo flag.

### Named Rules
**The Proven Gold Rule.** Champagne marks only what has been proven or what commits to proof: a verified file, the room left, the Send action. If an element has not been verified, it is not gold.

**The Named Failure Rule.** Red never appears alone. Every use of blood or blood-text sits beside the word (Failed, Refused, Stub, Does not fit), and blood reaches the ink field only as the single thread for a failed credit.

## Typography

**Display Font:** Julius Sans One (with Hanken Grotesk, sans-serif), self-hosted, shipped unmodified under its OFL.
**Body Font:** Hanken Grotesk variable (with system-ui, sans-serif), self-hosted and subset to Latin.

**Character:** The display face is a thin, monoline, wide capital that stays even at 0.24-0.42em tracking, which is what a title card needs. The grotesque is neutral and workmanlike, and its figures stay in columns.

### Hierarchy
- **Display** (400, clamp(1.625rem, 2.66vw, 2.125rem), 1.25, 0.28em, uppercase, balanced wrap): the one title line on a card view. At 1280px it is 34px, and "FORERUNNER 165 MUSIC" holds one line (609px) because the card field gives way to it (see Layout). The trailing tracking is canceled with a matching negative right margin.
- **Headline** (same face and tracking, fixed 1.625rem): the title of a list view (Library, Review, On the watch, Ledger).
- **Wordmark** (0.8125rem, 0.42em, uppercase): "PELICAN" top-left. The same face at 0.32em carries the drop veil's "Release to add".
- **Lede** (400, 1.0625rem, 1.5, max 44ch): the one line under a title, such as room left or the send outcome.
- **Body** (400, 0.9375rem, 1.5): track names, rows, and quiet lines (max 60ch).
- **Body Small** (0.8125rem): facts, row subtitles, sizes, credit status lines, and the permanence statement.
- **Label, Action** (500, 0.8125rem, 0.16em, uppercase): button labels. Send uses weight 600.
- **Label, Nav** (0.75rem, 0.2em, uppercase): top-right links. The step index uses 0.24em.
- **Caption** (0.75rem): estimates and field labels.

### Named Rules
**The Title Card Rule.** The display face sets title lines, the wordmark and the drop veil. It is never used for an operable control, a row, a number, or a sentence.

**The Tabular Truth Rule.** Every figure is tabular and lining (set on body), so sizes, counts, track numbers and hash prefixes hold their columns.

## Layout

The window is one grid: a 64px top bar, a stage, and a 64px foot (52px when the viewport is under 700px tall). The ink field spans all three rows on the left, and its width follows the view:
- clamp(40%, 100% − 716px, 52%) on card views (Watch, Name the playlist, Start over): 52% on a wide window, giving way only as far as the title column needs to hold "FORERUNNER 165 MUSIC" (609px) on one line at 34px / 0.28em. The 716px reserve covers the title, the 40px right gutter and the stage's left inset at its largest (64px), so the line holds through the 1280-1460 band where that inset still grows with 4vw. Measured one line at 1280, 1360, 1440, 1460 and 1600, including every frame of the title entrance. The silhouette sits at optical center, and the title column is vertically centered with a max width of 620px; it scrolls rather than clips when "What it runs" is open.
- 48% on Send and Done, which gives the credits a little more room.
- clamp(200px, 27%, 380px) on list views (Library, Review, On the watch, Ledger), where the field becomes a column and the stage holds a header, scrolling hairline rows, and a bottom bar.

Review splits into a track list and a 250-300px side column. Permanence and Send are sticky at the foot of that column and stay in view together.

The outer gutter is 40px (28px under 1100px). The stage's inner left edge is clamp(24px, 4vw, 64px). Vertical rhythm comes from a small set of steps: 10px rows, 14px gaps, 18px, 22px, 28px blocks, and 36px above an action row. Under 820px the frame stacks. The field becomes a 240px band behind the top bar with a 200px watch, the stage flows below it with 16px gutters.

The foot holds only the step index (Watch · Choose · Review · Send; in Start over, Erases · Back up · Reset · Confirm). A hairline travels under the current step (360 ms). Nothing else sits in the foot.

## Elevation & Depth

The system is flat. Depth comes from the ink itself, a plume of lit, domain-warped noise behind the silhouette, and not from the interface. Surfaces do not cast shadows. The one raised tone is Raised Ink, used for inputs and the notice band. The field's right edge and the credit roll's top and bottom edges dissolve through alpha masks. Those masks exist so rows can leave the box, and they never fade a row that is in reading position. No row is ever shown cut: a credit that starts to pass under the pinned failure, or out of the roll's top edge, is hidden whole until it clears.

### Shadow Vocabulary
- None. The pinned failed credit stands on an opaque Ink ground, and rows beneath it are hidden whole rather than dissolved, so no shadow is needed.

### Named Rules
**The Still Chrome Rule.** Ink moves; chrome does not. UI transitions use the exponential ease-out (cubic-bezier(0.16, 1, 0.3, 1)) at 180 ms, with entrances no longer than 520 ms, and then stop. The canvas animates only while a bloom is live, pauses when the window is hidden, and bakes to its residue under prefers-reduced-motion. The breathing plume is the one loop at rest (see Overview), and it is paused with no watch, with the window hidden, under reduced motion and under `?still`.

**The Entrance Grammar.** Entrances use the Web Animations API, so a re-render never replays them, and every call is guarded by reduced motion.
- A title card's title line opens from its center out of a 6px blur (clip-path inset from 50% to fully open, with opacity, 520 ms). It never animates tracking, size or any layout property, so the line is laid out once at its final width and cannot re-wrap mid-motion.
- The lines beneath it settle 8px up, staggered 34 ms from 90 ms, capped at the sixth.
- List views fade their header in (240 ms) and bring rows in as a list (220 ms, 16 ms stagger, capped at 14). A folder's contents unfold under it the same way.
- A reordered playlist row slides into its new place.
- The bezel clicks 6° counterclockwise per proven track and rests where the send left it; the room arc sweeps in on first read and gives up each proven track's room as it lands.

## Shapes

The shape language is a hairline and a right angle. Buttons and inputs have a 1px radius, which is optically square. Inline code takes 2px. Nothing is pill-shaped or card-shaped. Lists are hairline rows with a hairline above the first row, and no row carries a background. The only curves belong to the world: the watch, the room-left arc with round caps, and the ink. State marks are drawn on a 16px grid with a 1.4 round-capped stroke in currentColor: a check, a dash, a cross, a ring, a ring with a dot, and chevrons. The explorer's places use the same set: a house (Home), a note (Music), spines (Library), stacked servers (network shares), a drive body with its activity dots (drives, removable or not; nothing in it reads as a lock), a monitor on its stand (Computer, the whole file system), and three lines (Chosen).

## Components

### Buttons
- **Shape:** optically square (1px radius), 44px minimum height, 22px side padding, label in tracked capitals.
- **Quiet (default):** transparent, with a 1px bone border at 50% and bone text. Every flow action except Send uses it, for example "Choose music" and "Stop after this song".
- **Hover / Active:** a bone wash at 7% (12% when pressed) and a full bone border, over 180 ms.
- **Send:** a champagne fill with ink text at weight 600, full width in the review side column. It lifts to Champagne Lift on hover. There is one Send per screen.
- **Disabled:** transparent with a strong-hairline border and faint-bone text. A disabled Send loses its champagne, and a dim caption beneath it says why.
- **Text button:** dim bone, 0.8125rem, underlined with a hairline offset 5px. It brightens to bone on hover.

### Navigation
- **Top-right links** (On the watch · Ledger) and the **step index** (Watch · Choose · Review · Send) are tracked capital words separated by faint middle dots. At rest they are dim or faint. The current item turns bone; the current page link also gets a hairline underline offset 6px. There are no tabs, pills or icons.

### Inputs / Fields
- **Style:** Raised Ink fill, 1px strong-hairline border, 1px radius, 38px tall (34px in the Review overrides grid).
- **Focus:** the border turns bone, with no glow. Everything else uses a 1px bone outline offset 3px.
- **Error:** the border turns blood-text, with `aria-invalid` set.
- **Checkboxes:** native, 16px, with a bone accent.

### Notice
- A single band pinned above the stage: Raised Ink, strong-hairline border, a mark plus a bold capitalized word (Note, Problem) and then the sentence. On error the border turns blood.

### The USB Rule
- Shown in the no-watch states when the udev rule is missing or outdated (a permission problem, or no watch found). It is then the view's one quiet action, "Install the USB rule", and Check again drops to a text button. It is not champagne: installing a file proves nothing.
- One Body Small line in dim bone beneath it names the cost: the password, once, and the one file it writes. A "What it runs" disclosure (the Every-track chevron pattern) shows the rule text, the exact polkit command and a terminal fallback in High Ink blocks that wrap and never scroll sideways.
- While the password prompt is open the button is disabled, busy, and reads "Waiting for your password" after the in-flight mark. A cancel or failure lands as one line with a mark under the button; a failure's mark is blood-text beside the words "Not installed". After an install that still leaves the watch unreachable, the title becomes "Unplug and replug the watch".

### State Marks
- **Mark plus word:** each state is a stroked 14px mark followed by its word in weight 600 (Verified, Skipped, Failed, Converting, Sending). Color follows the state: champagne for verified or ledger, dim bone for pending, skipped or foreign, bone for in flight, and blood-text for failed, refused or stub.

### The Credits Roll (signature)
- During Send, each track is a credit: its name at 1.0625rem in bone, with a status line beneath of a mark, a word, and detail (a hash prefix, or the failure reason).
- A new credit rises 22px over 520 ms, and its status resolves out of a 3px blur over 240 ms.
- While a file transfers, a 64px hairline meter fills in bone.
- A verified credit turns its status champagne.
- A failed credit sticks at the top of the roll at full contrast, on an opaque Ink ground, until a later failure replaces it, and its reason wraps instead of truncating. A credit passing beneath it (or an earlier failure under a later one) is hidden whole, never half shown.
- The roll fades only in its outer 12px and 40px, and it hides its scrollbar.

### The Watch Silhouette (signature)
A dive watch in the Seamaster manner (owner, 2026-09-26: "the silhouette of an Omega, class it up"), drawn as an authored SVG silhouette only: no name, logo, dial text or index. Its parts, in the 320 × 360 viewBox with the case centered at (160, 240):
- **Case and bezel.** A 118 case under a 124 unidirectional bezel with 30 shallow scallops. A pip at 12 on the bezel (bone 16%) is the one interior mark, so the bezel's turn reads. A dial ring at r 105 (bone 7%) marks where the bezel insert meets the dial, so the head reads round even with nothing lit.
- **Lyre lugs.** One slim blade, mirrored four ways: it leaves the case at the flank and sweeps up to a narrow (15-unit) tip beside the end link, with air between it and the bracelet. One facet hairline (bone 10%) runs from the inner side at the tip to the outer side at the shoulder, which is what makes it read twisted.
- **Crowns.** A guarded crown at 3 and a conical helium-valve crown at 10.
- **Bracelet.** Three-link, tapering from 112 at the case to 100, running off the field's top and bottom edges. The links are hairlines on the silhouette, not cuts (bone 8%): a narrow polished center link (bone 3% wash) and outer links set half a pitch off it, so it reads as a bracelet, not a grid.
- **One opaque shape.** Case, lugs, crowns and bracelet are drawn twice, stroked then filled over, so only the outer edge carries bone (7%); the bezel's scalloped edge is drawn once more over the end links, so the head stays round at 12 and 6. The whole silhouette is painted over the ink canvas, so no ink (not even the red thread) crosses it.
- **Lit.** Only the champagne room arc, riding the bezel at r 115, which eases over 900 ms. The bezel turns 6° per proven track. No counter, no brand.
- **No watch.** The fill becomes ink and the edge a continuous bone hairline at 22% (never dotted), with the dial ring at 18% and the lug facets at 14%: an empty but unmistakably round dive watch. Links, pip and arc are hidden, the field dims to 40%, and the plume stops breathing.

### No Watch and Recovery
Every no-watch state says what to do in the owner's own moves, without docs or a terminal:
- **Connect your watch:** "Plug the watch into this computer with the cable that came with it", then three numbered moves (the Garmin cable, not a charge-only cable; straight into the computer, not a hub; wake the watch, then check again).
- **Release the watch** (the file manager holds it): close the file manager window showing the watch; press the eject mark next to it in the side bar; check again.
- **The watch is busy:** close Garmin Express, the file manager or any music app that opened it; replug if that doesn't free it.
- **Permission needed:** replug; restart once with the watch unplugged. When the USB rule is missing, the rule's one quiet action takes the place of the moves.
- **The watch isn't answering** (wedged): the replug instruction is the lede, the three moves run in one line (Unplug · Wait five seconds · Plug it back in), and a send that meets it ends on "The watch stopped answering" with what already arrived.
The moves use the reset steps' numbered hairline list. File names, commands and the shell's own sentence never appear in the lede: they sit folded under "What Pelican saw", beneath the actions.

### The Explorer (Choose)
Nobody types a path. A places column (Home, Music, the library, network shares, drives, Computer = `/`, and Chosen with its count) sits beside a tree that opens in place and reads each folder only when it is first opened. Every folder and song has a tick; folders are tri-state. Ticking a folder absorbs anything ticked inside it; unticking a song inside a ticked folder splits the folder into its other contents. Chosen lists the send in tick order, each with Leave out. The bar under the tree carries the count, Clear, Make a playlist and Review.

### The Playlist Card
Make a playlist opens a title card, "Name the playlist": one line saying the songs go in tick order and appear under Albums on the watch, one field, and Review. Enter reviews it. One fact says a song already on the watch goes again as a new copy inside the playlist. Review keeps a "Send as a playlist" toggle with the same honest line, and the rows reorder with the up and down marks.

### Send Again
A row skipped as already on the watch offers Send again. Once chosen, it reads "Send again · another copy, under a new name" with Keep skipped beside it. A run-wide toggle, "Send every skipped song again", sits in Review's side column.

### The Permanence Statement
One statement in two lengths, used on On the watch, Review (beside Send) and the first Start over card: nothing can be deleted from the watch one song at a time; the only way to clear it is a factory reset, and that erases everything on the watch: activities, health data, settings, Garmin Pay and music. It is set in Body Small bone, never red: it is a fact, not a failure.

### Start Over (factory-reset walkthrough)
Four title cards with their own step index in the foot (Erases · Back up · Reset · Confirm). Pelican never resets the watch.
1. **What a reset erases:** the permanence statement and the five categories as hairline rows.
2. **Back up first:** sync with Garmin Connect; "If you use Agoge or another fitness app, pull your activities into it first"; a read-only copy of the watch's GARMIN folder with a live count, file and meter; the settings backup on the watch. While the copy runs, Continue is disabled with the reason beneath it ("Continue opens when the backup finishes. The next step erases the watch.") and the only other move is Stop the backup; no control, the step index included, goes past this card mid-copy. A stopped or partial copy names itself on the way on: "Continue without a full backup".
3. **Reset the watch:** the six steps on the watch, with the Forerunner 165's buttons named.
4. **Plug it back in:** "The watch is clean" reads the watch's music folder; the ledger restarts only if it is empty. Refused is its own card (the word Refused in blood-text, the likely cause). A clean result is "Verified clean" in champagne, and the ink blooms once.

## Do's and Don'ts

### Do:
- **Do** give every state a mark and a word. Color is never the only signal.
- **Do** keep one title line per view in the display face, tracked 0.28em in uppercase, and one action (or one Send) beneath it.
- **Do** separate rows with hairlines (bone at 10%) and leave the rows themselves unfilled.
- **Do** set every size, count and hash in tabular lining figures.
- **Do** keep UI transitions to 150-250 ms exponential ease-out and entrances within 520 ms. Let only the ink field move at length, and stop it when idle or under reduced motion.
- **Do** vendor every font and asset. The window makes no network request.

### Don't:
- **Don't** use champagne on anything unproven, such as a hover, a heading, a selection or an ornament.
- **Don't** show blood or blood-text without the failure word next to it, and don't use blood (#8e1b1b) for text.
- **Don't** set buttons, rows, numbers or body copy in the display face.
- **Don't** box content in cards, frosted panels or gradient-filled surfaces. Alpha masks that dissolve an edge are allowed; filled gradients are not.
- **Don't** add shadows of any kind. There is no shadow vocabulary.
- **Don't** put Bond, 007 or Omega marks, a gun barrel, or franchise type anywhere.
- **Don't** add a counter or brand detail to the watch silhouette. Its only lit element is the room arc.
- **Don't** animate letter-spacing, size or any layout property in an entrance. Reveal with clip-path, opacity, blur and transform.
- **Don't** put a file name, command or raw error in a lede. Say the move; fold the evidence.
- **Don't** offer a way past the backup card while a backup is copying.
