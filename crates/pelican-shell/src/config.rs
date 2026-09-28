//! The window's config file, which names the music library. The window
//! reads it and never writes it.
//!
//! `$XDG_CONFIG_HOME/pelican/config.json`, else `~/.config/pelican/`. The
//! same fail-closed rule as `pelican_core::paths`: an unset, empty or
//! relative base is ignored rather than resolved against the working
//! directory, and with no absolute `HOME` there is no config file at all.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// `None` means `~/Music`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_root: Option<PathBuf>,
}

impl Config {
    /// The library to open: the configured one, else `~/Music`. The
    /// default is returned whether or not it exists; browsing it says so.
    pub fn library_root_or(&self, home: Option<&Path>) -> Option<PathBuf> {
        self.library_root
            .clone()
            .or_else(|| home.map(|h| h.join("Music")))
    }
}

/// The config file, from the two environment values it depends on.
pub fn file_from(xdg_config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = match absolute(xdg_config_home) {
        Some(x) => x,
        None => absolute(home)?.join(".config"),
    };
    Some(base.join("pelican").join("config.json"))
}

pub fn file() -> Option<PathBuf> {
    file_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

pub fn home() -> Option<PathBuf> {
    absolute(std::env::var_os("HOME"))
}

fn absolute(v: Option<OsString>) -> Option<PathBuf> {
    let p = PathBuf::from(v?);
    p.is_absolute().then_some(p)
}

/// Never fails: a missing, unreadable or corrupt file is the first-launch
/// state, and needs no explanation.
pub fn load(path: Option<&Path>) -> Config {
    path.and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_wins_and_home_is_the_fallback() {
        assert_eq!(
            file_from(Some("/x/conf".into()), Some("/home/six".into())),
            Some(PathBuf::from("/x/conf/pelican/config.json"))
        );
        assert_eq!(
            file_from(None, Some("/home/six".into())),
            Some(PathBuf::from("/home/six/.config/pelican/config.json"))
        );
    }

    /// An empty or relative value would put the config wherever the app
    /// was started from; it is ignored, and with no absolute HOME there is
    /// no file at all.
    #[test]
    fn empty_or_relative_bases_are_unset() {
        for v in ["", "conf", "./conf", "~/.config"] {
            assert_eq!(
                file_from(Some(v.into()), Some("/home/six".into())),
                Some(PathBuf::from("/home/six/.config/pelican/config.json")),
                "{v:?}"
            );
        }
        assert_eq!(file_from(Some("rel".into()), Some("rel".into())), None);
        assert_eq!(file_from(None, None), None);
    }

    #[test]
    fn the_default_library_is_music_under_home() {
        let c = Config::default();
        assert_eq!(
            c.library_root_or(Some(Path::new("/home/six"))),
            Some(PathBuf::from("/home/six/Music"))
        );
        let c = Config {
            library_root: Some("/mnt/nas/Music".into()),
        };
        assert_eq!(
            c.library_root_or(Some(Path::new("/home/six"))),
            Some(PathBuf::from("/mnt/nas/Music"))
        );
    }

    #[test]
    fn a_written_root_loads_back_and_a_corrupt_file_is_first_launch() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("pelican").join("config.json");
        assert_eq!(load(Some(&f)), Config::default(), "missing file");

        let c = Config {
            library_root: Some("/mnt/nas/Music".into()),
        };
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, serde_json::to_vec(&c).unwrap()).unwrap();
        assert_eq!(load(Some(&f)), c);

        std::fs::write(&f, b"{ not json").unwrap();
        assert_eq!(load(Some(&f)), Config::default());
        assert_eq!(load(None), Config::default());
    }
}
