//! Per-user cache and data directories, resolved per platform.
//!
//! Both resolvers **fail closed**: when no home-anchored location can be
//! determined they return `None` rather than falling back to a world-
//! writable directory. On a shared host `/tmp` is writable by every local
//! user and `/tmp/.local` does not normally exist, so any local user could
//! pre-create the chain and own our store — the ledger that decides which
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
/// started from `~/Music` would open a different — empty — ledger than a
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
    let mut p = match absolute(std::env::var_os("XDG_DATA_HOME")) {
        Some(x) => x,
        None => {
            let mut h = home()?;
            h.push(".local");
            h.push("share");
            h
        }
    };
    p.push("pelican");
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_are_home_anchored_or_none() {
        // Whatever the platform, we never hand back a path outside the
        // user's own tree — that is the fail-closed promise the module
        // header makes, and the one an attacker would want to break.
        for d in [cache_dir_unchecked(), data_dir_unchecked()] {
            let Some(d) = d else { continue };
            let home = home().expect("HOME set in test env");
            assert!(d.starts_with(&home), "{d:?} escaped {home:?}");
        }
    }

    #[test]
    fn empty_or_relative_values_are_unset() {
        // `XDG_DATA_HOME=""` and `XDG_DATA_HOME=data` both used to become
        // a path relative to the cwd; the ledger has to be the same file
        // wherever the command is run from.
        for v in ["", "data", "./data", "pelican", "~/.local/share"] {
            assert_eq!(absolute(Some(v.into())), None, "{v:?}");
        }
        assert_eq!(absolute(None), None);
        assert_eq!(
            absolute(Some("/home/six/.local/share".into())),
            Some(PathBuf::from("/home/six/.local/share"))
        );
    }

    #[test]
    fn cache_and_data_are_distinct() {
        if let (Some(c), Some(d)) = (cache_dir_unchecked(), data_dir_unchecked()) {
            assert_ne!(c, d, "a sweep of the cache would eat the ledger");
        }
    }
}
