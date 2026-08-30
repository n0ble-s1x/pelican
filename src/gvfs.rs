//! GVFS-MTP detection.
//!
//! On most Linux desktops, plugging in a Garmin watch causes gvfs-mtp to
//! auto-mount it. While GVFS holds the device, libusb-based MTP backends
//! (mtp-rs, libmtp) get LIBUSB_ERROR_BUSY. We surface this clearly rather
//! than letting the underlying error confuse the user.

use std::fs;

use anyhow::Result;

use crate::garmin::GARMIN_VENDOR_ID;

/// A gvfs-mtp mount that appears to belong to a Garmin device.
pub struct GvfsMount {
    /// Full mount directory, e.g. `/run/user/1000/gvfs/mtp:host=091E_4CA1_0123456789`.
    pub path: String,
    /// Device id lifted out of the directory name, e.g. `091E_4CA1_0123456789`.
    /// This comes from USB descriptors the device controls, so it is untrusted:
    /// quote it before it goes anywhere near a shell.
    pub host: String,
}

/// Wrap an untrusted string in single quotes for safe shell pasting.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Returns Some(mount) if a gvfs MTP mount appears to belong to a Garmin
/// device. The caller should warn the user and offer to `gio mount -u` it.
pub fn detect_garmin_gvfs_mount() -> Option<GvfsMount> {
    // SAFETY: geteuid is a side-effect-free POSIX syscall returning the
    // effective uid. The only reason it's `unsafe` in libc is FFI-by-default;
    // there's nothing to misuse. We localize the unsafe so the rest of the
    // crate keeps `unsafe_code = "forbid"`.
    #[allow(unsafe_code)]
    let uid = unsafe { libc::geteuid() };
    let base = format!("/run/user/{uid}/gvfs");
    let entries = fs::read_dir(&base).ok()?;
    let needle = "mtp:host=".to_string();
    let vendor = format!("{GARMIN_VENDOR_ID:04X}");
    for ent in entries.flatten() {
        let name = ent.file_name().to_string_lossy().to_string();
        // gvfs-mtp encodes the device URL into the directory name.
        // Match `mtp:host=` plus a Garmin vendor hint (`091E` or `091e`).
        if name.starts_with(&needle)
            && (name.contains(&vendor) || name.contains(&vendor.to_lowercase()))
        {
            // Strip the whole `mtp:host=` prefix — leaving the `host=` behind
            // produces `mtp://host=...`, which gio does not accept.
            let host = name.strip_prefix(&needle).unwrap_or(&name).to_string();
            return Some(GvfsMount {
                path: format!("{base}/{name}"),
                host,
            });
        }
    }
    None
}

pub fn warn_if_holding_garmin() -> Result<()> {
    if let Some(mount) = detect_garmin_gvfs_mount() {
        // The URI is built here rather than in a shell substitution: the old
        // `basename | sed 's/^mtp://'` pipeline emitted `mtp://host=...`, which
        // gio rejects, and it interpolated device-controlled text unquoted.
        let uri = shell_quote(&format!("mtp://{}", mount.host));
        let path = &mount.path;
        eprintln!(
            "warning: GVFS appears to have mounted your Garmin device at:\n  {path}\n\
             This will block direct USB access. Unmount with:\n  gio mount -u {uri}\n\
             …then re-run garmin-music."
        );
    }
    Ok(())
}
