//! On-the-fly transcode to MP3 via ffmpeg.
//!
//! Garmin firmware accepts MP3, M4A/M4B, AAC, WAV. Anything else (FLAC, OGG,
//! Opus, WMA, APE, AIFF) must be transcoded before upload. This module shells
//! out to `ffmpeg` and writes a temp MP3 alongside the original.
//!
//! Tags are preserved (`-map_metadata 0 -id3v2_version 3`) so the resulting
//! MP3 satisfies Garmin's title+artist requirement when the source was tagged.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, Context, Result};

/// Where transcoded MP3s live during their brief life. We use a dedicated
/// subdir under the system tmp so a startup sweep can reliably find leftovers
/// from a prior crash without touching unrelated /tmp content.
pub fn cache_dir() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push("pelican");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Remove transcode artifacts older than `max_age` from the cache dir.
/// Called once at app startup to clean up after a crashed previous session.
pub fn sweep(max_age: Duration) {
    let dir = cache_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    let now = SystemTime::now();
    for ent in rd.flatten() {
        let Ok(meta) = ent.metadata() else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        if now
            .duration_since(mtime)
            .map(|d| d > max_age)
            .unwrap_or(false)
        {
            let _ = std::fs::remove_file(ent.path());
        }
    }
}

/// Audio extensions we'll route through ffmpeg for normalization. Everything
/// here gets a strict ID3v2.3 tag (title/artist/album/track/date/genre only)
/// before upload — Garmin firmware rejects files with non-standard frames.
pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "m4a", "m4b", "aac", "wav", "flac", "ogg", "oga", "opus", "wma", "ape", "aiff", "aif",
    "wv", "alac",
];

pub fn is_audio(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.iter().any(|s| s.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

pub fn is_mp3(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mp3"))
        .unwrap_or(false)
}

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Transcoded MP3. Cleans up the temp file on drop.
pub struct Transcoded {
    pub path: PathBuf,
    pub mp3_name: String,
}

impl Drop for Transcoded {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Extract standard tag fields from a source file via ffprobe. Garmin's MP3
/// indexer rejects files whose ID3 tag contains non-standard frames (custom
/// vorbis fields like `QBZ:TID`, `DISCTOTAL`, the `℗` unicode in `COPYRIGHT`,
/// etc.). We re-build the tag from a strict allowlist on the way out.
///
/// Lookup is case-insensitive — FLAC vorbis comments are uppercase by
/// convention, ID3v2 frames are mixed-case, and ffprobe normalizes to the
/// source's case. Map all to lowercase before matching.
fn extract_safe_tags(src: &Path) -> Vec<(&'static str, String)> {
    let res = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(src)
        .output();
    let Ok(o) = res else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&o.stdout);
    let mut all = std::collections::HashMap::<String, String>::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("TAG:") else {
            continue;
        };
        let Some((k, v)) = rest.split_once('=') else {
            continue;
        };
        all.insert(k.to_ascii_lowercase(), v.to_string());
    }
    // Prefer `album_artist` as the ARTIST we write — Garmin's library groups
    // tracks by ARTIST+ALBUM, so per-track composer credits ("Iva Davies"
    // vs "Richard Tognetti") fragment a single album into multiple albums.
    // The album-wide credit ("Iva Davies, Christopher Gordon, Richard
    // Tognetti") is the right grouping key. Same for `albumartist` (some
    // taggers use the no-underscore form).
    let resolved_artist = all
        .get("album_artist")
        .or_else(|| all.get("albumartist"))
        .or_else(|| all.get("artist"))
        .cloned();
    let mut out = Vec::new();
    for (key, value) in [
        ("title", all.get("title").cloned()),
        ("artist", resolved_artist.clone()),
        ("album_artist", resolved_artist), // also write TPE2 for players that read it
        ("album", all.get("album").cloned()),
        ("track", all.get("track").cloned()),
        ("date", all.get("date").cloned()),
        ("genre", all.get("genre").cloned()),
    ] {
        if let Some(v) = value {
            let s = sanitize_tag_value(&v);
            if !s.is_empty() {
                out.push((key, s));
            }
        }
    }
    out
}

/// Truncate + sanitize a filename stem for Garmin's `/Music` folder.
/// FR165 firmware silently drops writes whose filename exceeds ~60 chars or
/// contains certain punctuation. We keep ASCII letters/digits, spaces,
/// dashes, dots, and underscores; collapse runs of unsafe chars into a
/// single dash; truncate to 56 characters (leaving room for ".mp3").
fn sanitize_filename_stem(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | ' ' | '-' | '_' | '.' => c,
            _ => '-',
        })
        .collect();
    // Collapse repeated dashes.
    let mut out = String::with_capacity(cleaned.len());
    let mut prev_dash = false;
    for ch in cleaned.chars() {
        if ch == '-' {
            if prev_dash {
                continue;
            }
            prev_dash = true;
        } else {
            prev_dash = false;
        }
        out.push(ch);
    }
    let trimmed = out.trim_matches(|c: char| c == '-' || c == ' ' || c == '.');
    let mut s: String = trimmed.chars().take(56).collect();
    if s.is_empty() {
        s = "audio".into();
    }
    s
}

