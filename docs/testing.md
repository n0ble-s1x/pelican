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
| `probe_objprops`       | Ask the watch for object properties (`0x9801`-`0x9805`, `GetObjectPropValue`). The probe behind `garmin-mtp.md` "Object properties", the evidence that the watch *can* report its own tags |

## Recovering from a wedged USB session

Symptoms:
- `Error: Usb { kind: Busy, code: 16, message: "interface is busy" }`
  on `MtpDevice::open_first` even though no other process holds an fd
- `Error: Timeout` on session open after the busy state appears to clear

This commonly happens when an MTP probe is killed mid-session: Garmin
firmware leaves `OpenSession` in a broken state. It also happens after the
watch reboots, when it can enumerate as `091e:0003` ("Garmin GPS usb/tty
converter") and time out on every open.

Recovery ladder (try in order):

1. **Close any GUI file manager that browses MTP.** COSMIC Files, Nautilus
   and others open the device on demand and can race with Pelican's claim.
   Pelican warns when gvfs holds the watch and prints the `gio mount -u`
   command that releases it.
2. **`echo 3-1:1.0 | sudo tee /sys/bus/usb/drivers/usbfs/unbind`**:
   substitute the actual interface address from
   `ls /sys/bus/usb/drivers/usbfs/` if it is not `3-1:1.0`. This forces the
   kernel to release a stale claim.
3. **Unplug the watch, wait five seconds, and plug it back in.** This always
   works, and it is the only fix for a watch wedged after a reboot or by
   vendor opcodes.

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

1. **Killing a probe mid-transfer** (Ctrl-C, SIGTERM): `OpenSession` state
   becomes inconsistent.
2. **Sending unknown vendor opcodes** (the removed `probe_vendor_ops`
   example did this): Garmin's responder stops answering. A USB reset is
   **not enough** on FR165 Music FW 2506; a physical replug is required. Do
   not run such a probe again (`vendor-ops.md`).

## Buffered output trap

`cargo run --example foo 2>&1 | tail -40` will *buffer the entire stdout
stream until the process exits*. If the example produces line-by-line
progress, you won't see anything until the very end (or until you Ctrl-C and
get nothing).

For live progress:
- Drop the `| tail -40` and let the full output stream
- Or have the example write to a log file under `target/`
