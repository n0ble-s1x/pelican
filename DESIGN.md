---
name: Pelican
description: A cold, quiet desktop music player whose ground is the capacity of your watch.
colors:
  air: "#0d1622"
  air-2: "#111c2a"
  ground: "#070c13"
  d1: "#16283a"
  d2: "#101f2e"
  d3: "#0a1622"
  d4: "#060e17"
  line: "#4a8cbe"
  ink: "#e3eaf2"
  ink-dim: "#8b9bab"
  ink-faint: "#7d8c9b"
  sun: "#d8963f"
  sun-ink: "#160f06"
  ok: "#5aa8a0"
  alert: "#d0574a"
  surface: "rgba(226,236,247,.048)"
  surface-2: "rgba(226,236,247,.078)"
  hairline: "rgba(226,236,247,.105)"
  hairline-2: "rgba(226,236,247,.058)"
typography:
  display:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"SF Pro Text\", \"Segoe UI\", system-ui, sans-serif"
    fontSize: "25px"
    fontWeight: 660
    lineHeight: 1.1
    letterSpacing: "-0.022em"
  headline:
    fontFamily: "{typography.display.fontFamily}"
    fontSize: "26px"
    fontWeight: 640
    lineHeight: 1
    letterSpacing: "-0.024em"
    fontFeature: "tabular-nums"
  title:
    fontFamily: "{typography.display.fontFamily}"
    fontSize: "14px"
    fontWeight: 640
    lineHeight: 1.45
    letterSpacing: "normal"
  body:
    fontFamily: "{typography.display.fontFamily}"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: "normal"
  dense:
    fontFamily: "{typography.display.fontFamily}"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: "normal"
  detail:
    fontFamily: "{typography.display.fontFamily}"
    fontSize: "11.5px"
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: "normal"
  label:
    fontFamily: "{typography.display.fontFamily}"
    fontSize: "10px"
    fontWeight: 590
    lineHeight: 1.45
    letterSpacing: "0.14em"
rounded:
  sm: "6px"
  md: "10px"
  lg: "14px"
  cover: "8px"
  control: "4px"
spacing:
  hair: "4px"
  tight: "6px"
  snug: "8px"
  gutter: "10px"
  step: "11px"
  room: "16px"
  panel: "18px"
  channel: "24px"
components:
  button:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.ink}"
    typography: "{typography.detail}"
    rounded: "{rounded.sm}"
    padding: "8px 16px"
  button-hover:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
  button-primary:
    backgroundColor: "{colors.sun}"
    textColor: "{colors.sun-ink}"
    typography: "{typography.detail}"
    rounded: "{rounded.sm}"
    padding: "8px 16px"
  button-sm:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.ink}"
    rounded: "{rounded.sm}"
    padding: "5px 11px"
  button-playpause:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.ink}"
    rounded: "50%"
    width: "31px"
    height: "31px"
  nav-item:
    backgroundColor: "transparent"
    textColor: "{colors.ink-dim}"
    rounded: "{rounded.sm}"
    padding: "7px 10px"
    size: "12.5px"
  nav-item-hover:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
  nav-item-current:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.ink}"
  checkbox:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.control}"
    width: "15px"
    height: "15px"
  checkbox-checked:
    backgroundColor: "{colors.sun}"
  card-transfer:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    padding: "14px 16px"
  notice-alert:
    backgroundColor: "rgba(208,87,74,.08)"
    textColor: "{colors.ink-dim}"
    rounded: "{rounded.sm}"
    padding: "10px 12px"
  meter:
    backgroundColor: "{colors.surface-2}"
    rounded: "2px"
    height: "3px"
  meter-fill:
    backgroundColor: "{colors.sun}"
    height: "3px"
---

# Design System: Pelican

## Overview

**Creative North Star: "The Fjord"**

