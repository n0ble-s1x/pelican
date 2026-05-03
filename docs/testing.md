# Testing Notes

## Examples

All `examples/*.rs` are self-contained binaries you can run with `cargo run --example <name>`.

| Example                | Purpose                                                                     |
|------------------------|-----------------------------------------------------------------------------|
| `usb_inspect`          | Dump USB descriptors / interface info for the connected Garmin              |
| `usb_reset`            | Issue a USB-level reset to the device (use when the session is wedged)      |
| `diagnose`             | Open MTP session, list storage, walk top-level folders                      |
| `long_open`            | Repeatedly open + close the session — surfaces flaky enumeration            |
| `check_formats`        | Print the device's `playback_formats` and `capture_formats`                 |
| `probe_audiobooks`     | Probe the `Audiobooks/` folder behavior                                     |
| `probe_vendor_ops`     | Sweep `0x9000-0x900B` + `0x9810/0x9811` with no params (5s timeout each)    |
| `wipe_music`           | Delete every entry under `/Music` (recovery from broken stubs)              |
| `test_delete`          | Targeted single-file delete                                                 |
| `claim_test`           | Diagnostic: open device + claim interface 0 directly via nusb               |

## Recovering from a wedged USB session

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
