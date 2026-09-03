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
  alert: "#d9634f"
  notice-ground: "#191b25"
  surface: "rgba(226,236,247,.048)"
  surface-2: "rgba(226,236,247,.078)"
  hairline: "rgba(226,236,247,.105)"
  hairline-2: "rgba(226,236,247,.058)"
  edge: "rgba(226,236,247,.40)"
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
    backgroundColor: "rgba(13,22,34,.97)"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    padding: "14px 16px"
  notice-alert:
    backgroundColor: "{colors.notice-ground}"
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
1px waterline with a bloom that falls only downward, a hairline that is 5.8% ink and not a
border-coloured line, a scrollbar drawn rather than inherited — and there is
no applied ornament anywhere. Nothing is decorated to look designed. The
density is a desktop utility's: 13px body, 46px titlebar, 66px player, rows
at 7px vertical padding. It reads like a native Mac tool because it is built
out of the same materials one is: the system type stack, no bundled asset, no
raster of any kind.

The product's promises are load-bearing on the visuals. Nothing leaves the
machine, which means no webfont, no remote image, no telemetry pixel; the
frontend is vanilla HTML/CSS/JS with no framework and no bundler, so the
system has to be expressible in one 494-line stylesheet and custom properties
on `:root`. And the app ships zero rasters: album art is a code-drawn
frame; **this build never fills it.** `read_fast` passes
`read_cover_art(false)`, `SourceInfo` has no picture field, `TrackDto` has no
art field, and nothing in `ui/app.js` sets a background or a `src` on
`.cover`. The frame is the whole of it. The measured comp spec records
10 regions and 0 plates.

**Key Characteristics:**
- Depth as the only spatial metaphor; five named grounds, no shadow ladder.
- One absolute horizontal — the waterline — as the only structural rule, and
  it now has a stated range: it travels within the gauge and never crosses
  type.
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
  waterline (a left-fading gradient carrying a directional bloom,
  `0 6px 12px -4px rgba(74,140,190,.32)`) and the diagonal hatch of the
  pending band. It is a rule, not an accent: it is never a text colour and
  never a fill. Measured as text it would be 5.01:1 on `--air`; it is not used
  as text anywhere, and should not be.

### Tertiary
- **Kelp** (`#5aa8a0`): the connection dot when a watch is present, 7px. Never
  used for text.
- **Signal Red** (`#d9634f`): the untagged-file warning, the over-capacity
  selection readout, the failure notice's tag and hairline, and — the fourth
  use — **the delete control and its confirmation, the only irreversible
  action in the app.** Always paired with a word: `Needs a title`, `26 MB too
  much`, `Delete`.

  The destructive control is allowed on exactly two grounds, and both are
  opaque: the selection bar's `--d3` (**5.08:1**) and the confirm's
  `--notice-ground` (**4.77:1**). It is **forbidden** on a wall row, where the
  ratio is a property of whatever veil the row is wearing rather than of the
  rule: `--alert` is **3.80:1** on a selected row (`--surface-2` over `--d2` =
  `#202f3e`) and **4.12:1** on a hovered one. A 10% alert-hued hover tint on
  its two legal grounds drops them to 4.60 and 4.28, so **hover moves the
  border, never the ground** — `--ink-dim` as a border is 6.41:1 on `--d3` and
  6.02:1 on `--notice-ground`.

- **Edge** (`rgba(226,236,247,.40)`): **control boundaries only.** Not a
  hairline and not a replacement for one — the same
  `rgba(226,236,247, x)` veil family, a new alpha rather than a new hue.
  Measured on every ground a control sits on: `--air` 3.40, `--air-2` 3.37,
  `--d1` 3.23, `--d2` 3.34, `--d3` 3.40, `--d4` 3.37, `--notice-ground` 3.39,
  hovered wall row 3.23, selected wall row 3.16. Worst case **3.16:1**, clear
  of 1.4.11's 3:1. `--hairline` measures **1.25–1.35:1** on those same grounds
  and identifies nothing; it stays a structural divider. This token exists
  because the wall grew a checkbox on all 22 rows, on a darker ground, which
  turned a small pre-existing problem in the library table into the panel's
  main affordance.

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
- **Notice Ground** (`#191b25`): the one opaque ground invented for a
  legibility reason rather than a depth one. It is `--air` with the alert hue
  flattened into it, and it exists so the transfer failure headline's contrast
  is a property of its own rule instead of a property of whatever three layers
  happen to be behind it. `--alert` measures 4.77:1 here under any backdrop.
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

