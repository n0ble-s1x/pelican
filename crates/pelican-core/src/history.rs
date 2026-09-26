//! Persistent record of files we've uploaded to a given watch.
//!
//! Garmin's firmware does not expose its indexed music library over MTP —
//! `list_dir("Music")` is the only view we get, and it is a view of files,
//! not of what the watch's music app will play. This module keeps a local
//! journal, per device serial, of every upload we completed.
//!
//! What it is: our record that we sent a file. What it is not: evidence the
//! file is still on the watch. On FR165 firmware 2506 `/Music` is durable —
//! a probe found all 20 of the owner's tracks still listed there — so a
//! journal entry the listing does not confirm most likely means the file has
//! since gone. Callers must not present the two sources as the same fact.
//!
//! `UploadRecord::name` is the *sanitized remote stem* — the name written to
//! the device — precisely so it can be matched against a later listing.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct UploadRecord {
    pub name: String,
    pub bytes: u64,
    /// UNIX seconds.
    pub at: u64,

    // The three tag fields below are additive, and every one of them carries
    // `#[serde(default)]` for a load-bearing reason: `load` swallows a
    // deserialize error and returns an empty history. A field without a
    // default would turn every journal written before this existed into a
    // parse failure, and the user's record of what they have sent would
    // silently reset to nothing on first launch of the new build.
    /// Tags read from the *source* file at send time. `None` on every row
    /// written before this field existed, and on files with no tags at all —
    /// the UI treats both identically, because in both cases Pelican
    /// genuinely does not know the album. Never guessed from the filename:
    /// the sanitized 56-char remote stem cannot be reversed into metadata,
    /// and inventing one would be the app asserting a fact the device never
    /// gave it.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
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

/// Per-user data directory. See [`crate::paths`] for the per-platform
/// locations and the reasoning behind failing closed instead of falling
/// back to a world-writable directory.
fn data_dir() -> Option<PathBuf> {
    crate::paths::data_dir()
}

fn file_for(serial: &str) -> Option<PathBuf> {
    let safe: String = serial
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let mut p = data_dir()?;
    p.push(format!("uploads-{safe}.json"));
    Some(p)
}

pub fn load(serial: &str) -> DeviceHistory {
    let Some(path) = file_for(serial) else {
        return DeviceHistory {
            serial: serial.to_string(),
            uploads: Vec::new(),
            playlists: Vec::new(),
        };
    };
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

/// Write the journal, or say why it could not be written.
///
/// Temp-then-rename, not a straight overwrite. `record` runs once per
/// uploaded file, in the middle of a transfer the user may quit or the OS
/// may kill; a truncated `fs::write` over the live file loses the entire
/// history rather than one row. `rename` is atomic on APFS and on every
/// filesystem this ships on, so the file on disk is only ever the old whole
/// journal or the new whole journal.
pub fn save(history: &DeviceHistory) -> Result<()> {
    let Some(path) = file_for(&history.serial) else {
        anyhow::bail!("no writable data directory — nothing was recorded");
    };
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(history)?;
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// Record an upload, with whatever the source file said about itself.
///
/// The de-dupe key is `name + bytes` and stays that way: a re-send with a
/// corrected tag must refresh the existing row, not add a second one beside
/// it. Tags ride along; they never take part in identity.
pub fn record_upload(
    serial: &str,
    name: &str,
    bytes: u64,
    tags: Option<&crate::transcode::tags::Tags>,
) -> Result<()> {
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
        title: tags.and_then(|t| t.title.clone()),
        artist: tags.and_then(|t| t.artist.clone()),
        album: tags.and_then(|t| t.album.clone()),
    });
    if h.uploads.len() > 1000 {
        let drop = h.uploads.len() - 1000;
        h.uploads.drain(0..drop);
    }
    save(&h)
}

/// The pre-tags entry point, kept for `crates/pelican` — the egui build,
/// which this work does not touch. New code calls [`record_upload`] and
/// handles the failure; that crate has no channel to report one on, and
/// dropping it here is exactly what it did before.
pub fn record(serial: &str, name: &str, bytes: u64) {
    let _ = record_upload(serial, name, bytes, None);
}

