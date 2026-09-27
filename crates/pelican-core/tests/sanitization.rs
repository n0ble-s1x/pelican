//! End-to-end logic tests for the Garmin-quirk workarounds. These are the
//! parts of the pipeline most likely to regress silently — they encode hard-
//! won knowledge about what the watch firmware accepts. If any of these
//! ever fails, double-check against `docs/garmin-mtp.md` before
//! "fixing" the test.
//!
//! These used to assert against constants re-declared inside this file,
//! because `pelican` was a binary crate and an integration test could not
//! see into it. That made them documentation that could not fail. Now that
//! the engine is a library they bind to the real implementation, so drift
//! in `transcode` breaks the build instead of passing quietly.

use std::path::Path;

use pelican_core::transcode;

/// Longest stem the sanitizer will emit, and the value the firmware limit
/// is reasoned against.
const STEM_CAP: usize = 56;

#[test]
fn filename_cap_holds_against_the_firmware_limit() {
    // Garmin firmware silently rejects writes with a remote_name longer
    // than ~60 chars (verified empirically on FR165 / firmware 2506).
    // We cap stems at 56 to leave room for ".mp3" plus a safety margin,
    // since some firmware trims at 60 inclusive of the dot-extension.
    assert!(STEM_CAP + ".mp3".len() <= 60);

    // And the implementation actually honours it, for input far over the
    // limit and for input that is pure punctuation.
    for raw in [
        "11 - Iva Davies, Christopher Gordon, Richard Tognetti - Ghost of Time - Tognetti Into the Fog",
        &"x".repeat(400),
        "…".repeat(200).as_str(),
    ] {
        let out = transcode::sanitize_filename_stem(raw);
        assert!(
            out.chars().count() <= STEM_CAP,
            "{} chars from {raw:?}",
            out.chars().count()
        );
        assert!(!out.is_empty(), "sanitizer must never return an empty stem");
    }
}

#[test]
fn every_format_we_claim_is_picked_up_as_audio() {
    // Everything goes through ffmpeg, so "accepted" means one thing: a
    // directory walk picks it up. FLAC is the load-bearing one (Qobuz /
    // Bandcamp downloads); MP3 and WAV are re-encoded too, never passed
    // through.
    for ext in [
        "mp3", "m4a", "m4b", "aac", "wav", "flac", "ogg", "oga", "opus", "wma", "ape", "aiff",
        "aif", "aifc", "wv", "alac",
    ] {
        assert!(
            transcode::is_audio(Path::new(&format!("track.{ext}"))),
            "{ext} must be seen as audio or it is silently skipped"
        );
        // Case must not matter: a file off a FAT volume can arrive shouting.
        assert!(transcode::is_audio(Path::new(&format!(
            "track.{}",
            ext.to_uppercase()
        ))));
    }
    // Non-audio must stay out of the plan entirely.
    for ext in ["jpg", "cue", "txt", "log", "pdf"] {
        assert!(!transcode::is_audio(Path::new(&format!("f.{ext}"))));
    }
}

#[test]
fn tag_values_are_stripped_of_what_the_indexer_rejects() {
    // Garmin's music indexer silently rejects MP3s whose ID3v2.3 tag has
    // non-standard frames or exotic glyphs (verified: QBZ:TID, COPYRIGHT
    // with ℗ unicode, DISCTOTAL, TRACKTOTAL, SUBTITLE, ISRC, APIC art).
    for glyph in ['\u{2117}', '\u{00A9}', '\u{2122}', '\u{00AE}'] {
        let out = transcode::sanitize_tag_value(&format!("Album {glyph} 2024"));
        assert!(!out.contains(glyph), "{glyph:?} survived: {out:?}");
    }
    // Control bytes would let a crafted tag forge lines in terminal output.
    let out = transcode::sanitize_tag_value("Track\x1b[31m\x07Name\n");
    assert!(!out.chars().any(char::is_control), "{out:?}");
    // But real text must survive intact — over-sanitizing mangles libraries.
    assert_eq!(
        transcode::sanitize_tag_value("  Café Tacvba  "),
        "Café Tacvba"
    );
}
