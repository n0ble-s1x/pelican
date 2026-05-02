//! GVFS-MTP detection.
//!
//! On most Linux desktops, plugging in a Garmin watch causes gvfs-mtp to
//! auto-mount it. While GVFS holds the device, libusb-based MTP backends
//! (mtp-rs, libmtp) get LIBUSB_ERROR_BUSY. We surface this clearly rather
//! than letting the underlying error confuse the user.

use std::fs;

use anyhow::Result;

use crate::garmin::GARMIN_VENDOR_ID;

/// Returns Some(path) if a gvfs MTP mount appears to belong to a Garmin
/// device. The caller should warn the user and offer to `gio mount -u` it.
pub fn detect_garmin_gvfs_mount() -> Option<String> {
    let uid = unsafe { libc::geteuid() };
    let base = format!("/run/user/{uid}/gvfs");
    let entries = fs::read_dir(&base).ok()?;
    let needle = format!("mtp:host=");
    let vendor = format!("{GARMIN_VENDOR_ID:04X}");
    for ent in entries.flatten() {
        let name = ent.file_name().to_string_lossy().to_string();
        // gvfs-mtp encodes the device URL into the directory name.
        // Match `mtp:host=` plus a Garmin vendor hint (`091E` or `091e`).
        if name.starts_with(&needle)
            && (name.contains(&vendor) || name.contains(&vendor.to_lowercase()))
        {
            return Some(format!("{base}/{name}"));
        }
    }
    None
}

pub fn warn_if_holding_garmin() -> Result<()> {
    if let Some(path) = detect_garmin_gvfs_mount() {
        eprintln!(
            "warning: GVFS appears to have mounted your Garmin device at:\n  {path}\n\
             This will block direct USB access. Unmount with:\n  gio mount -u \"mtp://$(basename '{path}' | sed 's/^mtp://')\"\n\
             …then re-run garmin-music."
        );
    }
    Ok(())
}

// Tiny libc shim so we don't pull the libc crate just for geteuid.
mod libc {
    extern "C" {
        pub fn geteuid() -> u32;
    }
}