/// Drop journal rows by remote stem, lowercased.
///
/// This is the same key `ui/app.js`'s `stemOf` produces — it lowercases too —
/// which is what makes the wall's dedupe line up. The two live in different
/// languages and nothing enforces the agreement, so it is stated in both
/// places rather than left to be rediscovered.
///
/// Called after a successful delete. Without it, a file removed from the
/// watch immediately reappears in the wall as "Sent today · the watch is not
/// listing it now" — which is true of the journal and useless to the user,
/// because they are the one who just removed it.
pub fn forget(serial: &str, stems: &[String]) -> Result<()> {
    let drop: std::collections::HashSet<String> = stems.iter().map(|s| s.to_lowercase()).collect();
    let mut h = load(serial);
    let before = h.uploads.len();
    h.uploads.retain(|u| !drop.contains(&u.name.to_lowercase()));
    if h.uploads.len() == before {
        return Ok(());
    }
    save(&h)
}

/// Forget by device path — `Music/Some Track.mp3` — rather than by stem.
///
/// The delete path has paths, not stems, so the conversion lives here next
/// to the key it has to match instead of being open-coded at the call site.
pub fn forget_paths(serial: &str, paths: &[String]) -> Result<()> {
    let stems: Vec<String> = paths.iter().map(|p| stem_of_path(p)).collect();
    forget(serial, &stems)
}

/// Last path segment, extension removed, lowercased.
///
/// A leading dot is not an extension separator: `.hidden` is a whole stem.
fn stem_of_path(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        Some(i) if i > 0 => name[..i].to_lowercase(),
        _ => name.to_lowercase(),
    }
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
    // Playlists are unchanged by this work, and their one caller is the egui
    // build. Same disposition as before: best effort, nothing to report on.
    let _ = save(&h);
}

pub fn remove_playlist(serial: &str, name: &str) {
    let mut h = load(serial);
    h.playlists.retain(|p| p.name != name);
    let _ = save(&h);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard on the owner's real journal.
    ///
    /// `load` swallows a deserialize error and hands back an empty history,
    /// so a field added without `#[serde(default)]` would not fail loudly —
    /// it would quietly erase the record of every file already sent. This is
    /// the exact JSON on disk before the tag fields existed.
    #[test]
    fn a_journal_written_before_tags_existed_still_loads() {
        let old = r#"{
          "serial": "0000a1b2c3d4",
          "uploads": [
            { "name": "01 - Iva Davies- Christopher Gordon- Richard Tognetti",
              "bytes": 13526597, "at": 1788395288 },
            { "name": "02 - Iva Davies- Christopher Gordon- Richard Tognetti",
              "bytes": 3205513, "at": 1788395289 }
          ],
          "playlists": []
        }"#;
        let h: DeviceHistory = serde_json::from_str(old).expect("old rows must still parse");
        assert_eq!(h.uploads.len(), 2, "no row may be lost");
        assert_eq!(h.uploads[0].bytes, 13_526_597);
        for u in &h.uploads {
            assert_eq!(u.title, None);
            assert_eq!(u.artist, None);
            assert_eq!(u.album, None);
        }
        // And round-trips: writing it back must not invent tags.
        let again: DeviceHistory =
            serde_json::from_slice(&serde_json::to_vec(&h).unwrap()).unwrap();
        assert_eq!(again.uploads.len(), 2);
        assert_eq!(again.uploads[1].album, None);
    }

    /// `forget` keys on the lowercased stem, which is what `stemOf` in
    /// `ui/app.js` produces. If these drift, deleting a file from the watch
    /// leaves its journal row behind and the wall re-lists it as "sent, not
    /// on the watch now" the instant the delete succeeds.
    #[test]
    fn forget_paths_keys_on_the_same_stem_the_ui_does() {
        assert_eq!(
            stem_of_path("Music/01 - Great Southern Land.mp3"),
            "01 - great southern land"
        );
        assert_eq!(
            stem_of_path("01 - Vol. 1 - Opening.m4a"),
            "01 - vol. 1 - opening"
        );
        // No extension at all: the whole name is the stem.
        assert_eq!(stem_of_path("Music/no-extension"), "no-extension");
        // A leading dot is not an extension separator.
        assert_eq!(stem_of_path(".hidden"), ".hidden");
    }
}