Pelican is a cold blue-black room with one absolute horizontal in it. The
ground (`#070c13`) is fjord water seen from above; every surface in the app
sits at a stated depth in that water, and depth is the only spatial idea the
system has. Above the waterline is `--air` — the library, where you browse
your own music. Below it are `--d1` through `--d4`, each colder, darker and
denser than the last, ending at the player bar, which is the deepest surface
on screen. One warm value, brass (`#d8963f`), is the low sun on the water; it
appears on the active state and the primary action and nowhere else.

The register is Scandinavian restraint: the craft lives in the joinery — a
1px waterline with a 15px bloom, a hairline that is 5.8% ink and not a
border-coloured line, a scrollbar drawn rather than inherited — and there is
no applied ornament anywhere. Nothing is decorated to look designed. The
density is a desktop utility's: 13px body, 46px titlebar, 66px player, rows
at 7px vertical padding. It reads like a native Mac tool because it is built
out of the same materials one is: the system type stack, no bundled asset, no
raster of any kind.

The product's promises are load-bearing on the visuals. Nothing leaves the
machine, which means no webfont, no remote image, no telemetry pixel; the
frontend is vanilla HTML/CSS/JS with no framework and no bundler, so the
system has to be expressible in one 415-line stylesheet and custom properties
on `:root`. And the app ships zero rasters: album art is a code-drawn frame
filled at runtime from the user's own files. The measured comp spec records
10 regions and 0 plates.

**Key Characteristics:**
- Depth as the only spatial metaphor; five named grounds, no shadow ladder.
- One absolute horizontal — the waterline — as the only structural rule.
- Exactly one warm value, reserved; everything else is blue-black and ink.
- Colour never signals alone; every state also says its name in words.
- No shipped rasters, no bundled fonts, no icon font: SVG paths and CSS.
- Container queries, not media queries — this is a window, not a viewport.
- One thing moves with intent (the water); nothing loops or pulses at rest.

## Colors

A single cold hue family (blue-black, ~205–212°) carrying five stated depths,
with one warm accent and two status values that never carry meaning alone.

### Primary
- **Brass** (`#d8963f`): the low sun on the water. The only warm value in the
  system and the only fill that is not blue-black. It appears on the primary
  action (`Send N tracks`), the checked checkbox, the currently-playing track
  title and its transport icon, the transfer meter fill, the caret, the text
  selection wash (28% brass) and the focus ring. That list is exhaustive; the
  comp measured brass at 0.41% coverage of the frame and that is the target.
  Dark ink (`#160f06`) sits on it at 7.56:1.

### Secondary
- **Waterline Blue** (`#4a8cbe`): the structural line. Used for the 1px
  waterline (as a left-fading gradient with a 15px bloom at 32% opacity) and
  the diagonal hatch of the pending band. It is a rule, not an accent: it is
  never a text colour and never a fill.

### Tertiary
- **Kelp** (`#5aa8a0`): the connection dot when a watch is present, 7px. Never
  used for text.
- **Signal Red** (`#d0574a`): the untagged-file warning, the over-capacity
  selection readout, and the failure notice's tag and hairline. Always paired
  with a word: `Needs a title`, `26 MB too much`.

### Neutral
- **Fjord Ground** (`#070c13`): the body behind the app; visible only at the
  window's own edges.
- **Air** (`#0d1622`) / **Air Raised** (`#111c2a`): the surface above the
  waterline — the app field, the titlebar, and the top of the sources rail's
  vertical gradient.
- **Depths d1–d4** (`#16283a`, `#101f2e`, `#0a1622`, `#060e17`): the ordered
  ladder below the line. `d3→d2` is the watch wall's own ground; `d1→d3→d4` is
  the water fill itself, lightest at the surface and darkest at the floor;
  `d4` alone is the player bar.
- **Ink** (`#e3eaf2`, 14.99:1 on air): titles, primary text, the seek thumb.
- **Ink Dim** (`#8b9bab`, 6.38:1): secondary lines — track durations, album
  credits, the transfer's running commentary.