/// Strip control bytes and exotic unicode that has historically tripped
/// Garmin's tag parser. Keeps printable UTF-8 letters, numbers, common
/// punctuation, and accented characters.
fn sanitize_tag_value(v: &str) -> String {
    v.chars()
        .filter(|c| {
            !c.is_control()
                && *c != '\u{2117}'  // ℗
                && *c != '\u{00A9}'  // ©
                && *c != '\u{2122}'  // ™
                && *c != '\u{00AE}' // ®
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// One pass through ffmpeg that produces a Garmin-clean MP3:
/// - For MP3 sources: re-mux with `-c:a copy` (no re-encode), strip & rewrite
///   tags using only the safe allowlist. Fast (~1 sec).
/// - For other audio (FLAC, OGG, M4A, WAV, etc.): full transcode to CBR
///   192 kbps 44.1 kHz stereo MP3 with the same strict tag set.
///
/// In both cases the output is a temp file in `cache_dir()` that auto-cleans
/// on `Transcoded::Drop`. Garmin firmware verified to accept the result on
/// Forerunner 165 Music (firmware 2506).
pub fn normalize(src: &Path) -> Result<Transcoded> {
    let raw_stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "audio".into());
    // Garmin firmware silently rejects writes whose filename is too long or
    // contains exotic characters — observed cap on FR165 is around 60 chars
    // including the ".mp3" suffix. We truncate to 56 stem chars and replace
    // FAT-hostile punctuation; the audio's ID3 tags carry the real title.
    let stem = sanitize_filename_stem(&raw_stem);
    let mp3_name = format!("{stem}.mp3");

    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut tmp = cache_dir();
    tmp.push(format!("pelican-{pid}-{nanos}.mp3"));

    let tags = extract_safe_tags(src);
    let mp3_source = is_mp3(src);

    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(src)
        .arg("-vn"); // strip embedded album art / video streams in all cases
    if mp3_source {
        // Tag rewrite only — preserve original audio bitstream.
        cmd.args(["-c:a", "copy"]);
    } else {
        // Full transcode to Garmin's reliable profile.
        cmd.args([
            "-ac",
            "2",
            "-ar",
            "44100",
            "-codec:a",
            "libmp3lame",
            "-b:a",
            "192k",
        ]);
    }
    cmd.args([
        "-map_metadata",
        "-1",
        "-id3v2_version",
        "3",
        "-write_id3v1",
        "0",
    ]);
    for (k, v) in &tags {
        cmd.arg("-metadata").arg(format!("{k}={v}"));
    }
    cmd.arg(&tmp);
    let output = cmd
        .output()
        .with_context(|| format!("running ffmpeg on {}", src.display()))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!(
            "ffmpeg failed for {} :: {}",
            src.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(Transcoded {
        path: tmp,
        mp3_name,
    })
}
