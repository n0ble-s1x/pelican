//! Per-user cache and data directories, resolved per platform.
//!
//! Both resolvers **fail closed**: when no home-anchored location can be
//! determined they return `None` rather than falling back to a world-
//! writable directory. On a shared host `/tmp` is writable by every local
//! user and `/tmp/.local` does not normally exist, so any local user could
//! pre-create the chain and own our store: the ledger that decides which
//! names are free, or the staging dir whose files get uploaded. Losing
//! persistence is the strictly better failure.

use std::path::PathBuf;

/// Reverse-DNS identifier, used for the macOS directory names and as the
/// bundle identifier for a packaged app.
pub const BUNDLE_ID: &str = "com.krypteia.pelican";

/// Directory for short-lived transcode artifacts (see [`crate::staging`]).
///
/// Linux: `$XDG_CACHE_HOME/pelican`, falling back to `$HOME/.cache/pelican`.
/// macOS: `$HOME/Library/Caches/com.krypteia.pelican`.
pub fn cache_dir() -> Option<PathBuf> {
    ensure(cache_dir_unchecked()?)
}

/// Directory for the per-device ledger of every name ever written.
///
/// Linux: `$XDG_DATA_HOME/pelican`, falling back to `$HOME/.local/share/pelican`.
/// macOS: `$HOME/Library/Application Support/com.krypteia.pelican`.
pub fn data_dir() -> Option<PathBuf> {
    ensure(data_dir_unchecked()?)
}

fn ensure(p: PathBuf) -> Option<PathBuf> {
    std::fs::create_dir_all(&p).ok()?;
    Some(p)
}

/// `$HOME`, if it is an absolute path. A relative or empty `HOME` would
/// anchor the store to whatever directory the command was run from.
fn home() -> Option<PathBuf> {
    absolute(std::env::var_os("HOME"))
}

/// An environment value as a base directory, or `None` if it is unset,
/// empty or relative. The XDG Base Directory spec says an empty value is
/// to be treated as unset and a relative one is invalid and ignored; taken
/// verbatim either one resolves against the current directory, so a run
/// started from `~/Music` would open a different (empty) ledger than a
/// run started from `~`, and every name only that ledger knew would stop
/// being guarded.
fn absolute(v: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let p = PathBuf::from(v?);
    p.is_absolute().then_some(p)
}

#[cfg(target_os = "macos")]
fn cache_dir_unchecked() -> Option<PathBuf> {
    let mut p = home()?;
    p.push("Library");
    p.push("Caches");
    p.push(BUNDLE_ID);
    Some(p)
}

#[cfg(target_os = "macos")]
fn data_dir_unchecked() -> Option<PathBuf> {
    let mut p = home()?;
    p.push("Library");
    p.push("Application Support");
    p.push(BUNDLE_ID);
    Some(p)
}

#[cfg(not(target_os = "macos"))]
fn cache_dir_unchecked() -> Option<PathBuf> {
    let mut p = match absolute(std::env::var_os("XDG_CACHE_HOME")) {
        Some(x) => x,
        None => {
            let mut h = home()?;
            h.push(".cache");
            h
        }
    };
    p.push("pelican");
    Some(p)
}

#[cfg(not(target_os = "macos"))]
fn data_dir_unchecked() -> Option<PathBuf> {
    data_dir_from(std::env::var_os("XDG_DATA_HOME"), home(), in_flatpak())
}

/// The data directory from its inputs, so the Flatpak case can be tested.
///
/// Inside a Flatpak, `XDG_DATA_HOME` is the sandbox's private
/// `~/.var/app/<id>/data`. A ledger there would be a second ledger for the
/// same watch, blind to every name a source or AUR build had already used,
/// and its counter would start again at 1. So in a Flatpak the store is the
/// host's `~/.local/share/pelican`, which the manifest grants with
/// `--filesystem=xdg-data/pelican:create`: one ledger per watch, whichever
/// way Pelican was installed.
#[cfg(not(target_os = "macos"))]
fn data_dir_from(
    xdg_data_home: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
    flatpak: bool,
) -> Option<PathBuf> {
    let mut p = match (flatpak, absolute(xdg_data_home)) {
        (false, Some(x)) => x,
        _ => {
            let mut h = home?;
            h.push(".local");
            h.push("share");
            h
        }
    };
    p.push("pelican");
    Some(p)
}

/// True inside a Flatpak sandbox.
#[cfg(not(target_os = "macos"))]
pub fn in_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some_and(|v| !v.is_empty())
        || std::path::Path::new("/.flatpak-info").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_are_home_anchored_or_none() {
        // Whatever the platform, we never hand back a path outside the
        // user's own tree. That is the fail-closed promise the module
        // header makes, and the one an attacker would want to break.
        for d in [cache_dir_unchecked(), data_dir_unchecked()] {
            let Some(d) = d else { continue };
            let home = home().expect("HOME set in test env");
            assert!(d.starts_with(&home), "{d:?} escaped {home:?}");
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn a_flatpak_shares_the_hosts_data_dir() {
        let home = Some(PathBuf::from("/home/u"));
        let sandbox = Some("/home/u/.var/app/io.github.n0ble_s1x.Pelican/data".into());
        assert_eq!(
            data_dir_from(sandbox.clone(), home.clone(), true),
            Some(PathBuf::from("/home/u/.local/share/pelican")),
            "the sandbox's private XDG_DATA_HOME is not used"
        );
        assert_eq!(
            data_dir_from(sandbox, home.clone(), false),
            Some(PathBuf::from(
                "/home/u/.var/app/io.github.n0ble_s1x.Pelican/data/pelican"
            )),
            "outside a Flatpak, XDG_DATA_HOME wins as before"
        );
        assert_eq!(
            data_dir_from(None, home, false),
            Some(PathBuf::from("/home/u/.local/share/pelican"))
        );
        assert_eq!(data_dir_from(None, None, true), None, "still fails closed");
    }

    #[test]
    fn empty_or_relative_values_are_unset() {
        // Empty or relative XDG_DATA_HOME is ignored, so the ledger path does
        // not depend on the cwd.
        for v in ["", "data", "./data", "pelican", "~/.local/share"] {
            assert_eq!(absolute(Some(v.into())), None, "{v:?}");
        }
        assert_eq!(absolute(None), None);
        assert_eq!(
            absolute(Some("/home/user/.local/share".into())),
            Some(PathBuf::from("/home/user/.local/share"))
        );
    }

    #[test]
    fn cache_and_data_are_distinct() {
        if let (Some(c), Some(d)) = (cache_dir_unchecked(), data_dir_unchecked()) {
            assert_ne!(c, d, "a sweep of the cache would eat the ledger");
        }
    }
}