- **Ink Faint** (`#7d8c9b`, 5.28:1): the quietest legible tier — counts,
  formats, timecodes, connection status, uppercase labels.
- **Surface veils** (`rgba(226,236,247,.048)` and `.078`): the only way an
  element becomes "raised". Hover is the 4.8% veil, current/rest is the 7.8%.
- **Hairlines** (`rgba(226,236,247,.105)` and `.058`): control borders and
  structural dividers respectively. Every panel edge in the app is `.058`.

### Named Rules
**The One Warm Value Rule.** Brass marks the active state and the primary
action. It is never a hover colour, never a heading colour, never a border on
something inert. If a second thing on screen wants to be brass, one of them is
not actually the primary action.

**The Word With The Colour Rule.** No state is carried by hue alone. Red
appears only alongside its sentence; the connection dot only alongside the
word `connected` or `no watch found`; over-capacity turns red *and* changes
its text to `… too much`. Test: convert the screen to greyscale — if a state
became unreadable, the state is under-built.

**The Deeper Is Colder Rule.** A surface's colour states its depth. Moving a
panel down the ladder means moving it toward `#060e17`; there is no lighter-
means-nearer inversion anywhere in the system.

## Typography

**Display / Body / Label Font:** the macOS system stack — `-apple-system,
BlinkMacSystemFont, "SF Pro Text", "Segoe UI", system-ui, sans-serif`. One
family, no pairing, no distinct mono.

This is a deliberate, recorded decision and it should survive review. The
comp was authored and approved in this stack; it is the correct register for a
native Mac utility; and it needs no bundled asset, which matters in an app
that promises no network and audits its dependency surface with cargo-deny.
A type-matching pass ranked catalogue faces from a fingerprint of a
*screenshot of this same stack* and reported that its browser ranking could
not run, so its suggestion is a recorded deviation, not a finding. Swapping to
a webfont trades away the no-network promise and the native register for
nothing.

**Character:** neutral, tightly-tracked at large sizes, unstyled at small
ones. Numerals are tabular everywhere they appear (`.num`) because nearly
every number in this app — durations, capacities, counts, formats — is one a
user compares against another number.

### Hierarchy
Cap heights below were measured from the approved comp (captured 730×467 for
an 1180×760 frame, 1.62× scale); app-pixel values follow in brackets.

- **Display / album title** (660, 25px, 1.1, −0.022em): the album hero title,
  one line, no wrap at desktop. Measured cap height 11.4px [18.4px app].
- **Headline / capacity figure** (640, 26px, 1, −0.024em, tabular): the free-
  space number on the watch wall — the largest text in the app, on purpose,
  because capacity is the fact that governs every decision. Measured cap
  height 12.1px [19.6px app]; the region reads as all-caps because it is
  numerals.
- **Title** (640, 14px): panel headings — `On your watch`, the empty-state
  head. 13.5px for the transfer card's heading.
- **Body** (400, 13px, 1.45): the default. Track titles, album credits, the
  disconnected explanation. Measured cap height in the track list 6.7px
  [10.8px app].
- **Dense** (400, 12px–12.5px): the watch wall list (`--fs-wall` 12px, cap
  height 6.3px [10.2px app]), nav items (12.5px), player now-playing (12.5px),
  button faces (12.5px, weight 590).
- **Detail** (400, 11px–11.5px): sub-lines and metadata — durations, sizes,
  hero facts, timecodes, hints, notice bodies.
- **Label** (590, 10px, 0.14em, uppercase): the wordmark, rail section heads,
  the player's format badge. At 10.5px and 0.05em, non-uppercase, the same
  role covers per-track counts and format cells.

### Named Rules
**The One Family Rule.** There is no display face and no mono face. Hierarchy
is made from weight (400 / 590 / 640 / 660), size, and ink tier — never from a
second family. A bundled webfont is a network promise this product does not
make.

