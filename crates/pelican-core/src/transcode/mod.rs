//! Turning a source file into the one thing the watch is sent.
//!
//! [`encoder`] runs ffmpeg with the single proven profile; [`tags`] decides
//! what the fresh tag says. Tags are never carried across (Garmin's
//! indexer silently rejects files whose tag holds non-standard frames), so
//! the output carries exactly the seven allowlisted fields and no art.
//!
//! Where the output goes is [`crate::staging`]'s business, not this
//! module's.

pub mod encoder;
pub mod tags;

use std::path::Path;

/// Extensions Pelican will pick up when walking a directory. Everything here
/// is re-encoded by ffmpeg; nothing is sent as-is.
pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "m4a", "m4b", "aac", "wav", "flac", "ogg", "oga", "opus", "wma", "ape", "aiff", "aif",
    "aifc", "wv", "alac",
];

pub fn is_audio(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.iter().any(|s| s.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

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
