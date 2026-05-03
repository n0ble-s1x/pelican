# Pelican — Protocol & Reverse-Engineering Notes

Working notes on Garmin's USB/MTP protocol as it applies to music-capable
watches. This directory exists so hard-won findings (often from hours of
trial-and-error against real hardware) don't have to be re-discovered.

## Layout

| File                                          | Contents                                                                                  |
|-----------------------------------------------|-------------------------------------------------------------------------------------------|
| [`status.md`](status.md)                      | What works, what doesn't, what's blocked. Read this first.                                |
| [`garmin-mtp.md`](garmin-mtp.md)              | Definitive protocol reference — IDs, format codes, folder layout, firmware quirks         |
| [`vendor-ops.md`](vendor-ops.md)              | What we know about Garmin's vendor MTP opcodes (0x9000-0x900B + 0x9810/0x9811)            |
| [`playlists.md`](playlists.md)                | Playlist sync recipe + 2026-05-03 FR165 probe results (all variants rejected)             |
| [`testing.md`](testing.md)                    | How to run probes, what to expect, recovery from a wedged USB session                     |
| [`audit-2026-05-03.md`](audit-2026-05-03.md)  | Code audit — fixes shipped, follow-ups, things deliberately left alone                    |
| [`research-log.md`](research-log.md)          | Dated entries — links followed, references compared, decisions made                       |
| [`references/`](references/)                  | Snapshots of the external code/threads we relied on (so links rotting doesn't kill us)    |

## Conventions

- Every protocol claim should cite **what was verified, on what device, on what firmware** — Garmin's behavior shifts across models and FW versions.
- Prefer code snippets / hex dumps over prose when describing wire-level details.
- When a finding contradicts a previous one, *update in place and add a footnote* — don't leave stale advice in place.