**The Tabular Number Rule.** Any numeral a user might compare against another
numeral carries `font-variant-numeric: tabular-nums`. In practice that is
every number in the app.

**The Measure Rule.** Prose blocks are capped at 34ch (album credits, the
disconnected explanation). Nothing in this app is long-form; if a string needs
more than 34 characters of width, it is data, not prose.

## Layout

The window is a three-column, three-row CSS grid with named areas, and it is
the whole layout: `198px | 1fr | 300px` across, `46px | 1fr | 66px` down —
titlebar spanning the top, sources rail / album channel / watch wall across
the middle, player spanning the bottom. Both flexible tracks are `minmax(0,
1fr)` so a long title can never push the grid wider than the window.

Spacing is a fine 2px-based rhythm rather than a strict ramp, but the reused
steps are: 4px (intra-line gaps), 6–8px (control padding, list-row vertical),
10px (rail padding, table cell horizontal, the common gap), 11px (the
component gap — player, wall list, transfer internals), 14–16px (titlebar
gap, rail padding, section separation), 18px (titlebar and wall horizontal
padding), 24px (channel horizontal padding). The channel is the widest gutter
in the app and the wall is one step tighter; that difference is what makes the
wall read as a further, colder plane.

**Adaptation is by container query on `.shell`, not media query.** This is a
window, not a page: its layout answers to its own width so the breakpoints
stay honest when the user resizes the window rather than the display. Two
steps:

- **≤1020px** — the rail becomes a horizontal scrolling strip under the
  titlebar (`"rail rail"` spanning both columns), channel and wall sit side by
  side with the wall at 280px. The rail's local note is dropped here because
  the hero still carries that truth. Nothing becomes unreachable at any size:
  the rail must not vanish, because it carries Albums, Artists, Recently added
  and every playlist.
- **≤720px** — one column, stacked titlebar / rail / channel / wall / player.
  The water stops being a fill behind the whole panel and becomes an 84px band
  behind the capacity readout; the wall list is capped at 32vh. The hero
  wraps, the cover drops to 88px, actions go full width and split evenly. The
  format column leaves the track table and the player's now-playing block and
  format badge are hidden outright — truncating a title to `G…` is worse than
  not showing it.

The player bar is fluid: the now-playing block is `clamp(220px, 21%, 340px)`
and the scrubber absorbs the leftover width, as Music.app, Spotify and VLC all
do. This means the scrubber's start position does not hold a fixed percentage
across window widths. That is a recorded, accepted trade — the alternative
pinned the comp's exact position at the cost of ~540px of empty now-playing
block at 2560 wide — and it is one CSS line to reverse.

### Named Rules
**The Window Not Viewport Rule.** Every breakpoint in this system is
`@container shell (max-width: …)`. Adding a `@media (max-width: …)` layout
rule is a defect: it makes the app answer to the display instead of to itself.

**The Nothing Unreachable Rule.** No breakpoint may remove a navigation
target. Panels may re-flow, re-orient or scroll; they may not disappear with
their contents. Content that is duplicated elsewhere (the rail's playlist
note) may be dropped; content that is unique may not.

## Elevation & Depth

There is no shadow ladder in this system. Depth is tonal: an element is
"deeper" because it is a colder, darker blue, and "raised" because it carries
a white veil at 4.8% or 7.8%. The full ordering, air to floor, is
`air-2 → air → d3 → d2 → d1 → d3 → d4`: raised rail, app field, watch wall
top, watch wall bottom, then the water's own gradient from its lit surface
down to the floor, with the player bar at `d4` beneath everything.

Only three real shadows exist and two of them are inner highlights, not lifts.
The one true drop shadow in the app is under the large album cover.

### Shadow Vocabulary
- **Cover lift** (`box-shadow: 0 18px 44px rgba(0,0,0,.55)`): the 132px album
  cover only. It is the one object presented as a physical thing.
