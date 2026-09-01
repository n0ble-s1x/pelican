---
version: 1
slug: "ui-index-html"
primary_target: "ui/index.html"
related_targets: []
---

## Scope

The Pelican desktop main window — the only surface. A local music player that
also syncs to a Garmin watch over USB.

## Visitor mode

Operate. The visitor is completing a task: find music, audition it, and get a
chosen set onto a watch that holds ~3.7 GB.

## Audience and job

Public open-source product; the design target is a stranger on first run, not
the maintainer. No technical literacy assumed — MTP, tags and transcoding must
never surface as concepts. The job: browse a library on local disk or a NAS,
play tracks to decide, build playlists, push a set to the watch, and take
things off to make room.

## Chosen direction

**The Fjord.** Cold blue-black ground drawn from fjord depth; an absolute
waterline as the only structural rule; one warm value — brass, low sun on
water — reserved for the active state and the primary action. Scandinavian
discipline means restraint and craft in the joinery, never applied ornament:
no runes, no knotwork, no longship silhouettes.

Depth is the single spatial idea. Surfaces sit at a stated depth (`--air`
above the line, `--d1`..`--d4` below); deeper is colder, darker, denser. The
product maps onto it exactly: the library is open air, the watch is under
water, and capacity is how high the water has risen.

## Composition — approved comp F, with G's now-playing

`.impeccable/mocks/comp-f.html` is the approved comp.

Three columns: a sources and playlists rail; an album channel carrying a full
hero (art at real size, HiFi metadata beside it) above the track list; and the
watch as the far wall, whose water level *is* the capacity gauge. A player bar
runs the full width at the bottom. G's now-playing treatment — large art, big
title, HiFi badge — folds into the channel's hero.

## Memorable moment

The waterline. One value, `#4a8cbe`, with a soft bloom, and the strongest value
change anywhere in the app — because capacity is the fact that governs every
decision the user makes. E's downward-send gesture is kept as a behaviour: a
selection travels down into the water.

## Constraints this surface must respect

- One MTP session at a time; a fresh session per file. Transfers are serial,
  slow, and cannot be parallelised. The UI must stay responsive and honest
  about queue position.
- Files without title+artist tags transfer but are invisible on the watch.
  This must be surfaced before sending, in words, not as a colour.
- The watch does not expose its indexed library; the local journal is the only
  record of what Pelican put there.
- Failure is routine — firmware rejections, orphaned stubs, pulled cables.
  Every failure must be legible and fixable without a terminal.

## Unresolved

- **Playlists on the device.** The FR165 rejects MTP playlist writes;
  `better-sync` reports success on FR945/FR255/Venu. Playlists are real in the
  app regardless; whether they also land on the watch needs a hardware probe.
  The UI must not imply the watch has playlists until that is settled.
- **Bit-perfect playback.** Webview audio goes through CoreAudio's mixer and
  may resample. The comp's "bit-perfect" badge is not yet a claim we can make.
- **Network sources (Navidrome).** Out of scope. It would break the product's
  stated no-network promise and needs an explicit opt-in and a rewritten claim
  if ever taken up.
