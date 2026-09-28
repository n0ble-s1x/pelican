//! What kind of trouble the watch is in, in words a person can act on.
//!
//! Every device error reaches the user through one of these kinds, so a
//! front-end can show the fix without anyone reading a log. The one that
//! matters most is [`DeviceErrorKind::Wedged`]: after the watch reboots it
//! can come back half-alive (on the Forerunner 165 it briefly enumerates
//! as `091e:0003`, "Garmin GPS usb/tty converter"), and every MTP open or
//! transfer then times out (30 s in mtp-rs) until it is physically
//! re-plugged. Nothing in software brings it back, so the message says
//! exactly what to do with your hands.

use serde::Serialize;

/// The instruction for a wedged watch, word for word.
pub const REPLUG: &str =
    "The watch isn't answering. Unplug it, wait five seconds, plug it back in.";

/// The kinds a front-end tells apart. Serialized as the IPC's
/// `error_kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceErrorKind {
    /// No Garmin watch on USB.
    NotFound,
    /// The device node exists but this user may not open it (no udev rule).
    Permission,
    /// Something else holds the watch: another Pelican session, or a
    /// process this platform has no detector for.
    Busy,
    /// gvfs-mtp holds the watch; [`crate::platform::detect`] has the fix.
    Gvfs,
    /// An MTP open or transfer timed out: the watch stopped answering.
    /// Only a replug clears it; see [`REPLUG`].
    Wedged,
    Other,
}

/// Marker attached to an error when the watch stopped answering. Its
/// `Display` is [`REPLUG`], so the instruction is in the message whatever
/// else prints it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wedged;

impl std::fmt::Display for Wedged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(REPLUG)
    }
}

impl std::error::Error for Wedged {}

/// Sort an error into a [`DeviceErrorKind`]. Pure: it looks only at the
/// error chain, never at the machine. A [`DeviceErrorKind::Busy`] with gvfs
/// present is [`DeviceErrorKind::Gvfs`]; that takes a look at the machine,
/// so it is [`classify_with`]'s job.
pub fn classify(err: &anyhow::Error) -> DeviceErrorKind {
    for cause in err.chain() {
        if cause.downcast_ref::<Wedged>().is_some() {
            return DeviceErrorKind::Wedged;
        }
        #[cfg(feature = "mtp-backend")]
        if let Some(e) = cause.downcast_ref::<mtp::Error>() {
            match e {
                mtp::Error::Timeout => return DeviceErrorKind::Wedged,
                mtp::Error::NoDevice | mtp::Error::Disconnected => {
                    return DeviceErrorKind::NotFound
                }
                _ => {}
            }
        }
        if let Some(e) = cause.downcast_ref::<std::io::Error>() {
            match e.kind() {
                std::io::ErrorKind::TimedOut => return DeviceErrorKind::Wedged,
                std::io::ErrorKind::PermissionDenied => return DeviceErrorKind::Permission,
                _ => {}
            }
        }
    }
    classify_text(&format!("{err:#}"))
}

/// [`classify`], with contention on this machine taken into account.
pub fn classify_with(err: &anyhow::Error, gvfs_holds_it: bool) -> DeviceErrorKind {
    match classify(err) {
        DeviceErrorKind::Busy if gvfs_holds_it => DeviceErrorKind::Gvfs,
        k => k,
    }
}

/// The message-text fallback: the concrete error types differ per
/// platform and per layer (nusb, mtp-rs, anyhow context), and some arrive
/// only as text. The order matters: a timeout is checked first because
/// "timed out" is the one a person can fix with their hands.
fn classify_text(msg: &str) -> DeviceErrorKind {
    let m = msg.to_lowercase();
    if m.contains("timed out") || m.contains("timeout") || m.contains(&REPLUG.to_lowercase()) {
        DeviceErrorKind::Wedged
    } else if m.contains("permission denied")
        || m.contains("eacces")
        || m.contains("access is denied")
        || m.contains("operation not permitted")
    {
        DeviceErrorKind::Permission
    } else if m.contains("exclusive access") || m.contains("busy") {
        DeviceErrorKind::Busy
    } else if m.contains("no garmin device")
        || m.contains("no mtp device")
        || m.contains("device disconnected")
        || m.contains("no such device")
    {
        DeviceErrorKind::NotFound
    } else {
        DeviceErrorKind::Other
    }
}

/// True when the error means the watch stopped answering.
pub fn is_wedged(err: &anyhow::Error) -> bool {
    classify(err) == DeviceErrorKind::Wedged
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn a_timeout_anywhere_in_the_chain_is_a_wedge() {
        let io = std::io::Error::new(std::io::ErrorKind::TimedOut, "bulk in");
        let e = anyhow::Error::new(io).context("opening MTP session to Forerunner");
        assert_eq!(classify(&e), DeviceErrorKind::Wedged);
        let e = anyhow!("Operation timed out").context("listing Music");
        assert_eq!(classify(&e), DeviceErrorKind::Wedged);
        let e = anyhow!("whatever").context(Wedged);
        assert_eq!(classify(&e), DeviceErrorKind::Wedged);
        assert!(format!("{e:#}").contains(REPLUG));
        assert!(is_wedged(&e));
    }

    #[cfg(feature = "mtp-backend")]
    #[test]
    fn mtp_rs_errors_are_sorted_by_type() {
        let e = anyhow::Error::new(mtp::Error::Timeout).context("opening");
        assert_eq!(classify(&e), DeviceErrorKind::Wedged);
        let e = anyhow::Error::new(mtp::Error::NoDevice);
        assert_eq!(classify(&e), DeviceErrorKind::NotFound);
    }

    #[test]
    fn permission_busy_and_not_found() {
        let io = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            classify(&anyhow::Error::new(io)),
            DeviceErrorKind::Permission
        );
        assert_eq!(
            classify(&anyhow!("USB error: Permission denied (os error 13)")),
            DeviceErrorKind::Permission
        );
        let busy = anyhow!("Device or resource busy (os error 16)")
            .context("could not get exclusive access to the watch");
        assert_eq!(classify(&busy), DeviceErrorKind::Busy);
        assert_eq!(classify_with(&busy, true), DeviceErrorKind::Gvfs);
        assert_eq!(classify_with(&busy, false), DeviceErrorKind::Busy);
        assert_eq!(
            classify(&anyhow!(
                "no Garmin device found on USB (vendor 0x091e). Plug in your watch"
            )),
            DeviceErrorKind::NotFound
        );
        assert_eq!(classify(&anyhow!("disk full")), DeviceErrorKind::Other);
        // gvfs never turns a wedge into something else.
        assert_eq!(
            classify_with(&anyhow!("Operation timed out"), true),
            DeviceErrorKind::Wedged
        );
    }

    #[test]
    fn the_instruction_is_the_spec_wording() {
        assert_eq!(
            Wedged.to_string(),
            "The watch isn't answering. Unplug it, wait five seconds, plug it back in."
        );
        assert_eq!(
            serde_json::to_string(&DeviceErrorKind::NotFound).unwrap(),
            r#""not_found""#
        );
    }
}