**The Ground You Actually Sit On Rule.** A contrast ratio is measured against
the composite an element renders over, not against the token named in its
parent's rule. Three failures in this build came from measuring against the
wrong ground: `--alert` inside `.notice` inside a translucent `.transfer` read
5.06:1 on paper and 4.18:1 on the real three-layer composite; `--ink-faint` in
the rail read 5.28:1 on `--air` and 4.09:1 on `--surface-2` over the rail
gradient; the capacity readout read fine until `.water`'s `--d1` stop rose
behind it at 4.18:1. Where a stack cannot be pinned down — the transfer card
floats over scrolling album art — give the element an opaque ground and the
question stops being open.

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
  The gauge drops from 132px to 84px and nothing else about the water changes;
  the wall list is capped at 32vh. The hero
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
down to the floor, with the player bar at `d4` beneath everything. The
`d1 → d3 → d4` run is now compressed into the 132px gauge rather than spanning
the whole panel; the ordering is unchanged, its extent is not.

Only three real shadows exist and two of them are inner highlights, not lifts.
The one true drop shadow in the app is under the large album cover.

### Shadow Vocabulary
- **Cover lift** (`box-shadow: 0 18px 44px rgba(0,0,0,.55)`): the 132px album
  cover only. It is the one object presented as a physical thing.
- **Frame highlight** (`inset 0 1px 0 rgba(226,236,247,.05–.06)`): a 1px top
  light on every cover frame, at every size. Joinery, not elevation.
- **Waterline bloom** (`box-shadow: 0 6px 12px -4px rgba(74,140,190,.32)`):
  the glow under the 1px waterline. The only luminous object in the app, and
  it is deliberately *directional* — offset 6px down with a −4px spread, so the
  light falls into the water and not into the air. A symmetrical halo washed
  the ground the wall list scrolls across and pushed those rows under AA.

The single blur in the system is the transfer card: `backdrop-filter:
blur(24px) saturate(140%)` over a **near-opaque base** (`rgba(13,22,34,.97)`)
with a 10.5% hairline. The base is the load-bearing part. The card floats over
`.channel`, which scrolls album art at full size, and over a 4.8% veil every
ink on the card inherited whatever was behind it — bright art dragged the
failure headline to 3.99:1. The blur stays; it is still glass, it just has a
floor. At 97% over worst-case white artwork the card's ground composites to
about `#141d29`, where ink is 13.99:1, `--ink-dim` 5.96:1 and `--ink-faint`
4.93:1 — all still AA. It is the only element that floats, and it is docked — `left:210px; right:314px;
bottom:80px` — deliberately stopping short of the watch wall, because capacity
is the number actually changing during a transfer and the card must never
cover it.

### Named Rules
**The Tonal Depth Rule.** New surfaces take a depth from the ladder; they do
not take a shadow. If a panel needs to read as further away, it moves toward
`#060e17`, not under a blur.

**The One Luminous Object Rule.** The waterline is the only thing in the app
that emits light, and it lights downward only. No second glow, no accent
bloom, no brass halo, and no light cast up into the air where text lives.

**The Floor Under The Glass Rule.** A translucent surface that can float over
arbitrary content gets an opaque or near-opaque base beneath its blur. Glass
is a look, not a contrast strategy; without a floor, every ink on the panel
inherits its ratio from whatever scrolls behind it.

**The Never Cover The Number Rule.** Floating surfaces are docked with
explicit insets that clear the capacity readout. A transient panel may cover
the library; it may never cover the fact that is changing.

## Shapes

Rectangles with small, quiet radii and hairline edges. The scale is
6 / 10 / 14px and it governs panels and controls: 6px does nearly all the work
(buttons, nav items, the rail's new-playlist control, the untagged warning,
the failure notice), 10px is defined but unused in the shipped surface and
reserved as the mid step, and 14px is on the one floating card. Covers are their own
family — 5px base, 8px at the 132px hero size, so the corner stays visually
constant as the frame grows. Small interactive parts round to 4px (checkbox,
icon buttons) and circles are reserved for exactly two things: the 7px
connection dot and the 31px play/pause control.

