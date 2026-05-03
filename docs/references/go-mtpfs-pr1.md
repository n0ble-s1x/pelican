# Reference snapshot — go-mtpfs PR #1 (Garmin MTP fix)

URL: https://github.com/ganeshrvel/go-mtpfs/pull/1
Author: CodyJung

## Summary

Garmin watches deviate from the standard MTP responder behavior in two ways
that break naïve `libusb`-style MTP clients:

1. **Header / payload split across separate USB bulk transfers.**
   The 12-byte MTP container header (Length, Type, Code, TransactionID) is
   sent in one bulk packet, then the payload arrives in subsequent packets.
   Some MTP libraries assume header + payload share a single bulk transfer.

2. **Short data phases.** The header's `Length` field announces a transfer
   size, but Garmin sometimes sends fewer bytes than promised. Naïve readers
   block forever waiting for "missing" bytes that will never arrive. Fix:
   keep reading until the device closes the transfer (zero-length packet) or
   the device-promised length is reached, whichever comes first — and don't
   expect another container header inside an in-progress payload.

## How this affects Pelican

- We already address (1) via `mtp.session().set_split_header_data(true)` in
  `src/mtp.rs::MtpRsBackend::open`. Without it, mtp-rs's combined-bulk
  default hangs Garmin's responder. Documented in the project memory.
- (2) is mtp-rs's responsibility — we use mtp-rs's typed reader rather than
  raw bulk reads, and haven't observed short-data hangs in practice. If
  reads ever start hanging post-success, this is the bug class to inspect.
