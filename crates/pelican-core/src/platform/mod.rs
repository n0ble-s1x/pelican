//! Detecting the OS component that is holding the watch.
//!
//! Desktop Linux ships something that grabs MTP devices the moment they
//! enumerate, and while it holds the device our USB backend gets
//! `LIBUSB_ERROR_BUSY`. The underlying error tells the user nothing useful,
//! so the detector names the culprit and hands back the exact command that
//! frees the device.
//!
//! - **Linux** — `gvfs-mtp` auto-mounts the watch. The user owns the fix
//!   (`gio mount -u`), no privileges needed.
//! - Everything else — no detector. macOS is out of scope for the rebuild;
//!   its `ptpcamerad` detector lives in git history.

#[cfg(target_os = "linux")]
pub mod gvfs;

/// Something on this machine is holding the Garmin device.
#[derive(Debug, Clone)]
pub struct Contention {
    /// What is holding it, in words a user recognises.
    pub holder: String,
    /// Where we saw it — a mount path.
    pub detail: String,
    /// The exact command that releases the device. Any device-controlled
    /// text inside is already shell-quoted.
    pub remedy: String,
}

impl Contention {
    /// One-paragraph warning, printed by every device-touching command.
    pub fn message(&self) -> String {
        format!(
            "{} is holding your Garmin device ({}). This blocks direct USB access.\n\
             Free it with:\n  {}\n…then re-run Pelican.",
            self.holder, self.detail, self.remedy
        )
    }
}

/// Returns `Some` if something appears to be holding a Garmin device.
///
/// Cheap and side-effect free. Platforms without a known offender always
/// return `None`.
pub fn detect() -> Option<Contention> {
    #[cfg(target_os = "linux")]
    {
        gvfs::detect()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Advice for an open that failed with an exclusive-access error.
///
/// [`detect`] stays silent without hard evidence, so this is where the
/// plausible-but-unproven causes belong: they are only worth raising once
/// something has actually gone wrong.
pub fn explain_exclusive_access() -> String {
    let mut out = String::from("could not get exclusive access to the watch. Likely causes:");
    out.push_str("\n  1. another Pelican session is still open — the watch allows one at a time");
    if let Some(c) = detect() {
        out.push_str(&format!("\n  2. {} holds it: {}", c.holder, c.remedy));
    }
    out
}

/// Print the warning to stderr if the device is held. Used by the CLI
/// before it opens a session.
pub fn warn_if_holding_garmin() {
    if let Some(c) = detect() {
        eprintln!("warning: {}", c.message());
    }
}
