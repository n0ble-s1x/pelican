//! Persistent record of files we've uploaded to a given watch.
//!
//! Garmin's firmware does not expose its indexed music library over MTP, so
//! after the watch absorbs files out of `/Music` they're invisible to us.
//! This module keeps a local journal — per device serial — of every upload
//! we've successfully completed, so the GUI can show the user "what's on the
//! watch (according to us)" even after the indexer has eaten the staging.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct UploadRecord {
    pub name: String,
    pub bytes: u64,
    /// UNIX seconds.
    pub at: u64,
}

/// A user-defined collection of local source paths. Playlists are local-only
/// because Garmin firmware on the FR165 silently rejects MTP writes for
/// `.m3u`/`.m3u8` files (regardless of format code). The "Send playlist"
/// action just queues every track for upload — the *watch* sees them as
/// individual files, but the user gets a stable group on this side.
#[derive(Clone, Serialize, Deserialize)]
pub struct LocalPlaylist {
    pub name: String,
    pub tracks: Vec<std::path::PathBuf>,
    /// UNIX seconds.
    #[serde(default)]
    pub created_at: u64,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct DeviceHistory {
    pub serial: String,
    pub uploads: Vec<UploadRecord>,
    #[serde(default)]
    pub playlists: Vec<LocalPlaylist>,
}

fn data_dir() -> PathBuf {
    let mut p = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut p = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            p.push(".local");
            p.push("share");
            p
        });
    p.push("pelican");
    let _ = std::fs::create_dir_all(&p);
    p
}

fn file_for(serial: &str) -> PathBuf {
    let safe: String = serial
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let mut p = data_dir();
    p.push(format!("uploads-{safe}.json"));
    p
}

pub fn load(serial: &str) -> DeviceHistory {
    let path = file_for(serial);
    let Ok(bytes) = std::fs::read(&path) else {
        return DeviceHistory {
            serial: serial.to_string(),
            uploads: Vec::new(),
            playlists: Vec::new(),
        };
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|_| DeviceHistory {
        serial: serial.to_string(),
        uploads: Vec::new(),
        playlists: Vec::new(),
    })
}

pub fn save(history: &DeviceHistory) {
    let path = file_for(&history.serial);
    if let Ok(bytes) = serde_json::to_vec_pretty(history) {
        let _ = std::fs::write(path, bytes);
    }
}

pub fn record(serial: &str, name: &str, bytes: u64) {
    let mut h = load(serial);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // De-dup by name+bytes — re-uploading the same file should refresh
    // its timestamp rather than create a duplicate row.
    h.uploads.retain(|u| !(u.name == name && u.bytes == bytes));
    h.uploads.push(UploadRecord {
        name: name.to_string(),
        bytes,
        at: now,
    });
    if h.uploads.len() > 1000 {
        let drop = h.uploads.len() - 1000;
        h.uploads.drain(0..drop);
    }
    save(&h);
}

pub fn add_playlist(serial: &str, name: String, tracks: Vec<std::path::PathBuf>) {
    let mut h = load(serial);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    h.playlists.retain(|p| p.name != name);
    h.playlists.push(LocalPlaylist {
        name,
        tracks,
        created_at: now,
    });
    save(&h);
}

pub fn remove_playlist(serial: &str, name: &str) {
    let mut h = load(serial);
    h.playlists.retain(|p| p.name != name);
    save(&h);
}
