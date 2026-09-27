# Proposal: a survivable probe of Garmin's vendor opcodes

> **STATUS: NOT RUN.** Nothing in this document has been executed. It is a
> written plan, deliberately kept in `docs/` rather than in `examples/` so
> that nobody can run it by tab-completing a `cargo run --example`. It should
> not be run on a watch whose music the owner cares about, and not without
> physical access to the cable.

## Why anyone would want to

`docs/garmin-mtp.md` §8 records a delete that succeeded at the MTP layer and
left the watch's music app still listing the tracks. Pelican has no operation
that touches that library, and standard MTP does not appear to expose one.
If a library-management operation exists at all, the undocumented block in
`docs/vendor-ops.md` — `0x9000`–`0x900B`, plus `0x9810`/`0x9811` in the MS
PropList range — is where it would be.

That is a real motive, and it is not a good enough reason to be careless.
The last probe cost a physical replug.

## What the last probe cost, and the likely reason

`examples/probe_vendor_ops.rs` sent each vendor op with **no parameters** and
a 5s timeout. On FR165 Music FW 2506 (2026-05-03):

| Opcode | Result |
|---|---|
| `0x9000` | DATA phase started |
| `0x9001` | SHORT response (11 bytes) |
| `0x9002`–`0x900B`, `0x9810`, `0x9811` | TIMEOUT |

After it, every `MtpDevice::open_first` timed out until the watch was
**physically unplugged and replugged** — a USB reset was not enough.

The mechanism is not proven, but the shape is familiar: a responder that
expects a data phase for an op it received with the wrong parameter count sits
waiting for bytes that never come. The host's 5s timeout abandons the
transaction; the device does not. Every later `OpenSession` then arrives
inside a transaction that never closed. **Each TIMEOUT is therefore probably
not a survivable event — it is the wedge.** A probe that sends eleven of them
in a row is eleven wedges deep before it prints anything.

That reading has one clear consequence for any redesign: **a probe must stop
at the first TIMEOUT.** The old one did not.

## Do the cheap things first

Two lines of enquiry carry no device risk at all and should be exhausted
before any vendor op is sent again.

1. **Settle §8's open question without the protocol.** Delete a file whose
   track the music app still lists, then try to *play* that track on the
   watch. If it fails, the app indexes in place and holds a dangling entry.
   If it plays, the watch kept its own copy. This is a five-minute test with
   a finger, it needs no code, and it decides which of the two models in §8
   is true. Everything below is worth less until it is done.
2. **Capture Garmin Express.** Wireshark + USBPcap on a Windows VM, sync one
   track, then remove it in Express and capture that. A capture answers what
   the ops *are* rather than what they tolerate, and it puts zero traffic of
   ours on the device. `docs/vendor-ops.md` already lists this as future work;
   it should be ranked above the probe, not beside it.

## The probe, if it is run anyway

### Preconditions

- The owner is present and the cable is reachable by hand.
- The watch's `/Music` holds nothing the owner would mind losing. Pelican can
  read files back (`Backend::download_file`), so a copy of `/Music` is
  possible — but §8 shows a delete can succeed at the MTP layer without the
  watch's library agreeing, so treat a restore as unproven too.
- Nothing else is going to want the watch for the next hour. On macOS,
  `ptpcamerad` grabs an MTP device on replug; see `docs/macos-port.md`.
- The log file is opened, written and **flushed line by line**. The old probe's
  value was nearly lost to a wedge holding unflushed output.

### Design rules

1. **One opcode per USB session, one session per physical connection.** Open,
   send exactly one op, close the session cleanly, exit the process. A
   wedged device cannot corrupt a result that was already on disk, and it
   cannot contaminate the next op's reading with its own broken state.
2. **Stop at the first TIMEOUT and say so.** Do not continue down the table.
   The next op's reading would be a reading of the wedge, not of the op.
3. **Read-shaped ops only, to begin with.** `0x9000` is the only vendor op
   with evidence that it completes and emits a payload. It is the whole of
   phase one.
4. **Never send an op whose effect could be a write, until a capture says
   what it does.** `0x9810`/`0x9811` sit where `SetObjectPropList` lives.
   An op that can clear a library can also clear the wrong one.
5. **Timeout short, 2s not 5s.** It does not change whether the device
   wedges; it changes how long a run takes to tell you that it did.
6. **Verify liveness after every op** with a `GetDeviceInfo` on a freshly
   opened session. That call is the wedge detector, and its result belongs in
   the log next to the op that preceded it.

### Phase 1 — read `0x9000`'s data phase

The one op already known to answer. Replace `session.execute(op, &[])` with a
typed read that captures the data phase, and hex-dump it to the log.

What to watch for, in order of what it would settle:

- **Filenames or track titles that the music app lists but `/Music` does not
  contain.** That is model (b) in §8 — the watch keeps its own copies — and it
  would be the single most valuable result available.
- A count that matches the music app's track count rather than `/Music`'s.
- Handles or object IDs. Cross-reference them against a `list_dir("Music")`
  taken in a separate session immediately before.
- A length-prefixed UTF-16LE run, which is how this responder returns strings
  (`docs/garmin-mtp.md`, object-properties section).

If the payload is opaque, that is a complete and publishable result. Stop.

### Phase 2 — a parameter sweep, only if phase 1 justified it

Only for the specific op a capture or a phase-1 payload gave a reason to
suspect. Not the whole block.

Sequence, one parameter set per physical connection:
`[0]`, `[storage_id]`, `[storage_id, 0xFFFFFFFF]`, `[0xFFFFFFFF]`.

Expect a replug between each. Budget the session in replugs, not minutes:
four parameter sets is four replugs, and that is the honest cost.

### Recording

Per attempt, one line, flushed: date, firmware, opcode, parameters, response
code (`0x2001` OK, `0x2005` etc.), data-phase length, and the post-op
`GetDeviceInfo` result. A run that wedges on the first op is still a result
worth keeping, and it is only worth keeping if it reached the disk.

### Recovery

Physical unplug and replug. A USB reset alone was not enough on FR165 FW
2506. If the device still does not answer, leave it on the charger and off
the cable for a few minutes before trying again — Garmin's responder is
restarted by the watch's own supervisor, not by the host.

Nothing in this repo has ever bricked a watch, and nothing here is expected
to. But the honest statement of risk is that we do not know what
`0x9002`–`0x900B` do, and an operation you cannot name is an operation whose
worst case you cannot bound. That is the reason for rule 4.

## What would make this unnecessary

A libmtp upstream entry for FR165 Music (`0x091E:0x5151`) and a shared
Garmin Express capture would let the next person start from a map instead of
from a wedge. Both are in `docs/vendor-ops.md` under future work.
