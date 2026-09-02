//! The one thing the shell remembers between launches: which folder the
//! user pointed it at.
//!
//! It is deliberately not the asset-protocol grant. That grant is re-applied
//! from this value at startup rather than persisted by Tauri, because a
//! serialized scope is a standing capability sitting in a file, and this is
//! a plain path the user chose and can delete.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct ShellConfig {
    /// Absolute, canonical. `None` before the user has picked anything.
    #[serde(default)]
    pub library_root: Option<PathBuf>,
}

fn file() -> Option<PathBuf> {
    let mut p = pelican_core::paths::data_dir()?;
    p.push("shell.json");
    Some(p)
}

/// Never fails: an unreadable or corrupt file is treated as "nothing
/// remembered", which is exactly the first-launch state and needs no
/// explanation to the user.
pub fn load() -> ShellConfig {
    let Some(path) = file() else {
        return ShellConfig::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return ShellConfig::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn set_library_root(root: &Path) {
    let Some(path) = file() else { return };
    let cfg = ShellConfig {
        library_root: Some(root.to_path_buf()),
    };
    if let Ok(bytes) = serde_json::to_vec_pretty(&cfg) {
        let _ = std::fs::write(path, bytes);
    }
}