- **Frame highlight** (`inset 0 1px 0 rgba(226,236,247,.05–.06)`): a 1px top
  light on every cover frame, at every size. Joinery, not elevation.
- **Waterline bloom** (`box-shadow: 0 0 15px rgba(74,140,190,.32)`): the glow
  under the 1px waterline. The only luminous object in the app.

The single blur in the system is the transfer card: `backdrop-filter:
blur(24px) saturate(140%)` over a 4.8% veil with a 10.5% hairline. It is the
only element that floats, and it is docked — `left:210px; right:314px;
bottom:80px` — deliberately stopping short of the watch wall, because capacity
is the number actually changing during a transfer and the card must never
cover it.

### Named Rules
**The Tonal Depth Rule.** New surfaces take a depth from the ladder; they do
not take a shadow. If a panel needs to read as further away, it moves toward
`#060e17`, not under a blur.

**The One Luminous Object Rule.** The waterline is the only thing in the app
that emits light. No second glow, no accent bloom, no brass halo.

**The Never Cover The Number Rule.** Floating surfaces are docked with
explicit insets that clear the capacity readout. A transient panel may cover
the library; it may never cover the fact that is changing.

## Shapes

Rectangles with small, quiet radii and hairline edges. Three radius steps do
almost all the work: 6px on controls (buttons, nav items, the untagged
warning, the failure notice), 10px unused in the shipped surface but reserved
as the mid step, and 14px on the one floating card. Covers are their own
family — 5px base, 8px at the 132px hero size, so the corner stays visually
constant as the frame grows. Small interactive parts round to 4px (checkbox,
icon buttons) and circles are reserved for exactly two things: the 7px
connection dot and the 31px play/pause control.

Borders are hairlines, never lines: `rgba(226,236,247,.058)` for structure
(every grid divider in the app is this one value) and `.105` for the edge of
something you can press. The 3px progress meter and 3px seek track round to
2px — a hair, enough to stop the ends reading as cut.

The cover frame is the system's one drawn object: a 155° blue gradient with a
9% hairline and an inner top highlight, standing in for artwork that is read
from the user's own files at runtime. The gradient is a value stand-in, not a
graphic.

### Named Rules
**The Hairline Rule.** Structural separation is a 5.8%-ink 1px line. Not a
grey, not a `--border` colour, not a 2px rule, and never two lines where the
tonal step already separates the panels.

**The No Applied Ornament Rule.** No borders that do not separate, no
gradients that do not describe depth, no rounded corner larger than 14px, no
decorative divider. If a shape is not doing structural work, remove it.

## Components

The feel is refined and restrained: controls are quiet at rest, and they
answer in 120ms with a veil, never a jump.

### Buttons
- **Shape:** small, quiet radius (6px), 1px hairline edge.
- **Primary:** brass fill (`#d8963f`) with near-black ink (`#160f06`), 590
  weight, 12.5px, `8px 16px` padding. Hover brightens the fill 8%; there is no
  second brass button on any screen.
- **Secondary:** 7.8% veil with a 10.5% hairline and ink text; hover *drops*
  to the 4.8% veil rather than brightening — the surface recedes under the
  cursor.
- **Small:** `5px 11px`, 11.5px face. Used inside notices and warnings.
- **Active:** `translateY(1px)`. That is the whole press feedback.
- **Disabled:** 45% opacity, `cursor: default`, and it keeps pointer events on
  purpose. `pointer-events: none` would hide the reason from a mouse user as
  well as from a screen reader; the reason is rendered as real text and wired
  with `aria-describedby`.
- **Play/pause:** a 31px circle on the 7.8% veil with a hairline, ink glyph.
  It was a filled white disc and was demoted: it was the brightest object on
  screen, outshouting the brass primary it is meant to sit beneath.
- **Icon button:** no background, no border, `--ink-faint` at rest, ink on
  hover, brass when its row is playing.

