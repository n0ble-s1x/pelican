//! The one environment fix Pelican makes to itself before the webview
//! starts.
//!
//! On Wayland with the NVIDIA proprietary driver, WebKitGTK dies at launch
//! with "Error 71 (Protocol error) dispatching to Wayland display" unless
//! explicit sync is turned off (verified on the maintainer's machine;
//! Tauri documents the same workaround:
//! <https://v2.tauri.app/develop/debug/linux-graphics/>). So Pelican sets
//! `__NV_DISABLE_EXPLICIT_SYNC=1` for its own process: only on Wayland,
//! only with the `nvidia` kernel module loaded, and never over a value the
//! user set themselves, whatever it is.

use std::ffi::OsStr;
use std::path::Path;

pub const VAR: &str = "__NV_DISABLE_EXPLICIT_SYNC";

/// What the decision looks at, read from the process and `/sys`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Seen<'a> {
    pub session_type: Option<&'a OsStr>,
    pub wayland_display: Option<&'a OsStr>,
    pub nvidia_loaded: bool,
    /// The variable's current value; set at all (even empty) means the
    /// user chose.
    pub current: Option<&'a OsStr>,
}

/// Whether to set [`VAR`]`=1`. Pure.
pub fn needs_workaround(s: Seen<'_>) -> bool {
    let nonempty = |v: Option<&OsStr>| v.is_some_and(|v| !v.is_empty());
    let wayland = s
        .session_type
        .is_some_and(|t| t.eq_ignore_ascii_case("wayland"))
        || nonempty(s.wayland_display);
    wayland && s.nvidia_loaded && s.current.is_none()
}

/// Read the machine and apply the decision. Called first thing in `main`,
/// while the process has one thread, so setting the variable races nothing.
pub fn apply_nvidia_wayland_workaround() {
    let session = std::env::var_os("XDG_SESSION_TYPE");
    let display = std::env::var_os("WAYLAND_DISPLAY");
    let current = std::env::var_os(VAR);
    let seen = Seen {
        session_type: session.as_deref(),
        wayland_display: display.as_deref(),
        nvidia_loaded: Path::new("/sys/module/nvidia").exists(),
        current: current.as_deref(),
    };
    if needs_workaround(seen) {
        std::env::set_var(VAR, "1");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(s: &str) -> Option<&OsStr> {
        Some(OsStr::new(s))
    }

    #[test]
    fn wayland_with_nvidia_and_nothing_set_gets_the_workaround() {
        let s = Seen {
            session_type: os("wayland"),
            nvidia_loaded: true,
            ..Seen::default()
        };
        assert!(needs_workaround(s));
        // WAYLAND_DISPLAY alone is Wayland too (no logind session type).
        let s = Seen {
            wayland_display: os("wayland-0"),
            nvidia_loaded: true,
            ..Seen::default()
        };
        assert!(needs_workaround(s));
    }

    #[test]
    fn a_value_the_user_set_is_never_overridden() {
        for v in ["0", "1", ""] {
            let s = Seen {
                session_type: os("wayland"),
                wayland_display: os("wayland-0"),
                nvidia_loaded: true,
                current: os(v),
            };
            assert!(!needs_workaround(s), "overrode {v:?}");
        }
    }

    #[test]
    fn x11_or_no_nvidia_is_left_alone() {
        let x11 = Seen {
            session_type: os("x11"),
            nvidia_loaded: true,
            ..Seen::default()
        };
        assert!(!needs_workaround(x11));
        let empty_display = Seen {
            session_type: os("tty"),
            wayland_display: os(""),
            nvidia_loaded: true,
            ..Seen::default()
        };
        assert!(!needs_workaround(empty_display));
        let amd = Seen {
            session_type: os("wayland"),
            wayland_display: os("wayland-0"),
            nvidia_loaded: false,
            ..Seen::default()
        };
        assert!(!needs_workaround(amd));
    }
}
