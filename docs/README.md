# Pelican docs

Reference notes on the current build and on Garmin's USB/MTP behavior, so
findings from hours of work against real hardware do not have to be
rediscovered.

| File | Contents |
|---|---|
| [`status.md`](status.md) | What the current build does and does not do, what was verified on hardware, and the test counts. Read this first. |
| [`rebuild-plan.md`](rebuild-plan.md) | The requirements (R1-R12) the core is held to. |
| [`garmin-mtp.md`](garmin-mtp.md) | Protocol reference: IDs, format codes, folder layout, firmware quirks. |
| [`garmin-library-persistence.md`](garmin-library-persistence.md) | Why a deleted track stays in the watch's library, what the community has established, and the experiments that settled it. |
| [`playlists.md`](playlists.md) | Why MTP playlist writes fail on the FR165. |
| [`testing.md`](testing.md) | The read-only examples, and how to recover a watch that stops answering. |
| [`vendor-ops.md`](vendor-ops.md) | What is known about Garmin's vendor MTP opcodes (`0x9000-0x900B`, `0x9810`, `0x9811`). |
| [`references/`](references/) | Snapshots of external code and threads Pelican relied on, kept in case the links rot. |
| [`archive/`](archive/) | Historical notes, including the macOS port. They do not describe the current code. |

## Conventions

- Every protocol claim says what was verified, on which device, on which
  firmware. Garmin's behavior changes across models and firmware versions.
- Prefer code snippets and hex dumps over prose for wire-level details.
- When a finding contradicts an earlier one, update the text in place and
  note the correction. Do not leave stale advice standing.