**Where the scale stops.** Four radii sit deliberately beneath the 6/10/14
scale, because a 6px corner on a 3px-tall bar is a pill rather than a corner.
They are registered as sanctioned exceptions in `.impeccable/config.json`, not
drift: **2px** on the 3px seek track and the 3px transfer meter; **3px** on the
focus ring (`:focus-visible`, 2px brass outline at 2px offset); **4px** on the
15px checkbox and the icon buttons; **5px** on the scrollbar thumb, which is
9px wide, so 5px is its own half-width. Covers are their own family for the
opposite reason — 5px base and 8px at the 132px hero size, so the corner reads
as visually constant as the frame grows. Circles are reserved for exactly
three things: the 7px connection dot, the 31px play/pause control and the 11px
seek thumb.

Borders are hairlines, never lines: `rgba(226,236,247,.058)` for structure
(every panel edge and grid divider in the app is this one value) and `.105`
for a button's resting edge. **The edge of a control you tick or press is
`--edge` at 40%**, which is the only one of the three that measures over 3:1
against the grounds it sits on. A disabled control keeps `--hairline`
deliberately: something that cannot be operated should not advertise a
3:1 boundary as if it could.

The cover frame is the system's one drawn object: a 155° blue gradient with a
9% hairline and an inner top highlight, standing in for artwork that is read
from the user's own files at runtime. The gradient is a value stand-in, not a
graphic.

### Named Rules
**The Hairline Rule.** Structural separation is a 5.8%-ink 1px line. Not a
grey, not a `--border` colour, not a 2px rule, and never two lines where the
tonal step already separates the panels.

**The Scale Has A Floor Rule.** 6 / 10 / 14 governs anything with an interior.
An element smaller than about 16px in its short dimension takes a radius from
its own geometry instead, and that exception is registered rather than
assumed. Adding a fifth off-scale radius means registering it too, or it is
drift.

**The Destructive Control Rule.** The one irreversible action in the app has
two constraints and they are not negotiable. It never lives in a row — only
on an opaque ground, so its contrast is a property of its own rule. And it
never tints its own ground — hover moves the border, because every alert-hued
wash tested drops the ink under AA. There is also no modal: the confirmation
is inline, above the list, because "Never Cover The Number" forbids anything
floating over the capacity readout and the free-space figure is exactly what
someone deciding whether to delete needs in view while they decide.

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
  **`--edge` (40%)** border; checked is a solid brass fill with a brass
  border, `--sun` being 5.43:1 on the darkest ground a checkbox sits on.
  Disabled drops back to `--hairline` at 45% opacity. The same control appears
  on both sides of the window on purpose: a tick on the left sends to the
  watch and a tick on the right removes from it, which is one symmetrical
  idea. Two row vocabularies made it two.
- **Danger button:** transparent ground, `--edge` border, `--alert` text.
  Hover moves the border to `--ink-dim` and never fills. See Signal Red above
  for the two grounds it is allowed on and the two it is not.
- **Seek slider:** a 3px track painted as a live gradient (`--ink-dim` up to
  the played fraction, 10% ink after). The 11px ink thumb is `opacity: 0` at
  rest and appears on hover and `:focus-visible` — an idle player should be a
  quiet line, not a bright dot demanding attention.
- **Caret:** brass, in every input.

### Navigation

**Two controls, deliberately different, because they answer different
questions.** The rail switches *what the library shows you* — All music,
Albums, Artists — which is navigation, so it is `.nav`. The wall's Files /
Album / Artist control switches *how one fixed list is arranged*: the same
files, differently ordered, so it is a segmented toggle and it is `.chip`.
Making them look alike would say the two do the same kind of thing. They do
not, and the next reviewer to file this as an inconsistency should read this
paragraph first.

