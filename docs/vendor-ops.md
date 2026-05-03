# Garmin Vendor MTP Opcodes

The watch advertises a block of vendor-defined MTP operation codes via
`GetDeviceInfo.OperationsSupported`. None of these are documented by Garmin
publicly, and none appear to be required for music sync (`better-sync` uploads
playlists using only standard ops). They likely back features Garmin Express
exposed — sync state, device-managed metadata, library indexing.

## Discovery

```bash
cargo run --example probe_vendor_ops --release
# logs to target/probe_vendor_ops.log
```

**⚠ Destructive side effect:** Sending unknown vendor opcodes wedges the
device's MTP session — every subsequent `MtpDevice::open_first` will time
out until the watch is **physically replugged** (USB reset alone is not
enough on FR165 Music FW 2506). Run the probe only when you intend to do
nothing else with the watch in the same session.

The probe issues each op with **no parameters** and a 5s timeout, captures:
- `OK` — responder returned a normal Response container
- `DATA` — responder started a data phase (op exists, returns data)
- `SHORT` — Response container too small (real response, unexpected shape)
- `TIMEOUT` — no reply within 5s (likely needs parameters)
- `ERR` — any other failure

## FR165 Music · FW 2506 · 2026-05-03

| Opcode    | Result                | Interpretation                                |
|-----------|-----------------------|-----------------------------------------------|
| `0x9000`  | DATA phase started    | Op exists; emits a payload. Probably a *get* of some device-state or list. |
| `0x9001`  | SHORT response (11 B) | Op exists; a Response container with truncated params (we expected ≥12 B for `Code+TxnID+SessionID`). Suggests the op completes but with a non-standard reply shape. |
| `0x9002`–`0x900B` | TIMEOUT       | Op listed as supported but did not reply with empty params. Likely needs operation-specific parameters; without them the responder waits indefinitely. |
| `0x9810`, `0x9811` | TIMEOUT      | Same — unreachable without correct params. These codes sit in the MS PropList range (`0x9800-0x98xx`); likely Garmin extensions of `GetObjectPropList`/`SetObjectPropList`. |

## Cross-references

- **libmtp bug #1779 (FR645 Music)** — only public dump of the Garmin vendor opcode set in the wild. FR645 advertises `0x9000-0x9006`; FR165 has the broader `0x9000-0x900B` + `0x9810/0x9811`.  https://sourceforge.net/p/libmtp/bugs/1779/
- **MTP standard `0x9800-0x98FF`** — Microsoft Property-List extensions. `0x9805 = GetObjectPropList`, `0x9806 = SetObjectPropList`.  Garmin's `0x9810/0x9811` likely customize these.
- **Android extension `0x95C1` = GetPartialObject64** — *not* in Garmin's set.
- **Garmin Express captured via Wireshark+USBPcap** — would be the most direct way to map these. No public capture has been shared yet.

## What we still don't know

- Whether Garmin Express actually uses any of these ops, or only standard MTP. (`better-sync` proves you can sync music + playlists with only standard ops, so vendor ops are not on the hot path for our v0.x.)
- The parameter signature for any op `0x9002`-`0x900B`. Probing each with combinations of 0–5 zero params would be the next step.
- Whether `0x9000` data-phase output is parseable into something meaningful.

## Future work

1. **Read `0x9000`'s data phase**: replace `session.execute(op, &[])` with a typed read, capture the bytes, hex-dump.
2. **Parameter sweep** for `0x9002`-`0x900B`: try `[0]`, `[storage_id]`, `[storage_id, 0xFFFFFFFF]`, etc. Wrap each in a 2-second timeout and stop at the first reply.
3. **Wireshark Garmin Express** on a Windows VM: capture a known operation (e.g. "sync new playlist") and diff against our standard-op trace.
4. **File a libmtp upstream entry** for FR165 Music (`0x091E:0x5151`) so their device table includes it.
