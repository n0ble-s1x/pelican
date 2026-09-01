//! Staging a source file into something the watch will accept.
//!
//! Garmin firmware plays MP3, M4A/M4B, AAC and WAV. Those are copied
//! verbatim; anything else (FLAC, OGG, Opus, WMA, APE, AIFF) has to be
//! converted first. [`encoder`] decides which, and does the work.
//!
//! Tags are never carried across. Garmin's indexer silently rejects files
//! whose tag holds non-standard frames, so [`tags`] reads the source with
//! `lofty` and writes back a fresh tag containing six fields and no cover
//! art — see [`tags::Tags`].

pub mod encoder;
pub mod tags;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, Context, Result};

/// Where transcoded audio lives during its brief life. Per-user cache dir
/// (see [`crate::paths`]) — a startup sweep can reliably find leftovers from
/// a prior crash without touching unrelated content, and it is per-user (not
/// `/tmp`) so a hostile local user on a shared box can't pre-create the dir
/// as a symlink to redirect our writes.
pub fn cache_dir() -> Option<PathBuf> {
    crate::paths::cache_dir()
}

/// Remove transcode artifacts older than `max_age` from the cache dir.
/// Called once at app startup to clean up after a crashed previous session.
pub fn sweep(max_age: Duration) {
    let Some(dir) = cache_dir() else {
        return;
    };
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

/// Audio extensions Pelican will accept. Everything here gets a tag rebuilt
/// from a strict six-field allowlist before upload — Garmin firmware rejects
/// files carrying non-standard frames. How each one is converted (or simply
/// copied) is [`encoder::plan`]'s decision.
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

/// A file staged in the cache dir, ready to upload. Cleans itself up on
/// drop, including when the transfer fails partway.
#[derive(Debug)]
pub struct Transcoded {
    pub path: PathBuf,
    /// Filename to write on the device. Carries the encoder's container
    /// extension, which is not always `.mp3` — see [`encoder::Encoder::output_ext`].
    pub remote_name: String,
}

impl Drop for Transcoded {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Truncate + sanitize a filename stem for Garmin's `/Music` folder.
/// FR165 firmware silently drops writes whose filename exceeds ~60 chars or
/// contains certain punctuation. We keep ASCII letters/digits, spaces,
/// dashes, dots, and underscores; collapse runs of unsafe chars into a
/// single dash; truncate to 56 characters (leaving room for ".mp3").
pub fn sanitize_filename_stem(raw: &str) -> String {
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
    // Truncate first, then trim again: cutting at 56 chars can re-expose a
    // trailing '-', ' ' or '.' that the first trim removed, and Garmin's
    // firmware rejects those.
    let cut: String = trimmed.chars().take(56).collect();
    let mut s = cut
        .trim_matches(|c: char| c == '-' || c == ' ' || c == '.')
        .to_string();
    if s.is_empty() {
        s = "audio".into();
    }
    s
}

/// Strip control bytes and exotic unicode that has historically tripped
/// Garmin's tag parser. Keeps printable UTF-8 letters, numbers, common
/// punctuation, and accented characters.
pub fn sanitize_tag_value(v: &str) -> String {
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

/// A staging filename no other in-flight conversion can collide with.
///
/// The pid alone is not enough: threads in one process share it, and two
/// `normalize` calls starting in the same clock tick would otherwise pick
/// the same path — one deleting the other's audio mid-upload. The counter
/// makes collision impossible within a process, the pid across processes.
/// Caught by the pipeline tests, which run in parallel and went
/// intermittently red on exactly this.
fn staging_name(ext: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let pid = std::process::id();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("pelican-{pid}-{nanos}-{seq}.{ext}")
}

/// Stage `src` in the cache directory as something the watch will accept.
///
/// The pipeline is chosen per file by [`encoder::plan`]: an already-playable
/// container is copied and re-tagged in process, anything else goes through
/// ffmpeg (to MP3) or afconvert (to M4A/AAC). In every case the tag is
/// rebuilt from the six-field allowlist rather than carried across, because
/// Garmin's indexer silently rejects non-standard frames.
///
/// The result auto-deletes on `Transcoded::Drop`.
///
/// `planned_stem` is the collision-free stem chosen at plan time by
/// `transfer::dedupe_remote_names`. Deriving the name from `src` here instead
/// would reintroduce the collision the planner just resolved.
pub fn normalize(src: &Path, planned_stem: Option<&str>) -> Result<Transcoded> {
    let raw_stem = planned_stem.map(str::to_string).unwrap_or_else(|| {
        src.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "audio".into())
    });
    // Garmin firmware silently rejects writes whose filename is too long or
    // contains exotic characters — observed cap on FR165 is around 60 chars
    // including the extension. We truncate to 56 stem chars and replace
    // FAT-hostile punctuation; the tags carry the real title.
    let stem = sanitize_filename_stem(&raw_stem);

    // Plan before reading tags. Both can fail, but only one of them can say
    // something useful: for a format nothing installed here can convert,
    // "install ffmpeg" beats a tag-parser error against a container we were
    // never going to be able to open in the first place.
    let pipeline = encoder::plan(src)?;
    let tags = tags::Tags::read(src)?;

    let out_ext = match pipeline {
        encoder::Pipeline::Passthrough => src
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_else(|| "mp3".into()),
        encoder::Pipeline::Encode(e) => e.output_ext().to_string(),
    };
    let remote_name = format!("{stem}.{out_ext}");

    let mut tmp = cache_dir()
        .ok_or_else(|| anyhow!("no per-user cache dir (neither XDG_CACHE_HOME nor HOME is set)"))?;
    tmp.push(staging_name(&out_ext));

    // Bind the temp path to a guard before doing any work, so an error on
    // the next line still removes the partial file.
    let staged = Transcoded {
        path: tmp,
        remote_name,
    };

    match pipeline {
        encoder::Pipeline::Passthrough => {
            std::fs::copy(src, &staged.path)
                .with_context(|| format!("staging {}", src.display()))?;
            retag_in_place(&staged.path, &out_ext, &tags)?;
        }
        encoder::Pipeline::Encode(e) => e.encode(src, &staged.path, &tags)?,
    }
    Ok(staged)
}

/// Rewrite the tag of an already-playable file that we copied verbatim.
///
/// WAV is deliberately left alone: it has no tag format Garmin reads, and
/// the firmware treats WAV tags as optional in practice.
fn retag_in_place(path: &Path, ext: &str, tags: &tags::Tags) -> Result<()> {
    match ext {
        "mp3" => tags.write_id3v23(path),
        "m4a" | "m4b" | "aac" => tags.write_mp4(path),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn staging_names_are_unique_under_concurrency() {
        // The bug this guards: two threads in one process picking the same
        // staging path, so one deletes the other's audio mid-transfer.
        let names: std::collections::HashSet<String> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..16)
                .map(|_| {
                    s.spawn(|| {
                        (0..64)
                            .map(|_| super::staging_name("mp3"))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap())
                .collect()
        });
        assert_eq!(names.len(), 16 * 64, "staging names collided");
    }

    #[test]
    fn filename_stem_has_no_trailing_punctuation_after_truncation() {
        // 56th char is a '-', so truncating re-exposes what the first trim removed.
        let raw = format!("{}-tail", "a".repeat(55));
        let out = sanitize_filename_stem(&raw);
        assert!(out.chars().count() <= 56);
        assert!(
            !out.ends_with('-') && !out.ends_with(' ') && !out.ends_with('.'),
            "stem {out:?} ends in punctuation Garmin rejects"
        );
    }

    use super::*;

    #[test]
    fn filename_stem_caps_at_56() {
        let raw = "11 - Iva Davies, Christopher Gordon, Richard Tognetti - Ghost of Time - Tognetti Into the Fog";
        let out = sanitize_filename_stem(raw);
        assert!(out.len() <= 56, "got {} chars: {out}", out.len());
    }

    #[test]
    fn filename_stem_replaces_unsafe_chars() {
        let out = sanitize_filename_stem("Track #3: <Live!> @ Some/Place");
        for ch in ['#', ':', '<', '>', '@', '/', '!'] {
            assert!(!out.contains(ch), "char {ch:?} survived in {out:?}");
        }
    }

    #[test]
    fn filename_stem_collapses_dash_runs() {
        let out = sanitize_filename_stem("a !@#$%^&* b");
        assert!(
            !out.contains("--"),
            "consecutive dashes not collapsed: {out:?}"
        );
    }

    #[test]
    fn filename_stem_trims_outer_dashes_dots_spaces() {
        for raw in [".track.", "-track-", " track ", "...---track---..."] {
            let out = sanitize_filename_stem(raw);
            assert!(
                !out.starts_with(['-', '.', ' ']) && !out.ends_with(['-', '.', ' ']),
                "outer trim missed in {out:?} (from {raw:?})"
            );
        }
    }

    #[test]
    fn filename_stem_never_returns_empty() {
        for raw in ["", "@@@", "///", "   "] {
            let out = sanitize_filename_stem(raw);
            assert!(!out.is_empty(), "empty result for input {raw:?}");
        }
    }

    #[test]
    fn tag_value_strips_copyright_glyphs() {
        for glyph in ["\u{2117}", "\u{00A9}", "\u{2122}", "\u{00AE}"] {
            let out = sanitize_tag_value(&format!("Foo {glyph}2024 Bar"));
            assert!(!out.contains(glyph), "glyph {glyph:?} survived: {out:?}");
        }
    }

    #[test]
    fn tag_value_strips_control_bytes_and_trims() {
        let out = sanitize_tag_value("  Track\x07Name\n  ");
        assert_eq!(out, "TrackName");
    }

    #[test]
    fn tag_value_keeps_accented_letters() {
        let out = sanitize_tag_value("Café Tacvba — Olé");
        assert!(out.contains("Café"));
        assert!(out.contains("Tacvba"));
        assert!(out.contains("Olé"));
    }

    #[test]
    fn is_audio_covers_known_extensions() {
        for ok in ["a.mp3", "a.flac", "a.OPUS", "a.M4B"] {
            assert!(is_audio(Path::new(ok)));
        }
        for ko in ["a.txt", "a.jpg", "a"] {
            assert!(!is_audio(Path::new(ko)));
        }
    }
}
