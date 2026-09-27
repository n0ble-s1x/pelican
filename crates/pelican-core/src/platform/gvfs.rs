//! Linux: `gvfs-mtp` auto-mounts the watch.
//!
//! On most Linux desktops, plugging in a Garmin watch causes gvfs-mtp to
//! auto-mount it. While GVFS holds the device, USB-level MTP backends get
//! `LIBUSB_ERROR_BUSY`. We surface this clearly rather than letting the
//! underlying error confuse the user.
//!
//! The user can fix it with no privileges — `gio mount -u` is enough.

use std::fs;

use super::Contention;
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
/// device.
pub fn detect_garmin_gvfs_mount() -> Option<GvfsMount> {
    // SAFETY: geteuid is a side-effect-free POSIX syscall returning the
    // effective uid. The only reason it's `unsafe` in libc is FFI-by-default;
    // there's nothing to misuse. We localize the unsafe so the rest of the
    // crate keeps `unsafe_code = "deny"`.
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

pub fn detect() -> Option<Contention> {
    let mount = detect_garmin_gvfs_mount()?;
    // The URI is built here rather than in a shell substitution: the old
    // `basename | sed 's/^mtp://'` pipeline emitted `mtp://host=...`, which
    // gio rejects, and it interpolated device-controlled text unquoted.
    let uri = shell_quote(&format!("mtp://{}", mount.host));
    Some(Contention {
        holder: "GVFS".to_string(),
        detail: mount.path,
        remedy: format!("gio mount -u {uri}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The host string comes off USB descriptors the device controls.
    ///
    /// This test used to assert the output did not *contain* `'; rm`, and
    /// failed against correct code: the standard `'\''` idiom closes the
    /// quote, emits an escaped quote and reopens, so the characters `'; rm`
    /// do appear — inside a quoted word, where they are inert. The code was
    /// right and the substring check was wrong. What matters is what a shell
    /// makes of the result, so that is what is asserted now.
    #[test]
    fn shell_quote_neutralises_embedded_quotes() {
        let hostile = "a'; rm -rf ~; echo '";
        assert_eq!(shell_quote(hostile), r"'a'\''; rm -rf ~; echo '\'''");
    }

    /// Round-trip through a real `sh`: one argument in, the same bytes out,
    /// nothing executed.
    #[test]
    fn shell_quote_survives_a_real_shell() {
        for hostile in [
            "a'; rm -rf ~; echo '",
            "$(touch /nonexistent/pwned)",
            "`id`",
            r#"a"b\c"#,
            "091E_4CA1_'\n'x",
        ] {
            let script = format!("printf %s {}", shell_quote(hostile));
            let Ok(out) = std::process::Command::new("sh")
                .arg("-c")
                .arg(&script)
                .output()
            else {
                return; // no sh here; the exact-string test above still holds
            };
            assert!(out.status.success(), "{script}");
            assert_eq!(String::from_utf8_lossy(&out.stdout), hostile, "{script}");
        }
    }

    #[test]
    fn detect_does_not_panic_without_gvfs() {
        let _ = detect();
    }
}
