# Testing Notes

## Examples

All 15 live in `crates/pelican-core/examples/` and are self-contained binaries
you can run with `cargo run --example <name>` from the repo root.

**Three of them destroy data on the watch.** They are marked ☠ below and the
mark is the only thing in this table that distinguishes `diagnose` from
`wipe_music`. Read the source before running any of them against a watch that
holds music you care about.

| Example                | Purpose                                                                     |
|------------------------|-----------------------------------------------------------------------------|
| `usb_inspect`          | Dump USB descriptors / interface info for the connected Garmin              |
| `usb_reset`            | Issue a USB-level reset to the device (use when the session is wedged)      |
| `probe_platform`       | Report what the platform contention detector sees — gvfs on Linux, ptpcamerad and `UsbExclusiveOwner` on macOS |
| `diagnose`             | Open MTP session, list storage, walk top-level folders                      |
| `long_open`            | Repeatedly open + close the session — surfaces flaky enumeration            |
| `check_formats`        | Print the device's `playback_formats` and `capture_formats`                 |
| `dump_file`            | Download one object off the watch to local disk, byte-for-byte              |
| `probe_audiobooks`     | Probe the `Audiobooks/` folder behavior                                     |
| `probe_playlist`       | Try every playlist path style × format code and record which are rejected — the probe behind `playlists.md` |
| `probe_objprops`       | Ask the watch for object properties (`0x9801`–`0x9805`, `GetObjectPropValue`). The probe behind `garmin-mtp.md` "Object properties" — the evidence that the watch *can* report its own tags |
| `probe_vendor_ops`     | Sweep `0x9000-0x900B` + `0x9810/0x9811` with no params (5s timeout each). Wedges the session; replug afterwards |
| `claim_test`           | Diagnostic: open device + claim interface 0 directly via nusb               |
| ☠ `wipe_music`         | **Destructive.** Deletes every entry under `/Music`                         |
| ☠ `wipe_stubs`         | **Destructive.** Attempts a delete against every unreadable stub, counting refusals separately (they are the expected answer — `garmin-mtp.md` §6) |
| ☠ `test_delete`        | **Destructive.** Targeted single-file delete                                |

## Recovering from a wedged USB session

### macOS

Shorter ladder, because the usual macOS suspect turns out not to be one.

1. **Check who, if anyone, holds it.** `cargo run --example probe_platform`
   reads the IORegistry and names a real holder. Or by hand:
   `ioreg -p IOUSB -l -w 0 | grep -A 30 '"idVendor" = 2334'` and look for
   `UsbExclusiveOwner`. `ioreg -p IOUSB -l -w 0 | grep -c '"idVendor" = 2334'`
   returning 0 means the watch is not on the bus at all.
2. **It is not `ptpcamerad`.** Verified 2026-08-30 on FR165 / FW 2506: the
   watch presents `bDeviceClass = 0`, never matches the still-image class, and
   carries no `UsbExclusiveOwner`. Do not spend time on the `pkill` workaround
   from the mtp-rs README — `ptpcamerad` is SIP-protected, a same-user
   `killall` returns 0 while the process survives with its PID unchanged, and
   it was never holding the device anyway. See `macos-port.md`.
3. **There is no sysfs equivalent.** Nothing on macOS corresponds to the
   `usbfs` unbind below.
4. **Physically unplug + replug** — the terminal step on both platforms.

### Linux

Symptoms:
- `Error: Usb { kind: Busy, code: 16, message: "interface is busy" }`
  on `MtpDevice::open_first` even though no other process holds an fd
- `Error: Timeout` on session open after the busy state appears to clear

This commonly happens when an MTP probe is killed mid-session — Garmin
firmware leaves `OpenSession` in a broken state.

Recovery ladder (try in order):

1. **`cargo run --example usb_reset --release`** — issues a USB-level device
   reset. Often clears it.
2. **Close any GUI file manager that browses MTP** — COSMIC Files, Nautilus,
   etc. open the device on demand and can race with our claim.
3. **`echo 3-1:1.0 | sudo tee /sys/bus/usb/drivers/usbfs/unbind`** —
   substitute the actual interface address from
   `ls /sys/bus/usb/drivers/usbfs/` if it's not `3-1:1.0`. Forces the kernel
   to release a stale claim.
4. **Physically unplug + replug** — guaranteed to work, last resort.

To find the device's interface address:
```bash
for d in /sys/bus/usb/devices/*/idVendor; do
  v=$(cat "$d" 2>/dev/null)
  if [ "$v" = "091e" ]; then
    dir=$(dirname "$d")
    echo "device: $dir devnum=$(cat $dir/devnum)"
    for iface in "$dir"/*:*; do
      [ -d "$iface" ] && echo "  $(basename $iface) -> $(readlink $iface/driver | sed 's|.*/||')"
    done
  fi
done
```

## MTP session lifecycle gotchas

Two distinct ways to wedge the device's MTP session:

1. **Killing a probe mid-transfer** (Ctrl-C, SIGTERM) — `OpenSession` state
   becomes inconsistent. `usb_reset` usually clears this.
2. **Running `probe_vendor_ops`** to completion — sending unknown vendor
   opcodes confuses Garmin's responder. `usb_reset` is **not enough** on
   FR165 Music FW 2506; physical replug is required.

Plan accordingly: do all the work you need from a single MTP session
before running the vendor-op probe, and replug afterward.

## Buffered output trap

`cargo run --example foo 2>&1 | tail -40` will *buffer the entire stdout
stream until the process exits*. If the example produces line-by-line
progress, you won't see anything until the very end (or until you Ctrl-C and
get nothing).

For live progress:
- Drop the `| tail -40` and let the full output stream
- Or have the example write to a log file (`probe_vendor_ops.rs` does this:
  appends to `target/probe_vendor_ops.log`)
