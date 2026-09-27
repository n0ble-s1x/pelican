# Testing Notes

## Examples

Five live in `crates/pelican-core/examples/`, all read-only, run with
`cargo run --example <name>` from the repo root. The rest were removed in the
rebuild; git history has them.

| Example                | Purpose                                                                     |
|------------------------|-----------------------------------------------------------------------------|
| `usb_inspect`          | Dump USB descriptors / interface info for the connected Garmin              |
| `diagnose`             | Open MTP session, list storage, walk top-level folders                      |
| `dump_file`            | Download one object off the watch and hex-dump its bytes                    |
| `verify_roundtrip`     | Download every `/Music` object and compare listed size with bytes received; optionally content against a local file |
| `probe_objprops`       | Ask the watch for object properties (`0x9801`–`0x9805`, `GetObjectPropValue`). The probe behind `garmin-mtp.md` "Object properties" — the evidence that the watch *can* report its own tags |

## Recovering from a wedged USB session

### macOS

Shorter ladder, because the usual macOS suspect turns out not to be one.

1. **Check who, if anyone, holds it.** By hand:
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

1. **A USB-level device reset.** Often clears it. The `usb_reset` example
   that did this was removed in the rebuild; recover it from git history
   (`git show d2e6608^:crates/pelican-core/examples/usb_reset.rs`).
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
   becomes inconsistent. A USB reset usually clears this.
2. **Running the (removed) `probe_vendor_ops`** to completion — sending unknown vendor
   opcodes confuses Garmin's responder. A USB reset is **not enough** on
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
- Or have the example write to a log file under `target/`
