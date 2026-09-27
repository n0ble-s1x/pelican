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
    fontSize: "clamp(1.625rem, 2.35vw, 2.125rem)"
    fontWeight: 400
    lineHeight: 1.25
    letterSpacing: "0.24em"
  headline:
    fontFamily: "Julius Sans One, Hanken Grotesk, sans-serif"
    fontSize: "1.625rem"
    fontWeight: 400
    lineHeight: 1.25
    letterSpacing: "0.24em"
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

Every step of the send is a title card from a modern Bond opening: black ink, one silhouette, one line of widely tracked capitals. The window is a split frame. On the left, a bounded field of ink holds a generic round watch as a solid silhouette, lit from behind. On the right, one title line, one or two quiet lines, and one action. Sending is the credits: track lines rise into place and resolve to a proven state, and a thread of champagne ink blooms in the field once for each verified file.

The register is classy, elegant and simple. That means the craft and restraint of title design, not a spy-gadget or terminal pastiche, and no franchise marks of any kind. Density stays low even in the list views. Rows are separated by hairlines, never boxed, and the ink field narrows to a column so the list has room. Colour is almost entirely ink and bone. The two accents are permissions, not decoration. Champagne means proven. Blood means failed, and the word always goes with it.

Motion belongs to the ink. Chrome settles in 150 to 250 ms and then stays still. The field renders once and does nothing at rest. It runs a bounded bloom only when a track resolves, and it bakes to a still residue under reduced motion.

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
- **Proof Champagne** (champagne): the colour of evidence. It appears on the verified mark and word, a verified credit's hash prefix, the room-left arc on the watch, ledger-sent entries on the watch listing, the "Fits" verdict, the filled Send button, and the lit bloom in the ink field. Nothing else is champagne.
- **Champagne Lift** (champagne-lift): the hover state of the Send button, and only that.

### Secondary
- **Blood** (blood): failure as ink. It appears as the notice border on errors and as the one red thread laid into the field beside a failed credit. It is too dark to use for text.
- **Blood, Legible** (blood-text): the failure colour for words and marks, including Failed, Refused, Stub, "Does not fit", an invalid field border, and the reason a credit failed. It keeps AA contrast on ink.

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

**Character:** The display face is a thin, monoline, wide capital that stays even at 0.24–0.42em tracking, which is what a title card needs. The grotesque is neutral and workmanlike, and its figures stay in columns.

### Hierarchy
- **Display** (400, clamp(1.625rem, 2.35vw, 2.125rem), 1.25, 0.24em, uppercase, balanced wrap): the one title line on a card view. "FORERUNNER 165 MUSIC" holds one line from 1280px up. The trailing tracking is cancelled with a matching negative right margin.
- **Headline** (same face, fixed 1.625rem): the title of a list view (Library, Review, On the watch, Ledger).
- **Wordmark** (0.8125rem, 0.42em, uppercase): "PELICAN" top-left. The same face at 0.32em carries the drop veil's "Release to add".
- **Lede** (400, 1.0625rem, 1.5, max 44ch): the one line under a title, such as room left or the send outcome.
- **Body** (400, 0.9375rem, 1.5): track names, rows, and quiet lines (max 60ch).
- **Body Small** (0.8125rem): facts, row subtitles, sizes, credit status lines, and the permanence statement.
- **Label, Action** (500, 0.8125rem, 0.16em, uppercase): button labels. Send uses weight 600.
- **Label, Nav** (0.75rem, 0.2em, uppercase): top-right links. The step index uses 0.24em, and form legends use 0.18em in dim bone.
- **Caption** (0.75rem): the no-network promise, estimates, and field labels.

### Named Rules
**The Title Card Rule.** The display face sets title lines, the wordmark and the drop veil. It is never used for an operable control, a row, a number, or a sentence.

**The Tabular Truth Rule.** Every figure is tabular and lining (set on body), so sizes, counts, track numbers and hash prefixes hold their columns.

## Layout

The window is one grid: a 64px top bar, a stage, and a 64px foot (52px when the viewport is under 700px tall). The ink field spans all three rows on the left, and its width follows the view:
- 52% on card views (Watch, Choose), so the silhouette sits at optical centre and the title column is vertically centred with a max width of 620px.
- 48% on Send and Done, which gives the credits a little more room.
- clamp(200px, 27%, 380px) on list views (Library, Review, On the watch, Ledger), where the field becomes a column and the stage holds a header, scrolling hairline rows, and a bottom bar.

Review splits into a track list and a 250–300px side column. Permanence and Send are sticky at the foot of that column and stay in view together.