### Inputs / Fields
- **Checkbox:** appearance-none, 15px square, 4px radius, 4.8% veil with a
  10.5% hairline; checked is a solid brass fill with a brass border.
- **Seek slider:** a 3px track painted as a live gradient (`--ink-dim` up to
  the played fraction, 10% ink after). The 11px ink thumb is `opacity: 0` at
  rest and appears on hover and `:focus-visible` — an idle player should be a
  quiet line, not a bright dot demanding attention.
- **Caret:** brass, in every input.

### Navigation
- Rail items are 12.5px, `--ink-dim`, `7px 10px`, 6px radius, with the count
  pushed right at 10.5px `--ink-faint`. Hover takes the 4.8% veil and ink
  text; the current item takes the 7.8% veil, ink text and 590 weight —
  emphasis by weight and veil, never by brass.
- Section heads are 10px/0.14em uppercase `--ink-faint`.
- At ≤1020px the rail becomes a horizontal strip, items `white-space: nowrap`,
  on the raised `--air-2` ground with a bottom hairline instead of a right one.

### Cards / Containers
- **Transfer card:** 14px radius, 4.8% veil, `blur(24px) saturate(140%)`,
  10.5% hairline, `14px 16px` padding, docked over the channel. Enters with
  `@starting-style` from `opacity:0; translateY(10px)` over 260ms.
- **Notice / pre-warning:** 6px radius, `10px 12px`, red at 8% fill and 32–34%
  border, with a two-column grid so the actions sit beside the body and the
  uppercase tag spans the full width above them. The tag is the state's name
  in words; it exists so the red is never the only signal.

### Signature Component: the watch wall and its waterline
The right-hand panel is the app's thesis made literal. The panel itself is a
`d3→d2` gradient — a step deeper than the channel beside it, because the
library is open air and the watch is under water. The water is an absolutely
positioned box pinned to the bottom whose `height` is the fraction of capacity
in use, filled `d1 → d3 (60%) → d4`. Its `::before` draws the waterline: 1px,
fading in from 20% at the left edge to solid `#4a8cbe`, with a 15px bloom.

The pending band shows what the current selection would add: a −45° hatch of
the line colour (20% / 7%) with a dashed top edge, sitting on top of the
current level. **It is a sibling of the water, not a child** — as a child its
percentage height resolved against the water's own height, so the band shrank
as the watch emptied. It is a fraction of total capacity, so it must measure
against the wall.

Everything else in the wall (head, list, more-link, capacity readout) sits at
`z-index: 2` above the water. The disconnected state is driven by
`[data-device="none"]` on the app root: list, count, water and capacity all
hide, the empty-state paragraph appears, and the connection dot desaturates
from kelp to `--ink-faint`.

### Motion
Scandinavian restraint applies to time as well as ink. One thing moves with
intent — the water — and everything else settles quickly or not at all.
Nothing loops or pulses while idle. Three durations, one easing curve
(`cubic-bezier(.22,.61,.36,1)`):

- **Quick, 120ms linear** — background, colour and opacity on every
  interactive element (nav, buttons, icon buttons, table rows, seek thumb).
- **Settle, 260ms** — the transfer card's entry, the meter fill, the pending
  band, and the `sink` keyframe that carries a sent row down 14px to
  `opacity: 0`. That gesture is comp E's downward send, surviving as behaviour
  rather than as a layout.
- **Tide, 900ms** — the water rising. The only slow thing in the app, and it
  runs once per transfer.

**`.water` animates `height`, deliberately, against the usual advice to
animate `transform`.** It is out of flow and childless, so the layout cost is
a single absolutely-positioned box, not a frame budget — and `scaleY` would
squash the depth gradient and thicken the 1px waterline, which is the one
detail the whole design rests on. `--used` carries the initial level from the
markup so the panel is correct before script runs; script then drives `height`
directly, because WKWebView will not transition a property whose value comes
from a custom property. `.meter i` is the opposite case and takes `scaleX`: a
solid fill with no gradient or border looks identical scaled and animates on
the compositor.

