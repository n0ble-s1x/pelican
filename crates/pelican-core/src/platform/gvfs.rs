//! Linux: `gvfs-mtp` auto-mounts the watch.
//!
//! On most Linux desktops, plugging in a Garmin watch causes gvfs-mtp to
//! auto-mount it. While GVFS holds the device, USB-level MTP backends get
//! `LIBUSB_ERROR_BUSY`. We surface this clearly rather than letting the
//! underlying error confuse the user.
//!
//! The user can fix it with no privileges: `gio mount -u` is enough.

use std::fs;

use super::Contention;
use crate::garmin::GARMIN_VENDOR_ID;

/// A gvfs-mtp mount that appears to belong to a Garmin device.
pub struct GvfsMount {
    /// Full mount directory, e.g. `/run/user/1000/gvfs/mtp:host=091E_4CA1_0123456789`.
    pub path: String,
    /// Device id lifted out of the directory name, e.g. `091E_4CA1_0123456789`.
    /// This comes from USB descriptors the device controls, so it is untrusted
    /// and goes into a pasteable command only through `remedy`.
    pub host: String,
}

/// The characters udev allows in `ID_SERIAL`, which gvfs builds the host from.
fn is_plain_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.:@=+#-".contains(c))
}

/// The command that frees the watch, safe to paste into sh, bash, zsh or fish.
///
/// Quoting rules differ between shells (fish treats `\'` inside single quotes
/// as an escape), so a host outside udev's character set is never put in the
/// command. With the allowlist, the single-quoted word has no quote or
/// backslash in it and means the same thing in every shell.
fn remedy(host: &str) -> String {
    if is_plain_host(host) {
        format!("gio mount -u 'mtp://{host}'")
    } else {
        "gio mount -l (find the mtp:// address of the watch), then gio mount -u <that address>"
            .to_string()
    }
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
            // Strip the whole `mtp:host=` prefix: gio does not accept
            // `mtp://host=...`.
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
    Some(Contention {
        holder: "GVFS".to_string(),
        remedy: remedy(&mount.host),
        detail: mount.path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTILE: [&str; 6] = [
        "a'; rm -rf ~; echo '",
        "$(touch /nonexistent/pwned)",
        "`id`",
        r#"a"b\c"#,
        "091E_4CA1_'\n'x",
        // Breaks out of POSIX-style quoting under fish.
        r"091E_x\' ; echo PWNED ; echo \",
    ];

    #[test]
    fn a_plain_host_is_named_in_the_command() {
        assert_eq!(
            remedy("091E_4CA1_0123456789"),
            "gio mount -u 'mtp://091E_4CA1_0123456789'"
        );
    }

    #[test]
    fn a_host_outside_udevs_set_never_reaches_the_command() {
        for hostile in HOSTILE {
            let r = remedy(hostile);
            assert!(!r.contains(hostile), "{r}");
            assert!(!r.contains('\''), "{r}");
        }
    }

    /// Every shell on the machine reads the command as `gio`, `mount`, `-u`
    /// and one URI argument.
    #[test]
    fn the_remedy_is_one_argument_in_every_shell() {
        let host = "091E_4CA1_a.b:c@d=e+f#g-h";
        let uri = format!("mtp://{host}");
        let cmd = remedy(host).replacen("gio mount -u", "printf %s", 1);
        for shell in ["sh", "bash", "zsh", "fish"] {
            let Ok(out) = std::process::Command::new(shell)
                .arg("-c")
                .arg(&cmd)
                .output()
            else {
                continue;
            };
            assert!(out.status.success(), "{shell}: {cmd}");
            assert_eq!(String::from_utf8_lossy(&out.stdout), uri, "{shell}");
        }
    }

    #[test]
    fn detect_does_not_panic_without_gvfs() {
        let _ = detect();
    }
}