The outer gutter is 40px (28px under 1100px). The stage's inner left edge is clamp(24px, 4vw, 64px). Vertical rhythm comes from a small set of steps: 10px rows, 14px gaps, 18px, 22px, 28px blocks, and 36px above an action row. Under 820px the frame stacks. The field becomes a 240px band behind the top bar with a 200px watch, the stage flows below it with 16px gutters, and the promise line is hidden.

## Elevation & Depth

The system is flat. Depth comes from the ink itself, a plume of lit, domain-warped noise behind the silhouette, and not from the interface. Surfaces do not cast shadows. The one raised tone is Raised Ink, used for inputs and the notice band. The field's right edge and the credit roll's top and bottom edges dissolve through alpha masks. Those masks exist so rows can leave the box, and they never fade a row that is in reading position.

### Shadow Vocabulary
- **Ink dissolve** (`box-shadow: 0 12px 14px -2px var(--ink)`): used only under a failed credit while it sticks at the top of the roll, so rows passing beneath dissolve into ink instead of showing a cut edge. It is ink over ink, not a lift.

### Named Rules
**The Still Chrome Rule.** Ink moves; chrome does not. UI transitions use the exponential ease-out (cubic-bezier(0.16, 1, 0.3, 1)) at 180 ms, with 240 ms view entrances, and then stop. The canvas animates only while a bloom is live, pauses when the window is hidden, and bakes to its residue under prefers-reduced-motion.

## Shapes

The shape language is a hairline and a right angle. Buttons and inputs have a 1px radius, which is optically square. Inline code takes 2px. Nothing is pill-shaped or card-shaped. Lists are hairline rows with a hairline above the first row, and no row carries a background. The only curves belong to the world: the round watch case, the room-left arc with round caps, and the ink. State marks are drawn on a 16px grid with a 1.4 round-capped stroke in currentColor: a check, a dash, a cross, a ring, a ring with a dot, and chevrons.

## Components

### Buttons
- **Shape:** optically square (1px radius), 44px minimum height, 22px side padding, label in tracked capitals.
- **Quiet (default):** transparent, with a 1px bone border at 50% and bone text. Every flow action except Send uses it, for example "Choose music" and "Stop after this track".
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
- A single band pinned above the stage: Raised Ink, strong-hairline border, a mark plus a bold capitalised word (Note, Problem) and then the sentence. On error the border turns blood.

### State Marks
- **Mark plus word:** each state is a stroked 14px mark followed by its word in weight 600 (Verified, Skipped, Failed, Converting, Sending). Colour follows the state: champagne for verified or ledger, dim bone for pending, skipped or foreign, bone for in flight, and blood-text for failed, refused or stub.

### The Credits Roll (signature)
- During Send, each track is a credit: its name at 1.0625rem in bone, with a status line beneath of a mark, a word, and detail (a hash prefix, or the failure reason).
- A new credit rises 22px over 520 ms, and its status resolves out of a 3px blur over 240 ms.
- While a file transfers, a 64px hairline meter fills in bone.
- A verified credit turns its status champagne.
- A failed credit sticks at the top of the roll at full contrast until a later failure replaces it, and its reason wraps instead of truncating.
- The roll fades only in its outer 12px and 40px, and it hides its scrollbar.

### The Watch Silhouette (signature)
- A generic round case with a fading strap and four buttons. It is one solid shape, darker than the ground, edged in bone at 5%. The only thing lit on it is the champagne room arc, which eases over 900 ms. It carries no counter and no brand. With no watch connected, the case becomes a dashed bone outline over empty ink, the arc is hidden, and the field dims to 40%.

## Do's and Don'ts

### Do:
- **Do** give every state a mark and a word. Colour is never the only signal.
- **Do** keep one title line per view in the display face, tracked 0.24em in uppercase, and one action (or one Send) beneath it.
- **Do** separate rows with hairlines (bone at 10%) and leave the rows themselves unfilled.
- **Do** set every size, count and hash in tabular lining figures.
- **Do** keep UI motion to 150–250 ms exponential ease-out. Let only the ink field move at length, and stop it when idle or under reduced motion.
- **Do** vendor every font and asset. The window makes no network request.

### Don't:
- **Don't** use champagne on anything unproven, such as a hover, a heading, a selection or an ornament.
- **Don't** show blood or blood-text without the failure word next to it, and don't use blood (#8e1b1b) for text.
- **Don't** set buttons, rows, numbers or body copy in the display face.
- **Don't** box content in cards, frosted panels or gradient-filled surfaces. Alpha masks that dissolve an edge are allowed; filled gradients are not.
- **Don't** add elevation shadows. The one ink-coloured dissolve under a sticky failed credit is the whole vocabulary.
- **Don't** put Bond, 007 or Omega marks, a gun barrel, or franchise type anywhere.
- **Don't** add a counter or brand detail to the watch silhouette. Its only lit element is the room arc.