- **Chips** (the wall's arrangement toggle) are 11px/590, `4px 9px`, 6px
  radius, a `--edge` border on a transparent ground, `--ink-dim` text
  (**6.41:1** on the head's `--d3`). Pressed takes the 7.8% veil and `--ink`
  (**12.51:1** on the `#1b2733` composite) and carries `aria-pressed`, which
  is the state, not the class. **Files is the default and stays the default:**
  it is the order `list_dir("Music")` returned, and the panel should open on
  the device's answer rather than on Pelican's memory of it.
- Rail items are 12.5px, `--ink-dim`, `7px 10px`, 6px radius, with the count
  pushed right at 10.5px **`--ink-dim`** — not `--ink-faint`. The count sits on
  a composite, not on `--air`: `.is-current` stacks `--surface-2` over the
  rail's `--air-2` stop for `#212c3a`, where `--ink-faint` is 4.09:1, and
  `:hover` gives `#1b2634` at 4.44:1. Both fail AA, and the default markup
  ships `.is-current`, so the badge was under AA at open. `--ink-dim` is
  4.95:1 and 5.37:1 on those same grounds. No palette token changed to fix it. Hover takes the 4.8% veil and ink
  text; the current item takes the 7.8% veil, ink text and 590 weight —
  emphasis by weight and veil, never by brass.
- Section heads are 10px/0.14em uppercase `--ink-faint`.
- **The album / artist list** in the channel is a `.group` button per row: a
  38px cover frame, title at 13px/590, a `--ink-dim` subtitle and a
  `--ink-dim` count. `--ink-faint` is out of bounds here too — the hover veil
  composites to `#1b2634` where it measures 4.44:1.
- **Crumb.** One `.linkbtn`, underlined, `--ink-dim`. Leaving an album is
  navigation inside the view the rail already chose, so it is a link and not a
  button.
- At ≤1020px the rail becomes a horizontal strip, items `white-space: nowrap`,
  on the raised `--air-2` ground with a bottom hairline instead of a right one.

### Cards / Containers
- **Transfer card:** 14px radius, near-opaque base `rgba(13,22,34,.97)` under
  `blur(24px) saturate(140%)`, 10.5% hairline, `14px 16px` padding, docked over
  the channel. Enters with
  `@starting-style` from `opacity:0; translateY(10px)` over 260ms.
- **Failure notice:** 6px radius, `10px 12px`, an **opaque** `--notice-ground`
  fill with a `rgba(217,99,79,.34)` border, in a two-column grid so the actions
  sit beside the body and the uppercase tag spans the full width above them.
  The opacity is the point: `.notice` nests inside `.transfer`, and a red tint
  over that composite put `--alert` at 4.18:1 — below AA on the one string in
  the app that reports a failure. Flattened, `--alert` is 4.77:1 and
  `--ink-dim` 6.02:1 there, regardless of what the channel is scrolling behind
  the card. Do not restore the translucent fill.
- **Pre-send warning (`.prewarn`):** the same 6px / `10px 12px` / two-column
  shape, but it sits directly on `--air` rather than inside glass, so it keeps
  a tint: `rgba(217,99,79,.07)` fill with a `.32` border. That composite is
  `#1b1b25`, where `--alert` is 4.74:1 and `--ink-dim` 5.97:1. The uppercase
  tag is the state's name in words; it exists so the red is never the only
  signal.
- **Capacity readout:** opaque `--d3` backing, `12px 18px 16px`. `.water` is
  anchored to the same bottom edge, so on a nearly empty watch its lightest
  stop `--d1` rises directly behind this block and dropped the over-capacity
  warning to 4.18:1. On the opaque `--d3` it is 5.08:1 at any level.

### Signature Component: the watch wall and its waterline
The right-hand panel is the app's thesis made literal. The panel itself is a
`d3→d2` gradient — a step deeper than the channel beside it, because the
library is open air and the watch is under water.

**The water lives in a gauge, and nowhere else.** `.gauge` is a fixed
`--gauge: 132px` band between the track list and the capacity readout, with
`overflow: hidden`. **Its height *is* total capacity**, so `--used` and
`--pending` resolve against it. Inside it the water is an absolutely
positioned box at **full height of the gauge**, filled `d1 → d3 (60%) → d4`
and **translated down** to the level:
`transform: translateY(calc(100% - var(--used, 0%)))`. Its `::before` draws the
waterline: 1px, fading in from 20% at the left edge to solid `#4a8cbe`, with a
downward-only bloom.

The gauge is why the waterline can no longer cut a title in half. The water
used to be a full-panel fill behind the entire wall, so at any level under
100% its line and its bloom crossed the track rows horizontally. The ≤720px
breakpoint had already found this and fixed it locally with a fixed band; the
gauge generalises that fix to every width, which **removes a special case
rather than adding one**. Nothing inside the band is text, so there is nothing
left for the line to cross.

**Translating a full-height gradient, rather than compressing a gradient into
a short box, is what makes the two details true.** The waterline stays exactly
1px at every level, because nothing about the box is being scaled. And the
depth ramp becomes honest: a nearly-empty watch shows only near-surface tones
instead of cramming `d1` through `d4` into a sliver, because shallow water is
not abyssal. The geometry is exact and measured — in the 132px gauge, levels
of 8 / 35 / 88 / 100% expose **10.6 / 46.2 / 116.2 / 132px** of water.

The pending band shows what the current selection would add: a −45° hatch of
the line colour (20% / 7%) with a dashed top edge, sitting on top of the
current level. **It is a child of the gauge and a sibling of the water, never
a child of the water** — as a child its percentage height resolved against the
water's own height, so the band shrank as the watch emptied. It is a fraction
of total capacity, so it must measure against the gauge. It carries a
`max(2px, …)` floor, which the gauge makes necessary: a 3% selection was ~20px
of a 668px wall and is 4px of a 132px gauge, and a real selection must not
round away to nothing.

At ≤720px only the band's height changes — `--gauge: 84px`, set on `.wall`
because a container query cannot reach `:root`. One mechanism at every width.

The stacking scaffold is gone with the fill it existed for: the head, list and
capacity readout no longer need `z-index: 2`, because nothing is behind them
any more. The capacity block keeps its opaque `--d3` backing for a different
reason — the panel's own gradient reaches its darkest stop exactly there, and
an opaque ground makes the ratio a property of the rule. The disconnected
state is driven by `[data-device="none"]` on the app root: list, count, gauge
and capacity all hide, the empty-state paragraph appears, and the connection
dot desaturates from kelp to `--ink-faint`.

**The wall's ink floor is `--ink-dim`.** `--ink-faint` is out of bounds inside
`.wall__list`, and the reason changed with the gauge. It used to be the
water's `--d1` stop rising behind the rows. The water is gone from behind
them, but every row is now hoverable and selectable, and those veils are the
ground the text actually sits on:

| ground | `--ink` | `--ink-dim` | `--ink-faint` |
|---|---|---|---|
| wall gradient, worst stop `--d2` `#101f2e` | 13.77 | **5.86** | 4.85 |
| hovered row, `--surface` over `--d2` = `#1a2938` | 12.21 | **5.20** | **4.30 FAIL** |
| selected row, `--surface-2` over `--d2` = `#202f3e` | 11.26 | **4.80** | **3.97 FAIL** |

### Motion
Scandinavian restraint applies to time as well as ink. One thing moves with
intent — the water — and everything else settles quickly or not at all.
Nothing loops or pulses while idle. Three durations, one easing curve
(`cubic-bezier(.22,.61,.36,1)`):

- **Quick, 120ms linear** — background, colour and opacity on every
  interactive element (nav, buttons, icon buttons, table rows, seek thumb).
- **Settle, 260ms** — the transfer card's entry, the meter fill, the pending
  band's opacity, and the `sink` keyframe that carries a sent row down 14px to
  `opacity: 0`. That gesture is comp E's downward send, surviving as behaviour
  rather than as a layout.
- **Tide, 900ms** — the water rising. The only slow thing in the app, and it
  runs once per transfer.

**`.water` animates `transform`, not `height`.** The element is full-height
and slides down to the level, so the one animation the design actually cares
about is compositor-only and nothing lays out on an animation frame. It also
avoids the trap that made `scaleY` unusable: translation moves the gradient
without squashing it and without thickening the 1px waterline, which is the
one detail the whole design rests on. `.meter i` takes `scaleX` for the
adjacent reason — a solid fill with no gradient or border looks identical
scaled. `.water__pending` transitions **opacity**, not height: animating the
height of a hatch pattern is a layout animation for something that would
distort under `scaleY` anyway.

**The initial level is the CSS fallback, `0%`** — empty, because before the
first device scan the level is genuinely unknown. The markup carries no
`--used`; there are no inline styles in `index.html` at all, and the shell's
CSP is `style-src 'self'` with no `'unsafe-inline'`, so a style attribute
would be dropped even if one were added. `app.js` writes the level through
the CSSOM once the watch reports.

One correction worth keeping, because it was believed during the build and is
false: WKWebView **does** transition a property whose value comes from a custom
property. The readings that suggested otherwise were taken against a
backgrounded webview, where transitions freeze and computed values read stale.
Do not reintroduce that claim, and do not shape a rule around it.

Under `prefers-reduced-motion: reduce`, every animation and transition is
clamped to 1ms and the send gesture skips straight to the settled state. State
still changes; it just arrives rather than travels.

### Accessibility commitments
Every ratio below is measured against the ground the element actually renders
over, including composites. Where a composite could not be pinned down, the
element was given an opaque ground rather than a hopeful number.

- **`--ink-faint` is `#7d8c9b`: 5.28:1 on `--air`.** It was `#5a6875` = 3.18:1
  and failed AA on every count, size, duration and the connection status. This
  value is not a tone choice and must not be "tidied" darker. Where it ships,
  its worst real ground is the transfer card over bright album art (4.93:1);
  on the player's `--d4` it is 5.63:1 and on the rail gradient 4.98–5.28:1.
- **`--ink-faint` is banned from two grounds it used to sit on**, because on
  those composites it fails: the rail's item count (4.09:1 current, 4.44:1
  hover) and the watch wall's head and list, where the water's `--d1` stop
  rises behind them (4.36:1). Both took `--ink-dim` instead — 4.95:1 / 5.37:1
  in the rail, 5.27:1 over `--d1` — and no palette token changed.
- **Ink `#e3eaf2`** is 14.99:1 on `--air` and never below 12.38:1 anywhere in
  the app (its darkest-ratio ground is `--d1`, the lit surface of the water).
  **`--ink-dim`** is 6.38:1 on `--air` and 5.27:1 at its worst ground (`--d1`).
  **Brass** is 7.23:1 on `--air`; `--sun-ink` on brass is 7.56:1.
- **`--alert` is `#d9634f`.** It shipped as `#d0574a` = 4.45:1 on `--air`, just
  under AA. It now measures 5.06:1 on `--air` (the send-blocked reason),
  4.77:1 on the opaque `--notice-ground` (the failure headline), 4.74:1 on the
  pre-warning's `rgba(217,99,79,.07)` tint over `--air`, and 5.08:1 on the
  capacity readout's opaque `--d3`. Same hue; do not lower it back.
- **Two of those four grounds are opaque on purpose.** On the composites they
  replaced, `--alert` measured 4.18:1 — inside the translucent transfer card,
  and behind the capacity readout when `--d1` rose under it. A near-opaque
  transfer base was not enough on its own: at 94% over bright album art the
  headline still read 3.99:1. The ratio has to be a property of the rule.
- **`--line` (`#4a8cbe`) is 5.01:1 on `--air`** but is never used as text, and
  should not be. It is a 1px rule and a hatch.

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
- **Do** register an off-scale radius in `.impeccable/config.json` when an
  element is too small for the 6/10/14 scale, rather than letting it pass
  unremarked.
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
- **Don't** make the pending band a child of `.water`: its percentage height
  resolves against the water's own height, so the band shrinks as the watch
  empties. It is a fraction of total capacity, so it measures against the
  gauge.
- **Don't** put the water behind text. It owns the gauge and nothing else.
  Every level below 100% used to drag a 1px line and a bloom horizontally
  through the track rows.
- **Don't** animate `.water` by scaling or by compressing the gradient into a
  short box. It is full-height and translated; that is what keeps the waterline
  exactly 1px and the depth ramp truthful at low levels.
- **Don't** claim a contrast ratio against a parent's declared colour when the
  element renders over a composite. Measure the stack, or make the ground
  opaque.
- **Don't** put a translucent surface over content you do not control without
  a floor under the blur.
- **Don't** use `pointer-events: none` to disable a control; opacity plus a
  stated reason.
- **Don't** let a floating panel cover the capacity readout.
- **Don't** remove a navigation target at a breakpoint. Re-flow it.
- **Don't** invent placeholder numbers. Capacities, counts, sizes and formats
  are read from the device and the user's files; a fabricated value in this
  app is a lie about their hardware.
- **Don't** add anything that loops, pulses or breathes at rest.