Under `prefers-reduced-motion: reduce`, every animation and transition is
clamped to 1ms and the send gesture skips straight to the settled state. State
still changes; it just arrives rather than travels.

### Accessibility commitments
- **`--ink-faint` is `#7d8c9b` for contrast: 5.28:1 on `--air`.** It was
  `#5a6875` = 3.18:1 and failed AA on every count, size, duration and the
  connection status. This value is not a tone choice and must not be "tidied"
  darker. Its worst ground in the app is `--d2` (4.85:1), still AA.
- Ink is 14.99:1, ink-dim 6.38:1, brass 7.23:1 on `--air`; dark ink on brass
  is 7.56:1.
- **Known shortfall, recorded not endorsed:** `--alert` `#d0574a` measures
  4.45:1 on `--air` and 4.22:1 on its own 7% tinted background — just under
  AA for its 10–11.5px use. It is mitigated by the Word With The Colour Rule,
  not fixed by it. A future pass should lift the red rather than enlarge the
  text.
- Focus is a 2px brass outline at 2px offset with a 3px radius, on
  `:focus-visible` only, globally.
- Every state carries a word, never a colour alone.
- Disabled controls keep their cursor and publish their reason as real DOM
  text referenced by `aria-describedby`.
- Status strings are written into the DOM, never via CSS `content` — generated
  text is not reliably announced.
- The transfer region is `aria-live="polite"`; the meter's `aria-valuenow` and
  its visual fill are driven from one function so they cannot drift.
- Table headers, captions and column headings exist and are `.sr-only`; icons
  are `aria-hidden` with the label on the button.

## Do's and Don'ts

### Do:
- **Do** place a new surface on the depth ladder (`--air`, `--air-2`,
  `--d1`–`--d4`) and let its colour state how deep it is.
- **Do** use the two white veils (4.8% / 7.8%) for raised and current states,
  and the two hairlines (5.8% structural / 10.5% control) for edges.
- **Do** pair every colour-carried state with its word, and check the screen
  in greyscale before calling it done.
- **Do** keep `--ink-faint` at `#7d8c9b`; the 5.28:1 ratio is the reason it is
  that value.
- **Do** write new breakpoints as `@container shell (max-width: …)`.
- **Do** mark comparable numerals `.num`.
- **Do** draw new icons as inline SVG paths in the sprite, `fill/stroke:
  currentColor` at 15×14 (17×16 large), stroke-width 1.4, round caps.
- **Do** keep prose blocks at or under 34ch.
- **Do** animate colour and background at 120ms, state arrivals at 260ms, and
  reserve 900ms for the water alone.

### Don't:
- **Don't** put brass on anything that is not the primary action or the active
  state — not on hover, not on a heading, not on a border.
- **Don't** add a drop shadow to convey elevation; the cover lift is the only
  one, and depth is tonal.
- **Don't** introduce a second font family, a webfont, or an icon font. The
  no-network promise and the auditable dependency surface both depend on it.
- **Don't** ship a raster. Album art is a code-drawn frame filled from the
  user's own files at runtime; the build records 0 plates and should stay
  there.
- **Don't** make the pending band a child of `.water`, or animate `.water`
  with `transform` — both were tried and both break the thing the design rests
  on.
- **Don't** use `pointer-events: none` to disable a control; opacity plus a
  stated reason.
- **Don't** let a floating panel cover the capacity readout.
- **Don't** remove a navigation target at a breakpoint. Re-flow it.
- **Don't** invent placeholder numbers. Capacities, counts, sizes and formats
  are read from the device and the user's files; a fabricated value in this
  app is a lie about their hardware.
- **Don't** add anything that loops, pulses or breathes at rest.
